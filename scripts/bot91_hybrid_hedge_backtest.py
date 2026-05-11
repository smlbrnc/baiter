#!/usr/bin/env python3
"""Bot 91 — Hybrid: Binance Latency + Arbitrage Hedge backtest.

Hipotez: Sinyal yön main BUY + karşı taraf küçük hedge BUY = sigortalı versiyon.
Yanlış yön (%11 case) kayıp azalır mı, NET nasıl etkilenir?

Test:
  - main_usdc: $100 (sabit)
  - hedge_usdc: $0, $5, $10, $20, $30, $50 (0 = pure directional)
  - cost_max: 0.95, 0.97, 0.99 (hedge avg_sum cap)
"""
import math, sqlite3, sys, time, json, argparse
from urllib.request import urlopen, Request

BINANCE_URL = "https://api.binance.com/api/v3/klines"
FEE_RATE = 0.02
MIN_PRICE = 0.10
MAX_PRICE = 0.95


def fetch_btc(start_ms, end_ms):
    klines = []
    cur = start_ms
    n = 0
    while cur < end_ms:
        url = f"{BINANCE_URL}?symbol=BTCUSDT&interval=1s&startTime={cur}&endTime={end_ms}&limit=1000"
        try:
            req = Request(url, headers={"User-Agent": "Mozilla/5.0"})
            with urlopen(req, timeout=10) as r:
                d = json.loads(r.read().decode())
        except Exception:
            time.sleep(1); continue
        if not d: break
        klines.extend(d); cur = int(d[-1][6]) + 1; n += 1
        if n % 50 == 0: print(f"  {n} batch...", file=sys.stderr)
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
        (bot_id, sess)).fetchone()
    if not r or r[0] is None: return None
    ub, db = r[0] or 0.0, r[1] or 0.0
    if ub > 0.95: return "UP"
    if db > 0.95: return "DOWN"
    return None


def sim_session(con, bot_id, sess, btc_lk,
                sig_thr, t_window, main_usdc, hedge_usdc, cost_max,
                cooldown_s, max_trades):
    """Hybrid: main BUY + opt hedge BUY."""
    w = winner_of(con, bot_id, sess)
    if w is None: return None
    sm = con.execute("SELECT start_ts, end_ts FROM market_sessions WHERE id=?", (sess,)).fetchone()
    start_ts, end_ts = sm[0], sm[1]
    btc_open = get_btc(btc_lk, start_ts)
    if btc_open is None: return None
    ticks = con.execute(
        "SELECT ts_ms, up_best_bid, up_best_ask, down_best_bid, down_best_ask "
        "FROM market_ticks WHERE bot_id=? AND market_session_id=? ORDER BY ts_ms",
        (bot_id, sess)).fetchall()
    n_t = main_wins = hedge_wins = hedge_n = 0
    cost = pnl = 0.0
    last_t = 0
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
        opp_dir = "DOWN" if sig_dir == "UP" else "UP"
        main_ask = ua if sig_dir == "UP" else da
        main_bid = ub if sig_dir == "UP" else db
        hedge_bid = db if sig_dir == "UP" else ub
        if main_ask <= 0 or main_bid < MIN_PRICE or main_bid > MAX_PRICE or main_ask >= 0.99:
            continue

        # Main leg
        main_size = math.ceil(main_usdc / main_ask)
        c_main = main_size * main_ask
        if c_main < 5: continue
        cost += c_main
        if sig_dir == w:
            pnl += main_size * 1.0 - c_main; main_wins += 1
        else:
            pnl -= c_main
        n_t += 1
        last_t = ts_sec

        # Hedge leg (opt)
        if hedge_usdc > 0 and hedge_bid > 0:
            avg_sum = main_ask + hedge_bid
            if avg_sum < cost_max and hedge_bid >= 0.01:
                hedge_size = math.ceil(hedge_usdc / hedge_bid)
                c_hedge = hedge_size * hedge_bid
                if c_hedge >= 5:
                    cost += c_hedge
                    hedge_n += 1
                    if opp_dir == w:
                        pnl += hedge_size * 1.0 - c_hedge; hedge_wins += 1
                    else:
                        pnl -= c_hedge
    return dict(cost=cost, pnl=pnl, n=n_t, main_wins=main_wins,
                hedge_n=hedge_n, hedge_wins=hedge_wins, w=w)


