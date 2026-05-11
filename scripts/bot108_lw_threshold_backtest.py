#!/usr/bin/env python3
"""Bot 108 — LW threshold/max varyantları backtest.

Mevcut RealBot v3 (lw_max=5, lw_thr=0.92) catastrophic session'larda
LW spam yapıp tüm karı götürüyor. Test edilen senaryolar:

  MEVCUT:  lw_max=5, lw_thr=0.92, lw_burst=12s/$200
  A:       lw_max=1, lw_thr=0.92 (multi-LW kes)
  B:       lw_thr=1.0 (LW TAMAMEN KAPALI)
  C:       lw_max=1, lw_thr=0.95 (sıkı + tek atış)
"""
import math
import sqlite3
import sys

DB = sys.argv[1] if len(sys.argv) > 1 else "/home/ubuntu/baiter/data/baiter.db"
BOT_ID = 108

# Sabit (RealBot v3 dışındaki)
BUY_CD_MS = 3_000
LW_BURST_SECS = 12
LW_BURST_USDC = 200.0
IMB_THR = 50.0
MAX_AVG_SUM = 1.30
SIZE_LONGSHOT = 5.0
SIZE_MID = 10.0
SIZE_HIGH = 15.0
LOSER_MIN_PRICE = 0.01
LOSER_SCALP_USDC = 1.0
LOSER_SCALP_MAX_PRICE = 0.30
LATE_PYRAMID_SECS = 60
WINNER_SIZE_FACTOR = 5.0
AVG_LOSER_MAX = 0.50
LW_SECS = 30
LW_USDC = 500.0
FIRST_SPREAD_MIN = 0.02
MIN_PRICE = 0.10
MAX_PRICE = 0.95
FEE_RATE = 0.0002


def winner_of(con, sess):
    r = con.execute(
        "SELECT up_best_bid, down_best_bid FROM market_ticks "
        "WHERE bot_id=? AND market_session_id=? ORDER BY ts_ms DESC LIMIT 1",
        (BOT_ID, sess),
    ).fetchone()
    if not r or r[0] is None:
        return None
    return "UP" if r[0] > r[1] else "DOWN"


def loser_side(avg_up, avg_dn, up_filled, dn_filled, up_bid, dn_bid):
    if up_filled <= 0 and dn_filled <= 0:
        return "UP" if up_bid <= dn_bid else "DOWN"
    if up_filled > 0 and dn_filled > 0:
        return "DOWN" if avg_up >= avg_dn else "UP"
    return "DOWN" if up_filled > 0 else "UP"


