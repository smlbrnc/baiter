#!/usr/bin/env python3
"""Bot 100 — signed Δbid yön seçimi backtest karşılaştırması.

Mevcut strateji (`bonereaper.rs`): `|Δbid|.abs()` → düşen tarafa da BUY tetikler.
Önerilen: signed Δ → bid'i ARTAN tarafa BUY (winner momentum, AMM-uyumlu).

Aynı LIVE_safe_500 parametreleri ile (LW=$500, thr=0.92, cooldown=15s) tüm
bot 100 session'ları için her tick yön kararını yeniden hesaplar; trades/cost/
realized PnL'i karşılaştırır.
"""
import math
import sqlite3
import sys

DB = sys.argv[1] if len(sys.argv) > 1 else "/home/ubuntu/baiter/data/baiter.db"
BOT_ID = 100

# === LIVE_safe_500 parametreleri (DB'deki değerler) ===
BUY_CD_MS = 15_000
LW_SECS = 30
LW_THR = 0.92
LW_USDC = 500.0
LW_MAX = 1
IMB_THR = 200.0
MAX_AVG_SUM = 1.10
SIZE_LONGSHOT = 5.0
SIZE_MID = 10.0
SIZE_HIGH = 15.0
MIN_PRICE = 0.10
MAX_PRICE = 0.95
FEE_RATE = 0.0002  # DRYRUN_FEE_RATE


def fetchall(con, q, args=()):
    return con.execute(q, args).fetchall()


def winner(con, sess):
    r = con.execute(
        "SELECT up_best_bid, down_best_bid FROM market_ticks "
        "WHERE bot_id=? AND market_session_id=? ORDER BY ts_ms DESC LIMIT 1",
        (BOT_ID, sess),
    ).fetchone()
    if not r or r[0] is None:
        return None
    return "UP" if r[0] > r[1] else "DOWN"


def real_stats(con, sessions):
    total_cost = total_realized = total_fee = 0.0
    wins = losses = 0
    correct_first = first_count = 0
    for s in sessions:
        w = winner(con, s)
        if w is None:
            continue
        snap = con.execute(
            "SELECT cost_basis, fee_total, pnl_if_up, pnl_if_down FROM pnl_snapshots "
            "WHERE bot_id=? AND market_session_id=? ORDER BY ts_ms DESC LIMIT 1",
            (BOT_ID, s),
        ).fetchone()
        if not snap:
            continue
        cost, fee, ifu, ifd = snap
        realized = ifu if w == "UP" else ifd
        total_cost += cost
        total_realized += realized
        total_fee += fee or 0
        if realized > 0:
            wins += 1
        else:
            losses += 1
        ft = con.execute(
            "SELECT outcome FROM trades WHERE bot_id=? AND market_session_id=? "
            "ORDER BY ts_ms LIMIT 1",
            (BOT_ID, s),
        ).fetchone()
        if ft:
            first_count += 1
            if ft[0] == w:
                correct_first += 1
    return dict(
        n=wins + losses, wins=wins, losses=losses,
        cost=total_cost, realized=total_realized, fee=total_fee,
        correct_first=correct_first, first_count=first_count,
    )


def sim_session(con, sess):
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
    trades = []

    for ts_ms, ub, ua, db, da in ticks:
        if ub <= 0 or db <= 0 or ua <= 0 or da <= 0:
            continue
        if not book_ready:
            book_ready = True
            last_up, last_dn = ub, db
            continue
        sec_to_end = end_ts - ts_ms / 1000.0

        # --- LATE WINNER ---
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
                trades.append((ts_ms, wd, size, w_ask, "lw"))
                last_buy_ms = ts_ms
                lw_done = True
                continue

        if last_buy_ms > 0 and (ts_ms - last_buy_ms) < BUY_CD_MS:
            last_up, last_dn = ub, db
            continue

        # --- YÖN: SIGNED Δbid ---
        imb = up_filled - dn_filled
        if abs(imb) > IMB_THR:
            dir_ = "DOWN" if imb > 0 else "UP"
        else:
            d_up = ub - last_up
            d_dn = db - last_dn
            if d_up == 0 and d_dn == 0:
                dir_ = "UP" if ub >= db else "DOWN"
            else:
                dir_ = "UP" if d_up > d_dn else "DOWN"

        last_up, last_dn = ub, db
        bid = ub if dir_ == "UP" else db
        ask = ua if dir_ == "UP" else da
        if bid < MIN_PRICE or bid > MAX_PRICE:
            continue
        if bid <= 0.30:
            usdc = SIZE_LONGSHOT
        elif bid <= 0.85:
            usdc = SIZE_MID
        else:
            usdc = SIZE_HIGH
        size = math.ceil(usdc / ask)

        if dir_ == "UP":
            cf, cc, of, oc = up_filled, up_cost, dn_filled, dn_cost
        else:
            cf, cc, of, oc = dn_filled, dn_cost, up_filled, up_cost
        cur_avg = cc / cf if cf > 0 else 0.0
        opp_avg = oc / of if of > 0 else 0.0
        new_avg = (cur_avg * cf + ask * size) / (cf + size) if cf > 0 else ask
        if of > 0 and (new_avg + opp_avg) > MAX_AVG_SUM:
            continue

        cost_t = size * ask
        if dir_ == "UP":
            up_filled += size
            up_cost += cost_t
        else:
            dn_filled += size
            dn_cost += cost_t
        fees += cost_t * FEE_RATE
        trades.append((ts_ms, dir_, size, ask, "buy"))
        last_buy_ms = ts_ms

    cost = up_cost + dn_cost
    if w == "UP":
        realized = up_filled - cost
    else:
        realized = dn_filled - cost
    first = trades[0] if trades else None
    return dict(
        sess=sess, w=w, cost=cost, realized=realized, fees=fees,
        trades=len(trades), upf=up_filled, dnf=dn_filled,
        first_dir=(first[1] if first else None),
        correct_first=(first and first[1] == w),
    )


