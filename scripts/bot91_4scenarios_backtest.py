#!/usr/bin/env python3
"""Bot 91 — 4 strateji seçeneği karşılaştırmalı backtest.

Senaryolar:
  A) Sub-second BID Arb: bid_winner + bid_loser < cost_max
  B) Sub-second ASK Taker: ask_winner + ask_loser < cost_max (taker, %100 fill)
  C) Binance Latency: BTC delta sinyali → direction BUY (taker @ ask)
  D) Hybrid: Sinyal güçlü → Binance latency, zayıf → Arb (BID)

Her senaryo için: NET, ROI, WR, trade count.
PnL canonical kuralı: bid > 0.95.
"""
import argparse
import math
import sqlite3
import sys
import time
import json
from urllib.request import urlopen, Request
from collections import defaultdict


BINANCE_URL = "https://api.binance.com/api/v3/klines"
FEE_RATE = 0.02
MIN_PRICE = 0.10
MAX_PRICE = 0.95


def fetch_btc(start_ms, end_ms, interval="1s"):
    klines = []
    cur = start_ms
    n = 0
    while cur < end_ms:
        url = (f"{BINANCE_URL}?symbol=BTCUSDT&interval={interval}"
               f"&startTime={cur}&endTime={end_ms}&limit=1000")
        try:
            req = Request(url, headers={"User-Agent": "Mozilla/5.0"})
            with urlopen(req, timeout=10) as r:
                d = json.loads(r.read().decode())
        except Exception as e:
            print(f"  [warn] {e}", file=sys.stderr)
            time.sleep(1)
            continue
        if not d:
            break
        klines.extend(d)
        cur = int(d[-1][6]) + 1
        n += 1
        if n % 30 == 0:
            print(f"  Binance batch {n}, {len(klines)} kline...", file=sys.stderr)
        time.sleep(0.05)
        if len(d) < 1000:
            break
    return {int(k[0]) // 1000: float(k[4]) for k in klines}


def get_btc(lk, ts, drift=5):
    for d in range(drift):
        if ts - d in lk: return lk[ts - d]
        if ts + d in lk: return lk[ts + d]
    return None


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


def sim_session_a(con, bot_id, sess, btc_lk, cost_max=0.95, order=20,
                  max_trades=5, cooldown_s=5, fill_rate=0.74):
    """Sub-second BID Arb (mevcut Arbitrage stratejisi)."""
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
    for ts_ms, ub, ua, db, da in ticks:
        if not all(x and x > 0 for x in (ub, ua, db, da)): continue
        ts_sec = ts_ms // 1000
        if end_ts - ts_sec <= 0: break
        if n_t >= max_trades: break
        if ts_sec - last_t < cooldown_s: continue
        if ub > 0.5 and db <= 0.5:
            w_bid, l_bid = ub, db
        elif db > 0.5 and ub <= 0.5:
            w_bid, l_bid = db, ub
        else:
            continue
        cp = w_bid + l_bid
        if cp >= cost_max: continue
        size = math.ceil(order / cp)
        actual = size * fill_rate
        c = actual * cp
        if c < 5: continue
        cost += c
        pnl += actual * 1.0 - c
        fees += c * FEE_RATE
        n_t += 1
        last_t = ts_sec
    return dict(cost=cost, pnl=pnl, fees=fees, n_trades=n_t, w=w)


def sim_session_b(con, bot_id, sess, btc_lk, cost_max=1.00, order=20,
                  max_trades=5, cooldown_s=5):
    """Sub-second ASK Taker (her iki tarafa ASK BUY, %100 fill)."""
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
    for ts_ms, ub, ua, db, da in ticks:
        if not all(x and x > 0 for x in (ub, ua, db, da)): continue
        ts_sec = ts_ms // 1000
        if end_ts - ts_sec <= 0: break
        if n_t >= max_trades: break
        if ts_sec - last_t < cooldown_s: continue
        cp = ua + da
        if cp >= cost_max: continue
        size = math.ceil(order / cp)
        c = size * cp
        if c < 5: continue
        cost += c
        pnl += size * 1.0 - c
        fees += c * FEE_RATE
        n_t += 1
        last_t = ts_sec
    return dict(cost=cost, pnl=pnl, fees=fees, n_trades=n_t, w=w)


def sim_session_c(con, bot_id, sess, btc_lk, sig_thr=10, t_window=300, order=100):
    """Binance Latency Direction (taker @ ask)."""
    w = winner_of(con, bot_id, sess)
    if w is None: return None
    sm = con.execute("SELECT start_ts, end_ts FROM market_sessions WHERE id=?", (sess,)).fetchone()
    start_ts, end_ts = sm[0], sm[1]
    btc_open = get_btc(btc_lk, start_ts)
    if btc_open is None: return None
    ticks = con.execute(
        "SELECT ts_ms, up_best_bid, up_best_ask, down_best_bid, down_best_ask "
        "FROM market_ticks WHERE bot_id=? AND market_session_id=? ORDER BY ts_ms",
        (bot_id, sess),
    ).fetchall()
    for ts_ms, ub, ua, db, da in ticks:
        if not all(x and x > 0 for x in (ub, ua, db, da)): continue
        ts_sec = ts_ms // 1000
        sec_to_end = end_ts - ts_sec
        if sec_to_end <= 0 or sec_to_end > t_window: continue
        btc_now = get_btc(btc_lk, ts_sec)
        if btc_now is None: continue
        delta = btc_now - btc_open
        if abs(delta) < sig_thr: continue
        sig_dir = "UP" if delta > 0 else "DOWN"
        ask = ua if sig_dir == "UP" else da
        bid = ub if sig_dir == "UP" else db
        if ask <= 0 or bid < MIN_PRICE or bid > MAX_PRICE or ask >= 0.99: continue
        size = math.ceil(order / ask)
        c = size * ask
        if c < 5: continue
        if sig_dir == w:
            p = size * 1.0 - c
        else:
            p = -c
        return dict(cost=c, pnl=p, fees=c * FEE_RATE, n_trades=1, w=w)
    return dict(cost=0, pnl=0, fees=0, n_trades=0, w=w)


def sim_session_d(con, bot_id, sess, btc_lk, sig_thr=30,
                  cost_max=0.95, t_window=300, order=100, fill_rate=0.74):
    """Hybrid: güçlü sinyal → Binance latency, zayıf → BID arb."""
    w = winner_of(con, bot_id, sess)
    if w is None: return None
    sm = con.execute("SELECT start_ts, end_ts FROM market_sessions WHERE id=?", (sess,)).fetchone()
    start_ts, end_ts = sm[0], sm[1]
    btc_open = get_btc(btc_lk, start_ts)
    if btc_open is None: return None
    ticks = con.execute(
        "SELECT ts_ms, up_best_bid, up_best_ask, down_best_bid, down_best_ask "
        "FROM market_ticks WHERE bot_id=? AND market_session_id=? ORDER BY ts_ms",
        (bot_id, sess),
    ).fetchall()
    for ts_ms, ub, ua, db, da in ticks:
        if not all(x and x > 0 for x in (ub, ua, db, da)): continue
        ts_sec = ts_ms // 1000
        sec_to_end = end_ts - ts_sec
        if sec_to_end <= 0 or sec_to_end > t_window: continue
        btc_now = get_btc(btc_lk, ts_sec)
        if btc_now is None: continue
        delta = btc_now - btc_open
        sig_strong = abs(delta) >= sig_thr
        if sig_strong:
            # Directional
            sig_dir = "UP" if delta > 0 else "DOWN"
            ask = ua if sig_dir == "UP" else da
            bid = ub if sig_dir == "UP" else db
            if ask <= 0 or bid < MIN_PRICE or bid > MAX_PRICE or ask >= 0.99: continue
            size = math.ceil(order / ask)
            c = size * ask
            if c < 5: continue
            if sig_dir == w:
                p = size * 1.0 - c
            else:
                p = -c
            return dict(cost=c, pnl=p, fees=c * FEE_RATE, n_trades=1, w=w)
        else:
            # Arb fırsatı
            if ub > 0.5 and db <= 0.5:
                w_bid, l_bid = ub, db
            elif db > 0.5 and ub <= 0.5:
                w_bid, l_bid = db, ub
            else:
                continue
            cp = w_bid + l_bid
            if cp >= cost_max: continue
            size = math.ceil(order / cp)
            actual = size * fill_rate
            c = actual * cp
            if c < 5: continue
            return dict(cost=c, pnl=actual * 1.0 - c, fees=c * FEE_RATE, n_trades=1, w=w)
    return dict(cost=0, pnl=0, fees=0, n_trades=0, w=w)


def aggregate(con, bot_id, sessions, btc_lk, sim_fn, **kwargs):
    triggered = wins = losses = 0
    cost = pnl = fees = 0.0
    n_t = 0
    for s in sessions:
        r = sim_fn(con, bot_id, s, btc_lk, **kwargs)
        if r is None or r["n_trades"] == 0: continue
        triggered += 1
        cost += r["cost"]; pnl += r["pnl"]; fees += r["fees"]; n_t += r["n_trades"]
        if r["pnl"] > r["fees"]: wins += 1
        else: losses += 1
    n = wins + losses
    return dict(
        triggered=triggered, wins=wins, losses=losses, n=n,
        cost=cost, pnl=pnl, fees=fees, net=pnl - fees,
        roi=100 * (pnl - fees) / max(1, cost),
        wr=100 * wins / max(1, n),
        n_trades=n_t,
    )


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("bot_id", type=int)
    ap.add_argument("db", nargs="?", default="/home/ubuntu/baiter/data/baiter.db")
    args = ap.parse_args()

    con = sqlite3.connect(args.db)
    sessions = [r[0] for r in con.execute(
        "SELECT id FROM market_sessions WHERE bot_id=? ORDER BY id", (args.bot_id,)
    ).fetchall()]
    span = con.execute(
        "SELECT MIN(start_ts), MAX(end_ts) FROM market_sessions WHERE bot_id=?",
        (args.bot_id,),
    ).fetchone()
    span_start, span_end = span
    span_h = (span_end - span_start) / 3600

    print("=" * 110)
    print(f"BOT {args.bot_id} | {len(sessions)} session | {span_h:.1f}h | 4-Senaryo Backtest")
    print("=" * 110)
    print(f"\nBinance API çekiliyor (~{int(span_h * 3600 / 1000)} batch)...")
    btc_lk = fetch_btc(span_start * 1000, span_end * 1000)
    print(f"  Toplam {len(btc_lk)} unique sec\n")

    # SENARYOLAR
    print(f"  {'Senaryo':<55} {'trig':>5} {'WR%':>5} {'cost':>11} "
          f"{'NET':>10} {'ROI%':>6} {'trades':>6}")
    print("  " + "-" * 110)

    sc = [
        # A senaryoları
        ("A1: BID arb $20 cost<0.95 mt=5",
         sim_session_a, dict(cost_max=0.95, order=20, max_trades=5)),
        ("A2: BID arb $100 cost<0.95 mt=5",
         sim_session_a, dict(cost_max=0.95, order=100, max_trades=5)),
        ("A3: BID arb $100 cost<0.97 mt=5",
         sim_session_a, dict(cost_max=0.97, order=100, max_trades=5)),
        # B senaryoları
        ("B1: ASK taker $20 cost<1.00 mt=5",
         sim_session_b, dict(cost_max=1.00, order=20, max_trades=5)),
        ("B2: ASK taker $20 cost<1.01 mt=5",
         sim_session_b, dict(cost_max=1.01, order=20, max_trades=5)),
        ("B3: ASK taker $100 cost<1.01 mt=5",
         sim_session_b, dict(cost_max=1.01, order=100, max_trades=5)),
        # C Binance latency
        ("C1: Binance lat sig>$10 T-300 $20",
         sim_session_c, dict(sig_thr=10, t_window=300, order=20)),
        ("C2: Binance lat sig>$10 T-15 $100",
         sim_session_c, dict(sig_thr=10, t_window=15, order=100)),
        ("C3: Binance lat sig>$10 T-300 $100",
         sim_session_c, dict(sig_thr=10, t_window=300, order=100)),
        ("C4: Binance lat sig>$30 T-60 $100",
         sim_session_c, dict(sig_thr=30, t_window=60, order=100)),
        # D Hybrid
        ("D1: Hybrid sig>$30 cost<0.95 $100",
         sim_session_d, dict(sig_thr=30, cost_max=0.95, t_window=300, order=100)),
        ("D2: Hybrid sig>$50 cost<0.95 $100",
         sim_session_d, dict(sig_thr=50, cost_max=0.95, t_window=300, order=100)),
    ]

    results = []
    for label, fn, kw in sc:
        r = aggregate(con, args.bot_id, sessions, btc_lk, fn, **kw)
        results.append((label, r))
        print(f"  {label:<55} {r['triggered']:>5} {r['wr']:>5.1f} {r['cost']:>11,.2f} "
              f"{r['net']:>+10,.2f} {r['roi']:>+6.2f} {r['n_trades']:>6}")

    # Top NET
    print(f"\n[NET TOP 5]")
    for i, (l, r) in enumerate(sorted(results, key=lambda x: x[1]["net"], reverse=True)[:5], 1):
        m = " ⭐" if i == 1 else ""
        # Yıllık tahmini
        yearly_factor = 8760 / span_h
        yearly_net = r["net"] * yearly_factor
        print(f"  {i}. {l:<55} NET=${r['net']:+8.2f} ROI={r['roi']:+.2f}% "
              f"yıllık~${yearly_net:>+10,.0f}{m}")

    # Top ROI
    print(f"\n[ROI TOP 5 (>=30 trigger)]")
    valid = [(l, r) for l, r in results if r["triggered"] >= 30]
    for i, (l, r) in enumerate(sorted(valid, key=lambda x: x[1]["roi"], reverse=True)[:5], 1):
        m = " ⭐" if i == 1 else ""
        print(f"  {i}. {l:<55} ROI={r['roi']:+6.2f}% NET=${r['net']:+8.2f} "
              f"trig={r['triggered']} WR={r['wr']:.1f}%{m}")


if __name__ == "__main__":
    main()
