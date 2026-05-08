"""Bot 66 — TÜM 232 marketin AKTÜEL vs FIX (KELLY p=0.55) yan yana karşılaştırma.

FIX POLİTİKA:
  T-45 → T-6 arası ilk flip tespit edildiğinde:
    Bot zaten doğru tarafta → SMART SKIP + DUR
    Bot yanlış tarafta + Kelly factor (p_win=0.55) > 0 → HEDGE küçük + DUR
    Bot yanlış tarafta + Kelly factor ≤ 0 → DUR (hedge yok)
"""

import sqlite3
import csv
from collections import defaultdict
from typing import Optional

DB = "/home/ubuntu/baiter/data/baiter.db"
BOT_ID = 66
DRYRUN_FEE = 0.0002
T_AGG = 45
T_STOP = 6
P_WIN = 0.55

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
    if initial is None: return None, None
    init_up = initial["up_best_bid"]
    if init_up > 0.5: favorite = "UP"
    elif init_up < 0.5: favorite = "DOWN"
    else: return None, None
    for r in ticks_by_sess.get(sid, []):
        if r["ts_ms"] <= t_start_ms: continue
        if r["ts_ms"] > t_end_ms: break
        if favorite == "UP" and r["up_best_bid"] < 0.5: return r, favorite
        if favorite == "DOWN" and r["up_best_bid"] > 0.5: return r, favorite
    return None, favorite


def kelly_factor(hedge_price):
    if hedge_price <= 0 or hedge_price >= 1: return 0.0
    b = (1 - hedge_price) / hedge_price
    f_star = (b * P_WIN - (1 - P_WIN)) / b
    return max(0.0, min(1.0, f_star))


