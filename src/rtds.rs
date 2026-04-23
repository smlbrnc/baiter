//! Polymarket RTDS Chainlink feed — `Arc<RwLock<RtdsState>>` ile strateji
//! katmanına anlık fiyat + window delta sinyali yayar.

use std::collections::VecDeque;
use std::sync::Arc;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tokio::time::{interval, timeout};
use tokio_tungstenite::tungstenite::{Message, Utf8Bytes};

use crate::ipc;
use crate::time::now_ms;

/// Velocity kayar penceresi; RTDS ~1-2 tick/sn, 5 sn ≈ 5-10 sample.
const VELOCITY_WINDOW_MS: u64 = 5_000;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RtdsState {
    /// Son Chainlink tick fiyatı (USD).
    pub current_price: f64,
    pub window_open_price: Option<f64>,
    pub window_open_ts_ms: Option<u64>,
    pub window_start_ts_ms: u64,
    /// `(current − open) / open × 10_000`; open yoksa `0.0`.
    pub window_delta_bps: f64,
    /// Son `VELOCITY_WINDOW_MS` ortalama hız (bps/sn). `tick.rs` bunu
    /// `lookahead_secs` ile çarpıp `window_delta_bps`'e ekleyerek ileri tahmin yapar.
    pub recent_velocity_bps_per_sec: f64,
    pub last_tick_ms: u64,
    #[serde(skip)]
    pub recent_samples: VecDeque<(u64, f64)>,
}

pub type SharedRtdsState = Arc<RwLock<RtdsState>>;

pub fn new_shared_state() -> SharedRtdsState {
    Arc::new(RwLock::new(RtdsState::default()))
}

/// Pencere sınırını güncelle — `window_open_*` ve `delta` sıfırlanır;
/// `current_price`/`last_tick_ms`/velocity bilgileri canlı feed için korunur.
pub async fn reset_window(state: &SharedRtdsState, window_start_ts_ms: u64) {
    let mut s = state.write().await;
    s.window_open_price = None;
    s.window_open_ts_ms = None;
    s.window_start_ts_ms = window_start_ts_ms;
    s.window_delta_bps = 0.0;
}

/// `recent_samples`'tan velocity (bps/sn); tek nokta veya <0.5 sn → 0.0.
fn compute_velocity(samples: &VecDeque<(u64, f64)>) -> f64 {
    if samples.len() < 2 {
        return 0.0;
    }
    let (t0, p0) = *samples.front().expect("non-empty");
    let (t1, p1) = *samples.back().expect("non-empty");
    let dt_sec = (t1.saturating_sub(t0)) as f64 / 1000.0;
    if dt_sec < 0.5 || p0 <= 0.0 {
        return 0.0;
    }
    let bps_change = (p1 - p0) / p0 * 10_000.0;
    bps_change / dt_sec
}

/// `window_delta_bps` → `[0, 10]` skor (5.0 = nötr); 5-dk kalibre, uzun
/// pencerede [`interval_scale`] ile `√T` ölçeklenir.
pub fn window_delta_score(bps: f64, interval_scale: f64) -> f64 {
    let scale = if interval_scale > 0.0 {
        interval_scale
    } else {
        1.0
    };
    let x = bps / scale;
    let d = x.abs();
    let sgn = x.signum();
    let score_delta = if d < 0.5 {
        x * 0.4
    } else if d < 2.0 {
        sgn * (0.2 + (d - 0.5) * 0.8)
    } else if d < 7.0 {
        sgn * (1.4 + (d - 2.0) * 0.5)
    } else if d < 15.0 {
        sgn * (3.9 + (d - 7.0) * 0.1375)
    } else {
        sgn * 5.0
    };
    (5.0 + score_delta).clamp(0.0, 10.0)
}

/// `√T` volatilite ölçeği (GBM); 5-dk baseline 1.0.
pub const fn interval_scale(interval_secs: u64) -> f64 {
    match interval_secs {
        300 => 1.0,
        900 => 1.73,
        3600 => 3.46,
        14400 => 6.93,
        _ => 1.0,
    }
}

