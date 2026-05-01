//! Bonereaper stratejisi — 5 dakikalık BTC-updown marketleri için
//! BUY-only grid + Dutch Book + Scoop + Rebalance + opsiyonel Lottery.
//!
//! Doküman: `docs/bonereaper.md`
//!
//! ## Strateji özeti
//!
//! Her **2 saniyede** (Bölüm 1) bir kez decision loop çalışır. Sadece BUY,
//! çıkış yok — kazanan taraf kapanışta REDEEM ile $1.00/share alır.
//!
//! Karar öncelik sırası (Bölüm 14):
//! 1. POST-MARKET — bekle ya da iptal et
//! 2. INIT       — ilk OB tick'inde yön belirle, opener + opening grid
//! 3. LOTTERY    — (opt-in) kapanışa ≤15s, ask ≤ $0.02 → 10 000sh
//! 4. SCOOP      — kapanışa ≤100s, karşı ask ≤ scoop_thr → tiered lot
//! 5. DUTCH BOOK — up_ask + dn_ask < $1.00 → her iki tarafa 40-45sh
//! 6. REBALANCE  — |imbalance| ≥ 50sh → açık tarafa lot
//! 7. BUILD/WAIT — dominant tarafı büyüt veya avg-down, yoksa stale iptal
//!
//! ## Reason etiketleri
//!
//! `bonereaper:opener:{up,down}` — INIT açılış emri (dominant taraf)
//! `bonereaper:grid:{up,down}`   — INIT opening grid (her iki taraf)
//! `bonereaper:lottery:{up,down}` — lottery tail emri
//! `bonereaper:scoop:{up,down}`  — scoop emri
//! `bonereaper:dutch:{up,down}`  — Dutch Book arbitraj
//! `bonereaper:rebalance:{up,down}` — rebalance fill
//! `bonereaper:build:{up,down}`  — build / avg-down

use serde::{Deserialize, Serialize};

use super::common::{Decision, OpenOrder, PlannedOrder, StrategyContext};
use crate::types::{OrderType, Outcome, Side};

// ─────────────────────────────────────────────
// Sabit parametreler (Bölüm 0)
// ─────────────────────────────────────────────

const TICK_INTERVAL_SECS: u64 = 2;
const BASE_LOT: f64 = 40.0;
const LOT_MAX: f64 = 45.0;
const REBALANCE_MIN: f64 = 50.0;
const SCOOP_WINDOW: f64 = 100.0;
const LOTTERY_THRESHOLD: f64 = 0.02;
const LOTTERY_WINDOW: f64 = 15.0;
const LOTTERY_LOT: f64 = 10_000.0;
const POST_MARKET_WAIT: f64 = 30.0;
/// Stale emirlerin max fiyat sapması (bid'den uzaklık). Üstündeyse iptal.
const STALE_SPREAD_MAX: f64 = 0.05;
/// Avg-down tetik: dominant fiyat bu kadar avg'nin altına düşerse al.
const AVG_DOWN_DELTA: f64 = 0.02;

// ─────────────────────────────────────────────
// FSM State
// ─────────────────────────────────────────────

/// Bonereaper FSM durumu.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BonereaperState {
    /// OB henüz hazır değil; ilk tick beklenıyor.
    Idle,
    /// Market aktif — yön kararı verilmiş, loop çalışıyor.
    Active(Box<BonereaperActive>),
    /// Market kapandı ve POST_MARKET_WAIT aşıldı.
    Done,
}

impl Default for BonereaperState {
    fn default() -> Self {
        Self::Idle
    }
}

