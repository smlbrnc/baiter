"""Bot 66 — SÜREKLİ TAKİP HEDGE simülasyonu.

KULLANICININ ÖNERİSİ:
  T-75 (AggTrade başı) → T-0 (market sonu) arası bot SÜREKLİ izlenir.
  Her tick'te (1sn) bot net pozisyonu vs sinyal yönü kontrol edilir.
  Bot yanlış tarafta ise → sinyale göre |net| × 0.5 adet taker emir.
  Cooldown: aynı yönde art arda emir engellemek için (örn. 5sn).

SİNYAL KAYNAĞI VARYANTLARI:
  V1: BBA — UP_best_bid > 0.5 → UP favorit, < 0.5 → DOWN favorit
  V2: COMPOSITE — composite > 5.5 → UP, < 4.5 → DOWN, arası NÖTR
  V3: HİBRİT — UP_bid AND composite aynı yönde gösteriyorsa

POLİTİKA:
  her tick:
    bot net poz hesapla
    sinyal yönü hesapla (yukarıdaki varyanta göre)
    eğer bot YANLIŞ tarafta + sinyal kararlı ise:
        last_hedge_ms ile cooldown kontrolü
        |net| × 0.5 adet taker emir (yeni yöne)
        last_hedge_ms = now
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


def signal_dir(tick: sqlite3.Row, variant: str) -> Optional[str]:
    """Tick'in sinyal yönünü döner: 'UP', 'DOWN' veya None (kararsız)."""
    if variant == "V1_BBA":
        ub = tick["up_best_bid"]
        if ub > 0.50:
            return "UP"
        if ub < 0.50:
            return "DOWN"
        return None
    if variant == "V1_BBA_BUFFER":
        ub = tick["up_best_bid"]
        if ub > 0.55:
            return "UP"
        if ub < 0.45:
            return "DOWN"
        return None
    if variant == "V2_COMPOSITE":
        cs = tick["signal_score"] or 5.0
        if cs > 5.5:
            return "UP"
        if cs < 4.5:
            return "DOWN"
        return None
    if variant == "V3_HYBRID":
        ub = tick["up_best_bid"]
        cs = tick["signal_score"] or 5.0
        if ub > 0.50 and cs > 5.5:
            return "UP"
        if ub < 0.50 and cs < 4.5:
            return "DOWN"
        return None
    raise ValueError(f"Bilinmeyen varyant: {variant}")


