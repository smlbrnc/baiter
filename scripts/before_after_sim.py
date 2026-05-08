"""Bot 66 — E × 0.25 ASYM-FOLLOW fix öncesi/sonrası market-by-market kıyaslama.

POLİTİKA (E × 0.25):
  T-45 anında favori belirle (UP_bid > 0.5 → UP, < 0.5 → DOWN)
  T-45 .. T-6 arası UP_bid 0.5 sınırını ters yöne geçen ilk tick = FLIP

  Flip + bot yanlış tarafta → |net| × 0.25 adet yeni yöne taker emir + DUR
  Flip + bot doğru tarafta → SKIP, sadece DUR
  Flip + bot pozisyonsuz → no-op
  Flip yok → bot olağan davranır
"""

import sqlite3
from collections import defaultdict
from typing import Optional

DB = "/home/ubuntu/baiter/data/baiter.db"
BOT_ID = 66
DRYRUN_FEE = 0.0002
T_AGG = 45
T_STOP = 6
THR = 0.50
FOLLOW_FACTOR = 0.25

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


def state_at(sid, cutoff_ms):
    cb = up = dn = fee = 0.0
    for t in trades_by_sess.get(sid, []):
        if t["ts_ms"] > cutoff_ms:
            break
        cb += t["size"] * t["price"]
        fee += t["fee"]
        if t["outcome"] == "UP":
            up += t["size"]
        elif t["outcome"] == "DOWN":
            dn += t["size"]
    return cb, up, dn, fee


def tick_at(sid, cutoff_ms):
    chosen = None
    for r in ticks_by_sess.get(sid, []):
        if r["ts_ms"] > cutoff_ms:
            break
        chosen = r
    return chosen


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


def detect_flip(sid, t_agg_ms, t_stop_ms):
    initial = tick_at(sid, t_agg_ms)
    if initial is None:
        return None, None
    init_up = initial["up_best_bid"]
    if init_up > 0.5:
        favorite, cross = "UP", THR
    elif init_up < 0.5:
        favorite, cross = "DOWN", THR
    else:
        return None, None
    for r in ticks_by_sess.get(sid, []):
        if r["ts_ms"] <= t_agg_ms:
            continue
        if r["ts_ms"] > t_stop_ms:
            break
        if favorite == "UP" and r["up_best_bid"] < cross:
            return r, favorite
        if favorite == "DOWN" and r["up_best_bid"] > cross:
            return r, favorite
    return None, favorite


# === Tüm marketleri analiz et ===
class MarketResult:
    __slots__ = (
        "slug", "winner", "actual_pnl", "fixed_pnl", "delta", "category",
        "actual_cost", "actual_up", "actual_dn",
        "flip_secs", "favorite", "new_dir", "net_at_flip", "follow_size",
        "follow_price", "extra_cost",
    )
    def __init__(self):
        self.slug = ""
        self.winner = None
        self.actual_pnl = 0.0
        self.fixed_pnl = 0.0
        self.delta = 0.0
        self.category = ""
        self.actual_cost = 0.0
        self.actual_up = 0.0
        self.actual_dn = 0.0
        self.flip_secs = None
        self.favorite = None
        self.new_dir = None
        self.net_at_flip = None
        self.follow_size = 0.0
        self.follow_price = 0.0
        self.extra_cost = 0.0


