#!/usr/bin/env python3
"""Bot 91 — High-frequency Binance Latency varyantları.

Soru: Trade sıklığı arttıkça NET / ROI nasıl değişiyor?
Bonereaper benzeri 50-100 trade/session yakalayan parametreler aranıyor.

Test edilen değişkenler:
  - sig_thr: $1, $2, $3, $5, $10, $20
  - cooldown: 1s, 2s, 3s, 5s, 10s
  - max_trades: 1, 5, 10, 20, 50, 100
  - t_window: 15s, 60s, 300s

Hedef: en yüksek mutlak NET (Bonereaper-tarzı sürekli trade).
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
    """Multi-trade Binance latency."""
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

    n_t = 0; cost = pnl = 0.0
    last_t = 0
    wins = 0
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
        cost += c
        if sig_dir == w:
            pnl += size * 1.0 - c; wins += 1
        else:
            pnl -= c
        n_t += 1
        last_t = ts_sec
    return dict(cost=cost, pnl=pnl, n=n_t, wins=wins, w=w)


def aggregate(con, bot_id, sessions, btc_lk, **kw):
    triggered = total_trades = wins = 0
    cost = pnl = 0.0
    for s in sessions:
        r = sim_session(con, bot_id, s, btc_lk, **kw)
        if r is None or r["n"] == 0: continue
        triggered += 1
        total_trades += r["n"]
        wins += r["wins"]
        cost += r["cost"]; pnl += r["pnl"]
    fees = cost * FEE_RATE
    n_s = len(sessions)
    return dict(
        n_sessions=n_s, triggered=triggered, total_trades=total_trades,
        cost=cost, pnl=pnl, fees=fees, net=pnl - fees,
        roi=100 * (pnl - fees) / max(1, cost),
        wr=100 * wins / max(1, total_trades),
        avg_per_session=total_trades / max(1, n_s),
        avg_per_active=total_trades / max(1, triggered),
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
    span_h = (span[1] - span[0]) / 3600

    print("=" * 130)
    print(f"BOT {args.bot_id} | {len(sessions)} session | {span_h:.1f}h | "
          f"HIGH-FREQUENCY Binance Latency (Bonereaper-tarzı)")
    print("=" * 130)

    print(f"\nBinance API çekiliyor...")
    btc_lk = fetch_btc(span[0] * 1000, span[1] * 1000)
    print(f"  {len(btc_lk)} kline\n")

    # GENİŞ GRID
    print(f"  {'Senaryo':<55} {'trade':>5} {'avg/s':>6} {'avg/a':>6} {'WR%':>5} "
          f"{'cost':>11} {'NET':>10} {'ROI%':>6}")
    print("  " + "-" * 120)

    sc = [
        # Mevcut (referans)
        ("MEVCUT C2: T-15s sig=$10 mt=3 cd=3s", dict(sig_thr=10, t_window=15, order=100, cooldown_s=3, max_trades=3)),
        ("MEVCUT C3: T-300s sig=$10 mt=5 cd=15", dict(sig_thr=10, t_window=300, order=100, cooldown_s=15, max_trades=5)),

        # AŞIRI YOĞUN — sig çok düşük + cooldown 1s + mt yüksek
        ("ULTRA: T-300 sig=$1 mt=50 cd=1s", dict(sig_thr=1, t_window=300, order=100, cooldown_s=1, max_trades=50)),
        ("ULTRA: T-300 sig=$1 mt=100 cd=1s", dict(sig_thr=1, t_window=300, order=100, cooldown_s=1, max_trades=100)),
        ("ULTRA: T-300 sig=$3 mt=50 cd=1s", dict(sig_thr=3, t_window=300, order=100, cooldown_s=1, max_trades=50)),
        ("ULTRA: T-300 sig=$5 mt=50 cd=1s", dict(sig_thr=5, t_window=300, order=100, cooldown_s=1, max_trades=50)),
        ("ULTRA: T-300 sig=$3 mt=100 cd=2s", dict(sig_thr=3, t_window=300, order=100, cooldown_s=2, max_trades=100)),

        # Orta yoğunluk
        ("MID: T-300 sig=$5 mt=20 cd=3s", dict(sig_thr=5, t_window=300, order=100, cooldown_s=3, max_trades=20)),
        ("MID: T-300 sig=$10 mt=20 cd=3s", dict(sig_thr=10, t_window=300, order=100, cooldown_s=3, max_trades=20)),
        ("MID: T-300 sig=$5 mt=10 cd=5s", dict(sig_thr=5, t_window=300, order=100, cooldown_s=5, max_trades=10)),

        # Çok düşük order ile yüksek frekans (sermaye az tutarak)
        ("LOW-CAP: T-300 sig=$3 mt=100 cd=1s ord=$10", dict(sig_thr=3, t_window=300, order=10, cooldown_s=1, max_trades=100)),
        ("LOW-CAP: T-300 sig=$3 mt=100 cd=1s ord=$20", dict(sig_thr=3, t_window=300, order=20, cooldown_s=1, max_trades=100)),

        # Uzun pencere agresif
        ("PYR: T-300 sig=$1 mt=300 cd=1s ord=$20", dict(sig_thr=1, t_window=300, order=20, cooldown_s=1, max_trades=300)),
    ]

    results = []
    for label, kw in sc:
        r = aggregate(con, args.bot_id, sessions, btc_lk, **kw)
        results.append((label, r))
        print(f"  {label:<55} {r['total_trades']:>5} {r['avg_per_session']:>6.2f} "
              f"{r['avg_per_active']:>6.2f} {r['wr']:>5.1f} {r['cost']:>11,.0f} "
              f"{r['net']:>+10,.2f} {r['roi']:>+6.2f}")

    # NET TOP
    print(f"\n[NET TOP 5]")
    for i, (l, r) in enumerate(sorted(results, key=lambda x: x[1]["net"], reverse=True)[:5], 1):
        m = " ⭐" if i == 1 else ""
        yearly = r["net"] * (8760 / span_h)
        print(f"  {i}. {l:<55} NET=${r['net']:+10,.2f} ROI={r['roi']:+.2f}% "
              f"trades={r['total_trades']} yıllık~${yearly:+,.0f}{m}")

    # Bonereaper karşılaştırma
    print(f"\n[Trade frekansı kıyaslama]")
    print(f"  Gerçek Bonereaper (5dk pencere): ~17 trade/dk = ~85 trade/session")
    print(f"  Bizim en yüksek: {max(r[1]['avg_per_session'] for r in results):.2f} trade/session")
    print(f"  Bizim en yüksek (active session başına): {max(r[1]['avg_per_active'] for r in results):.2f}")


if __name__ == "__main__":
    main()