def simulate_market(s):
    """Tek bir market için aktüel ve fix PnL hesapla."""
    sid = s["id"]
    end_ts = s["end_ts"]
    start_ts = s["start_ts"]
    winner = winner_of(sid)

    # AKTÜEL
    cb_a, up_a, dn_a, fee_a = state_at(sid, end_ts * 1000)
    actual_pnl = realized_pnl(cb_a, up_a, dn_a, fee_a, winner)

    if cb_a == 0 and up_a == 0 and dn_a == 0:
        return {
            "actual_pnl": 0.0, "fix_pnl": 0.0, "delta": 0.0,
            "category": "X-no_trades", "winner": winner,
            "actual_cost": 0.0, "actual_up": 0.0, "actual_dn": 0.0,
            "flip_secs": None, "favorite": None, "new_dir": None,
            "net_at_flip": None, "hedge_price": None, "hedge_size": 0.0,
            "extra_cost": 0.0, "kelly_factor": None,
        }

    # FIX simülasyonu
    t_start_ms = (end_ts - T_AGG) * 1000
    t_end_ms = (end_ts - T_STOP) * 1000
    flip_tick, favorite = detect_first_flip(sid, t_start_ms, t_end_ms)

    if flip_tick is None:
        return {
            "actual_pnl": actual_pnl, "fix_pnl": actual_pnl, "delta": 0.0,
            "category": "A-no_flip", "winner": winner,
            "actual_cost": cb_a, "actual_up": up_a, "actual_dn": dn_a,
            "flip_secs": None, "favorite": favorite, "new_dir": None,
            "net_at_flip": None, "hedge_price": None, "hedge_size": 0.0,
            "extra_cost": 0.0, "kelly_factor": None,
        }

    # Flip anındaki bot pozisyonu
    cb_t, up_t, dn_t, fee_t = state_at(sid, flip_tick["ts_ms"])
    net = up_t - dn_t
    new_dir = "DOWN" if favorite == "UP" else "UP"
    flip_secs = (flip_tick["ts_ms"] - start_ts * 1000) / 1000

    if abs(net) < 0.5:  # bot pozisyonsuz
        sim_pnl = realized_pnl(cb_t, up_t, dn_t, fee_t, winner)
        return {
            "actual_pnl": actual_pnl, "fix_pnl": sim_pnl, "delta": sim_pnl - actual_pnl,
            "category": "B-flip_no_pos", "winner": winner,
            "actual_cost": cb_a, "actual_up": up_a, "actual_dn": dn_a,
            "flip_secs": flip_secs, "favorite": favorite, "new_dir": new_dir,
            "net_at_flip": net, "hedge_price": None, "hedge_size": 0.0,
            "extra_cost": 0.0, "kelly_factor": None,
        }

    bot_in_winner = (new_dir == "UP" and net > 0) or (new_dir == "DOWN" and net < 0)
    hedge_price = flip_tick["up_best_ask"] if new_dir == "UP" else flip_tick["down_best_ask"]
    if hedge_price <= 0: hedge_price = 0.99

    if bot_in_winner:
        # SMART SKIP — bot zaten doğru tarafta, sadece DUR
        sim_pnl = realized_pnl(cb_t, up_t, dn_t, fee_t, winner)
        return {
            "actual_pnl": actual_pnl, "fix_pnl": sim_pnl, "delta": sim_pnl - actual_pnl,
            "category": "C-smart_skip", "winner": winner,
            "actual_cost": cb_a, "actual_up": up_a, "actual_dn": dn_a,
            "flip_secs": flip_secs, "favorite": favorite, "new_dir": new_dir,
            "net_at_flip": net, "hedge_price": hedge_price, "hedge_size": 0.0,
            "extra_cost": 0.0, "kelly_factor": 0.0,
        }

    # Bot yanlış tarafta → Kelly hesabı
    f_star = kelly_factor(hedge_price)
    if f_star <= 0:
        # Kelly negatif → hedge yok, sadece DUR
        sim_pnl = realized_pnl(cb_t, up_t, dn_t, fee_t, winner)
        return {
            "actual_pnl": actual_pnl, "fix_pnl": sim_pnl, "delta": sim_pnl - actual_pnl,
            "category": "D-kelly_neg_dur", "winner": winner,
            "actual_cost": cb_a, "actual_up": up_a, "actual_dn": dn_a,
            "flip_secs": flip_secs, "favorite": favorite, "new_dir": new_dir,
            "net_at_flip": net, "hedge_price": hedge_price, "hedge_size": 0.0,
            "extra_cost": 0.0, "kelly_factor": f_star,
        }

    # Kelly pozitif → küçük hedge + DUR
    size = abs(net) * f_star
    if new_dir == "UP":
        new_up = up_t + size
        new_dn = dn_t
    else:
        new_up = up_t
        new_dn = dn_t + size
    cost = size * hedge_price
    new_cb = cb_t + cost
    new_fee = fee_t + cost * DRYRUN_FEE
    sim_pnl = realized_pnl(new_cb, new_up, new_dn, new_fee, winner)
    return {
        "actual_pnl": actual_pnl, "fix_pnl": sim_pnl, "delta": sim_pnl - actual_pnl,
        "category": "E-kelly_hedge", "winner": winner,
        "actual_cost": cb_a, "actual_up": up_a, "actual_dn": dn_a,
        "flip_secs": flip_secs, "favorite": favorite, "new_dir": new_dir,
        "net_at_flip": net, "hedge_price": hedge_price, "hedge_size": size,
        "extra_cost": cost, "kelly_factor": f_star,
    }


# Tüm marketleri işle
all_results = []
for s in sessions:
    res = simulate_market(s)
    res["slug"] = s["slug"]
    res["start_ts"] = s["start_ts"]
    all_results.append(res)


