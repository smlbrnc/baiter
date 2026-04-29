//! Aras stratejisi — DCA + Kademeli Hedge Arbitrajı
//!
//! Doküman: `docs/aras.md`
//! Backtest: `scripts/arb_dca_v3.py` (16 market, +$88.15, +6.22% ROI, 13W/3L)
//!
//! ## Strateji özeti
//!
//! - **DCA tarafı** (`mid > CHEAP_THRESHOLD && mid < BAND_HIGH`): Pahalı/kazanan taraf.
//!   Her `poll_secs` saniyede bir, bid fiyatı `dca_min_drop` kadar düştüyse `bid-1tick`
//!   fiyatına GTC emir verilir.
//!
//! - **Hedge tarafı** (`BAND_LOW < mid < CHEAP_THRESHOLD`): Ucuz/kaybeden taraf.
//!   DCA fill oluşunca 3-adım kademeli hedge emri açılır:
//!   `bid-3tick → (step_secs bekle) → bid-2tick → (step_secs bekle) → bid-1tick`
//!
//! - **ARB kilidi**: Hedge fill'inde `avg_up + avg_down < 1.00` → garantili kâr.
//!
//! ## Reason etiketleri (open_orders tespiti için)
//!
//! - `"aras:dca:up"` / `"aras:dca:down"` — DCA emirleri
//! - `"aras:hedge:up:N"` / `"aras:hedge:down:N"` — Hedge emri (N = adım 1/2/3)

use serde::{Deserialize, Serialize};

use super::common::{Decision, OpenOrder, PlannedOrder, StrategyContext};
use crate::types::{OrderType, Outcome, Side};

// ─────────────────────────────────────────────
// Sabitler (config'den geçersiz kılınabilir)
// ─────────────────────────────────────────────

/// Hedge tick offset sırası: bid - N*TICK
const HEDGE_TICK_OFFSETS: [i32; 3] = [3, 2, 1];

// ─────────────────────────────────────────────
// Veri yapıları
// ─────────────────────────────────────────────

/// Tek bir DCA fill'inin ardından açılan kademeli hedge denemesi.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HedgeJob {
    /// DCA fill olan taraf (pahalı taraf).
    pub main_outcome: Outcome,
    /// Hedge gönderilecek taraf (ucuz taraf).
    pub hedge_outcome: Outcome,
    /// Sonraki deneme adımı indeksi (0 = bid-3t, 1 = bid-2t, 2 = bid-1t).
    pub step: usize,
    /// Mevcut adımın başlangıç timestamp'i (ms).
    pub step_start_ms: u64,
    /// Mevcut aktif hedge emrinin fiyatı.
    pub order_price: Option<f64>,
    /// Hedge emri en az bir tick'te `open_orders`'da görüldü mü?
    pub order_confirmed: bool,
    /// Bu görev tamamlandı (fill veya tüketildi) mi?
    pub done: bool,
}

impl HedgeJob {
    fn new(main_outcome: Outcome, now_ms: u64) -> Self {
        Self {
            main_outcome,
            hedge_outcome: main_outcome.opposite(),
            step: 0,
            step_start_ms: now_ms,
            order_price: None,
            order_confirmed: false,
            done: false,
        }
    }

    fn exhausted(&self) -> bool {
        self.step >= HEDGE_TICK_OFFSETS.len()
    }

    /// Hedge emri hâlâ open_orders'da var mı?
    fn is_order_open(&self, open_orders: &[OpenOrder]) -> bool {
        if self.order_price.is_none() {
            return false;
        }
        open_orders.iter().any(|o| {
            o.outcome == self.hedge_outcome
                && o.side == Side::Buy
                && o.reason.starts_with(&format!(
                    "aras:hedge:{}:",
                    self.hedge_outcome.as_lowercase()
                ))
        })
    }
}

