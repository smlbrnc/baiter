//! RTDS canlı probe — gerçek Polymarket RTDS endpoint'ine bağlanıp tick
//! akışını ve `window_delta_bps` / `window_delta_score` / `interval_scale`
//! hesaplarını 60 sn boyunca izler. Asıl `rtds_task`'ı spawn eder; window
//! boundary'yi "şu an" olarak set eder ki ilk tick yakalansın.
//!
//! Çalıştırma:
//!   cargo run --example rtds_probe -- btc/usd 60
//!   cargo run --example rtds_probe -- eth/usd 30
//!
//! Default symbol `btc/usd`, default süre `60` sn.

use std::time::Duration;

use baiter_pro::rtds::{
    self, composite_score, effective_composite, interval_scale, window_delta_score,
};
use baiter_pro::time::now_ms;

#[tokio::main]
async fn main() {
    // rustls 0.23 çoklu-provider ayıklaması: bin hedeflerinde
    // (src/bin/bot.rs, supervisor.rs) olduğu gibi explicit install.
    let _ = rustls::crypto::ring::default_provider().install_default();

    let args: Vec<String> = std::env::args().collect();
    let symbol = args.get(1).cloned().unwrap_or_else(|| "btc/usd".into());
    let duration_secs: u64 = args
        .get(2)
        .and_then(|s| s.parse().ok())
        .unwrap_or(60);

    let ws_url = std::env::var("RTDS_WS_URL")
        .unwrap_or_else(|_| "wss://ws-live-data.polymarket.com".into());
    let stale_ms: u64 = std::env::var("RTDS_STALE_THRESHOLD_MS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(30_000);
    let max_backoff_ms: u64 = std::env::var("RTDS_RECONNECT_MAX_BACKOFF_MS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(60_000);

    println!("==============================================");
    println!("RTDS Probe");
    println!("  endpoint : {ws_url}");
    println!("  symbol   : {symbol}");
    println!("  duration : {duration_secs}s");
    println!("  stale_ms : {stale_ms}");
    println!("==============================================");

    let state = rtds::new_shared_state();
    // Window boundary = şimdi → ilk gelen tick window_open olarak yakalanacak.
    let window_start_ms = now_ms();
    rtds::reset_window(&state, window_start_ms).await;

    let task_state = state.clone();
    let task_symbol = symbol.clone();
    let task_url = ws_url.clone();
    let handle = tokio::spawn(async move {
        rtds::run_rtds_task(
            task_url,
            task_symbol,
            stale_ms,
            max_backoff_ms,
            task_state,
            0, // bot_id=0 → log label'ı "0"
        )
        .await;
    });

    // 5-dk (300s) = 1.0 ölçeği: probe için sabit kullanılıyor.
    let scale = interval_scale(300);

    let mut tick = tokio::time::interval(Duration::from_secs(1));
    let deadline = now_ms() + duration_secs * 1000;
    let mut last_tick_ms_seen: u64 = 0;
    let mut tick_count: u32 = 0;

    while now_ms() < deadline {
        tick.tick().await;
        let snap = {
            let guard = state.read().await;
            (
                guard.connected,
                guard.current_price,
                guard.window_open_price,
                guard.window_open_ts_ms,
                guard.window_delta_bps,
                guard.last_tick_ms,
                guard.window_start_ts_ms,
            )
        };
        let (connected, current, open, open_ts, delta_bps, last_ms, win_start) = snap;

        if last_ms != last_tick_ms_seen {
            tick_count += 1;
            last_tick_ms_seen = last_ms;
        }

        let wd_score = window_delta_score(delta_bps, scale);
        // Demonstration: Binance nötr (5.0) kabul → composite = 0.7*wd + 0.3*5
        let comp = composite_score(wd_score, 5.0, 0.70);
        let eff = effective_composite(comp, 10.0);

        let open_str = match open {
            Some(p) => format!("{p:>10.2}"),
            None => "     pending".into(),
        };
        let open_ts_str = open_ts.map(|t| t.to_string()).unwrap_or_else(|| "-".into());
        let conn = if connected { "UP  " } else { "DOWN" };
        let age_ms = if last_ms > 0 {
            (now_ms().saturating_sub(last_ms)) as i64
        } else {
            -1
        };
        println!(
            "[{conn}] price={current:>10.2} open={open_str} open_ts={open_ts_str:>14} \
             delta={delta_bps:>+7.2}bps wd_score={wd_score:>5.2} composite={comp:>5.2} \
             eff={eff:>5.2} ticks={tick_count:>3} last_age={age_ms:>5}ms win_start={win_start}"
        );
    }

    println!("==============================================");
    println!("Summary:");
    {
        let guard = state.read().await;
        println!("  connected         : {}", guard.connected);
        println!("  current_price     : {}", guard.current_price);
        println!("  window_open_price : {:?}", guard.window_open_price);
        println!("  window_open_ts_ms : {:?}", guard.window_open_ts_ms);
        println!("  window_delta_bps  : {:.4}", guard.window_delta_bps);
        println!("  tick_count_observed: {tick_count}");
        println!("  last_tick_ms      : {}", guard.last_tick_ms);
        println!("  window_start_ts_ms: {}", guard.window_start_ts_ms);
    }
    println!("==============================================");

    handle.abort();
}
