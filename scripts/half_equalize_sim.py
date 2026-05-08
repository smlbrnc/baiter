"""Bot 66 — KULLANICININ POLİTİKASI: Sinyali önemseme, eksik tarafa |net|/2 al.

POLİTİKA:
  T-75 → T-6 (AggTrade ile StopTrade arası) ilk flip tespit edildiğinde:
    eksik_taraf = bot net pozisyonun küçük olduğu yön
    eksik_taraf'a |net pozisyon| × 0.5 adet TAKER emir
    bot DURUR (yeni signal emir vermez)

  → 'eksik tarafa alım' = pozisyon farkını yarıya indir (yarım hedge)
  → Sinyal yönüne BAKMAZ — sadece bot pozisyon dengelemesi yapar

Karşılaştırılan varyantlar:
  • factor: 0.25, 0.50, 0.75, 1.00
  • bot DUR: ON/OFF
  • pencere: T-75→T-6 (önerilen) ve T-45→T-6 (önceki kıyas)
"""

import sqlite3
from collections import defaultdict
from typing import Optional

DB = "/home/ubuntu/baiter/data/baiter.db"
BOT_ID = 66
DRYRUN_FEE = 0.0002

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


def detect_first_flip(sid, t_start_ms, t_end_ms):
    """T-start anındaki UP_bid'e göre favori belirlenir, aralıktaki ilk flip dönülür."""
    initial = None
    for r in ticks_by_sess.get(sid, []):
        if r["ts_ms"] > t_start_ms:
            break
        initial = r
    if initial is None:
        return None, None
    init_up = initial["up_best_bid"]
    if init_up > 0.5:
        favorite = "UP"
    elif init_up < 0.5:
        favorite = "DOWN"
    else:
        return None, None
    for r in ticks_by_sess.get(sid, []):
        if r["ts_ms"] <= t_start_ms:
            continue
        if r["ts_ms"] > t_end_ms:
            break
        if favorite == "UP" and r["up_best_bid"] < 0.5:
            return r, favorite
        if favorite == "DOWN" and r["up_best_bid"] > 0.5:
            return r, favorite
    return None, favorite


