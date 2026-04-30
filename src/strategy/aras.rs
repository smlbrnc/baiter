//! Aras stratejisi — Çift Taraflı Eş Zamanlı Alım Arbitrajı
//!
//! Doküman: `docs/aras.md`
//!
//! ## Strateji özeti
//!
//! Her `poll_secs` saniyede UP **ve** DOWN taraflarına EŞ ZAMANLI GTC emir verilir.
//!
//! - Giriş koşulu: `entry_a + ask_b < 1.00` (pair alımı kârlı olmalı)
//! - Fiyat yükselse de, düşse de alım devam eder — `dca_min_drop` koşulu yok.
//! - **İmbalans koruması**: bir taraf diğerinden > 1 emir (shares) kadar ileride olamaz.
//! - **Bant**: her taraf `BAND_LOW–BAND_HIGH` aralığında olmalı.
//! - **ARB kilidi**: fill sonrası `avg_up + avg_dn < 1.00` → garantili kâr loglanır.
//!
//! ## Reason etiketleri
//!
//! - `"aras:buy:up"` / `"aras:buy:down"` — anlık GTC emirler

use serde::{Deserialize, Serialize};

use super::common::{Decision, OpenOrder, PlannedOrder, StrategyContext};
use crate::types::{OrderType, Outcome, Side};

// ─────────────────────────────────────────────
// Veri yapıları
// ─────────────────────────────────────────────

/// Aras aktif pozisyon durumu.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArasActive {
    // ── Timing ──────────────────────────────────────────────────────────────
    /// Son poll zamanı (ms) — [Outcome::Up as usize, Outcome::Down as usize]
    pub last_poll_ms: [u64; 2],

    // ── Fill tracking ────────────────────────────────────────────────────────
    /// Önceki tick'te görülen `metrics.up_filled`; artış = yeni fill.
    pub seen_up_filled: f64,
    /// Önceki tick'te görülen `metrics.down_filled`.
    pub seen_dn_filled: f64,

    // ── Bekleyen emirler ─────────────────────────────────────────────────────
    /// Şu an UP tarafında bekleyen emir var mı?
    pub pending: [bool; 2],
    /// Verilen emirlerin fiyatı (requote kontrolü için).
    pub pending_price: [f64; 2],
    /// Emir `open_orders`'da en az bir kez görüldü mü? (yanlış-fill önlemi)
    pub confirmed: [bool; 2],

    // ── ARB istatistikleri ────────────────────────────────────────────────────
    /// Kilitlenmiş ARB güncelleme sayısı (loglama).
    pub arb_lock_count: u32,
    /// Mevcut pozisyonun garantili PnL'i ($).
    pub guaranteed_pnl: f64,
}

impl Default for ArasActive {
    fn default() -> Self {
        Self {
            last_poll_ms: [0; 2],
            seen_up_filled: 0.0,
            seen_dn_filled: 0.0,
            pending: [false; 2],
            pending_price: [0.0; 2],
            confirmed: [false; 2],
            arb_lock_count: 0,
            guaranteed_pnl: 0.0,
        }
    }
}

/// Aras FSM durumları.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ArasState {
    /// Kitap henüz hazır değil.
    Idle,
    /// Market aktif — dual-side poll döngüsü çalışıyor.
    Active(Box<ArasActive>),
    /// Piyasa kapandı.
    Done,
}

impl Default for ArasState {
    fn default() -> Self {
        Self::Idle
    }
}

// ─────────────────────────────────────────────
// Yardımcı fonksiyonlar
// ─────────────────────────────────────────────

#[inline]
fn outcome_idx(o: Outcome) -> usize {
    match o {
        Outcome::Up => 0,
        Outcome::Down => 1,
    }
}

/// Anlık orta fiyat.
#[inline]
fn mid(ctx: &StrategyContext<'_>, outcome: Outcome) -> f64 {
    (ctx.best_bid(outcome) + ctx.best_ask(outcome)) / 2.0
}

