#!/usr/bin/env python3
"""Bot 108 — sentetik arbitrage detaylı kontrol.

5 farklı kontrol:
  1. Tüm tick'lerde MIN/MAX/AVG ask_sum ve bid_sum
  2. Tick → tick fiyat değişimleri (ne kadar volatile)
  3. Bid_sum > 1.0 (sell tarafı arbitrage) fırsatları
  4. ask_up - bid_down kombinasyonu (cross-leg sentetik)
  5. Pencere içi MIN ask_sum (her session'ın en düşük noktası)
  6. Tick cadence analizi (sub-second kayıp var mı)
"""
import argparse
import sqlite3
from collections import defaultdict


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("bot_id", type=int)
    ap.add_argument("db", nargs="?", default="/home/ubuntu/baiter/data/baiter.db")
    args = ap.parse_args()

    con = sqlite3.connect(args.db)

    print("=" * 100)
    print(f"BOT {args.bot_id} — SENTETİK ARBİTRAGE DERİN KONTROL")
    print("=" * 100)

    # === [1] Genel ask/bid sum istatistikleri ===
    print("\n[1] Genel ask_sum / bid_sum istatistikleri")
    row = con.execute(
        """
        SELECT
          COUNT(*) tick_count,
          MIN(up_best_ask + down_best_ask)  min_ask_sum,
          MAX(up_best_ask + down_best_ask)  max_ask_sum,
          AVG(up_best_ask + down_best_ask)  avg_ask_sum,
          MIN(up_best_bid + down_best_bid)  min_bid_sum,
          MAX(up_best_bid + down_best_bid)  max_bid_sum,
          AVG(up_best_bid + down_best_bid)  avg_bid_sum
        FROM market_ticks
        WHERE bot_id=?
          AND up_best_ask > 0 AND down_best_ask > 0
          AND up_best_bid > 0 AND down_best_bid > 0
        """,
        (args.bot_id,),
    ).fetchone()
    print(f"  Toplam tick:        {row[0]:,}")
    print(f"  ask_up + ask_down:  min={row[1]:.4f}  max={row[2]:.4f}  avg={row[3]:.4f}")
    print(f"  bid_up + bid_down:  min={row[4]:.4f}  max={row[5]:.4f}  avg={row[6]:.4f}")
    print()
    print(f"  → Ask asla <1.0 değil → BUY arbitrage YOK")
    print(f"  → Bid asla >1.0 değil → SELL arbitrage YOK")

    # === [2] ask_sum dağılımı ===
    print("\n[2] ask_sum bins (kaç tick hangi banda denk geldi?)")
    bins = [(1.00, 1.01), (1.01, 1.02), (1.02, 1.03), (1.03, 1.05),
            (1.05, 1.10), (1.10, 1.20), (1.20, 2.00)]
    for lo, hi in bins:
        c = con.execute(
            """SELECT COUNT(*) FROM market_ticks
               WHERE bot_id=?
                 AND up_best_ask > 0 AND down_best_ask > 0
                 AND (up_best_ask + down_best_ask) >= ?
                 AND (up_best_ask + down_best_ask) < ?""",
            (args.bot_id, lo, hi),
        ).fetchone()[0]
        marker = " ← arbitrage olası" if lo < 1.00 else ""
        print(f"  [{lo:.2f}, {hi:.2f}): {c:>10,} tick{marker}")

    # === [3] En düşük ask_sum'lı 20 tick ===
    print("\n[3] En düşük ask_sum'lı 20 tick (potansiyel fırsatlar)")
    rows = con.execute(
        """
        SELECT market_session_id, ts_ms,
               up_best_bid, up_best_ask, down_best_bid, down_best_ask,
               (up_best_ask + down_best_ask) ask_sum
        FROM market_ticks
        WHERE bot_id=?
          AND up_best_ask > 0 AND down_best_ask > 0
        ORDER BY ask_sum
        LIMIT 20
        """,
        (args.bot_id,),
    ).fetchall()
    print(f"  {'sess':>5} | {'ts_ms':>14} | {'ub':>5} {'ua':>5} {'db':>5} {'da':>5} | {'ask_sum':>8}")
    for r in rows:
        marker = " ✅" if r[6] < 1.00 else ""
        print(f"  {r[0]:>5} | {r[1]:>14} | {r[2]:>5.3f} {r[3]:>5.3f} {r[4]:>5.3f} {r[5]:>5.3f} | "
              f"{r[6]:>8.4f}{marker}")

    # === [4] ASK - BID spread (kendi içinde) ===
    print("\n[4] Spread istatistikleri (her outcome içi: ask - bid)")
    row = con.execute(
        """
        SELECT
          AVG(up_best_ask - up_best_bid) avg_up_spread,
          AVG(down_best_ask - down_best_bid) avg_dn_spread,
          MIN(up_best_ask - up_best_bid) min_up_spread,
          MIN(down_best_ask - down_best_bid) min_dn_spread,
          MAX(up_best_ask - up_best_bid) max_up_spread,
          MAX(down_best_ask - down_best_bid) max_dn_spread
        FROM market_ticks
        WHERE bot_id=?
          AND up_best_ask > 0 AND up_best_bid > 0
          AND down_best_ask > 0 AND down_best_bid > 0
        """,
        (args.bot_id,),
    ).fetchone()
    print(f"  UP   spread (ask-bid): min={row[2]:.4f} avg={row[0]:.4f} max={row[4]:.4f}")
    print(f"  DOWN spread (ask-bid): min={row[3]:.4f} avg={row[1]:.4f} max={row[5]:.4f}")
    # Polymarket binary market mekaniği: ask_up + bid_dn ≈ 1 ve bid_up + ask_dn ≈ 1
    # (eğer market dengeli ise)

    # === [5] Cross-leg: ask_up + bid_down ===
    print("\n[5] Cross-leg sums (mekanik çift)")
    row = con.execute(
        """
        SELECT
          MIN(up_best_ask + down_best_bid) min_a_b,
          MAX(up_best_ask + down_best_bid) max_a_b,
          AVG(up_best_ask + down_best_bid) avg_a_b,
          MIN(up_best_bid + down_best_ask) min_b_a,
          MAX(up_best_bid + down_best_ask) max_b_a,
          AVG(up_best_bid + down_best_ask) avg_b_a
        FROM market_ticks
        WHERE bot_id=?
          AND up_best_ask > 0 AND down_best_bid > 0
          AND up_best_bid > 0 AND down_best_ask > 0
        """,
        (args.bot_id,),
    ).fetchone()
    print(f"  ask_up + bid_down: min={row[0]:.4f} avg={row[2]:.4f} max={row[1]:.4f}  "
          f"(<1.0 → buy UP + sell DOWN arbitrage)")
    print(f"  bid_up + ask_down: min={row[3]:.4f} avg={row[5]:.4f} max={row[4]:.4f}  "
          f"(<1.0 → sell UP + buy DOWN arbitrage)")

    # === [6] Tick cadence analizi ===
    print("\n[6] Tick cadence analizi — kaç tick hangi aralıkta geldi?")
    rows = con.execute(
        """
        WITH t AS (
          SELECT market_session_id, ts_ms,
                 LAG(ts_ms) OVER (PARTITION BY market_session_id ORDER BY ts_ms) prev_ts
          FROM market_ticks
          WHERE bot_id=?
        )
        SELECT
          AVG(ts_ms - prev_ts) avg_gap_ms,
          MIN(ts_ms - prev_ts) min_gap_ms,
          MAX(ts_ms - prev_ts) max_gap_ms,
          COUNT(*) total
        FROM t WHERE prev_ts IS NOT NULL
        """,
        (args.bot_id,),
    ).fetchone()
    print(f"  Avg gap: {row[0]:.0f}ms  min={row[1]}ms  max={row[2]}ms  total={row[3]:,} gaps")
    if row[0] > 800:
        print(f"  → Tick cadence ~1sn (sub-second arbitrage fırsatları KAYBOLDUYOR olabilir)")

    # Bin dağılımı
    bin_q = """
        WITH t AS (
          SELECT ts_ms - LAG(ts_ms) OVER (PARTITION BY market_session_id ORDER BY ts_ms) g
          FROM market_ticks WHERE bot_id=?
        )
        SELECT
          SUM(CASE WHEN g < 100 THEN 1 ELSE 0 END) lt_100,
          SUM(CASE WHEN g >= 100 AND g < 500 THEN 1 ELSE 0 END) lt_500,
          SUM(CASE WHEN g >= 500 AND g < 1000 THEN 1 ELSE 0 END) lt_1000,
          SUM(CASE WHEN g >= 1000 AND g < 2000 THEN 1 ELSE 0 END) lt_2000,
          SUM(CASE WHEN g >= 2000 THEN 1 ELSE 0 END) gte_2000
        FROM t WHERE g IS NOT NULL
    """
    r = con.execute(bin_q, (args.bot_id,)).fetchone()
    print(f"  Tick gaps:  <100ms: {r[0]:>6,}  100-500ms: {r[1]:>6,}  "
          f"500-1000ms: {r[2]:>6,}  1-2sn: {r[3]:>6,}  >=2sn: {r[4]:>6,}")

    # === [7] Tek bir session'ın tick-by-tick UP/DOWN fiyat hareketi ===
    print("\n[7] Örnek session 5163 (en büyük cost'lu sessionlardan biri)")
    print("    Pencere boyunca UP/DOWN fiyat dalgalanması")
    rows = con.execute(
        """
        SELECT ts_ms, up_best_bid, up_best_ask, down_best_bid, down_best_ask
        FROM market_ticks
        WHERE bot_id=? AND market_session_id=5163
          AND up_best_ask > 0
        ORDER BY ts_ms
        """,
        (args.bot_id,),
    ).fetchall()
    if rows:
        # Min/max ask_sum
        ask_sums = [(r[2] + r[4], r[0], r) for r in rows]
        ask_sums.sort()
        print(f"  Toplam tick: {len(rows)}")
        print(f"  Min ask_sum: {ask_sums[0][0]:.4f} @ ts={ask_sums[0][1]}")
        print(f"  Max ask_sum: {ask_sums[-1][0]:.4f}")
        # En düşük 5 tick
        print(f"\n  En düşük 5 ask_sum tick'i (sess 5163):")
        for sum_v, ts, r in ask_sums[:5]:
            print(f"    ts={ts} | ub={r[1]:.3f} ua={r[2]:.3f} db={r[3]:.3f} da={r[4]:.3f} "
                  f"| ask_sum={sum_v:.4f}")

    # === [8] LSM tick örneği — pencere açılışında ===
    print("\n[8] Açılış 30 sn (sess 5163) — fiyat hareketi çok sıkıysa arbitrage olur mu?")
    rows = con.execute(
        """
        SELECT (ts_ms/1000 - 1778453100) sec, up_best_bid, up_best_ask, down_best_bid, down_best_ask,
               (up_best_ask + down_best_ask) ask_sum
        FROM market_ticks
        WHERE bot_id=? AND market_session_id=5163
          AND up_best_ask > 0 AND ts_ms < 1778453130000
        ORDER BY ts_ms LIMIT 30
        """,
        (args.bot_id,),
    ).fetchall()
    print(f"  {'sec':>4} | {'ub':>5} {'ua':>5} {'db':>5} {'da':>5} | {'ask_sum':>8}")
    for r in rows:
        marker = " ✅" if r[5] < 1.0 else ""
        print(f"  {r[0]:>4} | {r[1]:>5.3f} {r[2]:>5.3f} {r[3]:>5.3f} {r[4]:>5.3f} | {r[5]:>8.4f}{marker}")


if __name__ == "__main__":
    main()