def simulate(
    follow_factor: float,
    cooldown_ms: int,
    t_start_offset: int,
    t_end_offset: int,
    variant: str,
    freeze_bot_after_secs: Optional[int] = None,
):
    """freeze_bot_after_secs: bot bu kadar sn kala signal emirlerini durdurur.
    None = bot olağan davranır (hedge layer ek)."""
    """Her market için sürekli takip uygula, ek hedge emirlerini bot'un mevcut
    trade'lerinin üstüne ekle.
    """
    total_pnl = 0.0
    total_actual = 0.0
    total_extra_cost = 0.0
    n_markets_acted = 0
    n_total_orders = 0
    helped = []
    hurt = []
    market_results = []

    for s in sessions:
        sid = s["id"]
        start_ts = s["start_ts"]
        end_ts = s["end_ts"]
        winner = winner_of(sid)
        t_start_ms = (end_ts - t_start_offset) * 1000
        t_end_ms = (end_ts - t_end_offset) * 1000

        # Aktüel: tüm trade'leri al
        cb = up = dn = fee = 0.0
        for t in trades_by_sess.get(sid, []):
            cb += t["size"] * t["price"]
            fee += t["fee"]
            if t["outcome"] == "UP":
                up += t["size"]
            elif t["outcome"] == "DOWN":
                dn += t["size"]
        actual_pnl = realized_pnl(cb, up, dn, fee, winner)
        total_actual += actual_pnl
        if cb == 0 and up == 0 and dn == 0:
            total_pnl += 0
            market_results.append((s["slug"], 0, 0, 0, 0))
            continue

        # SİMÜLASYON: trade'leri zaman sırasına göre işle, tick'leri arada izle
        sim_cb = sim_up = sim_dn = sim_fee = 0.0
        last_hedge_ms = 0
        n_orders_this_market = 0
        extra_cost_this_market = 0.0

        # Bot freeze cutoff: bu zamandan sonra bot trade'leri ATILIR (bot durdu)
        freeze_cutoff_ms = (
            (end_ts - freeze_bot_after_secs) * 1000
            if freeze_bot_after_secs is not None else float("inf")
        )

        # Trade'leri (ts_ms, type, payload) listesine çevir, freeze sonrası atla
        events = []
        for t in trades_by_sess.get(sid, []):
            if t["ts_ms"] > freeze_cutoff_ms:
                continue  # bot durmuş, bu trade gerçekleşmedi
            events.append(("trade", t["ts_ms"], t))
        # Tick'leri sadece [t_start_ms, t_end_ms] aralığında ekle (hedge tetikleyici)
        for r in ticks_by_sess.get(sid, []):
            if t_start_ms <= r["ts_ms"] <= t_end_ms:
                events.append(("tick", r["ts_ms"], r))
        events.sort(key=lambda x: (x[1], 0 if x[0] == "trade" else 1))

        for ev_type, ev_ms, payload in events:
            if ev_type == "trade":
                t = payload
                sim_cb += t["size"] * t["price"]
                sim_fee += t["fee"]
                if t["outcome"] == "UP":
                    sim_up += t["size"]
                elif t["outcome"] == "DOWN":
                    sim_dn += t["size"]
                continue
            # tick — hedge kontrolü
            net = sim_up - sim_dn
            if abs(net) == 0:
                continue
            sig = signal_dir(payload, variant)
            if sig is None:
                continue
            # Bot yanlış tarafta mı?
            bot_wrong = (sig == "UP" and net < 0) or (sig == "DOWN" and net > 0)
            if not bot_wrong:
                continue
            # Cooldown
            if ev_ms - last_hedge_ms < cooldown_ms:
                continue
            # Hedge emri
            hedge_size = abs(net) * follow_factor
            if hedge_size <= 0:
                continue
            if sig == "UP":
                price = payload["up_best_ask"] or 0.99
                sim_up += hedge_size
            else:
                price = payload["down_best_ask"] or 0.99
                sim_dn += hedge_size
            cost = hedge_size * price
            sim_cb += cost
            sim_fee += cost * DRYRUN_FEE
            extra_cost_this_market += cost
            last_hedge_ms = ev_ms
            n_orders_this_market += 1

        sim_pnl = realized_pnl(sim_cb, sim_up, sim_dn, sim_fee, winner)
        total_pnl += sim_pnl
        total_extra_cost += extra_cost_this_market
        if n_orders_this_market > 0:
            n_markets_acted += 1
            n_total_orders += n_orders_this_market
        diff = sim_pnl - actual_pnl
        if diff > 0.01:
            helped.append((s["slug"], actual_pnl, sim_pnl, diff, n_orders_this_market))
        elif diff < -0.01:
            hurt.append((s["slug"], actual_pnl, sim_pnl, diff, n_orders_this_market))
        market_results.append(
            (s["slug"], actual_pnl, sim_pnl, n_orders_this_market, extra_cost_this_market)
        )

    return {
        "total_pnl": total_pnl,
        "total_actual": total_actual,
        "delta": total_pnl - total_actual,
        "n_markets_acted": n_markets_acted,
        "n_total_orders": n_total_orders,
        "extra_cost": total_extra_cost,
        "helped": helped,
        "hurt": hurt,
        "market_results": market_results,
    }


print(f"Bot {BOT_ID} — SÜREKLİ TAKİP HEDGE simülasyonu\n")

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
print(f"AKTÜEL toplam PnL: {total_actual:+.2f} USDC\n")