/// Fiyatı en yakın tick'e yuvarla.
#[inline]
fn round_tick(price: f64, tick: f64) -> f64 {
    (price / tick).round() * tick
}

/// Bant kontrolü: mid, `band_low–band_high` aralığında mı?
#[inline]
fn pass_band(ctx: &StrategyContext<'_>, outcome: Outcome, p: &ArasParams) -> bool {
    let m = mid(ctx, outcome);
    m >= p.band_low && m <= p.band_high
}

/// Emir reason etiketi.
fn buy_reason(outcome: Outcome) -> String {
    format!("aras:buy:{}", outcome.as_lowercase())
}

/// `open_orders`'da bu outcome için Aras emri var mı?
fn has_open_order(open_orders: &[OpenOrder], outcome: Outcome) -> bool {
    let reason = buy_reason(outcome);
    open_orders
        .iter()
        .any(|o| o.outcome == outcome && o.side == Side::Buy && o.reason == reason)
}

// ─────────────────────────────────────────────
// Config wrapper
// ─────────────────────────────────────────────

struct ArasParams {
    poll_ms: u64,
    shares: f64,
    max_usd_per_side: f64,
    band_low: f64,
    band_high: f64,
    tick: f64,
}

impl ArasParams {
    fn from_ctx(ctx: &StrategyContext<'_>) -> Self {
        let sp = ctx.strategy_params;
        Self {
            poll_ms: (sp.aras_poll_secs() * 1000.0) as u64,
            shares: sp.aras_shares_per_order(),
            max_usd_per_side: sp.aras_max_usd_per_side(),
            band_low: sp.aras_band_low(),
            band_high: sp.aras_band_high(),
            tick: ctx.tick_size.max(0.001),
        }
    }
}

// ─────────────────────────────────────────────
// Karar motoru
// ─────────────────────────────────────────────

pub struct ArasEngine;

impl ArasEngine {
    pub fn decide(state: ArasState, ctx: &StrategyContext<'_>) -> (ArasState, Decision) {
        if let Some(rem) = ctx.market_remaining_secs {
            if rem <= 0.0 {
                return (ArasState::Done, Decision::NoOp);
            }
        }

        let book_ready = ctx.up_best_bid > 0.0
            && ctx.up_best_ask > 0.0
            && ctx.down_best_bid > 0.0
            && ctx.down_best_ask > 0.0;

        match state {
            ArasState::Done => (ArasState::Done, Decision::NoOp),

            ArasState::Idle => {
                if book_ready {
                    let active = Box::new(ArasActive::default());
                    Self::decide_active(active, ctx)
                } else {
                    (ArasState::Idle, Decision::NoOp)
                }
            }

            ArasState::Active(active) => {
                if !book_ready {
                    return (ArasState::Active(active), Decision::NoOp);
                }
                Self::decide_active(active, ctx)
            }
        }
    }