/// Window + Binance skorlarının ağırlıklı ortalaması (`[0, 10]` clamp).
pub fn composite_score(window_score: f64, binance_score: f64, window_weight: f64) -> f64 {
    let w = window_weight.clamp(0.0, 1.0);
    (w * window_score + (1.0 - w) * binance_score).clamp(0.0, 10.0)
}

/// Asset için makul fiyat bandı (bozuk veri / spike koruması).
fn sane_price(symbol: &str, value: f64) -> bool {
    if !value.is_finite() || value <= 0.0 {
        return false;
    }
    let head = symbol.split('/').next().unwrap_or(symbol);
    match head {
        "btc" => (1_000.0..=10_000_000.0).contains(&value),
        "eth" => (10.0..=1_000_000.0).contains(&value),
        "sol" => (0.1..=100_000.0).contains(&value),
        "xrp" => (0.001..=10_000.0).contains(&value),
        _ => (1e-6..=1e9).contains(&value),
    }
}

#[derive(Debug, Deserialize)]
struct RtdsEnvelope<'a> {
    #[serde(default)]
    topic: Option<&'a str>,
    #[serde(default, rename = "type")]
    kind: Option<&'a str>,
    #[serde(default)]
    payload: Option<RtdsPayload>,
}

#[derive(Debug, Deserialize)]
struct RtdsPayload {
    #[serde(default)]
    symbol: String,
    #[serde(default)]
    timestamp: u64,
    value: f64,
}

const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const PING_INTERVAL: Duration = Duration::from_secs(5);

/// RTDS WS task'ı; kopuşta exponential backoff, `stale_threshold_ms` üstü
/// sessizlikte force reconnect (zombie detection).
pub async fn run_rtds_task(
    ws_url: String,
    symbol: String,
    stale_threshold_ms: u64,
    max_backoff_ms: u64,
    state: SharedRtdsState,
    bot_id: i64,
) {
    let label = bot_id.to_string();
    let mut backoff_ms: u64 = 1_000;
    let max_backoff = max_backoff_ms.max(1_000);
    loop {
        ipc::log_line(
            &label,
            format!("🌐 RTDS connecting (symbol={symbol}) → {ws_url}"),
        );
        let res = connect_and_stream(&ws_url, &symbol, stale_threshold_ms, &state, &label).await;
        match res {
            Ok(()) => {
                ipc::log_line(
                    &label,
                    format!("⚠️  RTDS ws closed, reconnect in {}s", backoff_ms / 1000),
                );
            }
            Err(e) => {
                ipc::log_line(
                    &label,
                    format!(
                        "❌ RTDS ws error: {e} (reconnect in {}s)",
                        backoff_ms / 1000
                    ),
                );
            }
        }
        tokio::time::sleep(Duration::from_millis(backoff_ms)).await;
        backoff_ms = (backoff_ms * 2).min(max_backoff);
    }
}

