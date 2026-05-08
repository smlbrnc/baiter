"""Bot 66 — flip sonrası 5 farklı reaksiyon politikası karşılaştırma.

ORTAK TETİK: T-45 anında UP_bid'e göre favori belirle, T-6'ya kadar tara.
  Favori UP + UP_bid<0.5 → market DOWN'a kayıyor → "yeni kazanan = DOWN"
  Favori DOWN + UP_bid>0.5 → market UP'a kayıyor → "yeni kazanan = UP"

POLİTİKALAR (flip + bot pozisyonsuzsa hepsi pas):

  A) SMART HEDGE (mevcut baseline)
     • Bot zaten yeni kazanan tarafta → SKIP, DUR
     • Bot yanlış tarafta → net pozisyonu eşitle (taker), DUR

  B) AGGRESSIVE FOLLOW (no hedge, sadece yeni yöne dive)
     • Sinyal o anda hangi yöndeyse, ona |net| × follow_factor adet taker emir
     • Bot mevcut pozisyonu DOKUNMAZ
     • Risk: cost artar; flip yönü yanlışsa zarar büyür

  C) FULL FOLLOW (taker eşitleme — yeni yöne hedge gibi ama amaç farklı)
     • Bot net pozisyonu eşitle (yani A ile aynı eşitleme), sonra yeni emir vermez
     • Fark: smart filtre yok — bot doğru tarafta da olsa hedge yapar
     • Aslında: önceki "Always Hedge" politikası

  D) HYBRID = HEDGE + FOLLOW
     • Bot yanlış tarafta → mevcut |net| eşitleme + ek olarak |net| × follow_factor taker yeni yöne
     • Bot doğru tarafta → SKIP, DUR
     • Hem zararı sıfırla hem yeni yönden ek kâr al

  E) ASYMMETRIC FOLLOW (sadece yanlış taraftaysa)
     • Bot yanlış tarafta → eşitleme yapma, sadece yeni yönde |net| × follow_factor taker
     • Bot doğru tarafta → SKIP
     • B'nin smart filtreli versiyonu

follow_factor = 0.5 (varsayılan), 0.25, 1.0 ve 1.5 ile test edilir.
"""

import sqlite3
from collections import defaultdict
from typing import Optional

DB = "/home/ubuntu/baiter/data/baiter.db"
BOT_ID = 66
DRYRUN_FEE = 0.0002
T_AGG = 45
T_STOP = 6
THR_UP = 0.50
THR_DN = 0.50

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
        cb += t["size"] * t["price"]
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


def winner_of(sess_id: int) -> Optional[str]:
    ticks = ticks_by_sess.get(sess_id, [])
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


def detect_flip(sess_id, t_agg_ms, t_stop_ms, up_thr, down_thr):
    initial = tick_at(sess_id, t_agg_ms)
    if initial is None:
        return None, None
    init_up = initial["up_best_bid"]
    if init_up > 0.5:
        favorite = "UP"
        cross_threshold = down_thr
    elif init_up < 0.5:
        favorite = "DOWN"
        cross_threshold = up_thr
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


def apply_policy(
    policy: str,
    favorite: str,
    flip_tick: sqlite3.Row,
    cb_t: float,
    up_t: float,
    dn_t: float,
    fee_t: float,
    follow_factor: float = 0.5,
):
    """Politikaya göre flip anındaki bot pozisyonunu transform et.
    Dönüş: (new_cb, new_up, new_dn, new_fee, action_label)
    """
    net = up_t - dn_t
    new_winner_dir = "DOWN" if favorite == "UP" else "UP"
    bot_in_winner = (favorite == "UP" and net < 0) or (favorite == "DOWN" and net > 0)

    if policy == "A":  # SMART HEDGE
        if bot_in_winner or abs(net) == 0:
            return cb_t, up_t, dn_t, fee_t, "SKIP+DUR"
        # eşitleme
        return _do_hedge(cb_t, up_t, dn_t, fee_t, net, flip_tick, "HEDGE+DUR")

    if policy == "B":  # AGGRESSIVE FOLLOW (no hedge)
        if abs(net) == 0:
            return cb_t, up_t, dn_t, fee_t, "FLAT-NO-OP"
        size = abs(net) * follow_factor
        return _do_taker_buy(cb_t, up_t, dn_t, fee_t, new_winner_dir, size, flip_tick, f"FOLLOW×{follow_factor}")

    if policy == "C":  # FULL FOLLOW = always equalize
        if abs(net) == 0:
            return cb_t, up_t, dn_t, fee_t, "FLAT-NO-OP"
        return _do_hedge(cb_t, up_t, dn_t, fee_t, net, flip_tick, "EQUALIZE+DUR")

    if policy == "D":  # HYBRID: hedge + follow
        if bot_in_winner or abs(net) == 0:
            return cb_t, up_t, dn_t, fee_t, "SKIP+DUR"
        cb_t, up_t, dn_t, fee_t, _ = _do_hedge(cb_t, up_t, dn_t, fee_t, net, flip_tick, "")
        size = abs(net) * follow_factor
        return _do_taker_buy(cb_t, up_t, dn_t, fee_t, new_winner_dir, size, flip_tick, f"HEDGE+FOLLOW×{follow_factor}")

    if policy == "E":  # ASYMMETRIC FOLLOW (only when wrong-side)
        if bot_in_winner or abs(net) == 0:
            return cb_t, up_t, dn_t, fee_t, "SKIP+DUR"
        size = abs(net) * follow_factor
        return _do_taker_buy(cb_t, up_t, dn_t, fee_t, new_winner_dir, size, flip_tick, f"ASYM-FOLLOW×{follow_factor}")

    raise ValueError(f"Bilinmeyen politika: {policy}")


