"""Bot 66 — Flip frekansı ve zone dağılımı analizi.

UP_best_bid 0.5 sınırını geçen her tick = "flip" olarak sayılır.
MarketZone bantları (300sn = 5dk):
  DeepTrade:    0   - 30  sn  (0-10%)
  NormalTrade:  30  - 225 sn  (10-75%)
  AggTrade:     225 - 270 sn  (75-90%)
  FakTrade:     270 - 294 sn  (90-98%)
  StopTrade:    294 - 300 sn  (98-100%)

Çıktılar:
  1) Tüm marketlerin flip sayısı dağılımı (histogram)
  2) Zone bazlı flip frekansı
  3) Flip sayısı ↔ PnL korelasyonu
  4) İlk flip zonu ↔ kazanan/kaybeden market
  5) Top-10 en çok flip olan market (detaylı timeline)
"""

import sqlite3
from collections import defaultdict, Counter
from typing import Optional

DB = "/home/ubuntu/baiter/data/baiter.db"
BOT_ID = 66

con = sqlite3.connect(DB)
con.row_factory = sqlite3.Row

sessions = con.execute(
    "SELECT id, slug, start_ts, end_ts FROM market_sessions WHERE bot_id = ? ORDER BY start_ts",
    (BOT_ID,),
).fetchall()

trades_by_sess: dict[int, list[sqlite3.Row]] = defaultdict(list)
for r in con.execute(
    "SELECT market_session_id, outcome, side, price, size, fee, ts_ms "
    "FROM trades WHERE bot_id = ? ORDER BY ts_ms",
    (BOT_ID,),
):
    trades_by_sess[r["market_session_id"]].append(r)

ticks_by_sess: dict[int, list[sqlite3.Row]] = defaultdict(list)
for r in con.execute(
    "SELECT market_session_id, up_best_bid, up_best_ask, down_best_bid, down_best_ask, "
    "       signal_score, ts_ms "
    "FROM market_ticks WHERE bot_id = ? ORDER BY ts_ms",
    (BOT_ID,),
):
    ticks_by_sess[r["market_session_id"]].append(r)


def winner_of(sid):
    ticks = ticks_by_sess.get(sid, [])
    if not ticks:
        return None
    last = ticks[-1]
    if last["up_best_bid"] > 0.95:
        return "UP"
    if last["down_best_bid"] > 0.95:
        return "DOWN"
    return None


def realized_pnl(cb, up, dn, fee, winner):
    if winner == "UP":
        return up - cb
    if winner == "DOWN":
        return dn - cb
    return -cb


def zone_of(rel_sec: float) -> str:
    if rel_sec < 30:
        return "Deep"
    if rel_sec < 225:
        return "Normal"
    if rel_sec < 270:
        return "Agg"
    if rel_sec < 294:
        return "Fak"
    return "Stop"


# Her market için flip analizi
class MarketAnalysis:
    __slots__ = (
        "slug", "winner", "actual_pnl", "actual_cost", "actual_up", "actual_dn",
        "n_flips", "flip_zones", "first_flip_zone", "last_flip_zone",
        "flips", "first_dir", "n_dir_changes",
    )
    def __init__(self):
        self.slug = ""
        self.winner = None
        self.actual_pnl = 0.0
        self.actual_cost = 0.0
        self.actual_up = 0.0
        self.actual_dn = 0.0
        self.n_flips = 0
        self.flip_zones = Counter()
        self.first_flip_zone = None
        self.last_flip_zone = None
        self.flips = []  # [(rel_sec, "UP→DOWN" or "DOWN→UP", up_bid)]
        self.first_dir = None  # ilk tick'teki UP_bid > 0.5 mi
        self.n_dir_changes = 0


