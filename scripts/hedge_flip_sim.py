"""Bonereaper "0.5 favorit flip" hedge simülasyonu — Bot 66 tüm marketleri.

POLİTİKA:
  T_agg  = end_ts - 75 sn  (AggTrade başlangıcı, %75 noktası)
  T_stop = end_ts -  6 sn  (StopTrade başlangıcı, %98 noktası)

  1. T_agg anında UP_best_bid'e bak:
     - UP_bid > 0.5 → "favori UP"
     - UP_bid < 0.5 → "favori DOWN"
     - UP_bid = 0.5 → kayıt dışı (eşik üstünde değil)

  2. T_agg .. T_stop arasındaki ilk tick'te UP_bid favori sınırını geçerse:
     - Favori UP, UP_bid < 0.5 olur → ANINDA HEDGE
     - Favori DOWN, UP_bid > 0.5 olur → ANINDA HEDGE

  3. Hedge: O anki taker fiyatından (karşı tarafın best_ask) net pozisyonu eşitle.
     Hedge sonrası bot DURUR (yeni emir vermez).

Ek varyantlar: hedge eşiği 0.5 sabit; ek olarak 0.45/0.55 "buffer" varyantı
(false positive azaltmak için) ve T_agg yerine T_60 / T_45 başlangıçları test edilir.
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
    "SELECT id, slug, start_ts, end_ts FROM market_sessions WHERE bot_id = ?",
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


def state_at(sess_id: int, cutoff_ms: int):
    cb = up = dn = fee = 0.0
    for t in trades_by_sess.get(sess_id, []):
        if t["ts_ms"] > cutoff_ms:
            break
        notional = t["size"] * t["price"]
        cb += notional
        fee += t["fee"]
        if t["outcome"] == "UP":
            up += t["size"]
        elif t["outcome"] == "DOWN":
            dn += t["size"]
    return cb, up, dn, fee


def tick_at(sess_id: int, cutoff_ms: int) -> Optional[sqlite3.Row]:
    chosen = None
    for r in ticks_by_sess.get(sess_id, []):
        if r["ts_ms"] > cutoff_ms:
            break
        chosen = r
    return chosen


def winner_of(sess_id: int):
    ticks = ticks_by_sess.get(sess_id, [])
    if not ticks:
        return None
    last = ticks[-1]
    if last["up_best_bid"] > 0.95:
        return "UP"
    if last["down_best_bid"] > 0.95:
        return "DOWN"
    return None


def realized_pnl(cb: float, up: float, dn: float, fee: float, winner: Optional[str]):
    if winner == "UP":
        return up - cb
    if winner == "DOWN":
        return dn - cb
    return -cb


def detect_flip(
    sess_id: int,
    t_agg_ms: int,
    t_stop_ms: int,
    up_threshold: float,
    down_threshold: float,
) -> tuple[Optional[sqlite3.Row], Optional[str]]:
    """T_agg anındaki tick'in UP_bid'ine göre favori belirlenir; arada
    favorinin değiştiği ilk tick döner. Eşik:
      - UP favori → UP_bid < down_threshold (örn. 0.5 veya 0.45)
      - DOWN favori → UP_bid > up_threshold (örn. 0.5 veya 0.55)
    """
    initial = tick_at(sess_id, t_agg_ms)
    if initial is None:
        return None, None
    init_up = initial["up_best_bid"]
    if init_up > 0.5:
        favorite = "UP"
        cross_threshold = down_threshold
    elif init_up < 0.5:
        favorite = "DOWN"
        cross_threshold = up_threshold
    else:
        return None, None
    for r in ticks_by_sess.get(sess_id, []):
        if r["ts_ms"] <= t_agg_ms:
            continue
        if r["ts_ms"] > t_stop_ms:
            break
        if favorite == "UP" and r["up_best_bid"] < cross_threshold:
            return r, favorite
        if favorite == "DOWN" and r["up_best_bid"] > cross_threshold:
            return r, favorite
    return None, favorite


def simulate(
    t_agg_offset: int,
    t_stop_offset: int,
    up_threshold: float,
    down_threshold: float,
    label: str,
    smart: bool = False,
):
    """smart=True → hedge sadece bot 'yanlış tarafta' ise tetiklenir.

    Mantık:
      - UP favori başlangıçta, sonra UP_bid <0.5 (DOWN'a kayıyor):
          bot UP ağırlıklı (yanlış) → HEDGE (DOWN al)
          bot DOWN ağırlıklı (zaten doğru tarafta) → HEDGE YAPMA
      - DOWN favori başlangıçta, sonra UP_bid >0.5 (UP'a kayıyor):
          bot DOWN ağırlıklı (yanlış) → HEDGE (UP al)
          bot UP ağırlıklı (zaten doğru) → HEDGE YAPMA
    """
    total_pnl = 0.0
    n_hedged = 0
    n_flips_no_pos = 0
    helped: list[tuple[str, float, float]] = []
    hurt: list[tuple[str, float, float]] = []
    pnl_per_market: dict[str, float] = {}
    flips_by_fav = {"UP": 0, "DOWN": 0}
    win_after_flip = 0
    loss_after_flip = 0
    hedge_total_cost = 0.0

    for s in sessions:
        sid = s["id"]
        end_ts = s["end_ts"]
        t_agg_ms = (end_ts - t_agg_offset) * 1000
        t_stop_ms = (end_ts - t_stop_offset) * 1000

        flip_tick, favorite = detect_flip(
            sid, t_agg_ms, t_stop_ms, up_threshold, down_threshold
        )
        winner = winner_of(sid)

        actual_cb, actual_up, actual_dn, actual_fee = state_at(sid, end_ts * 1000)
        actual_pnl = realized_pnl(actual_cb, actual_up, actual_dn, actual_fee, winner)
        if actual_cb == 0 and actual_up == 0 and actual_dn == 0:
            continue

        if flip_tick is None:
            # Flip yok → bot olağan davransın, aktüel PnL'yi al
            total_pnl += actual_pnl
            pnl_per_market[s["slug"]] = actual_pnl
            continue

        # Flip tetiklendi → o anki snapshot
        cutoff_ms = flip_tick["ts_ms"]
        cb_t, up_t, dn_t, fee_t = state_at(sid, cutoff_ms)
        net = up_t - dn_t

        if cb_t == 0 and abs(net) == 0:
            # Flip oldu ama bot pozisyon almamış → hedge gereksiz
            n_flips_no_pos += 1
            total_pnl += actual_pnl  # zaten 0
            pnl_per_market[s["slug"]] = actual_pnl
            continue

        # Smart varyantı: bot zaten "doğru tarafta" ise hedge yapma
        if smart:
            # Favori UP, flip → UP_bid<0.5: market DOWN'a kayıyor
            # bot DOWN ağırlıklı (net<0) ise zaten doğru taraf → SKIP
            # Favori DOWN, flip → UP_bid>0.5: market UP'a kayıyor
            # bot UP ağırlıklı (net>0) ise zaten doğru taraf → SKIP
            if (favorite == "UP" and net < 0) or (favorite == "DOWN" and net > 0):
                # Bot zaten yeni doğru tarafta — sadece DUR (yeni emir verme)
                sim_pnl_skip = realized_pnl(cb_t, up_t, dn_t, fee_t, winner)
                total_pnl += sim_pnl_skip
                pnl_per_market[s["slug"]] = sim_pnl_skip
                diff = sim_pnl_skip - actual_pnl
                if diff > 0.01:
                    helped.append((s["slug"], actual_pnl, sim_pnl_skip))
                    win_after_flip += 1
                elif diff < -0.01:
                    hurt.append((s["slug"], actual_pnl, sim_pnl_skip))
                    loss_after_flip += 1
                continue

        # Hedge: net pozisyonu eşitle
        if abs(net) > 0:
            hedge_size = abs(net)
            if net < 0:  # DOWN ağırlık → UP al
                hedge_price = flip_tick["up_best_ask"] or 0.99
                up_t += hedge_size
            else:  # UP ağırlık → DOWN al
                hedge_price = flip_tick["down_best_ask"] or 0.99
                dn_t += hedge_size
            hedge_cost = hedge_size * hedge_price
            hedge_fee = hedge_cost * DRYRUN_FEE
            cb_t += hedge_cost
            fee_t += hedge_fee
            hedge_total_cost += hedge_cost
            n_hedged += 1
            flips_by_fav[favorite] = flips_by_fav.get(favorite, 0) + 1

        # Bot DURUR — flip sonrası tüm trade'leri görmezden gel
        sim_pnl = realized_pnl(cb_t, up_t, dn_t, fee_t, winner)
        total_pnl += sim_pnl
        pnl_per_market[s["slug"]] = sim_pnl

        diff = sim_pnl - actual_pnl
        if diff > 0.01:
            helped.append((s["slug"], actual_pnl, sim_pnl))
            win_after_flip += 1
        elif diff < -0.01:
            hurt.append((s["slug"], actual_pnl, sim_pnl))
            loss_after_flip += 1

    return {
        "label": label,
        "total_pnl": total_pnl,
        "n_hedged": n_hedged,
        "n_flips_no_pos": n_flips_no_pos,
        "wins": win_after_flip,
        "losses": loss_after_flip,
        "helped": helped,
        "hurt": hurt,
        "flips_by_fav": flips_by_fav,
        "hedge_total_cost": hedge_total_cost,
    }


# Aktüel
total_actual = 0.0
for s in sessions:
    sid = s["id"]
    cb, up, dn, fee = state_at(sid, s["end_ts"] * 1000)
    if cb == 0 and up == 0 and dn == 0:
        continue
    total_actual += realized_pnl(cb, up, dn, fee, winner_of(sid))

print(f"Bot {BOT_ID} — toplam {len(sessions)} market\n")
print(f"AKTÜEL PnL: {total_actual:+.2f} USDC\n")

print("0.5 FAVORİT FLIP HEDGE — AggTrade [..T_agg..] başı, StopTrade [..T_stop..] sonu\n")
print(
    f"{'Konfigürasyon':<48} {'PnL':>10} {'Δ':>9} "
    f"{'Hedge':>6} {'Yardım':>7} {'Zarar':>6} {'HedgeMaliyet':>12}"
)
print("-" * 105)

CONFIGS = [
    # (t_agg_offset, t_stop_offset, up_thr, down_thr, label, smart)
    # ── PREVIOUS: AggTrade → StopTrade başlangıcı ─────────────────────
    (45, 6, 0.50, 0.50, "T-45 → T-6  | thr=0.50 | basit", False),
    (45, 6, 0.50, 0.50, "T-45 → T-6  | thr=0.50 | SMART", True),
    (45, 6, 0.55, 0.45, "T-45 → T-6  | thr=0.55/0.45 | SMART", True),
    # ── NEW: FakTrade + StopTrade tamamı (T-30 → T-0) ─────────────────
    (30, 0, 0.50, 0.50, "T-30 → T-0  | thr=0.50 | basit (FakTrade+StopTrade)", False),
    (30, 0, 0.50, 0.50, "T-30 → T-0  | thr=0.50 | SMART", True),
    (30, 0, 0.55, 0.45, "T-30 → T-0  | thr=0.55/0.45 | SMART (buffer)", True),
    (30, 0, 0.60, 0.40, "T-30 → T-0  | thr=0.60/0.40 | SMART (geniş)", True),
    # ── Karşılaştırma: T-30 ile farklı bitiş ──────────────────────────
    (30, 6, 0.50, 0.50, "T-30 → T-6  | thr=0.50 | SMART", True),
    (30, 15, 0.50, 0.50, "T-30 → T-15 | thr=0.50 | SMART (sadece FakTrade üst yarı)", True),
]

results = []
for cfg in CONFIGS:
    r = simulate(*cfg)
    results.append((cfg, r))
    delta = r["total_pnl"] - total_actual
    print(
        f"{r['label']:<55} {r['total_pnl']:+10.2f} {delta:+9.2f} "
        f"{r['n_hedged']:>6} {r['wins']:>7} {r['losses']:>6} {r['hedge_total_cost']:>12.0f}"
    )

print("\n--- En iyi konfigürasyon detayı ---")
best = max(results, key=lambda x: x[1]["total_pnl"])
cfg_best, r_best = best
print(f"\nKonfigürasyon: {r_best['label']}")
print(f"  PnL: {r_best['total_pnl']:+.2f} USDC (Δ vs aktüel: {r_best['total_pnl']-total_actual:+.2f})")
print(f"  Hedge tetiklendi: {r_best['n_hedged']} market")
print(f"  Pozisyon yokken flip oldu: {r_best['n_flips_no_pos']} market (hedge yapılmadı)")
print(f"  Flip → favori dağılımı: {r_best['flips_by_fav']}")
print(f"  Hedge'in YARDIM ettiği: {r_best['wins']} | ZARAR verdiği: {r_best['losses']}")

print(f"\n  En çok YARDIM ETTİĞİ 7 market:")
print(f"    {'Slug':<32} {'Aktüel':>9} {'Hedge':>9} {'Δ':>9}")
for slug, a, h in sorted(r_best["helped"], key=lambda x: -(x[2] - x[1]))[:7]:
    print(f"    {slug:<32} {a:+9.2f} {h:+9.2f} {h-a:+9.2f}")

print(f"\n  En çok ZARAR VERDİĞİ 7 market:")
for slug, a, h in sorted(r_best["hurt"], key=lambda x: x[2] - x[1])[:7]:
    print(f"    {slug:<32} {a:+9.2f} {h:+9.2f} {h-a:+9.2f}")

# İki politikanın aynı marketlerde aynı kararı verip vermediği — TUTARLILIK KONTROLÜ
print("\n--- TUTARLILIK KONTROLÜ: T-45→T-6 vs T-30→T-0 (ikisi de SMART, thr=0.50) ---\n")
r_old = simulate(45, 6, 0.50, 0.50, "T-45→T-6 SMART", smart=True)
r_new = simulate(30, 0, 0.50, 0.50, "T-30→T-0 SMART", smart=True)

helped_old = {slug for slug, _, _ in r_old["helped"]}
hurt_old = {slug for slug, _, _ in r_old["hurt"]}
helped_new = {slug for slug, _, _ in r_new["helped"]}
hurt_new = {slug for slug, _, _ in r_new["hurt"]}

both_helped = helped_old & helped_new
both_hurt = hurt_old & hurt_new
only_old = (helped_old | hurt_old) - (helped_new | hurt_new)
only_new = (helped_new | hurt_new) - (helped_old | hurt_old)
sign_change = (helped_old & hurt_new) | (hurt_old & helped_new)

print(f"  T-45→T-6 SMART  müdahale: {len(helped_old | hurt_old):>3} market (yardım {len(helped_old)}, zarar {len(hurt_old)})")
print(f"  T-30→T-0 SMART  müdahale: {len(helped_new | hurt_new):>3} market (yardım {len(helped_new)}, zarar {len(hurt_new)})")
print(f"  ─ Her ikisi de yardım: {len(both_helped):>3}")
print(f"  ─ Her ikisi de zarar:  {len(both_hurt):>3}")
print(f"  ─ Sadece T-45→T-6:     {len(only_old):>3} (yeni politika kaçırdı)")
print(f"  ─ Sadece T-30→T-0:     {len(only_new):>3} (yeni politika ek yakaladı)")
print(f"  ─ Yön değişti (yardım↔zarar): {len(sign_change):>3}")

if sign_change:
    print(f"\n  Yön değişen marketler:")
    for slug in sorted(sign_change):
        old_pnl = next((h for s, _, h in r_old["helped"] if s == slug), None) or \
                  next((h for s, _, h in r_old["hurt"] if s == slug), None)
        new_pnl = next((h for s, _, h in r_new["helped"] if s == slug), None) or \
                  next((h for s, _, h in r_new["hurt"] if s == slug), None)
        actual = next((a for s, a, _ in r_old["helped"] + r_old["hurt"] if s == slug), None)
        print(f"    {slug}  aktüel={actual:+.2f}  T-45→T-6={old_pnl:+.2f}  T-30→T-0={new_pnl:+.2f}")


# M1, M2, M3 detayları — HER İKİ POLİTİKAYI YAN YANA
print("\n\n--- M1 / M2 / M3 — HER İKİ POLİTİKA YAN YANA (mantık doğrulaması) ---\n")
TARGETS = {
    "btc-updown-5m-1778204100": "M1",
    "btc-updown-5m-1778213700": "M2",
    "btc-updown-5m-1778217000": "M3",
}
def show_market_for_policy(sess, t_agg_offset, t_stop_offset, up_thr, down_thr, smart, label):
    """Tek market için politikanın ne yaptığını detaylı bas."""
    sid = sess["id"]
    end_ts = sess["end_ts"]
    t_agg_ms = (end_ts - t_agg_offset) * 1000
    t_stop_ms = (end_ts - t_stop_offset) * 1000

    init_tick = tick_at(sid, t_agg_ms)
    flip_tick, favorite = detect_flip(sid, t_agg_ms, t_stop_ms, up_thr, down_thr)
    winner = winner_of(sid)

    init_up = init_tick["up_best_bid"] if init_tick else 0.0
    cb_act, up_act, dn_act, fee_act = state_at(sid, end_ts * 1000)
    actual_pnl = realized_pnl(cb_act, up_act, dn_act, fee_act, winner)

    print(f"  [{label}]")
    print(f"    T_start UP_bid={init_up:.3f} → favori={favorite}, kazanan={winner}")

    if not flip_tick:
        print(f"    Flip TETİKLENMEDİ → bot olağan davranır → PnL: {actual_pnl:+.2f}")
        return actual_pnl

    rel_sec = (flip_tick["ts_ms"] - sess["start_ts"] * 1000) / 1000
    cb_at, up_at, dn_at, fee_at = state_at(sid, flip_tick["ts_ms"])
    net = up_at - dn_at
    bot_dir = "UP" if net > 0 else ("DOWN" if net < 0 else "FLAT")
    new_winner_dir = "DOWN" if favorite == "UP" else "UP"
    bot_in_new_winning_side = (favorite == "UP" and net < 0) or (
        favorite == "DOWN" and net > 0
    )

    print(
        f"    FLIP @ {rel_sec:.0f}s | UP_bid={flip_tick['up_best_bid']:.3f} "
        f"DN_bid={flip_tick['down_best_bid']:.3f}"
    )
    print(
        f"    T anı pozisyon: cost=${cb_at:.2f} UP={up_at:.0f} DN={dn_at:.0f} net={net:+.0f} ({bot_dir})"
    )
    print(f"    Yeni 'kazanan' yön (flip yönü): {new_winner_dir}")

    if smart and bot_in_new_winning_side:
        sim_pnl = realized_pnl(cb_at, up_at, dn_at, fee_at, winner)
        print(f"    → bot {bot_dir} = yeni kazanan yön → SMART SKIP (sadece DUR, hedge YOK)")
    elif abs(net) > 0:
        if net < 0:
            hedge_price = flip_tick["up_best_ask"] or 0.99
            new_up = up_at + abs(net)
            new_dn = dn_at
            buy_side = "UP"
        else:
            hedge_price = flip_tick["down_best_ask"] or 0.99
            new_up = up_at
            new_dn = dn_at + abs(net)
            buy_side = "DOWN"
        hcost = abs(net) * hedge_price
        new_cb = cb_at + hcost
        sim_pnl = realized_pnl(new_cb, new_up, new_dn, fee_at + hcost * DRYRUN_FEE, winner)
        print(
            f"    → bot {bot_dir} ≠ yeni kazanan ({new_winner_dir}) → "
            f"HEDGE: {abs(net):.0f} {buy_side} @ ${hedge_price:.3f} = ${hcost:.2f}"
        )
    else:
        sim_pnl = actual_pnl
        print(f"    net=0 → hedge yok, bot durur")

    print(f"    Aktüel PnL: {actual_pnl:+.2f}  →  Sim PnL: {sim_pnl:+.2f}  (Δ {sim_pnl-actual_pnl:+.2f})")
    return sim_pnl


for s in sessions:
    if s["slug"] not in TARGETS:
        continue
    label = TARGETS[s["slug"]]
    print(f"━━━ {label} ({s['slug']}) ━━━")
    show_market_for_policy(s, 45, 6, 0.50, 0.50, True, "T-45 → T-6  SMART thr=0.50")
    show_market_for_policy(s, 30, 0, 0.50, 0.50, True, "T-30 → T-0  SMART thr=0.50 (yeni)")
    show_market_for_policy(s, 30, 0, 0.55, 0.45, True, "T-30 → T-0  SMART thr=0.55/0.45 (yeni buffer)")
    print()
