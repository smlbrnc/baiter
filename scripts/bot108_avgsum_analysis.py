#!/usr/bin/env python3
"""Bot 108 — avg_sum < 1.0 koşulu altında derin analiz.

3 kademeli analiz:
  A) Tick verisinde sentetik arbitrage (ask_up + ask_down < 1) sıklığı
  B) Bonereaper backtest: max_avg_sum varyasyonları (0.95, 0.98, 1.00, 1.05, 1.10, 1.30)
  C) Karşılıklı pozisyon (KARMA) durumunda min(avg_up + avg_down) dağılımı
"""
import argparse
import math
import sqlite3
import sys
from collections import defaultdict

FEE_RATE = 0.0002
MIN_PRICE = 0.10
MAX_PRICE = 0.95


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


def loser_side(avg_up, avg_dn, up_filled, dn_filled, up_bid, dn_bid):
    if up_filled <= 0 and dn_filled <= 0:
        return "UP" if up_bid <= dn_bid else "DOWN"
    if up_filled > 0 and dn_filled > 0:
        return "DOWN" if avg_up >= avg_dn else "UP"
    return "DOWN" if up_filled > 0 else "UP"


def sim_session_with_max_avg(con, bot_id, sess, max_avg_sum,
                             cooldown_ms=3000,
                             first_spread_min=0.02,
                             api_min_order_size=5.0):
    """Bonereaper basit simülasyonu — sadece max_avg_sum cap değişiyor."""
    w = winner_of(con, bot_id, sess)
    if w is None:
        return None
    sm = con.execute(
        "SELECT end_ts FROM market_sessions WHERE id=?", (sess,)
    ).fetchone()
    end_ts = sm[0]
    ticks = con.execute(
        "SELECT ts_ms, up_best_bid, up_best_ask, down_best_bid, down_best_ask "
        "FROM market_ticks WHERE bot_id=? AND market_session_id=? ORDER BY ts_ms",
        (bot_id, sess),
    ).fetchall()

    # LW
    LW_SECS = 30
    LW_THR = 0.95
    LW_USDC = 500.0
    LW_MAX = 1
    # Sizing
    SZ_LONG = 5.0
    SZ_MID = 10.0
    SZ_HIGH = 15.0
    IMB_THR = 50.0

    last_buy_ms = 0
    last_up = last_dn = 0.0
    book_ready = False
    first_done = False
    up_filled = dn_filled = 0.0
    up_cost = dn_cost = 0.0
    fees = 0.0
    lw_inj = 0
    n_buys = 0
    cap_blocks = 0

    for ts_ms, ub, ua, db, da in ticks:
        if ub <= 0 or db <= 0 or ua <= 0 or da <= 0:
            continue
        if not book_ready:
            book_ready = True
            last_up, last_dn = ub, db
            continue
        sec_to_end = end_ts - ts_ms / 1000.0

        # LW
        lw_quota_ok = LW_MAX == 0 or lw_inj < LW_MAX
        if (lw_quota_ok and 0 < sec_to_end <= LW_SECS):
            wd = "UP" if ub >= db else "DOWN"
            w_bid = ub if wd == "UP" else db
            w_ask = ua if wd == "UP" else da
            if w_bid >= LW_THR and w_ask > 0:
                size = math.ceil(LW_USDC / w_ask)
                cost_t = size * w_ask
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
                last_up, last_dn = ub, db
                continue

        if last_buy_ms > 0 and (ts_ms - last_buy_ms) < cooldown_ms:
            last_up, last_dn = ub, db
            continue

        if not first_done:
            spread = ub - db
            if abs(spread) < first_spread_min:
                last_up, last_dn = ub, db
                continue
            dir_ = "UP" if spread > 0 else "DOWN"
        else:
            imb = up_filled - dn_filled
            if abs(imb) > IMB_THR:
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
        if bid < MIN_PRICE or bid > MAX_PRICE:
            continue

        if dir_ == "UP":
            cur_filled, cur_cost = up_filled, up_cost
            opp_filled, opp_cost = dn_filled, dn_cost
        else:
            cur_filled, cur_cost = dn_filled, dn_cost
            opp_filled, opp_cost = up_filled, up_cost
        cur_avg = cur_cost / cur_filled if cur_filled > 0 else 0
        opp_avg = opp_cost / opp_filled if opp_filled > 0 else 0

        # Sizing (sade — RealBot v3 winner pyramid yok burada, baseline test)
        if bid <= 0.30:
            usdc = SZ_LONG
        elif bid <= 0.85:
            usdc = SZ_MID
        else:
            usdc = SZ_HIGH
        size = math.ceil(usdc / ask)
        cost_t = size * ask
        if cost_t < api_min_order_size:
            continue

        # avg_sum cap
        if opp_filled > 0:
            new_avg = (cur_avg * cur_filled + ask * size) / (cur_filled + size) \
                if cur_filled > 0 else ask
            if new_avg + opp_avg > max_avg_sum:
                cap_blocks += 1
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
        n_buys += 1

    cost = up_cost + dn_cost
    if w == "UP":
        pnl = up_filled - cost
    else:
        pnl = dn_filled - cost
    is_karma = up_filled > 0 and dn_filled > 0
    return dict(
        sess=sess, w=w, n_buys=n_buys, cap_blocks=cap_blocks,
        cost=cost, pnl=pnl, fees=fees,
        upf=up_filled, dnf=dn_filled,
        up_cost=up_cost, dn_cost=dn_cost,
        is_karma=is_karma,
    )