analyses: list[MarketAnalysis] = []
for s in sessions:
    a = MarketAnalysis()
    a.slug = s["slug"]
    sid = s["id"]
    start_ts = s["start_ts"]
    winner = winner_of(sid)
    a.winner = winner

    cb = up = dn = fee = 0.0
    for t in trades_by_sess.get(sid, []):
        cb += t["size"] * t["price"]
        fee += t["fee"]
        if t["outcome"] == "UP":
            up += t["size"]
        elif t["outcome"] == "DOWN":
            dn += t["size"]
    a.actual_cost = cb
    a.actual_up = up
    a.actual_dn = dn
    a.actual_pnl = realized_pnl(cb, up, dn, fee, winner)

    # Flip tespit et: UP_bid 0.5 sınırını her geçişte sayılır
    ticks = ticks_by_sess.get(sid, [])
    if not ticks:
        analyses.append(a)
        continue
    prev_side = None
    for r in ticks:
        rel_sec = (r["ts_ms"] - start_ts * 1000) / 1000
        ub = r["up_best_bid"]
        if ub > 0.50:
            cur_side = "UP"
        elif ub < 0.50:
            cur_side = "DOWN"
        else:
            cur_side = None
        if cur_side is None or prev_side is None:
            if cur_side is not None and a.first_dir is None:
                a.first_dir = cur_side
            prev_side = cur_side or prev_side
            continue
        if cur_side != prev_side:
            transition = f"{prev_side}→{cur_side}"
            zone = zone_of(rel_sec)
            a.n_flips += 1
            a.flip_zones[zone] += 1
            a.flips.append((rel_sec, transition, ub))
            if a.first_flip_zone is None:
                a.first_flip_zone = zone
            a.last_flip_zone = zone
        prev_side = cur_side
    a.n_dir_changes = a.n_flips
    analyses.append(a)


# === ANALİZ 1: Flip sayısı dağılımı ===
print("=" * 95)
print("ANALİZ 1 — Tüm 232 marketin flip sayısı dağılımı")
print("=" * 95)
flip_counts = Counter(a.n_flips for a in analyses if a.actual_cost > 0)
total_with_trades = sum(flip_counts.values())
print(f"\n{'Flip Sayısı':<12} {'#Mkt':>6} {'%':>6} {'Σ PnL':>10} {'Avg PnL':>10}  Histogram")
print("-" * 95)
cumulative = 0
for n in sorted(flip_counts.keys()):
    cnt = flip_counts[n]
    cumulative += cnt
    pct = 100 * cnt / total_with_trades
    matching = [a for a in analyses if a.n_flips == n and a.actual_cost > 0]
    sum_pnl = sum(a.actual_pnl for a in matching)
    avg_pnl = sum_pnl / cnt if cnt else 0
    bar = "█" * min(int(pct), 50)
    print(f"{n:>4} flip    {cnt:>6} {pct:>5.1f}% {sum_pnl:>+10.2f} {avg_pnl:>+10.2f}  {bar}")
print("-" * 95)
print(f"TOPLAM       {total_with_trades:>6}")


# === ANALİZ 2: Hangi zone'da kaç flip ===
print("\n" + "=" * 95)
print("ANALİZ 2 — Tüm flip'lerin zone bazlı dağılımı (her tick 0.5 geçişi sayılır)")
print("=" * 95)
total_flips_by_zone = Counter()
for a in analyses:
    for zone, cnt in a.flip_zones.items():
        total_flips_by_zone[zone] += cnt
total_flips = sum(total_flips_by_zone.values())
zone_order = ["Deep", "Normal", "Agg", "Fak", "Stop"]
zone_ranges = {
    "Deep": "0-30s (0-10%)",
    "Normal": "30-225s (10-75%)",
    "Agg": "225-270s (75-90%)",
    "Fak": "270-294s (90-98%)",
    "Stop": "294-300s (98-100%)",
}
print(f"\n{'Zone':<12} {'Aralık':<22} {'#Flip':>7} {'%':>6}  Histogram")
print("-" * 90)
for z in zone_order:
    cnt = total_flips_by_zone.get(z, 0)
    pct = 100 * cnt / total_flips if total_flips else 0
    bar = "█" * min(int(pct), 50)
    print(f"{z:<12} {zone_ranges[z]:<22} {cnt:>7} {pct:>5.1f}%  {bar}")
print("-" * 90)
print(f"TOPLAM    {total_flips:>33}")