results = []
for s in sessions:
    r = MarketResult()
    r.slug = s["slug"]
    sid = s["id"]
    end_ts = s["end_ts"]
    winner = winner_of(sid)
    r.winner = winner

    # Aktüel
    cb_a, up_a, dn_a, fee_a = state_at(sid, end_ts * 1000)
    r.actual_cost = cb_a
    r.actual_up = up_a
    r.actual_dn = dn_a
    r.actual_pnl = realized_pnl(cb_a, up_a, dn_a, fee_a, winner)

    # Fix uygulanmış simülasyon
    t_agg_ms = (end_ts - T_AGG) * 1000
    t_stop_ms = (end_ts - T_STOP) * 1000
    flip_tick, favorite = detect_flip(sid, t_agg_ms, t_stop_ms)
    r.favorite = favorite

    if cb_a == 0 and up_a == 0 and dn_a == 0:
        # Bot bu markette hiç işlem yapmadı
        r.category = "X-Trade-Yok"
        r.fixed_pnl = 0.0
        r.delta = 0.0
        results.append(r)
        continue

    if flip_tick is None:
        # Flip oluşmadı → bot olağan davranır → fix etkisiz
        r.category = "A-Flip-Yok"
        r.fixed_pnl = r.actual_pnl
        r.delta = 0.0
        results.append(r)
        continue

    # Flip oluştu → T anındaki snapshot
    r.flip_secs = (flip_tick["ts_ms"] - s["start_ts"] * 1000) / 1000
    cb_t, up_t, dn_t, fee_t = state_at(sid, flip_tick["ts_ms"])
    net = up_t - dn_t
    r.net_at_flip = net
    new_dir = "DOWN" if favorite == "UP" else "UP"
    r.new_dir = new_dir

    if cb_t == 0 and abs(net) == 0:
        # T anında bot pozisyonsuz → no-op
        r.category = "B-Flip-Var-Pozisyonsuz"
        r.fixed_pnl = r.actual_pnl
        r.delta = 0.0
        results.append(r)
        continue

    bot_in_winner = (favorite == "UP" and net < 0) or (favorite == "DOWN" and net > 0)
    if bot_in_winner:
        # SMART SKIP — bot zaten doğru tarafta, sadece DUR (T sonrası ek emirler atılmaz)
        r.category = "C-Skip-Dur"
        sim_pnl = realized_pnl(cb_t, up_t, dn_t, fee_t, winner)
        r.fixed_pnl = sim_pnl
        r.delta = sim_pnl - r.actual_pnl
        results.append(r)
        continue

    # Bot yanlış tarafta → ASYM-FOLLOW × 0.25
    follow_size = abs(net) * FOLLOW_FACTOR
    if new_dir == "UP":
        price = flip_tick["up_best_ask"] or 0.99
        new_up = up_t + follow_size
        new_dn = dn_t
    else:
        price = flip_tick["down_best_ask"] or 0.99
        new_up = up_t
        new_dn = dn_t + follow_size
    extra_cost = follow_size * price
    new_cb = cb_t + extra_cost
    new_fee = fee_t + extra_cost * DRYRUN_FEE
    r.follow_size = follow_size
    r.follow_price = price
    r.extra_cost = extra_cost

    sim_pnl = realized_pnl(new_cb, new_up, new_dn, new_fee, winner)
    r.fixed_pnl = sim_pnl
    r.delta = sim_pnl - r.actual_pnl
    r.category = "D-Yanlis-Side-Follow"
    results.append(r)


# === Özet istatistikler ===
def summarize(rs):
    actual_total = sum(r.actual_pnl for r in rs if r.category != "X-Trade-Yok")
    fixed_total = sum(r.fixed_pnl for r in rs if r.category != "X-Trade-Yok")
    delta_total = fixed_total - actual_total

    actual_wins = sum(1 for r in rs if r.actual_pnl > 0)
    actual_losses = sum(1 for r in rs if r.actual_pnl < 0)
    fixed_wins = sum(1 for r in rs if r.fixed_pnl > 0)
    fixed_losses = sum(1 for r in rs if r.fixed_pnl < 0)

    actual_max_loss = min((r.actual_pnl for r in rs), default=0.0)
    fixed_max_loss = min((r.fixed_pnl for r in rs), default=0.0)
    actual_max_win = max((r.actual_pnl for r in rs), default=0.0)
    fixed_max_win = max((r.fixed_pnl for r in rs), default=0.0)

    extra_notional = sum(r.extra_cost for r in rs)

    sum_loss_actual = sum(r.actual_pnl for r in rs if r.actual_pnl < 0)
    sum_loss_fixed = sum(r.fixed_pnl for r in rs if r.fixed_pnl < 0)
    sum_gain_actual = sum(r.actual_pnl for r in rs if r.actual_pnl > 0)
    sum_gain_fixed = sum(r.fixed_pnl for r in rs if r.fixed_pnl > 0)

    return {
        "n": len(rs),
        "actual_total": actual_total,
        "fixed_total": fixed_total,
        "delta_total": delta_total,
        "actual_wins": actual_wins,
        "actual_losses": actual_losses,
        "fixed_wins": fixed_wins,
        "fixed_losses": fixed_losses,
        "actual_max_loss": actual_max_loss,
        "fixed_max_loss": fixed_max_loss,
        "actual_max_win": actual_max_win,
        "fixed_max_win": fixed_max_win,
        "extra_notional": extra_notional,
        "sum_loss_actual": sum_loss_actual,
        "sum_loss_fixed": sum_loss_fixed,
        "sum_gain_actual": sum_gain_actual,
        "sum_gain_fixed": sum_gain_fixed,
    }