def simulate(
    follow_factor: float,
    t_start_offset: int,
    t_end_offset: int,
    bot_freeze: bool,
    label: str,
):
    total_pnl = 0.0
    total_actual = 0.0
    n_acted = 0
    n_pos_zero = 0
    n_no_flip = 0
    extra_cost_total = 0.0
    helped = []
    hurt = []
    market_results = {}

    for s in sessions:
        sid = s["id"]
        end_ts = s["end_ts"]
        winner = winner_of(sid)
        t_start_ms = (end_ts - t_start_offset) * 1000
        t_end_ms = (end_ts - t_end_offset) * 1000

        # Aktüel
        cb_a = up_a = dn_a = fee_a = 0.0
        for t in trades_by_sess.get(sid, []):
            cb_a += t["size"] * t["price"]
            fee_a += t["fee"]
            if t["outcome"] == "UP":
                up_a += t["size"]
            elif t["outcome"] == "DOWN":
                dn_a += t["size"]
        actual_pnl = realized_pnl(cb_a, up_a, dn_a, fee_a, winner)
        total_actual += actual_pnl
        if cb_a == 0 and up_a == 0 and dn_a == 0:
            continue

        # Flip tespit
        flip_tick, favorite = detect_first_flip(sid, t_start_ms, t_end_ms)

        if flip_tick is None:
            total_pnl += actual_pnl
            n_no_flip += 1
            market_results[s["slug"]] = (actual_pnl, actual_pnl)
            continue

        # Bot pozisyonu (flip anına kadar)
        cb_t = up_t = dn_t = fee_t = 0.0
        for t in trades_by_sess.get(sid, []):
            if t["ts_ms"] > flip_tick["ts_ms"]:
                break
            cb_t += t["size"] * t["price"]
            fee_t += t["fee"]
            if t["outcome"] == "UP":
                up_t += t["size"]
            elif t["outcome"] == "DOWN":
                dn_t += t["size"]

        net = up_t - dn_t
        if abs(net) == 0:
            # Flip oldu ama bot net pozisyon yok → eylem gereksiz
            n_pos_zero += 1
            if bot_freeze:
                # Bot dursa ne olur — flip sonrası trade'leri atla
                sim_pnl = realized_pnl(cb_t, up_t, dn_t, fee_t, winner)
                total_pnl += sim_pnl
                market_results[s["slug"]] = (actual_pnl, sim_pnl)
            else:
                total_pnl += actual_pnl
                market_results[s["slug"]] = (actual_pnl, actual_pnl)
            continue

        # EKSİK TARAFA |net| × follow_factor adet taker emir
        eksik_taraf = "UP" if net < 0 else "DOWN"  # net<0 → DOWN ağırlıklı → UP eksik
        size = abs(net) * follow_factor
        if eksik_taraf == "UP":
            price = flip_tick["up_best_ask"] or 0.99
            sim_up = up_t + size
            sim_dn = dn_t
        else:
            price = flip_tick["down_best_ask"] or 0.99
            sim_up = up_t
            sim_dn = dn_t + size
        cost = size * price
        sim_cb = cb_t + cost
        sim_fee = fee_t + cost * DRYRUN_FEE
        extra_cost_total += cost
        n_acted += 1

        # Bot freeze sonrası ek trade'leri ekle (eğer bot freeze değilse)
        if not bot_freeze:
            for t in trades_by_sess.get(sid, []):
                if t["ts_ms"] <= flip_tick["ts_ms"]:
                    continue
                sim_cb += t["size"] * t["price"]
                sim_fee += t["fee"]
                if t["outcome"] == "UP":
                    sim_up += t["size"]
                elif t["outcome"] == "DOWN":
                    sim_dn += t["size"]

        sim_pnl = realized_pnl(sim_cb, sim_up, sim_dn, sim_fee, winner)
        total_pnl += sim_pnl
        market_results[s["slug"]] = (actual_pnl, sim_pnl)

        diff = sim_pnl - actual_pnl
        if diff > 0.01:
            helped.append((s["slug"], actual_pnl, sim_pnl, diff, eksik_taraf, size, price))
        elif diff < -0.01:
            hurt.append((s["slug"], actual_pnl, sim_pnl, diff, eksik_taraf, size, price))

    return {
        "label": label,
        "total_pnl": total_pnl,
        "delta": total_pnl - total_actual,
        "n_acted": n_acted,
        "n_pos_zero": n_pos_zero,
        "n_no_flip": n_no_flip,
        "extra_cost": extra_cost_total,
        "helped": helped,
        "hurt": hurt,
        "market_results": market_results,
    }


# Aktüel referans
total_actual = 0.0
for s in sessions:
    sid = s["id"]
    cb = up = dn = fee = 0.0
    for t in trades_by_sess.get(sid, []):
        cb += t["size"] * t["price"]
        fee += t["fee"]
        if t["outcome"] == "UP":
            up += t["size"]
        elif t["outcome"] == "DOWN":
            dn += t["size"]
    if cb == 0 and up == 0 and dn == 0:
        continue
    total_actual += realized_pnl(cb, up, dn, fee, winner_of(sid))


print(f"Bot {BOT_ID} — KULLANICI POLİTİKASI: 'sinyali önemseme, eksik tarafa |net|/2 al'\n")
print(f"AKTÜEL toplam PnL: {total_actual:+.2f} USDC\n")

