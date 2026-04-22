//! WS event capture & test trade tool — Polymarket User Channel raw payload dump.
//!
//! Usage:
//!   cargo run --release --bin wstest -- \
//!       --slug btc-updown-5m-1776879600 \
//!       [--outcome UP] [--size 5] [--no-trade] [--wait 60]
//!
//! Davranış:
//! - global_credentials'tan auth bilgilerini okur, gamma'dan market metadata çeker.
//! - User WS'e auth ile abone olur; gelen TÜM ham metin frame'leri timestamp'li
//!   olarak `wstest-<epoch_ms>.txt` dosyasına satır satır append edilir
//!   (parse YAPILMAZ → spec doğrulaması için ham JSON).
//! - `--no-trade` verilmediyse: 1 adet `FAK BUY @ 0.99` (taker olarak instant fill)
//!   verilir; trade event WS akışında görünür.
//! - `--wait` saniye bekler, sonra `cancel-all` çağırır ve çıkar.

use std::env;
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use futures_util::{SinkExt, StreamExt};
use tokio::io::AsyncWriteExt;
use tokio::sync::mpsc;
use tokio::time::sleep;
use tokio_tungstenite::tungstenite::Message;

use baiter_pro::config::{Credentials, RuntimeEnv};
use baiter_pro::db::get_global_credentials;
use baiter_pro::polymarket::order::{
    build_order, expiration_for, order_to_json, sign_order, BuildArgs,
};
use baiter_pro::polymarket::{shared_http_client, ClobClient, GammaClient};
use baiter_pro::time::now_ms;
use baiter_pro::types::{Outcome, Side};

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let _ = dotenvy::dotenv();
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::new(
            "info,hyper=warn,sqlx=warn,tungstenite=warn,reqwest=warn",
        ))
        .with_target(false)
        .init();

    let argv: Vec<String> = env::args().collect();
    let slug = arg(&argv, "--slug").ok_or_else(|| anyhow!("--slug zorunlu"))?;
    let outcome_str = arg(&argv, "--outcome").unwrap_or_else(|| "UP".to_string());
    let outcome =
        Outcome::parse(&outcome_str).ok_or_else(|| anyhow!("--outcome UP|DOWN"))?;
    let size: f64 = arg(&argv, "--size")
        .map(|s| s.parse())
        .transpose()
        .map_err(|e| anyhow!("--size parse: {e}"))?
        .unwrap_or(5.0);
    let wait_secs: u64 = arg(&argv, "--wait")
        .map(|s| s.parse())
        .transpose()
        .map_err(|e| anyhow!("--wait parse: {e}"))?
        .unwrap_or(60);
    let do_trade = !flag(&argv, "--no-trade");

    let runtime = RuntimeEnv::from_env().context("RuntimeEnv")?;
    let pool = baiter_pro::db::open(&runtime.db_path).await?;
    let creds: Credentials = get_global_credentials(&pool)
        .await?
        .ok_or_else(|| anyhow!("global_credentials boş — Settings'ten gir"))?
        .into();

    println!(
        "EOA={} funder={:?} api_key={} sig_type={}",
        creds.poly_address, creds.funder, creds.poly_api_key, creds.signature_type
    );

    let http = shared_http_client();
    let gamma = GammaClient::new(http.clone(), runtime.gamma_base_url.clone());
    let market = gamma.get_market_by_slug(&slug).await?;
    let condition_id = market
        .condition_id
        .clone()
        .ok_or_else(|| anyhow!("conditionId eksik"))?;
    let (up_token, down_token) = market.parse_token_ids()?;
    let neg_risk = market.neg_risk.unwrap_or(false);
    let tick_size = market.tick_size.unwrap_or(0.01);

    println!("market={slug}");
    println!("  condition_id={condition_id}");
    println!("  up_token   ={up_token}");
    println!("  down_token ={down_token}");
    println!("  tick_size={tick_size} neg_risk={neg_risk}");

    let token_id = match outcome {
        Outcome::Up => up_token.clone(),
        Outcome::Down => down_token.clone(),
    };

    let clob = ClobClient::new(
        http.clone(),
        runtime.clob_base_url.clone(),
        Some(creds.clone()),
    );
    let fee_rate_bps = clob.fetch_fee_rate_bps(&token_id).await?;
    println!("  fee_rate_bps={fee_rate_bps}");

    let out_path = format!("wstest-{}.txt", now_ms());
    println!("RAW WS dump -> {out_path}");

    let (raw_tx, raw_rx) = mpsc::unbounded_channel::<String>();
    spawn_writer(out_path.clone(), raw_rx);

    let creds_ws = creds.clone();
    let condition_ws = condition_id.clone();
    let ws_url = format!("{}/user", runtime.clob_ws_base);
    tokio::spawn(async move {
        ws_capture_loop(ws_url, creds_ws, vec![condition_ws], raw_tx).await;
    });

    println!("waiting 3s for WS handshake...");
    sleep(Duration::from_secs(3)).await;

    if do_trade {
        println!(
            "placing FAK BUY {outcome:?} size={size} @ 0.99 (will cross & fill as TAKER)..."
        );
        let exp = expiration_for("FAK", 0);
        let order = build_order(&BuildArgs {
            creds: &creds,
            token_id: &token_id,
            side: Side::Buy,
            size,
            price: 0.99,
            expiration_secs: exp,
            neg_risk,
            fee_rate_bps,
            tick_size,
        })?;
        let sig = sign_order(&order, &creds, runtime.polygon_chain_id, neg_risk).await?;
        let body = order_to_json(&order, &sig);
        let resp = clob.post_order(body, "FAK", &creds.poly_api_key).await?;
        println!(
            "POST /order: success={} status={} order_id={} err='{}'",
            resp.success,
            resp.status.as_str(),
            resp.order_id,
            resp.error_msg
        );
    }

    println!("listening for WS events for {wait_secs}s...");
    sleep(Duration::from_secs(wait_secs)).await;

    if do_trade {
        match clob.cancel_all().await {
            Ok(c) => println!("cancel-all: canceled={}", c.canceled.len()),
            Err(e) => eprintln!("cancel-all err: {e}"),
        }
    }

    println!("done. Inspect raw payloads at: {out_path}");
    Ok(())
}