def aggregate(con, bot_id, sessions, btc_lk, **kw):
    triggered = total = main_wins = hedge_n = hedge_wins = 0
    cost = pnl = 0.0
    for s in sessions:
        r = sim_session(con, bot_id, s, btc_lk, **kw)
        if r is None or r["n"] == 0: continue
        triggered += 1; total += r["n"]; main_wins += r["main_wins"]
        hedge_n += r["hedge_n"]; hedge_wins += r["hedge_wins"]
        cost += r["cost"]; pnl += r["pnl"]
    fees = cost * FEE_RATE
    n_s = len(sessions)
    return dict(triggered=triggered, total=total, hedge_n=hedge_n,
                cost=cost, pnl=pnl, fees=fees,
                net=pnl - fees, roi=100 * (pnl - fees) / max(1, cost),
                main_wr=100 * main_wins / max(1, total),
                hedge_wr=100 * hedge_wins / max(1, hedge_n))


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("bot_id", type=int)
    ap.add_argument("db", nargs="?", default="/home/ubuntu/baiter/data/baiter.db")
    args = ap.parse_args()
    con = sqlite3.connect(args.db)
    sessions = [r[0] for r in con.execute(
        "SELECT id FROM market_sessions WHERE bot_id=? ORDER BY id", (args.bot_id,)).fetchall()]
    span = con.execute("SELECT MIN(start_ts), MAX(end_ts) FROM market_sessions WHERE bot_id=?",
                       (args.bot_id,)).fetchone()
    span_h = (span[1] - span[0]) / 3600
    print(f"BOT {args.bot_id} | {len(sessions)} session | {span_h:.1f}h | HYBRID Hedge backtest")
    print(f"\nBinance API çekiliyor...")
    btc_lk = fetch_btc(span[0] * 1000, span[1] * 1000)
    print(f"  {len(btc_lk)} kline\n")

    print(f"  {'Hedge USDC':>10} {'cost_max':>9} {'main_wr':>7} {'hedge_wr':>8} "
          f"{'trades':>7} {'hedges':>7} {'cost':>10} {'NET':>10} {'ROI%':>7}")
    print("  " + "-" * 105)

    base_kw = dict(sig_thr=50, t_window=300, main_usdc=100, cooldown_s=3, max_trades=10)
    sc = []
    for hedge in [0, 5, 10, 20, 30, 50]:
        for cm in [0.90, 0.93, 0.95, 0.97, 0.99]:
            sc.append(("h" + str(hedge), hedge, cm))

    results = []
    for label, hedge, cm in sc:
        r = aggregate(con, args.bot_id, sessions, btc_lk,
                      hedge_usdc=hedge, cost_max=cm, **base_kw)
        results.append((label, hedge, cm, r))
        marker = " (pure)" if hedge == 0 else ""
        print(f"  ${hedge:>9} {cm:>9.2f} {r['main_wr']:>6.1f}% "
              f"{r['hedge_wr']:>7.1f}% {r['total']:>7} {r['hedge_n']:>7} "
              f"{r['cost']:>10,.0f} {r['net']:>+10,.2f} {r['roi']:>+7.2f}{marker}")

    print(f"\n[NET TOP 10]")
    for i, (l, h, cm, r) in enumerate(sorted(results, key=lambda x: x[3]["net"], reverse=True)[:10], 1):
        m = " ⭐" if i == 1 else ""
        print(f"  {i:>2}. hedge=${h:<3} cost_max={cm} → "
              f"NET=${r['net']:+10,.2f} ROI={r['roi']:+6.2f}% "
              f"main_wr={r['main_wr']:.1f}% hedge_n={r['hedge_n']}{m}")

    print(f"\n[ROI TOP 10 (>= 200 trade)]")
    valid = [(l, h, cm, r) for l, h, cm, r in results if r["total"] >= 200]
    for i, (l, h, cm, r) in enumerate(sorted(valid, key=lambda x: x[3]["roi"], reverse=True)[:10], 1):
        m = " ⭐" if i == 1 else ""
        print(f"  {i:>2}. hedge=${h:<3} cost_max={cm} → "
              f"ROI={r['roi']:+6.2f}% NET=${r['net']:+10,.2f} trades={r['total']}{m}")


if __name__ == "__main__":
    main()