CONFIGS = [
    # (follow_factor, t_start, t_end, bot_freeze, label)
    # ── KULLANICININ ÖNERİSİ: T-75 → T-6, ff=0.5 ──
    (0.50, 75, 6, True,  "T-75→T-6 | ff=0.50 | bot DUR  (KULLANICI ÖNERİSİ)"),
    (0.50, 75, 6, False, "T-75→T-6 | ff=0.50 | bot devam"),
    # ── Farklı factor'larla T-75 → T-6 ──
    (0.25, 75, 6, True,  "T-75→T-6 | ff=0.25 | bot DUR"),
    (0.75, 75, 6, True,  "T-75→T-6 | ff=0.75 | bot DUR"),
    (1.00, 75, 6, True,  "T-75→T-6 | ff=1.00 | bot DUR  (tam hedge)"),
    # ── Farklı pencere ile ff=0.5 ──
    (0.50, 45, 6, True,  "T-45→T-6 | ff=0.50 | bot DUR  (dar pencere)"),
    (0.50, 45, 6, False, "T-45→T-6 | ff=0.50 | bot devam"),
    (0.50, 90, 6, True,  "T-90→T-6 | ff=0.50 | bot DUR  (geniş pencere)"),
    # ── Geniş + farklı factor karşılaştırması ──
    (0.25, 75, 6, True,  "T-75→T-6 | ff=0.25 | bot DUR"),
    (0.50, 75, 0, True,  "T-75→T-0 | ff=0.50 | bot DUR  (StopTrade dahil)"),
]

print(f"{'Konfigürasyon':<60} {'PnL':>10} {'Δ':>9} {'Eylem':>5} {'Flat':>5} {'Hiç':>4} {'Ekstra$':>9}")
print("-" * 105)

results = {}
for ff, ts, te, bf, label in CONFIGS:
    r = simulate(ff, ts, te, bf, label)
    results[label] = r
    print(
        f"{label:<60} {r['total_pnl']:+10.2f} {r['delta']:+9.2f} "
        f"{r['n_acted']:>5} {r['n_pos_zero']:>5} {r['n_no_flip']:>4} {r['extra_cost']:>9.0f}"
    )

best_label = max(results, key=lambda k: results[k]["total_pnl"])
best = results[best_label]
print(f"\n=== EN İYİ KONFİGÜRASYON: {best_label} ===")
print(f"  Net PnL: {best['total_pnl']:+.2f} (Δ vs aktüel: {best['delta']:+.2f})")
print(f"  Eylem yapılan: {best['n_acted']} market")
print(f"  Yardım: {len(best['helped'])} market (toplam +${sum(d for _,_,_,d,_,_,_ in best['helped']):.2f})")
print(f"  Zarar: {len(best['hurt'])} market (toplam {sum(d for _,_,_,d,_,_,_ in best['hurt']):+.2f})")
print(f"  Ekstra notional: ${best['extra_cost']:.0f}")

print(f"\n  En çok YARDIM ETTİĞİ 10:")
print(f"    {'Slug':<32} {'Aktüel':>9} {'Sim':>9} {'Δ':>9}  {'Hedge':>13}")
for slug, a, s_pnl, d, side, sz, px in sorted(best["helped"], key=lambda x: -x[3])[:10]:
    print(f"    {slug:<32} {a:+9.2f} {s_pnl:+9.2f} {d:+9.2f}  {sz:.0f}{side}@${px:.2f}")

print(f"\n  En çok ZARAR VERDİĞİ 10:")
for slug, a, s_pnl, d, side, sz, px in sorted(best["hurt"], key=lambda x: x[3])[:10]:
    print(f"    {slug:<32} {a:+9.2f} {s_pnl:+9.2f} {d:+9.2f}  {sz:.0f}{side}@${px:.2f}")