/// Aras aktif pozisyon durumu.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArasActive {
    // ── Timing ──────────────────────────────────────────────────────────────
    /// Son DCA poll zamanı (ms) — [Outcome::Up as usize, Outcome::Down as usize]
    pub last_poll_ms: [u64; 2],

    // ── Fill tracking ────────────────────────────────────────────────────────
    /// Önceki tick'te görülen `metrics.up_filled` değeri; artış = yeni fill.
    pub seen_up_filled: f64,
    /// Önceki tick'te görülen `metrics.down_filled` değeri.
    pub seen_dn_filled: f64,

    // ── Bekleyen DCA emirleri ────────────────────────────────────────────────
    /// Şu an UP tarafında bekleyen DCA var mı?
    pub dca_pending: [bool; 2],
    /// Bu emrin yerleştirildiği fiyat (band ve requote kontrolü için).
    pub dca_price: [f64; 2],
    /// Emir `open_orders`'da en az bir kez görüldü mü? (yanlış-fill önlemi)
    pub dca_confirmed: [bool; 2],

    // ── Hedge görevleri ─────────────────────────────────────────────────────
    pub hedge_jobs: Vec<HedgeJob>,

    // ── ARB istatistikleri ───────────────────────────────────────────────────
    /// UP tarafındaki hedge edilmiş (pair'lenmiş) toplam share.
    pub up_hedged: f64,
    /// DOWN tarafındaki hedge edilmiş toplam share.
    pub dn_hedged: f64,
    /// Kilitlenmiş ARB sayısı (loglama için).
    pub arb_lock_count: u32,
    /// Birikimli garantili PnL ($).
    pub guaranteed_pnl: f64,
}

impl Default for ArasActive {
    fn default() -> Self {
        Self {
            last_poll_ms: [0; 2],
            seen_up_filled: 0.0,
            seen_dn_filled: 0.0,
            dca_pending: [false; 2],
            dca_price: [0.0; 2],
            dca_confirmed: [false; 2],
            hedge_jobs: Vec::new(),
            up_hedged: 0.0,
            dn_hedged: 0.0,
            arb_lock_count: 0,
            guaranteed_pnl: 0.0,
        }
    }
}

/// Aras FSM durumları.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ArasState {
    /// Kitap henüz hazır değil (her iki bid/ask = 0).
    Idle,
    /// Market aktif — DCA + hedge döngüsü çalışıyor.
    Active(Box<ArasActive>),
    /// Piyasa kapandı (market_remaining_secs ≤ 0).
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

/// DCA band kontrolü: pahalı taraf için.
#[inline]
fn pass_band_dca(ctx: &StrategyContext<'_>, outcome: Outcome, p: &ArasParams) -> bool {
    let m = mid(ctx, outcome);
    m > p.cheap_threshold && m < p.band_high
}

/// Hedge band kontrolü: ucuz taraf için.
#[inline]
fn pass_band_hedge(ctx: &StrategyContext<'_>, outcome: Outcome, p: &ArasParams) -> bool {
    let m = mid(ctx, outcome);
    m >= p.band_low && m < p.cheap_threshold
}

/// DCA reason etiketi.
fn dca_reason(outcome: Outcome) -> String {
    format!("aras:dca:{}", outcome.as_lowercase())
}

/// `open_orders`'da bu outcome için DCA emri var mı?
fn has_open_dca(open_orders: &[OpenOrder], outcome: Outcome) -> bool {
    let reason = dca_reason(outcome);
    open_orders
        .iter()
        .any(|o| o.outcome == outcome && o.side == Side::Buy && o.reason == reason)
}

// ─────────────────────────────────────────────
// Config wrapper
// ─────────────────────────────────────────────

struct ArasParams {
    poll_ms: u64,
    dca_min_drop: f64,
    shares: f64,
    max_usd_per_side: f64,
    hedge_step_ms: u64,
    band_low: f64,
    band_high: f64,
    cheap_threshold: f64,
    tick: f64,
}