def _do_hedge(cb, up, dn, fee, net, flip_tick, label):
    hsize = abs(net)
    if net < 0:
        hprice = flip_tick["up_best_ask"] or 0.99
        up += hsize
    else:
        hprice = flip_tick["down_best_ask"] or 0.99
        dn += hsize
    hcost = hsize * hprice
    cb += hcost
    fee += hcost * DRYRUN_FEE
    return cb, up, dn, fee, label


def _do_taker_buy(cb, up, dn, fee, direction, size, flip_tick, label):
    if size <= 0:
        return cb, up, dn, fee, "NOOP-zero-size"
    if direction == "UP":
        price = flip_tick["up_best_ask"] or 0.99
        up += size
    else:
        price = flip_tick["down_best_ask"] or 0.99
        dn += size
    cost = size * price
    cb += cost
    fee += cost * DRYRUN_FEE
    return cb, up, dn, fee, label


def simulate(policy: str, follow_factor: float = 0.5):
    total_pnl = 0.0
    n_action = 0
    actions_breakdown = defaultdict(int)
    helped: list[tuple[str, float, float]] = []
    hurt: list[tuple[str, float, float]] = []
    extra_notional = 0.0

    for s in sessions:
        sid = s["id"]
        end_ts = s["end_ts"]
        t_agg_ms = (end_ts - T_AGG) * 1000
        t_stop_ms = (end_ts - T_STOP) * 1000

        flip_tick, favorite = detect_flip(sid, t_agg_ms, t_stop_ms, THR_UP, THR_DN)
        winner = winner_of(sid)

        actual_cb, actual_up, actual_dn, actual_fee = state_at(sid, end_ts * 1000)
        actual_pnl = realized_pnl(actual_cb, actual_up, actual_dn, actual_fee, winner)
        if actual_cb == 0 and actual_up == 0 and actual_dn == 0:
            continue

        if flip_tick is None:
            total_pnl += actual_pnl
            continue

        cb_t, up_t, dn_t, fee_t = state_at(sid, flip_tick["ts_ms"])
        if cb_t == 0 and up_t == 0 and dn_t == 0:
            total_pnl += actual_pnl
            continue

        new_cb, new_up, new_dn, new_fee, action = apply_policy(
            policy, favorite, flip_tick, cb_t, up_t, dn_t, fee_t, follow_factor
        )
        sim_pnl = realized_pnl(new_cb, new_up, new_dn, new_fee, winner)
        total_pnl += sim_pnl

        if "SKIP" not in action and "NOOP" not in action and "FLAT" not in action:
            n_action += 1
            extra_notional += new_cb - cb_t
        actions_breakdown[action] += 1

        diff = sim_pnl - actual_pnl
        if diff > 0.01:
            helped.append((s["slug"], actual_pnl, sim_pnl))
        elif diff < -0.01:
            hurt.append((s["slug"], actual_pnl, sim_pnl))

    return {
        "total_pnl": total_pnl,
        "n_action": n_action,
        "wins": len(helped),
        "losses": len(hurt),
        "helped": helped,
        "hurt": hurt,
        "extra_notional": extra_notional,
        "actions": dict(actions_breakdown),
    }


# Aktüel
total_actual = 0.0
for s in sessions:
    sid = s["id"]
    cb, up, dn, fee = state_at(sid, s["end_ts"] * 1000)
    if cb == 0 and up == 0 and dn == 0:
        continue
    total_actual += realized_pnl(cb, up, dn, fee, winner_of(sid))

print(f"Bot {BOT_ID} — toplam {len(sessions)} market, T_agg=T-{T_AGG}, T_stop=T-{T_STOP}, thr={THR_UP}/{THR_DN}\n")
print(f"AKTÜEL PnL: {total_actual:+.2f} USDC\n")

print(
    f"{'Politika':<60} {'PnL':>10} {'Δ':>9} {'Eylem':>6} {'Y/Z':>9} {'Ekstra$':>10}"
)
print("-" * 109)

