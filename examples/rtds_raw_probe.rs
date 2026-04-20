//! Ham RTDS subscribe test aracı — farklı subscribe formatlarını dener
//! ve ilk 20 sn'deki TÜM mesajları (ACK, error, update) dump eder.

use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use tokio::time::timeout;
use tokio_tungstenite::tungstenite::{Message, Utf8Bytes};

#[tokio::main]
async fn main() {
    let _ = rustls::crypto::ring::default_provider().install_default();

    let url = "wss://ws-live-data.polymarket.com";
    let symbol = "btc/usd";

    // Farklı subscribe varyantlarını sırayla test et.
    let variants: Vec<(&str, serde_json::Value)> = vec![
        (
            "A: filters=string",
            serde_json::json!({
                "action": "subscribe",
                "subscriptions": [{
                    "topic": "crypto_prices_chainlink",
                    "type": "update",
                    "filters": symbol,
                }],
            }),
        ),
        (
            "B: filters={symbol}",
            serde_json::json!({
                "action": "subscribe",
                "subscriptions": [{
                    "topic": "crypto_prices_chainlink",
                    "type": "update",
                    "filters": { "symbol": symbol },
                }],
            }),
        ),
        (
            "C: filter (singular)=string",
            serde_json::json!({
                "action": "subscribe",
                "subscriptions": [{
                    "topic": "crypto_prices_chainlink",
                    "type": "update",
                    "filter": symbol,
                }],
            }),
        ),
        (
            "D: no filter",
            serde_json::json!({
                "action": "subscribe",
                "subscriptions": [{
                    "topic": "crypto_prices_chainlink",
                    "type": "update",
                }],
            }),
        ),
        (
            "E: symbols-array",
            serde_json::json!({
                "action": "subscribe",
                "subscriptions": [{
                    "topic": "crypto_prices_chainlink",
                    "type": "update",
                    "symbols": [symbol],
                }],
            }),
        ),
        (
            "F: filters=stringified JSON",
            serde_json::json!({
                "action": "subscribe",
                "subscriptions": [{
                    "topic": "crypto_prices_chainlink",
                    "type": "update",
                    "filters": serde_json::json!({ "symbol": symbol }).to_string(),
                }],
            }),
        ),
        (
            "G: filters=stringified array",
            serde_json::json!({
                "action": "subscribe",
                "subscriptions": [{
                    "topic": "crypto_prices_chainlink",
                    "type": "update",
                    "filters": format!("[\"{symbol}\"]"),
                }],
            }),
        ),
    ];

    for (name, sub) in variants {
        println!("\n================= {name} =================");
        println!(">>> SEND: {sub}");
        match test_subscribe(url, sub).await {
            Ok(n) => println!("<<< received {n} messages"),
            Err(e) => println!("<<< ERROR: {e}"),
        }
    }
}

async fn test_subscribe(url: &str, sub: serde_json::Value) -> Result<usize, String> {
    let (ws, _) = tokio_tungstenite::connect_async(url)
        .await
        .map_err(|e| format!("connect: {e}"))?;
    let (mut w, mut r) = ws.split();

    w.send(Message::Text(Utf8Bytes::from(sub.to_string())))
        .await
        .map_err(|e| format!("send: {e}"))?;

    let mut count = 0usize;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(8);
    while tokio::time::Instant::now() < deadline {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        let res = timeout(remaining, r.next()).await;
        match res {
            Ok(Some(Ok(Message::Text(t)))) => {
                count += 1;
                let s: &str = t.as_ref();
                let preview: String = s.chars().take(220).collect();
                println!("<<< TEXT[{count}]: {preview}");
            }
            Ok(Some(Ok(Message::Binary(b)))) => {
                count += 1;
                if let Ok(s) = std::str::from_utf8(&b) {
                    let preview: String = s.chars().take(220).collect();
                    println!("<<< BIN[{count}]: {preview}");
                } else {
                    println!("<<< BIN[{count}]: {} bytes (non-utf8)", b.len());
                }
            }
            Ok(Some(Ok(Message::Ping(p)))) => {
                println!("<<< PING ({} bytes)", p.len());
                let _ = w.send(Message::Pong(p)).await;
            }
            Ok(Some(Ok(Message::Pong(_)))) => println!("<<< PONG"),
            Ok(Some(Ok(Message::Close(f)))) => {
                println!("<<< CLOSE: {f:?}");
                break;
            }
            Ok(Some(Ok(_))) => {}
            Ok(Some(Err(e))) => return Err(format!("read: {e}")),
            Ok(None) => {
                println!("<<< stream ended");
                break;
            }
            Err(_) => break,
        }
    }
    Ok(count)
}
