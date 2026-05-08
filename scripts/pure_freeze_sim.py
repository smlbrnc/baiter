"""Bot 66 — PURE FREEZE politikası testi.

POLİTİKA:
  T_start → T_end arası ilk flip tespit edildiğinde:
    Bot pozisyon var mı?
      HAYIR → no-op
      EVET → bot DUR (yeni signal emir verme), HEDGE YOK

Karşılaştırılan varyantlar:
  • Pencere genişliği: T-30, T-45, T-60, T-75, T-90 başlangıç
  • Bitiş: T-6 (StopTrade öncesi)
  • Karşılaştırma: önceki en iyi politikalar
"""

import sqlite3
from collections import defaultdict
from typing import Optional

DB = "/home/ubuntu/baiter/data/baiter.db"
BOT_ID = 66

con = sqlite3.connect(DB)
con.row_factory = sqlite3.Row

sessions = con.execute(
    "SELECT id, slug, start_ts, end_ts FROM market_sessions WHERE bot_id = ? ORDER BY start_ts",
    (BOT_ID,),
).fetchall()

trades_by_sess = defaultdict(list)
for r in con.execute(
    "SELECT market_session_id, outcome, side, price, size, fee, ts_ms "
    "FROM trades WHERE bot_id = ? ORDER BY ts_ms", (BOT_ID,),
):
    trades_by_sess[r["market_session_id"]].append(r)

ticks_by_sess = defaultdict(list)
for r in con.execute(
    "SELECT market_session_id, up_best_bid, up_best_ask, down_best_bid, down_best_ask, ts_ms "
    "FROM market_ticks WHERE bot_id = ? ORDER BY ts_ms", (BOT_ID,),
):
    ticks_by_sess[r["market_session_id"]].append(r)


def winner_of(sid):
    ticks = ticks_by_sess.get(sid, [])
    if not ticks: return None
    last = ticks[-1]
    if last["up_best_bid"] > 0.95: return "UP"
    if last["down_best_bid"] > 0.95: return "DOWN"
    return None


def realized_pnl(cb, up, dn, fee, winner):
    if winner == "UP": return up - cb
    if winner == "DOWN": return dn - cb
    return -cb


def state_at(sid, cutoff_ms):
    cb = up = dn = fee = 0.0
    for t in trades_by_sess.get(sid, []):
        if t["ts_ms"] > cutoff_ms: break
        cb += t["size"] * t["price"]
        fee += t["fee"]
        if t["outcome"] == "UP": up += t["size"]
        elif t["outcome"] == "DOWN": dn += t["size"]
    return cb, up, dn, fee


def tick_at(sid, cutoff_ms):
    chosen = None
    for r in ticks_by_sess.get(sid, []):
        if r["ts_ms"] > cutoff_ms: break
        chosen = r
    return chosen


def detect_first_flip(sid, t_start_ms, t_end_ms):
    initial = tick_at(sid, t_start_ms)
    if initial is None: return None
    init_up = initial["up_best_bid"]
    if init_up > 0.5: favorite = "UP"
    elif init_up < 0.5: favorite = "DOWN"
    else: return None
    for r in ticks_by_sess.get(sid, []):
        if r["ts_ms"] <= t_start_ms: continue
        if r["ts_ms"] > t_end_ms: break
        if favorite == "UP" and r["up_best_bid"] < 0.5: return r
        if favorite == "DOWN" and r["up_best_bid"] > 0.5: return r
    return None