/// Aktif durum — `Box` ile heap'te; her tick clone ediliyor.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BonereaperActive {
    /// t=0'da belirlenen dominant yön; değişmez.
    pub direction: Outcome,
    /// Opening grid (her iki tarafa) zaten gönderildi mi?
    pub opening_grid_placed: bool,
    /// Son işlem yapılan çift saniye (tekrar işlemi önler).
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

                // Post-market temizlik
                if to_end < -POST_MARKET_WAIT {
                    return (BonereaperState::Done, Decision::NoOp);
                }

                // İlk tick — yön belirle, opener ve opening grid
                let direction = decide_direction(ctx);
                let (init_state, decision) = Self::place_init(ctx, direction);
                (BonereaperState::Active(Box::new(init_state)), decision)
            }

            BonereaperState::Active(mut st) => {
                // ── POST-MARKET ──────────────────────────────────────────────
                if to_end < -POST_MARKET_WAIT {
                    let cancels = cancel_all(ctx);
                    return (BonereaperState::Done, cancels);
                }
                if to_end < 0.0 {
                    // Kapandı ama POST_MARKET_WAIT dolmadı — yeni emir yok
                    return (BonereaperState::Active(st), Decision::NoOp);
                }

                // ── 2-SANİYE GATE ───────────────────────────────────────────
                // Sadece çift saniyede ve aynı çift saniyeyi tekrar işlemeden
                if rel_secs % TICK_INTERVAL_SECS != 0 {
                    return (BonereaperState::Active(st), Decision::NoOp);
                }
                if rel_secs == st.last_acted_even_sec {
                    return (BonereaperState::Active(st), Decision::NoOp);
                }
                st.last_acted_even_sec = rel_secs;

                // OB kontrolü
                if ctx.up_best_bid == 0.0 || ctx.down_best_bid == 0.0 {
                    return (BonereaperState::Active(st), Decision::NoOp);
                }

                let p = Params::from_ctx(ctx);
                let m = ctx.metrics;

                // ── LOTTERY TAIL (önce — kapanışa ≤15s) ────────────────────
                if p.lottery_enabled && to_end <= LOTTERY_WINDOW {
                    if let Some(order) = check_lottery(ctx) {
                        return (BonereaperState::Active(st), Decision::PlaceOrders(vec![order]));
                    }
                }

                // ── SCOOP (kapanışa ≤100s) ───────────────────────────────────
                if to_end <= SCOOP_WINDOW {
                    if let Some(order) = check_scoop(ctx, &p) {
                        return (BonereaperState::Active(st), Decision::PlaceOrders(vec![order]));
                    }
                }

                // ── DUTCH BOOK ───────────────────────────────────────────────
                if let Some(orders) = check_dutch_book(ctx) {
                    return (BonereaperState::Active(st), Decision::PlaceOrders(orders));
                }

                // ── REBALANCE ────────────────────────────────────────────────
                let imbalance = m.up_filled - m.down_filled;
                if imbalance.abs() >= REBALANCE_MIN {
                    let deficit_side = if imbalance > 0.0 { Outcome::Down } else { Outcome::Up };
                    let lot = rebalance_lot(imbalance);
                    let price = ctx.best_bid(deficit_side);
                    if let Some(order) = make_buy(ctx, deficit_side, price, lot,
                        &format!("bonereaper:rebalance:{}", deficit_side.as_lowercase())) {
                        return (BonereaperState::Active(st), Decision::PlaceOrders(vec![order]));
                    }
                }

                // ── BUILD / AVG-DOWN / WAIT ──────────────────────────────────
                let dom = st.direction;
                let dom_bid = ctx.best_bid(dom);
                let avg_cost = match dom {
                    Outcome::Up => m.avg_up,
                    Outcome::Down => m.avg_down,
                };

                let should_build = if avg_cost == 0.0 {
                    // Henüz fill yok — opener bekleniyor, yeni emir verme
                    false
                } else if dom_bid >= avg_cost {
                    true
                } else if dom_bid < avg_cost - AVG_DOWN_DELTA {
                    true
                } else {
                    false
                };

                if should_build {
                    let lot = init_lot(ctx.now_ms);
                    let reason = format!("bonereaper:build:{}", dom.as_lowercase());
                    if let Some(order) = make_buy(ctx, dom, dom_bid, lot, &reason) {
                        return (BonereaperState::Active(st), Decision::PlaceOrders(vec![order]));
                    }
                }

                // ── WAIT: stale emir iptali ──────────────────────────────────
                let stale = cancel_stale(ctx);
                (BonereaperState::Active(st), stale)
            }
        }
    }

    /// INIT anında opener + opening grid emirlerini yerleştir.
    fn place_init(ctx: &StrategyContext<'_>, direction: Outcome) -> (BonereaperActive, Decision) {
        let rel_secs = (ctx.now_ms / 1000).saturating_sub(ctx.start_ts);
        let state = BonereaperActive {
            direction,
            opening_grid_placed: true,
            last_acted_even_sec: rel_secs,
        };

        let lot = init_lot(ctx.now_ms);
        let mut orders: Vec<PlannedOrder> = Vec::new();

        // Opener: dominant tarafa bid'den emir (resting bid)
        let opener_price = ctx.best_bid(direction);
        let opener_reason = format!("bonereaper:opener:{}", direction.as_lowercase());
        if let Some(o) = make_buy(ctx, direction, opener_price, lot, &opener_reason) {
            orders.push(o);
        }

        // Opening grid: her iki tarafa mevcut ask'tan emir (Dutch Book tetikleyici)
        for &side in &[Outcome::Up, Outcome::Down] {
            let price = ctx.best_ask(side);
            let reason = format!("bonereaper:grid:{}", side.as_lowercase());
            if let Some(o) = make_buy(ctx, side, price, lot, &reason) {
                orders.push(o);
            }
        }

        let decision = if orders.is_empty() {
            Decision::NoOp
        } else {
            Decision::PlaceOrders(orders)
        };

        (state, decision)
    }
}

