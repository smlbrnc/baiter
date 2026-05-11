#!/usr/bin/env python3
"""Bot 91 — Binance Latency C2/C3 strategy avg_sum dağılımı.

Her session sonu için pozisyon analizi:
  - Tek tarafa giriş (SAF) → avg_sum = avg_one_side
  - İki tarafa giriş (KARMA) → avg_sum = avg_up + avg_dn
  - avg_sum < 1.0 garantili mi?
"""
import math
import sqlite3
import sys
import time
import json
import argparse
from urllib.request import urlopen, Request
from collections import defaultdict


BINANCE_URL = "https://api.binance.com/api/v3/klines"
FEE_RATE = 0.02
MIN_PRICE = 0.10
MAX_PRICE = 0.95


def fetch_btc(start_ms, end_ms):
    klines = []
    cur = start_ms
    n = 0
    while cur < end_ms:
        url = (f"{BINANCE_URL}?symbol=BTCUSDT&interval=1s"
               f"&startTime={cur}&endTime={end_ms}&limit=1000")
        try:
            req = Request(url, headers={"User-Agent": "Mozilla/5.0"})
            with urlopen(req, timeout=10) as r:
                d = json.loads(r.read().decode())
        except Exception:
            time.sleep(1); continue
        if not d: break
        klines.extend(d)
        cur = int(d[-1][6]) + 1
        n += 1
        if n % 50 == 0:
            print(f"  {n} batch...", file=sys.stderr)
        time.sleep(0.05)
        if len(d) < 1000: break
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


def sim_session(con, bot_id, sess, btc_lk, sig_thr, t_window, order, cooldown_s, max_trades):
    """Multi-trade Binance latency. Pozisyon detayını döndür."""
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

    n_t = 0
    cost = pnl = 0.0
    last_t = 0
    up_filled = dn_filled = 0.0
    up_cost = dn_cost = 0.0
    trades = []  # (dir, price, size)
    for ts_ms, ub, ua, db, da in ticks:
        if not all(x and x > 0 for x in (ub, ua, db, da)): continue
        ts_sec = ts_ms // 1000
        sec_to_end = end_ts - ts_sec
        if sec_to_end <= 0: break
        if sec_to_end > t_window: continue
        if n_t >= max_trades: break
        if ts_sec - last_t < cooldown_s: continue
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
        if sig_dir == "UP":
            up_filled += size; up_cost += c
        else:
            dn_filled += size; dn_cost += c
        cost += c
        if sig_dir == w:
            pnl += size * 1.0 - c
        else:
            pnl -= c
        trades.append((sig_dir, ask, size))
        n_t += 1
        last_t = ts_sec

    avg_up = up_cost / up_filled if up_filled > 0 else 0.0
    avg_dn = dn_cost / dn_filled if dn_filled > 0 else 0.0
    avg_sum = avg_up + avg_dn
    is_karma = up_filled > 0 and dn_filled > 0
    return dict(
        sess=sess, w=w, n_trades=n_t, cost=cost, pnl=pnl,
        up_filled=up_filled, dn_filled=dn_filled,
        avg_up=avg_up, avg_dn=avg_dn, avg_sum=avg_sum,
        is_karma=is_karma, trades=trades,
    )


