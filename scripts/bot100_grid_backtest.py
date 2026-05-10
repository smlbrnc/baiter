#!/usr/bin/env python3
"""Bot 100 — max_avg_sum sıkılaştırma + ilk N trade longshot grid backtest.

Mevcut yön seçimi (`|Δbid|` abs) korunur. İki parametre taranır:
  * MAX_AVG_SUM ∈ {0.95, 1.00, 1.05, 1.10 (mevcut), 1.15}
  * MIN_SIZE_FIRST_N ∈ {0 (kapalı), 3, 5, 8} → ilk N trade'de longshot ($5) zorla

Tüm bot 100 session'ları için her tick gerçek `market_ticks` üzerinden
yeniden simüle edilir. Sonuç: NET PnL, ROI, winrate, # trade.
"""
import math
import sqlite3
import sys

DB = sys.argv[1] if len(sys.argv) > 1 else "/home/ubuntu/baiter/data/baiter.db"
BOT_ID = 100

# Sabit (LIVE_safe_500)
BUY_CD_MS = 15_000
LW_SECS = 30
LW_THR = 0.92
LW_USDC = 500.0
LW_MAX = 1
IMB_THR = 200.0
SIZE_LONGSHOT = 5.0
SIZE_MID = 10.0
SIZE_HIGH = 15.0
MIN_PRICE = 0.10
MAX_PRICE = 0.95
FEE_RATE = 0.0002


def winner(con, sess):
    r = con.execute(
        "SELECT up_best_bid, down_best_bid FROM market_ticks "
        "WHERE bot_id=? AND market_session_id=? ORDER BY ts_ms DESC LIMIT 1",
        (BOT_ID, sess),
    ).fetchone()
    if not r or r[0] is None:
        return None
    return "UP" if r[0] > r[1] else "DOWN"


def sim_session(con, sess, max_avg_sum, min_size_first_n):
    """min_size_first_n: ilk N trade'de longshot zorla; 0 = kapalı."""
    w = winner(con, sess)
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
    up_filled = dn_filled = 0.0
    up_cost = dn_cost = 0.0
    fees = 0.0
    lw_done = False
    n_buys = 0  # normal BUY (LW hariç)

    for ts_ms, ub, ua, db, da in ticks:
        if ub <= 0 or db <= 0 or ua <= 0 or da <= 0:
            continue
        if not book_ready:
            book_ready = True
            last_up, last_dn = ub, db
            continue
        sec_to_end = end_ts - ts_ms / 1000.0

        # LATE WINNER
        if not lw_done and 0 < sec_to_end <= LW_SECS:
            if ub >= db:
                wd, w_bid, w_ask = "UP", ub, ua
            else:
                wd, w_bid, w_ask = "DOWN", db, da
            if w_bid >= LW_THR and w_ask > 0:
                size = math.ceil(LW_USDC / w_ask)
                cost_t = size * w_ask
                if wd == "UP":
                    up_filled += size
                    up_cost += cost_t
                else:
                    dn_filled += size
                    dn_cost += cost_t
                fees += cost_t * FEE_RATE
                last_buy_ms = ts_ms
                lw_done = True
                continue

        if last_buy_ms > 0 and (ts_ms - last_buy_ms) < BUY_CD_MS:
            last_up, last_dn = ub, db
            continue

        # Yön: MEVCUT abs
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
        if bid < MIN_PRICE or bid > MAX_PRICE:
            continue

        # Dinamik size — ilk N trade longshot zorla
        if min_size_first_n > 0 and n_buys < min_size_first_n:
            usdc = SIZE_LONGSHOT
        else:
            if bid <= 0.30:
                usdc = SIZE_LONGSHOT
            elif bid <= 0.85:
                usdc = SIZE_MID
            else:
                usdc = SIZE_HIGH
        size = math.ceil(usdc / ask)

        # avg_sum cap
        if dir_ == "UP":
            cf, cc, of, oc = up_filled, up_cost, dn_filled, dn_cost
        else:
            cf, cc, of, oc = dn_filled, dn_cost, up_filled, up_cost
        cur_avg = cc / cf if cf > 0 else 0.0
        opp_avg = oc / of if of > 0 else 0.0
        new_avg = (cur_avg * cf + ask * size) / (cf + size) if cf > 0 else ask
        if of > 0 and (new_avg + opp_avg) > max_avg_sum:
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
        n_buys += 1

    cost = up_cost + dn_cost
    realized = (up_filled if w == "UP" else dn_filled) - cost
    return dict(cost=cost, realized=realized, fees=fees, w=w,
                upf=up_filled, dnf=dn_filled, n_buys=n_buys)


