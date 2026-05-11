#!/usr/bin/env python3
"""Bot 108 — avg_sum < 1 garantili tüm alternatif trade yöntemleri.

8 boyutlu grid:
  1. Tick interval (check rate): 1s, 2s, 3s, 5s, 10s, 20s
  2. Cost threshold: 0.95, 0.97, 0.98, 0.985, 0.99
  3. Order size: 20, 50, 100
  4. Multi-trade per session: 1, 2, 3, 5
  5. Cooldown between trades: 5s, 10s, 30s
  6. Entry timing window: full, T-200, T-100, T-30
  7. Wait-for-best (en düşük cost'u bekle): false, true (T-30s'e kadar)
  8. Size adaptation: fixed, kelly_like (cost düşük=büyük order)

Tüm trade'ler garanti avg_sum<1 → arbitrage.
"""
import argparse
import math
import sqlite3
import time
import json
from urllib.request import urlopen, Request
from collections import defaultdict


BINANCE_URL = "https://api.binance.com/api/v3/klines"
FEE_RATE = 0.02
MIN_PRICE = 0.10


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


def sim_arbitrage(con, bot_id, sess,
                  tick_interval_secs=1,
                  cost_max=0.99,
                  order_usdc=20,
                  max_trades=1,
                  cooldown_secs=10,
                  entry_window_secs=300,
                  wait_for_best_until=0,  # T-X sn'ye kadar bekle, sonra al
                  size_adaptive=False,
                  fill_rate=0.74):
    """Pure arbitrage: bid_winner + bid_loser < cost_max → her iki tarafa BUY."""
    w = winner_of(con, bot_id, sess)
    if w is None:
        return None
    sm = con.execute(
        "SELECT start_ts, end_ts FROM market_sessions WHERE id=?", (sess,)
    ).fetchone()
    end_ts = sm[1]
    ticks = con.execute(
        "SELECT ts_ms, up_best_bid, up_best_ask, down_best_bid, down_best_ask "
        "FROM market_ticks WHERE bot_id=? AND market_session_id=? ORDER BY ts_ms",
        (bot_id, sess),
    ).fetchall()

    last_check_ts = 0
    last_trade_ts = 0
    n_trades = 0
    cost = pnl = fees = 0.0
    avg_sums = []
    best_arb = None  # wait_for_best modunda

    for ts_ms, ub, ua, db, da in ticks:
        if not all(x and x > 0 for x in (ub, ua, db, da)):
            continue
        ts_sec = ts_ms // 1000
        sec_to_end = end_ts - ts_sec
        if sec_to_end <= 0:
            break

        # Tick interval kontrolü (örnek 3s'de bir kontrol)
        if ts_sec - last_check_ts < tick_interval_secs:
            continue
        last_check_ts = ts_sec

        # Entry window kontrolü
        if sec_to_end > entry_window_secs:
            continue

        # Cooldown
        if last_trade_ts > 0 and ts_sec - last_trade_ts < cooldown_secs:
            continue
        if n_trades >= max_trades:
            break

        # Winner side bid > 0.5
        if ub > 0.5 and db <= 0.5:
            w_bid, l_bid = ub, db
        elif db > 0.5 and ub <= 0.5:
            w_bid, l_bid = db, ub
        else:
            continue

        cost_per = w_bid + l_bid
        if cost_per >= cost_max:
            continue

        # wait_for_best_until > 0 ise, T-X sn'ye kadar bekle (en iyi fırsatı seç)
        if wait_for_best_until > 0 and sec_to_end > wait_for_best_until:
            # En iyi fırsatı kaydet, alma
            if best_arb is None or cost_per < best_arb[1]:
                best_arb = (ts_sec, cost_per, w_bid, l_bid, ub, ua, db, da)
            continue
        elif wait_for_best_until > 0 and best_arb is not None:
            # Şu an T-X sn'ye geldik, en iyi fırsatı al (eğer şu anki cost daha düşükse onu)
            if cost_per > best_arb[1]:
                _, cost_per, w_bid, l_bid = best_arb[0], best_arb[1], best_arb[2], best_arb[3]
            best_arb = None  # bir kere kullan

        # Size adaptation
        if size_adaptive:
            # Cost düşükse büyük order, yüksekse küçük
            scale = (cost_max - cost_per) / (cost_max - 0.85) if cost_max > 0.85 else 1.0
            scale = max(0.5, min(2.5, scale))
            order_eff = order_usdc * scale
        else:
            order_eff = order_usdc

        size = math.ceil(order_eff / cost_per)
        actual = size * fill_rate
        cost_t = actual * cost_per
        if cost_t < 5:
            continue
        fees_t = cost_t * FEE_RATE
        gross = actual * 1.0  # winner side share = $1, loser = $0 → toplam $size
        pnl_t = gross - cost_t - fees_t
        cost += cost_t
        pnl += pnl_t
        fees += fees_t
        avg_sums.append(cost_per)
        n_trades += 1
        last_trade_ts = ts_sec

    return dict(
        sess=sess, w=w, n_trades=n_trades,
        cost=cost, pnl=pnl, fees=fees, net=pnl - fees,
        avg_sums=avg_sums,
    )


