"""Bot 66 — AKADEMİK TEMELLİ OPTİMAL POLİTİKA testi.

KAYNAKLAR:
  • Luoyelittledream "Polymarket Binary Hedging" (Medium, Mart 2026):
    LAST_MIN_S, FLOOR_PRICE, EARLY_TAKE_PROFIT, slippage parametreleri
  • Kelly Criterion (binary outcome): f* = (bp - q) / b
  • Koijen et al. 2009 — momentum + mean reversion in asset allocation
  • IMDEA Networks 2025 — Polymarket arbitrage (Up+Down=$1 anchor)

POLİTİKA: "PRICE-ADAPTIVE SMART HEDGE + FLOOR + LAST-MINUTE STOP"

  Tetik penceresi: T-45 → T-6 (önceki testlerin sweet spot'u)

  1. SMART FILTER:
     • Bot doğru tarafta (yeni kazanan yön bot'un net pozisyonu ile aynı) → SKIP+DUR
     • Bot pozisyonsuz (|net| == 0) → SKIP+DUR

  2. PRICE-ADAPTIVE FACTOR (hedge price'a göre Kelly-inspired sizing):
     • hedge_price < 0.50 (UCUZ, b > 1.0)        → factor = 1.00 (tam hedge)
     • 0.50 ≤ hedge_price < 0.70 (orta)          → factor = 0.50 (yarım hedge)
     • 0.70 ≤ hedge_price < 0.85 (pahalı)        → factor = 0.25 (küçük hedge)
     • hedge_price ≥ 0.85 (çok pahalı, b < 0.18) → factor = 0.00 (HEDGE YAPMA)
     Mantık: Yüksek fiyatta payoff/risk oranı kötü → Kelly negatif → trade etme.

  3. FLOOR PROTECTION (her tick):
     • Bot pozisyon yönündeki bid < 0.10 → tüm pozisyon kayıp tarafta sayılır,
       hedge ile breakeven yakalama anlamsız → DUR (catastrophic loss kabul et)

  4. LAST-MIN STOP LOSS (T-6 sonrası, StopTrade fazı):
     • Bot durduktan sonra ek emir verilmez (bot freeze zaten aktif)

  Karşılaştırma: önceki politikalar + bu yeni politika
"""

import sqlite3
from collections import defaultdict
from typing import Optional

DB = "/home/ubuntu/baiter/data/baiter.db"
BOT_ID = 66
DRYRUN_FEE = 0.0002
T_AGG = 45
T_STOP = 6
FLOOR_PRICE = 0.10

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


def price_adaptive_factor(hedge_price: float) -> float:
    """Kelly-inspired adaptive sizing — hedge price arttıkça factor küçülür."""
    if hedge_price < 0.50:
        return 1.00
    if hedge_price < 0.70:
        return 0.50
    if hedge_price < 0.85:
        return 0.25
    return 0.00


def kelly_factor(hedge_price: float, p_win: float = 0.6) -> float:
    """Kelly Criterion: f* = (bp - q) / b
       b = (1 - hedge_price) / hedge_price  (payoff ratio: kâr / risk)
       p = win probability (sinyalin doğru olma ihtimali, empirik 0.6)
       q = 1 - p
    """
    if hedge_price <= 0 or hedge_price >= 1:
        return 0.0
    b = (1 - hedge_price) / hedge_price
    q = 1 - p_win
    f_star = (b * p_win - q) / b
    return max(0.0, min(1.0, f_star))


