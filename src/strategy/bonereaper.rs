//! Bonereaper stratejisi — Polymarket "Bonereaper" wallet
//! (`0xeebde7a0e019a63e6b476eb425505b7b3e6eba30`) davranış kopyası.
//!
//! Strateji **signal-driven değildir**; pure order-book reactive martingale +
//! price-triggered winner injection. `data/realbot.log` (3472 trade, 3h window)
//! analizi: real bot winner_bid ≥ $0.98 olduğunda HEMEN $0.99 injection yapıyor
//! — zaman kısıtı yok! T-161s'de de T-15s'de de tetiklenebiliyor. Her injection
//! ~$100-$200, 20-40 kez tekrarlanıyor (toplam $4000-$5000/market). Loser taraf
//! $0.10-$0.20 bandında $40-$450 arası küçük scalp topluyor (lottery aspect).
//!
//! ## Karar zinciri (v3 — realbot.log doğrulamalı)
//!
//! 1. **Window**: `now ∈ [start, end]`; OB ready.
//! 2. **LATE WINNER (ana)** (`max(bid) ≥ bid_thr [0.98]` — fiyat bazlı, ZAMAN BAĞIMSIZ):
//!    winner tarafa `late_winner_usdc` notional taker BUY @ ask. Cooldown bypass.
//!    `lw_secs=300` default → tüm market boyunca aktif; quota ile toplam cap.
//! 3. **LATE WINNER (burst)** — default KAPALI (`lw_burst_secs=0`); gerçek bot
//!    ayrı burst wave kullanmıyor, tüm injection tek mekanizmadan geliyor.
//! 4. **Cooldown** (`now − last_buy < buy_cooldown_ms`): NoOp.
//! 5. **İlk emir kapısı** (`!first_done`): `|up_bid − down_bid| < first_spread_min`
//!    ise NoOp; aşılınca yön = yüksek bid tarafı (winner momentum).
//! 6. **Yön seçimi (sonraki emirler)**:
//!    - `|up_filled − down_filled| > imbalance_thr` → weaker side rebalance
//!    - aksi: `|Δup_bid|` vs `|Δdn_bid|` → büyük delta tarafı (`ob_driven`)
//! 7. **Yön bazlı min_price filter**: winner side `ctx.min_price`,
//!    loser side `loser_min_price` (1¢ scalp).
//! 8. **Martingale-down guard**: loser side avg fiyatı `avg_loser_max` aşarsa
//!    o yöne `loser_scalp_usdc` minimal scalp ile sınırlı.
//! 9. **Dinamik size**:
//!    - Loser side scalp: `loser_scalp_usdc`
//!    - Bid bucket'a göre: longshot / mid / high
//!    - **Winner pyramid scaling**: `to_end < late_pyramid_secs && dir == winner`
//!      ise size × `winner_size_factor`.
//! 10. **avg_sum soft cap** (`new_avg + opp_avg > max_avg_sum`): NoOp (loser
//!     scalp HARİÇ — scalp her zaman serbest).
//! 11. **Place taker BUY @ ask** (GTC limit, anında fill).
//!
//! ## Reason etiketleri
//!
//! `bonereaper:buy:{up,down}` — normal BUY (winner pyramid dahil).
//! `bonereaper:scalp:{up,down}` — loser side 1¢ long-shot scalp.
//! `bonereaper:lw:{up,down}` — late winner ana dalga.
//! `bonereaper:lwb:{up,down}` — late winner burst (2. dalga).

use serde::{Deserialize, Serialize};

use super::common::{Decision, PlannedOrder, StrategyContext};
use crate::types::{OrderType, Outcome, Side};

#[inline]
const fn reason_buy(dir: Outcome) -> &'static str {
    match dir {
        Outcome::Up => "bonereaper:buy:up",
        Outcome::Down => "bonereaper:buy:down",
    }
}

#[inline]
const fn reason_lw(dir: Outcome) -> &'static str {
    match dir {
        Outcome::Up => "bonereaper:lw:up",
        Outcome::Down => "bonereaper:lw:down",
    }
}

#[inline]
const fn reason_lw_burst(dir: Outcome) -> &'static str {
    match dir {
        Outcome::Up => "bonereaper:lwb:up",
        Outcome::Down => "bonereaper:lwb:down",
    }
}