fn arg(argv: &[String], key: &str) -> Option<String> {
    let mut i = 0;
    while i < argv.len() {
        if argv[i] == key && i + 1 < argv.len() {
            return Some(argv[i + 1].clone());
        }
        i += 1;
    }
    None
}

fn flag(argv: &[String], key: &str) -> bool {
    argv.iter().any(|a| a == key)
}

fn spawn_writer(path: String, mut rx: mpsc::UnboundedReceiver<String>) {
    tokio::spawn(async move {
        let mut f = match tokio::fs::File::create(&path).await {
            Ok(f) => f,
            Err(e) => {
                eprintln!("writer open err: {e}");
                return;
            }
        };
        while let Some(line) = rx.recv().await {
            let stamped = format!("[{}] {}\n", now_ms(), line);
            let _ = f.write_all(stamped.as_bytes()).await;
            let _ = f.flush().await;
        }
    });
}

/// User Channel'a abone olur; her gelen text/binary frame'i RAW olarak yollar.
/// `tokio_tungstenite` kullanır, tek bir reconnect döngüsü yeterli (kısa run).
async fn ws_capture_loop(
    url: String,
    creds: Credentials,
    markets: Vec<String>,
    tx: mpsc::UnboundedSender<String>,
) {
    let sub = serde_json::json!({
        "auth": {
            "apiKey": creds.poly_api_key,
            "secret":  creds.poly_secret,
            "passphrase": creds.poly_passphrase,
        },
        "markets": markets,
        "type": "user",
    });

    loop {
        let _ = tx.send(format!("--- connecting {url} markets={markets:?} ---"));
        let (ws, _) = match tokio_tungstenite::connect_async(&url).await {
            Ok(c) => c,
            Err(e) => {
                let _ = tx.send(format!("--- connect err: {e} ---"));
                sleep(Duration::from_secs(2)).await;
                continue;
            }
        };
        let (mut write, mut read) = ws.split();
        if let Err(e) = write
            .send(Message::Text(sub.to_string().into()))
            .await
        {
            let _ = tx.send(format!("--- send sub err: {e} ---"));
            continue;
        }
        let _ = tx.send("--- subscribed ---".to_string());

        while let Some(msg) = read.next().await {
            match msg {
                Ok(Message::Text(t)) => {
                    let _ = tx.send(t.to_string());
                }
                Ok(Message::Binary(b)) => {
                    if let Ok(s) = String::from_utf8(b.to_vec()) {
                        let _ = tx.send(s);
                    }
                }
                Ok(Message::Ping(p)) => {
                    let _ = write.send(Message::Pong(p)).await;
                }
                Ok(Message::Close(_)) => break,
                Err(e) => {
                    let _ = tx.send(format!("--- read err: {e} ---"));
                    break;
                }
                _ => {}
            }
        }
        let _ = tx.send("--- ws closed, reconnect in 2s ---".to_string());
        sleep(Duration::from_secs(2)).await;
    }
}