def simulate_pure_freeze(t_start_offset, t_end_offset):
    total_pnl = 0.0
    total_actual = 0.0
    n_freeze = 0
    n_no_flip = 0
    n_no_pos = 0
    helped = []
    hurt = []
    market_results = {}

    for s in sessions:
        sid = s["id"]
        end_ts = s["end_ts"]
        winner = winner_of(sid)
        t_start_ms = (end_ts - t_start_offset) * 1000
        t_end_ms = (end_ts - t_end_offset) * 1000

        cb_a, up_a, dn_a, fee_a = state_at(sid, end_ts * 1000)
        actual_pnl = realized_pnl(cb_a, up_a, dn_a, fee_a, winner)
        total_actual += actual_pnl
        if cb_a == 0 and up_a == 0 and dn_a == 0:
            continue

        flip_tick = detect_first_flip(sid, t_start_ms, t_end_ms)
        if flip_tick is None:
            n_no_flip += 1
            total_pnl += actual_pnl
            market_results[s["slug"]] = (actual_pnl, actual_pnl, "no_flip")
            continue

        cb_t, up_t, dn_t, fee_t = state_at(sid, flip_tick["ts_ms"])
        if cb_t == 0 and up_t == 0 and dn_t == 0:
            n_no_pos += 1
            total_pnl += actual_pnl
            market_results[s["slug"]] = (actual_pnl, actual_pnl, "no_pos")
            continue

        # PURE FREEZE: bot durur, ek trade yok, hedge yok
        sim_pnl = realized_pnl(cb_t, up_t, dn_t, fee_t, winner)
        total_pnl += sim_pnl
        n_freeze += 1
        market_results[s["slug"]] = (actual_pnl, sim_pnl, "freeze")

        diff = sim_pnl - actual_pnl
        if diff > 0.01:
            helped.append((s["slug"], actual_pnl, sim_pnl, diff))
        elif diff < -0.01:
            hurt.append((s["slug"], actual_pnl, sim_pnl, diff))

    return {
        "total_pnl": total_pnl,
        "delta": total_pnl - total_actual,
        "n_freeze": n_freeze,
        "n_no_flip": n_no_flip,
        "n_no_pos": n_no_pos,
        "helped": helped,
        "hurt": hurt,
    }


# Aktüel
total_actual = 0.0
for s in sessions:
    sid = s["id"]
    cb = up = dn = fee = 0.0
    for t in trades_by_sess.get(sid, []):
        cb += t["size"] * t["price"]
        fee += t["fee"]
        if t["outcome"] == "UP": up += t["size"]
        elif t["outcome"] == "DOWN": dn += t["size"]
    if cb == 0 and up == 0 and dn == 0: continue
    total_actual += realized_pnl(cb, up, dn, fee, winner_of(sid))


print(f"Bot {BOT_ID} — PURE FREEZE testi (sadece 'bot DUR', hedge yok)\n")
print(f"AKTÜEL toplam PnL: {total_actual:+.2f} USDC\n")

CONFIGS = [
    # (t_start_offset, t_end_offset)
    (30, 6, "PURE FREEZE | T-30→T-6"),
    (45, 6, "PURE FREEZE | T-45→T-6 (önerilen)"),
    (60, 6, "PURE FREEZE | T-60→T-6"),
    (75, 6, "PURE FREEZE | T-75→T-6"),
    (90, 6, "PURE FREEZE | T-90→T-6"),
    (45, 0, "PURE FREEZE | T-45→T-0 (StopTrade dahil)"),
    (45, 15, "PURE FREEZE | T-45→T-15 (FakTrade üst)"),
    (30, 0, "PURE FREEZE | T-30→T-0"),
    (60, 15, "PURE FREEZE | T-60→T-15"),
]

print(f"{'Konfigürasyon':<55} {'PnL':>10} {'Δ':>9} {'Freeze':>7} {'Yardım':>7} {'Zarar':>6}")
print("-" * 105)

results = {}
for ts, te, label in CONFIGS:
    r = simulate_pure_freeze(ts, te)
    results[label] = r
    print(
        f"{label:<55} {r['total_pnl']:+10.2f} {r['delta']:+9.2f} "
        f"{r['n_freeze']:>7} {len(r['helped']):>7} {len(r['hurt']):>6}"
    )


best_label = max(results, key=lambda k: results[k]["total_pnl"])
best = results[best_label]
print(f"\n=== EN İYİ KONFİGÜRASYON: {best_label} ===")
print(f"  Net PnL: {best['total_pnl']:+.2f} (Δ vs aktüel: {best['delta']:+.2f})")
print(f"  Freeze tetiklendi: {best['n_freeze']} market")
print(f"  Flip yok: {best['n_no_flip']}, pozisyon yok: {best['n_no_pos']}")
print(f"  Yardım: {len(best['helped'])} market (+${sum(d for _,_,_,d in best['helped']):.2f})")
print(f"  Zarar: {len(best['hurt'])} market ({sum(d for _,_,_,d in best['hurt']):+.2f})")

print(f"\n  En çok YARDIM ETTİĞİ TÜM marketler:")
print(f"    {'Slug':<32} {'Aktüel':>9} {'Sim':>9} {'Δ':>9}")
for slug, a, s, d in sorted(best["helped"], key=lambda x: -x[3]):
    print(f"    {slug:<32} {a:+9.2f} {s:+9.2f} {d:+9.2f}")

