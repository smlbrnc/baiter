//! Polymarket RTDS Chainlink feed — `Arc<RwLock<RtdsState>>` ile strateji
//! katmanına anlık fiyat + window delta sinyali yayar.

use std::sync::Arc;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tokio::time::{interval, timeout};
use tokio_tungstenite::tungstenite::{Message, Utf8Bytes};

use crate::ipc;
use crate::time::now_ms;

/// RTDS task'ının strateji katmanına açtığı anlık durum.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RtdsState {
    /// Son Chainlink tick fiyatı (USD).
    pub current_price: f64,
    /// Pencere açılışında yakalanan ilk tick. `None` → henüz yakalanmadı.
    pub window_open_price: Option<f64>,
    /// `window_open_price` yakalandığında payload zaman damgası (unix ms).
    pub window_open_ts_ms: Option<u64>,
    /// Güncel pencere başlangıcı (unix ms); altındaki tick'ler open olarak yakalanmaz.
    pub window_start_ts_ms: u64,
    /// `(current − open) / open × 10_000` (bps); open `None` iken `0.0`.
    pub window_delta_bps: f64,
    /// Son tick unix ms — stale/zombie bağlantı tespiti için.
    pub last_tick_ms: u64,
    /// WS bağlantısı aktif mi.
    pub connected: bool,
}

pub type SharedRtdsState = Arc<RwLock<RtdsState>>;

pub fn new_shared_state() -> SharedRtdsState {
    Arc::new(RwLock::new(RtdsState::default()))
}

/// Pencere sınırını güncelle — `window_open_*` ve `delta` sıfırlanır;
/// `current_price`/`last_tick_ms` canlı feed için korunur.
pub async fn reset_window(state: &SharedRtdsState, window_start_ts_ms: u64) {
    let mut s = state.write().await;
    s.window_open_price = None;
    s.window_open_ts_ms = None;
    s.window_start_ts_ms = window_start_ts_ms;
    s.window_delta_bps = 0.0;
}

/// `window_delta_bps` → `[0, 10]` skor (`5.0` = nötr). Piecewise linear; 5-dk
/// market için kalibre, daha uzun pencerede [`interval_scale`] ile `√T` ölçeklenir.
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

/// `√T` volatilite ölçeği (GBM yaklaşımı); 5-dk baseline `1.0`.
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

