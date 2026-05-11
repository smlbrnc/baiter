#!/usr/bin/env python3
"""Bot scenario backtest — canonical PnL kuralıyla (bid > 0.95).

Bonereaper stratejisinin tick-by-tick simülasyonu, parametre varyantları
için. PnL hesabı UI/backend ile birebir aynı kuralı kullanır:
  - up_best_bid (last tick) > 0.95  → UP kazandı, pnl_if_up topla
  - down_best_bid > 0.95            → DOWN kazandı, pnl_if_down topla
  - Aksi halde session toplamdan HARİÇ (henüz çözülmemiş)

Kullanım:
    python3 bot_scenario_backtest.py BOT_ID [DB_PATH]

Çıktı: senaryo karşılaştırma tablosu, her satırda canonical PnL.
"""
import argparse
import math
import sqlite3
import sys
from dataclasses import dataclass

FEE_RATE = 0.0002
MIN_PRICE = 0.10
MAX_PRICE = 0.95


@dataclass
class Params:
    # Cooldown / timing
    cooldown_ms: int = 3_000
    first_spread_min: float = 0.02
    # LW
    lw_secs: int = 30
    lw_thr: float = 0.92
    lw_usdc: float = 500.0
    lw_max: int = 5
    lw_burst_secs: int = 12
    lw_burst_usdc: float = 200.0
    # Sizing
    sz_long: float = 5.0
    sz_mid: float = 10.0
    sz_high: float = 15.0
    # Imbalance / pyramid
    imb_thr: float = 50.0
    max_avg_sum: float = 1.30
    late_pyramid_secs: int = 60
    winner_size_factor: float = 5.0
    # Loser scalp
    loser_min_price: float = 0.01
    loser_scalp_usdc: float = 1.0
    loser_scalp_max_price: float = 0.30
    avg_loser_max: float = 0.50
    # YENI Tier 1 — risk yönetimi
    stop_loss_usdc: float = 0.0  # 0 = kapalı; cost > X olunca yeni BUY yok
    # YENI Tier 2 — dinamik adaptasyon
    winner_factor_dynamic: bool = False  # True: factor = max(1, 5 - cur_avg*5)
    pre_window_secs: int = 0  # Pencere açılışında ilk X saniye trade etme


def winner_of(con, bot_id, sess) -> str | None:
    """Canonical winner kuralı: bid > 0.95."""
    r = con.execute(
        "SELECT up_best_bid, down_best_bid FROM market_ticks "
        "WHERE bot_id=? AND market_session_id=? ORDER BY ts_ms DESC LIMIT 1",
        (bot_id, sess),
    ).fetchone()
    if not r or r[0] is None:
        return None
    ub, db = r[0] or 0.0, r[1] or 0.0
    if ub > 0.95:
        return "UP"
    if db > 0.95:
        return "DOWN"
    return None  # çözülmemiş


def loser_side(avg_up, avg_dn, up_filled, dn_filled, up_bid, dn_bid):
    if up_filled <= 0 and dn_filled <= 0:
        return "UP" if up_bid <= dn_bid else "DOWN"
    if up_filled > 0 and dn_filled > 0:
        return "DOWN" if avg_up >= avg_dn else "UP"
    return "DOWN" if up_filled > 0 else "UP"