# === ANALİZ 3: Flip sayısı vs PnL ===
print("\n" + "=" * 95)
print("ANALİZ 3 — Flip sayısı ↔ Win/Loss/PnL korelasyonu")
print("=" * 95)
buckets = [
    ("0 flip", lambda a: a.n_flips == 0),
    ("1 flip", lambda a: a.n_flips == 1),
    ("2 flip", lambda a: a.n_flips == 2),
    ("3-5 flip", lambda a: 3 <= a.n_flips <= 5),
    ("6-10 flip", lambda a: 6 <= a.n_flips <= 10),
    ("11+ flip", lambda a: a.n_flips >= 11),
]
print(f"\n{'Bucket':<12} {'#Mkt':>5} {'Win':>4} {'Loss':>4} {'WinRate':>8} {'Σ PnL':>9} {'Avg PnL':>9} {'Avg Cost':>9}")
print("-" * 80)
for label, pred in buckets:
    matching = [a for a in analyses if pred(a) and a.actual_cost > 0]
    if not matching:
        continue
    n = len(matching)
    win = sum(1 for a in matching if a.actual_pnl > 0)
    loss = sum(1 for a in matching if a.actual_pnl < 0)
    sum_pnl = sum(a.actual_pnl for a in matching)
    avg_pnl = sum_pnl / n
    avg_cost = sum(a.actual_cost for a in matching) / n
    wr = 100 * win / n
    print(f"{label:<12} {n:>5} {win:>4} {loss:>4} {wr:>7.1f}% {sum_pnl:>+9.2f} {avg_pnl:>+9.2f} {avg_cost:>9.2f}")


# === ANALİZ 4: İlk flip nerede oldu? Kazanma oranı? ===
print("\n" + "=" * 95)
print("ANALİZ 4 — İlk flip hangi zone'da? Marketin kazanma/kaybetme oranı?")
print("=" * 95)
print(f"\n{'İlk flip zonu':<18} {'#Mkt':>5} {'Win':>4} {'Loss':>4} {'WinRate':>8} {'Σ PnL':>9} {'Avg PnL':>9}")
print("-" * 75)
for z in [None] + zone_order:
    matching = [a for a in analyses if a.first_flip_zone == z and a.actual_cost > 0]
    if not matching:
        continue
    n = len(matching)
    win = sum(1 for a in matching if a.actual_pnl > 0)
    loss = sum(1 for a in matching if a.actual_pnl < 0)
    sum_pnl = sum(a.actual_pnl for a in matching)
    avg_pnl = sum_pnl / n
    wr = 100 * win / n
    label = "Hiç flip yok" if z is None else f"{z}Trade"
    print(f"{label:<18} {n:>5} {win:>4} {loss:>4} {wr:>7.1f}% {sum_pnl:>+9.2f} {avg_pnl:>+9.2f}")


# === ANALİZ 5: AggTrade ya da sonrasında flip var mı? ===
print("\n" + "=" * 95)
print("ANALİZ 5 — AggTrade veya SONRA (T-75 sonrası) flip var mı? PnL etkisi?")
print("=" * 95)
late_zones = {"Agg", "Fak", "Stop"}
late_flip_buckets = [
    ("0 geç-flip", lambda a: sum(a.flip_zones[z] for z in late_zones) == 0),
    ("1 geç-flip", lambda a: sum(a.flip_zones[z] for z in late_zones) == 1),
    ("2 geç-flip", lambda a: sum(a.flip_zones[z] for z in late_zones) == 2),
    ("3+ geç-flip", lambda a: sum(a.flip_zones[z] for z in late_zones) >= 3),
]
print(f"\n{'Geç-flip sayısı':<16} {'#Mkt':>5} {'Win':>4} {'Loss':>4} {'WinRate':>8} {'Σ PnL':>9} {'Avg PnL':>9} {'Avg Cost':>9}")
print("-" * 85)
for label, pred in late_flip_buckets:
    matching = [a for a in analyses if pred(a) and a.actual_cost > 0]
    if not matching:
        continue
    n = len(matching)
    win = sum(1 for a in matching if a.actual_pnl > 0)
    loss = sum(1 for a in matching if a.actual_pnl < 0)
    sum_pnl = sum(a.actual_pnl for a in matching)
    avg_pnl = sum_pnl / n
    avg_cost = sum(a.actual_cost for a in matching) / n
    wr = 100 * win / n
    print(f"{label:<16} {n:>5} {win:>4} {loss:>4} {wr:>7.1f}% {sum_pnl:>+9.2f} {avg_pnl:>+9.2f} {avg_cost:>9.2f}")