def part_a_synthetic_arb(con, bot_id, sessions):
    """Tick'lerde ask_up + ask_down < 1 fırsatı sıklığı."""
    print("=" * 100)
    print("[A] SENTETİK ARBİTRAJ ANALİZİ — ask_up + ask_down < 1.0")
    print("=" * 100)
    print()
    thresholds = [0.95, 0.97, 0.98, 0.99, 1.00]
    print(f"  {'eşik':<8} | {'tick sayısı':>12} | {'oran %':>10} | {'distinct session':>18}")
    print("  " + "-" * 70)
    total_ticks = 0
    counts = {t: 0 for t in thresholds}
    sess_counts = {t: set() for t in thresholds}
    for s in sessions:
        ticks = con.execute(
            "SELECT up_best_ask, down_best_ask FROM market_ticks "
            "WHERE bot_id=? AND market_session_id=?",
            (bot_id, s),
        ).fetchall()
        for ua, da in ticks:
            if ua is None or da is None or ua <= 0 or da <= 0:
                continue
            total_ticks += 1
            ssum = ua + da
            for t in thresholds:
                if ssum < t:
                    counts[t] += 1
                    sess_counts[t].add(s)
    for t in thresholds:
        pct = 100 * counts[t] / max(1, total_ticks)
        print(f"  ask<{t:<5}| {counts[t]:>12,} | {pct:>9.3f}% | {len(sess_counts[t]):>15}")
    print(f"\n  Toplam tick: {total_ticks:,}")
    print(f"  Toplam session: {len(sessions)}")
    print()
    # Detaylı: en düşük ask sum
    min_ssum = 999
    min_detail = None
    for s in sessions:
        ticks = con.execute(
            "SELECT ts_ms, up_best_ask, down_best_ask FROM market_ticks "
            "WHERE bot_id=? AND market_session_id=?",
            (bot_id, s),
        ).fetchall()
        for ts_ms, ua, da in ticks:
            if ua and da and ua > 0 and da > 0:
                if ua + da < min_ssum:
                    min_ssum = ua + da
                    min_detail = (s, ts_ms, ua, da)
    if min_detail:
        print(f"  EN DÜŞÜK ask_up + ask_down: {min_ssum:.4f}")
        print(f"    sess={min_detail[0]} ts={min_detail[1]} ua={min_detail[2]} da={min_detail[3]}")


def part_b_max_avg_scenarios(con, bot_id, sessions):
    """Bonereaper backtest — farklı max_avg_sum değerleri."""
    print()
    print("=" * 100)
    print("[B] BONEREAPER BACKTEST — max_avg_sum varyasyonları")
    print("=" * 100)
    print()
    caps = [0.95, 0.98, 1.00, 1.05, 1.10, 1.20, 1.30, 2.00]
    print(f"  {'cap':<5} | {'tot':>4} {'KARMA':>5} {'SAF':>4} {'no_t':>4} | "
          f"{'trades':>6} {'cap_blk':>7} | {'cost':>9} {'pnl':>9} {'NET':>9} {'ROI%':>6}")
    print("  " + "-" * 95)
    for cap in caps:
        tot = karma = saf = no_trade = 0
        n_trades = n_blocks = 0
        cost = pnl = fee = 0.0
        for s in sessions:
            r = sim_session_with_max_avg(con, bot_id, s, cap)
            if r is None:
                continue
            tot += 1
            n_trades += r["n_buys"]
            n_blocks += r["cap_blocks"]
            cost += r["cost"]
            pnl += r["pnl"]
            fee += r["fees"]
            if r["n_buys"] == 0:
                no_trade += 1
            elif r["is_karma"]:
                karma += 1
            else:
                saf += 1
        net = pnl - fee
        roi = 100 * net / max(1, cost)
        marker = " ← gevşek" if cap >= 1.30 else (" ← sıkı" if cap <= 1.00 else "")
        print(f"  {cap:<5} | {tot:>4} {karma:>5} {saf:>4} {no_trade:>4} | "
              f"{n_trades:>6} {n_blocks:>7} | {cost:>9,.2f} {pnl:>+9,.2f} "
              f"{net:>+9,.2f} {roi:>+6.2f}{marker}")