def aggregate(con, sessions, max_avg_sum, min_size_first_n):
    tot_cost = tot_realized = tot_fee = 0.0
    wins = losses = 0
    for s in sessions:
        r = sim_session(con, s, max_avg_sum, min_size_first_n)
        if r is None:
            continue
        tot_cost += r["cost"]
        tot_realized += r["realized"]
        tot_fee += r["fees"]
        if r["realized"] > 0:
            wins += 1
        else:
            losses += 1
    n = wins + losses
    return dict(
        n=n, wins=wins, cost=tot_cost, realized=tot_realized, fee=tot_fee,
        net=tot_realized - tot_fee,
        roi=100 * (tot_realized - tot_fee) / max(1, tot_cost),
        wr=100 * wins / max(1, n),
    )


def main():
    con = sqlite3.connect(DB)
    sessions = [r[0] for r in con.execute(
        "SELECT id FROM market_sessions WHERE bot_id=? ORDER BY id", (BOT_ID,)
    ).fetchall()]
    print(f"BOT {BOT_ID} | {len(sessions)} session | grid backtest\n")
    print("Yön mantığı: MEVCUT abs |Δbid| (signed denenmez, geçen testte −$1150).")
    print("Sabit: LW=$500 thr=0.92 cd=15s longshot=$5 mid=$10 high=$15\n")

    avg_caps = [0.95, 1.00, 1.05, 1.10, 1.15]
    first_ns = [0, 3, 5, 8]

    print("[Tablo 1] NET PnL ($) — satır: max_avg_sum, sütun: ilk N longshot")
    print(f"  {'avg_sum':>8} | " + " ".join(f"N={n:<2}".rjust(10) for n in first_ns))
    print(f"  {'-'*8}-+-" + "-".join("-"*10 for _ in first_ns))
    grid = {}
    for cap in avg_caps:
        row = []
        for n in first_ns:
            r = aggregate(con, sessions, cap, n)
            grid[(cap, n)] = r
            row.append(f"{r['net']:>+10.2f}")
        marker = " ← mevcut" if cap == 1.10 else ""
        print(f"  {cap:>8.2f} | " + " ".join(row) + marker)

    print("\n[Tablo 2] ROI (%) — aynı grid")
    print(f"  {'avg_sum':>8} | " + " ".join(f"N={n:<2}".rjust(10) for n in first_ns))
    print(f"  {'-'*8}-+-" + "-".join("-"*10 for _ in first_ns))
    for cap in avg_caps:
        row = [f"{grid[(cap, n)]['roi']:>+9.2f}%" for n in first_ns]
        print(f"  {cap:>8.2f} | " + " ".join(row))

    print("\n[Tablo 3] Winrate (%) ve Cost ($)")
    print(f"  {'avg_sum':>8} | {'N':>3} {'WR%':>6} {'cost':>10} {'realized':>10}")
    print(f"  {'-'*8}-+-{'-'*3} {'-'*6} {'-'*10} {'-'*10}")
    best = max(grid.values(), key=lambda r: r["net"])
    best_key = [k for k, v in grid.items() if v is best][0]
    for cap in avg_caps:
        for n in first_ns:
            r = grid[(cap, n)]
            mark = " ← BEST" if (cap, n) == best_key else ""
            print(f"  {cap:>8.2f} | {n:>3} {r['wr']:>6.1f} {r['cost']:>10,.2f} {r['realized']:>+10,.2f}{mark}")

    base = grid[(1.10, 0)]
    print("\n[Özet] Mevcut (avg_sum=1.10, N=0): "
          f"NET ${base['net']:+,.2f}  ROI {base['roi']:+.2f}%  WR {base['wr']:.1f}%")
    print(f"[En iyi] avg_sum={best_key[0]}, N={best_key[1]}: "
          f"NET ${best['net']:+,.2f}  ROI {best['roi']:+.2f}%  WR {best['wr']:.1f}%")
    diff = best["net"] - base["net"]
    print(f"[Δ]      NET +${diff:,.2f} ({100*diff/abs(base['net']):+.1f}% relatif)")


if __name__ == "__main__":
    main()
