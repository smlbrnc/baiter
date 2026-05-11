//! Binance Latency Arbitrage stratejisi.
//!
//! Polymarket BTC 5dk markette, Binance Spot BTC/USDT fiyatının lag'ini
//! sömürür: Polymarket bid/ask güncellenmeden önce Binance fiyatı hareket
//! ettiğinde yönü erken yakalar.
//!
//! ## Karar zinciri
//!
//! 1. **BTC oracle ready**: `btc_open_price` (start_ts snapshot) + güncel
//!    `btc_current_price` mevcut mu?
//! 2. **Window guard**: `now ∈ [start, end]` ve `to_end <= entry_window_secs`
//!    (default 300 = tüm pencere).
//! 3. **Cooldown**: `now − last_buy < cooldown_ms` ise NoOp.
//! 4. **Max trades cap**: `n_trades >= max_trades_per_session` ise NoOp.
//! 5. **Sinyal**: `delta = current − open`. `|delta| < sig_thr` (USD) ise NoOp.
//! 6. **Yön**: `delta > 0 → UP`, `delta < 0 → DOWN`.
//! 7. **Fiyat guard**: `bid ∈ [min_price, max_price]` ve `ask < 0.99`.
//! 8. **Size**: `ceil(order_usdc / ask)`, `min_order_size` altıysa skip.
//! 9. **PlaceOrder**: GTC limit BUY @ ask (taker).
//!
//! ## Backtest (bot 91, 64h, 665 session)
//!
//! | Profil | sig | mt | cd  | WR  | NET     | ROI   |
//! |--------|----:|---:|----:|----:|--------:|------:|
//! | A      | $80 |  3 | 3s  | 93% | +$2 222 | +9.1% |
//! | B      | $50 | 10 | 3s  | 89% | +$8 323 | +4.8% |
//! | C      | $50 | 50 | 3s  | 91% | +$12808 | +3.2% |
//!
//! Default: Profil B (sig=$50, mt=10, cd=3000ms — denge: yüksek WR + yıllık $1.14M).
//!
//! ## Reason etiketleri
//!
//! `binance_latency:up:$+50.3` — UP BUY, delta +$50.3
//! `binance_latency:down:$-72.1` — DOWN BUY, delta −$72.1

use serde::{Deserialize, Serialize};