def sim(con, sess, lw_max, lw_thr, lw_burst_usdc):
    w = winner_of(con, sess)
    if w is None:
        return None
    end_ts = con.execute(
        "SELECT end_ts FROM market_sessions WHERE id=?", (sess,)
    ).fetchone()[0]
    ticks = con.execute(
        "SELECT ts_ms, up_best_bid, up_best_ask, down_best_bid, down_best_ask "
        "FROM market_ticks WHERE bot_id=? AND market_session_id=? ORDER BY ts_ms",
        (BOT_ID, sess),
    ).fetchall()

    last_buy_ms = 0
    last_up = last_dn = 0.0
    book_ready = False
    first_done = False
    up_filled = dn_filled = 0.0
    up_cost = dn_cost = 0.0
    fees = 0.0
    lw_inj = 0
    n_buys = 0
    n_scalp = 0
    n_lw = 0
    n_burst = 0

    for ts_ms, ub, ua, db, da in ticks:
        if ub <= 0 or db <= 0 or ua <= 0 or da <= 0:
            continue
        if not book_ready:
            book_ready = True
            last_up, last_dn = ub, db
            continue
        sec_to_end = end_ts - ts_ms / 1000.0

        # LW (ana + burst)
        lw_quota_ok = lw_max == 0 or lw_inj < lw_max
        if lw_quota_ok and sec_to_end > 0:
            burst_active = (
                lw_burst_usdc > 0
                and LW_BURST_SECS > 0
                and sec_to_end <= LW_BURST_SECS
            )
            main_active = (
                LW_USDC > 0
                and LW_SECS > 0
                and sec_to_end <= LW_SECS
                and not burst_active
            )
            usdc_lw = None
            is_burst = False
            if burst_active:
                usdc_lw = lw_burst_usdc
                is_burst = True
            elif main_active:
                usdc_lw = LW_USDC

            if usdc_lw is not None and usdc_lw > 0:
                if ub >= db:
                    wd, w_bid, w_ask = "UP", ub, ua
                else:
                    wd, w_bid, w_ask = "DOWN", db, da
                if w_bid >= lw_thr and w_ask > 0:
                    size = math.ceil(usdc_lw / w_ask)
                    cost_t = size * w_ask
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
                    if is_burst:
                        n_burst += 1
                    else:
                        n_lw += 1
                    last_up, last_dn = ub, db
                    continue

        if last_buy_ms > 0 and (ts_ms - last_buy_ms) < BUY_CD_MS:
            last_up, last_dn = ub, db
            continue

        if not first_done:
            spread = ub - db
            if abs(spread) < FIRST_SPREAD_MIN:
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

        avg_up = up_cost / up_filled if up_filled > 0 else 0
        avg_dn = dn_cost / dn_filled if dn_filled > 0 else 0
        loser = loser_side(avg_up, avg_dn, up_filled, dn_filled, ub, db)
        is_loser_dir = dir_ == loser

        effective_min = min(LOSER_MIN_PRICE, MIN_PRICE) if is_loser_dir else MIN_PRICE
        if bid < effective_min or bid > MAX_PRICE:
            continue

        cur_filled = up_filled if dir_ == "UP" else dn_filled
        cur_avg = avg_up if dir_ == "UP" else avg_dn
        opp_filled = dn_filled if dir_ == "UP" else up_filled
        opp_avg = avg_dn if dir_ == "UP" else avg_up

        scalp_only = is_loser_dir and cur_filled > 0 and cur_avg > AVG_LOSER_MAX
        is_scalp_band = is_loser_dir and bid <= LOSER_SCALP_MAX_PRICE and LOSER_SCALP_USDC > 0

        if scalp_only and LOSER_SCALP_USDC > 0:
            usdc = LOSER_SCALP_USDC
        elif is_scalp_band:
            usdc = LOSER_SCALP_USDC
        else:
            if bid <= 0.30:
                base = SIZE_LONGSHOT
            elif bid <= 0.85:
                base = SIZE_MID
            else:
                base = SIZE_HIGH
            if (not is_loser_dir and LATE_PYRAMID_SECS > 0
                    and 0 < sec_to_end <= LATE_PYRAMID_SECS):
                usdc = base * WINNER_SIZE_FACTOR
            else:
                usdc = base

        if usdc <= 0:
            continue
        size = math.ceil(usdc / ask)

        is_any_scalp = scalp_only or is_scalp_band
        if not is_any_scalp and opp_filled > 0:
            new_avg = (cur_avg * cur_filled + ask * size) / (cur_filled + size) if cur_filled > 0 else ask
            if new_avg + opp_avg > MAX_AVG_SUM:
                continue

        cost_t = size * ask
        if dir_ == "UP":
            up_filled += size
            up_cost += cost_t
        else:
            dn_filled += size
            dn_cost += cost_t
        fees += cost_t * FEE_RATE
        last_buy_ms = ts_ms
        first_done = True
        if is_any_scalp:
            n_scalp += 1
        else:
            n_buys += 1

    cost = up_cost + dn_cost
    realized = (up_filled if w == "UP" else dn_filled) - cost
    return dict(
        cost=cost, realized=realized, fees=fees, w=w,
        upf=up_filled, dnf=dn_filled,
        n_buys=n_buys, n_scalp=n_scalp, n_lw=n_lw, n_burst=n_burst,
    )


