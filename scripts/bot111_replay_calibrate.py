#!/usr/bin/env python3
"""Bot 111 — sim ile gerçek arasında kalibrasyon.

Bot 111'i gerçek tick verisinde simüle et, NET'i -$25.47 ile karşılaştır.
fill_rate parametresini değiştirerek doğru değeri bul.

İki leg ayrı ayrı modellenecek:
  - p_karma = fill_rate ^ 2 (her iki leg fill → KARMA pozisyon, $1 garanti payoff)
  - p_saf = 2 * fr * (1-fr) (sadece bir leg fill → SAF pozisyon, directional risk)
  - p_no_fill = (1-fr)^2 (hiç fill yok)

PnL canonical: bid > 0.95 (UI ile aynı kural).
"""
import math
import sqlite3
import sys
import argparse
import random


FEE_RATE = 0.02
MIN_PRICE = 0.10
MAX_PRICE = 0.95


def winner_of(con, bot_id, sess):
    r = con.execute(
        "SELECT up_best_bid, down_best_bid FROM market_ticks "
        "WHERE bot_id=? AND market_session_id=? ORDER BY ts_ms DESC LIMIT 1",
        (bot_id, sess),
    ).fetchone()
    if not r or r[0] is None: return None
    ub, db = r[0] or 0.0, r[1] or 0.0
    if ub > 0.95: return "UP"
    if db > 0.95: return "DOWN"
    return None


def sim_session(con, bot_id, sess, cost_max, order, max_trades, cooldown_s, fill_rate, rng):
    """Doğru iki-leg fill modeli ile simulasyon."""
    w = winner_of(con, bot_id, sess)
    if w is None: return None
    sm = con.execute("SELECT end_ts FROM market_sessions WHERE id=?", (sess,)).fetchone()
    end_ts = sm[0]
    ticks = con.execute(
        "SELECT ts_ms, up_best_bid, up_best_ask, down_best_bid, down_best_ask "
        "FROM market_ticks WHERE bot_id=? AND market_session_id=? ORDER BY ts_ms",
        (bot_id, sess),
    ).fetchall()

    n_t = 0
    cost = pnl = fees = 0.0
    last_t = 0
    karma = saf = no_fill = 0

    for ts_ms, ub, ua, db, da in ticks:
        if not all(x and x > 0 for x in (ub, ua, db, da)): continue
        ts_sec = ts_ms // 1000
        if end_ts - ts_sec <= 0: break
        if n_t >= max_trades: break
        if ts_sec - last_t < cooldown_s: continue
        if ub > 0.5 and db <= 0.5:
            w_dir = "UP"; w_bid, l_bid = ub, db
        elif db > 0.5 and ub <= 0.5:
            w_dir = "DOWN"; w_bid, l_bid = db, ub
        else:
            continue
        cp = w_bid + l_bid
        if cp >= cost_max: continue
        size = math.ceil(order / cp)
        if size * w_bid < 5 or size * l_bid < 5: continue

        # İki leg ayrı fill
        winner_filled = rng.random() < fill_rate
        loser_filled = rng.random() < fill_rate

        if winner_filled and loser_filled:
            # KARMA: $1 garanti payoff
            c = size * w_bid + size * l_bid
            cost += c
            pnl += size * 1.0 - c  # winner share = $1, loser = $0
            fees += c * FEE_RATE
            karma += 1
        elif winner_filled:
            # SAF winner side
            c = size * w_bid
            cost += c
            if w_dir == w:  # bid yüksek olan winner = gerçek winner mi?
                pnl += size * 1.0 - c
            else:
                pnl -= c
            fees += c * FEE_RATE
            saf += 1
        elif loser_filled:
            # SAF loser side (loser bid post fill)
            c = size * l_bid
            cost += c
            l_dir = "DOWN" if w_dir == "UP" else "UP"
            if l_dir == w:
                pnl += size * 1.0 - c
            else:
                pnl -= c
            fees += c * FEE_RATE
            saf += 1
        else:
            no_fill += 1
            continue
        n_t += 1
        last_t = ts_sec
    return dict(cost=cost, pnl=pnl, fees=fees, n=n_t, w=w,
                karma=karma, saf=saf, no_fill=no_fill)


def aggregate(con, bot_id, sessions, fill_rate, n_runs=5):
    """N kez Monte Carlo, ortalama döndür."""
    rng = random.Random(42)
    nets = []
    rois = []
    karmas_total = []
    saf_total = []
    cost_total = []
    for _ in range(n_runs):
        cost = pnl = fees = 0.0
        karma = saf = no_fill = 0
        for s in sessions:
            r = sim_session(con, bot_id, s, 0.95, 20, 5, 5, fill_rate, rng)
            if r is None: continue
            cost += r["cost"]; pnl += r["pnl"]; fees += r["fees"]
            karma += r["karma"]; saf += r["saf"]; no_fill += r["no_fill"]
        net = pnl - fees
        roi = 100 * net / max(1, cost)
        nets.append(net); rois.append(roi)
        karmas_total.append(karma); saf_total.append(saf); cost_total.append(cost)
    return dict(
        net_avg=sum(nets) / len(nets), net_min=min(nets), net_max=max(nets),
        roi_avg=sum(rois) / len(rois),
        karma_avg=sum(karmas_total) / len(karmas_total),
        saf_avg=sum(saf_total) / len(saf_total),
        cost_avg=sum(cost_total) / len(cost_total),
    )


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("bot_id", type=int)
    ap.add_argument("expected_net", type=float, default=-25.47)
    ap.add_argument("db", nargs="?", default="/home/ubuntu/baiter/data/baiter.db")
    args = ap.parse_args()

    con = sqlite3.connect(args.db)
    sessions = [r[0] for r in con.execute(
        "SELECT id FROM market_sessions WHERE bot_id=? ORDER BY id", (args.bot_id,)
    ).fetchall()]
    print(f"BOT {args.bot_id} | {len(sessions)} session | Fill rate kalibrasyonu")
    print(f"  Beklenen NET (canlı): ${args.expected_net}")
    print()
    print(f"  {'fill_rate':>10} | {'NET (avg)':>10} {'NET min':>10} {'NET max':>10} "
          f"{'ROI%':>6} {'KARMA':>6} {'SAF':>6} {'cost':>9}")
    print("  " + "-" * 90)
    for fr in [0.20, 0.30, 0.40, 0.50, 0.60, 0.70, 0.74, 0.80, 0.89, 0.95, 1.00]:
        r = aggregate(con, args.bot_id, sessions, fr, n_runs=10)
        marker = ""
        if abs(r["net_avg"] - args.expected_net) < 50:
            marker = " ← yakın eşleşme"
        print(f"  fr={fr:.2f}    | {r['net_avg']:>+10.2f} {r['net_min']:>+10.2f} "
              f"{r['net_max']:>+10.2f} {r['roi_avg']:>+6.2f} "
              f"{r['karma_avg']:>6.1f} {r['saf_avg']:>6.1f} {r['cost_avg']:>9.2f}{marker}")


if __name__ == "__main__":
    main()