    fn decide_active(
        mut st: Box<ArasActive>,
        ctx: &StrategyContext<'_>,
    ) -> (ArasState, Decision) {
        let p = ArasParams::from_ctx(ctx);
        let now = ctx.now_ms;
        let m = ctx.metrics;

        let mut cancels: Vec<String> = Vec::new();
        let mut places: Vec<PlannedOrder> = Vec::new();

        // ── 1. Bekleyen emir fill/iptal tespiti ──────────────────────────────
        for &outcome in &[Outcome::Up, Outcome::Down] {
            let idx = outcome_idx(outcome);
            if !st.pending[idx] {
                continue;
            }

            let open = has_open_order(ctx.open_orders, outcome);

            if open {
                st.confirmed[idx] = true;
            }

            // Confirmed iken artık open değilse → fill veya harici iptal
            if st.confirmed[idx] && !open {
                st.pending[idx] = false;
                st.confirmed[idx] = false;
                st.pending_price[idx] = 0.0;
            }
        }

        // ── 2. Fill delta tespiti + ARB kilidi kontrol ────────────────────────
        let new_up = m.up_filled - st.seen_up_filled;
        let new_dn = m.down_filled - st.seen_dn_filled;
        st.seen_up_filled = m.up_filled;
        st.seen_dn_filled = m.down_filled;

        // Her yeni fill sonrası: her iki taraf da fill'liyse pair cost hesapla
        if (new_up > 0.0 || new_dn > 0.0) && m.up_filled > 0.0 && m.down_filled > 0.0 {
            let pair_cost = m.avg_up + m.avg_down;
            if pair_cost < 1.00 {
                let locked = m.up_filled.min(m.down_filled);
                let pnl = (1.0 - pair_cost) * locked;
                if pnl > st.guaranteed_pnl {
                    st.arb_lock_count += 1;
                    st.guaranteed_pnl = pnl;
                    tracing::info!(
                        pair_cost,
                        pnl,
                        locked,
                        up_avg = m.avg_up,
                        dn_avg = m.avg_down,
                        arb_count = st.arb_lock_count,
                        "aras: arb_lock"
                    );
                }
            }
        }

        // ── 3. Çift Taraflı Poll ─────────────────────────────────────────────
        // Her poll_ms'de her iki tarafa eş zamanlı emir verilir.
        // Filtreler: bant, maliyet limiti, imbalans koruması, çift pair cost.
        for &outcome in &[Outcome::Up, Outcome::Down] {
            let idx = outcome_idx(outcome);

            // Zamanlama
            if now - st.last_poll_ms[idx] < p.poll_ms {
                continue;
            }
            st.last_poll_ms[idx] = now;

            // Bant kontrolü
            if !pass_band(ctx, outcome, &p) {
                continue;
            }

            // Maliyet limiti
            let side_cost = match outcome {
                Outcome::Up => m.avg_up * m.up_filled,
                Outcome::Down => m.avg_down * m.down_filled,
            };
            if side_cost >= p.max_usd_per_side {
                continue;
            }

            // İmbalans koruması: bu taraf karşı taraftan > 1 emir kadar ileride olamaz.
            // Karşı tarafın bekleyen (henüz fill olmamış) emri de sayılır —
            // aksi takdirde bu taraf fill alırken karşı taraf pending'de bekliyorsa
            // imbalans koruması bypass edilmiş olur.
            let this_filled = match outcome {
                Outcome::Up => m.up_filled,
                Outcome::Down => m.down_filled,
            };
            let opp_idx = outcome_idx(outcome.opposite());
            let opp_effective = match outcome {
                Outcome::Up => m.down_filled,
                Outcome::Down => m.up_filled,
            } + if st.pending[opp_idx] { p.shares } else { 0.0 };
            if this_filled > opp_effective + p.shares {
                tracing::debug!(
                    side = outcome.as_str(),
                    this_filled,
                    opp_effective,
                    opp_pending = st.pending[opp_idx],
                    "aras: skipped (imbalance guard)"
                );
                continue;
            }

            // Emir fiyatı: bid − 1tick (her iki taraf için pasif maker alımı)
            // Not: bu marketlerde spread 1-tick olduğundan ask-1tick = bid oluyor,
            // dolayısıyla directional ayrım pratikte fark yaratmıyor.
            let cur_bid = ctx.best_bid(outcome);
            let entry = round_tick(cur_bid - p.tick, p.tick).clamp(p.tick, 0.99);

            // Çift pair cost filtresi — iki katmanlı:
            //
            // 1. Anlık kontrol: entry + opp_ask < 1.00
            //    Yeni emir + karşı tarafın şu anki ask'ı pair kârlılığını bozmayacak mı?
            //
            // 2. Kümülatif kontrol: new_avg_this + opp_avg < 1.00
            //    Bu emir eklendikten sonra toplam pozisyonun ortalama pair maliyeti < 1.00 kalacak mı?
            //    Karşı taraf henüz fill almamışsa bu kontrol atlanır (cold-start güvenliği).
            let opp_ask = ctx.best_ask(outcome.opposite());
            if entry + opp_ask >= 1.00 {
                tracing::debug!(
                    side = outcome.as_str(),
                    entry,
                    opp_ask,
                    pair_cost = entry + opp_ask,
                    "aras: skipped (pair cost >= 1.00)"
                );
                continue;
            }
            let (this_filled, this_avg, opp_filled, opp_avg) = match outcome {
                Outcome::Up => (m.up_filled, m.avg_up, m.down_filled, m.avg_down),
                Outcome::Down => (m.down_filled, m.avg_down, m.up_filled, m.avg_up),
            };
            if opp_filled > 0.0 {
                let new_avg_this =
                    (this_avg * this_filled + entry * p.shares) / (this_filled + p.shares);
                if new_avg_this + opp_avg >= 1.00 {
                    tracing::debug!(
                        side = outcome.as_str(),
                        new_avg_this,
                        opp_avg,
                        cum_pair_cost = new_avg_this + opp_avg,
                        "aras: skipped (cumulative pair cost >= 1.00)"
                    );
                    continue;
                }
            }

            // Mevcut emir varsa: iki durumda iptal + yenile
            //   1. Fiyat düştü (entry < pending − 1tick): daha ucuz emir imkânı
            //   2. Fiyat yükseldi (entry > pending + 3tick): emir bayatladı, piyasa uzaklaştı
            if st.pending[idx] {
                let stale_up   = entry > st.pending_price[idx] + 3.0 * p.tick;
                let better_dn  = entry < st.pending_price[idx] - p.tick;
                if !stale_up && !better_dn {
                    continue; // Mevcut emir hâlâ geçerli
                }
                // İptal et ve güncel fiyatla yenile
                let reason = buy_reason(outcome);
                for o in ctx.open_orders.iter() {
                    if o.outcome == outcome && o.side == Side::Buy && o.reason == reason {
                        cancels.push(o.id.clone());
                    }
                }
                st.pending[idx] = false;
                st.confirmed[idx] = false;
            }

            // Emir ver
            st.pending[idx] = true;
            st.pending_price[idx] = entry;
            st.confirmed[idx] = false;

            let is_first = match outcome {
                Outcome::Up => m.up_filled == 0.0,
                Outcome::Down => m.down_filled == 0.0,
            };

            let reason = if is_first {
                format!("aras:buy:{}:init", outcome.as_lowercase())
            } else {
                buy_reason(outcome)
            };

            places.push(PlannedOrder {
                outcome,
                token_id: ctx.token_id(outcome).to_string(),
                side: Side::Buy,
                price: entry,
                size: p.shares,
                order_type: OrderType::Gtc,
                reason,
            });

            tracing::debug!(
                side = outcome.as_str(),
                price = entry,
                cur_bid,
                opp_ask,
                pair_cost = entry + opp_ask,
                this_filled,
                opp_effective,
                "aras: buy order"
            );
        }

        // ── Karar üret ────────────────────────────────────────────────────────
        let decision = if !cancels.is_empty() && !places.is_empty() {
            Decision::CancelAndPlace { cancels, places }
        } else if !cancels.is_empty() {
            Decision::CancelOrders(cancels)
        } else if !places.is_empty() {
            Decision::PlaceOrders(places)
        } else {
            Decision::NoOp
        };

        (ArasState::Active(st), decision)
    }
}

// ─────────────────────────────────────────────
// ArasState yardımcıları (engine için)
// ─────────────────────────────────────────────

impl ArasState {
    /// Yeni bir ARB kilidi oluştu mu? (engine'in IPC emit'i için)
    pub fn arb_lock_count(&self) -> u32 {
        match self {
            Self::Active(st) => st.arb_lock_count,
            _ => 0,
        }
    }

    /// Birikimli garantili PnL.
    pub fn guaranteed_pnl(&self) -> f64 {
        match self {
            Self::Active(st) => st.guaranteed_pnl,
            _ => 0.0,
        }
    }

    /// Aktif hedge görev sayısı (eski API uyumluluğu — her zaman 0).
    pub fn active_hedge_count(&self) -> usize {
        0
    }
}
