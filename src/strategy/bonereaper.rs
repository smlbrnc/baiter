//! Bonereaper stratejisi — sinyal tabanlı 2 saniyelik emir döngüsü.
//!
//! ## Çalışma mantığı
//!
//! Her **2 saniyede** bir karar döngüsü çalışır. Karar öncelik sırası:
//!
//! 1. POST-MARKET  — tüm emirleri iptal et, Done'a geç.
//! 2. DUTCH BOOK   — up_ask + dn_ask < $1.00 → her iki tarafa arbitraj emri.
//! 3. REBALANCE    — |up_fill − dn_fill| ≥ 5 sh → eksik tarafa emir.
//! 4. SIGNAL       — skor → UP veya DOWN, yön değiştiyse önceki signal emirleri
//!                   iptal edilir; yeni yönde `best_bid`'den GTC maker emir verilir.
//! 5. STALE CANCEL — fiyatı current bid'den STALE_SPREAD_MAX'tan fazla sapan
//!                   açık signal emirleri iptal edilir.
//!
//! ## Reason etiketleri
//!
//! `bonereaper:signal:{up,down}` — sinyal yönlü opener (her döngü)
//! `bonereaper:dutch:{up,down}`  — Dutch Book arbitraj
//! `bonereaper:rebalance:{up,down}` — rebalance fill

use serde::{Deserialize, Serialize};

use super::common::{Decision, OpenOrder, PlannedOrder, StrategyContext};
use crate::types::{OrderType, Outcome, Side};

// ─────────────────────────────────────────────
// Sabitler
// ─────────────────────────────────────────────

const TICK_INTERVAL_SECS: u64 = 2;
const POST_MARKET_WAIT: f64 = 30.0;
/// Rebalance tetiklenme eşiği: bu kadar fark oluşunca devreye gir.
const REBALANCE_TRIGGER: f64 = 5.0;
/// Minimum lot: her rebalance tick'inde en az bu kadar al.
const REBALANCE_MIN_LOT: f64 = 1.0;
/// Stale emir maksimum fiyat sapması (bid'den uzaklık).
const STALE_SPREAD_MAX: f64 = 0.05;
/// Convergence guard eşiği: karşı tarafın bid'i bu değeri geçerse o tarafa emir verilmez.
const CONVERGENCE_THRESHOLD: f64 = 0.80;

// ─────────────────────────────────────────────
// FSM State
// ─────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BonereaperState {
    /// OB henüz hazır değil; ilk tick beklenıyor.
    Idle,
    /// Market aktif — sinyal döngüsü çalışıyor.
    Active(Box<BonereaperActive>),
    /// Market kapandı ve POST_MARKET_WAIT aşıldı.
    Done,
}