print(f"\n  En çok ZARAR VERDİĞİ TÜM marketler:")
print(f"    {'Slug':<32} {'Aktüel':>9} {'Sim':>9} {'Δ':>9}")
for slug, a, s, d in sorted(best["hurt"], key=lambda x: x[3]):
    print(f"    {slug:<32} {a:+9.2f} {s:+9.2f} {d:+9.2f}")


# === FİNAL SIRALAMA TÜM POLİTİKALAR ===
print("\n\n=== TÜM POLİTİKALARIN FİNAL SIRALAMASI ===\n")
all_policies = {
    "B) AGGRESSIVE FOLLOW × 1.00 (T-45→T-6, sinyal yön)": 184,
    "KELLY p=0.55 (T-45→T-6, smart skip + Kelly hedge)": 172,
    "E) ASYM-FOLLOW × 0.25 (T-45→T-6, smart filter)": 161,
    "PRICE-ADAPTIVE (T-45→T-6, price band)": 160,
    "A) SMART HEDGE × 1.0 (T-45→T-6)": 115,
}
for label, r in results.items():
    all_policies[label] = round(r["delta"], 2)

print(f"{'Politika':<60} {'Δ':>8} {'Notional':>10}")
print("-" * 95)
notional_map = {
    "B) AGGRESSIVE FOLLOW × 1.00 (T-45→T-6, sinyal yön)": "$1 608",
    "KELLY p=0.55 (T-45→T-6, smart skip + Kelly hedge)": "$10",
    "E) ASYM-FOLLOW × 0.25 (T-45→T-6, smart filter)": "$299",
    "PRICE-ADAPTIVE (T-45→T-6, price band)": "$456",
    "A) SMART HEDGE × 1.0 (T-45→T-6)": "$1 195",
}
for label, delta in sorted(all_policies.items(), key=lambda x: -x[1]):
    notional = notional_map.get(label, "$0")
    marker = " ✅" if label == best_label else ""
    print(f"{label:<60} {delta:>+8.2f} {notional:>10}{marker}")
print(f"{'AKTÜEL':<60} {0:>+8.2f} {'$0':>10}")


# M1 / M2 / M3 detay
print("\n\n=== M1 / M2 / M3 — PURE FREEZE DETAYI ===")
TARGETS = {
    "btc-updown-5m-1778204100": "M1",
    "btc-updown-5m-1778213700": "M2",
    "btc-updown-5m-1778217000": "M3",
}
ts_b, te_b, _ = next(c for c in CONFIGS if c[2] == best_label)

for s in sessions:
    if s["slug"] not in TARGETS:
        continue
    label = TARGETS[s["slug"]]
    sid = s["id"]
    end_ts = s["end_ts"]
    start_ts = s["start_ts"]
    winner = winner_of(sid)
    t_start_ms = (end_ts - ts_b) * 1000
    t_end_ms = (end_ts - te_b) * 1000

    cb_a, up_a, dn_a, fee_a = state_at(sid, end_ts * 1000)
    actual_pnl = realized_pnl(cb_a, up_a, dn_a, fee_a, winner)

    flip_tick = detect_first_flip(sid, t_start_ms, t_end_ms)
    print(f"\n━━━ {label} ({s['slug']}) — winner={winner}, aktüel={actual_pnl:+.2f}")
    if flip_tick is None:
        print(f"  Flip yok → bot olağan, sim={actual_pnl:+.2f}")
        continue
    rel_sec = (flip_tick["ts_ms"] - start_ts * 1000) / 1000
    cb_t, up_t, dn_t, fee_t = state_at(sid, flip_tick["ts_ms"])
    sim_pnl = realized_pnl(cb_t, up_t, dn_t, fee_t, winner)
    print(f"  FLIP @{rel_sec:.0f}s")
    print(f"  T anı: cost=${cb_t:.2f} UP={up_t:.0f} DN={dn_t:.0f}")
    print(f"  Aktüel final: cost=${cb_a:.2f} UP={up_a:.0f} DN={dn_a:.0f}")
    print(f"  → Bot DUR (T sonrası ek trade yok)")
    print(f"  Sim PnL: {sim_pnl:+.2f}  (Δ vs aktüel: {sim_pnl-actual_pnl:+.2f})")