def sim_session(con, bot_id, sess, p: Params):
    """Tek session için tick-by-tick simülasyon. Çözülmemiş session → None.

    Çözülmüş session için: cost, fees, pnl (winner side fill - cost), trade counts.
    `min_order_size`: market_sessions tablosundan; engine `make_buy` reddi için.
    """
    w = winner_of(con, bot_id, sess)
    if w is None:
        return None  # canonical kural: hariç
    sess_meta = con.execute(
        "SELECT end_ts, min_order_size FROM market_sessions WHERE id=?", (sess,)
    ).fetchone()
    end_ts = sess_meta[0]
    api_min_order_size = sess_meta[1] or 5.0  # default 5
    ticks = con.execute(
        "SELECT ts_ms, up_best_bid, up_best_ask, down_best_bid, down_best_ask "
        "FROM market_ticks WHERE bot_id=? AND market_session_id=? ORDER BY ts_ms",
        (bot_id, sess),
    ).fetchall()

    last_buy_ms = 0
    last_up = last_dn = 0.0
    book_ready = False
    first_done = False
    up_filled = dn_filled = 0.0
    up_cost = dn_cost = 0.0
    fees = 0.0
    lw_inj = 0
    n_buys = n_scalp = n_lw = n_burst = 0
    book_ready_ts = None  # pre_window guard için
    stop_hit = False

    for ts_ms, ub, ua, db, da in ticks:
        if ub <= 0 or db <= 0 or ua <= 0 or da <= 0:
            continue
        if not book_ready:
            book_ready = True
            book_ready_ts = ts_ms
            last_up, last_dn = ub, db
            continue
        sec_to_end = end_ts - ts_ms / 1000.0
        # Pre-window observation
        if p.pre_window_secs > 0 and book_ready_ts is not None:
            if (ts_ms - book_ready_ts) < p.pre_window_secs * 1000:
                last_up, last_dn = ub, db
                continue
        # Stop-loss guard (LW dahil ama scalp ekstra serbest değil — tüm BUY durur)
        cur_total_cost = up_cost + dn_cost
        if p.stop_loss_usdc > 0 and cur_total_cost >= p.stop_loss_usdc:
            stop_hit = True
            last_up, last_dn = ub, db
            continue

        # ── LW (ana + burst) ──
        lw_quota_ok = p.lw_max == 0 or lw_inj < p.lw_max
        if lw_quota_ok and sec_to_end > 0:
            burst_active = (
                p.lw_burst_usdc > 0
                and p.lw_burst_secs > 0
                and sec_to_end <= p.lw_burst_secs
            )
            main_active = (
                p.lw_usdc > 0
                and p.lw_secs > 0
                and sec_to_end <= p.lw_secs
                and not burst_active
            )
            usdc_lw = None
            is_burst = False
            if burst_active:
                usdc_lw = p.lw_burst_usdc
                is_burst = True
            elif main_active:
                usdc_lw = p.lw_usdc

            if usdc_lw is not None and usdc_lw > 0:
                if ub >= db:
                    wd, w_bid, w_ask = "UP", ub, ua
                else:
                    wd, w_bid, w_ask = "DOWN", db, da
                if w_bid >= p.lw_thr and w_ask > 0:
                    size = math.ceil(usdc_lw / w_ask)
                    cost_t = size * w_ask
                    # Engine make_buy reddi: notional < api_min_order_size
                    if cost_t >= api_min_order_size:
                        if wd == "UP":
                            up_filled += size
                            up_cost += cost_t
                        else:
                            dn_filled += size
                            dn_cost += cost_t
                        fees += cost_t * FEE_RATE
                        last_buy_ms = ts_ms
                        lw_inj += 1
                        first_done = True
                        if is_burst:
                            n_burst += 1
                        else:
                            n_lw += 1
                    last_up, last_dn = ub, db
                    continue

        # ── Cooldown ──
        if last_buy_ms > 0 and (ts_ms - last_buy_ms) < p.cooldown_ms:
            last_up, last_dn = ub, db
            continue

        # ── Yön seçimi ──
        if not first_done:
            spread = ub - db
            if abs(spread) < p.first_spread_min:
                last_up, last_dn = ub, db
                continue
            dir_ = "UP" if spread > 0 else "DOWN"
        else:
            imb = up_filled - dn_filled
            if abs(imb) > p.imb_thr:
                dir_ = "DOWN" if imb > 0 else "UP"
            else:
                d_up = abs(ub - last_up)
                d_dn = abs(db - last_dn)
                if d_up == 0 and d_dn == 0:
                    dir_ = "UP" if ub >= db else "DOWN"
                elif d_up >= d_dn:
                    dir_ = "UP"
                else:
                    dir_ = "DOWN"

        last_up, last_dn = ub, db
        bid = ub if dir_ == "UP" else db
        ask = ua if dir_ == "UP" else da
        if bid <= 0 or ask <= 0:
            continue

        avg_up = up_cost / up_filled if up_filled > 0 else 0
        avg_dn = dn_cost / dn_filled if dn_filled > 0 else 0
        loser = loser_side(avg_up, avg_dn, up_filled, dn_filled, ub, db)
        is_loser_dir = dir_ == loser

        effective_min = min(p.loser_min_price, MIN_PRICE) if is_loser_dir else MIN_PRICE
        if bid < effective_min or bid > MAX_PRICE:
            continue

        cur_filled = up_filled if dir_ == "UP" else dn_filled
        cur_avg = avg_up if dir_ == "UP" else avg_dn
        opp_filled = dn_filled if dir_ == "UP" else up_filled
        opp_avg = avg_dn if dir_ == "UP" else avg_up

        scalp_only = is_loser_dir and cur_filled > 0 and cur_avg > p.avg_loser_max
        is_scalp_band = (
            is_loser_dir and bid <= p.loser_scalp_max_price and p.loser_scalp_usdc > 0
        )

        if scalp_only and p.loser_scalp_usdc > 0:
            usdc = p.loser_scalp_usdc
        elif is_scalp_band:
            usdc = p.loser_scalp_usdc
        else:
            if bid <= 0.30:
                base = p.sz_long
            elif bid <= 0.85:
                base = p.sz_mid
            else:
                base = p.sz_high
            if (
                not is_loser_dir
                and p.late_pyramid_secs > 0
                and 0 < sec_to_end <= p.late_pyramid_secs
            ):
                if p.winner_factor_dynamic:
                    # Dinamik faktör: avg düşükse büyük, yüksekse küçük
                    factor = max(1.0, 5.0 - cur_avg * 5.0)
                else:
                    factor = p.winner_size_factor
                usdc = base * factor
            else:
                usdc = base

        if usdc <= 0:
            continue
        size = math.ceil(usdc / ask)
        cost_t = size * ask
        # Engine make_buy reddi: notional < api_min_order_size
        if cost_t < api_min_order_size:
            continue

        is_any_scalp = scalp_only or is_scalp_band
        if not is_any_scalp and opp_filled > 0:
            new_avg = (
                (cur_avg * cur_filled + ask * size) / (cur_filled + size)
                if cur_filled > 0
                else ask
            )
            if new_avg + opp_avg > p.max_avg_sum:
                continue

        if dir_ == "UP":
            up_filled += size
            up_cost += cost_t
        else:
            dn_filled += size
            dn_cost += cost_t
        fees += cost_t * FEE_RATE
        last_buy_ms = ts_ms
        first_done = True
        if is_any_scalp:
            n_scalp += 1
        else:
            n_buys += 1

    cost = up_cost + dn_cost
    # Canonical PnL: kazanan tarafın share'i $1 öder, kaybeden $0.
    if w == "UP":
        pnl = up_filled - cost
    else:
        pnl = dn_filled - cost

    return dict(
        sess=sess,
        winner=w,
        cost=cost,
        pnl=pnl,
        net=pnl - fees,
        fees=fees,
        upf=up_filled,
        dnf=dn_filled,
        n_buys=n_buys,
        n_scalp=n_scalp,
        n_lw=n_lw,
        n_burst=n_burst,
    )


