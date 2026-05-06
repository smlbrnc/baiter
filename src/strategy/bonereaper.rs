//! Bonereaper stratejisi — sinyal tabanlı 2 saniyelik emir döngüsü.
//!
//! ## Çalışma mantığı
//!
//! Her **2 saniyede** bir karar döngüsü çalışır. Karar öncelik sırası:
//!
//! 1. DUTCH BOOK   — up_ask + dn_ask < $1.00 → her iki tarafa arbitraj emri.
//! 2. SIGNAL       — skor → UP veya DOWN, yön değiştiyse önceki signal emirleri
//!                   iptal edilir; yeni yönde `best_bid`'den GTC maker emir verilir.
//! 3. STALE CANCEL — fiyatı current bid'den STALE_SPREAD_MAX'tan fazla sapan
//!                   açık signal emirleri iptal edilir.
//!
//! ## Reason etiketleri
//!
//! `bonereaper:signal:{up,down}` — sinyal yönlü opener (her döngü)
//! `bonereaper:dutch:{up,down}`  — Dutch Book arbitraj

use serde::{Deserialize, Serialize};

use super::common::{Decision, OpenOrder, PlannedOrder, StrategyContext};
use crate::types::{OrderType, Outcome, Side};

// ─────────────────────────────────────────────
// Sabitler
// ─────────────────────────────────────────────

const TICK_INTERVAL_SECS: u64 = 2;
/// Minimum lot: her rebalance tick'inde en az bu kadar al.
/// Stale emir maksimum fiyat sapması (bid'den uzaklık).
const STALE_SPREAD_MAX: f64 = 0.05;
/// İmbalance kapısı: toplam pozisyonun bu oranını aşan imbalance durumunda
/// sinyal, imbalance'ı azaltacak yönde ateşlenir (ters sinyal).
/// Gerçek bot CHE/DOM ≈ 0.91x — %50 sınırı makul bir denge noktası.
const IMBALANCE_CAP: f64 = 0.50;

