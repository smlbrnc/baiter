#!/usr/bin/env python3
"""Canonical Bot PnL doğrulama / backtest aracı.

UI'nın gösterdiği "Toplam K/Z" değerini birebir replicate eder.
Backend kuralı (src/db/sessions.rs::total_pnl_for_bot):

    SUM(
      CASE
        WHEN ub > 0.95 THEN pnl_if_up      -- UP kesin kazandı
        WHEN db > 0.95 THEN pnl_if_down    -- DOWN kesin kazandı
        ELSE NULL                          -- henüz çözülmemiş, hariç
      END
    )

Bu aracı her zaman backtest karşılaştırması ve PnL doğrulamasında kullan.

Kullanım:
    python3 bot_pnl_verify.py [BOT_ID] [DB_PATH] [--detail]

Örnek:
    python3 bot_pnl_verify.py 108
    python3 bot_pnl_verify.py 108 /home/ubuntu/baiter/data/baiter.db --detail
"""
import argparse
import sqlite3
import sys


def fetch_total_pnl(con, bot_id: int) -> float | None:
    """Backend SQL ile birebir aynı sorgu — UI değerini döner."""
    row = con.execute(
        """
        SELECT SUM(
            CASE
                WHEN lt.up_best_bid   > 0.95 THEN p.pnl_if_up
                WHEN lt.down_best_bid > 0.95 THEN p.pnl_if_down
                ELSE NULL
            END
        ) AS total_pnl
        FROM market_sessions s
        LEFT JOIN pnl_snapshots p
               ON p.market_session_id = s.id
              AND p.ts_ms = (SELECT MAX(ts_ms) FROM pnl_snapshots
                              WHERE market_session_id = s.id)
        LEFT JOIN market_ticks lt
               ON lt.market_session_id = s.id
              AND lt.ts_ms = (SELECT MAX(ts_ms) FROM market_ticks
                               WHERE market_session_id = s.id)
        WHERE s.bot_id = ?
        """,
        (bot_id,),
    ).fetchone()
    return row[0]


def fetch_session_breakdown(con, bot_id: int):
    """Her session için: bid'ler, pnl_if_up/down, kullanılan değer, winner."""
    return con.execute(
        """
        SELECT s.id, s.slug, s.start_ts, s.end_ts,
               lt.up_best_bid, lt.down_best_bid,
               p.pnl_if_up, p.pnl_if_down, p.cost_basis, p.fee_total,
               p.up_filled, p.down_filled, p.avg_up, p.avg_down
        FROM market_sessions s
        LEFT JOIN pnl_snapshots p
               ON p.market_session_id = s.id
              AND p.ts_ms = (SELECT MAX(ts_ms) FROM pnl_snapshots
                              WHERE market_session_id = s.id)
        LEFT JOIN market_ticks lt
               ON lt.market_session_id = s.id
              AND lt.ts_ms = (SELECT MAX(ts_ms) FROM market_ticks
                               WHERE market_session_id = s.id)
        WHERE s.bot_id = ?
        ORDER BY s.start_ts
        """,
        (bot_id,),
    ).fetchall()


def fetch_bot_meta(con, bot_id: int):
    return con.execute(
        "SELECT name, strategy, run_mode, state, order_usdc, min_price, max_price "
        "FROM bots WHERE id = ?",
        (bot_id,),
    ).fetchone()


def fetch_trades_summary(con, bot_id: int):
    return con.execute(
        """SELECT COUNT(*) trades, ROUND(SUM(size*price),2) notional,
                  ROUND(SUM(fee),3) fee
           FROM trades WHERE bot_id = ?""",
        (bot_id,),
    ).fetchone()


def fetch_reason_breakdown(con, bot_id: int):
    """Strategy reason etiketi başına trade sayısı (logs'tan)."""
    return con.execute(
        """SELECT
              SUM(CASE WHEN message LIKE '%bonereaper:buy:up%' THEN 1 ELSE 0 END) buy_up,
              SUM(CASE WHEN message LIKE '%bonereaper:buy:down%' THEN 1 ELSE 0 END) buy_dn,
              SUM(CASE WHEN message LIKE '%bonereaper:scalp:up%' THEN 1 ELSE 0 END) scalp_up,
              SUM(CASE WHEN message LIKE '%bonereaper:scalp:down%' THEN 1 ELSE 0 END) scalp_dn,
              SUM(CASE WHEN message LIKE '%bonereaper:lw:%' THEN 1 ELSE 0 END) lw_main,
              SUM(CASE WHEN message LIKE '%bonereaper:lwb:%' THEN 1 ELSE 0 END) lw_burst
           FROM logs WHERE bot_id = ? AND message LIKE '%bonereaper:%'""",
        (bot_id,),
    ).fetchone()