def aggregate(con, sessions, lw_max, lw_thr, lw_burst_usdc):
    tot_cost = tot_real = tot_fee = 0.0
    wins = losses = 0
    tot_buys = tot_scalp = tot_lw = tot_burst = 0
    worst_5 = []
    for s in sessions:
        r = sim(con, s, lw_max, lw_thr, lw_burst_usdc)
        if r is None:
            continue
        tot_cost += r["cost"]
        tot_real += r["realized"]
        tot_fee += r["fees"]
        if r["realized"] > 0:
            wins += 1
        else:
            losses += 1
        tot_buys += r["n_buys"]
        tot_scalp += r["n_scalp"]
        tot_lw += r["n_lw"]
        tot_burst += r["n_burst"]
        worst_5.append((s, r["realized"]))
    n = wins + losses
    worst_5.sort(key=lambda t: t[1])
    return dict(
        n=n, wins=wins, cost=tot_cost, realized=tot_real, fee=tot_fee,
        net=tot_real - tot_fee,
        roi=100 * (tot_real - tot_fee) / max(1, tot_cost),
        wr=100 * wins / max(1, n),
        n_buys=tot_buys, n_scalp=tot_scalp, n_lw=tot_lw, n_burst=tot_burst,
        worst_5=worst_5[:5],
    )


def main():
    con = sqlite3.connect(DB)
    sessions = [r[0] for r in con.execute(
        "SELECT id FROM market_sessions WHERE bot_id=? ORDER BY id", (BOT_ID,)
    ).fetchall()]
    print("=" * 95)
    print(f"BOT {BOT_ID} | {len(sessions)} session | LW threshold/max varyantları")
    print("=" * 95)

    scenarios = [
        ("MEVCUT (lw_max=5, thr=0.92, burst=$200)", 5, 0.92, 200.0),
        ("F: thr=1.0 + burst=0 (LW tam KAPALI)", 1, 1.00, 0.0),
        ("G: thr=1.0 + burst=$200 (lw_max=5)", 5, 1.00, 200.0),
        ("E: thr=0.95 + burst=0 + lw_max=1", 1, 0.95, 0.0),
        ("J: thr=0.97 + burst=0 + lw_max=1", 1, 0.97, 0.0),
        ("K: thr=0.99 + burst=0 + lw_max=1", 1, 0.99, 0.0),
    ]
    results = {}
    print(f"\n{'Senaryo':<40} {'WR%':>6} {'cost':>11} {'realized':>11} "
          f"{'NET':>11} {'ROI%':>7} {'lw':>5} {'lwb':>5}")
    print("-" * 110)
    for label, lw_max, lw_thr, lw_burst in scenarios:
        r = aggregate(con, sessions, lw_max, lw_thr, lw_burst)
        results[label] = r
        print(f"{label:<40} {r['wr']:>6.1f} {r['cost']:>11,.2f} "
              f"{r['realized']:>+11,.2f} {r['net']:>+11,.2f} {r['roi']:>+7.2f} "
              f"{r['n_lw']:>5} {r['n_burst']:>5}")

    # En kötü 5 session karşılaştırması
    print("\n[En kötü 5 session karşılaştırma]")
    print(f"  {'sess':>6} | " + " | ".join(f"{lab.split(':')[0]:>10}" for lab, *_ in scenarios))
    print("  " + "-" * (8 + len(scenarios) * 14))
    # MEVCUT'un en kötü 5 session'ı
    worst_sessions = [s for s, _ in results["MEVCUT"]["worst_5"]]
    for s in worst_sessions:
        row = [f"{s:>6}"]
        for label, lw_max, lw_thr, lw_burst in scenarios:
            r = sim(con, s, lw_max, lw_thr, lw_burst)
            row.append(f"{r['realized']:>+10.2f}" if r else f"{'?':>10}")
        print("  " + " | ".join(row))

    # Δ özeti (her senaryo MEVCUT'tan kazanç)
    base = results["MEVCUT"]
    print("\n[Mevcut'a göre fark]")
    print(f"  {'Senaryo':<40} {'NET Δ':>12} {'ROI Δ':>10}")
    for label, *_ in scenarios:
        r = results[label]
        d_net = r["net"] - base["net"]
        d_roi = r["roi"] - base["roi"]
        print(f"  {label:<40} {d_net:>+12,.2f} {d_roi:>+10.2f}")


if __name__ == "__main__":
    main()
