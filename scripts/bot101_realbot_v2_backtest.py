#!/usr/bin/env python3
"""Bot 101 — RealBot v2 (gerçek bot davranışına yakın) backtest.

6 aşamalı refactor: SAF guard, cooldown 3s, loser scalp, winner pyramid,
multi-LW burst, martingale-down guard. ESKİ vs YENİ tüm bot 101 session'ları
için tick-by-tick simülasyon.
"""
import math
import sqlite3
import sys

DB = sys.argv[1] if len(sys.argv) > 1 else "/home/ubuntu/baiter/data/baiter.db"
BOT_ID = 101

# ESKI mantık parametreleri (deploy önceki defaults)
ESKI = dict(
    cooldown_ms=15_000, lw_secs=30, lw_thr=0.92, lw_usdc=500.0, lw_max=1,
    lw_burst_secs=0, lw_burst_usdc=0,
    imb_thr=200.0, max_avg_sum=1.05, first_spread_min=0.02,
    sz_long=5.0, sz_mid=10.0, sz_high=15.0,
    loser_min_price=0.10, loser_scalp_usdc=0.0,  # KAPALI
    late_pyramid_secs=0, winner_size_factor=1.0,  # KAPALI
    avg_loser_max=2.0,  # KAPALI (asla tetiklenmez)
)

# YENI mantık (RealBot v2 — yeni defaults)
YENI = dict(
    cooldown_ms=3_000, lw_secs=30, lw_thr=0.92, lw_usdc=500.0, lw_max=5,
    lw_burst_secs=12, lw_burst_usdc=200.0,
    imb_thr=50.0, max_avg_sum=1.05, first_spread_min=0.02,
    sz_long=5.0, sz_mid=10.0, sz_high=15.0,
    loser_min_price=0.01, loser_scalp_usdc=1.0,
    late_pyramid_secs=100, winner_size_factor=2.0,
    avg_loser_max=0.50,
)

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


def sim(con, sess, p):
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
        lw_quota_ok = p["lw_max"] == 0 or lw_inj < p["lw_max"]
        if lw_quota_ok and sec_to_end > 0:
            burst_active = (
                p["lw_burst_usdc"] > 0
                and p["lw_burst_secs"] > 0
                and sec_to_end <= p["lw_burst_secs"]
            )
            main_active = (
                p["lw_usdc"] > 0
                and p["lw_secs"] > 0
                and sec_to_end <= p["lw_secs"]
                and not burst_active
            )
            usdc_lw = None
            is_burst = False
            if burst_active:
                usdc_lw = p["lw_burst_usdc"]
                is_burst = True
            elif main_active:
                usdc_lw = p["lw_usdc"]

            if usdc_lw is not None:
                if ub >= db:
                    wd, w_bid, w_ask = "UP", ub, ua
                else:
                    wd, w_bid, w_ask = "DOWN", db, da
                if w_bid >= p["lw_thr"] and w_ask > 0:
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

        if last_buy_ms > 0 and (ts_ms - last_buy_ms) < p["cooldown_ms"]:
            last_up, last_dn = ub, db
            continue

        # Yön
        if not first_done:
            spread = ub - db
            if abs(spread) < p["first_spread_min"]:
                last_up, last_dn = ub, db
                continue
            dir_ = "UP" if spread > 0 else "DOWN"
        else:
            imb = up_filled - dn_filled
            if abs(imb) > p["imb_thr"]:
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

        # Yön bazlı min_price
        if is_loser_dir:
            effective_min = min(p["loser_min_price"], MIN_PRICE)
        else:
            effective_min = MIN_PRICE
        if bid < effective_min or bid > MAX_PRICE:
            continue

        cur_filled = up_filled if dir_ == "UP" else dn_filled
        cur_avg = avg_up if dir_ == "UP" else avg_dn
        opp_filled = dn_filled if dir_ == "UP" else up_filled
        opp_avg = avg_dn if dir_ == "UP" else avg_up

        # Martingale-down guard
        scalp_only = is_loser_dir and cur_filled > 0 and cur_avg > p["avg_loser_max"]

        # Size
        if scalp_only and p["loser_scalp_usdc"] > 0:
            usdc = p["loser_scalp_usdc"]
        elif is_loser_dir and bid < MIN_PRICE and p["loser_scalp_usdc"] > 0:
            usdc = p["loser_scalp_usdc"]
        else:
            if bid <= 0.30:
                base = p["sz_long"]
            elif bid <= 0.85:
                base = p["sz_mid"]
            else:
                base = p["sz_high"]
            if (not is_loser_dir and p["late_pyramid_secs"] > 0
                    and 0 < sec_to_end <= p["late_pyramid_secs"]):
                usdc = base * p["winner_size_factor"]
            else:
                usdc = base

        if usdc <= 0:
            continue
        size = math.ceil(usdc / ask)

        # avg_sum cap (scalp HARİÇ)
        if not scalp_only and opp_filled > 0:
            new_avg = (cur_avg * cur_filled + ask * size) / (cur_filled + size) if cur_filled > 0 else ask
            if new_avg + opp_avg > p["max_avg_sum"]:
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
        if scalp_only or (is_loser_dir and bid < MIN_PRICE):
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