impl Default for BonereaperState {
    fn default() -> Self {
        Self::Idle
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BonereaperActive {
    /// Son 2-sn döngüsünde verilen sinyal yönü (yön değişimi tespiti için).
    pub last_signal_dir: Option<Outcome>,
    /// Son işlem yapılan çift saniye (2-sn gate için).
    pub last_acted_even_sec: u64,
}

// ─────────────────────────────────────────────
// Karar motoru
// ─────────────────────────────────────────────

pub struct BonereaperEngine;

impl BonereaperEngine {
    pub fn decide(state: BonereaperState, ctx: &StrategyContext<'_>) -> (BonereaperState, Decision) {
        let to_end = ctx.market_remaining_secs.unwrap_or(f64::MAX);
        let rel_secs = (ctx.now_ms / 1000).saturating_sub(ctx.start_ts);

        match state {
            BonereaperState::Done => (BonereaperState::Done, Decision::NoOp),

            BonereaperState::Idle => {
                // OB hazır mı?
                let book_ready = ctx.up_best_bid > 0.0
                    && ctx.up_best_ask > 0.0
                    && ctx.down_best_bid > 0.0
                    && ctx.down_best_ask > 0.0;
                if !book_ready {
                    return (BonereaperState::Idle, Decision::NoOp);
                }
                if to_end < -POST_MARKET_WAIT {
                    return (BonereaperState::Done, Decision::NoOp);
                }
                // Active'e geç
                let active = BonereaperActive {
                    last_signal_dir: None,
                    last_acted_even_sec: 0,
                };
                (BonereaperState::Active(Box::new(active)), Decision::NoOp)
            }

            BonereaperState::Active(mut st) => {
                // ── POST-MARKET ──────────────────────────────────────────────
                if to_end < -POST_MARKET_WAIT {
                    let cancels = cancel_all(ctx);
                    return (BonereaperState::Done, cancels);
                }
                if to_end < 0.0 {
                    return (BonereaperState::Active(st), Decision::NoOp);
                }

                // ── 2-SANİYE GATE ───────────────────────────────────────────
                if rel_secs % TICK_INTERVAL_SECS != 0 {
                    return (BonereaperState::Active(st), Decision::NoOp);
                }
                if rel_secs == st.last_acted_even_sec {
                    return (BonereaperState::Active(st), Decision::NoOp);
                }
                st.last_acted_even_sec = rel_secs;

                // OB hazır mı?
                if ctx.up_best_bid == 0.0 || ctx.down_best_bid == 0.0 {
                    return (BonereaperState::Active(st), Decision::NoOp);
                }

                let m = ctx.metrics;

                // ── DUTCH BOOK ───────────────────────────────────────────────
                if let Some(orders) = check_dutch_book(ctx) {
                    return (BonereaperState::Active(st), Decision::PlaceOrders(orders));
                }

                // ── REBALANCE ────────────────────────────────────────────────
                let fill_imbalance = m.up_filled - m.down_filled;
                if fill_imbalance.abs() >= REBALANCE_TRIGGER {
                    let deficit = if fill_imbalance > 0.0 { Outcome::Down } else { Outcome::Up };
                    // Convergence guard: karşı taraf converge ediyorsa deficit tarafa emir verme.
                    let opp_bid = ctx.best_bid(deficit.opposite());
                    if opp_bid <= CONVERGENCE_THRESHOLD {
                        let def_bid = ctx.best_bid(deficit);
                        // Deficit taraf dominant (yükselen) ise taker ask → anında fill (parametre ile kontrol).
                        let price = if def_bid > 0.50 && ctx.strategy_params.bonereaper_rebalance_taker() {
                            ctx.best_ask(deficit)
                        } else {
                            def_bid
                        };
                        let lot = rebalance_lot(fill_imbalance);
                        if pair_cost_ok(ctx, deficit, price) {
                            let reason = format!("bonereaper:rebalance:{}", deficit.as_lowercase());
                            if let Some(order) = make_buy(ctx, deficit, price, lot, &reason) {
                                return (BonereaperState::Active(st), Decision::PlaceOrders(vec![order]));
                            }
                        }
                    }
                }

                // ── SIGNAL ───────────────────────────────────────────────────
                // signal_ready değilse (warmup tamamlanmadı) emir verilmez.
                if !ctx.signal_ready {
                    return (BonereaperState::Active(st), Decision::NoOp);
                }

                let new_dir = signal_direction(ctx);

                // Yön değiştiyse eski signal emirlerini iptal et.
                let prev_dir = st.last_signal_dir;
                st.last_signal_dir = Some(new_dir);

                if prev_dir == Some(new_dir.opposite()) {
                    // Eski yöndeki signal emirlerini iptal + yeni emir tek adımda.
                    let cancel_ids: Vec<String> = ctx
                        .open_orders
                        .iter()
                        .filter(|o| {
                            o.reason.starts_with("bonereaper:signal:")
                                && o.outcome == new_dir.opposite()
                        })
                        .map(|o| o.id.clone())
                        .collect();

                    if let Some(order) = signal_order(ctx, new_dir) {
                        if cancel_ids.is_empty() {
                            return (BonereaperState::Active(st), Decision::PlaceOrders(vec![order]));
                        }
                        return (
                            BonereaperState::Active(st),
                            Decision::CancelAndPlace {
                                cancels: cancel_ids,
                                places: vec![order],
                            },
                        );
                    }
                    if !cancel_ids.is_empty() {
                        return (BonereaperState::Active(st), Decision::CancelOrders(cancel_ids));
                    }
                    return (BonereaperState::Active(st), Decision::NoOp);
                }

                // Aynı yön: mevcut signal emirlerini iptal et (fiyat tazeleme) + yenisini koy.
                let stale_signal_ids: Vec<String> = ctx
                    .open_orders
                    .iter()
                    .filter(|o| o.reason.starts_with("bonereaper:signal:"))
                    .map(|o| o.id.clone())
                    .collect();

                if let Some(order) = signal_order(ctx, new_dir) {
                    if stale_signal_ids.is_empty() {
                        return (BonereaperState::Active(st), Decision::PlaceOrders(vec![order]));
                    }
                    return (
                        BonereaperState::Active(st),
                        Decision::CancelAndPlace {
                            cancels: stale_signal_ids,
                            places: vec![order],
                        },
                    );
                }

                // ── STALE CANCEL ─────────────────────────────────────────────
                let stale = cancel_stale(ctx);
                (BonereaperState::Active(st), stale)
            }
        }
    }
}

// ─────────────────────────────────────────────
// Sinyal yön kararı
// ─────────────────────────────────────────────

/// `effective_score > 5.0` → UP, `≤ 5.0` → DOWN.
/// Eşik yoktur; her zaman bir yön döner.
fn signal_direction(ctx: &StrategyContext<'_>) -> Outcome {
    if ctx.effective_score > 5.0 {
        Outcome::Up
    } else {
        Outcome::Down
    }
}

/// Sinyal yönünde emir:
///   bid > 0.50 (yükselen / dominant taraf) → `best_ask` taker, live'da anında fill.
///   bid ≤ 0.50 (ucuz / durağan taraf)      → `best_bid` maker, hız kritik değil.
/// Boyut: `order_usdc / price` — notional ≥ min_order_size olacak şekilde ceil kullanılır.
/// Signal emirleri tek taraflı directional bet olduğundan pair_cost_ok kontrolü uygulanmaz.
/// Convergence guard: karşı tarafın bid'i CONVERGENCE_THRESHOLD'u geçmişse None döner.
fn signal_order(ctx: &StrategyContext<'_>, dir: Outcome) -> Option<PlannedOrder> {
    // Karşı taraf converge ediyorsa bu tarafa emir verme.
    if ctx.best_bid(dir.opposite()) > CONVERGENCE_THRESHOLD {
        return None;
    }
    let bid = ctx.best_bid(dir);
    if bid <= 0.0 {
        return None;
    }
    // Dominant (yükselen) taraf taker mı? Parametre ile kontrol edilir.
    let price = if bid > 0.50 && ctx.strategy_params.bonereaper_signal_taker() {
        ctx.best_ask(dir)
    } else {
        bid
    };
    if price <= 0.0 {
        return None;
    }
    // ceil: $5 / $0.61 = 8.19 → 9 shares × $0.61 = $5.49 ≥ min_order_size
    let size = (ctx.order_usdc / price).ceil();
    let reason = format!("bonereaper:signal:{}", dir.as_lowercase());
    make_buy(ctx, dir, price, size, &reason)
}

// ─────────────────────────────────────────────
// Dutch Book
// ─────────────────────────────────────────────

fn check_dutch_book(ctx: &StrategyContext<'_>) -> Option<Vec<PlannedOrder>> {
    let up_ask = ctx.up_best_ask;
    let dn_ask = ctx.down_best_ask;
    if up_ask + dn_ask >= 1.0 || up_ask <= 0.0 || dn_ask <= 0.0 {
        return None;
    }
    let size = (ctx.order_usdc / up_ask.min(dn_ask)).floor();
    let mut orders = Vec::with_capacity(2);
    if let Some(o) = make_buy(ctx, Outcome::Up, up_ask, size, "bonereaper:dutch:up") {
        orders.push(o);
    }
    if let Some(o) = make_buy(ctx, Outcome::Down, dn_ask, size, "bonereaper:dutch:down") {
        orders.push(o);
    }
    if orders.is_empty() { None } else { Some(orders) }
}

// ─────────────────────────────────────────────
// Yardımcılar
// ─────────────────────────────────────────────

/// Rebalance lot: `max(REBALANCE_MIN_LOT, |imbalance| / 2)`.
#[inline]
fn rebalance_lot(imbalance: f64) -> f64 {
    (imbalance.abs() / 2.0).max(REBALANCE_MIN_LOT)
}

/// `side + karşı_taraf < $1.00` kontrolü.
#[inline]
fn pair_cost_ok(ctx: &StrategyContext<'_>, side: Outcome, price: f64) -> bool {
    let m = ctx.metrics;
    let opp_ref = match side.opposite() {
        Outcome::Up   => if m.up_filled   > 0.0 { m.avg_up   } else { ctx.up_best_ask   },
        Outcome::Down => if m.down_filled > 0.0 { m.avg_down } else { ctx.down_best_ask },
    };
    price + opp_ref < 1.00
}

/// BUY GTC limit emir. `price ≤ 0`, `size ≤ 0` veya notional < min → `None`.
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

/// Tüm `bonereaper:` emirlerini iptal et (post-market).
fn cancel_all(ctx: &StrategyContext<'_>) -> Decision {
    let ids: Vec<String> = ctx
        .open_orders
        .iter()
        .filter(|o| o.reason.starts_with("bonereaper:") && o.side == Side::Buy)
        .map(|o| o.id.clone())
        .collect();
    if ids.is_empty() { Decision::NoOp } else { Decision::CancelOrders(ids) }
}

/// Current bid'den `STALE_SPREAD_MAX`'tan fazla sapan signal emirlerini iptal et.
fn cancel_stale(ctx: &StrategyContext<'_>) -> Decision {
    let ids: Vec<String> = ctx
        .open_orders
        .iter()
        .filter(|o| {
            if !o.reason.starts_with("bonereaper:signal:") || o.side != Side::Buy {
                return false;
            }
            let cur_bid = ctx.best_bid(o.outcome);
            cur_bid > 0.0 && (o.price - cur_bid).abs() > STALE_SPREAD_MAX
        })
        .map(|o| o.id.clone())
        .collect();
    if ids.is_empty() { Decision::NoOp } else { Decision::CancelOrders(ids) }
}

/// Derleyici uyarısını bastır.
#[allow(dead_code)]
fn _uses_open_order(_: &OpenOrder) {}