# CSV olarak kaydet
csv_path = "/tmp/bot66_full_comparison.csv"
with open(csv_path, "w", newline="") as f:
    writer = csv.writer(f)
    writer.writerow([
        "slug", "winner", "category", "actual_pnl", "fix_pnl", "delta",
        "actual_cost", "actual_up", "actual_dn",
        "flip_secs", "favorite", "new_dir", "net_at_flip",
        "hedge_price", "hedge_size", "extra_cost", "kelly_factor",
    ])
    for r in all_results:
        writer.writerow([
            r["slug"], r["winner"] or "-", r["category"],
            f"{r['actual_pnl']:+.2f}", f"{r['fix_pnl']:+.2f}", f"{r['delta']:+.2f}",
            f"{r['actual_cost']:.2f}", f"{r['actual_up']:.0f}", f"{r['actual_dn']:.0f}",
            f"{r['flip_secs']:.0f}" if r['flip_secs'] is not None else "",
            r["favorite"] or "", r["new_dir"] or "",
            f"{r['net_at_flip']:+.0f}" if r['net_at_flip'] is not None else "",
            f"{r['hedge_price']:.3f}" if r['hedge_price'] is not None else "",
            f"{r['hedge_size']:.0f}",
            f"{r['extra_cost']:.2f}",
            f"{r['kelly_factor']:.3f}" if r['kelly_factor'] is not None else "",
        ])

print(f"CSV kaydedildi: {csv_path}\n")

# === ÖZET İSTATİSTİKLER ===
total_actual = sum(r["actual_pnl"] for r in all_results)
total_fix = sum(r["fix_pnl"] for r in all_results)
total_delta = total_fix - total_actual
total_extra_cost = sum(r["extra_cost"] for r in all_results)

actual_wins = sum(1 for r in all_results if r["actual_pnl"] > 0.01)
actual_losses = sum(1 for r in all_results if r["actual_pnl"] < -0.01)
fix_wins = sum(1 for r in all_results if r["fix_pnl"] > 0.01)
fix_losses = sum(1 for r in all_results if r["fix_pnl"] < -0.01)

actual_max_loss = min(r["actual_pnl"] for r in all_results)
fix_max_loss = min(r["fix_pnl"] for r in all_results)
actual_max_win = max(r["actual_pnl"] for r in all_results)
fix_max_win = max(r["fix_pnl"] for r in all_results)

actual_sum_loss = sum(r["actual_pnl"] for r in all_results if r["actual_pnl"] < 0)
fix_sum_loss = sum(r["fix_pnl"] for r in all_results if r["fix_pnl"] < 0)
actual_sum_gain = sum(r["actual_pnl"] for r in all_results if r["actual_pnl"] > 0)
fix_sum_gain = sum(r["fix_pnl"] for r in all_results if r["fix_pnl"] > 0)

print("=" * 100)
print(f"BOT 66 — TÜM {len(all_results)} MARKET — AKTÜEL vs FIX (KELLY p={P_WIN}) KARŞILAŞTIRMA")
print("=" * 100)
print()
print(f"{'Metrik':<45} {'Aktüel':>15} {'Fix':>15} {'Δ':>15}")
print("-" * 100)
print(f"{'Net PnL (USDC)':<45} {total_actual:>+15.2f} {total_fix:>+15.2f} {total_delta:>+15.2f}")
print(f"{'Kazanan market':<45} {actual_wins:>15} {fix_wins:>15} {fix_wins-actual_wins:>+15}")
print(f"{'Kaybeden market':<45} {actual_losses:>15} {fix_losses:>15} {fix_losses-actual_losses:>+15}")
print(f"{'Win rate (%)':<45} {100*actual_wins/len(all_results):>15.2f} {100*fix_wins/len(all_results):>15.2f} {100*(fix_wins-actual_wins)/len(all_results):>+15.2f}")
print(f"{'En büyük tek-market zararı':<45} {actual_max_loss:>+15.2f} {fix_max_loss:>+15.2f} {fix_max_loss-actual_max_loss:>+15.2f}")
print(f"{'En büyük tek-market kazancı':<45} {actual_max_win:>+15.2f} {fix_max_win:>+15.2f} {fix_max_win-actual_max_win:>+15.2f}")
print(f"{'Toplam zararların toplamı':<45} {actual_sum_loss:>+15.2f} {fix_sum_loss:>+15.2f} {fix_sum_loss-actual_sum_loss:>+15.2f}")
print(f"{'Toplam kazançların toplamı':<45} {actual_sum_gain:>+15.2f} {fix_sum_gain:>+15.2f} {fix_sum_gain-actual_sum_gain:>+15.2f}")
print(f"{'Fix kaynaklı ekstra notional':<45} {'-':>15} {'$' + str(round(total_extra_cost)):>15}")
print(f"{'Sermaye verimi (Δ / Ekstra $)':<45} {'-':>15} {f'%{total_delta/max(total_extra_cost,0.01)*100:.0f}':>15}")
print()