CONFIGS = [
    # (follow_factor, cooldown_sec, t_start_offset, t_end_offset, variant, freeze_after, label)
    # ── BOT OLAĞAN ÇALIŞIR + sürekli hedge layer ──
    (0.50, 10, 75, 0, "V1_BBA", None, "V1 BBA | T-75→T-0 | ff=0.5 | cd=10s | bot çalışır"),
    (0.50, 10, 75, 6, "V1_BBA", None, "V1 BBA | T-75→T-6 | ff=0.5 | cd=10s | bot çalışır"),
    (0.25, 5,  75, 0, "V1_BBA", None, "V1 BBA | T-75→T-0 | ff=0.25 | cd=5s | bot çalışır"),
    # ── BOT FREEZE: T-X anından sonra bot signal emir vermez ──
    (0.50, 10, 75, 0, "V1_BBA", 75, "V1 BBA | T-75→T-0 | ff=0.5 | cd=10s | bot T-75'te DUR"),
    (0.50, 10, 75, 0, "V1_BBA", 45, "V1 BBA | T-75→T-0 | ff=0.5 | cd=10s | bot T-45'te DUR"),
    (0.50, 10, 75, 0, "V1_BBA", 30, "V1 BBA | T-75→T-0 | ff=0.5 | cd=10s | bot T-30'da DUR"),
    (0.50, 10, 45, 0, "V1_BBA", 45, "V1 BBA | T-45→T-0 | ff=0.5 | cd=10s | bot T-45'te DUR"),
    (0.50, 10, 30, 0, "V1_BBA", 30, "V1 BBA | T-30→T-0 | ff=0.5 | cd=10s | bot T-30'da DUR"),
    (0.25, 5,  75, 0, "V1_BBA", 75, "V1 BBA | T-75→T-0 | ff=0.25 | cd=5s | bot T-75'te DUR"),
    (0.25, 5,  45, 0, "V1_BBA", 45, "V1 BBA | T-45→T-0 | ff=0.25 | cd=5s | bot T-45'te DUR"),
    (0.50, 10, 75, 6, "V1_BBA", 45, "V1 BBA | T-75→T-6 | ff=0.5 | cd=10s | bot T-45'te DUR"),
    # ── COMPOSITE varyantı (freeze ile) ──
    (0.50, 10, 75, 0, "V2_COMPOSITE", 45, "V2 COMP | T-75→T-0 | ff=0.5 | cd=10s | bot T-45'te DUR"),
    (0.50, 10, 75, 0, "V3_HYBRID",    45, "V3 HYBRID | T-75→T-0 | ff=0.5 | cd=10s | bot T-45'te DUR"),
]

print(f"{'Konfigürasyon':<60} {'PnL':>10} {'Δ':>9} {'Mkt':>5} {'Emir':>5} {'Ekstra$':>10}")
print("-" * 105)

results = {}
for ff, cd, ts, te, var, freeze, label in CONFIGS:
    r = simulate(ff, cd * 1000, ts, te, var, freeze)
    results[label] = r
    print(
        f"{label:<70} {r['total_pnl']:+10.2f} {r['delta']:+9.2f} "
        f"{r['n_markets_acted']:>5} {r['n_total_orders']:>5} {r['extra_cost']:>10.0f}"
    )

# En iyi politika detayı
best_label = max(results, key=lambda k: results[k]["total_pnl"])
best = results[best_label]

print(f"\n=== EN İYİ KONFİGÜRASYON: {best_label} ===")
print(f"  Net PnL: {best['total_pnl']:+.2f} (Δ vs aktüel: {best['delta']:+.2f})")
print(f"  Eylem yapılan market sayısı: {best['n_markets_acted']}")
print(f"  Toplam hedge emir sayısı: {best['n_total_orders']}")
print(f"  Ortalama emir/market: {best['n_total_orders']/max(best['n_markets_acted'],1):.1f}")
print(f"  Ekstra notional yatırım: ${best['extra_cost']:.0f}")
print(f"  Yardım edilen: {len(best['helped'])} market (toplam +${sum(d for _,_,_,d,_ in best['helped']):.2f})")
print(f"  Zarar verilen: {len(best['hurt'])} market (toplam {sum(d for _,_,_,d,_ in best['hurt']):+.2f})")

print(f"\n  En çok YARDIM ETTİĞİ 10 market:")
print(f"    {'Slug':<32} {'Aktüel':>9} {'Sim':>9} {'Δ':>9} {'#Em':>4}")
for slug, a, s_pnl, d, n in sorted(best["helped"], key=lambda x: -x[3])[:10]:
    print(f"    {slug:<32} {a:+9.2f} {s_pnl:+9.2f} {d:+9.2f} {n:>4}")

print(f"\n  En çok ZARAR VERDİĞİ 10 market:")
for slug, a, s_pnl, d, n in sorted(best["hurt"], key=lambda x: x[3])[:10]:
    print(f"    {slug:<32} {a:+9.2f} {s_pnl:+9.2f} {d:+9.2f} {n:>4}")


