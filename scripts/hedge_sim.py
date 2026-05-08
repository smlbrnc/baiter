"""Bonereaper hedge simülasyonu — Bot 66 tüm marketleri.

Politikalar:
  C  : "Coward" — T anında bot durur, hedge yapmaz. Açık pozisyon olduğu gibi.
  HA : "Always Hedge" — T anında net pozisyonu eşitle (taker, karşı tarafın ask).
  HC : "Conditional Hedge (BBA)" — T anında karşı tarafın bid'i >= 0.50 ise hedge.
  HS : "Conditional Hedge (Signal flip)" — T anındaki composite sinyal pozisyonun
       aleyhine ise (DOWN ağırlık + composite > 5.5, veya UP ağırlık + composite < 4.5)
       hedge yap. Triple Gate'in tek bir "yön değişti" sinyalini taklit eder.

Tetikleme zamanları (T = end_ts - X):
  T75 = 75 sn  (AggTrade başlangıcı, %75)
  T30 = 30 sn  (FakTrade başlangıcı, %90)
  T6  =  6 sn  (StopTrade başlangıcı, %98)
"""

import sqlite3
import sys
from collections import defaultdict

DB = "/home/ubuntu/baiter/data/baiter.db"
BOT_ID = 66
DRYRUN_FEE = 0.0002

con = sqlite3.connect(DB)
con.row_factory = sqlite3.Row

sessions = con.execute(
    "SELECT id, slug, start_ts, end_ts FROM market_sessions WHERE bot_id = ?",
    (BOT_ID,),
).fetchall()

trades_by_sess = defaultdict(list)
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


def bba_at(sess_id: int, cutoff_ms: int):
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


def realized_pnl(cb: float, up: float, dn: float, fee: float, winner: str | None):
    if winner == "UP":
        return up - cb
    if winner == "DOWN":
        return dn - cb
    return -cb  # belirsiz: tüm pozisyon değersiz


def simulate(trigger_secs_before_end: int, policy: str):
    total_pnl = 0.0
    total_cb = 0.0
    total_hedge_cost = 0.0
    n_hedged = 0
    pnl_per_market = []

    for s in sessions:
        sid = s["id"]
        end_ms = s["end_ts"] * 1000
        cutoff_ms = (s["end_ts"] - trigger_secs_before_end) * 1000

        cb_t, up_t, dn_t, fee_t = state_at(sid, cutoff_ms)
        bba = bba_at(sid, cutoff_ms)
        winner = winner_of(sid)

        if cb_t == 0 and up_t == 0 and dn_t == 0:
            continue

        net = up_t - dn_t  # + → UP fazla, - → DOWN fazla

        do_hedge = False
        hedge_cost = 0.0
        hedge_size = 0.0
        hedge_fee = 0.0
        if abs(net) > 0 and bba is not None:
            if policy == "HA":
                do_hedge = True
            elif policy == "HC":
                if net < 0 and bba["up_best_bid"] >= 0.50:
                    do_hedge = True
                elif net > 0 and bba["down_best_bid"] >= 0.50:
                    do_hedge = True
            elif policy == "HS":
                comp = bba["signal_score"] or 5.0
                if net < 0 and comp > 5.5:
                    do_hedge = True
                elif net > 0 and comp < 4.5:
                    do_hedge = True

        if do_hedge:
            hedge_size = abs(net)
            if net < 0:
                hedge_price = bba["up_best_ask"]
                up_t += hedge_size
            else:
                hedge_price = bba["down_best_ask"]
                dn_t += hedge_size
            if hedge_price <= 0:
                hedge_price = 0.99
            hedge_cost = hedge_size * hedge_price
            hedge_fee = hedge_cost * DRYRUN_FEE
            cb_t += hedge_cost
            fee_t += hedge_fee
            n_hedged += 1

        # T sonrası bot'un yaptığı ek trade'leri **görmezden gel**:
        # bot T anında durdu varsayımı.
        m_pnl = realized_pnl(cb_t, up_t, dn_t, fee_t, winner) - hedge_fee
        total_pnl += m_pnl
        total_cb += cb_t
        total_hedge_cost += hedge_cost
        pnl_per_market.append((s["slug"], m_pnl, winner, do_hedge))

    return {
        "total_pnl": total_pnl,
        "total_cb": total_cb,
        "total_hedge_cost": total_hedge_cost,
        "n_hedged": n_hedged,
        "wins": sum(1 for _, p, _, _ in pnl_per_market if p > 0),
        "losses": sum(1 for _, p, _, _ in pnl_per_market if p < 0),
        "n": len(pnl_per_market),
        "best": max(pnl_per_market, key=lambda x: x[1], default=None),
        "worst": min(pnl_per_market, key=lambda x: x[1], default=None),
    }


