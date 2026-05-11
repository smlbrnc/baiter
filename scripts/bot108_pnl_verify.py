#!/usr/bin/env python3
"""Bot 108 — gerçek UI PnL'ini DB'den birebir replicate et.

UI total_pnl_for_bot SQL kuralı (src/db/sessions.rs:258):
  SELECT SUM(
    CASE
      WHEN ub > 0.95 THEN pnl_if_up
      WHEN db > 0.95 THEN pnl_if_down
      ELSE NULL
    END
  )

Bu script SQL'i Python'da koşar + her session detayını listeler.
Eğer toplam UI'da gösterilen -$2623.61 ile eşleşiyorsa simülasyon
mantığımız doğru (winner kuralı = bid > 0.95).
"""
import sqlite3
import sys

DB = sys.argv[1] if len(sys.argv) > 1 else "/home/ubuntu/baiter/data/baiter.db"
BOT_ID = 108
EXPECTED = -2623.61  # UI'da gösterilen değer


def main():
    con = sqlite3.connect(DB)

    # Birebir aynı SQL
    sql_total = con.execute(
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
        (BOT_ID,),
    ).fetchone()
    db_total = sql_total[0] if sql_total[0] is not None else 0.0
    print(f"DB SQL total_pnl_for_bot: ${db_total:+,.4f}")
    print(f"UI gösterilen:            ${EXPECTED:+,.4f}")
    print(f"Fark:                     ${db_total - EXPECTED:+,.4f}")
    print()

    # Per-session breakdown — her session winner durumu
    rows = con.execute(
        """
        SELECT s.id, s.slug,
               lt.up_best_bid, lt.down_best_bid,
               p.pnl_if_up, p.pnl_if_down, p.cost_basis,
               p.up_filled, p.down_filled
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
        (BOT_ID,),
    ).fetchall()

    counted = 0
    skipped_no_winner = 0
    skipped_no_pnl = 0
    total = 0.0
    skipped_pnl_total = 0.0
    print(f"{'sess':>5} {'ub':>5} {'db':>5} {'ifu':>8} {'ifd':>8} {'used':>9} {'note':<20}")
    for r in rows:
        sess_id, slug, ub, db_, ifu, ifd, cost, upf, dnf = r
        ub = ub or 0.0
        db_ = db_ or 0.0
        ifu = ifu if ifu is not None else 0.0
        ifd = ifd if ifd is not None else 0.0
        if ub > 0.95:
            used = ifu
            note = "winner=UP"
            total += used
            counted += 1
        elif db_ > 0.95:
            used = ifd
            note = "winner=DOWN"
            total += used
            counted += 1
        else:
            used = None
            note = "no_winner_yet"
            skipped_no_winner += 1
            # Skipped session'ın hangi tarafa yatırılmış olabileceği bilgisi
            if upf or dnf:
                skipped_pnl_total += max(ifu, ifd)
        if cost is None or upf is None:
            skipped_no_pnl += 1
        used_str = f"{used:+8.2f}" if used is not None else "    --   "
        # Sadece dikkate alınmamış / ilginç session'ları göster
        if note != "no_winner_yet" or (upf or dnf):
            print(f"{sess_id:>5} {ub:>5.2f} {db_:>5.2f} {ifu:>+8.2f} {ifd:>+8.2f} {used_str} {note:<20}")

    print()
    print(f"Toplam session: {len(rows)}")
    print(f"  Winner=UP/DOWN olanlar (toplama dahil): {counted}")
    print(f"  No-winner (toplama hariç):              {skipped_no_winner}")
    print(f"  No-pnl-snapshot:                        {skipped_no_pnl}")
    print(f"Toplam (replicated): ${total:+,.4f}")


if __name__ == "__main__":
    main()