use super::common::{Decision, PlannedOrder, StrategyContext};
use crate::types::{OrderType, Outcome, Side};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub enum BinanceLatencyState {
    #[default]
    Idle,
    Active(BinanceLatencyActive),
    Done,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BinanceLatencyActive {
    /// Session başlangıç (start_ts) anındaki BTC mid; oracle henüz hazır
    /// değilse `None` ve ilk hazır olduğu tick'te set edilir.
    #[serde(default)]
    pub btc_open_price: Option<f64>,
    /// Son BUY zamanı (cooldown).
    #[serde(default)]
    pub last_buy_ms: u64,
    /// Bu session'da kaç trade yapıldı (cap için).
    #[serde(default)]
    pub n_trades: u32,
}

pub struct BinanceLatencyEngine;

impl BinanceLatencyEngine {
    /// `btc_current_price` strategi context'inden değil, dışarıdan parametre
    /// olarak geliyor (engine `tick()` içinde sync read yapar). Open price
    /// ilk valid tick'te lazy snapshot edilir.
    pub fn decide(
        state: BinanceLatencyState,
        ctx: &StrategyContext<'_>,
        btc_current_price: Option<f64>,
    ) -> (BinanceLatencyState, Decision) {
        let p = ctx.strategy_params;

        match state {
            BinanceLatencyState::Done => (BinanceLatencyState::Done, Decision::NoOp),

            BinanceLatencyState::Idle => {
                let book_ready = ctx.up_best_bid > 0.0
                    && ctx.up_best_ask > 0.0
                    && ctx.down_best_bid > 0.0
                    && ctx.down_best_ask > 0.0;
                if !book_ready {
                    return (BinanceLatencyState::Idle, Decision::NoOp);
                }
                let mut st = BinanceLatencyActive::default();
                if let Some(px) = btc_current_price {
                    st.btc_open_price = Some(px);
                }
                (BinanceLatencyState::Active(st), Decision::NoOp)
            }

            BinanceLatencyState::Active(mut st) => {
                if st.btc_open_price.is_none() {
                    if let Some(px) = btc_current_price {
                        st.btc_open_price = Some(px);
                    }
                }

                let (Some(open), Some(now_px)) = (st.btc_open_price, btc_current_price) else {
                    return (BinanceLatencyState::Active(st), Decision::NoOp);
                };

                let entry_window_secs = p.binance_latency_entry_window_secs() as f64;
                if let Some(to_end) = ctx.market_remaining_secs {
                    if to_end > entry_window_secs || to_end <= 0.0 {
                        return (BinanceLatencyState::Active(st), Decision::NoOp);
                    }
                }

                let cooldown_ms = p.binance_latency_cooldown_ms();
                if st.last_buy_ms > 0
                    && ctx.now_ms.saturating_sub(st.last_buy_ms) < cooldown_ms
                {
                    return (BinanceLatencyState::Active(st), Decision::NoOp);
                }

                let max_trades = p.binance_latency_max_trades_per_session();
                if max_trades > 0 && st.n_trades >= max_trades {
                    return (BinanceLatencyState::Active(st), Decision::NoOp);
                }

                let delta = now_px - open;
                let sig_thr = p.binance_latency_sig_thr_usd();
                if delta.abs() < sig_thr {
                    return (BinanceLatencyState::Active(st), Decision::NoOp);
                }

                let dir = if delta > 0.0 { Outcome::Up } else { Outcome::Down };
                let bid = ctx.best_bid(dir);
                let ask = ctx.best_ask(dir);
                if bid <= 0.0 || ask <= 0.0 {
                    return (BinanceLatencyState::Active(st), Decision::NoOp);
                }
                if bid < ctx.min_price || bid > ctx.max_price {
                    return (BinanceLatencyState::Active(st), Decision::NoOp);
                }
                if ask >= 0.99 {
                    return (BinanceLatencyState::Active(st), Decision::NoOp);
                }

                let order_usdc = p.binance_latency_order_usdc();
                if order_usdc <= 0.0 {
                    return (BinanceLatencyState::Active(st), Decision::NoOp);
                }
                let size = (order_usdc / ask).ceil();
                if size <= 0.0 || size * ask < ctx.api_min_order_size {
                    return (BinanceLatencyState::Active(st), Decision::NoOp);
                }

                let reason = format!(
                    "binance_latency:{}:${:+.1}",
                    dir.as_lowercase(),
                    delta
                );
                let main_order = PlannedOrder {
                    outcome: dir,
                    token_id: ctx.token_id(dir).to_string(),
                    side: Side::Buy,
                    price: ask,
                    size,
                    order_type: OrderType::Gtc,
                    reason,
                };

                // ── Hedge leg (loser scalp; opt-in) ──
                // Karşı taraf bid çok düşükse FAK BID hedge — yön yanlışsa
                // tam payoff sigortası. Backtest: hedge>0 NET'i azaltır
                // (matematik aleyhine), opt-in olarak default 0 (kapalı).
                let hedge_usdc = p.binance_latency_hedge_usdc();
                let mut orders = vec![main_order];
                if hedge_usdc > 0.0 {
                    let opp_dir = dir.opposite();
                    let opp_bid = ctx.best_bid(opp_dir);
                    let hedge_max_bid = p.binance_latency_hedge_max_bid();
                    if opp_bid >= 0.01 && opp_bid <= hedge_max_bid {
                        let hedge_size = (hedge_usdc / opp_bid).ceil();
                        if hedge_size > 0.0 && hedge_size * opp_bid >= ctx.api_min_order_size {
                            let hedge_reason = format!(
                                "binance_latency:hedge:{}:bid={:.3}",
                                opp_dir.as_lowercase(),
                                opp_bid
                            );
                            orders.push(PlannedOrder {
                                outcome: opp_dir,
                                token_id: ctx.token_id(opp_dir).to_string(),
                                side: Side::Buy,
                                price: opp_bid,
                                size: hedge_size,
                                order_type: OrderType::Gtc,
                                reason: hedge_reason,
                            });
                        }
                    }
                }

                st.last_buy_ms = ctx.now_ms;
                st.n_trades = st.n_trades.saturating_add(1);
                (
                    BinanceLatencyState::Active(st),
                    Decision::PlaceOrders(orders),
                )
            }
        }
    }
}