CONFIGS = [
    ("A", 0.0,  "A) SMART HEDGE                       (eşitle yanlış-side, sonra dur)"),
    ("C", 0.0,  "C) FULL FOLLOW (=Always Hedge)       (her flip eşitle, sonra dur)"),
    ("E", 0.25, "E) ASYM-FOLLOW (yanlış-side, küçük)  (yeni yön taker × 0.25 net)"),
    ("E", 0.50, "E) ASYM-FOLLOW (yanlış-side, orta)   (yeni yön taker × 0.50 net)"),
    ("E", 1.00, "E) ASYM-FOLLOW (yanlış-side, tam)    (yeni yön taker × 1.00 net)"),
    ("E", 1.50, "E) ASYM-FOLLOW (yanlış-side, agresif)(yeni yön taker × 1.50 net)"),
    ("D", 0.25, "D) HEDGE+FOLLOW (eşitle + ek taker)  (factor 0.25)"),
    ("D", 0.50, "D) HEDGE+FOLLOW                      (factor 0.50)"),
    ("D", 1.00, "D) HEDGE+FOLLOW                      (factor 1.00 — overweight)"),
    ("B", 0.50, "B) AGGRESSIVE FOLLOW (no smart filt) (factor 0.50, hedge yok)"),
    ("B", 1.00, "B) AGGRESSIVE FOLLOW                 (factor 1.00, hedge yok)"),
]

results = {}
for code, ff, label in CONFIGS:
    r = simulate(code, ff)
    results[label] = r
    delta = r["total_pnl"] - total_actual
    print(
        f"{label:<60} {r['total_pnl']:+10.2f} {delta:+9.2f} "
        f"{r['n_action']:>6} {r['wins']:>3}/{r['losses']:<5} {r['extra_notional']:>10.0f}"
    )

# En iyi politika detayı
best_label = max(results, key=lambda k: results[k]["total_pnl"])
best = results[best_label]
print(f"\n--- EN İYİ POLİTİKA: {best_label} ---")
print(f"  PnL: {best['total_pnl']:+.2f} (Δ vs aktüel: {best['total_pnl']-total_actual:+.2f})")
print(f"  Eylem dağılımı: {best['actions']}")
print(f"  Yardım edilen: {best['wins']}, zarar verilen: {best['losses']}")
print(f"  Ekstra notional yatırım: ${best['extra_notional']:.0f}")
print(f"\n  EN ÇOK YARDIM ETTİĞİ 7 market:")
for slug, a, h in sorted(best["helped"], key=lambda x: -(x[2] - x[1]))[:7]:
    print(f"    {slug}  aktüel={a:+8.2f}  sim={h:+8.2f}  Δ={h-a:+8.2f}")
print(f"\n  EN ÇOK ZARAR VERDİĞİ 7 market:")
for slug, a, h in sorted(best["hurt"], key=lambda x: x[2] - x[1])[:7]:
    print(f"    {slug}  aktüel={a:+8.2f}  sim={h:+8.2f}  Δ={h-a:+8.2f}")


# M1, M2, M3 — her politikayı yan yana göster
print("\n\n--- M1 / M2 / M3 — TÜM POLİTİKALAR YAN YANA ---")
TARGETS = {
    "btc-updown-5m-1778204100": "M1",
    "btc-updown-5m-1778213700": "M2",
    "btc-updown-5m-1778217000": "M3",
}
for s in sessions:
    if s["slug"] not in TARGETS:
        continue
    label = TARGETS[s["slug"]]
    sid = s["id"]
    end_ts = s["end_ts"]
    t_agg_ms = (end_ts - T_AGG) * 1000
    t_stop_ms = (end_ts - T_STOP) * 1000
    flip_tick, favorite = detect_flip(sid, t_agg_ms, t_stop_ms, THR_UP, THR_DN)
    winner = winner_of(sid)
    actual_cb, actual_up, actual_dn, actual_fee = state_at(sid, end_ts * 1000)
    actual_pnl = realized_pnl(actual_cb, actual_up, actual_dn, actual_fee, winner)

    print(f"\n━━━ {label} ({s['slug']}) ━━━")
    if flip_tick is None:
        init = tick_at(sid, t_agg_ms)
        print(f"  T-{T_AGG} UP_bid={init['up_best_bid']:.3f} → favori={favorite}, FLIP YOK")
        continue
    cb_t, up_t, dn_t, fee_t = state_at(sid, flip_tick["ts_ms"])
    net = up_t - dn_t
    new_winner_dir = "DOWN" if favorite == "UP" else "UP"
    rel_sec = (flip_tick["ts_ms"] - s["start_ts"] * 1000) / 1000
    print(f"  T-{T_AGG} favori={favorite}, FLIP @{rel_sec:.0f}s → yeni yön={new_winner_dir}")
    print(f"  T anı pozisyon: cost=${cb_t:.2f} UP={up_t:.0f} DN={dn_t:.0f} net={net:+.0f}")
    print(f"  Flip BBA: UP_ask={flip_tick['up_best_ask']:.3f} DN_ask={flip_tick['down_best_ask']:.3f}")
    print(f"  Gerçek kazanan: {winner}, Aktüel PnL: {actual_pnl:+.2f}")
    print()
    for code, ff, plabel in CONFIGS:
        new_cb, new_up, new_dn, new_fee, action = apply_policy(
            code, favorite, flip_tick, cb_t, up_t, dn_t, fee_t, ff
        )
        sim_pnl = realized_pnl(new_cb, new_up, new_dn, new_fee, winner)
        diff = sim_pnl - actual_pnl
        print(f"    {plabel[:55]:<55} {action:<25} sim={sim_pnl:+8.2f} Δ={diff:+8.2f}")
