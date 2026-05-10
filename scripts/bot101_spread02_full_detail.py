#!/usr/bin/env python3
"""Bot 101 — spread_min=0.02 + yüksek bid tarafına başla mantığı.

Tek bir senaryoyu detaylı çıkarır:
  * Spread (|up_bid - down_bid|) >= 0.02 olana kadar BUY atılmaz
  * Spread eşiği aşılınca yön = signed spread (yüksek bid tarafı)
  * İlk emirden sonra mevcut akış (cooldown, abs Δbid, imbalance, LW, max_avg_sum=1.05)

Çıktılar:
  1) Genel toplam (WR, ROI, NET)
  2) Pozisyon tipi (SAF_UP / SAF_DOWN / KARMA) kırılımı
  3) Her session: ilk emir yönü, gerçek winner, ilk emir doğru mu, PnL
  4) "İlk emir DOWN + winner DOWN" alt kümesi
  5) "İlk emir UP + winner UP" alt kümesi
"""
import math
import sqlite3
import sys

DB = sys.argv[1] if len(sys.argv) > 1 else "/home/ubuntu/baiter/data/baiter.db"
BOT_ID = 101
SPREAD_MIN = 0.02

# Sabit (LIVE_safe_500 + max_avg_sum=1.05)
BUY_CD_MS = 15_000
LW_SECS = 30
LW_THR = 0.92
LW_USDC = 500.0
IMB_THR = 200.0
MAX_AVG_SUM = 1.05
SIZE_LONGSHOT = 5.0
SIZE_MID = 10.0
SIZE_HIGH = 15.0
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


def sim(con, sess):
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
    lw_done = False
    first_dir = None
    first_spread = None
    first_ts_ms = None

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
                    up_filled += size; up_cost += cost_t
                else:
                    dn_filled += size; dn_cost += cost_t
                fees += cost_t * FEE_RATE
                last_buy_ms = ts_ms
                lw_done = True
                if first_dir is None:
                    first_dir = wd
                    first_done = True
                    first_spread = ub - db
                    first_ts_ms = ts_ms
                continue

        if last_buy_ms > 0 and (ts_ms - last_buy_ms) < BUY_CD_MS:
            last_up, last_dn = ub, db
            continue

        if not first_done:
            spread = ub - db
            if abs(spread) < SPREAD_MIN:
                last_up, last_dn = ub, db
                continue
            dir_ = "UP" if spread > 0 else "DOWN"
            first_spread = spread
            first_ts_ms = ts_ms
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
            up_filled += size; up_cost += cost_t
        else:
            dn_filled += size; dn_cost += cost_t
        fees += cost_t * FEE_RATE
        last_buy_ms = ts_ms
        if first_dir is None:
            first_dir = dir_
            first_done = True

    cost = up_cost + dn_cost
    realized = (up_filled if w == "UP" else dn_filled) - cost
    net = realized - fees
    return dict(
        sess=sess, w=w, cost=cost, realized=realized, fees=fees, net=net,
        upf=up_filled, dnf=dn_filled,
        first_dir=first_dir, first_correct=(first_dir == w) if first_dir else None,
        first_spread=first_spread, first_ts_ms=first_ts_ms,
    )


def fmt_summary(label, items):
    n = len(items)
    if n == 0:
        return f"  {label:<22} {0:>3}"
    wins = sum(1 for r in items if r["realized"] > 0)
    cost = sum(r["cost"] for r in items)
    real = sum(r["realized"] for r in items)
    fee = sum(r["fees"] for r in items)
    net = real - fee
    wr = 100 * wins / n
    roi = 100 * net / max(1, cost)
    return (f"  {label:<22} {n:>3} | WR {wr:>5.1f}% | "
            f"cost ${cost:>10,.2f} | realized ${real:>+10,.2f} | "
            f"NET ${net:>+10,.2f} | ROI {roi:>+6.2f}%")