def actual_pnl():
    total = 0.0
    wins = losses = 0
    for s in sessions:
        sid = s["id"]
        end_ms = s["end_ts"] * 1000
        cb, up, dn, fee = state_at(sid, end_ms)
        winner = winner_of(sid)
        if cb == 0 and up == 0 and dn == 0:
            continue
        p = realized_pnl(cb, up, dn, fee, winner)
        total += p
        if p > 0:
            wins += 1
        elif p < 0:
            losses += 1
    return total, wins, losses


print(f"Toplam market: {len(sessions)}, BOT_ID={BOT_ID}\n")

actual, aw, al = actual_pnl()
print(f"AKTÜEL (mevcut bot davranışı):")
print(f"  Net PnL: {actual:+.2f} USDC | Kazanan trade: {aw} | Kaybeden: {al}\n")

print("HEDGE SİMÜLASYONU — bot T anında durur, politikaya göre hedge yapar:\n")
header = f"{'Trigger':<6} {'Politika':<20} {'PnL':>10} {'Δ vs aktüel':>14} {'Hedged':>8} {'W/L':>10} {'Hedge $':>10}"
print(header)
print("-" * len(header))

POLICIES = [
    ("C", "Coward (T'de dur)"),
    ("HA", "Always Hedge"),
    ("HC", "Hedge if BBA aleyhe"),
    ("HS", "Hedge if Signal flip"),
]
TRIGGERS = [(75, "T-75"), (60, "T-60"), (45, "T-45"), (30, "T-30"), (15, "T-15"), (6, "T-6")]

for trig_secs, trig_lbl in TRIGGERS:
    for code, name in POLICIES:
        r = simulate(trig_secs, code)
        delta = r["total_pnl"] - actual
        print(
            f"{trig_lbl:<6} {name:<22} {r['total_pnl']:+10.2f} {delta:+14.2f} "
            f"{r['n_hedged']:>8} {r['wins']:>4}/{r['losses']:<5} {r['total_hedge_cost']:>10.0f}"
        )
    print()


print("\n--- TÜM MARKETLERIN AKTUEL PnL DAĞILIMI ---")
all_results = []
for s in sessions:
    sid = s["id"]
    cb, up, dn, fee = state_at(sid, s["end_ts"] * 1000)
    winner = winner_of(sid)
    if cb == 0 and up == 0 and dn == 0:
        continue
    p = realized_pnl(cb, up, dn, fee, winner)
    all_results.append((s["slug"], p, cb, up, dn, winner))

all_results.sort(key=lambda x: x[1])
total_loss = sum(p for _, p, _, _, _, _ in all_results if p < 0)
total_gain = sum(p for _, p, _, _, _, _ in all_results if p > 0)
print(f"Toplam kayıp markette: {total_loss:+.2f}, kazanan: {total_gain:+.2f}, Net: {total_loss + total_gain:+.2f}")
print(f"\nEn KÖTÜ 5 market:")
print(f"{'Slug':<32} {'PnL':>9} {'Cost':>8} {'UP':>6} {'DN':>6} {'Win':<5}")
for slug, p, cb, up, dn, w in all_results[:5]:
    print(f"{slug:<32} {p:+9.2f} {cb:>8.2f} {up:>6.0f} {dn:>6.0f} {w or '-':<5}")
print(f"\nEn İYİ 5 market:")
for slug, p, cb, up, dn, w in all_results[-5:][::-1]:
    print(f"{slug:<32} {p:+9.2f} {cb:>8.2f} {up:>6.0f} {dn:>6.0f} {w or '-':<5}")


print("\n\n--- HEDGE: KÖTÜ MARKETLER vs İYİ MARKETLER (T-30 Always Hedge) ---")
ahedge_per_market: dict[str, float] = {}
for s in sessions:
    sid = s["id"]
    cutoff_ms = (s["end_ts"] - 30) * 1000
    cb_t, up_t, dn_t, fee_t = state_at(sid, cutoff_ms)
    bba = bba_at(sid, cutoff_ms)
    winner = winner_of(sid)
    if cb_t == 0 and up_t == 0 and dn_t == 0:
        continue
    net = up_t - dn_t
    if abs(net) > 0 and bba is not None:
        hsize = abs(net)
        hprice = bba["up_best_ask"] if net < 0 else bba["down_best_ask"]
        if hprice <= 0:
            hprice = 0.99
        hcost = hsize * hprice
        cb_t += hcost
        fee_t += hcost * DRYRUN_FEE
        if net < 0:
            up_t += hsize
        else:
            dn_t += hsize
    p = realized_pnl(cb_t, up_t, dn_t, fee_t, winner)
    ahedge_per_market[s["slug"]] = p