def aggregate(con, bot_id, sessions, p: Params):
    tot_cost = tot_pnl = tot_fee = 0.0
    wins = losses = 0
    excluded = 0
    tot_buys = tot_scalp = tot_lw = tot_burst = 0
    per_sess = []
    for s in sessions:
        r = sim_session(con, bot_id, s, p)
        if r is None:
            excluded += 1
            continue
        tot_cost += r["cost"]
        tot_pnl += r["pnl"]
        tot_fee += r["fees"]
        if r["pnl"] > 0:
            wins += 1
        else:
            losses += 1
        tot_buys += r["n_buys"]
        tot_scalp += r["n_scalp"]
        tot_lw += r["n_lw"]
        tot_burst += r["n_burst"]
        per_sess.append(r)
    n = wins + losses
    return dict(
        n=n,
        excluded=excluded,
        wins=wins,
        cost=tot_cost,
        pnl=tot_pnl,
        fee=tot_fee,
        net=tot_pnl - tot_fee,
        roi=100 * (tot_pnl - tot_fee) / max(1, tot_cost),
        wr=100 * wins / max(1, n),
        n_buys=tot_buys,
        n_scalp=tot_scalp,
        n_lw=tot_lw,
        n_burst=tot_burst,
        per_sess=per_sess,
    )


def real_pnl_from_db(con, bot_id):
    """UI ile birebir total — referans karşılaştırma için."""
    row = con.execute(
        """SELECT SUM(
            CASE
              WHEN lt.up_best_bid > 0.95 THEN p.pnl_if_up
              WHEN lt.down_best_bid > 0.95 THEN p.pnl_if_down
              ELSE NULL
            END
           )
           FROM market_sessions s
           LEFT JOIN pnl_snapshots p ON p.market_session_id = s.id
              AND p.ts_ms = (SELECT MAX(ts_ms) FROM pnl_snapshots WHERE market_session_id = s.id)
           LEFT JOIN market_ticks lt ON lt.market_session_id = s.id
              AND lt.ts_ms = (SELECT MAX(ts_ms) FROM market_ticks WHERE market_session_id = s.id)
           WHERE s.bot_id = ?""",
        (bot_id,),
    ).fetchone()
    return row[0] if row[0] is not None else 0.0


