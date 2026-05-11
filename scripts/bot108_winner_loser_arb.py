#!/usr/bin/env python3
"""Bot 108 — Winner ASK + Loser BID arbitrage backtest.

Strateji:
  - bid > 0.5 olan taraf = WINNER side (yükselen)
  - bid < 0.5 olan taraf = LOSER side (düşen)
  - Winner side: ASK fiyatından TAKER BUY (hemen fill)
  - Loser side: BID fiyatından MAKER limit BUY (post-only, fill bekle)

Cost analizi:
  - Total ödenen = ask_winner + bid_loser (eğer ikisi de fill olursa)
  - Garanti payoff = $1.00 (biri kazanır)
  - Net = 1.00 - (ask_w + bid_l) - taker_fee

Maker fee = 0 (Polymarket policy), %20 maker rebate var.
Taker fee yaklaşık ~%2 (price-dependent).

Senaryolar:
  A) Sadece taker (winner ask + loser ask) — baseline
  B) Hibrit (winner ask + loser bid, both fill assumed)
  C) Hibrit + minimum profit threshold (>= $0.01 kar olunca)
"""
import argparse
import math
import sqlite3
from collections import defaultdict


FEE_RATE_TAKER = 0.02  # %2 (Liu paper'a göre)
FEE_RATE_MAKER = 0.0   # Polymarket maker fee = 0
MAKER_REBATE = 0.20    # crypto markets, %20 of taker fee
MIN_PROFIT_USDC = 0.01  # Senaryo C için minimum profit threshold


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

    # === [1] Tick analizi: Winner_ask + Loser_bid dağılımı ===
    print("=" * 100)
    print("[1] Winner_ASK + Loser_BID dağılımı (her tick)")
    print("=" * 100)
    print()

    # Tüm ticklerde:
    #   winner_side = bid > 0.5 olan
    #   trio: ask_winner + bid_loser
    bins = defaultdict(int)
    fee_adj_bins = defaultdict(int)  # %2 fee düştükten sonra
    detail_arb = []  # arb fırsatları
    total = 0
    for s in sessions:
        ticks = con.execute(
            "SELECT ts_ms, up_best_bid, up_best_ask, down_best_bid, down_best_ask "
            "FROM market_ticks WHERE bot_id=? AND market_session_id=?",
            (args.bot_id, s),
        ).fetchall()
        for ts_ms, ub, ua, db, da in ticks:
            if not all(x and x > 0 for x in (ub, ua, db, da)):
                continue
            total += 1
            # Winner = bid > 0.5
            if ub > 0.5 and db <= 0.5:
                w_ask = ua
                l_bid = db
                winner_side = "UP"
            elif db > 0.5 and ub <= 0.5:
                w_ask = da
                l_bid = ub
                winner_side = "DOWN"
            else:
                # Belirsiz (bid_up + bid_dn < 1, ikisi de < 0.5 veya > 0.5)
                continue

            cost = w_ask + l_bid
            # Bin
            if cost < 0.95:
                b = "<0.95"
            elif cost < 0.97:
                b = "0.95-0.97"
            elif cost < 0.98:
                b = "0.97-0.98"
            elif cost < 0.99:
                b = "0.98-0.99"
            elif cost < 1.00:
                b = "0.99-1.00"
            elif cost < 1.01:
                b = "1.00-1.01"
            else:
                b = ">=1.01"
            bins[b] += 1
            # Fee adjusted: taker on winner_ask + maker(0) on loser_bid + winner payoff
            # Net per $1 cost = (1 - cost) - taker_fee_on_ask + maker_rebate
            taker_fee = w_ask * FEE_RATE_TAKER
            maker_rebate = taker_fee * MAKER_REBATE
            net_per_unit = 1.0 - cost - taker_fee + maker_rebate
            if net_per_unit > 0:
                detail_arb.append((s, ts_ms, winner_side, w_ask, l_bid, cost, net_per_unit))

    print(f"  Toplam analiz edilen tick: {total:,}")
    print()
    print(f"  {'cost bin':<12} | {'count':>8} | {'oran':>6}")
    print("  " + "-" * 38)
    order = ["<0.95", "0.95-0.97", "0.97-0.98", "0.98-0.99", "0.99-1.00",
             "1.00-1.01", ">=1.01"]
    for b in order:
        if b in bins:
            pct = 100 * bins[b] / max(1, total)
            marker = " ✅ ARB" if b in ("<0.95", "0.95-0.97", "0.97-0.98", "0.98-0.99", "0.99-1.00") else ""
            print(f"  {b:<12} | {bins[b]:>8,} | {pct:>5.2f}%{marker}")

    print(f"\n  Net pozitif (fee+rebate dahil) tick sayısı: {len(detail_arb):,}")
    if detail_arb:
        max_arb = max(detail_arb, key=lambda x: x[6])
        print(f"  En büyük arb marjı: ${max_arb[6]:.4f} per $1 (sess={max_arb[0]} winner={max_arb[2]})")

    # === [2] Backtest — bu fırsatları gerçekten alalım ===
    print()
    print("=" * 100)
    print("[2] Backtest — winner_ask + loser_bid arbitrage")
    print("=" * 100)
    print()

    # Senaryolar
    scenarios = [
        # (etiket, max_cost_threshold, order_usdc)
        ("Tüm fırsatlar (cost < 1.0)", 1.00, 5),
        ("cost < 0.99", 0.99, 5),
        ("cost < 0.98", 0.98, 5),
        ("cost < 0.97", 0.97, 5),
        ("cost < 0.99 + $20 order", 0.99, 20),
        ("cost < 0.99 + $100 order", 0.99, 100),
    ]

    print(f"  {'Senaryo':<35} {'trades':>7} {'cost':>10} {'gross':>9} {'fees':>7} "
          f"{'NET':>9} {'ROI%':>6}")
    print("  " + "-" * 95)

    for label, max_cost, order_usdc in scenarios:
        n_trades = 0
        total_cost = 0.0
        total_gross = 0.0  # PnL (winner share ödenir = $1)
        total_fees = 0.0
        # Her tick'te tek trade per session — pencere içinde 1 fırsat yeter
        triggered = set()
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
                    w_side, w_ask, l_bid = "UP", ua, db
                elif db > 0.5 and ub <= 0.5:
                    w_side, w_ask, l_bid = "DOWN", da, ub
                else:
                    continue
                cost_per_unit = w_ask + l_bid
                if cost_per_unit >= max_cost:
                    continue
                # Buradayız → arbitrage al
                size = math.ceil(order_usdc / cost_per_unit)
                # 2 trade: winner ASK BUY + loser BID maker BUY (assume both fill)
                cost_w = size * w_ask
                cost_l = size * l_bid
                cost_total = cost_w + cost_l
                # Fees
                taker_fee = cost_w * FEE_RATE_TAKER
                maker_fee = cost_l * FEE_RATE_MAKER  # 0
                rebate = taker_fee * MAKER_REBATE
                fees = taker_fee + maker_fee - rebate
                # Min order size
                if cost_w < 5.0 or cost_l < 5.0:
                    continue
                # Gross PnL: winner side share = $size, loser side $0
                if w == w_side:
                    gross = size * 1.0  # winner pays
                else:
                    # Beklenen winner farklı çıkmış → loser lottery kazandı
                    gross = size * 1.0  # kim kazanırsa o $1
                # Aslında: w_side bizim aldığımız taraf; gerçek winner farklı olabilir
                # Doğrusu: gerçek winner = w (final). Bizim 2 share alımımız.
                # winner share + loser share = $size + $0 = $size (her zaman)
                # Çünkü ikisinden biri kazanır $1, diğeri $0
                # Yani gross her zaman $size!
                pnl = gross - cost_total - fees
                if pnl > -10:  # makul aralık
                    n_trades += 1
                    total_cost += cost_total
                    total_gross += gross
                    total_fees += fees
                    triggered.add(s)
                break  # tek trade per session

        net = total_gross - total_cost - total_fees
        roi = 100 * net / max(1, total_cost)
        print(f"  {label:<35} {n_trades:>7} {total_cost:>10,.2f} {total_gross:>9,.2f} "
              f"{total_fees:>7.2f} {net:>+9,.2f} {roi:>+6.2f}")

    # === [3] Detay: en iyi senaryoda fırsat dağılımı ===
    print()
    print("=" * 100)
    print("[3] cost < 1.0 senaryosunda saatlik dağılım (order=$5)")
    print("=" * 100)
    print()

    hourly = defaultdict(lambda: {"n": 0, "pnl": 0.0, "cost": 0.0})
    total_n = 0
    for s in sessions:
        w = winner_of(con, args.bot_id, s)
        if w is None:
            continue
        ms = con.execute(
            "SELECT start_ts FROM market_sessions WHERE id=?", (s,)
        ).fetchone()
        start_ts = ms[0]
        from datetime import datetime, timezone
        hr = datetime.fromtimestamp(start_ts, tz=timezone.utc).strftime("%H")
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
                w_ask, l_bid = ua, db
            elif db > 0.5 and ub <= 0.5:
                w_ask, l_bid = da, ub
            else:
                continue
            cost_per = w_ask + l_bid
            if cost_per >= 1.00:
                continue
            size = math.ceil(5.0 / cost_per)
            cost_w = size * w_ask
            cost_l = size * l_bid
            if cost_w < 5.0 or cost_l < 5.0:
                continue
            taker_fee = cost_w * FEE_RATE_TAKER
            rebate = taker_fee * MAKER_REBATE
            fees = taker_fee - rebate
            cost = cost_w + cost_l
            pnl = size * 1.0 - cost - fees
            hourly[hr]["n"] += 1
            hourly[hr]["pnl"] += pnl
            hourly[hr]["cost"] += cost
            total_n += 1
            break

    print(f"  {'saat':<5} {'n':>4} {'cost':>9} {'pnl':>9} {'ROI%':>6}")
    for hr in sorted(hourly.keys()):
        h = hourly[hr]
        roi = 100 * h["pnl"] / max(1, h["cost"])
        print(f"  {hr:<5} {h['n']:>4} {h['cost']:>9,.2f} {h['pnl']:>+9,.2f} {roi:>+6.2f}")
    print(f"\n  Toplam {total_n} session'da arbitrage fırsatı bulundu")


if __name__ == "__main__":
    main()
