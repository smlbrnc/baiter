#!/usr/bin/env python3
"""Bot 108 — Grand parametric search.

5 boyutlu grid:
  1. Strategy mode: arb_only, dir_only, hybrid, hybrid_t15
  2. Signal threshold (BTC delta $): 5, 10, 15, 20, 30, 50
  3. Cost threshold (FAK arb max): 0.97, 0.98, 0.985, 0.99
  4. Order size USDC: 20, 50, 100
  5. T-window for directional: 15, 30, 60, 120, 300

Toplam ~720 senaryo. NET ve ROI top 20 + Sharpe-like metrik (Risk-adjusted).
"""
import argparse
import math
import sqlite3
import sys
import time
import json
from urllib.request import urlopen, Request
from urllib.error import URLError, HTTPError
from collections import defaultdict
from datetime import datetime, timezone

BINANCE_URL = "https://api.binance.com/api/v3/klines"
FEE_RATE = 0.02
MIN_PRICE = 0.10
MAX_PRICE = 0.95


def fetch_btc(start_ms, end_ms):
    klines = []
    cur = start_ms
    while cur < end_ms:
        url = (f"{BINANCE_URL}?symbol=BTCUSDT&interval=1s"
               f"&startTime={cur}&endTime={end_ms}&limit=1000")
        try:
            req = Request(url, headers={"User-Agent": "Mozilla/5.0"})
            with urlopen(req, timeout=10) as r:
                d = json.loads(r.read().decode())
        except Exception:
            time.sleep(1)
            continue
        if not d:
            break
        klines.extend(d)
        cur = int(d[-1][6]) + 1
        time.sleep(0.05)
        if len(d) < 1000:
            break
    return {int(k[0]) // 1000: float(k[4]) for k in klines}


def get_btc(lk, ts, drift=5):
    for d in range(drift):
        if ts - d in lk:
            return lk[ts - d]
        if ts + d in lk:
            return lk[ts + d]
    return None


def winner_of(con, bot_id, sess):
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
    return None


def sim_session(con, bot_id, sess, btc_lk, mode,
                fak_cost_max, sig_thr, order_usdc, t_window_dir,
                fill_rate=0.74, multi_arb=False):
    """Tek session simulasyonu — birleşik strateji."""
    w = winner_of(con, bot_id, sess)
    if w is None:
        return None
    sm = con.execute(
        "SELECT start_ts, end_ts FROM market_sessions WHERE id=?", (sess,)
    ).fetchone()
    start_ts, end_ts = sm[0], sm[1]
    btc_open = get_btc(btc_lk, start_ts)
    if btc_open is None:
        return None

    ticks = con.execute(
        "SELECT ts_ms, up_best_bid, up_best_ask, down_best_bid, down_best_ask "
        "FROM market_ticks WHERE bot_id=? AND market_session_id=? ORDER BY ts_ms",
        (bot_id, sess),
    ).fetchall()

    triggered = False
    cost = pnl = fees = 0.0
    n_arb = n_dir = 0
    arb_done = False  # multi_arb=False için

    for ts_ms, ub, ua, db, da in ticks:
        if not all(x and x > 0 for x in (ub, ua, db, da)):
            continue
        ts_sec = ts_ms // 1000
        sec_to_end = end_ts - ts_sec
        if sec_to_end <= 0:
            break
        if triggered and not multi_arb:
            break

        btc_now = get_btc(btc_lk, ts_sec)
        if btc_now is None:
            continue
        delta = btc_now - btc_open
        sig_dir = "UP" if delta > 0 else "DOWN"
        sig_strong = abs(delta) >= sig_thr

        # Winner side
        if ub > 0.5 and db <= 0.5:
            w_side, w_bid, l_bid, w_ask, l_ask = "UP", ub, db, ua, da
        elif db > 0.5 and ub <= 0.5:
            w_side, w_bid, l_bid, w_ask, l_ask = "DOWN", db, ub, da, ua
        else:
            continue

        cost_per = w_bid + l_bid
        fak_eligible = cost_per < fak_cost_max

        # === Strateji kararı ===
        do_arb = False
        do_dir = False

        if mode == "arb_only":
            do_arb = fak_eligible
        elif mode == "dir_only":
            if sig_strong and sec_to_end <= t_window_dir:
                do_dir = True
        elif mode == "hybrid":
            # Güçlü sinyal + T-window içinde → directional
            # Aksi halde arb fırsatı varsa arb
            if sig_strong and sec_to_end <= t_window_dir:
                do_dir = True
            elif fak_eligible:
                do_arb = True
        elif mode == "hybrid_strict":
            # Güçlü sinyal varsa SADECE directional, arb yok
            # Sinyal yoksa SADECE arb
            if sig_strong:
                if sec_to_end <= t_window_dir:
                    do_dir = True
                # T-window dışında ama sinyal varsa pas
            else:
                if fak_eligible:
                    do_arb = True

        if do_dir:
            ask = ua if sig_dir == "UP" else da
            bid = ub if sig_dir == "UP" else db
            if ask <= 0 or bid < MIN_PRICE or bid > MAX_PRICE or ask >= 0.99:
                continue
            size = math.ceil(order_usdc / ask)
            cost_t = size * ask
            if cost_t < 5:
                continue
            fees_t = cost_t * FEE_RATE
            if sig_dir == w:
                pnl_t = size * 1.0 - cost_t
            else:
                pnl_t = -cost_t
            cost += cost_t
            fees += fees_t
            pnl += pnl_t
            n_dir += 1
            triggered = True
            if not multi_arb:
                break

        if do_arb and (not arb_done or multi_arb):
            size = math.ceil(order_usdc / cost_per)
            actual = size * fill_rate
            cost_t = actual * cost_per
            if cost_t < 5:
                continue
            fees_t = cost_t * FEE_RATE
            gross = actual * 1.0
            pnl_t = gross - cost_t - fees_t
            cost += cost_t
            fees += fees_t
            pnl += pnl_t
            n_arb += 1
            triggered = True
            arb_done = True
            if not multi_arb:
                break

    return dict(
        sess=sess, w=w, triggered=triggered,
        cost=cost, pnl=pnl, fees=fees,
        n_arb=n_arb, n_dir=n_dir,
    )


def aggregate(con, bot_id, sessions, btc_lk, params):
    triggered = 0
    wins = losses = 0
    cost = pnl = fees = 0.0
    n_arb = n_dir = 0
    pnls = []
    for s in sessions:
        r = sim_session(con, bot_id, s, btc_lk, **params)
        if r is None or not r["triggered"]:
            continue
        triggered += 1
        cost += r["cost"]
        pnl += r["pnl"]
        fees += r["fees"]
        n_arb += r["n_arb"]
        n_dir += r["n_dir"]
        pnls.append(r["pnl"] - r["fees"])
        if r["pnl"] > r["fees"]:
            wins += 1
        else:
            losses += 1
    n = wins + losses
    net = pnl - fees
    # Sharpe-like: avg_pnl / stddev
    if pnls:
        avg = sum(pnls) / len(pnls)
        var = sum((p - avg) ** 2 for p in pnls) / len(pnls)
        std = var ** 0.5
        sharpe = (avg / std) if std > 0 else 0
    else:
        sharpe = 0
    return dict(
        triggered=triggered, wins=wins, losses=losses, n=n,
        cost=cost, pnl=pnl, fees=fees, net=net,
        roi=100 * net / max(1, cost),
        wr=100 * wins / max(1, n),
        sharpe=sharpe, n_arb=n_arb, n_dir=n_dir,
    )


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("bot_id", type=int)
    ap.add_argument("db", nargs="?", default="/home/ubuntu/baiter/data/baiter.db")
    args = ap.parse_args()

    con = sqlite3.connect(args.db)
    sessions = [
        r[0] for r in con.execute(
            "SELECT id FROM market_sessions WHERE bot_id=? ORDER BY id", (args.bot_id,)
        ).fetchall()
    ]
    span = con.execute(
        "SELECT MIN(start_ts), MAX(end_ts) FROM market_sessions WHERE bot_id=?",
        (args.bot_id,),
    ).fetchone()
    span_start, span_end = span

    print("=" * 110)
    print(f"BOT {args.bot_id} — {len(sessions)} session, "
          f"{(span_end-span_start)/3600:.1f}h | GRAND PARAMETRIC SEARCH")
    print("=" * 110)

    print("\nBinance API çekiliyor...")
    btc_lk = fetch_btc(span_start * 1000, span_end * 1000)
    print(f"  {len(btc_lk)} kline\n")

    if not btc_lk:
        return

    # === GRID ===
    modes = ["arb_only", "dir_only", "hybrid", "hybrid_strict"]
    sig_thrs = [5, 10, 15, 20, 30, 50]
    fak_costs = [0.97, 0.98, 0.985, 0.99]
    orders = [20, 50, 100]
    t_windows = [15, 30, 60, 120, 300]

    all_results = []
    total_combos = 0

    print("Grid taraması başlıyor...")
    for mode in modes:
        for sig_thr in sig_thrs:
            for fak_cost in fak_costs:
                for ord_usdc in orders:
                    for t_win in t_windows:
                        # arb_only için sig_thr ve t_win etkisiz
                        if mode == "arb_only" and (sig_thr != sig_thrs[0] or t_win != t_windows[0]):
                            continue
                        # dir_only için fak_cost etkisiz
                        if mode == "dir_only" and fak_cost != fak_costs[0]:
                            continue
                        params = dict(
                            mode=mode, fak_cost_max=fak_cost, sig_thr=sig_thr,
                            order_usdc=ord_usdc, t_window_dir=t_win,
                            fill_rate=0.74,
                        )
                        r = aggregate(con, args.bot_id, sessions, btc_lk, params)
                        label = (f"{mode:<14} sig=${sig_thr:<3} cost<{fak_cost} "
                                 f"$={ord_usdc:<3} T={t_win:<3}")
                        all_results.append((label, r, params))
                        total_combos += 1

    print(f"  Toplam {total_combos} senaryo test edildi\n")

    # En iyi NET (>= 30 trigger)
    valid = [(l, r, p) for l, r, p in all_results if r["triggered"] >= 30]
    print(f"[NET TOP 20] (>= 30 trigger)")
    print(f"  {'Senaryo':<55} {'trig':>4} {'WR%':>5} {'cost':>9} "
          f"{'NET':>9} {'ROI%':>6} {'Sharpe':>7}")
    print("  " + "-" * 105)
    sorted_n = sorted(valid, key=lambda x: x[1]["net"], reverse=True)
    for i, (l, r, _) in enumerate(sorted_n[:20], 1):
        marker = " ⭐" if i == 1 else ""
        print(f"  {l:<55} {r['triggered']:>4} {r['wr']:>5.1f} {r['cost']:>9,.2f} "
              f"{r['net']:>+9,.2f} {r['roi']:>+6.2f} {r['sharpe']:>7.3f}{marker}")

    # En iyi ROI
    print(f"\n[ROI TOP 20] (>= 30 trigger)")
    sorted_roi = sorted(valid, key=lambda x: x[1]["roi"], reverse=True)
    for i, (l, r, _) in enumerate(sorted_roi[:20], 1):
        marker = " ⭐" if i == 1 else ""
        print(f"  {l:<55} ROI={r['roi']:>+6.2f}% NET=${r['net']:>+8.2f} "
              f"trig={r['triggered']:>3} WR={r['wr']:.1f}% sharpe={r['sharpe']:.3f}{marker}")

    # Sharpe (risk-adjusted)
    print(f"\n[SHARPE TOP 10] (>= 30 trigger, NET pozitif)")
    valid_pos = [(l, r, p) for l, r, p in valid if r["net"] > 0]
    sorted_sharpe = sorted(valid_pos, key=lambda x: x[1]["sharpe"], reverse=True)
    for i, (l, r, _) in enumerate(sorted_sharpe[:10], 1):
        marker = " ⭐" if i == 1 else ""
        print(f"  {l:<55} Sharpe={r['sharpe']:>6.3f} NET=${r['net']:>+8.2f} "
              f"ROI={r['roi']:+.2f}%{marker}")

    # En iyi senaryo için detay
    if sorted_n:
        best_label, best_r, best_params = sorted_n[0]
        print(f"\n{'=' * 110}")
        print(f"EN İYİ NET SENARYO DETAY: {best_label}")
        print(f"{'=' * 110}")
        print(f"\n  Params: {best_params}")
        print(f"\n  Triggers:           {best_r['triggered']}/{len(sessions)}")
        print(f"  Wins / Losses:      {best_r['wins']} / {best_r['losses']}")
        print(f"  Winrate:            {best_r['wr']:.1f}%")
        print(f"  Trade kırılımı:     arb={best_r['n_arb']}, directional={best_r['n_dir']}")
        print(f"  Total cost:         ${best_r['cost']:,.2f}")
        print(f"  Total PnL:          ${best_r['pnl']:+,.2f}")
        print(f"  Fee:                ${best_r['fees']:.2f}")
        print(f"  NET:                ${best_r['net']:+,.2f}")
        print(f"  ROI:                {best_r['roi']:+.2f}%")
        print(f"  Sharpe-like:        {best_r['sharpe']:.3f}")
        # Yıllık tahmini
        span_h = (span_end - span_start) / 3600
        yearly_factor = 8760 / span_h
        print(f"\n  Yıllık tahmini NET: ${best_r['net'] * yearly_factor:,.0f}")
        print(f"  Yıllık tahmini cost: ${best_r['cost'] * yearly_factor:,.0f}")


if __name__ == "__main__":
    main()