# M = base güvenli kombinasyon (önceki testte kazanan)
M_BASE = {
    "lw_max": 1,
    "lw_thr": 0.95,
    "lw_burst_usdc": 0.0,
    "winner_size_factor": 2.0,
    "max_avg_sum": 1.10,
}


SCENARIOS = [
    # === Mevcut + önceki en iyi ===
    ("MEVCUT (v3 default)", {}),
    ("M: base güvenli kombinasyon", M_BASE),

    # === Tier 1 — risk yönetimi ===
    ("M + Ö1: stop_loss=$50", {**M_BASE, "stop_loss_usdc": 50.0}),
    ("M + Ö1: stop_loss=$80", {**M_BASE, "stop_loss_usdc": 80.0}),
    ("M + Ö1: stop_loss=$120", {**M_BASE, "stop_loss_usdc": 120.0}),
    ("M + Ö1: stop_loss=$200", {**M_BASE, "stop_loss_usdc": 200.0}),

    # === Tier 1 — sinyal kalitesi ===
    ("M + Ö2: spread=0.05", {**M_BASE, "first_spread_min": 0.05}),
    ("M + Ö2: spread=0.10", {**M_BASE, "first_spread_min": 0.10}),

    # === Tier 1 — loser scalp aktif ===
    ("M + Ö3: scalp=$5", {**M_BASE, "loser_scalp_usdc": 5.0}),

    # === Tier 2 — dinamik winner factor ===
    ("M + Ö5: winner_dynamic", {**M_BASE, "winner_factor_dynamic": True}),

    # === Tier 2 — pre-window observation ===
    ("M + Ö8: pre_window=15s", {**M_BASE, "pre_window_secs": 15}),
    ("M + Ö8: pre_window=30s", {**M_BASE, "pre_window_secs": 30}),

    # === Kombinasyonlar — Tier 1 paketi ===
    ("M + Ö1+Ö2+Ö3 (Tier 1 paket)",
     {**M_BASE, "stop_loss_usdc": 80.0, "first_spread_min": 0.05, "loser_scalp_usdc": 5.0}),
    ("M + Ö1+Ö2 (stop+spread)",
     {**M_BASE, "stop_loss_usdc": 80.0, "first_spread_min": 0.05}),
    ("M + Ö1+Ö8 (stop+pre_window)",
     {**M_BASE, "stop_loss_usdc": 80.0, "pre_window_secs": 15}),

    # === Süper kombo ===
    ("M + Ö1+Ö2+Ö5+Ö8 (4'lü)",
     {**M_BASE, "stop_loss_usdc": 80.0, "first_spread_min": 0.05,
      "winner_factor_dynamic": True, "pre_window_secs": 15}),
    ("M + Ö1+Ö2+Ö3+Ö8",
     {**M_BASE, "stop_loss_usdc": 80.0, "first_spread_min": 0.05,
      "loser_scalp_usdc": 5.0, "pre_window_secs": 15}),
    ("M + Ö1+Ö2+Ö3+Ö5+Ö8 (HEPSİ)",
     {**M_BASE, "stop_loss_usdc": 80.0, "first_spread_min": 0.05,
      "loser_scalp_usdc": 5.0, "winner_factor_dynamic": True, "pre_window_secs": 15}),

    # === Stop loss varyantları + spread ===
    ("Stop=$120 + spread=0.05",
     {**M_BASE, "stop_loss_usdc": 120.0, "first_spread_min": 0.05}),
    ("Stop=$120 + spread=0.05 + pre=15",
     {**M_BASE, "stop_loss_usdc": 120.0, "first_spread_min": 0.05, "pre_window_secs": 15}),
    ("Stop=$120 + spread=0.05 + scalp=$5",
     {**M_BASE, "stop_loss_usdc": 120.0, "first_spread_min": 0.05, "loser_scalp_usdc": 5.0}),
    ("Stop=$120 + spread=0.10 + pre=15",
     {**M_BASE, "stop_loss_usdc": 120.0, "first_spread_min": 0.10, "pre_window_secs": 15}),
    ("Stop=$150 + spread=0.05 + scalp=$5 + pre=15",
     {**M_BASE, "stop_loss_usdc": 150.0, "first_spread_min": 0.05,
      "loser_scalp_usdc": 5.0, "pre_window_secs": 15}),

    # === Yüksek spread + agresif winner ===
    ("spread=0.10 + winner_factor=3",
     {**M_BASE, "first_spread_min": 0.10, "winner_size_factor": 3.0}),
    ("spread=0.10 + winner_dyn",
     {**M_BASE, "first_spread_min": 0.10, "winner_factor_dynamic": True}),
    ("spread=0.10 + pre=15 + winner_dyn",
     {**M_BASE, "first_spread_min": 0.10, "pre_window_secs": 15,
      "winner_factor_dynamic": True}),
]