impl ArasParams {
    fn from_ctx(ctx: &StrategyContext<'_>) -> Self {
        let sp = ctx.strategy_params;
        Self {
            poll_ms: (sp.aras_poll_secs() * 1000.0) as u64,
            dca_min_drop: sp.aras_dca_min_drop(),
            shares: sp.aras_shares_per_order(),
            max_usd_per_side: sp.aras_max_usd_per_side(),
            hedge_step_ms: (sp.aras_hedge_step_secs() * 1000.0) as u64,
            band_low: sp.aras_band_low(),
            band_high: sp.aras_band_high(),
            cheap_threshold: sp.aras_cheap_threshold(),
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
        // Piyasa kapandıysa dur
        if let Some(rem) = ctx.market_remaining_secs {
            if rem <= 0.0 {
                return (ArasState::Done, Decision::NoOp);
            }
        }

        // Kitap hazır mı?
        let book_ready = ctx.up_best_bid > 0.0
            && ctx.up_best_ask > 0.0
            && ctx.down_best_bid > 0.0
            && ctx.down_best_ask > 0.0;

        match state {
            ArasState::Done => (ArasState::Done, Decision::NoOp),

            ArasState::Idle => {
                if book_ready {
                    let active = Box::new(ArasActive::default());
                    // İlk tick'te decide_active çağır
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

        // ── 1. DCA pending emir tespiti (open_orders kontrol + fill algılama) ──
        for &outcome in &[Outcome::Up, Outcome::Down] {
            let idx = outcome_idx(outcome);
            if !st.dca_pending[idx] {
                continue;
            }

            let open = has_open_dca(ctx.open_orders, outcome);

            // Emir open_orders'da göründüyse confirm et
            if open {
                st.dca_confirmed[idx] = true;
            }

            // Emir confirmed iken artık yoksa → fill (veya Polymarket iptali)
            if st.dca_confirmed[idx] && !open {
                st.dca_pending[idx] = false;
                st.dca_confirmed[idx] = false;
                st.dca_price[idx] = 0.0;
                // Fill tespiti — metrics.up/dn_filled artışı adım 2'de ele alınır
            }
        }

        // ── 2. Fill artışı tespiti → Hedge görevi aç ────────────────────────
        let new_up = m.up_filled - st.seen_up_filled;
        let new_dn = m.down_filled - st.seen_dn_filled;
        st.seen_up_filled = m.up_filled;
        st.seen_dn_filled = m.down_filled;

        // UP fill → DOWN için hedge job aç (DOWN ucuz bandındaysa)
        if new_up > 0.0 && pass_band_hedge(ctx, Outcome::Down, &p) {
            // Sadece DCA kaynaklı fill için hedge aç (ilk fill veya DCA fill)
            // Hedge fill'leri çift saymayı önlemek için: DOWN hedge job'u sadece
            // UP fill oluştuğunda eklenir.
            st.hedge_jobs.push(HedgeJob::new(Outcome::Up, now));
        }

        // DOWN fill → UP için hedge job aç
        if new_dn > 0.0 && pass_band_hedge(ctx, Outcome::Up, &p) {
            st.hedge_jobs.push(HedgeJob::new(Outcome::Down, now));
        }

        // ── 3. Hedge görevi işleme (fill + adım geçişi) ─────────────────────
        for job in st.hedge_jobs.iter_mut() {
            if job.done {
                continue;
            }

            let hedge_oc = job.hedge_outcome;

            // Hedge emri open_orders'da mı?
            let order_open = job.is_order_open(ctx.open_orders);
            if job.order_price.is_some() && order_open {
                job.order_confirmed = true;
            }

            // Fill tespiti: confirmed && artık açık değil
            if job.order_confirmed && !order_open && job.order_price.is_some() {
                // Fill gerçekleşti — ARB kontrolü
                let avg_main = match job.main_outcome {
                    Outcome::Up => m.avg_up,
                    Outcome::Down => m.avg_down,
                };
                let avg_hedge = match hedge_oc {
                    Outcome::Up => m.avg_up,
                    Outcome::Down => m.avg_down,
                };
                let pair_cost = avg_main + avg_hedge;

                let main_unhedged = match job.main_outcome {
                    Outcome::Up => m.up_filled - st.up_hedged,
                    Outcome::Down => m.down_filled - st.dn_hedged,
                };
                let hedge_unhedged = match hedge_oc {
                    Outcome::Up => m.up_filled - st.up_hedged,
                    Outcome::Down => m.down_filled - st.dn_hedged,
                };
                let hedgeable = main_unhedged.min(hedge_unhedged).min(p.shares);

                if pair_cost < 1.00 && hedgeable > 0.0 {
                    let pnl = (1.0 - pair_cost) * hedgeable;
                    // Hedge edilen share'leri kaydet
                    match job.main_outcome {
                        Outcome::Up => st.up_hedged += hedgeable,
                        Outcome::Down => st.dn_hedged += hedgeable,
                    }
                    match hedge_oc {
                        Outcome::Up => st.up_hedged += hedgeable,
                        Outcome::Down => st.dn_hedged += hedgeable,
                    }
                    st.arb_lock_count += 1;
                    st.guaranteed_pnl += pnl;

                    tracing::info!(
                        pair_cost,
                        pnl,
                        hedgeable,
                        arb_count = st.arb_lock_count,
                        "aras: arb_lock"
                    );
                }
                job.done = true;
                continue;
            }

            // Adım geçişi: emir henüz konmadı veya timeout doldu
            let step_timeout =
                job.order_price.is_some() && (now - job.step_start_ms) >= p.hedge_step_ms;
            let needs_new_order = job.order_price.is_none() || step_timeout;

            if !needs_new_order {
                continue;
            }

            // Önceki hedge emrini iptal et
            if step_timeout {
                // open_orders'daki bu job'a ait emri bul ve iptal et
                let hedge_reason_prefix =
                    format!("aras:hedge:{}:", hedge_oc.as_lowercase());
                for o in ctx.open_orders.iter() {
                    if o.outcome == hedge_oc
                        && o.side == Side::Buy
                        && o.reason.starts_with(&hedge_reason_prefix)
                    {
                        cancels.push(o.id.clone());
                    }
                }
                job.order_price = None;
                job.order_confirmed = false;
            }

            // Tüm adımlar tükendi → vazgeç
            if job.exhausted() {
                job.done = true;
                tracing::debug!(
                    hedge_side = hedge_oc.as_str(),
                    "aras: hedge exhausted after all steps"
                );
                continue;
            }

            // Bant kontrolü
            if !pass_band_hedge(ctx, hedge_oc, &p) {
                job.done = true;
                tracing::debug!(
                    hedge_side = hedge_oc.as_str(),
                    mid = mid(ctx, hedge_oc),
                    "aras: hedge cancelled (band)"
                );
                continue;
            }

            // Yeni hedge emri
            let tick_offset = HEDGE_TICK_OFFSETS[job.step];
            let h_bid = ctx.best_bid(hedge_oc);
            let h_price =
                round_tick(h_bid - tick_offset as f64 * p.tick, p.tick).clamp(p.tick, 0.99);

            let step_display = job.step + 1;
            let reason = format!(
                "aras:hedge:{}:{}",
                hedge_oc.as_lowercase(),
                step_display
            );

            places.push(PlannedOrder {
                outcome: hedge_oc,
                token_id: ctx.token_id(hedge_oc).to_string(),
                side: Side::Buy,
                price: h_price,
                size: p.shares,
                order_type: OrderType::Gtc,
                reason,
            });

            job.order_price = Some(h_price);
            job.step_start_ms = now;
            job.step += 1;

            tracing::debug!(
                hedge_side = hedge_oc.as_str(),
                step = step_display,
                price = h_price,
                h_bid,
                "aras: hedge order"
            );
        }

        // Tamamlanan job'ları temizle
        st.hedge_jobs.retain(|j| !j.done);

        // ── 4. DCA POLL — her poll_ms saniyede pahalı tarafa emir ────────────
        for &outcome in &[Outcome::Up, Outcome::Down] {
            let idx = outcome_idx(outcome);

            // Zamanlama kontrolü
            if now - st.last_poll_ms[idx] < p.poll_ms {
                continue;
            }
            st.last_poll_ms[idx] = now;

            // Band kontrolü (pahalı taraf)
            if !pass_band_dca(ctx, outcome, &p) {
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

            // Mevcut DCA emri varsa, daha iyi fiyat oluştuysa güncelle
            let cur_bid = ctx.best_bid(outcome);
            let entry = round_tick(cur_bid - p.tick, p.tick).clamp(p.tick, 0.99);

            if st.dca_pending[idx] {
                // Mevcut emirden > 1 tick daha iyi fiyat oluştu → iptal + yenile
                if entry < st.dca_price[idx] - p.tick {
                    // Eski emri iptal et
                    let reason = dca_reason(outcome);
                    for o in ctx.open_orders.iter() {
                        if o.outcome == outcome && o.side == Side::Buy && o.reason == reason {
                            cancels.push(o.id.clone());
                        }
                    }
                    st.dca_pending[idx] = false;
                    st.dca_confirmed[idx] = false;
                } else {
                    continue; // Mevcut emir yeterli
                }
            }

            // DCA tetik: ilk giriş veya avg'dan yeterince düşüş
            let avg = match outcome {
                Outcome::Up => m.avg_up,
                Outcome::Down => m.avg_down,
            };
            let filled = match outcome {
                Outcome::Up => m.up_filled,
                Outcome::Down => m.down_filled,
            };

            let first_entry = filled == 0.0;
            let dca_ok = filled > 0.0 && entry < avg - p.dca_min_drop;

            if !first_entry && !dca_ok {
                continue;
            }

            // Emir ver
            st.dca_pending[idx] = true;
            st.dca_price[idx] = entry;
            st.dca_confirmed[idx] = false;

            let reason = if first_entry {
                format!("aras:dca:{}:init", outcome.as_lowercase())
            } else {
                format!("aras:dca:{}", outcome.as_lowercase())
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
                avg,
                filled,
                first_entry,
                "aras: dca order"
            );
        }

        // ── Karar üret ───────────────────────────────────────────────────────
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

    /// Aktif hedge görev sayısı (monitor için).
    pub fn active_hedge_count(&self) -> usize {
        match self {
            Self::Active(st) => st.hedge_jobs.len(),
            _ => 0,
        }
    }
}
