#!/usr/bin/env python3
"""Bot 101 — İlk emir "spread-gated" varyantı backtest.

Yeni kural (sadece ilk emir için):
  * Idle→Active geçişinde sinyal kaydı başlar (init_up_bid, init_dn_bid).
  * Her tick |up_bid - down_bid| < SPREAD_MIN ise BUY ATILMAZ.
    Bu süre içinde Δup_kum ve Δdn_kum biriktirilir (init'ten beri toplam).
  * |spread| >= SPREAD_MIN olduğunda İLK emir verilir; yön =
       1) signed spread (`up_bid > down_bid` → UP)  — winner momentum
       2) eşitse Δkum → büyük taraf
  * İlk emirden SONRA mantık mevcut akışla aynı (cooldown + Δbid abs +
    imbalance + LW + max_avg_sum cap).

Not: Mevcut bot 101'in canlı verisi kullanılır; tick saniyede bir snapshot
olduğu için sub-second noise gözlenmez (gerçek bot WS event'leriyle daha
hassastır), ancak göreceli kıyas için yeterli.
"""
import math
import sqlite3
import sys

DB = sys.argv[1] if len(sys.argv) > 1 else "/home/ubuntu/baiter/data/baiter.db"
BOT_ID = 101

# Sabit (LIVE_safe_500 + max_avg_sum=1.05)
BUY_CD_MS = 15_000
LW_SECS = 30
LW_THR = 0.92
LW_USDC = 500.0
LW_MAX = 1
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


def sim(con, sess, spread_min):
    """spread_min = 0.0 → mevcut davranış. >0 → spread-gated ilk emir."""
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
    init_up = init_dn = 0.0
    book_ready = False
    first_done = False
    up_filled = dn_filled = 0.0
    up_cost = dn_cost = 0.0
    fees = 0.0
    lw_done = False
    n_buys = 0
    first_dir = None

    for ts_ms, ub, ua, db, da in ticks:
        if ub <= 0 or db <= 0 or ua <= 0 or da <= 0:
            continue
        if not book_ready:
            book_ready = True
            last_up, last_dn = ub, db
            init_up, init_dn = ub, db
            continue
        sec_to_end = end_ts - ts_ms / 1000.0

        # LATE WINNER (her durumda)
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
                continue

        if last_buy_ms > 0 and (ts_ms - last_buy_ms) < BUY_CD_MS:
            last_up, last_dn = ub, db
            continue

        # YÖN
        if not first_done:
            # Spread gate
            spread = ub - db  # signed
            if abs(spread) < spread_min:
                last_up, last_dn = ub, db
                continue
            # İlk emir: signed spread → yüksek bid tarafı (winner momentum)
            if spread > 0:
                dir_ = "UP"
            elif spread < 0:
                dir_ = "DOWN"
            else:
                # spread tam 0 (mümkün değil çünkü >= spread_min)
                d_up = ub - init_up
                d_dn = db - init_dn
                dir_ = "UP" if d_up >= d_dn else "DOWN"
        else:
            # Mevcut mantık (abs Δbid + imbalance)
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

        # avg_sum cap
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
        n_buys += 1
        if first_dir is None:
            first_dir = dir_
            first_done = True

    cost = up_cost + dn_cost
    realized = (up_filled if w == "UP" else dn_filled) - cost
    return dict(
        cost=cost, realized=realized, fees=fees, w=w,
        upf=up_filled, dnf=dn_filled, n_buys=n_buys,
        first_dir=first_dir,
        first_correct=(first_dir == w) if first_dir else None,
    )


def aggregate(con, sessions, spread_min):
    tot_cost = tot_real = tot_fee = 0.0
    wins = losses = 0
    first_correct = first_count = 0
    no_first = 0
    for s in sessions:
        r = sim(con, s, spread_min)
        if r is None:
            continue
        if r["first_dir"] is None:
            no_first += 1
            continue
        tot_cost += r["cost"]
        tot_real += r["realized"]
        tot_fee += r["fees"]
        if r["realized"] > 0:
            wins += 1
        else:
            losses += 1
        first_count += 1
        if r["first_correct"]:
            first_correct += 1
    n = wins + losses
    return dict(
        n=n, wins=wins, no_first=no_first,
        cost=tot_cost, realized=tot_real, fee=tot_fee,
        net=tot_real - tot_fee,
        roi=100 * (tot_real - tot_fee) / max(1, tot_cost),
        wr=100 * wins / max(1, n),
        first_acc=100 * first_correct / max(1, first_count),
        first_correct=first_correct, first_count=first_count,
    )