# === ANALİZ 6: En çok flip olan 15 market ===
print("\n" + "=" * 95)
print("ANALİZ 6 — En çok flip olan 15 market (detaylı timeline)")
print("=" * 95)
top = sorted(analyses, key=lambda a: -a.n_flips)[:15]
print(f"\n{'Slug':<32} {'Win':<5} {'#Flip':>6} {'PnL':>9} {'Deep':>5} {'Norm':>5} {'Agg':>4} {'Fak':>4} {'Stop':>5}")
print("-" * 90)
for a in top:
    fz = a.flip_zones
    print(
        f"{a.slug:<32} {a.winner or '-':<5} {a.n_flips:>6} {a.actual_pnl:>+9.2f} "
        f"{fz.get('Deep',0):>5} {fz.get('Normal',0):>5} {fz.get('Agg',0):>4} "
        f"{fz.get('Fak',0):>4} {fz.get('Stop',0):>5}"
    )


# === ANALİZ 7: M1, M2, M3 detay timeline ===
print("\n" + "=" * 95)
print("ANALİZ 7 — M1 / M2 / M3 detaylı flip timeline'ı")
print("=" * 95)
TARGETS = {
    "btc-updown-5m-1778204100": "M1",
    "btc-updown-5m-1778213700": "M2",
    "btc-updown-5m-1778217000": "M3",
}
for a in analyses:
    if a.slug not in TARGETS:
        continue
    label = TARGETS[a.slug]
    print(f"\n━━━ {label} ({a.slug}) ━━━")
    print(f"  Aktüel PnL: {a.actual_pnl:+.2f} (cost=${a.actual_cost:.2f}, UP={a.actual_up:.0f}, DN={a.actual_dn:.0f}, winner={a.winner})")
    print(f"  Toplam flip: {a.n_flips}, ilk yön: {a.first_dir}")
    print(f"  Zone dağılımı: {dict(a.flip_zones)}")
    if a.flips:
        print(f"\n  Flip timeline (ilk 20):")
        print(f"    {'@sec':<6} {'Zone':<7} {'Geçiş':<10} {'UP_bid':>7}")
        for rel_sec, transition, ub in a.flips[:20]:
            print(f"    {rel_sec:>5.0f}s {zone_of(rel_sec):<7} {transition:<10} {ub:>7.3f}")
        if len(a.flips) > 20:
            print(f"    ... +{len(a.flips)-20} flip daha")


# === ANALİZ 8: Flip pattern'i ile aktüel kayıp ilişkisi ===
print("\n" + "=" * 95)
print("ANALİZ 8 — Aktüel zarar marketlerde flip pattern'i nasıl?")
print("=" * 95)
losers = [a for a in analyses if a.actual_pnl < -50 and a.actual_cost > 0]
losers.sort(key=lambda a: a.actual_pnl)

print(f"\nAktüel < -$50 zarar veren {len(losers)} market — flip dağılımı:")
print(f"\n{'Slug':<32} {'PnL':>9} {'#Flip':>6} {'Deep':>5} {'Norm':>5} {'Agg':>4} {'Fak':>4} {'Stop':>5}")
print("-" * 90)
for a in losers[:20]:
    fz = a.flip_zones
    print(
        f"{a.slug:<32} {a.actual_pnl:>+9.2f} {a.n_flips:>6} "
        f"{fz.get('Deep',0):>5} {fz.get('Normal',0):>5} {fz.get('Agg',0):>4} "
        f"{fz.get('Fak',0):>4} {fz.get('Stop',0):>5}"
    )

# Geç bölgede flip olan kaybedenler
late_loss = [a for a in losers if sum(a.flip_zones[z] for z in late_zones) > 0]
no_late_flip_loss = [a for a in losers if sum(a.flip_zones[z] for z in late_zones) == 0]
sum_late = sum(a.actual_pnl for a in late_loss)
sum_no_late = sum(a.actual_pnl for a in no_late_flip_loss)
print(f"\n  Geç-flip (Agg/Fak/Stop) olan kaybedenler: {len(late_loss)}, toplam {sum_late:+.2f}")
print(f"  Hiç geç-flip olmayan kaybedenler:        {len(no_late_flip_loss)}, toplam {sum_no_late:+.2f}")
