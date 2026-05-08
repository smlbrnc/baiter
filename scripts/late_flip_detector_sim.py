"""Bot 66 — '1 GEÇ-FLİP DETECTOR' politikası testi.

POLİTİKA:
  T-75'ten itibaren her tick'te UP_bid 0.5 sınırını geçişini sayan counter:

    counter == 0 (henüz geç-flip yok)
      → bot olağan davranır (Triple Gate signal emirleri)

    counter == 1 (ilk geç-flip tespit edildi) AND |net pozisyon × price| >= POS_THRESHOLD
      → SMART HEDGE: bot yanlış tarafta ise yeni yöne |net| × FOLLOW_FACTOR taker emir
      → Bot DURUR (sonraki signal emirleri verilmez)

    counter >= 2 (whipsaw market — birden fazla geç-flip)
      → Bot DURUR (yeni signal emir verme), HEDGE YAPMA
      → Mevcut pozisyon olduğu gibi kalır
"""

import sqlite3
from collections import defaultdict
from typing import Optional

DB = "/home/ubuntu/baiter/data/baiter.db"
BOT_ID = 66
DRYRUN_FEE = 0.0002
T_AGG = 75  # T-75 = AggTrade başı (sayma penceresinin başlangıcı)
T_END = 0   # market sonu

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