// ─────────────────────────────────────────────
// Yön kararı (Bölüm 4)
// ─────────────────────────────────────────────

/// BSI primer (|bsi| ≥ eşik → BSI yönü), aksi halde bid karşılaştırması.
fn decide_direction(ctx: &StrategyContext<'_>) -> Outcome {
    let thr = ctx.strategy_params.bonereaper_bsi_threshold();
    if let Some(bsi) = ctx.bsi {
        if bsi.abs() >= thr {
            return if bsi > 0.0 { Outcome::Up } else { Outcome::Down };
        }
    }
    // Bid fallback
    if ctx.up_best_bid >= ctx.down_best_bid {
        Outcome::Up
    } else {
        Outcome::Down
    }
}

// ─────────────────────────────────────────────
// Lot sizing (Bölüm 5)
// ─────────────────────────────────────────────

/// INIT / BUILD / Dutch Book lot: 40-45 arası (now_ms'nin son hanesi ile varyasyon).
#[inline]
fn init_lot(now_ms: u64) -> f64 {
    (BASE_LOT + (now_ms % 6) as f64).min(LOT_MAX)
}

/// Rebalance lot: `max(40, |imbalance| / 4)` (Bölüm 5).
#[inline]
fn rebalance_lot(imbalance: f64) -> f64 {
    (imbalance.abs() / 4.0).max(BASE_LOT)
}

/// Scoop lot: tiered formül (Bölüm 5). `opp_ask` = karşı tarafın mevcut ask'ı.
fn scoop_lot(opp_ask: f64) -> f64 {
    if opp_ask <= 0.01 {
        // Bütçe limiti: en fazla 10_000 share, min 500
        (10_000.0_f64).min(500.0_f64.max(1.0 / opp_ask * 10.0))
    } else if opp_ask <= 0.10 {
        // 500 – 1000 arası lineer
        500.0 + (1000.0 - 500.0) * (0.10 - opp_ask) / 0.09
    } else if opp_ask <= 0.25 {
        // 50 – 150 arası lineer
        50.0 + (150.0 - 50.0) * (0.25 - opp_ask) / 0.15
    } else {
        BASE_LOT
    }
}

// ─────────────────────────────────────────────
// Phase checker'ları
// ─────────────────────────────────────────────

/// Lottery tail kontrolü (Bölüm 8). Herhangi bir tarafın ask ≤ $0.02 ise LOTTERY_LOT.
fn check_lottery(ctx: &StrategyContext<'_>) -> Option<PlannedOrder> {
    for &side in &[Outcome::Up, Outcome::Down] {
        let ask = ctx.best_ask(side);
        if ask <= LOTTERY_THRESHOLD && ask > 0.0 {
            let reason = format!("bonereaper:lottery:{}", side.as_lowercase());
            return make_buy(ctx, side, ask, LOTTERY_LOT, &reason);
        }
    }
    None
}