def main():
    con = sqlite3.connect(DB)
    sessions = [r[0] for r in con.execute(
        "SELECT id FROM market_sessions WHERE bot_id=? ORDER BY id", (BOT_ID,)
    ).fetchall()]
    print("=" * 95)
    print(f"BOT {BOT_ID} | spread_min={SPREAD_MIN} + yüksek bid tarafına başla")
    print(f"Sabit: max_avg_sum=1.05, LW=$500 thr=0.92 cd=15s sizes=5/10/15")
    print("=" * 95)

    results = [r for r in (sim(con, s) for s in sessions) if r is not None]

    print("\n[1] Genel özet")
    print(fmt_summary("TÜM SESSION", results))

    print("\n[2] Pozisyon tipi kırılımı")
    saf_up = [r for r in results if r["upf"] > 0 and r["dnf"] == 0]
    saf_dn = [r for r in results if r["dnf"] > 0 and r["upf"] == 0]
    karma = [r for r in results if r["upf"] > 0 and r["dnf"] > 0]
    print(fmt_summary("KARMA (UP+DOWN)", karma))
    print(fmt_summary("SAF_UP (sadece UP)", saf_up))
    print(fmt_summary("SAF_DOWN (sadece DN)", saf_dn))

    print("\n[3] İlk emir doğruluk × Winner kırılımı")
    quad = {("UP", "UP"): [], ("UP", "DOWN"): [],
            ("DOWN", "UP"): [], ("DOWN", "DOWN"): []}
    for r in results:
        if not r["first_dir"]:
            continue
        quad[(r["first_dir"], r["w"])].append(r)
    for (fd, w), items in quad.items():
        label = f"1st={fd:<4} winner={w:<4}"
        ok_str = "DOĞRU" if fd == w else "YANLIŞ"
        print(f"  {ok_str:<7} | {fmt_summary(label, items).strip()}")

    print("\n[4] Tüm session detayı")
    print(f"  {'sess':>5} {'w':>5} {'1st':>4} {'1st_OK':>7} {'spread':>7} "
          f"{'cost':>9} {'real':>9} {'NET':>9} {'up_sh':>6} {'dn_sh':>6}")
    for r in results:
        ok = "YES" if r["first_correct"] else ("NO" if r["first_correct"] is False else "-")
        sp = f"{r['first_spread']:+.3f}" if r["first_spread"] is not None else "-"
        print(f"  {r['sess']:>5} {r['w']:>5} {r['first_dir'] or '-':>4} {ok:>7} {sp:>7} "
              f"{r['cost']:>9.2f} {r['realized']:>+9.2f} {r['net']:>+9.2f} "
              f"{r['upf']:>6.0f} {r['dnf']:>6.0f}")

    print("\n[5] İlk emir DOWN + winner DOWN alt kümesi")
    sub_dd = quad[("DOWN", "DOWN")]
    print(fmt_summary("1st=DOWN, winner=DOWN", sub_dd))
    if sub_dd:
        print(f"  {'sess':>5} {'cost':>9} {'realized':>10} {'NET':>9} {'up_sh':>6} {'dn_sh':>6}")
        for r in sub_dd:
            print(f"  {r['sess']:>5} {r['cost']:>9.2f} {r['realized']:>+10.2f} "
                  f"{r['net']:>+9.2f} {r['upf']:>6.0f} {r['dnf']:>6.0f}")

    print("\n[6] İlk emir UP + winner UP alt kümesi")
    sub_uu = quad[("UP", "UP")]
    print(fmt_summary("1st=UP, winner=UP", sub_uu))
    if sub_uu:
        print(f"  {'sess':>5} {'cost':>9} {'realized':>10} {'NET':>9} {'up_sh':>6} {'dn_sh':>6}")
        for r in sub_uu:
            print(f"  {r['sess']:>5} {r['cost']:>9.2f} {r['realized']:>+10.2f} "
                  f"{r['net']:>+9.2f} {r['upf']:>6.0f} {r['dnf']:>6.0f}")


if __name__ == "__main__":
    main()