async fn connect_and_stream(
    ws_url: &str,
    symbol: &str,
    stale_threshold_ms: u64,
    state: &SharedRtdsState,
    label: &str,
) -> anyhow::Result<()> {
    let (ws_stream, _) = match timeout(CONNECT_TIMEOUT, tokio_tungstenite::connect_async(ws_url))
        .await
    {
        Ok(res) => res?,
        Err(_) => return Err(anyhow::anyhow!("connect timeout ({}s)", CONNECT_TIMEOUT.as_secs())),
    };
    let (mut write, mut read) = ws_stream.split();

    // RTDS server `filters` alanını stringified JSON olarak bekler (regex `[{\[]…`).
    let filter_str = serde_json::json!({ "symbol": symbol }).to_string();
    let subscribe = serde_json::json!({
        "action": "subscribe",
        "subscriptions": [{
            "topic": "crypto_prices_chainlink",
            "type": "update",
            "filters": filter_str,
        }],
    })
    .to_string();
    write
        .send(Message::Text(Utf8Bytes::from(subscribe)))
        .await
        .map_err(|e| anyhow::anyhow!("subscribe send: {e}"))?;

    let mut ping_tick = interval(PING_INTERVAL);
    ping_tick.tick().await;

    let mut stale_tick = interval(Duration::from_secs(5));
    stale_tick.tick().await;
    let stale_ms = stale_threshold_ms.max(5_000);

    loop {
        tokio::select! {
            _ = ping_tick.tick() => {
                // RTDS text "PING" keep-alive (WS protocol ping değil).
                if write.send(Message::Text(Utf8Bytes::from_static("PING"))).await.is_err() {
                    return Err(anyhow::anyhow!("ping send failed"));
                }
            }
            _ = stale_tick.tick() => {
                let last = state.read().await.last_tick_ms;
                if last > 0 && now_ms().saturating_sub(last) > stale_ms {
                    return Err(anyhow::anyhow!(
                        "stale feed: no tick for > {}ms — force reconnect", stale_ms
                    ));
                }
            }
            next = read.next() => {
                let msg = match next {
                    Some(Ok(m)) => m,
                    Some(Err(e)) => return Err(anyhow::anyhow!("read: {e}")),
                    None => return Ok(()),
                };
                match msg {
                    Message::Text(t) => {
                        handle_text(t.as_ref(), symbol, state, label).await;
                    }
                    Message::Binary(b) => {
                        if let Ok(s) = std::str::from_utf8(&b) {
                            handle_text(s, symbol, state, label).await;
                        }
                    }
                    Message::Ping(p) => {
                        if write.send(Message::Pong(p)).await.is_err() {
                            return Err(anyhow::anyhow!("pong send failed"));
                        }
                    }
                    Message::Close(_) => return Ok(()),
                    _ => {}
                }
            }
        }
    }
}

async fn handle_text(text: &str, symbol: &str, state: &SharedRtdsState, label: &str) {
    let trimmed = text.trim();
    if trimmed.is_empty()
        || trimmed.eq_ignore_ascii_case("pong")
        || trimmed.eq_ignore_ascii_case("ping")
    {
        return;
    }
    let Ok(env) = serde_json::from_str::<RtdsEnvelope>(trimmed) else {
        return;
    };
    if env.topic != Some("crypto_prices_chainlink") {
        return;
    }
    if env.kind.is_some() && env.kind != Some("update") {
        return;
    }
    let Some(payload) = env.payload else {
        return;
    };
    if !payload.symbol.is_empty() && !payload.symbol.eq_ignore_ascii_case(symbol) {
        return;
    }
    if !sane_price(symbol, payload.value) {
        tracing::warn!(bot=label, symbol, value=payload.value, "rtds insane value rejected");
        return;
    }

    let mut s = state.write().await;
    let was_none = s.window_open_price.is_none();
    s.current_price = payload.value;
    let now = now_ms();
    s.last_tick_ms = now;

    // Velocity için payload ts tercih (yoksa now).
    let sample_ts = if payload.timestamp > 0 {
        payload.timestamp
    } else {
        now
    };
    s.recent_samples.push_back((sample_ts, payload.value));
    let cutoff = sample_ts.saturating_sub(VELOCITY_WINDOW_MS);
    while let Some(&(t, _)) = s.recent_samples.front() {
        if t >= cutoff {
            break;
        }
        s.recent_samples.pop_front();
    }
    s.recent_velocity_bps_per_sec = compute_velocity(&s.recent_samples);

    if was_none && payload.timestamp > 0 && payload.timestamp >= s.window_start_ts_ms {
        s.window_open_price = Some(payload.value);
        s.window_open_ts_ms = Some(payload.timestamp);
        drop(s);
        ipc::log_line(
            label,
            format!(
                "🌐 RTDS window_open={:.4} (ts={})",
                payload.value, payload.timestamp
            ),
        );
        return;
    }

    if let Some(open) = s.window_open_price {
        if open > 0.0 {
            s.window_delta_bps = (payload.value - open) / open * 10_000.0;
        }
    }
}
