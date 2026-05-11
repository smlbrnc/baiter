#!/usr/bin/env python3
"""Bot 111 — her trade'i tek tek karşılaştır.

Her gerçek trade için:
  - Trade hangi yön (UP/DOWN)
  - Hangi fiyatta gerçekleşmiş
  - O anki tick'te bid/ask ne idi
  - Bu trade winner mi loser mi
  - Final outcome (gerçek winner side $1, loser $0)
  - PnL: trade_size - cost (winner ise +, loser ise -)
"""
import sqlite3
import sys
import argparse


def winner_of(con, bot_id, sess):
    r = con.execute(
        "SELECT up_best_bid, down_best_bid FROM market_ticks "
        "WHERE bot_id=? AND market_session_id=? ORDER BY ts_ms DESC LIMIT 1",
        (bot_id, sess),
    ).fetchone()
    if not r or r[0] is None: return None
    ub, db = r[0] or 0.0, r[1] or 0.0
    if ub > 0.95: return "UP"
    if db > 0.95: return "DOWN"
    return None


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("bot_id", type=int)
    ap.add_argument("db", nargs="?", default="/home/ubuntu/baiter/data/baiter.db")
    args = ap.parse_args()

    con = sqlite3.connect(args.db)

    # Her trade için PnL hesabı
    trades = con.execute(
        """SELECT t.market_session_id sess, t.outcome, t.size, t.price, t.fee, t.ts_ms,
                  ms.start_ts, ms.end_ts
           FROM trades t
           JOIN market_sessions ms ON ms.id = t.market_session_id
           WHERE t.bot_id = ? ORDER BY t.ts_ms""",
        (args.bot_id,),
    ).fetchall()

    print(f"BOT {args.bot_id} | {len(trades)} trade")
    print()

    # Session başına grupla
    from collections import defaultdict
    by_sess = defaultdict(list)
    for sess, outcome, size, price, fee, ts_ms, start_ts, end_ts in trades:
        by_sess[sess].append(dict(
            outcome=outcome, size=size, price=price, fee=fee or 0.0,
            ts_ms=ts_ms, sec_from_start=ts_ms // 1000 - start_ts,
        ))

    print(f"{'sess':>5} {'winner':>6} {'n':>3} {'cost':>7} {'pay':>6} {'fee':>5} {'NET':>8} {'KARMA?':>8}")
    print("-" * 60)
    total_cost = total_payoff = total_fees = 0.0
    saf = karma = 0
    for sess, ts in by_sess.items():
        w = winner_of(con, args.bot_id, sess)
        if w is None:
            continue
        cost = sum(t["size"] * t["price"] for t in ts)
        fees = sum(t["fee"] for t in ts)
        # Payoff: winner side share = $1
        payoff = sum(t["size"] for t in ts if t["outcome"] == w)
        net = payoff - cost - fees
        up_size = sum(t["size"] for t in ts if t["outcome"] == "UP")
        dn_size = sum(t["size"] for t in ts if t["outcome"] == "DOWN")
        is_karma = up_size > 0 and dn_size > 0
        if is_karma: karma += 1
        else: saf += 1
        total_cost += cost
        total_payoff += payoff
        total_fees += fees
        marker = "✅K" if is_karma else "❌S"
        print(f"{sess:>5} {w:>6} {len(ts):>3} {cost:>7.2f} {payoff:>6.2f} "
              f"{fees:>5.4f} {net:>+8.2f} {marker:>8}")
    print()
    print(f"TOPLAM cost: ${total_cost:.2f}")
    print(f"TOPLAM payoff: ${total_payoff:.2f}")
    print(f"TOPLAM fees: ${total_fees:.2f}")
    print(f"NET (cost'tan hesap): ${total_payoff - total_cost - total_fees:.2f}")
    print(f"KARMA: {karma}, SAF: {saf}")

    # UI değeri (canonical bid > 0.95)
    ui_pnl = con.execute(
        """SELECT SUM(CASE WHEN lt.up_best_bid > 0.95 THEN p.pnl_if_up
                           WHEN lt.down_best_bid > 0.95 THEN p.pnl_if_down ELSE NULL END)
           FROM market_sessions s
           LEFT JOIN pnl_snapshots p ON p.market_session_id = s.id
              AND p.ts_ms = (SELECT MAX(ts_ms) FROM pnl_snapshots WHERE market_session_id=s.id)
           LEFT JOIN market_ticks lt ON lt.market_session_id = s.id
              AND lt.ts_ms = (SELECT MAX(ts_ms) FROM market_ticks WHERE market_session_id=s.id)
           WHERE s.bot_id=?""", (args.bot_id,)
    ).fetchone()[0] or 0.0
    print(f"\nUI canonical PnL: ${ui_pnl:.2f}")
    print(f"Trade-tabanlı NET: ${total_payoff - total_cost - total_fees:.2f}")
    print(f"Fark: ${ui_pnl - (total_payoff - total_cost - total_fees):.2f}")


if __name__ == "__main__":
    main()
