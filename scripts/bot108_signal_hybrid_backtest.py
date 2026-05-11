#!/usr/bin/env python3
"""Bot 108 — Sinyal-driven hybrid arbitrage backtest.

4 yaklaşım test edilir:
  A) Sinyal-aware FAK BID arb: cost<thr + sinyal güçlü → arbitrage
  B) Sinyal yönüne göre directional (Binance latency tarzı, tek leg)
  C) Hibrit: güçlü sinyal varsa directional, küçük sinyal → cross-leg arb
  D) Sinyal-aware sizing: sinyal güçlü = büyük order (Kelly tarzı)

Sinyal:
  - delta_btc = current_btc - btc_window_open
  - momentum_30s = current_btc - btc_30s_ago
  - signal_strength = |delta_btc| ve |momentum_30s| birleşik

PnL canonical kuralı: bid > 0.95.
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

BINANCE_KLINES_URL = "https://api.binance.com/api/v3/klines"
FEE_RATE = 0.02
MIN_PRICE = 0.10
MAX_PRICE = 0.95


def fetch_binance_btc_klines(start_ms, end_ms, interval="1s"):
    all_klines = []
    cur = start_ms
    while cur < end_ms:
        url = (
            f"{BINANCE_KLINES_URL}?symbol=BTCUSDT&interval={interval}"
            f"&startTime={cur}&endTime={end_ms}&limit=1000"
        )
        try:
            req = Request(url, headers={"User-Agent": "Mozilla/5.0"})
            with urlopen(req, timeout=10) as resp:
                data = json.loads(resp.read().decode("utf-8"))
        except (URLError, HTTPError, TimeoutError) as e:
            time.sleep(1)
            continue
        if not data:
            break
        all_klines.extend(data)
        cur = int(data[-1][6]) + 1
        time.sleep(0.05)
        if len(data) < 1000:
            break
    return all_klines


def klines_to_lookup(klines):
    return {int(k[0]) // 1000: float(k[4]) for k in klines}


def get_btc(price_lookup, ts_sec, drift=5):
    for d in range(drift):
        if ts_sec - d in price_lookup:
            return price_lookup[ts_sec - d]
        if ts_sec + d in price_lookup:
            return price_lookup[ts_sec + d]
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


def sim_session_strategy(
    con, bot_id, sess, btc_lookup, mode,
    fak_max_cost=0.99,
    sig_min_delta=10.0,    # sinyal threshold ($)
    sig_window_secs=30,
    order_usdc=20,
    fill_rate=0.74,         # FAK fill probability
    require_signal_for_arb=False,
    require_signal_alignment=False,
):
    """
    mode:
      'fak_arb_only'    : FAK BID arb (sinyal yok, baseline)
      'fak_arb_signal'  : FAK BID arb + sinyal yönü ile filtreli
      'directional'     : Sadece sinyal varsa Binance latency BUY
      'hybrid'          : Güçlü sinyal → directional; zayıf → arb
      'sized_arb'       : Sinyal-aware sizing (signal strong = 2x order)
    """
    w = winner_of(con, bot_id, sess)
    if w is None:
        return None
    sm = con.execute(
        "SELECT start_ts, end_ts FROM market_sessions WHERE id=?", (sess,)
    ).fetchone()
    start_ts, end_ts = sm[0], sm[1]
    btc_open = get_btc(btc_lookup, start_ts)
    if btc_open is None:
        return None
    ticks = con.execute(
        "SELECT ts_ms, up_best_bid, up_best_ask, down_best_bid, down_best_ask "
        "FROM market_ticks WHERE bot_id=? AND market_session_id=? ORDER BY ts_ms",
        (bot_id, sess),
    ).fetchall()

    triggered = False
    direction_taken = None
    n_arb = 0
    n_dir = 0
    cost = 0.0
    fees = 0.0
    pnl = 0.0
    payoff_size = 0.0  # garanti (her iki leg fill ise)
    saf_size = 0.0     # tek leg fill
    saf_dir = None
    saf_pnl = 0.0
    skipped_no_signal = 0

    for ts_ms, ub, ua, db, da in ticks:
        if not all(x and x > 0 for x in (ub, ua, db, da)):
            continue
        ts_sec = ts_ms // 1000
        sec_to_end = end_ts - ts_sec
        if sec_to_end <= 0:
            break
        if triggered:
            break  # tek atış per session

        btc_now = get_btc(btc_lookup, ts_sec)
        if btc_now is None:
            continue
        delta = btc_now - btc_open
        sig_dir = "UP" if delta > 0 else "DOWN"
        sig_strong = abs(delta) >= sig_min_delta

        # Winner side belirleme (bid > 0.5)
        if ub > 0.5 and db <= 0.5:
            w_side, w_bid, l_bid, w_ask, l_ask = "UP", ub, db, ua, da
        elif db > 0.5 and ub <= 0.5:
            w_side, w_bid, l_bid, w_ask, l_ask = "DOWN", db, ub, da, ua
        else:
            continue

        cost_per = w_bid + l_bid
        if cost_per >= fak_max_cost:
            continue

        # === MODE'A göre karar ===
        if mode == "fak_arb_only":
            # Sade FAK BID arb
            do_arb = True
            do_dir = False
        elif mode == "fak_arb_signal":
            # Arb + sinyal yönü ile filtreli
            if require_signal_alignment:
                if not sig_strong:
                    continue
                # Winner side sinyal yönüyle uyumlu mu?
                if w_side != sig_dir:
                    skipped_no_signal += 1
                    continue
            do_arb = True
            do_dir = False
        elif mode == "directional":
            # Sadece güçlü sinyal varsa o yöne BUY (Binance latency)
            if not sig_strong:
                continue
            do_arb = False
            do_dir = True
        elif mode == "hybrid":
            # Güçlü sinyal → directional; zayıf → arb
            if sig_strong:
                do_arb = False
                do_dir = True
            else:
                do_arb = True
                do_dir = False
        elif mode == "sized_arb":
            # Sinyal güçlü = büyük order, zayıf = küçük order
            do_arb = True
            do_dir = False
            if sig_strong:
                order_usdc_eff = order_usdc * 2
            else:
                order_usdc_eff = order_usdc
        else:
            continue

        # Order size
        if mode == "sized_arb":
            ord_now = order_usdc_eff
        else:
            ord_now = order_usdc

        if do_arb:
            # FAK BID arbitrage: hem winner_bid hem loser_bid post
            # cost_per = w_bid + l_bid
            size = math.ceil(ord_now / cost_per)
            cost_w = size * w_bid * fill_rate
            cost_l = size * l_bid * fill_rate
            actual_size = size * fill_rate  # full leg fill assumed
            cost_t = cost_w + cost_l
            if cost_t < 5:  # min order
                continue
            fees_t = cost_t * FEE_RATE
            # Payoff: her iki leg fill olduğunda 1 share = $1 (kim kazansa)
            gross = actual_size * 1.0
            pnl_t = gross - cost_t - fees_t
            cost += cost_t
            fees += fees_t
            pnl += pnl_t
            payoff_size += actual_size
            n_arb += 1
            triggered = True

            # Single-leg risk: simüle et
            # Diyelim ki sadece w_bid leg fill olsa, l_bid fill olmasa
            # Bu directional olur: winner side share alındı
            # PnL: size * (1.0 if w_side==w else 0.0) - cost_w - fees_on_w

        if do_dir:
            # Directional taker BUY (sinyal yönüne)
            dir_ = sig_dir
            ask = ua if dir_ == "UP" else da
            bid = ub if dir_ == "UP" else db
            if ask <= 0 or bid < MIN_PRICE or bid > MAX_PRICE or ask >= 0.99:
                continue
            size = math.ceil(ord_now / ask)
            cost_t = size * ask
            if cost_t < 5:
                continue
            fees_t = cost_t * FEE_RATE
            if dir_ == w:
                pnl_t = size * 1.0 - cost_t
            else:
                pnl_t = -cost_t
            cost += cost_t
            fees += fees_t
            pnl += pnl_t
            n_dir += 1
            direction_taken = dir_
            triggered = True

    return dict(
        sess=sess, w=w, triggered=triggered,
        n_arb=n_arb, n_dir=n_dir,
        cost=cost, fees=fees, pnl=pnl,
        net=pnl - fees,
        skipped_no_signal=skipped_no_signal,
    )


def aggregate(con, bot_id, sessions, btc_lookup, mode, **kwargs):
    triggered = no_trigger = 0
    wins = losses = 0
    tot_cost = tot_pnl = tot_fee = 0.0
    n_arb = n_dir = 0
    for s in sessions:
        r = sim_session_strategy(con, bot_id, s, btc_lookup, mode, **kwargs)
        if r is None:
            continue
        if not r["triggered"]:
            no_trigger += 1
            continue
        triggered += 1
        tot_cost += r["cost"]
        tot_pnl += r["pnl"]
        tot_fee += r["fees"]
        n_arb += r["n_arb"]
        n_dir += r["n_dir"]
        if r["pnl"] > r["fees"]:  # NET pozitif
            wins += 1
        else:
            losses += 1
    return dict(
        triggered=triggered, no_trigger=no_trigger,
        wins=wins, losses=losses,
        cost=tot_cost, pnl=tot_pnl, fee=tot_fee,
        net=tot_pnl - tot_fee,
        roi=100 * (tot_pnl - tot_fee) / max(1, tot_cost),
        wr=100 * wins / max(1, wins + losses),
        n_arb=n_arb, n_dir=n_dir,
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

    print("=" * 100)
    print(f"BOT {args.bot_id} — {len(sessions)} session, "
          f"{(span_end-span_start)/3600:.1f}h | Sinyal-driven Hybrid Backtest")
    print("=" * 100)

    print("\nBinance API çekiliyor...")
    klines = fetch_binance_btc_klines(span_start * 1000, span_end * 1000, "1s")
    btc_lookup = klines_to_lookup(klines)
    print(f"  {len(klines)} kline\n")

    if not btc_lookup:
        return

    # SENARYOLAR
    print(f"  {'Senaryo':<55} {'trig':>4} {'WR%':>5} {'cost':>10} "
          f"{'pnl':>9} {'NET':>9} {'ROI%':>6} {'arb':>4} {'dir':>4}")
    print("  " + "-" * 115)

    scenarios = [
        # FAK arb baseline
        ("[FAK ARB] sade, fill=74%, $20",
         "fak_arb_only", dict(fak_max_cost=0.99, order_usdc=20, fill_rate=0.74)),
        ("[FAK ARB] sade, fill=74%, $100",
         "fak_arb_only", dict(fak_max_cost=0.99, order_usdc=100, fill_rate=0.74)),
        ("[FAK ARB] cost<0.98, fill=74%, $20",
         "fak_arb_only", dict(fak_max_cost=0.98, order_usdc=20, fill_rate=0.74)),
        # Sinyal aligned
        ("[ARB+SIG] sinyal>$10 winner align, $20",
         "fak_arb_signal", dict(fak_max_cost=0.99, sig_min_delta=10, order_usdc=20,
                                fill_rate=0.74, require_signal_alignment=True)),
        ("[ARB+SIG] sinyal>$30 winner align, $20",
         "fak_arb_signal", dict(fak_max_cost=0.99, sig_min_delta=30, order_usdc=20,
                                fill_rate=0.74, require_signal_alignment=True)),
        ("[ARB+SIG] sinyal>$10 winner align, $100",
         "fak_arb_signal", dict(fak_max_cost=0.99, sig_min_delta=10, order_usdc=100,
                                fill_rate=0.74, require_signal_alignment=True)),
        # Directional (Binance latency)
        ("[DIR] sinyal>$10, $20",
         "directional", dict(sig_min_delta=10, order_usdc=20)),
        ("[DIR] sinyal>$30, $20",
         "directional", dict(sig_min_delta=30, order_usdc=20)),
        ("[DIR] sinyal>$10, $100",
         "directional", dict(sig_min_delta=10, order_usdc=100)),
        # Hybrid
        ("[HYB] güçlü>$30 dir, zayıf arb, $20",
         "hybrid", dict(sig_min_delta=30, fak_max_cost=0.99,
                        order_usdc=20, fill_rate=0.74)),
        ("[HYB] güçlü>$50 dir, zayıf arb, $20",
         "hybrid", dict(sig_min_delta=50, fak_max_cost=0.99,
                        order_usdc=20, fill_rate=0.74)),
        ("[HYB] güçlü>$30 dir, zayıf arb, $100",
         "hybrid", dict(sig_min_delta=30, fak_max_cost=0.99,
                        order_usdc=100, fill_rate=0.74)),
        # Sized arb
        ("[SIZED] arb, sig>$30 → 2x size, $20 base",
         "sized_arb", dict(sig_min_delta=30, fak_max_cost=0.99,
                           order_usdc=20, fill_rate=0.74)),
        ("[SIZED] arb, sig>$30 → 2x size, $50 base",
         "sized_arb", dict(sig_min_delta=30, fak_max_cost=0.99,
                           order_usdc=50, fill_rate=0.74)),
    ]

    results = []
    for label, mode, kw in scenarios:
        r = aggregate(con, args.bot_id, sessions, btc_lookup, mode, **kw)
        results.append((label, r))
        print(f"  {label:<55} {r['triggered']:>4} {r['wr']:>5.1f} {r['cost']:>10,.2f} "
              f"{r['pnl']:>+9,.2f} {r['net']:>+9,.2f} {r['roi']:>+6.2f} "
              f"{r['n_arb']:>4} {r['n_dir']:>4}")

    # Sıralamalar
    print("\n[NET TOP 10]")
    sorted_n = sorted(results, key=lambda x: x[1]["net"], reverse=True)
    for i, (label, r) in enumerate(sorted_n[:10], 1):
        marker = " ⭐" if i == 1 else ""
        print(f"  {i}. {label:<55} NET=${r['net']:+8.2f} ROI={r['roi']:+.2f}% "
              f"trig={r['triggered']}{marker}")

    print("\n[ROI TOP 10 (>=20 trigger)]")
    valid = [(l, r) for l, r in results if r["triggered"] >= 20]
    sorted_r = sorted(valid, key=lambda x: x[1]["roi"], reverse=True)
    for i, (label, r) in enumerate(sorted_r[:10], 1):
        marker = " ⭐" if i == 1 else ""
        print(f"  {i}. {label:<55} ROI={r['roi']:+6.2f}% NET=${r['net']:+8.2f} "
              f"trig={r['triggered']} WR={r['wr']:.1f}%{marker}")


if __name__ == "__main__":
    main()