def main():
    ap = argparse.ArgumentParser(description="Bot PnL canonical doğrulama / breakdown")
    ap.add_argument("bot_id", type=int, help="Bot ID (DB)")
    ap.add_argument(
        "db",
        nargs="?",
        default="/home/ubuntu/baiter/data/baiter.db",
        help="SQLite DB path",
    )
    ap.add_argument("--detail", action="store_true", help="Per-session detay tablosu")
    ap.add_argument(
        "--expected", type=float, default=None, help="UI'da gösterilen değer (doğrulama)"
    )
    args = ap.parse_args()

    con = sqlite3.connect(args.db)
    bot = fetch_bot_meta(con, args.bot_id)
    if not bot:
        print(f"[hata] Bot {args.bot_id} DB'de yok.")
        sys.exit(1)
    name, strategy, run_mode, state, order_usdc, min_price, max_price = bot
    trades = fetch_trades_summary(con, args.bot_id)
    n_trades, notional, fee_total = trades

    total = fetch_total_pnl(con, args.bot_id) or 0.0
    rows = fetch_session_breakdown(con, args.bot_id)

    counted_up = counted_dn = unresolved = 0
    win_total_up = win_total_dn = 0.0
    cost_resolved = 0.0
    sum_up_filled = sum_dn_filled = 0.0
    for r in rows:
        ub = r[4] or 0.0
        dn = r[5] or 0.0
        ifu = r[6] or 0.0
        ifd = r[7] or 0.0
        cost = r[8] or 0.0
        upf = r[10] or 0.0
        dnf = r[11] or 0.0
        sum_up_filled += upf
        sum_dn_filled += dnf
        if ub > 0.95:
            counted_up += 1
            win_total_up += ifu
            cost_resolved += cost
        elif dn > 0.95:
            counted_dn += 1
            win_total_dn += ifd
            cost_resolved += cost
        else:
            unresolved += 1

    counted = counted_up + counted_dn

    print("=" * 70)
    print(f"BOT {args.bot_id} — {name} ({strategy} / {run_mode} / {state})")
    print(f"  order_usdc=${order_usdc} min={min_price} max={max_price}")
    print("=" * 70)
    print()
    print(f"Toplam trade:          {n_trades}")
    print(f"Notional:              ${notional:,.2f}" if notional else "Notional:              -")
    print(f"Fee toplam:            ${fee_total:,.4f}" if fee_total else "Fee toplam:            -")
    print()
    print(f"Toplam session:        {len(rows)}")
    print(f"  Çözüldü (UP win):    {counted_up}")
    print(f"  Çözüldü (DOWN win):  {counted_dn}")
    print(f"  Çözülmedi (hariç):   {unresolved}")
    print()
    print(f"Toplam UP shares:      {sum_up_filled:>10,.0f}")
    print(f"Toplam DOWN shares:    {sum_dn_filled:>10,.0f}")
    print(f"Cost (çözülmüş):       ${cost_resolved:>10,.2f}")
    print()
    print(f"PnL kazanan UP:        ${win_total_up:>+10,.2f}")
    print(f"PnL kazanan DOWN:      ${win_total_dn:>+10,.2f}")
    print(f"TOPLAM K/Z (UI):       ${total:>+10,.4f}")
    if args.expected is not None:
        diff = total - args.expected
        print(f"Beklenen (UI):         ${args.expected:>+10,.4f}")
        print(f"Fark:                  ${diff:>+10,.4f}  "
              f"{'✅ MATCH' if abs(diff) < 0.01 else '❌ MISMATCH'}")
    if cost_resolved > 0:
        roi = 100.0 * total / cost_resolved
        print(f"ROI (çözülmüş):        {roi:>+10.2f}%")

    if strategy == "bonereaper":
        print()
        rb = fetch_reason_breakdown(con, args.bot_id)
        if rb:
            buy_up, buy_dn, scalp_up, scalp_dn, lw_m, lw_b = rb
            print("Reason kırılımı (logs):")
            print(f"  bonereaper:buy:up       {buy_up}")
            print(f"  bonereaper:buy:down     {buy_dn}")
            print(f"  bonereaper:scalp:up     {scalp_up}")
            print(f"  bonereaper:scalp:down   {scalp_dn}")
            print(f"  bonereaper:lw:*         {lw_m}")
            print(f"  bonereaper:lwb:*        {lw_b}")

    if args.detail:
        print()
        print("=" * 70)
        print("Per-session detay")
        print("=" * 70)
        print(f"{'sess':>5} {'ub':>5} {'db':>5} {'cost':>8} {'ifu':>8} {'ifd':>8} "
              f"{'used':>9} {'note':<15}")
        for r in rows:
            sess_id = r[0]
            ub = r[4] or 0.0
            dn = r[5] or 0.0
            ifu = r[6] or 0.0
            ifd = r[7] or 0.0
            cost = r[8] or 0.0
            if ub > 0.95:
                used = ifu
                note = "winner=UP"
            elif dn > 0.95:
                used = ifd
                note = "winner=DOWN"
            else:
                used = None
                note = "no_winner_yet"
            used_s = f"{used:+8.2f}" if used is not None else "    --   "
            print(f"{sess_id:>5} {ub:>5.2f} {dn:>5.2f} {cost:>8.2f} "
                  f"{ifu:>+8.2f} {ifd:>+8.2f} {used_s} {note:<15}")


if __name__ == "__main__":
    main()
