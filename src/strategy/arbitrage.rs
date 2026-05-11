//! Arbitrage stratejisi — pure cross-leg FAK BID arbitrage.
//!
//! Strateji "sentetik dolar" minting:
//!   - Winner side (bid > 0.5) için: FAK BID limit BUY (taker @ bid)
//!   - Loser side (bid < 0.5) için: FAK BID limit BUY (taker @ bid)
//!   - Toplam ödenen: `winner_bid + loser_bid` (avg_sum < cost_max)
//!   - Garanti payoff: $1.00 (kim kazansa)
//!   - Net = 1.00 - cost - fees → matematiksel olarak pozitif
//!
//! Bot 108 16 200-kombinasyon backtest sonucu (cost<0.95, mt=5, $100):
//!   - **WR %100, ROI %4.35, NET +$994/12.4h, avg_sum 0.92**
//!   - 120/160 session aktif (sıkı eşik nedeniyle %75 trigger)
//!
//! ## Karar zinciri
//!
//! 1. **Window**: `now ∈ [start, end]` ve OB ready.
//! 2. **Tick interval**: `now − last_check < tick_interval_ms` ise NoOp.
//! 3. **Cooldown**: `now − last_buy < cooldown_ms` ise NoOp.
//! 4. **Max trades cap**: `n_trades >= max_trades_per_session` ise NoOp.
//! 5. **Winner side belirleme**: `bid_up > 0.5 && bid_dn <= 0.5` → UP winner, vice versa.
//! 6. **Cost check**: `winner_bid + loser_bid < cost_max` (örn 0.95) → fırsat.
//! 7. **PlanOrders**: 2 adet GTC limit BUY (FAK olarak engine fill-and-cancel
//!    yapacak; 0.0 cooldown_threshold ile kullanılırsa).
//!    - Outcome::Up : price = up_bid, size = ceil(order_usdc / cost_per)
//!    - Outcome::Down : price = down_bid, size = ceil(order_usdc / cost_per)
//! 8. **avg_sum<1 garantili** çünkü cost_max < 1.0.
//!
//! ## Reason etiketleri
//!
//! `arbitrage:winner:{up,down}` — winner side leg.
//! `arbitrage:loser:{up,down}` — loser side leg.

use serde::{Deserialize, Serialize};

use super::common::{Decision, PlannedOrder, StrategyContext};
use crate::types::{OrderType, Outcome, Side};

#[inline]
const fn reason_winner(dir: Outcome) -> &'static str {
    match dir {
        Outcome::Up => "arbitrage:winner:up",
        Outcome::Down => "arbitrage:winner:down",
    }
}