def main():
    con = sqlite3.connect(DB)
    sessions = [r[0] for r in fetchall(
        con, "SELECT id FROM market_sessions WHERE bot_id=? ORDER BY id", (BOT_ID,)
    )]

    print("=" * 60)
    print(f"BOT {BOT_ID} | {len(sessions)} session | params: LW=$500 thr=0.92 cd=15s")
    print("=" * 60)

    real = real_stats(con, sessions)
    print("\n[1] MEVCUT (|Δbid| absolute) — gerçek bot davranışı")
    print(f"  Sessions:        {real['n']}")
    print(f"  Winrate:         {real['wins']}/{real['n']} = {100*real['wins']/max(1,real['n']):.1f}%")
    print(f"  Total cost:      ${real['cost']:>10,.2f}")
    print(f"  Realized PnL:    ${real['realized']:>+10,.2f}")
    print(f"  Fee (dryrun):    ${real['fee']:.3f}")
    print(f"  NET PnL:         ${real['realized']-real['fee']:>+10,.2f}")
    print(f"  ROI:             {100*(real['realized']-real['fee'])/max(1,real['cost']):>+6.2f}%")
    print(f"  İlk trade doğru: {real['correct_first']}/{real['first_count']} = {100*real['correct_first']/max(1,real['first_count']):.1f}%")

    sim = []
    for s in sessions:
        r = sim_session(con, s)
        if r:
            sim.append(r)
    sw = sum(1 for r in sim if r["realized"] > 0)
    sl = len(sim) - sw
    sc = sum(r["cost"] for r in sim)
    sr = sum(r["realized"] for r in sim)
    sf = sum(r["fees"] for r in sim)
    cf = sum(1 for r in sim if r["correct_first"])
    fc = sum(1 for r in sim if r["first_dir"])

    print("\n[2] SIGNED Δbid — yeni mantık (Δup_signed > Δdn_signed)")
    print(f"  Sessions:        {len(sim)}")
    print(f"  Winrate:         {sw}/{len(sim)} = {100*sw/max(1,len(sim)):.1f}%")
    print(f"  Total cost:      ${sc:>10,.2f}")
    print(f"  Realized PnL:    ${sr:>+10,.2f}")
    print(f"  Fee (sim):       ${sf:.3f}")
    print(f"  NET PnL:         ${sr-sf:>+10,.2f}")
    print(f"  ROI:             {100*(sr-sf)/max(1,sc):>+6.2f}%")
    print(f"  İlk trade doğru: {cf}/{fc} = {100*cf/max(1,fc):.1f}%")

    diff_realized = sr - real["realized"]
    diff_net = (sr - sf) - (real["realized"] - real["fee"])
    print("\n[3] FARK (signed − abs)")
    print(f"  Realized PnL Δ:  ${diff_realized:>+10,.2f}")
    print(f"  NET PnL Δ:       ${diff_net:>+10,.2f}")
    print(f"  Winrate Δ:       {sw - real['wins']:+d} session")
    print(f"  İlk trade Δ:     {cf - real['correct_first']:+d} doğru")

    # Per-session detay
    print("\n[4] Per-session karşılaştırma")
    print(f"  {'sess':>5} {'win':>5} {'real_PnL':>10} {'sim_PnL':>10} {'Δ_PnL':>10} {'1st':>4} {'1st_OK':>6}")
    for r in sim:
        snap = con.execute(
            "SELECT pnl_if_up, pnl_if_down FROM pnl_snapshots "
            "WHERE bot_id=? AND market_session_id=? ORDER BY ts_ms DESC LIMIT 1",
            (BOT_ID, r["sess"]),
        ).fetchone()
        if not snap:
            continue
        rr = snap[0] if r["w"] == "UP" else snap[1]
        d = r["realized"] - rr
        print(f"  {r['sess']:>5} {r['w']:>5} {rr:>+10.2f} {r['realized']:>+10.2f} {d:>+10.2f} "
              f"{r['first_dir'] or '-':>4} {'YES' if r['correct_first'] else 'NO':>6}")


if __name__ == "__main__":
    main()