def main():
    con = sqlite3.connect(DB)
    sessions = [r[0] for r in con.execute(
        "SELECT id FROM market_sessions WHERE bot_id=? ORDER BY id", (BOT_ID,)
    ).fetchall()]
    print(f"BOT {BOT_ID} | {len(sessions)} session | spread-gated first order backtest")
    print(f"Sabit: max_avg_sum=1.05, LW=$500 thr=0.92 cd=15s sizes=5/10/15\n")

    print(f"{'spread_min':>10} | {'sessions':>8} {'no_first':>9} {'WR%':>6} "
          f"{'1st_acc%':>9} {'cost':>10} {'realized':>10} {'NET':>10} {'ROI%':>7}")
    print("-" * 95)
    for sm in [0.00, 0.01, 0.02, 0.03, 0.05]:
        r = aggregate(con, sessions, sm)
        marker = " ← MEVCUT" if sm == 0.00 else ""
        print(f"{sm:>10.2f} | {r['n']:>8} {r['no_first']:>9} {r['wr']:>6.1f} "
              f"{r['first_acc']:>9.1f} {r['cost']:>10,.2f} {r['realized']:>+10,.2f} "
              f"{r['net']:>+10,.2f} {r['roi']:>+7.2f}{marker}")

    spread_for_detail = 0.02

    # Pozisyon tipi (KARMA / SAF_UP / SAF_DOWN) bazlı kırılım
    print(f"\n[Pozisyon tipi kırılımı] spread_min={spread_for_detail}")
    buckets = {"SAF_UP": [], "SAF_DOWN": [], "KARMA": []}
    for s in sessions:
        r = sim(con, s, spread_for_detail)
        if not r or not r["first_dir"]:
            continue
        if r["upf"] > 0 and r["dnf"] == 0:
            ptype = "SAF_UP"
        elif r["dnf"] > 0 and r["upf"] == 0:
            ptype = "SAF_DOWN"
        else:
            ptype = "KARMA"
        buckets[ptype].append(r)

    print(f"  {'tip':<10} {'n':>4} {'WR%':>6} {'cost':>10} {'realized':>10} {'fee':>7} {'NET':>10} {'ROI%':>7}")
    print("  " + "-" * 70)
    overall = {"n": 0, "wins": 0, "cost": 0, "realized": 0, "fee": 0}
    for tip, items in buckets.items():
        if not items:
            continue
        wins = sum(1 for r in items if r["realized"] > 0)
        cost = sum(r["cost"] for r in items)
        real = sum(r["realized"] for r in items)
        fee = sum(r["fees"] for r in items)
        net = real - fee
        wr = 100 * wins / len(items)
        roi = 100 * net / max(1, cost)
        print(f"  {tip:<10} {len(items):>4} {wr:>6.1f} {cost:>10,.2f} {real:>+10,.2f} {fee:>7.3f} {net:>+10,.2f} {roi:>+7.2f}")
        overall["n"] += len(items); overall["wins"] += wins
        overall["cost"] += cost; overall["realized"] += real; overall["fee"] += fee
    if overall["n"]:
        net = overall["realized"] - overall["fee"]
        wr = 100 * overall["wins"] / overall["n"]
        roi = 100 * net / max(1, overall["cost"])
        print(f"  {'TOPLAM':<10} {overall['n']:>4} {wr:>6.1f} {overall['cost']:>10,.2f} {overall['realized']:>+10,.2f} {overall['fee']:>7.3f} {net:>+10,.2f} {roi:>+7.2f}")

    # SAF tip session örnekleri
    print(f"\n[SAF (tek taraf) session örnekleri] spread_min={spread_for_detail}")
    print(f"  {'sess':>5} {'tip':>9} {'win':>5} {'1st':>4} {'1st_OK':>7} {'cost':>9} {'realized':>10}")
    saf = buckets["SAF_UP"] + buckets["SAF_DOWN"]
    for r in saf:
        # bu r dict'ini sess id ile eşleştirmek için tekrar oluşturalım
        pass
    # sess ile eşleştir
    saf_with_sess = []
    for s in sessions:
        rr = sim(con, s, spread_for_detail)
        if not rr or not rr["first_dir"]: continue
        if rr["upf"] > 0 and rr["dnf"] == 0:
            saf_with_sess.append((s, "SAF_UP", rr))
        elif rr["dnf"] > 0 and rr["upf"] == 0:
            saf_with_sess.append((s, "SAF_DOWN", rr))
    saf_with_sess.sort(key=lambda t: t[2]["realized"])
    for s, tip, r in saf_with_sess:
        print(f"  {s:>5} {tip:>9} {r['w']:>5} {r['first_dir']:>4} {'YES' if r['first_correct'] else 'NO':>7} "
              f"{r['cost']:>9.2f} {r['realized']:>+10.2f}")

    print(f"\n[Per-session detay (top/bottom 5)] spread_min={spread_for_detail}")
    print(f"  {'sess':>5} {'win':>5} {'1st':>4} {'1st_OK':>7} {'real':>8} {'sim':>8} {'Δ':>8}")
    base_results = {}
    sm_results = {}
    for s in sessions:
        b = sim(con, s, 0.0)
        x = sim(con, s, spread_for_detail)
        base_results[s] = b
        sm_results[s] = x
    diffs = []
    for s in sessions:
        b = base_results[s]; x = sm_results[s]
        if not (b and x and b["first_dir"] and x["first_dir"]): continue
        d = x["realized"] - b["realized"]
        diffs.append((s, x, b, d))
    diffs.sort(key=lambda t: t[3], reverse=True)
    print("  En çok iyileşen 5 session:")
    for s, x, b, d in diffs[:5]:
        print(f"  {s:>5} {x['w']:>5} {x['first_dir']:>4} {'YES' if x['first_correct'] else 'NO':>7} "
              f"{b['realized']:>+8.2f} {x['realized']:>+8.2f} {d:>+8.2f}")
    print("  En çok kötüleşen 5 session:")
    for s, x, b, d in diffs[-5:]:
        print(f"  {s:>5} {x['w']:>5} {x['first_dir']:>4} {'YES' if x['first_correct'] else 'NO':>7} "
              f"{b['realized']:>+8.2f} {x['realized']:>+8.2f} {d:>+8.2f}")


if __name__ == "__main__":
    main()