s_all = summarize(results)
print("=" * 90)
print(f"BOT 66 — TOPLAM {s_all['n']} MARKET — FIX ÖNCESİ vs SONRASI")
print("=" * 90)
print()
print(f"{'Metrik':<40} {'Aktüel':>15} {'Fix Sonrası':>15} {'Δ':>15}")
print("-" * 90)
print(f"{'Net PnL (USDC)':<40} {s_all['actual_total']:>+15.2f} {s_all['fixed_total']:>+15.2f} {s_all['delta_total']:>+15.2f}")
print(f"{'Kazanan market':<40} {s_all['actual_wins']:>15} {s_all['fixed_wins']:>15} {s_all['fixed_wins']-s_all['actual_wins']:>+15}")
print(f"{'Kaybeden market':<40} {s_all['actual_losses']:>15} {s_all['fixed_losses']:>15} {s_all['fixed_losses']-s_all['actual_losses']:>+15}")
print(f"{'Win rate (%)':<40} {100*s_all['actual_wins']/s_all['n']:>15.2f} {100*s_all['fixed_wins']/s_all['n']:>15.2f} {100*(s_all['fixed_wins']-s_all['actual_wins'])/s_all['n']:>+15.2f}")
print(f"{'En büyük tek-market zararı':<40} {s_all['actual_max_loss']:>+15.2f} {s_all['fixed_max_loss']:>+15.2f} {s_all['fixed_max_loss']-s_all['actual_max_loss']:>+15.2f}")
print(f"{'En büyük tek-market kazancı':<40} {s_all['actual_max_win']:>+15.2f} {s_all['fixed_max_win']:>+15.2f} {s_all['fixed_max_win']-s_all['actual_max_win']:>+15.2f}")
print(f"{'Toplam zararların toplamı':<40} {s_all['sum_loss_actual']:>+15.2f} {s_all['sum_loss_fixed']:>+15.2f} {s_all['sum_loss_fixed']-s_all['sum_loss_actual']:>+15.2f}")
print(f"{'Toplam kazançların toplamı':<40} {s_all['sum_gain_actual']:>+15.2f} {s_all['sum_gain_fixed']:>+15.2f} {s_all['sum_gain_fixed']-s_all['sum_gain_actual']:>+15.2f}")
print(f"{'Fix kaynaklı ekstra notional':<40} {'-':>15} {'$' + str(round(s_all['extra_notional']))+' USDC':>15}")
print()

# Kategori dağılımı
print("=" * 90)
print("KATEGORİ DAĞILIMI — fix tüm marketlere uygulanınca her market hangi yola düştü?")
print("=" * 90)
cats = defaultdict(list)
for r in results:
    cats[r.category].append(r)

cat_labels = {
    "X-Trade-Yok": "Bot bu markette hiç trade yapmadı",
    "A-Flip-Yok": "Flip oluşmadı (favori T-45 → T-6 arası sabit kaldı)",
    "B-Flip-Var-Pozisyonsuz": "Flip var ama bot T-45'te pozisyonsuz",
    "C-Skip-Dur": "Flip + bot DOĞRU tarafta → SMART SKIP (sadece dur)",
    "D-Yanlis-Side-Follow": "Flip + bot YANLIŞ tarafta → ASYM-FOLLOW × 0.25",
}

print(f"\n{'Kategori':<55} {'#Mkt':>5} {'Σ Aktüel':>10} {'Σ Fixed':>10} {'Σ Δ':>10}")
print("-" * 95)
for code in ["X-Trade-Yok", "A-Flip-Yok", "B-Flip-Var-Pozisyonsuz", "C-Skip-Dur", "D-Yanlis-Side-Follow"]:
    rs = cats.get(code, [])
    sa = sum(r.actual_pnl for r in rs)
    sf = sum(r.fixed_pnl for r in rs)
    print(f"{cat_labels[code]:<55} {len(rs):>5} {sa:>+10.2f} {sf:>+10.2f} {sf-sa:>+10.2f}")