def part_c_karma_avgsum_distribution(con, bot_id, sessions):
    """Karşılıklı pozisyon (KARMA) olan session'larda min(avg_sum) dağılımı."""
    print()
    print("=" * 100)
    print("[C] KARMA SESSION'LARDA min(avg_up + avg_down) DAĞILIMI")
    print("    (RealBot v3 default ile simulasyon, max_avg_sum=1.30)")
    print("=" * 100)
    print()
    bins = defaultdict(int)
    karma_count = 0
    saf_count = 0
    detail = []
    for s in sessions:
        r = sim_session_with_max_avg(con, bot_id, s, 1.30)
        if r is None or r["n_buys"] == 0:
            continue
        if not r["is_karma"]:
            saf_count += 1
            continue
        karma_count += 1
        # Tick ilerlerken avg_sum hesabını yap (sim'de değil, snapshot bazında)
        # Final avg_up + avg_down
        avg_up = r["up_cost"] / r["upf"] if r["upf"] > 0 else 0
        avg_dn = r["dn_cost"] / r["dnf"] if r["dnf"] > 0 else 0
        final_sum = avg_up + avg_dn
        # Bin
        if final_sum < 0.90:
            b = "<0.90"
        elif final_sum < 0.95:
            b = "0.90-0.95"
        elif final_sum < 1.00:
            b = "0.95-1.00"
        elif final_sum < 1.05:
            b = "1.00-1.05"
        elif final_sum < 1.10:
            b = "1.05-1.10"
        elif final_sum < 1.20:
            b = "1.10-1.20"
        else:
            b = ">=1.20"
        bins[b] += 1
        detail.append((s, final_sum, avg_up, avg_dn, r["w"], r["pnl"]))
    print(f"  Toplam KARMA session: {karma_count}")
    print(f"  Toplam SAF session:   {saf_count}")
    print()
    print(f"  {'avg_sum bin':<12} | {'count':>6} | {'oran':>6}")
    print("  " + "-" * 35)
    order = ["<0.90", "0.90-0.95", "0.95-1.00", "1.00-1.05", "1.05-1.10", "1.10-1.20", ">=1.20"]
    for b in order:
        if b in bins:
            pct = 100 * bins[b] / max(1, karma_count)
            marker = " ✅ ARBITRAGE" if b in ("<0.90", "0.90-0.95", "0.95-1.00") else ""
            print(f"  {b:<12} | {bins[b]:>6} | {pct:>5.1f}%{marker}")

    print()
    # Final avg_sum < 1.0 olanların PnL'i
    arbitrage_sessions = [d for d in detail if d[1] < 1.0]
    print(f"  KARMA + final avg_sum<1.0: {len(arbitrage_sessions)} session "
          f"({100*len(arbitrage_sessions)/max(1,karma_count):.1f}% KARMA içinde)")
    if arbitrage_sessions:
        tot_pnl = sum(d[5] for d in arbitrage_sessions)
        print(f"    Toplam PnL: ${tot_pnl:+.2f}")
        print(f"    Avg PnL/session: ${tot_pnl/len(arbitrage_sessions):+.2f}")
    non_arb = [d for d in detail if d[1] >= 1.0]
    if non_arb:
        tot_pnl_n = sum(d[5] for d in non_arb)
        print(f"  KARMA + avg_sum>=1.0: {len(non_arb)} session, PnL ${tot_pnl_n:+.2f} "
              f"avg ${tot_pnl_n/len(non_arb):+.2f}")

    # En düşük avg_sum 5 KARMA session
    print()
    print("[En düşük avg_sum'lı 10 KARMA session — arbitrage fırsatı]")
    sorted_detail = sorted(detail, key=lambda x: x[1])
    print(f"  {'sess':>5} | {'avg_sum':>8} {'avg_up':>7} {'avg_dn':>7} | "
          f"{'winner':>6} | {'pnl':>+8}")
    for d in sorted_detail[:10]:
        marker = " ✅" if d[1] < 1.0 else ""
        print(f"  {d[0]:>5} | {d[1]:>8.4f} {d[2]:>7.3f} {d[3]:>7.3f} | "
              f"{d[4]:>6} | {d[5]:>+8.2f}{marker}")


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
    print(f"BOT {args.bot_id} — {len(sessions)} session\n")

    part_a_synthetic_arb(con, args.bot_id, sessions)
    part_b_max_avg_scenarios(con, args.bot_id, sessions)
    part_c_karma_avgsum_distribution(con, args.bot_id, sessions)


if __name__ == "__main__":
    main()