def aggregate(con, bot_id, sessions, **kwargs):
    triggered = 0
    wins = losses = 0
    cost = pnl = fees = 0.0
    n_trades = 0
    all_avg_sums = []
    for s in sessions:
        r = sim_arbitrage(con, bot_id, s, **kwargs)
        if r is None or r["n_trades"] == 0:
            continue
        triggered += 1
        cost += r["cost"]
        pnl += r["pnl"]
        fees += r["fees"]
        n_trades += r["n_trades"]
        all_avg_sums.extend(r["avg_sums"])
        if r["pnl"] > r["fees"]:
            wins += 1
        else:
            losses += 1
    n = wins + losses
    avg_avgsum = sum(all_avg_sums) / max(1, len(all_avg_sums))
    return dict(
        triggered=triggered, wins=wins, n=n,
        cost=cost, pnl=pnl, fees=fees, net=pnl - fees,
        roi=100 * (pnl - fees) / max(1, cost),
        wr=100 * wins / max(1, n),
        n_trades=n_trades,
        avg_avgsum=avg_avgsum,
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
    print("=" * 110)
    print(f"BOT {args.bot_id} — {len(sessions)} session | avg_sum<1 ARBITRAGE GRAND SEARCH")
    print("=" * 110)

    # === GRID ===
    tick_intervals = [1, 2, 3, 5, 10, 20]
    cost_maxes = [0.95, 0.97, 0.98, 0.985, 0.99]
    orders = [20, 50, 100]
    max_trades_list = [1, 2, 3, 5]
    cooldowns = [5, 10, 30]
    entry_windows = [300, 150, 60]
    wait_for_bests = [0, 30, 60]
    size_adaptives = [False, True]

    all_results = []
    print("\nGrid taraması başlıyor (binlerce kombinasyon)...")
    count = 0
    for ti in tick_intervals:
        for cm in cost_maxes:
            for od in orders:
                for mt in max_trades_list:
                    for cd in cooldowns:
                        for ew in entry_windows:
                            for wfb in wait_for_bests:
                                for sa in size_adaptives:
                                    if mt == 1 and cd != cooldowns[0]:
                                        continue  # cooldown etkisiz
                                    params = dict(
                                        tick_interval_secs=ti,
                                        cost_max=cm,
                                        order_usdc=od,
                                        max_trades=mt,
                                        cooldown_secs=cd,
                                        entry_window_secs=ew,
                                        wait_for_best_until=wfb,
                                        size_adaptive=sa,
                                    )
                                    r = aggregate(con, args.bot_id, sessions, **params)
                                    label = (f"int={ti}s cost<{cm} ${od:<3} mt={mt} "
                                             f"cd={cd}s win={ew} wait={wfb} adapt={sa}")
                                    all_results.append((label, r, params))
                                    count += 1
    print(f"  Toplam {count} kombinasyon test edildi\n")

    # En iyi NET
    valid = [(l, r, p) for l, r, p in all_results if r["triggered"] >= 30]
    print(f"[NET TOP 15]")
    print(f"  {'Senaryo':<75} {'trig':>4} {'WR%':>5} {'avgΣ':>5} {'NET':>8} {'ROI%':>6}")
    print("  " + "-" * 115)
    sorted_n = sorted(valid, key=lambda x: x[1]["net"], reverse=True)
    for i, (l, r, _) in enumerate(sorted_n[:15], 1):
        marker = " ⭐" if i == 1 else ""
        print(f"  {l:<75} {r['triggered']:>4} {r['wr']:>5.1f} {r['avg_avgsum']:>5.3f} "
              f"{r['net']:>+8.2f} {r['roi']:>+6.2f}{marker}")

    print(f"\n[ROI TOP 15]")
    sorted_r = sorted(valid, key=lambda x: x[1]["roi"], reverse=True)
    for i, (l, r, _) in enumerate(sorted_r[:15], 1):
        marker = " ⭐" if i == 1 else ""
        print(f"  {l:<75} ROI={r['roi']:>+6.2f}% NET=${r['net']:+8.2f} "
              f"trig={r['triggered']:>3} avgΣ={r['avg_avgsum']:.3f}{marker}")

    # WR x ROI birleşik (en istikrarlı)
    print(f"\n[İstikrarlı TOP 10 — WR>=80% ve ROI>0]")
    wr_high = [(l, r, p) for l, r, p in valid if r["wr"] >= 80 and r["roi"] > 0]
    sorted_h = sorted(wr_high, key=lambda x: x[1]["net"], reverse=True)
    for i, (l, r, _) in enumerate(sorted_h[:10], 1):
        marker = " ⭐" if i == 1 else ""
        print(f"  {l:<75} NET=${r['net']:+8.2f} ROI={r['roi']:+.2f}% "
              f"WR={r['wr']:.1f}% trig={r['triggered']}{marker}")

    # En düşük avg_sum (saf arbitrage)
    print(f"\n[En düşük avg_sum TOP 10]")
    sorted_a = sorted(valid, key=lambda x: x[1]["avg_avgsum"])
    for i, (l, r, _) in enumerate(sorted_a[:10], 1):
        marker = " ⭐" if i == 1 else ""
        print(f"  {l:<75} avgΣ={r['avg_avgsum']:.4f} NET=${r['net']:+8.2f} "
              f"ROI={r['roi']:+.2f}% trig={r['triggered']}{marker}")

    # En çok trade yapan
    print(f"\n[En yüksek trade hacmi TOP 5]")
    sorted_t = sorted(valid, key=lambda x: x[1]["n_trades"], reverse=True)
    for i, (l, r, _) in enumerate(sorted_t[:5], 1):
        print(f"  {l:<75} trades={r['n_trades']} NET=${r['net']:+8.2f} ROI={r['roi']:+.2f}%")


if __name__ == "__main__":
    main()