def simulate(
    follow_factor: float,
    pos_threshold_usd: float,
    label: str,
    whipsaw_freeze: bool = True,
):
    """1 geç-flip detector simülasyonu.
    whipsaw_freeze=False → 2+ geç-flip'te bot durmaz, olağan davranır."""
    total_pnl = 0.0
    total_actual = 0.0
    n_first_flip_hedge = 0  # counter==1 + size yeterli + bot yanlış → hedge
    n_first_flip_skip = 0   # counter==1 + bot doğru taraf → skip+dur
    n_first_flip_too_small = 0  # counter==1 + pos küçük → no action
    n_whipsaw_dur = 0  # counter>=2 → dur (no hedge)
    n_no_late_flip = 0  # counter==0 → bot olağan
    extra_cost_total = 0.0
    helped = []
    hurt = []

    for s in sessions:
        sid = s["id"]
        end_ts = s["end_ts"]
        start_ts = s["start_ts"]
        winner = winner_of(sid)
        t_agg_ms = (end_ts - T_AGG) * 1000
        t_end_ms = (end_ts - T_END) * 1000

        # AKTÜEL
        cb_act = up_act = dn_act = fee_act = 0.0
        for t in trades_by_sess.get(sid, []):
            cb_act += t["size"] * t["price"]
            fee_act += t["fee"]
            if t["outcome"] == "UP":
                up_act += t["size"]
            elif t["outcome"] == "DOWN":
                dn_act += t["size"]
        actual_pnl = realized_pnl(cb_act, up_act, dn_act, fee_act, winner)
        total_actual += actual_pnl
        if cb_act == 0 and up_act == 0 and dn_act == 0:
            continue

        # SİMÜLASYON: trade'leri ve tick'leri zaman sırasına göre işle
        sim_cb = sim_up = sim_dn = sim_fee = 0.0
        flip_counter = 0
        bot_frozen = False
        flip_action_done = False  # counter==1 anında bir kez aksiyon
        prev_side: Optional[str] = None

        events = []
        for t in trades_by_sess.get(sid, []):
            events.append(("trade", t["ts_ms"], t))
        for r in ticks_by_sess.get(sid, []):
            if t_agg_ms <= r["ts_ms"] <= t_end_ms:
                events.append(("tick", r["ts_ms"], r))
        events.sort(key=lambda x: (x[1], 0 if x[0] == "trade" else 1))

        flip_count_for_market = 0  # bu market için kaç hedge yapıldı

        for ev_type, ev_ms, payload in events:
            if ev_type == "trade":
                if bot_frozen:
                    continue  # bot durdu, bu trade gerçekleşmedi
                t = payload
                sim_cb += t["size"] * t["price"]
                sim_fee += t["fee"]
                if t["outcome"] == "UP":
                    sim_up += t["size"]
                elif t["outcome"] == "DOWN":
                    sim_dn += t["size"]
                continue

            # tick — flip kontrolü
            r = payload
            ub = r["up_best_bid"]
            cur_side = "UP" if ub > 0.50 else ("DOWN" if ub < 0.50 else None)
            if cur_side is None or prev_side is None:
                if cur_side is not None:
                    prev_side = cur_side
                continue
            if cur_side != prev_side:
                # Flip oluştu
                flip_counter += 1
                prev_side = cur_side

                if flip_counter == 1 and not flip_action_done:
                    flip_action_done = True
                    # SMART HEDGE değerlendirmesi
                    net = sim_up - sim_dn
                    new_winner_dir = cur_side  # flip yönü
                    bot_in_winner = (
                        (new_winner_dir == "UP" and net > 0)
                        or (new_winner_dir == "DOWN" and net < 0)
                    )

                    # Pozisyon değeri kontrolü (avg price ≈ cost / max(up,dn))
                    pos_value = abs(net) * (r["up_best_ask"] if new_winner_dir == "UP" else r["down_best_ask"])

                    if abs(net) == 0 or pos_value < pos_threshold_usd:
                        n_first_flip_too_small += 1
                        # Yine de bot DUR (whipsaw riski)
                        bot_frozen = True
                    elif bot_in_winner:
                        # Bot zaten doğru taraf → skip + dur
                        n_first_flip_skip += 1
                        bot_frozen = True
                    else:
                        # Bot yanlış taraf → SMART HEDGE
                        hedge_size = abs(net) * follow_factor
                        if new_winner_dir == "UP":
                            price = r["up_best_ask"] or 0.99
                            sim_up += hedge_size
                        else:
                            price = r["down_best_ask"] or 0.99
                            sim_dn += hedge_size
                        cost = hedge_size * price
                        sim_cb += cost
                        sim_fee += cost * DRYRUN_FEE
                        extra_cost_total += cost
                        n_first_flip_hedge += 1
                        bot_frozen = True
                elif flip_counter >= 2 and whipsaw_freeze:
                    # Whipsaw market — hala dur
                    n_whipsaw_dur += 1 if flip_counter == 2 else 0
                    bot_frozen = True

        sim_pnl = realized_pnl(sim_cb, sim_up, sim_dn, sim_fee, winner)
        total_pnl += sim_pnl

        diff = sim_pnl - actual_pnl
        if diff > 0.01:
            helped.append((s["slug"], actual_pnl, sim_pnl, diff, flip_counter))
        elif diff < -0.01:
            hurt.append((s["slug"], actual_pnl, sim_pnl, diff, flip_counter))
        if flip_counter == 0:
            n_no_late_flip += 1

    return {
        "label": label,
        "total_pnl": total_pnl,
        "delta": total_pnl - total_actual,
        "n_first_flip_hedge": n_first_flip_hedge,
        "n_first_flip_skip": n_first_flip_skip,
        "n_first_flip_too_small": n_first_flip_too_small,
        "n_whipsaw_dur": n_whipsaw_dur,
        "n_no_late_flip": n_no_late_flip,
        "extra_cost": extra_cost_total,
        "helped": helped,
        "hurt": hurt,
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


print(f"Bot {BOT_ID} — '1 geç-flip detector' politikası simülasyonu\n")
print(f"AKTÜEL toplam PnL: {total_actual:+.2f} USDC\n")

CONFIGS = [
    # (follow_factor, pos_threshold_usd, label, whipsaw_freeze)
    # ── WHIPSAW FREEZE = True (bot 2+ flip'te dur) ──────────────────────
    (0.25, 0,   "ff=0.25 thr=$0   | whipsaw_freeze=ON  (her flip eylem)", True),
    (0.50, 50,  "ff=0.50 thr=$50  | whipsaw_freeze=ON", True),
    (0.25, 100, "ff=0.25 thr=$100 | whipsaw_freeze=ON  (önceki en iyi)", True),
    # ── WHIPSAW FREEZE = False (bot 2+ flip'te olağan devam) ────────────
    (0.25, 0,   "ff=0.25 thr=$0   | whipsaw_freeze=OFF (sadece 1.flip eylem)", False),
    (0.50, 0,   "ff=0.50 thr=$0   | whipsaw_freeze=OFF", False),
    (1.00, 0,   "ff=1.00 thr=$0   | whipsaw_freeze=OFF", False),
    (0.25, 30,  "ff=0.25 thr=$30  | whipsaw_freeze=OFF", False),
    (0.50, 30,  "ff=0.50 thr=$30  | whipsaw_freeze=OFF", False),
    (1.00, 30,  "ff=1.00 thr=$30  | whipsaw_freeze=OFF", False),
    (0.25, 50,  "ff=0.25 thr=$50  | whipsaw_freeze=OFF", False),
    (0.50, 50,  "ff=0.50 thr=$50  | whipsaw_freeze=OFF", False),
    (1.00, 50,  "ff=1.00 thr=$50  | whipsaw_freeze=OFF", False),
    (0.25, 100, "ff=0.25 thr=$100 | whipsaw_freeze=OFF", False),
    (0.50, 100, "ff=0.50 thr=$100 | whipsaw_freeze=OFF", False),
    (1.00, 100, "ff=1.00 thr=$100 | whipsaw_freeze=OFF", False),
]

print(f"{'Konfigürasyon':<55} {'PnL':>10} {'Δ':>9} {'Hedge':>5} {'Skip':>4} {'Whip':>4} {'Küçük':>5} {'Ekstra$':>9}")
print("-" * 110)

results = {}
for ff, pt, label, ws in CONFIGS:
    r = simulate(ff, pt, label, ws)
    results[label] = r
    print(
        f"{label:<60} {r['total_pnl']:+10.2f} {r['delta']:+9.2f} "
        f"{r['n_first_flip_hedge']:>5} {r['n_first_flip_skip']:>4} "
        f"{r['n_whipsaw_dur']:>4} {r['n_first_flip_too_small']:>5} {r['extra_cost']:>9.0f}"
    )

# En iyi varyant detayı
best_label = max(results, key=lambda k: results[k]["total_pnl"])
best = results[best_label]
print(f"\n=== EN İYİ KONFİGÜRASYON: {best_label} ===")
print(f"  Net PnL: {best['total_pnl']:+.2f} (Δ vs aktüel: {best['delta']:+.2f})")
print(f"  Etki dağılımı:")
print(f"    • Geç-flip yok → bot olağan: {best['n_no_late_flip']} market")
print(f"    • İlk flip + hedge tetiklendi: {best['n_first_flip_hedge']} market")
print(f"    • İlk flip + bot doğru taraf → skip+dur: {best['n_first_flip_skip']} market")
print(f"    • İlk flip + pozisyon küçük → sadece dur: {best['n_first_flip_too_small']} market")
print(f"    • Whipsaw (2+ geç-flip) → dur: {best['n_whipsaw_dur']} market")
print(f"  Yardım: {len(best['helped'])} market (toplam +${sum(d for _,_,_,d,_ in best['helped']):.2f})")
print(f"  Zarar: {len(best['hurt'])} market (toplam {sum(d for _,_,_,d,_ in best['hurt']):+.2f})")
print(f"  Ekstra notional: ${best['extra_cost']:.0f}")

print(f"\n  En çok YARDIM ETTİĞİ 10 market:")
print(f"    {'Slug':<32} {'Aktüel':>9} {'Sim':>9} {'Δ':>9} {'#Flip':>5}")
for slug, a, s_pnl, d, n in sorted(best["helped"], key=lambda x: -x[3])[:10]:
    print(f"    {slug:<32} {a:+9.2f} {s_pnl:+9.2f} {d:+9.2f} {n:>5}")

print(f"\n  En çok ZARAR VERDİĞİ 10 market:")
for slug, a, s_pnl, d, n in sorted(best["hurt"], key=lambda x: x[3])[:10]:
    print(f"    {slug:<32} {a:+9.2f} {s_pnl:+9.2f} {d:+9.2f} {n:>5}")


# === KARŞILAŞTIRMA: önceki tüm politikalar ===
print("\n\n=== TÜM POLİTİKALAR ÖZET (Bot 66, 232 market, aktüel +$614) ===\n")
print(f"{'Politika':<60} {'PnL':>10} {'Δ':>9} {'Tip':<25}")
print("-" * 110)
print(f"{'AKTÜEL (mevcut bot davranışı)':<60} {total_actual:+10.2f} {0:+9.2f} {'baseline':<25}")
print(f"{'A) SMART HEDGE T-45→T-6 thr=0.5 (önceki)':<60} {'+729.40':>10} {'+115':>9} {'tek-shot, smart':<25}")
print(f"{'E) ASYM-FOLLOW × 0.25 T-45→T-6 (önceki)':<60} {'+775.92':>10} {'+161':>9} {'tek-shot, küçük':<25}")
print(f"{'B) AGGRESSIVE FOLLOW × 1.00 (önceki)':<60} {'+798.34':>10} {'+184':>9} {'tek-shot, agresif':<25}")
print(f"{'Sürekli takip en iyi (önceki)':<60} {'+421.60':>10} {'-142':>9} {'sürekli, gürültülü':<25}")
print(f"{best_label:<60} {best['total_pnl']:+10.2f} {best['delta']:+9.2f} {'1-geç-flip dedektör':<25}")


# M1, M2, M3 detayları
print("\n\n=== M1 / M2 / M3 — '1 GEÇ-FLİP DETECTOR' DETAYLI TIMELINE ===")
TARGETS = {
    "btc-updown-5m-1778204100": "M1",
    "btc-updown-5m-1778213700": "M2",
    "btc-updown-5m-1778217000": "M3",
}
ff_b = next(c[0] for c in CONFIGS if c[2] == best_label)
pt_b = next(c[1] for c in CONFIGS if c[2] == best_label)
ws_b = next(c[3] for c in CONFIGS if c[2] == best_label)

for s in sessions:
    if s["slug"] not in TARGETS:
        continue
    label = TARGETS[s["slug"]]
    sid = s["id"]
    end_ts = s["end_ts"]
    start_ts = s["start_ts"]
    winner = winner_of(sid)
    t_agg_ms = (end_ts - T_AGG) * 1000
    t_end_ms = (end_ts - T_END) * 1000

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

    # Simülasyon — log ile
    print(f"\n━━━ {label} ({s['slug']}) — winner={winner}, aktüel={actual_pnl:+.2f} ━━━")
    sim_cb = sim_up = sim_dn = sim_fee = 0.0
    flip_counter = 0
    bot_frozen = False
    flip_action_done = False
    prev_side = None

    events = []
    for t in trades_by_sess.get(sid, []):
        events.append(("trade", t["ts_ms"], t))
    for r in ticks_by_sess.get(sid, []):
        if t_agg_ms <= r["ts_ms"] <= t_end_ms:
            events.append(("tick", r["ts_ms"], r))
    events.sort(key=lambda x: (x[1], 0 if x[0] == "trade" else 1))

    log = []
    for ev_type, ev_ms, payload in events:
        if ev_type == "trade":
            if bot_frozen:
                continue
            t = payload
            sim_cb += t["size"] * t["price"]
            sim_fee += t["fee"]
            if t["outcome"] == "UP":
                sim_up += t["size"]
            elif t["outcome"] == "DOWN":
                sim_dn += t["size"]
            continue
        r = payload
        ub = r["up_best_bid"]
        cur_side = "UP" if ub > 0.50 else ("DOWN" if ub < 0.50 else None)
        if cur_side is None or prev_side is None:
            if cur_side is not None:
                prev_side = cur_side
            continue
        if cur_side != prev_side:
            flip_counter += 1
            prev_side = cur_side
            rel_sec = (ev_ms - start_ts * 1000) / 1000
            net = sim_up - sim_dn
            new_winner_dir = cur_side
            bot_in_winner = (
                (new_winner_dir == "UP" and net > 0)
                or (new_winner_dir == "DOWN" and net < 0)
            )
            pos_value = abs(net) * (r["up_best_ask"] if new_winner_dir == "UP" else r["down_best_ask"])

            if flip_counter == 1 and not flip_action_done:
                flip_action_done = True
                if abs(net) == 0 or pos_value < pt_b:
                    log.append((rel_sec, "FLIP#1", f"poz küçük ({pos_value:.0f}<{pt_b}), bot DUR"))
                    bot_frozen = True
                elif bot_in_winner:
                    log.append((rel_sec, "FLIP#1", f"bot doğru taraf, SKIP+DUR (poz=${pos_value:.0f})"))
                    bot_frozen = True
                else:
                    hedge_size = abs(net) * ff_b
                    if new_winner_dir == "UP":
                        price = r["up_best_ask"] or 0.99
                        sim_up += hedge_size
                    else:
                        price = r["down_best_ask"] or 0.99
                        sim_dn += hedge_size
                    cost = hedge_size * price
                    sim_cb += cost
                    sim_fee += cost * DRYRUN_FEE
                    log.append((rel_sec, "FLIP#1", f"HEDGE {hedge_size:.0f}{new_winner_dir}@${price:.2f}=${cost:.2f}, sonra DUR"))
                    bot_frozen = True
            elif flip_counter >= 2 and ws_b:
                log.append((rel_sec, f"FLIP#{flip_counter}", "whipsaw — bot zaten DUR"))
            elif flip_counter >= 2:
                log.append((rel_sec, f"FLIP#{flip_counter}", "whipsaw_freeze=OFF — bot olağan devam"))

    sim_pnl = realized_pnl(sim_cb, sim_up, sim_dn, sim_fee, winner)
    print(f"  Sim son durum: cost=${sim_cb:.2f}, UP={sim_up:.0f}, DN={sim_dn:.0f}")
    print(f"  Sim PnL: {sim_pnl:+.2f}  (Δ vs aktüel: {sim_pnl-actual_pnl:+.2f})")
    print(f"  Toplam geç-flip sayısı: {flip_counter}")
    print(f"  Olay log:")
    for rel, tag, msg in log[:10]:
        print(f"    @{rel:>5.0f}s  [{tag}]  {msg}")
    if len(log) > 10:
        print(f"    ... +{len(log)-10} olay daha")