# M1, M2, M3 detayları — en iyi politika ile
print("\n\n=== M1 / M2 / M3 — EN İYİ POLİTİKA İLE TICK-BY-TICK DETAY ===")
TARGETS = {
    "btc-updown-5m-1778204100": "M1",
    "btc-updown-5m-1778213700": "M2",
    "btc-updown-5m-1778217000": "M3",
}
ff_b, cd_b, ts_b, te_b, var_b, freeze_b, _ = next(c for c in CONFIGS if c[6] == best_label)
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

    # Aktüel
    cb = up = dn = fee = 0.0
    for t in trades_by_sess.get(sid, []):
        cb += t["size"] * t["price"]
        fee += t["fee"]
        if t["outcome"] == "UP":
            up += t["size"]
        elif t["outcome"] == "DOWN":
            dn += t["size"]
    actual_pnl = realized_pnl(cb, up, dn, fee, winner)

    # Simülasyon — log ile
    print(f"\n━━━ {label} ({s['slug']}) — winner={winner}, aktüel PnL={actual_pnl:+.2f} ━━━")
    sim_cb = sim_up = sim_dn = sim_fee = 0.0
    last_hedge_ms = 0
    cd_ms = cd_b * 1000

    freeze_cutoff = (end_ts - freeze_b) * 1000 if freeze_b else float("inf")
    events = []
    for t in trades_by_sess.get(sid, []):
        if t["ts_ms"] > freeze_cutoff:
            continue
        events.append(("trade", t["ts_ms"], t))
    for r in ticks_by_sess.get(sid, []):
        if t_start_ms <= r["ts_ms"] <= t_end_ms:
            events.append(("tick", r["ts_ms"], r))
    events.sort(key=lambda x: (x[1], 0 if x[0] == "trade" else 1))

    hedges_log = []
    for ev_type, ev_ms, payload in events:
        if ev_type == "trade":
            t = payload
            sim_cb += t["size"] * t["price"]
            sim_fee += t["fee"]
            if t["outcome"] == "UP":
                sim_up += t["size"]
            elif t["outcome"] == "DOWN":
                sim_dn += t["size"]
            continue
        net = sim_up - sim_dn
        if abs(net) == 0:
            continue
        sig = signal_dir(payload, var_b)
        if sig is None:
            continue
        bot_wrong = (sig == "UP" and net < 0) or (sig == "DOWN" and net > 0)
        if not bot_wrong:
            continue
        if ev_ms - last_hedge_ms < cd_ms:
            continue
        hedge_size = abs(net) * ff_b
        if hedge_size <= 0:
            continue
        if sig == "UP":
            price = payload["up_best_ask"] or 0.99
            sim_up += hedge_size
        else:
            price = payload["down_best_ask"] or 0.99
            sim_dn += hedge_size
        cost = hedge_size * price
        sim_cb += cost
        sim_fee += cost * DRYRUN_FEE
        rel_sec = (ev_ms - start_ts * 1000) / 1000
        hedges_log.append((rel_sec, sig, hedge_size, price, cost, net))
        last_hedge_ms = ev_ms

    sim_pnl = realized_pnl(sim_cb, sim_up, sim_dn, sim_fee, winner)
    print(f"  Sürekli izleme penceresi: T-{ts_b} → T-{te_b}, sinyal={var_b}, ff={ff_b}, cd={cd_b}s")
    print(f"  Tetiklenen hedge emir sayısı: {len(hedges_log)}")
    if hedges_log:
        print(f"  {'@sec':<6} {'sig':<5} {'size':>6} {'@px':>5} {'cost':>7} {'net_at':>8}")
        for rel, sig, sz, px, ct, n in hedges_log[:15]:
            print(f"  {rel:>5.0f}s {sig:<5} {sz:>6.1f} {px:>5.2f} {ct:>7.2f} {n:>+8.0f}")
        if len(hedges_log) > 15:
            print(f"  ... +{len(hedges_log)-15} emir daha")
    print(f"  Sim son durum: cost=${sim_cb:.2f}, UP={sim_up:.0f}, DN={sim_dn:.0f}")
    print(f"  Sim PnL: {sim_pnl:+.2f}  (Δ vs aktüel: {sim_pnl-actual_pnl:+.2f})")