# === KATEGORİ DAĞILIMI ===
print("=" * 100)
print("KATEGORİ DAĞILIMI — 232 marketin fix uygulamasındaki yolu")
print("=" * 100)
cats = defaultdict(list)
for r in all_results:
    cats[r["category"]].append(r)

cat_labels = {
    "X-no_trades": "Bot bu markette hiç trade yapmadı",
    "A-no_flip": "Flip oluşmadı (favori sabit) — bot olağan",
    "B-flip_no_pos": "Flip var ama bot pozisyonsuz",
    "C-smart_skip": "Flip + bot DOĞRU tarafta → SMART SKIP+DUR",
    "D-kelly_neg_dur": "Flip + bot yanlış + Kelly negatif → DUR (hedge yok)",
    "E-kelly_hedge": "Flip + bot yanlış + Kelly pozitif → HEDGE+DUR",
}

print(f"\n{'Kategori':<60} {'#Mkt':>5} {'%':>5} {'Σ Aktüel':>10} {'Σ Fix':>10} {'Σ Δ':>10}")
print("-" * 105)
for code in ["X-no_trades", "A-no_flip", "B-flip_no_pos", "C-smart_skip", "D-kelly_neg_dur", "E-kelly_hedge"]:
    rs = cats.get(code, [])
    if not rs: continue
    sa = sum(r["actual_pnl"] for r in rs)
    sf = sum(r["fix_pnl"] for r in rs)
    pct = 100 * len(rs) / len(all_results)
    print(f"{cat_labels[code]:<60} {len(rs):>5} {pct:>4.1f}% {sa:>+10.2f} {sf:>+10.2f} {sf-sa:>+10.2f}")


# === TÜM KATEGORİ C MARKETLERI (Smart Skip) ===
print("\n\n" + "=" * 100)
print("KATEGORİ C — SMART SKIP'in 11 marketi")
print("=" * 100)
print(f"\n{'Slug':<32} {'Win':<5} {'Aktüel':>9} {'Fix':>9} {'Δ':>9}  {'Bot Pos':>15} {'Net@flip':>10}")
print("-" * 105)
for r in sorted(cats.get("C-smart_skip", []), key=lambda x: x["delta"], reverse=True):
    bot_pos = f"UP{r['actual_up']:.0f}/DN{r['actual_dn']:.0f}"
    net_str = f"{r['net_at_flip']:+.0f}" if r['net_at_flip'] is not None else "-"
    print(f"{r['slug']:<32} {r['winner'] or '-':<5} {r['actual_pnl']:>+9.2f} {r['fix_pnl']:>+9.2f} {r['delta']:>+9.2f}  {bot_pos:>15} {net_str:>10}")


# === TÜM KATEGORİ D MARKETLERI (Kelly Negatif) ===
print("\n\n" + "=" * 100)
print("KATEGORİ D — Kelly negatif (DUR ama hedge yok)")
print("=" * 100)
print(f"\n{'Slug':<32} {'Win':<5} {'Aktüel':>9} {'Fix':>9} {'Δ':>9}  {'@px':>5} {'Net@flip':>10}")
print("-" * 100)
for r in sorted(cats.get("D-kelly_neg_dur", []), key=lambda x: x["delta"], reverse=True):
    net_str = f"{r['net_at_flip']:+.0f}" if r['net_at_flip'] is not None else "-"
    px = f"{r['hedge_price']:.2f}"
    print(f"{r['slug']:<32} {r['winner'] or '-':<5} {r['actual_pnl']:>+9.2f} {r['fix_pnl']:>+9.2f} {r['delta']:>+9.2f}  {px:>5} {net_str:>10}")