def report(con, bot_id, sessions, btc_lk, label, **kw):
    print(f"\n{'=' * 100}")
    print(f"[{label}]")
    print(f"{'=' * 100}")
    rs = []
    for s in sessions:
        r = sim_session(con, bot_id, s, btc_lk, **kw)
        if r and r["n_trades"] > 0:
            rs.append(r)
    if not rs:
        print("Trade yok")
        return
    print(f"\nToplam trade-yapan session: {len(rs)}")

    # Position type
    karma = sum(1 for r in rs if r["is_karma"])
    saf_up = sum(1 for r in rs if r["up_filled"] > 0 and r["dn_filled"] == 0)
    saf_dn = sum(1 for r in rs if r["dn_filled"] > 0 and r["up_filled"] == 0)
    print(f"\n[Position Type]")
    print(f"  KARMA (her iki taraf):  {karma:>3} ({100*karma/len(rs):.1f}%)")
    print(f"  SAF_UP  (sadece UP):    {saf_up:>3} ({100*saf_up/len(rs):.1f}%)")
    print(f"  SAF_DOWN (sadece DOWN): {saf_dn:>3} ({100*saf_dn/len(rs):.1f}%)")

    # avg_sum dağılımı
    bins = defaultdict(int)
    for r in rs:
        s = r["avg_sum"]
        if s < 0.10: b = "<0.10"
        elif s < 0.20: b = "0.10-0.20"
        elif s < 0.30: b = "0.20-0.30"
        elif s < 0.40: b = "0.30-0.40"
        elif s < 0.50: b = "0.40-0.50"
        elif s < 0.60: b = "0.50-0.60"
        elif s < 0.70: b = "0.60-0.70"
        elif s < 0.80: b = "0.70-0.80"
        elif s < 0.90: b = "0.80-0.90"
        elif s < 1.00: b = "0.90-1.00"
        else: b = ">=1.00"
        bins[b] += 1

    print(f"\n[avg_sum Dağılımı]")
    print(f"  {'bin':<12} | {'count':>5} | {'oran':>6}")
    print(f"  {'-' * 32}")
    order = ["<0.10", "0.10-0.20", "0.20-0.30", "0.30-0.40", "0.40-0.50",
             "0.50-0.60", "0.60-0.70", "0.70-0.80", "0.80-0.90", "0.90-1.00", ">=1.00"]
    for b in order:
        if b in bins:
            pct = 100 * bins[b] / len(rs)
            marker = " ✅" if b != ">=1.00" else " ⚠️"
            print(f"  {b:<12} | {bins[b]:>5} | {pct:>5.1f}%{marker}")

    sums = [r["avg_sum"] for r in rs]
    print(f"\n  Min avg_sum: {min(sums):.4f}")
    print(f"  Max avg_sum: {max(sums):.4f}")
    print(f"  Avg:         {sum(sums)/len(sums):.4f}")
    print(f"  avg_sum<1.0: {sum(1 for s in sums if s < 1.0)}/{len(rs)} ({100*sum(1 for s in sums if s < 1.0)/len(rs):.1f}%)")
    print(f"  avg_sum<0.5: {sum(1 for s in sums if s < 0.5)}/{len(rs)} ({100*sum(1 for s in sums if s < 0.5)/len(rs):.1f}%)")

    # Multi-trade direction analizi
    print(f"\n[Multi-Trade Direction Analizi]")
    n_dir_change = 0
    n_dir_same = 0
    for r in rs:
        if len(r["trades"]) > 1:
            dirs = [t[0] for t in r["trades"]]
            if len(set(dirs)) > 1:
                n_dir_change += 1
            else:
                n_dir_same += 1
    multi = n_dir_change + n_dir_same
    if multi > 0:
        print(f"  Multi-trade session: {multi}")
        print(f"  Yön değişen (KARMA potansiyel): {n_dir_change} ({100*n_dir_change/multi:.1f}%)")
        print(f"  Tek yön (SAF):                  {n_dir_same} ({100*n_dir_same/multi:.1f}%)")
    else:
        print("  Multi-trade yok")


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
    print(f"BOT {args.bot_id} | {len(sessions)} session — Binance Latency avg_sum analizi")
    print(f"Binance API çekiliyor...")
    btc_lk = fetch_btc(span[0] * 1000, span[1] * 1000)
    print(f"  {len(btc_lk)} kline\n")

    report(con, args.bot_id, sessions, btc_lk, "C2-base: T-15s mt=1",
           sig_thr=10, t_window=15, order=100, cooldown_s=5, max_trades=1)
    report(con, args.bot_id, sessions, btc_lk, "C2-mt3: T-15s mt=3 cd=3",
           sig_thr=10, t_window=15, order=100, cooldown_s=3, max_trades=3)
    report(con, args.bot_id, sessions, btc_lk, "C3-base: T-300s mt=1",
           sig_thr=10, t_window=300, order=100, cooldown_s=5, max_trades=1)
    report(con, args.bot_id, sessions, btc_lk, "C3-mt5: T-300s mt=5 cd=15",
           sig_thr=10, t_window=300, order=100, cooldown_s=15, max_trades=5)
    report(con, args.bot_id, sessions, btc_lk, "C3-mt10: T-300s mt=10 cd=10",
           sig_thr=10, t_window=300, order=100, cooldown_s=10, max_trades=10)


if __name__ == "__main__":
    main()