def main():
    ap = argparse.ArgumentParser(description="Bot scenario backtest (canonical PnL kural)")
    ap.add_argument("bot_id", type=int)
    ap.add_argument(
        "db",
        nargs="?",
        default="/home/ubuntu/baiter/data/baiter.db",
    )
    args = ap.parse_args()

    con = sqlite3.connect(args.db)
    sessions = [
        r[0] for r in con.execute(
            "SELECT id FROM market_sessions WHERE bot_id=? ORDER BY id", (args.bot_id,)
        ).fetchall()
    ]

    real = real_pnl_from_db(con, args.bot_id)
    print("=" * 100)
    print(f"BOT {args.bot_id} — {len(sessions)} session")
    print(f"DB UI gerçek toplam K/Z: ${real:+,.4f}  (referans, kural: bid > 0.95)")
    print("=" * 100)
    print()

    print(f"{'Senaryo':<50} {'WR%':>5} {'cost':>10} {'pnl':>10} "
          f"{'NET':>10} {'ROI%':>7} {'lw':>4} {'lwb':>4} {'scl':>4} {'excl':>4}")
    print("-" * 115)

    results = {}
    for label, overrides in SCENARIOS:
        p = Params(**overrides) if overrides else Params()
        r = aggregate(con, args.bot_id, sessions, p)
        results[label] = r
        print(f"{label:<50} {r['wr']:>5.1f} {r['cost']:>10,.2f} "
              f"{r['pnl']:>+10,.2f} {r['net']:>+10,.2f} {r['roi']:>+7.2f} "
              f"{r['n_lw']:>4} {r['n_burst']:>4} {r['n_scalp']:>4} {r['excluded']:>4}")

    base = results["MEVCUT (v3 default)"]
    print()
    print(f"[Mevcut'a göre fark]")
    print(f"  {'Senaryo':<50} {'NET Δ':>12} {'ROI Δ':>10}")
    for label, _ in SCENARIOS:
        r = results[label]
        d_net = r["net"] - base["net"]
        d_roi = r["roi"] - base["roi"]
        print(f"  {label:<50} {d_net:>+12,.2f} {d_roi:>+10.2f}")

    # En iyi 5 sıralama
    print()
    print("[NET TOP 5]")
    print(f"  {'#':>3} {'Senaryo':<50} {'NET':>12} {'ROI%':>7}")
    sorted_scenarios = sorted(
        ((label, results[label]) for label, _ in SCENARIOS),
        key=lambda x: x[1]["net"],
        reverse=True,
    )
    for i, (label, r) in enumerate(sorted_scenarios[:5], 1):
        marker = " ⭐" if i == 1 else ""
        print(f"  {i:>3} {label:<50} {r['net']:>+12,.2f} {r['roi']:>+7.2f}{marker}")

    print()
    print("[ROI TOP 5]")
    print(f"  {'#':>3} {'Senaryo':<50} {'ROI%':>7} {'NET':>12}")
    sorted_roi = sorted(
        ((label, results[label]) for label, _ in SCENARIOS),
        key=lambda x: x[1]["roi"],
        reverse=True,
    )
    for i, (label, r) in enumerate(sorted_roi[:5], 1):
        marker = " ⭐" if i == 1 else ""
        print(f"  {i:>3} {label:<50} {r['roi']:>+7.2f} {r['net']:>+12,.2f}{marker}")

    # Sim sonucu vs gerçek karşılaştırma
    print()
    print(f"[Doğrulama] Sim MEVCUT pnl: ${base['pnl']:+,.4f}  vs DB gerçek: ${real:+,.4f}")
    diff = base['pnl'] - real
    print(f"  Fark: ${diff:+,.4f}  ({100*diff/abs(real or 1):+.2f}%)")
    if abs(diff) < 50:
        print("  ✅ Yakın eşleşme — sim güvenilir")
    else:
        print("  ⚠️  Belirgin fark — sim ile gerçek davranış arasında sapma var")
        print("      (sub-second WS event'leri, api_min_order_size, vs. fark yaratır)")


if __name__ == "__main__":
    main()