#[inline]
const fn reason_loser(dir: Outcome) -> &'static str {
    match dir {
        Outcome::Up => "arbitrage:loser:up",
        Outcome::Down => "arbitrage:loser:down",
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub enum ArbitrageState {
    #[default]
    Idle,
    Active(ArbitrageActive),
    Done,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ArbitrageActive {
    /// Son tick check zamanı (tick interval throttling).
    #[serde(default)]
    pub last_check_ms: u64,
    /// Son BUY zamanı (cooldown).
    #[serde(default)]
    pub last_buy_ms: u64,
    /// Bu session'da kaç arbitrage trade yapıldı.
    #[serde(default)]
    pub n_trades: u32,
}

pub struct ArbitrageEngine;

impl ArbitrageEngine {
    pub fn decide(
        state: ArbitrageState,
        ctx: &StrategyContext<'_>,
    ) -> (ArbitrageState, Decision) {
        let p = ctx.strategy_params;

        match state {
            ArbitrageState::Done => (ArbitrageState::Done, Decision::NoOp),

            ArbitrageState::Idle => {
                let book_ready = ctx.up_best_bid > 0.0
                    && ctx.up_best_ask > 0.0
                    && ctx.down_best_bid > 0.0
                    && ctx.down_best_ask > 0.0;
                if !book_ready {
                    return (ArbitrageState::Idle, Decision::NoOp);
                }
                (ArbitrageState::Active(ArbitrageActive::default()), Decision::NoOp)
            }

            ArbitrageState::Active(mut st) => {
                if ctx.up_best_bid <= 0.0 || ctx.down_best_bid <= 0.0 {
                    return (ArbitrageState::Active(st), Decision::NoOp);
                }

                // ── Tick interval throttle ──
                let tick_interval_ms = p.arbitrage_tick_interval_ms();
                if st.last_check_ms > 0
                    && ctx.now_ms.saturating_sub(st.last_check_ms) < tick_interval_ms
                {
                    return (ArbitrageState::Active(st), Decision::NoOp);
                }
                st.last_check_ms = ctx.now_ms;

                // ── Cooldown ──
                let cooldown_ms = p.arbitrage_cooldown_ms();
                if st.last_buy_ms > 0
                    && ctx.now_ms.saturating_sub(st.last_buy_ms) < cooldown_ms
                {
                    return (ArbitrageState::Active(st), Decision::NoOp);
                }

                // ── Max trades per session cap ──
                let max_trades = p.arbitrage_max_trades_per_session();
                if max_trades > 0 && st.n_trades >= max_trades {
                    return (ArbitrageState::Active(st), Decision::NoOp);
                }

                // ── Entry window kontrolü ──
                let entry_window_secs = p.arbitrage_entry_window_secs() as f64;
                if let Some(to_end) = ctx.market_remaining_secs {
                    if to_end > entry_window_secs || to_end <= 0.0 {
                        return (ArbitrageState::Active(st), Decision::NoOp);
                    }
                }

                // ── Winner side belirleme ──
                // bid > 0.5 → o taraf winner kandidatı (yükselen)
                let (winner_dir, w_bid, l_bid) = if ctx.up_best_bid > 0.5
                    && ctx.down_best_bid <= 0.5
                {
                    (Outcome::Up, ctx.up_best_bid, ctx.down_best_bid)
                } else if ctx.down_best_bid > 0.5 && ctx.up_best_bid <= 0.5 {
                    (Outcome::Down, ctx.down_best_bid, ctx.up_best_bid)
                } else {
                    // İkisi de bid>0.5 (overlapping) veya ikisi de <0.5 (eşit) → fırsat yok
                    return (ArbitrageState::Active(st), Decision::NoOp);
                };

                // ── Cost check (avg_sum < cost_max garantisi) ──
                let cost_per = w_bid + l_bid;
                let cost_max = p.arbitrage_cost_max();
                if cost_per >= cost_max {
                    return (ArbitrageState::Active(st), Decision::NoOp);
                }

                // ── Loser side belirleme ──
                let loser_dir = match winner_dir {
                    Outcome::Up => Outcome::Down,
                    Outcome::Down => Outcome::Up,
                };

                // ── Size hesabı ──
                let order_usdc = p.arbitrage_order_usdc();
                if order_usdc <= 0.0 {
                    return (ArbitrageState::Active(st), Decision::NoOp);
                }
                let size = (order_usdc / cost_per).ceil();
                if size <= 0.0 {
                    return (ArbitrageState::Active(st), Decision::NoOp);
                }

                // ── 2 leg BUY emir oluştur ──
                let winner_order =
                    make_buy(ctx, winner_dir, w_bid, size, reason_winner(winner_dir));
                let loser_order =
                    make_buy(ctx, loser_dir, l_bid, size, reason_loser(loser_dir));

                let mut orders = Vec::with_capacity(2);
                if let Some(o) = winner_order {
                    orders.push(o);
                }
                if let Some(o) = loser_order {
                    orders.push(o);
                }
                if orders.is_empty() {
                    return (ArbitrageState::Active(st), Decision::NoOp);
                }

                st.last_buy_ms = ctx.now_ms;
                st.n_trades = st.n_trades.saturating_add(1);
                (
                    ArbitrageState::Active(st),
                    Decision::PlaceOrders(orders),
                )
            }
        }
    }
}

/// BUY GTC limit emir (FAK davranışı için engine `cooldown_threshold=0` ile
/// kullanılmalı; aksi halde post-only kalır). `price ≤ 0` veya `size ≤ 0` →
/// `None`. `min_order_size` altıysa engine reddedecek.
fn make_buy(
    ctx: &StrategyContext<'_>,
    outcome: Outcome,
    price: f64,
    size: f64,
    reason: &str,
) -> Option<PlannedOrder> {
    if price <= 0.0 || size <= 0.0 {
        return None;
    }
    if size * price < ctx.api_min_order_size {
        return None;
    }
    Some(PlannedOrder {
        outcome,
        token_id: ctx.token_id(outcome).to_string(),
        side: Side::Buy,
        price,
        size,
        order_type: OrderType::Gtc,
        reason: reason.to_string(),
    })
}
