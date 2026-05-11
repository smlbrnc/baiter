#!/usr/bin/env python3
"""Bot 108 — FAK BID arbitrage backtest.

Strateji:
  - Winner side (bid > 0.5): FAK BID limit BUY (price=bid, type=FAK)
  - Loser side (bid < 0.5): FAK BID limit BUY (price=bid, type=FAK)
  - FAK = Fill-And-Kill: o anda match olan kısmı dolar, kalanı iptal
  - Toplam ödeme: bid_winner + bid_loser
  - Garanti payoff: $1.00

Fill probability modelleme:
  - %100 fill (optimistik üst sınır)
  - %50 fill (gerçekçi orta)
  - %25 fill (konservatif)
  - Tek leg fill (directional risk — burada test edilmez, kullanıcı uyarısı)

Maker fee = 0; %20 rebate yok (FAK taker olarak işlem görür çünkü post-only değil)
Aslında Polymarket'te FAK aggressive limit = TAKER fee uygulanır (~%2)
"""
import argparse
import math
import sqlite3
from collections import defaultdict
from datetime import datetime, timezone


FEE_RATE = 0.02  # %2 taker (FAK aggressive)


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

    # === [1] bid_winner + bid_loser dağılımı ===
    print("=" * 100)
    print("[1] FAK BID maliyet dağılımı (bid_winner + bid_loser, her tick)")
    print("=" * 100)
    print()
    bins = defaultdict(int)
    arb_ticks = []
    total_eligible = 0
    for s in sessions:
        ticks = con.execute(
            "SELECT ts_ms, up_best_bid, up_best_ask, down_best_bid, down_best_ask "
            "FROM market_ticks WHERE bot_id=? AND market_session_id=?",
            (args.bot_id, s),
        ).fetchall()
        for ts_ms, ub, ua, db, da in ticks:
            if not all(x and x > 0 for x in (ub, ua, db, da)):
                continue
            # Winner = bid > 0.5
            if ub > 0.5 and db <= 0.5:
                w_bid = ub
                l_bid = db
            elif db > 0.5 and ub <= 0.5:
                w_bid = db
                l_bid = ub
            else:
                continue
            total_eligible += 1
            cost = w_bid + l_bid
            if cost < 0.90:
                b = "<0.90"
            elif cost < 0.95:
                b = "0.90-0.95"
            elif cost < 0.97:
                b = "0.95-0.97"
            elif cost < 0.98:
                b = "0.97-0.98"
            elif cost < 0.99:
                b = "0.98-0.99"
            elif cost < 1.00:
                b = "0.99-1.00"
            else:
                b = ">=1.00"
            bins[b] += 1
            if cost < 1.00:
                arb_ticks.append((s, ts_ms, w_bid, l_bid, cost))

    print(f"  Toplam eligible tick (winner_bid > 0.5): {total_eligible:,}")
    print()
    print(f"  {'cost bin':<12} | {'count':>8} | {'oran':>6}")
    print("  " + "-" * 38)
    order = ["<0.90", "0.90-0.95", "0.95-0.97", "0.97-0.98", "0.98-0.99",
             "0.99-1.00", ">=1.00"]
    for b in order:
        if b in bins:
            pct = 100 * bins[b] / max(1, total_eligible)
            marker = " ✅ ARB" if b in ("<0.90", "0.90-0.95", "0.95-0.97",
                                         "0.97-0.98", "0.98-0.99", "0.99-1.00") else ""
            print(f"  {b:<12} | {bins[b]:>8,} | {pct:>5.2f}%{marker}")

    print(f"\n  Arbitrage tick (cost < 1.00): {len(arb_ticks):,}")
    if arb_ticks:
        # En düşük 5
        arb_ticks.sort(key=lambda x: x[4])
        print("\n  En düşük 5 arbitrage fırsatı:")
        for s, ts, wb, lb, c in arb_ticks[:5]:
            print(f"    sess={s} ts={ts} | w_bid={wb:.3f} l_bid={lb:.3f} | cost={c:.4f}")

    # === [2] Backtest — FAK BID arbitrage ===
    print()
    print("=" * 100)
    print("[2] Backtest — FAK BID arbitrage (her tick fırsatı dene)")
    print("=" * 100)
    print()
    print("  NOT: Backtest %100 fill varsayar (üst sınır). Gerçek fill rate %25-50.")
    print()

    # Senaryolar
    scenarios = [
        # (etiket, max_cost, order_usdc, fill_rate)
        ("cost<1.0 + $5  + 100% fill",  1.00, 5,   1.00),
        ("cost<0.99+ $5  + 100% fill",  0.99, 5,   1.00),
        ("cost<0.98+ $5  + 100% fill",  0.98, 5,   1.00),
        ("cost<0.99+ $20 + 100% fill",  0.99, 20,  1.00),
        ("cost<0.99+ $100+ 100% fill",  0.99, 100, 1.00),
        # Gerçekçi fill rate
        ("cost<1.0 + $5  + 50% fill",   1.00, 5,   0.50),
        ("cost<0.99+ $5  + 50% fill",   0.99, 5,   0.50),
        ("cost<0.99+ $20 + 50% fill",   0.99, 20,  0.50),
        ("cost<0.99+ $100+ 50% fill",   0.99, 100, 0.50),
        ("cost<1.0 + $5  + 25% fill",   1.00, 5,   0.25),
        ("cost<0.99+ $20 + 25% fill",   0.99, 20,  0.25),
    ]

    print(f"  {'Senaryo':<40} {'trades':>7} {'cost':>10} {'gross':>9} "
          f"{'fees':>6} {'NET':>9} {'ROI%':>6}")
    print("  " + "-" * 95)

    for label, max_cost, order_usdc, fill_rate in scenarios:
        n_trades = 0
        total_cost = 0.0
        total_gross = 0.0
        total_fees = 0.0
        for s in sessions:
            w = winner_of(con, args.bot_id, s)
            if w is None:
                continue
            ticks = con.execute(
                "SELECT ts_ms, up_best_bid, up_best_ask, down_best_bid, down_best_ask "
                "FROM market_ticks WHERE bot_id=? AND market_session_id=? "
                "ORDER BY ts_ms",
                (args.bot_id, s),
            ).fetchall()
            for ts_ms, ub, ua, db, da in ticks:
                if not all(x and x > 0 for x in (ub, ua, db, da)):
                    continue
                if ub > 0.5 and db <= 0.5:
                    w_side, w_bid, l_bid = "UP", ub, db
                elif db > 0.5 and ub <= 0.5:
                    w_side, w_bid, l_bid = "DOWN", db, ub
                else:
                    continue
                cost_per = w_bid + l_bid
                if cost_per >= max_cost:
                    continue
                # Order size
                size = math.ceil(order_usdc / cost_per)
                # Fill rate uygula
                actual_size = size * fill_rate
                cost_w = actual_size * w_bid
                cost_l = actual_size * l_bid
                cost = cost_w + cost_l
                if cost < 5.0:  # min order size
                    continue
                fees = cost * FEE_RATE
                # PnL: payoff $1/share (ikisinden biri kazanır)
                gross = actual_size * 1.0
                pnl = gross - cost - fees
                n_trades += 1
                total_cost += cost
                total_gross += gross
                total_fees += fees
                break  # tek trade per session

        net = total_gross - total_cost - total_fees
        roi = 100 * net / max(1, total_cost)
        print(f"  {label:<40} {n_trades:>7} {total_cost:>10,.2f} {total_gross:>9,.2f} "
              f"{total_fees:>6.2f} {net:>+9,.2f} {roi:>+6.2f}")

    # === [3] Fee oranı duyarlılık analizi ===
    print()
    print("=" * 100)
    print("[3] Fee duyarlılık (cost<0.99 + $20 + 100% fill)")
    print("=" * 100)
    print()
    print(f"  {'fee':<10} | {'NET':>9} {'ROI%':>6}")
    for fr in [0.000, 0.005, 0.010, 0.015, 0.020, 0.025]:
        n_trades = 0
        total_cost = 0.0
        total_gross = 0.0
        total_fees = 0.0
        for s in sessions:
            w = winner_of(con, args.bot_id, s)
            if w is None:
                continue
            ticks = con.execute(
                "SELECT ts_ms, up_best_bid, up_best_ask, down_best_bid, down_best_ask "
                "FROM market_ticks WHERE bot_id=? AND market_session_id=? "
                "ORDER BY ts_ms",
                (args.bot_id, s),
            ).fetchall()
            for ts_ms, ub, ua, db, da in ticks:
                if not all(x and x > 0 for x in (ub, ua, db, da)):
                    continue
                if ub > 0.5 and db <= 0.5:
                    w_bid, l_bid = ub, db
                elif db > 0.5 and ub <= 0.5:
                    w_bid, l_bid = db, ub
                else:
                    continue
                cost_per = w_bid + l_bid
                if cost_per >= 0.99:
                    continue
                size = math.ceil(20 / cost_per)
                cost = size * cost_per
                if cost < 5:
                    continue
                fees = cost * fr
                gross = size * 1.0
                n_trades += 1
                total_cost += cost
                total_gross += gross
                total_fees += fees
                break
        net = total_gross - total_cost - total_fees
        roi = 100 * net / max(1, total_cost)
        marker = " (maker rebate dahil ~0.4%)" if fr == 0.005 else ""
        print(f"  fee={fr*100:>4.1f}% | {net:>+9.2f} {roi:>+6.2f}{marker}")

    # === [4] Gerçek fill probability tahmini ===
    # Her arbitrage tick'inde, sonraki N tick içinde best_bid değişti mi?
    print()
    print("=" * 100)
    print("[4] FAK BID gerçek fill probability tahmini")
    print("=" * 100)
    print()
    print("  Mantık: arbitrage tick'inde post edilen FAK BID ne kadar olasılıkla fill olur?")
    print("  Yaklaşık: sonraki 3 tick'te best_bid değişti mi (= başkası bizim bid'imizi yedi mi)?")
    print()
    fill_yes = 0
    fill_no = 0
    for s in sessions:
        ticks = con.execute(
            "SELECT ts_ms, up_best_bid, up_best_ask, down_best_bid, down_best_ask "
            "FROM market_ticks WHERE bot_id=? AND market_session_id=? "
            "ORDER BY ts_ms",
            (args.bot_id, s),
        ).fetchall()
        for i, (ts_ms, ub, ua, db, da) in enumerate(ticks):
            if not all(x and x > 0 for x in (ub, ua, db, da)):
                continue
            if ub > 0.5 and db <= 0.5:
                w_bid_initial, l_bid_initial = ub, db
            elif db > 0.5 and ub <= 0.5:
                w_bid_initial, l_bid_initial = db, ub
            else:
                continue
            cost_per = w_bid_initial + l_bid_initial
            if cost_per >= 1.00:
                continue
            # Sonraki 3 tick: bid değişti mi?
            for j in range(i + 1, min(i + 4, len(ticks))):
                ts2, ub2, ua2, db2, da2 = ticks[j]
                if ub > 0.5:  # winner UP idi
                    if ub2 != ub or db2 != db:
                        fill_yes += 1
                        break
                else:
                    if db2 != db or ub2 != ub:
                        fill_yes += 1
                        break
            else:
                fill_no += 1
    total_attempts = fill_yes + fill_no
    fill_pct = 100 * fill_yes / max(1, total_attempts) if total_attempts else 0
    print(f"  Arbitrage attempts: {total_attempts}")
    print(f"  Bid değişti (fill olası): {fill_yes} ({fill_pct:.1f}%)")
    print(f"  Bid değişmedi (fill olmadı): {fill_no} ({100-fill_pct:.1f}%)")
    print(f"\n  Tahmini fill rate: ~{fill_pct:.0f}% (üst sınır)")


if __name__ == "__main__":
    main()