// Reason etiketleri — `format!()` allocation'larını eler (hot path'te tick başına 1 alloc tasarrufu).
#[inline]
const fn reason_signal(dir: Outcome) -> &'static str {
    match dir {
        Outcome::Up => "bonereaper:signal:up",
        Outcome::Down => "bonereaper:signal:down",
    }
}



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
    /// Persistence K-tick onayı için kullanılan onaylanmış (confirmed) yön.
    /// `None` → henüz hiç sinyal görülmedi.
    #[serde(default)]
    pub confirmed_signal: Option<Outcome>,
    /// Persistence için bekleyen aday yön (confirmed'in tersi).
    #[serde(default)]
    pub pending_signal: Option<Outcome>,
    /// Bekleyen aday için ardışık tick sayısı.
    #[serde(default)]
    pub pending_count: u32,
    /// Hibrit composite skoru için EMA değeri (skor[-1,+1] cinsinden).
    /// `None` → henüz initialize edilmemiş; ilk değeri ham hibrit alır.
    #[serde(default)]
    pub signal_ema: Option<f64>,
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
                // Active'e geç
                let active = BonereaperActive {
                    last_signal_dir: None,
                    last_acted_even_sec: u64::MAX,
                    confirmed_signal: None,
                    pending_signal: None,
                    pending_count: 0,
                    signal_ema: None,
                };
                (BonereaperState::Active(Box::new(active)), Decision::NoOp)
            }

            BonereaperState::Active(mut st) => {
                // Pazar kapandıktan sonra yeni emir verilmez; max/min_price filtreleri
                // aktif olduğu süre boyunca emir döngüsü çalışmaya devam eder.
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

                // ── PROFIT LOCK ───────────────────────────────────────────────
                // Her iki tarafta fill var ve imbalance trigger altında ise
                // mevcut pozisyonu koru, yeni emir verme.
                if ctx.strategy_params.bonereaper_profit_lock()
                    && m.up_filled > 0.0
                    && m.down_filled > 0.0
                    && (m.up_filled - m.down_filled).abs()
                        < ctx.strategy_params.bonereaper_rebalance_trigger()
                {
                    return (BonereaperState::Active(st), Decision::NoOp);
                }

                // ── DUTCH BOOK ───────────────────────────────────────────────
                if let Some(orders) = check_dutch_book(ctx) {
                    return (BonereaperState::Active(st), Decision::PlaceOrders(orders));
                }

                // ── SIGNAL ───────────────────────────────────────────────────
                // signal_ready değilse (warmup tamamlanmadı) emir verilmez.
                if !ctx.signal_ready {
                    return (BonereaperState::Active(st), Decision::NoOp);
                }

                let persistence_k = ctx.strategy_params.bonereaper_signal_persistence_k();
                let ema_dir = signal_direction_persistent(&mut st, ctx, persistence_k);

                // ── İMBALANCE KAPISI ─────────────────────────────────────────
                // İmbalance > %50 ise sinyal yerine dengeleyici yönde ateşle.
                // Bu, aşırı tek yönlü birikimi önler (gerçek bot CHE/DOM ≈ 1x).
                let total_sh = m.up_filled + m.down_filled;
                let new_dir = if total_sh > 0.0 {
                    let imb_ratio = (m.up_filled - m.down_filled).abs() / total_sh;
                    if imb_ratio > IMBALANCE_CAP {
                        // Fazlalık hangi tarafta? Karşı tarafa yönlendir.
                        if m.up_filled > m.down_filled { Outcome::Down } else { Outcome::Up }
                    } else {
                        ema_dir
                    }
                } else {
                    ema_dir
                };

                // Yön değiştiyse eski signal emirlerini iptal et.
                let prev_dir = st.last_signal_dir;
                st.last_signal_dir = Some(new_dir);

                if prev_dir == Some(new_dir.opposite()) {
                    // Eski yöndeki signal emirlerini iptal + yeni emir tek adımda.
                    let mut cancel_ids: Vec<String> = Vec::with_capacity(ctx.open_orders.len());
                    for o in ctx.open_orders.iter() {
                        if o.reason.starts_with("bonereaper:signal:")
                            && o.outcome == new_dir.opposite()
                        {
                            cancel_ids.push(o.id.clone());
                        }
                    }

                    if let Some(order) = signal_order(&st, ctx, new_dir) {
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
                let mut stale_signal_ids: Vec<String> = Vec::with_capacity(ctx.open_orders.len());
                for o in ctx.open_orders.iter() {
                    if o.reason.starts_with("bonereaper:signal:") {
                        stale_signal_ids.push(o.id.clone());
                    }
                }

                if let Some(order) = signal_order(&st, ctx, new_dir) {
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

/// Hibrit + EMA + persistence-aware sinyal yön kararı.
///
/// 1. Hibrit composite: `signal_skor × (1 - w_market) + market_skor × w_market`
///    - signal_skor: Binance/OKX composite [(effective_score-5)/5] ∈ [-1, +1]
///    - market_skor: Polymarket UP_bid trendi [(up_bid-0.5) × 2] ∈ [-1, +1]
///    82 market tick analizinde Polymarket sinyalı %76 doğru, composite %55.
///
/// 2. EMA smoothing: `ema = α × hybrid + (1-α) × prev_ema`
///    Bimodal score'ları yumuşatır, gürültüyü filtreler.
///
/// 3. Persistence (K-tick onay):
///    - `K=1` → anlık karar (her tick yön değiştirebilir).
///    - `K=2+` → mevcut yön korunur; ters yön için K ardışık tick onayı gerekir.
fn signal_direction_persistent(
    st: &mut BonereaperActive,
    ctx: &StrategyContext<'_>,
    k: u32,
) -> Outcome {
    // Hibrit skor: composite (Binance/OKX) + Polymarket UP_bid trendi.
    let signal_skor = ((ctx.effective_score - 5.0) / 5.0).clamp(-1.0, 1.0);
    // KRİTİK: Book hazır değilse market_skor = 0 (nötr).
    // Aksi halde UP_bid=0 → market_skor=-1 → EMA aşırı DN bias.
    let market_skor = if ctx.up_best_bid <= 0.0 || ctx.down_best_bid <= 0.0 {
        0.0
    } else {
        ((ctx.up_best_bid - 0.5) * 2.0).clamp(-1.0, 1.0)
    };
    let w_market = ctx.strategy_params.bonereaper_signal_w_market();
    let w_signal = 1.0 - w_market;
    let hybrid_skor = signal_skor * w_signal + market_skor * w_market;

    // EMA smoothing. İlk tick'te EMA = ham hibrit (warm-start).
    // market_skor book hazır olmadan 0 döndüğü için ilk tick'te ekstrem değer
    // alma riski ortadan kalkar — bias yok.
    let alpha = ctx.strategy_params.bonereaper_signal_ema_alpha();
    let smoothed = match st.signal_ema {
        Some(prev) => alpha * hybrid_skor + (1.0 - alpha) * prev,
        None => hybrid_skor,
    };
    st.signal_ema = Some(smoothed);

    let raw: Outcome = if smoothed > 0.0 {
        Outcome::Up
    } else {
        Outcome::Down
    };
    // İlk sinyal: doğrudan kabul et.
    let confirmed = match st.confirmed_signal {
        Some(c) => c,
        None => {
            st.confirmed_signal = Some(raw);
            st.pending_signal = None;
            st.pending_count = 0;
            return raw;
        }
    };
    // K=1: persistence yok, anlık karar.
    if k <= 1 {
        st.confirmed_signal = Some(raw);
        st.pending_signal = None;
        st.pending_count = 0;
        return raw;
    }
    // Ham sinyal mevcut yönle aynı: pending sıfırla.
    if raw == confirmed {
        st.pending_signal = None;
        st.pending_count = 0;
        return confirmed;
    }
    // Ham sinyal ters: pending sayacını artır.
    if st.pending_signal == Some(raw) {
        st.pending_count = st.pending_count.saturating_add(1);
    } else {
        st.pending_signal = Some(raw);
        st.pending_count = 1;
    }
    if st.pending_count >= k {
        st.confirmed_signal = Some(raw);
        st.pending_signal = None;
        st.pending_count = 0;
        return raw;
    }
    confirmed
}


/// Sinyal yönünde emir:
///   bid > 0.50 (yükselen / dominant taraf) → `best_ask` taker, live'da anında fill.
///   bid ≤ 0.50 (ucuz / durağan taraf)      → `best_bid` maker, hız kritik değil.
/// Boyut: `order_usdc / price` — notional ≥ min_order_size olacak şekilde ceil kullanılır.
fn signal_order(
    _st: &BonereaperActive,
    ctx: &StrategyContext<'_>,
    dir: Outcome,
) -> Option<PlannedOrder> {
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
    // Pahalı taraf (bid > 0.50): avg_sum < 1.0 kontrolü — sinyal yanlış yönde ise
    // agresif birikim engellenir. Ucuz taraf (bid ≤ 0.50): serbest, avg_sum seyreltir.
    if bid > 0.50 {
        let m = ctx.metrics;
        let (cur_filled, cur_avg, opp_filled, opp_avg) = match dir {
            Outcome::Up   => (m.up_filled,   m.avg_up,   m.down_filled, m.avg_down),
            Outcome::Down => (m.down_filled, m.avg_down, m.up_filled,   m.avg_up),
        };
        if opp_filled > 0.0 {
            let new_avg = (cur_avg * cur_filled + price * size) / (cur_filled + size);
            if new_avg + opp_avg >= 1.05 {
                return None;
            }
        }
    }
    make_buy(ctx, dir, price, size, reason_signal(dir))
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
    if !pair_cost_ok(ctx, Outcome::Up, up_ask) || !pair_cost_ok(ctx, Outcome::Down, dn_ask) {
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

/// Current bid'den `STALE_SPREAD_MAX`'tan fazla sapan signal emirlerini iptal et.
/// Karşı taraf bid ≤ CHEAP_HEDGE_THRESHOLD ise ucuza hedge emri ver.

fn cancel_stale(ctx: &StrategyContext<'_>) -> Decision {
    let mut ids: Vec<String> = Vec::with_capacity(ctx.open_orders.len());
    for o in ctx.open_orders.iter() {
        if !o.reason.starts_with("bonereaper:signal:") || o.side != Side::Buy {
            continue;
        }
        let cur_bid = ctx.best_bid(o.outcome);
        if cur_bid > 0.0 && (o.price - cur_bid).abs() > STALE_SPREAD_MAX {
            ids.push(o.id.clone());
        }
    }
    if ids.is_empty() { Decision::NoOp } else { Decision::CancelOrders(ids) }
}

/// Derleyici uyarısını bastır.
#[allow(dead_code)]
fn _uses_open_order(_: &OpenOrder) {}