# === TÜM KATEGORİ E MARKETLERI (Kelly Hedge Yapıldı) ===
print("\n\n" + "=" * 100)
print("KATEGORİ E — Kelly POZITIF (gerçek hedge yapıldı)")
print("=" * 100)
print(f"\n{'Slug':<32} {'Win':<5} {'Aktüel':>9} {'Fix':>9} {'Δ':>9}  {'@px':>5} {'f*':>5} {'Hedge':<15}")
print("-" * 105)
for r in sorted(cats.get("E-kelly_hedge", []), key=lambda x: x["delta"], reverse=True):
    px = f"{r['hedge_price']:.2f}"
    f_star = f"{r['kelly_factor']:.2f}"
    hedge_desc = f"{r['hedge_size']:.0f}{r['new_dir']} ${r['extra_cost']:.1f}"
    print(f"{r['slug']:<32} {r['winner'] or '-':<5} {r['actual_pnl']:>+9.2f} {r['fix_pnl']:>+9.2f} {r['delta']:>+9.2f}  {px:>5} {f_star:>5} {hedge_desc:<15}")


# === EN ÇOK YARDIM EDEN VE ZARAR VEREN ===
helped = [r for r in all_results if r["delta"] > 0.01]
hurt = [r for r in all_results if r["delta"] < -0.01]
print(f"\n\n" + "=" * 100)
print(f"ETKİLENEN MARKETLER — toplam {len(helped) + len(hurt)} market")
print("=" * 100)
print(f"\nYardım edilen: {len(helped)} market (toplam +${sum(r['delta'] for r in helped):.2f})")
print(f"Zarar verilen: {len(hurt)} market (toplam {sum(r['delta'] for r in hurt):+.2f})")
print(f"Net etki: {sum(r['delta'] for r in helped + hurt):+.2f}")

print(f"\nEn çok YARDIM ETTİĞİ TÜM marketler:")
print(f"{'Slug':<32} {'Win':<5} {'Cat':<22} {'Aktüel':>9} {'Fix':>9} {'Δ':>9}")
for r in sorted(helped, key=lambda x: -x["delta"]):
    print(f"{r['slug']:<32} {r['winner'] or '-':<5} {r['category']:<22} {r['actual_pnl']:>+9.2f} {r['fix_pnl']:>+9.2f} {r['delta']:>+9.2f}")

print(f"\nEn çok ZARAR VERDİĞİ TÜM marketler:")
print(f"{'Slug':<32} {'Win':<5} {'Cat':<22} {'Aktüel':>9} {'Fix':>9} {'Δ':>9}")
for r in sorted(hurt, key=lambda x: x["delta"]):
    print(f"{r['slug']:<32} {r['winner'] or '-':<5} {r['category']:<22} {r['actual_pnl']:>+9.2f} {r['fix_pnl']:>+9.2f} {r['delta']:>+9.2f}")


# === SAATLIK KÜMÜLATIF ===
print(f"\n\n" + "=" * 100)
print("SAATLİK PnL TOPLAMLARI — kümülatif gelişim")
print("=" * 100)

import datetime
hourly = defaultdict(lambda: {"actual": 0.0, "fix": 0.0, "n": 0})
for r in all_results:
    if r["category"] == "X-no_trades": continue
    dt = datetime.datetime.fromtimestamp(r["start_ts"], datetime.timezone.utc)
    key = dt.strftime("%Y-%m-%d %H:00")
    hourly[key]["actual"] += r["actual_pnl"]
    hourly[key]["fix"] += r["fix_pnl"]
    hourly[key]["n"] += 1

print(f"\n{'Saat (UTC)':<17} {'#Mkt':>5} {'Aktüel':>10} {'Fix':>10} {'Δ':>10} {'Kümül Δ':>10}")
print("-" * 70)
cum_d = 0.0
for key in sorted(hourly.keys()):
    h = hourly[key]
    d = h["fix"] - h["actual"]
    cum_d += d
    marker = " ★" if abs(d) >= 50 else ""
    print(f"{key:<17} {h['n']:>5} {h['actual']:>+10.2f} {h['fix']:>+10.2f} {d:>+10.2f} {cum_d:>+10.2f}{marker}")