/// Composite'i `signal_weight` (0-10) ile skala eder (`binance::effective_score`
/// ile aynı semantik); `0 → 5.0`, `10 → ham composite`.
pub fn effective_composite(composite: f64, signal_weight: f64) -> f64 {
    5.0 + (composite - 5.0) * (signal_weight / 10.0)
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

/// RTDS WS task'ı: kopunca exponential backoff ile yeniden bağlanır.
/// `stale_threshold_ms` boyunca tick yoksa force reconnect (zombie detection).
pub async fn run_rtds_task(
    ws_url: String,
    symbol: String,
    stale_threshold_ms: u64,
    max_backoff_ms: u64,
    state: SharedRtdsState,
    bot_id: i64,
) {
    let label = bot_id.to_string();
    ipc::log_line(
        &label,
        format!("🌐 RTDS task starting (symbol={symbol}, url={ws_url})"),
    );
    let mut backoff_ms: u64 = 1_000;
    let max_backoff = max_backoff_ms.max(1_000);
    loop {
        ipc::log_line(&label, format!("🌐 RTDS connecting symbol={symbol}"));
        let res = connect_and_stream(&ws_url, &symbol, stale_threshold_ms, &state, &label).await;
        {
            let mut s = state.write().await;
            s.connected = false;
        }
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
    ipc::log_line(label, "🌐 RTDS connected");

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

    {
        let mut s = state.write().await;
        s.connected = true;
    }

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
    s.last_tick_ms = now_ms();

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interval_scale_values() {
        assert!((interval_scale(300) - 1.0).abs() < 0.01);
        assert!((interval_scale(900) - 1.73).abs() < 0.01);
        assert!((interval_scale(3600) - 3.46).abs() < 0.01);
        assert!((interval_scale(14400) - 6.93).abs() < 0.01);
        assert!((interval_scale(1234) - 1.0).abs() < 0.01);
    }

    #[test]
    fn window_delta_score_neutral_at_zero() {
        assert!((window_delta_score(0.0, 1.0) - 5.0).abs() < 1e-6);
    }

    #[test]
    fn window_delta_score_monotonic() {
        // Artan |bps| → skor nötrden uzaklaşır.
        let s_small = window_delta_score(0.5, 1.0);
        let s_mid = window_delta_score(2.0, 1.0);
        let s_edge = window_delta_score(7.0, 1.0);
        let s_big = window_delta_score(15.0, 1.0);
        assert!(s_small > 5.0 && s_small < s_mid);
        assert!(s_mid < s_edge);
        assert!(s_edge < s_big);
        assert!((s_big - 10.0).abs() < 1e-6);
    }

    #[test]
    fn window_delta_score_symmetry() {
        for bps in [0.5, 2.0, 7.0, 15.0, 30.0] {
            let up = window_delta_score(bps, 1.0);
            let down = window_delta_score(-bps, 1.0);
            assert!((up + down - 10.0).abs() < 1e-6, "bps={bps} up={up} down={down}");
        }
    }

    #[test]
    fn window_delta_score_edge_threshold() {
        // Oracle-lag-sniper edge eşiği: 7 bps → ~3.9 puanlık sapma → ~8.9 skor.
        let s = window_delta_score(7.0, 1.0);
        assert!((s - 8.9).abs() < 0.05, "got {s}");
    }

    #[test]
    fn window_delta_score_scaled_by_interval() {
        // 15-dk marketinde 7 bps = 5-dk karşılığı ~4 bps → daha zayıf skor.
        let s5 = window_delta_score(7.0, interval_scale(300));
        let s15 = window_delta_score(7.0, interval_scale(900));
        assert!(s15 < s5, "s5={s5} s15={s15}");
        assert!(s15 > 5.0);
    }

    #[test]
    fn window_delta_score_clamps() {
        assert!((window_delta_score(1e6, 1.0) - 10.0).abs() < 1e-6);
        assert!(window_delta_score(-1e6, 1.0).abs() < 1e-6);
    }

    #[test]
    fn composite_score_graceful_degrade() {
        // RTDS kopuk (window=5.0) → composite = 0.7*5 + 0.3*b
        let c = composite_score(5.0, 8.0, 0.70);
        assert!((c - (0.7 * 5.0 + 0.3 * 8.0)).abs() < 1e-6);
    }

    #[test]
    fn composite_score_weight_zero_uses_binance() {
        assert!((composite_score(10.0, 2.0, 0.0) - 2.0).abs() < 1e-6);
    }

    #[test]
    fn composite_score_weight_one_uses_window() {
        assert!((composite_score(8.0, 2.0, 1.0) - 8.0).abs() < 1e-6);
    }

    #[test]
    fn composite_score_clamps() {
        assert!((composite_score(11.0, 11.0, 0.5) - 10.0).abs() < 1e-6);
        assert!(composite_score(-1.0, -1.0, 0.5).abs() < 1e-6);
    }

    #[test]
    fn effective_composite_zero_weight_neutral() {
        assert!((effective_composite(8.0, 0.0) - 5.0).abs() < 1e-9);
        assert!((effective_composite(2.0, 0.0) - 5.0).abs() < 1e-9);
    }

    #[test]
    fn effective_composite_full_weight_raw() {
        assert!((effective_composite(8.0, 10.0) - 8.0).abs() < 1e-9);
    }

    #[test]
    fn sane_price_ranges() {
        assert!(sane_price("btc/usd", 67_000.0));
        assert!(!sane_price("btc/usd", 100.0));
        assert!(!sane_price("btc/usd", 20_000_000.0));
        assert!(sane_price("eth/usd", 3_500.0));
        assert!(sane_price("sol/usd", 150.0));
        assert!(sane_price("xrp/usd", 0.6));
        assert!(!sane_price("btc/usd", f64::NAN));
        assert!(!sane_price("btc/usd", -1.0));
    }

    #[tokio::test]
    async fn reset_window_clears_open() {
        let st = new_shared_state();
        {
            let mut s = st.write().await;
            s.window_open_price = Some(67_000.0);
            s.window_open_ts_ms = Some(100);
            s.window_delta_bps = 12.5;
            s.window_start_ts_ms = 100;
        }
        reset_window(&st, 12_345).await;
        let s = st.read().await;
        assert!(s.window_open_price.is_none());
        assert!(s.window_open_ts_ms.is_none());
        assert_eq!(s.window_delta_bps, 0.0);
        assert_eq!(s.window_start_ts_ms, 12_345);
    }

    #[tokio::test]
    async fn handle_text_captures_first_tick_and_updates_delta() {
        let st = new_shared_state();
        reset_window(&st, 1_000_000).await;
        // Boundary öncesi tick → yok sayılmalı (window_open kalmasın).
        let pre = r#"{"topic":"crypto_prices_chainlink","type":"update","payload":{"symbol":"btc/usd","timestamp":999999,"value":67000.0}}"#;
        handle_text(pre, "btc/usd", &st, "t").await;
        assert!(st.read().await.window_open_price.is_none());

        // Boundary sonrası ilk tick → window_open yakalanır.
        let first = r#"{"topic":"crypto_prices_chainlink","type":"update","payload":{"symbol":"btc/usd","timestamp":1000001,"value":67000.0}}"#;
        handle_text(first, "btc/usd", &st, "t").await;
        {
            let s = st.read().await;
            assert_eq!(s.window_open_price, Some(67_000.0));
            assert_eq!(s.window_open_ts_ms, Some(1_000_001));
            assert_eq!(s.window_delta_bps, 0.0);
        }

        // İkinci tick → delta hesaplanır.
        let up = r#"{"topic":"crypto_prices_chainlink","type":"update","payload":{"symbol":"btc/usd","timestamp":1000005,"value":67067.0}}"#;
        handle_text(up, "btc/usd", &st, "t").await;
        let s = st.read().await;
        assert_eq!(s.current_price, 67_067.0);
        assert_eq!(s.window_open_price, Some(67_000.0));
        // (67067-67000)/67000*10_000 = 10.0 bps
        assert!((s.window_delta_bps - 10.0).abs() < 0.01);
    }

    #[tokio::test]
    async fn handle_text_ignores_unrelated_topic_and_symbol() {
        let st = new_shared_state();
        reset_window(&st, 0).await;
        handle_text(
            r#"{"topic":"other","payload":{"symbol":"btc/usd","timestamp":1,"value":1.0}}"#,
            "btc/usd",
            &st,
            "t",
        )
        .await;
        handle_text(
            r#"{"topic":"crypto_prices_chainlink","payload":{"symbol":"eth/usd","timestamp":1,"value":3000.0}}"#,
            "btc/usd",
            &st,
            "t",
        )
        .await;
        let s = st.read().await;
        assert_eq!(s.current_price, 0.0);
        assert!(s.window_open_price.is_none());
    }

    #[tokio::test]
    async fn handle_text_rejects_insane_value() {
        let st = new_shared_state();
        reset_window(&st, 0).await;
        let bad = r#"{"topic":"crypto_prices_chainlink","type":"update","payload":{"symbol":"btc/usd","timestamp":1,"value":-5.0}}"#;
        handle_text(bad, "btc/usd", &st, "t").await;
        assert_eq!(st.read().await.current_price, 0.0);
    }
}
