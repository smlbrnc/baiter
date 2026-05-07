//! Bonereaper stratejisi — sinyal tabanlı 1 saniyelik emir döngüsü.
//!
//! ## Çalışma mantığı
//!
//! Her **1 saniyede** bir karar döngüsü çalışır. Karar öncelik sırası:
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

const TICK_INTERVAL_SECS: u64 = 1;
/// Stale emir maksimum fiyat sapması (bid'den uzaklık).
const STALE_SPREAD_MAX: f64 = 0.05;
/// Aynı yönde ardışık signal trade'ler arası minimum bekleme (ms).
/// Yön değişiminde bu cooldown atlanır (anlık reaksiyon için).
/// 3sn → ~100 trade/5dk markette (real bot medyan 73 trade/market ile uyumlu).
const SAME_DIR_COOLDOWN_MS: u64 = 3000;
/// Sinyal kuvveti minimum eşiği (|signal_ema|).
/// V3 stilinde 0.25'e yükseltildi (akademik araştırma: raised threshold = noise filter).
/// Simülasyon (Bot 61 + Bot 63): WR %3-6 puan artış, ROI +%0.64 puan iyileşme.
const SIGNAL_STRENGTH_MIN: f64 = 0.25;
/// Dynamic size multiplier eşikleri — signal kuvvetine göre 1x/2x/3x size.
/// Real bot pattern: medyan $12 trade ama p90 $48 (büyük volume güçlü sinyallerde).
const DYNAMIC_SIZE_STRONG: f64 = 0.7;  // > 0.7 → 3x
const DYNAMIC_SIZE_MEDIUM: f64 = 0.5;  // > 0.5 → 2x

