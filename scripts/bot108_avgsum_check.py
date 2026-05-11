#!/usr/bin/env python3
"""Bot 108 — Yeni stratejilerin avg_sum dağılımı kontrolü.

3 strateji için her trade'in avg_sum'unu izle:
  A) Mevcut Bonereaper (RealBot v3 cap=1.30)
  B) Direction-only (en iyi NET)
  C) Hybrid (Sharpe en iyi)

Her trade için:
  - cost_per_share (BUY için ödenen fiyat)
  - position_avg_up, position_avg_dn
  - avg_sum = avg_up + avg_dn
  - cost < 1.0 mı?
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
        if ts - d in lk: return lk[ts - d]
        if ts + d in lk: return lk[ts + d]
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
    if ub > 0.95: return "UP"
    if db > 0.95: return "DOWN"
    return None


def sim_directional(con, bot_id, sess, btc_lk, sig_thr=10, t_window=300, order=100):
    """Direction-only — en iyi NET."""
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
        cost_t = size * ask
        if cost_t < 5: continue
        # Tek tarafa pozisyon — avg_sum = ask (winner side avg, opp_avg = 0)
        avg_up = ask if sig_dir == "UP" else 0.0
        avg_dn = ask if sig_dir == "DOWN" else 0.0
        avg_sum = avg_up + avg_dn
        return dict(
            sess=sess, w=w, dir=sig_dir, mode="DIRECTIONAL",
            cost=cost_t, ask=ask, size=size,
            avg_up=avg_up, avg_dn=avg_dn, avg_sum=avg_sum,
            position_type="SAF",
        )
    return None


def sim_hybrid(con, bot_id, sess, btc_lk, sig_thr=15, fak_cost=0.98,
               t_window=300, order=100, fill_rate=0.74):
    """Hybrid — directional veya arb."""
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
        if sec_to_end <= 0: continue
        btc_now = get_btc(btc_lk, ts_sec)
        if btc_now is None: continue
        delta = btc_now - btc_open
        sig_strong = abs(delta) >= sig_thr
        sig_dir = "UP" if delta > 0 else "DOWN"
        # Winner side
        if ub > 0.5 and db <= 0.5:
            w_side, w_bid, l_bid, w_ask, l_ask = "UP", ub, db, ua, da
        elif db > 0.5 and ub <= 0.5:
            w_side, w_bid, l_bid, w_ask, l_ask = "DOWN", db, ub, da, ua
        else:
            continue

        # Hybrid: güçlü sinyal + T-window içinde → directional
        if sig_strong and sec_to_end <= t_window:
            ask = ua if sig_dir == "UP" else da
            bid = ub if sig_dir == "UP" else db
            if ask <= 0 or bid < MIN_PRICE or bid > MAX_PRICE or ask >= 0.99: continue
            size = math.ceil(order / ask)
            cost_t = size * ask
            if cost_t < 5: continue
            avg_up = ask if sig_dir == "UP" else 0.0
            avg_dn = ask if sig_dir == "DOWN" else 0.0
            return dict(sess=sess, w=w, dir=sig_dir, mode="HYB-DIR",
                        cost=cost_t, ask=ask, size=size,
                        avg_up=avg_up, avg_dn=avg_dn, avg_sum=avg_up + avg_dn,
                        position_type="SAF")
        # Arb fırsatı
        cost_per = w_bid + l_bid
        if cost_per >= fak_cost: continue
        size = math.ceil(order / cost_per)
        actual = size * fill_rate
        cost_t = actual * cost_per
        if cost_t < 5: continue
        # Hem UP hem DOWN'a pozisyon (KARMA)
        if w_side == "UP":
            avg_up = w_bid
            avg_dn = l_bid
        else:
            avg_dn = w_bid
            avg_up = l_bid
        return dict(sess=sess, w=w, dir=f"ARB({w_side}+{('DOWN' if w_side=='UP' else 'UP')})",
                    mode="HYB-ARB",
                    cost=cost_t, ask=cost_per, size=actual,
                    avg_up=avg_up, avg_dn=avg_dn, avg_sum=avg_up + avg_dn,
                    position_type="KARMA")
    return None


def report_strategy(con, bot_id, sessions, btc_lk, label, sim_fn, **kwargs):
    print(f"\n{'='*100}")
    print(f"[{label}]")
    print(f"{'='*100}")
    rs = []
    for s in sessions:
        r = sim_fn(con, bot_id, s, btc_lk, **kwargs)
        if r:
            rs.append(r)
    print(f"\nToplam trade: {len(rs)}")
    if not rs: return

    # avg_sum dağılımı
    bins = defaultdict(int)
    for r in rs:
        s = r["avg_sum"]
        if s < 0.10:    b = "<0.10"
        elif s < 0.30:  b = "0.10-0.30"
        elif s < 0.50:  b = "0.30-0.50"
        elif s < 0.70:  b = "0.50-0.70"
        elif s < 0.85:  b = "0.70-0.85"
        elif s < 0.95:  b = "0.85-0.95"
        elif s < 1.00:  b = "0.95-1.00"
        else:           b = ">=1.00"
        bins[b] += 1

    print(f"\n[avg_sum dağılımı]")
    print(f"  {'bin':<12} | {'count':>6} | {'oran':>6}")
    print(f"  {'-'*36}")
    order_bins = ["<0.10", "0.10-0.30", "0.30-0.50", "0.50-0.70", "0.70-0.85",
                  "0.85-0.95", "0.95-1.00", ">=1.00"]
    for b in order_bins:
        if b in bins:
            pct = 100 * bins[b] / len(rs)
            marker = " ✅ <1.0" if b != ">=1.00" else " ⚠️"
            print(f"  {b:<12} | {bins[b]:>6} | {pct:>5.1f}%{marker}")

    # Detay
    avg_sums = [r["avg_sum"] for r in rs]
    print(f"\n  Min avg_sum: {min(avg_sums):.4f}")
    print(f"  Max avg_sum: {max(avg_sums):.4f}")
    print(f"  Ortalama:    {sum(avg_sums)/len(avg_sums):.4f}")
    print(f"  avg_sum < 1.0 olan: {sum(1 for s in avg_sums if s < 1.0)} / {len(rs)} "
          f"({100*sum(1 for s in avg_sums if s < 1.0)/len(rs):.1f}%)")

    # Position type
    saf = sum(1 for r in rs if r["position_type"] == "SAF")
    karma = sum(1 for r in rs if r["position_type"] == "KARMA")
    print(f"\n[Position type]")
    print(f"  SAF (tek taraf):  {saf} ({100*saf/len(rs):.1f}%)")
    print(f"  KARMA (her iki):  {karma} ({100*karma/len(rs):.1f}%)")

    # Mode breakdown (hybrid için)
    modes = defaultdict(int)
    for r in rs:
        modes[r["mode"]] += 1
    print(f"\n[Mode breakdown]")
    for m, c in modes.items():
        print(f"  {m}: {c} ({100*c/len(rs):.1f}%)")

    # En düşük 5 avg_sum (arbitrage en karlı)
    sorted_avg = sorted(rs, key=lambda x: x["avg_sum"])
    print(f"\n[En düşük 5 avg_sum trade'i]")
    for r in sorted_avg[:5]:
        print(f"  sess={r['sess']} mode={r['mode']:<10} dir={r['dir']:<20} "
              f"avg_up={r['avg_up']:.3f} avg_dn={r['avg_dn']:.3f} "
              f"avg_sum={r['avg_sum']:.4f} cost=${r['cost']:.2f}")


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

    print(f"BOT {args.bot_id} — {len(sessions)} session — avg_sum kontrolü")
    print("Binance API çekiliyor...")
    btc_lk = fetch_btc(span_start * 1000, span_end * 1000)
    print(f"  {len(btc_lk)} kline\n")

    # Mevcut Bonereaper (RealBot v3) — DB'den gerçek pozisyon
    print("=" * 100)
    print("[A] MEVCUT BONEREAPER (RealBot v3, gerçek bot 108 datası)")
    print("=" * 100)
    rs_real = []
    for s in sessions:
        snap = con.execute(
            "SELECT up_filled, down_filled, avg_up, avg_down "
            "FROM pnl_snapshots WHERE bot_id=? AND market_session_id=? "
            "ORDER BY ts_ms DESC LIMIT 1",
            (args.bot_id, s)
        ).fetchone()
        if not snap or (snap[0] or 0) <= 0:
            continue
        upf, dnf, au, ad = snap
        au = au or 0.0
        ad = ad or 0.0
        if upf > 0 and dnf > 0:
            ptype = "KARMA"
            asum = au + ad
        elif upf > 0:
            ptype = "SAF_UP"
            asum = au
        else:
            ptype = "SAF_DOWN"
            asum = ad
        rs_real.append((s, upf, dnf, au, ad, asum, ptype))

    print(f"\nToplam pozisyonlu session: {len(rs_real)}")
    bins = defaultdict(int)
    for r in rs_real:
        s = r[5]
        if s < 0.50:    b = "<0.50"
        elif s < 0.70:  b = "0.50-0.70"
        elif s < 0.85:  b = "0.70-0.85"
        elif s < 0.95:  b = "0.85-0.95"
        elif s < 1.00:  b = "0.95-1.00"
        elif s < 1.05:  b = "1.00-1.05"
        elif s < 1.10:  b = "1.05-1.10"
        elif s < 1.20:  b = "1.10-1.20"
        else:           b = ">=1.20"
        bins[b] += 1
    print(f"\n  {'bin':<12} | {'count':>6} | {'oran':>6}")
    for b in ["<0.50", "0.50-0.70", "0.70-0.85", "0.85-0.95", "0.95-1.00",
              "1.00-1.05", "1.05-1.10", "1.10-1.20", ">=1.20"]:
        if b in bins:
            pct = 100 * bins[b] / len(rs_real)
            marker = " ✅" if b in ("<0.50","0.50-0.70","0.70-0.85","0.85-0.95","0.95-1.00") else " ⚠️"
            print(f"  {b:<12} | {bins[b]:>6} | {pct:>5.1f}%{marker}")
    asums = [r[5] for r in rs_real]
    print(f"\n  Min: {min(asums):.4f}  Max: {max(asums):.4f}  Avg: {sum(asums)/len(asums):.4f}")
    print(f"  avg_sum<1.0: {sum(1 for s in asums if s<1.0)} / {len(rs_real)} "
          f"({100*sum(1 for s in asums if s<1.0)/len(rs_real):.1f}%)")

    # Yeni stratejiler
    report_strategy(con, args.bot_id, sessions, btc_lk,
                    "B) DIRECTIONAL (sig=$10 T=300 $100, en iyi NET)",
                    sim_directional, sig_thr=10, t_window=300, order=100)

    report_strategy(con, args.bot_id, sessions, btc_lk,
                    "C) HYBRID (sig=$15 cost<0.98 T=300 $100, en iyi NET 2.)",
                    sim_hybrid, sig_thr=15, fak_cost=0.98, t_window=300,
                    order=100, fill_rate=0.74)

    report_strategy(con, args.bot_id, sessions, btc_lk,
                    "D) HYBRID STRICT (sig=$30 cost<0.97 T=15 $100, Sharpe top)",
                    sim_hybrid, sig_thr=30, fak_cost=0.97, t_window=15,
                    order=100, fill_rate=0.74)


if __name__ == "__main__":
    main()