#[inline]
const fn reason_scalp(dir: Outcome) -> &'static str {
    match dir {
        Outcome::Up => "bonereaper:scalp:up",
        Outcome::Down => "bonereaper:scalp:down",
    }
}

/// Loser tarafı anlık bid fiyatına göre belirler.
///
/// ÖNEMLI: Guard yalnızca bid farkı anlamlı olduğunda etkindir.
/// $0.40-$0.60 belirsiz bölgede $0.01-$0.05 bid farkı güvenilir sinyal değil
/// (market henüz karar vermemiş). Loser guard bu bölgede gerekmez/zararlıdır.
/// Fark büyük olduğunda (≥ 0.20) market net kazananı göstermiştir.
///
/// Örnek: UP_bid=$0.80, DOWN_bid=$0.19 → fark=$0.61 → DOWN loser kesin ✓
///        UP_bid=$0.51, DOWN_bid=$0.48 → fark=$0.03 → belirsiz, None döner
///
/// None → loser_guard uygulanmaz (her iki taraf serbestçe alınabilir).
#[inline]
fn loser_side(up_bid: f64, dn_bid: f64) -> Option<Outcome> {
    const LOSER_SPREAD_MIN: f64 = 0.20; // min fark: piyasa net karar verdi
    let spread = (up_bid - dn_bid).abs();
    if spread < LOSER_SPREAD_MIN {
        None // Belirsiz bölge → loser guard yok
    } else if up_bid >= dn_bid {
        Some(Outcome::Down) // UP dominant → DOWN loser
    } else {
        Some(Outcome::Up) // DOWN dominant → UP loser
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub enum BonereaperState {
    #[default]
    Idle,
    Active(BonereaperActive),
    /// Geriye uyumlu (eski serde); yeni akışta üretilmiyor.
    Done,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BonereaperActive {
    /// Son BUY emrinin ms zamanı; 0 = henüz emir yok.
    #[serde(default)]
    pub last_buy_ms: u64,
    /// Önceki tick UP bid (delta hesabı).
    #[serde(default)]
    pub last_up_bid: f64,
    /// Önceki tick DOWN bid.
    #[serde(default)]
    pub last_dn_bid: f64,
    /// Late winner injection sayacı (telemetri/log için).
    #[serde(default)]
    pub lw_injections: u32,
    /// İlk emir verildi mi? Spread-gated start için kullanılır.
    #[serde(default)]
    pub first_done: bool,
}

pub struct BonereaperEngine;

impl BonereaperEngine {
    pub fn decide(
        state: BonereaperState,
        ctx: &StrategyContext<'_>,
    ) -> (BonereaperState, Decision) {
        let to_end = ctx.market_remaining_secs.unwrap_or(f64::MAX);
        let p = ctx.strategy_params;

        match state {
            BonereaperState::Done => (BonereaperState::Done, Decision::NoOp),

            BonereaperState::Idle => {
                let book_ready = ctx.up_best_bid > 0.0
                    && ctx.up_best_ask > 0.0
                    && ctx.down_best_bid > 0.0
                    && ctx.down_best_ask > 0.0;
                if !book_ready {
                    return (BonereaperState::Idle, Decision::NoOp);
                }
                let active = BonereaperActive {
                    last_buy_ms: 0,
                    last_up_bid: ctx.up_best_bid,
                    last_dn_bid: ctx.down_best_bid,
                    lw_injections: 0,
                    first_done: false,
                };
                (BonereaperState::Active(active), Decision::NoOp)
            }

            BonereaperState::Active(mut st) => {
                if to_end < 0.0 {
                    return (BonereaperState::Active(st), Decision::NoOp);
                }
                if ctx.up_best_bid <= 0.0 || ctx.down_best_bid <= 0.0 {
                    return (BonereaperState::Active(st), Decision::NoOp);
                }

                // ── LATE WINNER (ana + burst) ───────────────────────────
                // Multi-LW pyramid: ana dalga T-late_winner_secs, burst dalga
                // T-lw_burst_secs. Her iki dalga `lw_max_per_session` quota'sı
                // ile sınırlı; cooldown bypass.
                let lw_secs = p.bonereaper_late_winner_secs() as f64;
                let lw_usdc = p.bonereaper_late_winner_usdc();
                let lw_thr = p.bonereaper_late_winner_bid_thr();
                let lw_max = p.bonereaper_lw_max_per_session();
                let lw_burst_secs = p.bonereaper_lw_burst_secs() as f64;
                let lw_burst_usdc = p.bonereaper_lw_burst_usdc();
                let lw_quota_ok = lw_max == 0 || st.lw_injections < lw_max;

                if lw_quota_ok && to_end > 0.0 {
                    // Burst dalga (daha öncelikli, daha geç tetiklenir)
                    let burst_active = lw_burst_usdc > 0.0
                        && lw_burst_secs > 0.0
                        && to_end <= lw_burst_secs;
                    let main_active = lw_usdc > 0.0
                        && lw_secs > 0.0
                        && to_end <= lw_secs
                        && !burst_active;

                    let lw_kind = if burst_active {
                        Some((lw_burst_usdc, true))
                    } else if main_active {
                        Some((lw_usdc, false))
                    } else {
                        None
                    };

                    if let Some((usdc, is_burst)) = lw_kind {
                        let (winner, w_bid, w_ask) = if ctx.up_best_bid >= ctx.down_best_bid {
                            (Outcome::Up, ctx.up_best_bid, ctx.up_best_ask)
                        } else {
                            (Outcome::Down, ctx.down_best_bid, ctx.down_best_ask)
                        };
                        if w_bid >= lw_thr && w_ask > 0.0 {
                            let size = (usdc / w_ask).ceil();

                            // LW opp_avg guard: karşı pozisyon pahalıysa ($0.50+) LW bloke.
                            // Gerçek bot loser tarafı avg $0.10-$0.35 tutar.
                            // $0.50+ loser = yanlış yönde pahalı birikim → LW zararlı.
                            //
                            // 5-market doğrulama:
                            //   1778565600: DN_avg=$0.451 < $0.50 → LW SERBEST ✓ (+$234)
                            //   1778566800: UP_avg=$0.531 > $0.50 → LW BLOKE ✓ (DryRun yanlış tetikleme)
                            //   1778568000: UP_avg=$0.351 < $0.50 → LW SERBEST ✓ (+$102)
                            let m = ctx.metrics;
                            let (opp_filled, opp_avg) = if winner == Outcome::Up {
                                (m.down_filled, m.avg_down)
                            } else {
                                (m.up_filled, m.avg_up)
                            };
                            const LW_OPP_AVG_MAX: f64 = 0.50;
                            if opp_filled > 0.0 && opp_avg > LW_OPP_AVG_MAX {
                                // Karşı taraf pahalı → LW bu tick'te atla
                            } else {
                                let reason = if is_burst {
                                    reason_lw_burst(winner)
                                } else {
                                    reason_lw(winner)
                                };
                                if let Some(o) = make_buy(ctx, winner, w_ask, size, reason) {
                                    st.last_buy_ms = ctx.now_ms;
                                    st.lw_injections = st.lw_injections.saturating_add(1);
                                    st.last_up_bid = ctx.up_best_bid;
                                    st.last_dn_bid = ctx.down_best_bid;
                                    st.first_done = true;
                                    return (
                                        BonereaperState::Active(st),
                                        Decision::PlaceOrders(vec![o]),
                                    );
                                }
                            }
                        }
                    }
                }

                // ── COOLDOWN ────────────────────────────────────────────
                let cd_ms = p.bonereaper_buy_cooldown_ms();
                if st.last_buy_ms > 0 && ctx.now_ms.saturating_sub(st.last_buy_ms) < cd_ms {
                    st.last_up_bid = ctx.up_best_bid;
                    st.last_dn_bid = ctx.down_best_bid;
                    return (BonereaperState::Active(st), Decision::NoOp);
                }

                // ── YÖN SEÇİMİ ──────────────────────────────────────────
                let dir = if !st.first_done {
                    // İlk emir: spread-gated. |up_bid - down_bid| eşiği aşılmadan
                    // emir verme; aşılınca yön karar zinciri:
                    //   1) BSI (Binance CVD imbalance) primer — |bsi| ≥ 0.30:
                    //      Gerçek Bonereaper'ın birincil sinyali. DOWN bid yüksek
                    //      olmasına rağmen BSI>0 ise UP alır (docs/bonereaper.md §4).
                    //      Canlı analiz: DOWN=$0.52 > UP=$0.46 iken Bonereaper UP aldı.
                    //   2) OB fallback: yüksek bid tarafı (winner momentum).
                    let spread_min = p.bonereaper_first_spread_min();
                    let spread = ctx.up_best_bid - ctx.down_best_bid;
                    if spread.abs() < spread_min {
                        // Sinyal henüz net değil — bid history güncelle, bekle.
                        st.last_up_bid = ctx.up_best_bid;
                        st.last_dn_bid = ctx.down_best_bid;
                        return (BonereaperState::Active(st), Decision::NoOp);
                    }
                    // BSI primer (docs/bonereaper.md §4): |imbalance| ≥ 0.30
                    const BSI_THRESHOLD: f64 = 0.30;
                    if let Some(bsi) = ctx.bsi {
                        if bsi >= BSI_THRESHOLD {
                            Outcome::Up
                        } else if bsi <= -BSI_THRESHOLD {
                            Outcome::Down
                        } else {
                            // |BSI| < 0.30 → OB fallback
                            if spread > 0.0 { Outcome::Up } else { Outcome::Down }
                        }
                    } else {
                        // BSI yok → OB fallback
                        if spread > 0.0 { Outcome::Up } else { Outcome::Down }
                    }
                } else {
                    let m = ctx.metrics;
                    let imb = m.up_filled - m.down_filled;
                    let imb_thr = p.bonereaper_imbalance_thr();
                    if imb.abs() > imb_thr {
                        // Weaker side rebalance
                        if imb > 0.0 { Outcome::Down } else { Outcome::Up }
                    } else {
                        // ob_driven: bid'i daha çok değişen taraf
                        let d_up = (ctx.up_best_bid - st.last_up_bid).abs();
                        let d_dn = (ctx.down_best_bid - st.last_dn_bid).abs();
                        if d_up == 0.0 && d_dn == 0.0 {
                            // Delta yoksa: bid'i yüksek olan taraf (winner momentum)
                            if ctx.up_best_bid >= ctx.down_best_bid {
                                Outcome::Up
                            } else {
                                Outcome::Down
                            }
                        } else if d_up >= d_dn {
                            Outcome::Up
                        } else {
                            Outcome::Down
                        }
                    }
                };

                // Bid history güncelle (her tick)
                st.last_up_bid = ctx.up_best_bid;
                st.last_dn_bid = ctx.down_best_bid;

                let bid = ctx.best_bid(dir);
                let ask = ctx.best_ask(dir);
                if bid <= 0.0 || ask <= 0.0 {
                    return (BonereaperState::Active(st), Decision::NoOp);
                }

                // Loser/winner: bid farkı ≥ $0.20 olduğunda aktif (piyasa net karar verdi)
                // $0.40-$0.60 belirsiz bölgede guard devreye girmez.
                let metrics = ctx.metrics;
                let loser_opt = loser_side(ctx.up_best_bid, ctx.down_best_bid);
                let is_loser_dir = loser_opt.map_or(false, |l| dir == l);

                // Yön bazlı min_price (loser side 1¢ scalp serbest)
                let effective_min = if is_loser_dir {
                    p.bonereaper_loser_min_price().min(ctx.min_price)
                } else {
                    ctx.min_price
                };
                if bid < effective_min || bid > ctx.max_price {
                    return (BonereaperState::Active(st), Decision::NoOp);
                }

                // Martingale-down guard: loser tarafta avg fiyatı yüksekse
                // (pahalı down-pyramid) sadece minimal scalp yap.
                let avg_loser_max = p.bonereaper_avg_loser_max();
                let (cur_filled, cur_avg, opp_filled, opp_avg) = match dir {
                    Outcome::Up => (
                        metrics.up_filled,
                        metrics.avg_up,
                        metrics.down_filled,
                        metrics.avg_down,
                    ),
                    Outcome::Down => (
                        metrics.down_filled,
                        metrics.avg_down,
                        metrics.up_filled,
                        metrics.avg_up,
                    ),
                };
                let scalp_only = is_loser_dir && cur_filled > 0.0 && cur_avg > avg_loser_max;

                // Dinamik size hesabı
                let scalp_usdc = p.bonereaper_loser_scalp_usdc();
                let scalp_max_price = p.bonereaper_loser_scalp_max_price();
                // Loser side scalp koşulu: bid scalp_max_price altında olduğunda
                // scalp boyutu kullan (1¢-30¢ bandı, real bot'a uygun).
                let is_scalp_band = is_loser_dir && bid <= scalp_max_price && scalp_usdc > 0.0;
                let usdc = if scalp_only && scalp_usdc > 0.0 {
                    // Pahalı martingale-down → sadece $1 bilet
                    scalp_usdc
                } else if is_scalp_band {
                    // Loser side scalp bandı → kuruşluk bilet
                    scalp_usdc
                } else {
                    // 14-market analizi (5 önceki + 9 yeni log):
                    // $0.30-0.65 band real avg $12-17 → size_mid_usdc ($15)
                    // $0.65-0.85 band real avg $33    → size_high_usdc ($30) ← threshold değişti
                    // $0.85+     band real avg $78    → size_high_usdc × winner_factor
                    let base = if bid <= 0.30 {
                        p.bonereaper_size_longshot_usdc()
                    } else if bid <= 0.65 {
                        p.bonereaper_size_mid_usdc()
                    } else {
                        p.bonereaper_size_high_usdc()
                    };
                    // Winner pyramid scaling: T-late_pyramid_secs içinde winner
                    // tarafa size çarpanı uygula.
                    let lp_secs = p.bonereaper_late_pyramid_secs() as f64;
                    if !is_loser_dir && lp_secs > 0.0 && to_end > 0.0 && to_end <= lp_secs {
                        base * p.bonereaper_winner_size_factor()
                    } else {
                        base
                    }
                };
                if usdc <= 0.0 {
                    return (BonereaperState::Active(st), Decision::NoOp);
                }

                // Scalp türü tespit (avg_sum cap ve order_price için kullanılır)
                let is_any_scalp = scalp_only || is_scalp_band;

                // ── LOSER GUARD ───────────────────────────────────────────────
                // Anlık bid fiyatı düşük olan taraf = loser. Loser tarafına
                // mid-fiyat ($0.20+) alım yapma; sadece scalp bandı ($0.01-$0.20)
                // veya LW (ayrı kod yolu) ile ucuza topla.
                //
                // Yeni bid-tabanlı loser_side ile ob_driven yönü her zaman WINNER
                // taraftır → is_loser_dir=false → guard asla yanlış bloklama yapmaz.
                // Loser yönüne ob_driven gönderme çok nadirdir; olursa scalp serbest.
                if is_loser_dir && !is_any_scalp && bid > scalp_max_price {
                    st.last_up_bid = ctx.up_best_bid;
                    st.last_dn_bid = ctx.down_best_bid;
                    return (BonereaperState::Active(st), Decision::NoOp);
                }

                // Emir fiyatı: Maker (BID) vs Taker (ASK)
                // Gerçek bot: normal alımlar LIMIT BID (maker) = stale fill avantajı.
                // Scalp: taker (ASK) — ucuz loser fiyatını hemen kilitler.
                // Kazanan taraf maker → piyasa yükselirken eski düşük BID'den fill olur.
                let order_price = if is_any_scalp {
                    ask // scalp: agresif taker (loser ucuzu anında kilitler)
                } else {
                    bid // winner accumulation: maker (resting limit bid)
                };
                let size = (usdc / order_price).ceil();

                // avg_sum soft cap — loser scalp HARİÇ (scalp her zaman serbest)
                if !is_any_scalp && opp_filled > 0.0 {
                    let max_avg_sum = p.bonereaper_max_avg_sum();
                    let new_avg = if cur_filled > 0.0 {
                        (cur_avg * cur_filled + order_price * size) / (cur_filled + size)
                    } else {
                        order_price
                    };
                    if new_avg + opp_avg > max_avg_sum {
                        return (BonereaperState::Active(st), Decision::NoOp);
                    }
                }

                let reason = if is_any_scalp {
                    reason_scalp(dir)
                } else {
                    reason_buy(dir)
                };
                if let Some(o) = make_buy(ctx, dir, order_price, size, reason) {
                    st.last_buy_ms = ctx.now_ms;
                    st.first_done = true;
                    return (
                        BonereaperState::Active(st),
                        Decision::PlaceOrders(vec![o]),
                    );
                }
                (BonereaperState::Active(st), Decision::NoOp)
            }
        }
    }
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