def simulate(
    policy: str,
    label: str,
    fixed_factor: Optional[float] = None,
    floor_protection: bool = False,
    p_win: float = 0.60,
):
    """Politikalar:
      A: SMART HEDGE × fixed_factor (önceki politikalar baseline)
      P: PRICE-ADAPTIVE (hedge price'a göre dinamik factor)
      K: KELLY-INSPIRED (Kelly formülüne göre dinamik factor)
    """
    total_pnl = 0.0
    total_actual = 0.0
    n_acted = 0
    n_smart_skip = 0
    n_floor_skip = 0
    n_no_flip = 0
    n_no_pos = 0
    extra_cost = 0.0
    helped = []
    hurt = []
    factor_dist = defaultdict(int)

    for s in sessions:
        sid = s["id"]
        end_ts = s["end_ts"]
        winner = winner_of(sid)
        t_start_ms = (end_ts - T_AGG) * 1000
        t_end_ms = (end_ts - T_STOP) * 1000

        # AKTÜEL
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

        flip_tick, favorite = detect_first_flip(sid, t_start_ms, t_end_ms)
        if flip_tick is None:
            n_no_flip += 1
            total_pnl += actual_pnl
            continue

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
            n_no_pos += 1
            sim_pnl = realized_pnl(cb_t, up_t, dn_t, fee_t, winner)
            total_pnl += sim_pnl
            continue

        # Yeni kazanan yön (flip yönü)
        new_dir = "DOWN" if favorite == "UP" else "UP"
        bot_in_winner = (new_dir == "UP" and net > 0) or (new_dir == "DOWN" and net < 0)

        # SMART FILTER: bot zaten doğru tarafta → SKIP
        if bot_in_winner:
            n_smart_skip += 1
            sim_pnl = realized_pnl(cb_t, up_t, dn_t, fee_t, winner)
            total_pnl += sim_pnl
            diff = sim_pnl - actual_pnl
            if diff > 0.01:
                helped.append((s["slug"], actual_pnl, sim_pnl, diff, "SKIP", 0, 0))
            elif diff < -0.01:
                hurt.append((s["slug"], actual_pnl, sim_pnl, diff, "SKIP", 0, 0))
            continue

        # Bot yanlış tarafta → hedge size'ı belirle
        hedge_price = flip_tick["up_best_ask"] if new_dir == "UP" else flip_tick["down_best_ask"]
        if hedge_price <= 0:
            hedge_price = 0.99

        # FLOOR PROTECTION: bot pozisyonun bid'i < 0.10 ise hedge yapma (kayıp kesinleşmiş)
        bot_side_bid = flip_tick["up_best_bid"] if net > 0 else flip_tick["down_best_bid"]
        if floor_protection and bot_side_bid < FLOOR_PRICE:
            n_floor_skip += 1
            sim_pnl = realized_pnl(cb_t, up_t, dn_t, fee_t, winner)
            total_pnl += sim_pnl
            continue

        # Politikaya göre factor seç
        if policy == "A":
            factor = fixed_factor or 0.5
        elif policy == "P":
            factor = price_adaptive_factor(hedge_price)
        elif policy == "K":
            factor = kelly_factor(hedge_price, p_win)
        else:
            raise ValueError(f"Bilinmeyen politika: {policy}")

        factor_dist[round(factor, 2)] += 1

        if factor <= 0:
            # Kelly negatif → hedge yapma (sadece DUR)
            sim_pnl = realized_pnl(cb_t, up_t, dn_t, fee_t, winner)
            total_pnl += sim_pnl
            diff = sim_pnl - actual_pnl
            if diff > 0.01:
                helped.append((s["slug"], actual_pnl, sim_pnl, diff, "NO-HEDGE", 0, hedge_price))
            elif diff < -0.01:
                hurt.append((s["slug"], actual_pnl, sim_pnl, diff, "NO-HEDGE", 0, hedge_price))
            continue

        # Hedge uygula
        size = abs(net) * factor
        if new_dir == "UP":
            new_up = up_t + size
            new_dn = dn_t
        else:
            new_up = up_t
            new_dn = dn_t + size
        cost = size * hedge_price
        new_cb = cb_t + cost
        new_fee = fee_t + cost * DRYRUN_FEE
        extra_cost += cost
        n_acted += 1

        sim_pnl = realized_pnl(new_cb, new_up, new_dn, new_fee, winner)
        total_pnl += sim_pnl

        diff = sim_pnl - actual_pnl
        if diff > 0.01:
            helped.append((s["slug"], actual_pnl, sim_pnl, diff, f"HEDGE×{factor:.2f}", size, hedge_price))
        elif diff < -0.01:
            hurt.append((s["slug"], actual_pnl, sim_pnl, diff, f"HEDGE×{factor:.2f}", size, hedge_price))

    return {
        "label": label,
        "total_pnl": total_pnl,
        "delta": total_pnl - total_actual,
        "n_acted": n_acted,
        "n_smart_skip": n_smart_skip,
        "n_floor_skip": n_floor_skip,
        "n_no_flip": n_no_flip,
        "n_no_pos": n_no_pos,
        "extra_cost": extra_cost,
        "helped": helped,
        "hurt": hurt,
        "factor_dist": dict(factor_dist),
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


print(f"Bot {BOT_ID} — AKADEMİK TEMELLİ OPTİMAL POLİTİKA TESTI\n")
print(f"AKTÜEL toplam PnL: {total_actual:+.2f} USDC\n")
print(f"Tetik penceresi: T-{T_AGG} → T-{T_STOP}, FLOOR_PRICE={FLOOR_PRICE}\n")

CONFIGS = [
    # (policy, fixed_factor, floor_protection, p_win, label)
    ("A", 0.25, False, 0.6, "BASELINE: Smart × 0.25 (önceki en iyi E)"),
    ("A", 0.50, False, 0.6, "BASELINE: Smart × 0.50"),
    ("A", 1.00, False, 0.6, "BASELINE: Smart × 1.00"),
    ("P", None, False, 0.6, "PRICE-ADAPTIVE (factor=price band)"),
    ("P", None, True,  0.6, "PRICE-ADAPTIVE + FLOOR (bid<0.10 ise hedge'siz)"),
    ("K", None, False, 0.55, "KELLY (p_win=0.55)"),
    ("K", None, False, 0.60, "KELLY (p_win=0.60) — empirik"),
    ("K", None, False, 0.65, "KELLY (p_win=0.65)"),
    ("K", None, False, 0.70, "KELLY (p_win=0.70) — yüksek güven"),
    ("K", None, True,  0.60, "KELLY (p_win=0.60) + FLOOR"),
]

print(f"{'Konfigürasyon':<55} {'PnL':>10} {'Δ':>9} {'Hedge':>5} {'Skip':>5} {'Floor':>5} {'Ekstra$':>9}")
print("-" * 105)

results = {}
for cfg in CONFIGS:
    pol, ff, fp, pw, label = cfg
    r = simulate(pol, label, ff, fp, pw)
    results[label] = r
    print(
        f"{label:<55} {r['total_pnl']:+10.2f} {r['delta']:+9.2f} "
        f"{r['n_acted']:>5} {r['n_smart_skip']:>5} {r['n_floor_skip']:>5} {r['extra_cost']:>9.0f}"
    )

best_label = max(results, key=lambda k: results[k]["total_pnl"])
best = results[best_label]
print(f"\n=== EN İYİ POLİTİKA: {best_label} ===")
print(f"  Net PnL: {best['total_pnl']:+.2f} (Δ vs aktüel: {best['delta']:+.2f})")
print(f"  Hedge tetiklendi: {best['n_acted']} market")
print(f"  Smart skip (bot doğru taraf): {best['n_smart_skip']}")
print(f"  Floor skip (bid<0.10): {best['n_floor_skip']}")
print(f"  Flip yok: {best['n_no_flip']}, pozisyon yok: {best['n_no_pos']}")
print(f"  Yardım: {len(best['helped'])} market (+${sum(d for _,_,_,d,_,_,_ in best['helped']):.2f})")
print(f"  Zarar: {len(best['hurt'])} market ({sum(d for _,_,_,d,_,_,_ in best['hurt']):+.2f})")
print(f"  Ekstra notional: ${best['extra_cost']:.0f}")
print(f"  Factor dağılımı: {best['factor_dist']}")

print(f"\n  En çok YARDIM ETTİĞİ 10 market:")
print(f"    {'Slug':<32} {'Aktüel':>9} {'Sim':>9} {'Δ':>9}  {'Eylem':<15} {'@px':>5}")
for slug, a, s, d, act, sz, px in sorted(best["helped"], key=lambda x: -x[3])[:10]:
    eylem = f"{act} ({sz:.0f})" if sz > 0 else act
    print(f"    {slug:<32} {a:+9.2f} {s:+9.2f} {d:+9.2f}  {eylem:<15} {px:>5.2f}")

print(f"\n  En çok ZARAR VERDİĞİ 10 market:")
for slug, a, s, d, act, sz, px in sorted(best["hurt"], key=lambda x: x[3])[:10]:
    eylem = f"{act} ({sz:.0f})" if sz > 0 else act
    print(f"    {slug:<32} {a:+9.2f} {s:+9.2f} {d:+9.2f}  {eylem:<15} {px:>5.2f}")


# === FİNAL SIRALAMA ===
print("\n\n=== TÜM POLİTİKALARIN FİNAL SIRALAMASI (akademik + önceki) ===\n")
all_results = {
    "B) AGGRESSIVE FOLLOW × 1.00 (önceki, T-45→T-6)": 184,
    "E) ASYM-FOLLOW × 0.25 (önceki, smart, T-45→T-6)": 161,
    "A) SMART HEDGE × 1.0 (önceki)": 115,
}
for label, r in results.items():
    all_results[label] = round(r["delta"], 2)

print(f"{'Politika':<60} {'Δ vs aktüel':>12}")
print("-" * 75)
for label, delta in sorted(all_results.items(), key=lambda x: -x[1]):
    marker = " ✅" if label == best_label else ""
    print(f"{label:<60} {delta:>+12.2f}{marker}")
print(f"{'AKTÜEL':<60} {0:>+12.2f}")


# M1 / M2 / M3 detay
print(f"\n\n=== M1 / M2 / M3 — EN İYİ POLİTİKA İLE DETAY ===")
TARGETS = {
    "btc-updown-5m-1778204100": "M1",
    "btc-updown-5m-1778213700": "M2",
    "btc-updown-5m-1778217000": "M3",
}
best_cfg = next(c for c in CONFIGS if c[4] == best_label)
pol_b, ff_b, fp_b, pw_b, _ = best_cfg

for s in sessions:
    if s["slug"] not in TARGETS:
        continue
    label = TARGETS[s["slug"]]
    sid = s["id"]
    end_ts = s["end_ts"]
    start_ts = s["start_ts"]
    winner = winner_of(sid)
    t_start_ms = (end_ts - T_AGG) * 1000
    t_end_ms = (end_ts - T_STOP) * 1000
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

    print(f"\n━━━ {label} ({s['slug']}) — winner={winner}, aktüel={actual_pnl:+.2f}")
    if flip_tick is None:
        print(f"  Flip yok → bot olağan, sim={actual_pnl:+.2f}")
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
    new_dir = "DOWN" if favorite == "UP" else "UP"
    bot_in_winner = (new_dir == "UP" and net > 0) or (new_dir == "DOWN" and net < 0)
    hedge_price = flip_tick["up_best_ask"] if new_dir == "UP" else flip_tick["down_best_ask"]

    print(f"  T-{T_AGG} favori={favorite}, FLIP @{rel_sec:.0f}s → yeni yön={new_dir}")
    print(f"  T anı: cost=${cb_t:.2f} UP={up_t:.0f} DN={dn_t:.0f} net={net:+.0f}")
    print(f"  Hedge price: ${hedge_price:.3f}")

    if bot_in_winner:
        print(f"  → Bot DOĞRU tarafta, SMART SKIP (DUR)")
        sim_pnl = realized_pnl(cb_t, up_t, dn_t, fee_t, winner)
    else:
        if pol_b == "A":
            factor = ff_b
            kind = f"Smart × {factor:.2f} (sabit)"
        elif pol_b == "P":
            factor = price_adaptive_factor(hedge_price)
            kind = f"Price-Adaptive (factor={factor:.2f}, price={hedge_price:.2f})"
        else:
            factor = kelly_factor(hedge_price, pw_b)
            b_pay = (1-hedge_price)/hedge_price
            kind = f"Kelly (factor={factor:.2f}, b={b_pay:.2f}, p={pw_b})"
        bot_side_bid = flip_tick["up_best_bid"] if net > 0 else flip_tick["down_best_bid"]
        if fp_b and bot_side_bid < FLOOR_PRICE:
            print(f"  → Bot pos bid={bot_side_bid:.3f} < FLOOR_PRICE → DUR (catastrophic loss kabul)")
            sim_pnl = realized_pnl(cb_t, up_t, dn_t, fee_t, winner)
        elif factor <= 0:
            print(f"  → factor={factor} (price={hedge_price:.2f}) → HEDGE YOK (Kelly negatif)")
            sim_pnl = realized_pnl(cb_t, up_t, dn_t, fee_t, winner)
        else:
            size = abs(net) * factor
            cost = size * hedge_price
            if new_dir == "UP":
                new_up, new_dn = up_t + size, dn_t
            else:
                new_up, new_dn = up_t, dn_t + size
            new_cb = cb_t + cost
            new_fee = fee_t + cost * DRYRUN_FEE
            sim_pnl = realized_pnl(new_cb, new_up, new_dn, new_fee, winner)
            print(f"  → {kind} → {size:.0f} {new_dir} @ ${hedge_price:.2f} = ${cost:.2f}")

    print(f"  Sim PnL: {sim_pnl:+.2f} (Δ {sim_pnl-actual_pnl:+.2f})")