# === KARŞILAŞTIRMA: tüm önemli politikalar ===
print("\n\n=== TÜM POLİTİKALARIN FİNAL SIRALAMASI ===")
print(f"{'Politika':<55} {'Δ':>8} {'Tip':<35}")
print("-" * 105)
print(f"{'B) AGGRESSIVE FOLLOW × 1.00 (T-45→T-6)':<55} {'+184':>8} {'tek-shot, agresif, no-smart':<35}")
print(f"{'E) ASYM-FOLLOW × 0.25 (T-45→T-6, smart)':<55} {'+161':>8} {'tek-shot, küçük, smart filter':<35}")
print(f"{'A) SMART HEDGE × 1.0 (T-45→T-6)':<55} {'+115':>8} {'tek-shot, eşitle, smart':<35}")
for label, r in sorted(results.items(), key=lambda x: -x[1]["delta"])[:5]:
    print(f"{label:<55} {r['delta']:>+8.0f} {'sinyali yok, |net|/2':<35}")
print(f"{'AKTÜEL':<55} {'0':>8} {'baseline':<35}")


# M1 / M2 / M3 detay
print("\n\n=== M1 / M2 / M3 — KULLANICI POLİTİKASI DETAYI ===\n")
TARGETS = {
    "btc-updown-5m-1778204100": "M1",
    "btc-updown-5m-1778213700": "M2",
    "btc-updown-5m-1778217000": "M3",
}
ff_b = next(c[0] for c in CONFIGS if c[4] == best_label)
ts_b = next(c[1] for c in CONFIGS if c[4] == best_label)
te_b = next(c[2] for c in CONFIGS if c[4] == best_label)
bf_b = next(c[3] for c in CONFIGS if c[4] == best_label)

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
    flip_tick, favorite = detect_first_flip(sid, t_start_ms, t_end_ms)

    cb = up = dn = fee = 0.0
    for t in trades_by_sess.get(sid, []):
        cb += t["size"] * t["price"]
        fee += t["fee"]
        if t["outcome"] == "UP":
            up += t["size"]
        elif t["outcome"] == "DOWN":
            dn += t["size"]
    actual_pnl = realized_pnl(cb, up, dn, fee, winner)

    print(f"━━━ {label} ({s['slug']}) — winner={winner}, aktüel={actual_pnl:+.2f}")
    if flip_tick is None:
        print(f"  Flip yok → bot olağan, sim PnL={actual_pnl:+.2f}\n")
        continue
    rel_sec = (flip_tick["ts_ms"] - start_ts * 1000) / 1000
    cb_t = up_t = dn_t = fee_t = 0.0
    for t in trades_by_sess.get(sid, []):
        if t["ts_ms"] > flip_tick["ts_ms"]:
            break
        cb_t += t["size"] * t["price"]
        fee_t += t["fee"]
        if t["outcome"] == "UP":
            up_t += t["size"]
        elif t["outcome"] == "DOWN":
            dn_t += t["size"]
    net = up_t - dn_t
    eksik = "UP" if net < 0 else "DOWN"
    size = abs(net) * ff_b
    if eksik == "UP":
        price = flip_tick["up_best_ask"] or 0.99
        new_up, new_dn = up_t + size, dn_t
    else:
        price = flip_tick["down_best_ask"] or 0.99
        new_up, new_dn = up_t, dn_t + size
    cost = size * price
    new_cb = cb_t + cost
    new_fee = fee_t + cost * DRYRUN_FEE
    sim_pnl = realized_pnl(new_cb, new_up, new_dn, new_fee, winner)

    print(f"  T-{ts_b} favori={favorite}, FLIP @{rel_sec:.0f}s")
    print(f"  T anı pozisyon: cost=${cb_t:.2f} UP={up_t:.0f} DN={dn_t:.0f} net={net:+.0f}")
    print(f"  Eksik taraf: {eksik}, hedge: {size:.0f} {eksik} @${price:.2f} = ${cost:.2f}")
    print(f"  Sim son durum: cost=${new_cb:.2f} UP={new_up:.0f} DN={new_dn:.0f}")
    print(f"  Sim PnL: {sim_pnl:+.2f} (Δ vs aktüel: {sim_pnl-actual_pnl:+.2f})\n")