def aggregate(con, sessions, p):
    tot_cost = tot_real = tot_fee = 0.0
    wins = losses = 0
    tot_buys = tot_scalp = tot_lw = tot_burst = 0
    for s in sessions:
        r = sim(con, s, p)
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
    n = wins + losses
    return dict(
        n=n, wins=wins, cost=tot_cost, realized=tot_real, fee=tot_fee,
        net=tot_real - tot_fee,
        roi=100 * (tot_real - tot_fee) / max(1, tot_cost),
        wr=100 * wins / max(1, n),
        n_buys=tot_buys, n_scalp=tot_scalp, n_lw=tot_lw, n_burst=tot_burst,
    )


def main():
    con = sqlite3.connect(DB)
    sessions = [r[0] for r in con.execute(
        "SELECT id FROM market_sessions WHERE bot_id=? ORDER BY id", (BOT_ID,)
    ).fetchall()]
    print("=" * 80)
    print(f"BOT {BOT_ID} | {len(sessions)} session | RealBot v2 vs ESKI mantık")
    print("=" * 80)

    a = aggregate(con, sessions, ESKI)
    b = aggregate(con, sessions, YENI)

    print(f"\n{'Metrik':<22} {'ESKI':>14} {'YENI v2':>14} {'Δ':>14}")
    print("-" * 66)

    def row(label, va, vb, fmt="{:>14.2f}"):
        d = vb - va
        print(f"{label:<22} {fmt.format(va)} {fmt.format(vb)} {fmt.format(d)}")

    row("Sessions", a["n"], b["n"], "{:>14d}")
    row("Trades (BUY)", a["n_buys"], b["n_buys"], "{:>14d}")
    row("Trades (scalp)", a["n_scalp"], b["n_scalp"], "{:>14d}")
    row("Trades (LW main)", a["n_lw"], b["n_lw"], "{:>14d}")
    row("Trades (LW burst)", a["n_burst"], b["n_burst"], "{:>14d}")
    row("Trades TOPLAM",
        a["n_buys"] + a["n_scalp"] + a["n_lw"] + a["n_burst"],
        b["n_buys"] + b["n_scalp"] + b["n_lw"] + b["n_burst"],
        "{:>14d}")
    row("Winrate %", a["wr"], b["wr"])
    row("Cost $", a["cost"], b["cost"])
    row("Realized $", a["realized"], b["realized"])
    row("Fee $", a["fee"], b["fee"], "{:>14.3f}")
    row("NET $", a["net"], b["net"])
    row("ROI %", a["roi"], b["roi"])

    # Per-session karşılaştırma (ilk 3 örnek session)
    print("\n[Per-session detay] 3 örnek session (4908, 4914, 4922)")
    print(f"  {'sess':>5} {'win':>5} {'ESKI_PnL':>10} {'YENI_PnL':>10} {'Δ':>10} "
          f"{'ESKI_n':>7} {'YENI_n':>7}")
    for s in [4908, 4914, 4922]:
        if s not in sessions:
            print(f"  {s:>5} (sessions listesinde yok)")
            continue
        ra = sim(con, s, ESKI)
        rb = sim(con, s, YENI)
        if ra and rb:
            n_a = ra["n_buys"] + ra["n_scalp"] + ra["n_lw"] + ra["n_burst"]
            n_b = rb["n_buys"] + rb["n_scalp"] + rb["n_lw"] + rb["n_burst"]
            d = rb["realized"] - ra["realized"]
            print(f"  {s:>5} {ra['w']:>5} {ra['realized']:>+10.2f} {rb['realized']:>+10.2f} "
                  f"{d:>+10.2f} {n_a:>7} {n_b:>7}")

    # En çok iyileşen / kötüleşen
    diffs = []
    for s in sessions:
        ra = sim(con, s, ESKI)
        rb = sim(con, s, YENI)
        if ra and rb:
            diffs.append((s, ra, rb, rb["realized"] - ra["realized"]))
    diffs.sort(key=lambda t: t[3], reverse=True)
    print("\n[En çok iyileşen 5]")
    for s, ra, rb, d in diffs[:5]:
        print(f"  {s:>5} {ra['w']:>5} ESKI={ra['realized']:>+8.2f} YENI={rb['realized']:>+8.2f} Δ={d:>+8.2f}")
    print("\n[En çok kötüleşen 5]")
    for s, ra, rb, d in diffs[-5:]:
        print(f"  {s:>5} {ra['w']:>5} ESKI={ra['realized']:>+8.2f} YENI={rb['realized']:>+8.2f} Δ={d:>+8.2f}")


if __name__ == "__main__":
    main()