/// Scoop kontrolü (Bölüm 7). Dominant taraf yüksekteyken karşı taraf ≤ scoop_thr.
fn check_scoop(ctx: &StrategyContext<'_>, p: &Params) -> Option<PlannedOrder> {
    for &other in &[Outcome::Up, Outcome::Down] {
        let other_ask = ctx.best_ask(other);
        if other_ask <= p.scoop_threshold && other_ask > 0.0 {
            // Dominant taraftan alım: scoop edilen taraf değil, dominant taraf
            // Doc Bölüm 7: `for side in ["UP","DOWN"]: other_side = karşı; 
            // if ob[other_side].ask <= threshold → place on side`
            // Yani dominant taraftan scoop emri verilir (bu tarafı büyüt).
            // Ama aslında dok'ta scoop buy yapılan taraf dominant taraf.
            let lot = scoop_lot(other_ask);
            // Scoop emrini other'ın karşısına değil, dominant'a veriyoruz
            // Doc: "return side, ob[side].ask, lot" — side = dominant
            let dom_side = other.opposite();
            let dom_ask = ctx.best_ask(dom_side);
            let reason = format!("bonereaper:scoop:{}", dom_side.as_lowercase());
            return make_buy(ctx, dom_side, dom_ask, lot, &reason);
        }
    }
    None
}

/// Dutch Book kontrolü (Bölüm 9). up_ask + dn_ask < $1.00 → her ikisine emir.
fn check_dutch_book(ctx: &StrategyContext<'_>) -> Option<Vec<PlannedOrder>> {
    let up_ask = ctx.up_best_ask;
    let dn_ask = ctx.down_best_ask;
    if up_ask + dn_ask >= 1.0 || up_ask <= 0.0 || dn_ask <= 0.0 {
        return None;
    }
    let lot = init_lot(ctx.now_ms);
    let mut orders = Vec::with_capacity(2);
    if let Some(o) = make_buy(ctx, Outcome::Up, up_ask, lot, "bonereaper:dutch:up") {
        orders.push(o);
    }
    if let Some(o) = make_buy(ctx, Outcome::Down, dn_ask, lot, "bonereaper:dutch:down") {
        orders.push(o);
    }
    if orders.is_empty() { None } else { Some(orders) }
}

// ─────────────────────────────────────────────
// Emir yardımcıları
// ─────────────────────────────────────────────

/// BUY GTC limit emir. `price ≤ 0` veya `size × price < api_min_order_size` → `None`.
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

/// `bonereaper:` prefix'li tüm açık BUY emirlerini iptal eder.
fn cancel_all(ctx: &StrategyContext<'_>) -> Decision {
    let ids: Vec<String> = ctx
        .open_orders
        .iter()
        .filter(|o| o.reason.starts_with("bonereaper:") && o.side == Side::Buy)
        .map(|o| o.id.clone())
        .collect();
    if ids.is_empty() { Decision::NoOp } else { Decision::CancelOrders(ids) }
}

/// WAIT phase: fiyatı current bid'den `STALE_SPREAD_MAX`'tan fazla sapan
/// `bonereaper:` emirleri iptal et.
fn cancel_stale(ctx: &StrategyContext<'_>) -> Decision {
    let ids: Vec<String> = ctx
        .open_orders
        .iter()
        .filter(|o| {
            if !o.reason.starts_with("bonereaper:") || o.side != Side::Buy {
                return false;
            }
            let cur_bid = ctx.best_bid(o.outcome);
            if cur_bid <= 0.0 {
                return false;
            }
            (o.price - cur_bid).abs() > STALE_SPREAD_MAX
        })
        .map(|o| o.id.clone())
        .collect();
    if ids.is_empty() { Decision::NoOp } else { Decision::CancelOrders(ids) }
}

// ─────────────────────────────────────────────
// Config wrapper
// ─────────────────────────────────────────────

struct Params {
    scoop_threshold: f64,
    lottery_enabled: bool,
}

impl Params {
    fn from_ctx(ctx: &StrategyContext<'_>) -> Self {
        let sp = ctx.strategy_params;
        Self {
            scoop_threshold: sp.bonereaper_scoop_threshold(),
            lottery_enabled: sp.bonereaper_lottery_enabled(),
        }
    }
}

// ─────────────────────────────────────────────
// OpenOrder yardımcısı (derleyici uyarısı önlemi)
// ─────────────────────────────────────────────

/// Kullanılmayan import uyarısını engelle.
#[allow(dead_code)]
fn _uses_open_order(_: &OpenOrder) {}