# C kategorisi (smart skip) detay — bu nadir kategorinin etkisi büyük olabilir
print("\n\n" + "=" * 90)
print("KATEGORİ C — SMART SKIP'in kurtardığı marketler (bot dur deyince zarar azaldı)")
print("=" * 90)
print(f"\n{'Slug':<32} {'Win':<5} {'Aktüel':>10} {'Fixed':>10} {'Δ':>10}  {'Bot Pos':>15}")
print("-" * 95)
for r in sorted(cats.get("C-Skip-Dur", []), key=lambda x: x.delta, reverse=True):
    bot_pos = f"UP{r.actual_up:.0f}/DN{r.actual_dn:.0f}"
    print(f"{r.slug:<32} {r.winner or '-':<5} {r.actual_pnl:>+10.2f} {r.fixed_pnl:>+10.2f} {r.delta:>+10.2f}  {bot_pos:>15}")

# D kategorisi (yanlış side follow) detay
print("\n\n" + "=" * 90)
print("KATEGORİ D — ASYM-FOLLOW × 0.25 tetiklendiği marketler (yanlış side)")
print("=" * 90)
print(f"\n{'Slug':<32} {'Win':<5} {'Aktüel':>10} {'Fixed':>10} {'Δ':>10}  {'Follow':>15}")
print("-" * 95)
for r in sorted(cats.get("D-Yanlis-Side-Follow", []), key=lambda x: x.delta, reverse=True):
    follow = f"{r.follow_size:.0f}{r.new_dir}@{r.follow_price:.2f}"
    print(f"{r.slug:<32} {r.winner or '-':<5} {r.actual_pnl:>+10.2f} {r.fixed_pnl:>+10.2f} {r.delta:>+10.2f}  {follow:>15}")


# En çok yardım eden ve en çok zarar veren marketler
print("\n\n" + "=" * 90)
print("EN ÇOK YARDIM EDEN VE ZARAR VEREN 10'AR MARKET")
print("=" * 90)

helped_only = [r for r in results if r.delta > 0]
hurt_only = [r for r in results if r.delta < 0]
print(f"\nYardım edilen: {len(helped_only)} market (toplam +${sum(r.delta for r in helped_only):.2f})")
print(f"Zarar verilen: {len(hurt_only)} market (toplam {sum(r.delta for r in hurt_only):+.2f})")

print(f"\nEn çok YARDIM ETTİĞİ 10:")
print(f"{'Slug':<32} {'Win':<5} {'Aktüel':>10} {'Fixed':>10} {'Δ':>10}")
for r in sorted(helped_only, key=lambda x: -x.delta)[:10]:
    print(f"{r.slug:<32} {r.winner or '-':<5} {r.actual_pnl:>+10.2f} {r.fixed_pnl:>+10.2f} {r.delta:>+10.2f}")

print(f"\nEn çok ZARAR VERDİĞİ 10:")
print(f"{'Slug':<32} {'Win':<5} {'Aktüel':>10} {'Fixed':>10} {'Δ':>10}")
for r in sorted(hurt_only, key=lambda x: x.delta)[:10]:
    print(f"{r.slug:<32} {r.winner or '-':<5} {r.actual_pnl:>+10.2f} {r.fixed_pnl:>+10.2f} {r.delta:>+10.2f}")


# Kümülatif PnL trendi (gün boyu)
print("\n\n" + "=" * 90)
print("ZAMANSAL DAĞILIM — Saatlik PnL toplamları")
print("=" * 90)

import datetime
hourly = defaultdict(lambda: {"actual": 0.0, "fixed": 0.0, "n": 0})
for r, s in zip(results, sessions):
    if r.category == "X-Trade-Yok":
        continue
    dt = datetime.datetime.utcfromtimestamp(s["start_ts"])
    key = dt.strftime("%Y-%m-%d %H:00")
    hourly[key]["actual"] += r.actual_pnl
    hourly[key]["fixed"] += r.fixed_pnl
    hourly[key]["n"] += 1

print(f"\n{'Saat (UTC)':<17} {'#Mkt':>5} {'Aktüel':>10} {'Fixed':>10} {'Δ':>10}")
print("-" * 60)
cum_actual = 0.0
cum_fixed = 0.0
for key in sorted(hourly.keys()):
    h = hourly[key]
    cum_actual += h["actual"]
    cum_fixed += h["fixed"]
    print(f"{key:<17} {h['n']:>5} {h['actual']:>+10.2f} {h['fixed']:>+10.2f} {h['fixed']-h['actual']:>+10.2f}")
print("-" * 60)
print(f"{'KÜMÜLATİF':<17} {'':>5} {cum_actual:>+10.2f} {cum_fixed:>+10.2f} {cum_fixed-cum_actual:>+10.2f}")