/// V3 Triple Gate eşikleri — 3 farklı sinyal aynı yönde olmalı:
/// - composite (Binance/OKX) > UP_THR veya < DOWN_THR
/// - market_skor (UP_bid) UP > 0.55, DOWN < 0.45
/// - multi-tf slope > +0.20 veya < -0.20
const TRIPLE_GATE_COMPOSITE_UP: f64 = 5.5;
const TRIPLE_GATE_COMPOSITE_DOWN: f64 = 4.5;
const TRIPLE_GATE_BID_UP: f64 = 0.55;
const TRIPLE_GATE_BID_DOWN: f64 = 0.45;
const TRIPLE_GATE_SLOPE_THRESHOLD: f64 = 0.20;
/// Multi-tf momentum lookback'leri (saniye) ve ağırlıkları.
/// Akademik v3 stili: long lookback dominant, kısa horizonda mean-reversion riski azalır.
const MULTI_TF_LOOKBACKS: [usize; 4] = [30, 60, 120, 240];
const MULTI_TF_WEIGHTS: [f64; 4] = [0.10, 0.20, 0.30, 0.40];
/// Counter-trend signal block — son 60 saniye composite ortalaması ile signal yönü
/// karşılaştırılır; ters yön + zayıf sinyal (|smoothed| < 0.40) ise BLOK.
const TREND_FILTER_LOOKBACK: usize = 60;
const TREND_FILTER_NEUTRAL: f64 = 5.0;
const TREND_FILTER_OVERRIDE: f64 = 0.40;
/// Composite history maksimum boyut (FIFO buffer).
const COMPOSITE_HISTORY_MAX: usize = 240;

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
    /// Son signal emri verilen ms (aynı yön cooldown gate için).
    /// 0 = henüz emir verilmedi (cooldown bypass).
    #[serde(default)]
    pub last_signal_trade_ms: u64,
    /// Multi-timeframe momentum hesabı için son 240 saniyenin composite (effective_score)
    /// değerleri. FIFO buffer; max 240 entry. V3 stili linear regression slope hesabı için.
    #[serde(default)]
    pub composite_history: Vec<f64>,
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
                    last_signal_trade_ms: 0,
                    composite_history: Vec::with_capacity(240),
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
                        < ctx.strategy_params.bonereaper_profit_lock_imbalance()
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
                let new_dir = signal_direction_persistent(&mut st, ctx, persistence_k);

                // ── SIGNAL STRENGTH FILTER ───────────────────────────────────
                // Sinyal kuvveti (|signal_ema|) eşik altında ise market "kararsız".
                let smoothed = st.signal_ema.unwrap_or(0.0);
                let smoothed_abs = smoothed.abs();
                if smoothed_abs < SIGNAL_STRENGTH_MIN {
                    let stale = cancel_stale(ctx);
                    return (BonereaperState::Active(st), stale);
                }

                // ── V3 TRIPLE GATE ───────────────────────────────────────────
                // 3 sinyal aynı yönde olmalı:
                //   - composite (Binance/OKX) > 5.5 (UP) veya < 4.5 (DOWN)
                //   - market_skor (UP_bid) > 0.55 (UP) veya < 0.45 (DOWN)
                //   - multi-tf slope > +0.20 (UP) veya < -0.20 (DOWN)
                // Akademik araştırma kanıtı (Liu Mar 2026): WR +%5, ROI +%0.6.
                let comp_dir: i8 = if ctx.effective_score > TRIPLE_GATE_COMPOSITE_UP {
                    1
                } else if ctx.effective_score < TRIPLE_GATE_COMPOSITE_DOWN {
                    -1
                } else {
                    0
                };
                let mkt_dir: i8 = if ctx.up_best_bid > TRIPLE_GATE_BID_UP {
                    1
                } else if ctx.up_best_bid > 0.0 && ctx.up_best_bid < TRIPLE_GATE_BID_DOWN {
                    -1
                } else {
                    0
                };
                let slope_dir: i8 = if smoothed > TRIPLE_GATE_SLOPE_THRESHOLD {
                    1
                } else if smoothed < -TRIPLE_GATE_SLOPE_THRESHOLD {
                    -1
                } else {
                    0
                };
                if comp_dir == 0 || mkt_dir == 0 || slope_dir == 0
                    || comp_dir != mkt_dir
                    || mkt_dir != slope_dir
                {
                    let stale = cancel_stale(ctx);
                    return (BonereaperState::Active(st), stale);
                }

                // ── V3 TREND FILTER ──────────────────────────────────────────
                // Counter-trend zayıf sinyal: BLOK. Son 60sn composite ortalaması ile
                // mevcut sinyal yönü ters ise ve smoothed < 0.40 ise trade alma.
                if st.composite_history.len() >= TREND_FILTER_LOOKBACK {
                    let n = st.composite_history.len();
                    let trend_avg: f64 = st.composite_history[n - TREND_FILTER_LOOKBACK..]
                        .iter()
                        .sum::<f64>()
                        / TREND_FILTER_LOOKBACK as f64;
                    let trend_dir: i8 = if trend_avg > TREND_FILTER_NEUTRAL { 1 } else { -1 };
                    let signal_dir_now: i8 = if smoothed > 0.0 { 1 } else { -1 };
                    if trend_dir != signal_dir_now && smoothed_abs < TREND_FILTER_OVERRIDE {
                        let stale = cancel_stale(ctx);
                        return (BonereaperState::Active(st), stale);
                    }
                }

                // Yön değiştiyse eski signal emirlerini iptal et.
                let prev_dir = st.last_signal_dir;
                st.last_signal_dir = Some(new_dir);

                if prev_dir == Some(new_dir.opposite()) {
                    // Eski yöndeki signal emirlerini iptal + yeni emir tek adımda.
                    // Yön değişimi cooldown'a tabi DEĞİL — anlık reaksiyon için.
                    let mut cancel_ids: Vec<String> = Vec::with_capacity(ctx.open_orders.len());
                    for o in ctx.open_orders.iter() {
                        if o.reason.starts_with("bonereaper:signal:")
                            && o.outcome == new_dir.opposite()
                        {
                            cancel_ids.push(o.id.clone());
                        }
                    }

                    if let Some(order) = signal_order(&st, ctx, new_dir) {
                        st.last_signal_trade_ms = ctx.now_ms;
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

                // Aynı yön cooldown: son signal trade'den `SAME_DIR_COOLDOWN_MS` geçmediyse
                // yeni emir vermeyip yalnızca stale cancel kontrolü yap. Yön değişiminde
                // cooldown atlanır (üstteki dal). Real bot 16 trade/dakika pattern uyumu.
                if st.last_signal_trade_ms > 0
                    && ctx.now_ms.saturating_sub(st.last_signal_trade_ms) < SAME_DIR_COOLDOWN_MS
                {
                    let stale = cancel_stale(ctx);
                    return (BonereaperState::Active(st), stale);
                }

                // Aynı yön: mevcut signal emirlerini iptal et (fiyat tazeleme) + yenisini koy.
                let mut stale_signal_ids: Vec<String> = Vec::with_capacity(ctx.open_orders.len());
                for o in ctx.open_orders.iter() {
                    if o.reason.starts_with("bonereaper:signal:") {
                        stale_signal_ids.push(o.id.clone());
                    }
                }

                if let Some(order) = signal_order(&st, ctx, new_dir) {
                    st.last_signal_trade_ms = ctx.now_ms;
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

/// V3 Triple Gate sinyal yön kararı.
///
/// Akademik araştırma temelli (Liu, Mar 2026 — Polymarket 5min BTC analizi).
///
/// 1. **Composite history kayıt**: `effective_score` son 240 sn FIFO buffer'a yazılır.
///
/// 2. **Multi-timeframe momentum**: 30s/60s/120s/240s linear regression slope'ları
///    [0.10, 0.20, 0.30, 0.40] ağırlıklarla toplanır → `momentum_signal ∈ [-1, +1]`.
///    Long lookback dominant (kısa horizon mean-reversion riski azalır).
///
/// 3. **Hibrit smoothed**: `momentum × 0.5 + market_skor × 0.5`
///    market_skor = Polymarket UP_bid trendi.
///
/// 4. **EMA smoothing** + **K-tick persistence**: Mevcut sistem korunur (alpha, K config).
///
/// **Triple Gate** ve **trend filter** `decide` içinde uygulanır.
fn signal_direction_persistent(
    st: &mut BonereaperActive,
    ctx: &StrategyContext<'_>,
    k: u32,
) -> Outcome {
    // 1. Composite history güncelle (FIFO, max 240 entry).
    st.composite_history.push(ctx.effective_score);
    if st.composite_history.len() > COMPOSITE_HISTORY_MAX {
        st.composite_history.remove(0);
    }

    // 2. Multi-timeframe momentum (akademik v3 stili).
    let momentum_signal = multi_tf_momentum(&st.composite_history);

    // 3. Hibrit: momentum + market_skor (UP_bid trendi).
    let market_skor = if ctx.up_best_bid <= 0.0 || ctx.down_best_bid <= 0.0 {
        0.0
    } else {
        ((ctx.up_best_bid - 0.5) * 2.0).clamp(-1.0, 1.0)
    };
    let hybrid_skor = momentum_signal * 0.5 + market_skor * 0.5;

    // EMA smoothing (config alpha — default 1.0 = anlık).
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


/// Sinyal yönünde emir — dinamik size + asimetrik avg_sum filtresi.
///
/// **Fiyat:** `signal_taker=true` → `best_ask` (taker, anında fill).
///             `signal_taker=false` → `best_bid` (maker, GTC limit).
///
/// **Boyut (dinamik 1x/2x/3x):** Signal kuvvetine göre çarpan:
///   - `|signal_ema| > 0.7` → 3x (çok güçlü sinyal)
///   - `|signal_ema| > 0.5` → 2x (güçlü sinyal)
///   - aksi → 1x (default)
///
/// `size = ceil(order_usdc * multiplier / price)`. order_usdc default 10
/// → dinamik aralık $10-$30. Real bot medyan $12, p90 $48 ile uyumlu.
///
/// **avg_sum filtresi (yalnız pahalı taraf, bid > 0.50):**
///   Karşı tarafta zaten pozisyon varsa (`opp_filled > 0`) ve mevcut yönde de
///   pozisyon varsa (`cur_filled > 0`), yeni alımın etkisiyle `new_avg + opp_avg ≥ 1.25`
///   olacaksa emir verilmez. Real bot p90 ~1.20'ye yakın eşik.
fn signal_order(
    st: &BonereaperActive,
    ctx: &StrategyContext<'_>,
    dir: Outcome,
) -> Option<PlannedOrder> {
    let bid = ctx.best_bid(dir);
    if bid <= 0.0 {
        return None;
    }
    let price = if ctx.strategy_params.bonereaper_signal_taker() {
        ctx.best_ask(dir)
    } else {
        bid
    };
    if price <= 0.0 {
        return None;
    }
    let s = st.signal_ema.unwrap_or(0.0).abs();
    let multiplier = if s > DYNAMIC_SIZE_STRONG {
        3.0
    } else if s > DYNAMIC_SIZE_MEDIUM {
        2.0
    } else {
        1.0
    };
    let usdc = ctx.order_usdc * multiplier;
    let size = (usdc / price).ceil();
    if bid > 0.50 {
        let m = ctx.metrics;
        let (cur_filled, cur_avg, opp_filled, opp_avg) = match dir {
            Outcome::Up   => (m.up_filled,   m.avg_up,   m.down_filled, m.avg_down),
            Outcome::Down => (m.down_filled, m.avg_down, m.up_filled,   m.avg_up),
        };
        if opp_filled > 0.0 && cur_filled > 0.0 {
            let new_avg = (cur_avg * cur_filled + price * size) / (cur_filled + size);
            if new_avg + opp_avg >= 1.25 {
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

// ─────────────────────────────────────────────
// V3 Triple Gate Helpers — Multi-timeframe momentum
// ─────────────────────────────────────────────

/// Linear regression slope hesabı (least squares).
/// Sıralı değerler için en iyi fit doğrunun eğimini döndürür.
fn linear_regression_slope(values: &[f64]) -> f64 {
    let n = values.len();
    if n < 2 {
        return 0.0;
    }
    let n_f = n as f64;
    let x_mean = (n_f - 1.0) / 2.0;
    let y_mean: f64 = values.iter().sum::<f64>() / n_f;
    let mut num = 0.0;
    let mut denom = 0.0;
    for (i, v) in values.iter().enumerate() {
        let dx = i as f64 - x_mean;
        num += dx * (v - y_mean);
        denom += dx * dx;
    }
    if denom == 0.0 { 0.0 } else { num / denom }
}

/// Multi-timeframe momentum sinyali — composite history üzerinde 30s/60s/120s/240s
/// linear regression slope'larının ağırlıklı toplamı. v3 stilinde long lookback dominant.
/// Sonuç clamp[-1, +1].
fn multi_tf_momentum(history: &[f64]) -> f64 {
    let mut signal = 0.0;
    for (tf, w) in MULTI_TF_LOOKBACKS.iter().zip(MULTI_TF_WEIGHTS.iter()) {
        let len = history.len();
        let slope = if len >= *tf {
            let recent = &history[len - tf..];
            (linear_regression_slope(recent) * 50.0).clamp(-1.0, 1.0)
        } else {
            0.0
        };
        signal += slope * w;
    }
    signal.clamp(-1.0, 1.0)
}

/// Derleyici uyarısını bastır.
#[allow(dead_code)]
fn _uses_open_order(_: &OpenOrder) {}