actual_per_market = {slug: p for slug, p, _, _, _, _ in all_results}

helped = []
hurt = []
for slug, actual_p in actual_per_market.items():
    h = ahedge_per_market.get(slug, actual_p)
    diff = h - actual_p
    if diff > 0.01:
        helped.append((slug, actual_p, h, diff))
    elif diff < -0.01:
        hurt.append((slug, actual_p, h, diff))

helped.sort(key=lambda x: -x[3])
hurt.sort(key=lambda x: x[3])
print(f"Hedge YARARLI olduğu market sayısı: {len(helped)} (toplam yarar: {sum(d for _,_,_,d in helped):+.2f})")
print(f"Hedge ZARARLI olduğu market sayısı: {len(hurt)}  (toplam zarar: {sum(d for _,_,_,d in hurt):+.2f})")

print(f"\nHedge'in EN ÇOK YARDIM ETTİĞİ 5 market (kötüden iyiye):")
print(f"{'Slug':<32} {'Aktüel':>9} {'Hedge':>9} {'Δ':>9}")
for slug, a, h, d in helped[:5]:
    print(f"{slug:<32} {a:+9.2f} {h:+9.2f} {d:+9.2f}")
print(f"\nHedge'in EN ÇOK ZARAR VERDİĞİ 5 market (kâr azaltma):")
for slug, a, h, d in hurt[:5]:
    print(f"{slug:<32} {a:+9.2f} {h:+9.2f} {d:+9.2f}")

print("\n--- ÖRNEK 3 MARKET DETAYI (M1=1778204100, M2=1778213700, M3=1778217000) ---")
TARGET_SLUGS = {
    "btc-updown-5m-1778204100": "M1",
    "btc-updown-5m-1778213700": "M2",
    "btc-updown-5m-1778217000": "M3",
}
target_sessions = [s for s in sessions if s["slug"] in TARGET_SLUGS]
print(
    f"\n{'Mkt':<4} {'Trigger':<8} {'Politika':<22} {'PnL':>9} {'Hedge':<7} {'Net poz':>10} {'Cost':>8}"
)
print("-" * 75)
for s in target_sessions:
    label = TARGET_SLUGS[s["slug"]]
    sid = s["id"]
    winner = winner_of(sid)
    for trig_secs, trig_lbl in [(75, "T-75"), (30, "T-30"), (6, "T-6")]:
        cutoff_ms = (s["end_ts"] - trig_secs) * 1000
        cb_t, up_t, dn_t, fee_t = state_at(sid, cutoff_ms)
        bba = bba_at(sid, cutoff_ms)
        net = up_t - dn_t
        for code, name in POLICIES:
            cb2, up2, dn2, fee2 = cb_t, up_t, dn_t, fee_t
            do_h = False
            if abs(net) > 0 and bba is not None:
                if code == "HA":
                    do_h = True
                elif code == "HC":
                    do_h = (net < 0 and bba["up_best_bid"] >= 0.50) or (
                        net > 0 and bba["down_best_bid"] >= 0.50
                    )
                elif code == "HS":
                    comp = bba["signal_score"] or 5.0
                    do_h = (net < 0 and comp > 5.5) or (net > 0 and comp < 4.5)
            if do_h:
                hsize = abs(net)
                hprice = bba["up_best_ask"] if net < 0 else bba["down_best_ask"]
                hcost = hsize * (hprice or 0.99)
                cb2 += hcost
                fee2 += hcost * DRYRUN_FEE
                if net < 0:
                    up2 += hsize
                else:
                    dn2 += hsize
            pnl = realized_pnl(cb2, up2, dn2, fee2, winner)
            tag = "EVET" if do_h else "—"
            print(
                f"{label:<4} {trig_lbl:<8} {name:<22} {pnl:+9.2f} {tag:<7} "
                f"{net:+10.1f} {cb_t:>8.2f}"
            )
        print()
    print(
        f"  └ winner={winner}, açılış BBA UP={bba_at(sid, s['start_ts']*1000+5000)['up_best_bid']:.2f}, "
        f"final UP={ticks_by_sess[sid][-1]['up_best_bid']:.2f}\n"
    )
