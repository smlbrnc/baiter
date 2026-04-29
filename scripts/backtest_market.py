#!/usr/bin/env python3
"""Elis backtest — Hibrit Maker Bid Grid (Alis-tabanlı + Composite Signal Yön Filtresi).

Versiyon: v3 (16 marketde test edildi: yön %92, kesin PnL +$609, net +$560)

Kullanım:
    python3 scripts/backtest_market.py <market_slug>

Örnek:
    python3 scripts/backtest_market.py btc-updown-5m-1777476600

10-katman karar zinciri:
  0. Pending (t<20)              — no-op
  1. Opening (t=20)              — composite open_pair (asymmetric)
  2. Deadline (rem≤8s)           — STOP, hiç emir yok
  3. Pre-resolve scoop           — opp_bid≤0.05 + rem≤35s → $50 dom @ ask-1tick
  4. Signal flip (max 1)         — |dscore_from_open|>5.0 → 2x dom + 0.3x hedge + freeze 60s
  5. Lock check                  — avg_sum≤0.97 → kâr garantili, alım yok
  6. Avg-down (one-shot)         — dom_bid+2.3tick≤avg_dom → $15 dom
  7. Pyramid                     — ofi≥0.83 + persist 5s + score yönü match → $15 dom
  8. Dom requote                 — |Δdom|≥2tick + 3s cooldown → $15 dom
  9. Hedge requote (KRİTİK!)     — opp YÜKSELDİ ≥2tick + opp≥0.15 + freeze geçti → $8 hedge
                                   (sadece artış — Alis'in en büyük hatası düzeltildi)
 10. Parity gap                  — |up-dn|>250 + 5s cooldown + freeze geçti → opp $8

Composite opener (5-rule ladder, t=20):
  1. BSI reversion: |bsi|>2.0 → bsi tersi
  2. OFI+CVD exhaustion: |ofi|>0.4 + |cvd|>3 → flow tersi
  3. OFI directional: |ofi|>0.4 → ofi yönü
  4. Strong dscore: |dscore|>1.0 → dscore yönü (momentum)
  5. Fallback: score_avg ≥ 5 → Up

Simülasyon NOTU: %100 fill varsayımı pessimistic değil agresif — gerçek Polymarket fill
rate %30-50, sim PnL gerçek PnL'in 2-3x olabilir.

Detaylı doküman: .cursor/docs/elis-strategy.md
Backtest raporu: exports/backtest-final-16-markets.md
"""

from __future__ import annotations

import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
TICKS_DIR = ROOT / "exports" / "bot14-ticks-20260429"

# ============================================================================
# ELIS PARAMETRELERİ — 16 marketde optimize edildi (yön %92, kesin PnL +$609)
# ============================================================================

# --- TICK ---
TICK_SIZE = 0.01

# --- COMPOSITE OPENER (5-rule ladder, t=20) ---
PRE_OPENER_TICKS = 20      # pre-opener pencere uzunluğu
BSI_REV_TH = 2.0           # rule 1: |bsi|>2.0 → bsi tersi (extreme reversion)
OFI_EXH_TH = 0.4           # rule 2: |ofi|>0.4 + |cvd|>3 → flow tersi (exhaustion)
CVD_EXH_TH = 3.0
OFI_DIR_TH = 0.4           # rule 3: |ofi|>0.4 → ofi yönü (aggressive flow)
DSCORE_STRONG = 1.0        # rule 4: |dscore|>1.0 → dscore yönü (momentum)
SCORE_NEUTRAL = 5.0        # rule 5: fallback — score_avg ≥ 5 → Up

# --- SIGNAL FLIP (yön düzeltici, sadece çok güçlü reversal'da) ---
SIGNAL_FLIP_THRESHOLD = 5.0   # düşük eşik fakeout'lara kapılır (7474800, 7476300)
SIGNAL_FLIP_MAX_COUNT = 1     # tek flip — max=2 zigzag pattern'larda daha kötü
SIGNAL_FLIP_COOLDOWN_S = 0
FLIP_FREEZE_OPP_S = 60        # flip sonrası 60s opp (eski intent) tarafa alım yok

# --- ASYMMETRIC SIZING ---
OPEN_USDC_DOM = 25.0       # opener dom (intent yönü) — ana pozisyon
OPEN_USDC_HEDGE = 12.0     # opener hedge — yarı boy
ORDER_USDC_DOM = 15.0      # requote / avg_down dom
ORDER_USDC_HEDGE = 8.0     # requote hedge
PYRAMID_USDC = 30.0
SCOOP_USDC = 50.0
MAX_SIZE = 50              # tek emir share cap

# --- REQUOTE ---
REQUOTE_PRICE_EPS = TICK_SIZE * 2   # 2 tick
REQUOTE_COOLDOWN_S = 3
# NOT: hedge_requote SADECE opp_bid YÜKSELDİĞİNDE tetiklenir (decide() içinde)
# Alis bot'unun en büyük hatası: opp düşerken de hedge ekleyip kayıp büyütmesi.

# --- AVG-DOWN (one-shot) ---
AVG_DOWN_MIN_EDGE = 2.3 * TICK_SIZE

# --- PYRAMID ---
PYRAMID_OFI_MIN = 0.83
PYRAMID_SCORE_PERSIST_S = 5
PYRAMID_COOLDOWN_S = 3

# --- PARITY ---
PARITY_MIN_GAP_QTY = 250
PARITY_COOLDOWN_S = 5
PARITY_OPP_BID_MIN = 0.15  # opp_bid < 0.15 ise hedge artma (kazanan netleşti)

# --- LOCK ---
LOCK_AVG_THRESHOLD = 0.97  # avg_up + avg_down ≤ 0.97 → kâr garantili

# --- SCOOP (pre-resolve) ---
SCOOP_OPP_BID_MAX = 0.05
SCOOP_MIN_REMAINING_S = 35
SCOOP_COOLDOWN_S = 2

# --- DEADLINE ---
DEADLINE_SAFETY_S = 8


def predict_opener(pre_ticks):
    """5-rule ladder, grid-search ile 9/9 doğruluk."""
    last = pre_ticks[-1]
    first = pre_ticks[0]
    dscore = last["signal_score"] - first["signal_score"]
    score_avg = sum(t["signal_score"] for t in pre_ticks) / len(pre_ticks)
    bsi = last["bsi"]
    ofi_avg = sum(t["ofi"] for t in pre_ticks) / len(pre_ticks)
    cvd = last["cvd"]

    # Rule 1: BSI extreme reversion
    if abs(bsi) > BSI_REV_TH:
        return ("Down" if bsi > 0 else "Up", "bsi_rev")

    # Rule 2: OFI+CVD exhaustion (aşırı tek-yön flow → reversion)
    if abs(ofi_avg) > OFI_EXH_TH and abs(cvd) > CVD_EXH_TH:
        if ofi_avg > 0 and cvd > 0:
            return ("Down", "exhaustion")
        elif ofi_avg < 0 and cvd < 0:
            return ("Up", "exhaustion")

    # Rule 3: OFI directional (aggressive flow takibi)
    if abs(ofi_avg) > OFI_DIR_TH:
        return ("Up" if ofi_avg > 0 else "Down", "ofi_dir")

    # Rule 4: dscore strong momentum
    if abs(dscore) > DSCORE_STRONG:
        return ("Up" if dscore > 0 else "Down", "momentum")

    # Rule 5: Fallback — score_avg
    return ("Up" if score_avg >= SCORE_NEUTRAL else "Down", "score_avg")


def opp(o):
    return "Down" if o == "Up" else "Up"


def bid(tick, outcome):
    return tick["up_best_bid"] if outcome == "Up" else tick["down_best_bid"]


def ask(tick, outcome):
    return tick["up_best_ask"] if outcome == "Up" else tick["down_best_ask"]


class Sim:
    def __init__(self, ticks):
        self.ticks = ticks
        self.t0_ms = ticks[0]["ts_ms"]
        self.phase = "pending"
        self.intent = None
        self.opener_rule = None
        self.opener_intent = None  # ilk opener kararı (rapor için)
        self.opener_score = None  # signal_flip için sabit referans
        self.flip_count = 0  # toplam flip sayısı (max=SIGNAL_FLIP_MAX_COUNT)
        self.last_flip_t_s = -999
        self.flip_freeze_until_s = -999  # flip sonrası opp alımı dondurma süresi sonu
        self.score_persist_since_s = None
        self.last_pyr_t_s = None
        self.last_requote_dom_t_s = -999
        self.last_requote_hedge_t_s = -999
        self.last_parity_t_s = -999
        self.last_scoop_t_s = -999
        self.avg_down_used = False
        self.up_filled = 0.0
        self.down_filled = 0.0
        self.avg_up = 0.0
        self.avg_down = 0.0
        self.last_dom_price = None
        self.last_hedge_price = None
        self.force_intent = None
        self.trades = []  # list of {t, role, outcome, side, price, size, reason}

    def t_off_s(self, tick):
        return (tick["ts_ms"] - self.t0_ms) / 1000.0

    def remaining_s(self, tick):
        return 300.0 - self.t_off_s(tick)

    def buy(self, tick, outcome, price, size, role, reason):
        if size < 1:
            return
        size = min(size, MAX_SIZE)
        if outcome == "Up":
            new_total = self.up_filled + size
            self.avg_up = (self.avg_up * self.up_filled + price * size) / new_total
            self.up_filled = new_total
        else:
            new_total = self.down_filled + size
            self.avg_down = (self.avg_down * self.down_filled + price * size) / new_total
            self.down_filled = new_total
        self.trades.append({
            "t": self.t_off_s(tick),
            "role": role,
            "outcome": outcome,
            "side": "BUY",
            "price": price,
            "size": size,
            "reason": reason,
        })

    def avg(self, outcome):
        return self.avg_up if outcome == "Up" else self.avg_down

    def filled(self, outcome):
        return self.up_filled if outcome == "Up" else self.down_filled

    def run(self):
        pre_ticks = []
        for tick in self.ticks:
            t_s = self.t_off_s(tick)
            # Tick henüz aktif değilse (bid==0) skip
            if tick["up_best_bid"] == 0.0 and tick["down_best_bid"] == 0.0:
                pre_ticks.append(tick)  # signal_score yine de var
                continue

            # Pending: pre-opener pencere biriktir
            if self.phase == "pending":
                pre_ticks.append(tick)
                if len(pre_ticks) >= PRE_OPENER_TICKS:
                    intent, rule = predict_opener(pre_ticks)
                    if self.force_intent in ("Up", "Down"):
                        intent = self.force_intent
                        rule = f"forced_{rule}"
                    self.intent = intent
                    self.opener_intent = intent
                    self.opener_rule = rule
                    self.opener_score = tick["signal_score"]
                    self.phase = "managing"
                    self.score_persist_since_s = t_s
                    dom_p = bid(tick, intent)
                    hedge_p = bid(tick, opp(intent))
                    dom_size = OPEN_USDC_DOM / max(dom_p, 0.01)
                    hedge_size = OPEN_USDC_HEDGE / max(hedge_p, 0.01)
                    self.buy(tick, intent, dom_p, dom_size, "opener_dom",
                             f"composite={rule}")
                    self.buy(tick, opp(intent), hedge_p, hedge_size, "opener_hedge",
                             "pair-init")
                    self.last_dom_price = dom_p
                    self.last_hedge_price = hedge_p
                continue

            if self.phase == "done":
                continue

            score = tick["signal_score"]
            # signal_flip için referans = opener_score (sabit), tek tick'lik dscore değil
            dscore_from_open = score - (self.opener_score or score)

            # Lock: avg_sum <= 0.97 → kâr garantili, yeni risk alma (sadece scoop/deadline)
            # avg_sum yüksek (≥1) ise zaten zarar bölgesinde, bot agresif aksiyon almaya devam eder
            avg_sum = self.avg_up + self.avg_down
            both_filled = self.up_filled > 0 and self.down_filled > 0
            locked = both_filled and avg_sum <= LOCK_AVG_THRESHOLD

            # 1. Deadline safety
            if t_s >= 300 - DEADLINE_SAFETY_S:
                opp_b = bid(tick, opp(self.intent))
                if opp_b <= SCOOP_OPP_BID_MAX:
                    dom_p = ask(tick, self.intent) - TICK_SIZE
                    if dom_p > 0:
                        self.buy(tick, self.intent, dom_p, 5, "deadline_scoop",
                                 "deadline+scoop")
                self.phase = "done"
                continue

            # 2. Pre-resolve scoop (lock'a aldırmaz)
            opp_b = bid(tick, opp(self.intent))
            if (opp_b <= SCOOP_OPP_BID_MAX
                and self.remaining_s(tick) <= SCOOP_MIN_REMAINING_S
                and t_s - self.last_scoop_t_s >= SCOOP_COOLDOWN_S):
                dom_a = ask(tick, self.intent)
                price = max(dom_a - TICK_SIZE, 0.01)
                size = SCOOP_USDC / max(price, 0.01)
                self.buy(tick, self.intent, price, size, "scoop",
                         f"opp_bid={opp_b:.3f}")
                self.last_scoop_t_s = t_s
                continue

            # 3. Signal flip (lock'a aldırmaz — yön değişikliği lock'tan önemli)
            # Max SIGNAL_FLIP_MAX_COUNT flip / market + cooldown
            if (abs(dscore_from_open) > SIGNAL_FLIP_THRESHOLD
                and self.flip_count < SIGNAL_FLIP_MAX_COUNT
                and t_s - self.last_flip_t_s >= SIGNAL_FLIP_COOLDOWN_S):
                new_intent = "Up" if dscore_from_open > 0 else "Down"
                if new_intent != self.intent:
                    self.flip_count += 1
                    self.last_flip_t_s = t_s
                    self.flip_freeze_until_s = t_s + FLIP_FREEZE_OPP_S
                    self.intent = new_intent
                    self.opener_score = score
                    self.avg_down_used = False
                    self.score_persist_since_s = t_s
                    dom_p = bid(tick, new_intent)
                    hedge_p = bid(tick, opp(new_intent))
                    # Flip sonrası dom'a 2x size (kayıpları telafi etmek için boost)
                    self.buy(tick, new_intent, dom_p, (ORDER_USDC_DOM * 2.0) / max(dom_p, 0.01),
                             "signal_flip", f"dscore_from_open={dscore_from_open:+.2f}")
                    # Flip hedge çok küçük (zaten eski intent tarafına çok pozisyon var)
                    self.buy(tick, opp(new_intent), hedge_p,
                             (ORDER_USDC_HEDGE * 0.3) / max(hedge_p, 0.01),
                             "flip_hedge", "flip-pair")
                    self.last_dom_price = dom_p
                    self.last_hedge_price = hedge_p
                    continue

            if locked:
                continue

            # 4. Avg-down (one-shot)
            dom_b = bid(tick, self.intent)
            avg_dom = self.avg(self.intent)
            if (not self.avg_down_used and avg_dom > 0
                and dom_b + AVG_DOWN_MIN_EDGE <= avg_dom):
                self.avg_down_used = True
                size = ORDER_USDC_DOM / max(dom_b, 0.01)
                self.buy(tick, self.intent, dom_b, size, "avg_down",
                         f"avg={avg_dom:.3f} px={dom_b:.3f}")
                self.last_dom_price = dom_b
                continue

            # 5. Pyramid
            if (tick["ofi"] >= PYRAMID_OFI_MIN
                and (t_s - (self.score_persist_since_s or 0)) >= PYRAMID_SCORE_PERSIST_S
                and (self.last_pyr_t_s is None or (t_s - self.last_pyr_t_s) >= PYRAMID_COOLDOWN_S)
                and abs(dscore_from_open) < 1.0):
                score_dir = "Up" if score >= SCORE_NEUTRAL else "Down"
                if score_dir == self.intent:
                    size = PYRAMID_USDC / max(dom_b, 0.01)
                    self.buy(tick, self.intent, dom_b, size, "pyramid",
                             f"ofi={tick['ofi']:.2f} score={score:.2f}")
                    self.last_pyr_t_s = t_s
                    self.last_dom_price = dom_b
                    continue

            # 6. Price drift requote (cooldown'lu)
            if (self.last_dom_price is not None
                and abs(dom_b - self.last_dom_price) >= REQUOTE_PRICE_EPS
                and t_s - self.last_requote_dom_t_s >= REQUOTE_COOLDOWN_S):
                size = ORDER_USDC_DOM / max(dom_b, 0.01)
                self.buy(tick, self.intent, dom_b, size, "requote_dom",
                         f"drift={dom_b-self.last_dom_price:+.3f}")
                self.last_dom_price = dom_b
                self.last_requote_dom_t_s = t_s

            opp_b = bid(tick, opp(self.intent))
            # Hedge requote SADECE opp YÜKSELDİĞİNDE (winner zayıflıyor sinyali)
            # Opp düşüyorsa = winner netleşiyor = hedge gereksiz, alım yapma
            hedge_drift = opp_b - (self.last_hedge_price or 0)
            if (self.last_hedge_price is not None
                and hedge_drift >= REQUOTE_PRICE_EPS  # sadece artış
                and t_s - self.last_requote_hedge_t_s >= REQUOTE_COOLDOWN_S
                and opp_b >= PARITY_OPP_BID_MIN
                and t_s >= self.flip_freeze_until_s):
                size = ORDER_USDC_HEDGE / max(opp_b, 0.01)
                self.buy(tick, opp(self.intent), opp_b, size, "requote_hedge",
                         f"hedge_drift={hedge_drift:+.3f}")
                self.last_hedge_price = opp_b
                self.last_requote_hedge_t_s = t_s

            # 7. Parity gap (gevşek + cooldown + opp_bid floor)
            gap = abs(self.up_filled - self.down_filled)
            opp_b_for_parity = bid(tick, opp(self.intent))
            if (gap > PARITY_MIN_GAP_QTY
                and t_s - self.last_parity_t_s >= PARITY_COOLDOWN_S
                and opp_b_for_parity >= PARITY_OPP_BID_MIN
                and t_s >= self.flip_freeze_until_s):
                size = min(gap, 80)
                self.buy(tick, opp(self.intent), opp_b_for_parity, size, "parity_topup",
                         f"gap={gap:.0f}")
                self.last_parity_t_s = t_s


def report(sim, ticks):
    last_tick = ticks[-1]
    up_bid_final = last_tick["up_best_bid"]
    down_bid_final = last_tick["down_best_bid"]
    if up_bid_final >= 0.95:
        winner = "Up"
    elif down_bid_final >= 0.95:
        winner = "Down"
    else:
        winner = "?"
    cost_up = sim.avg_up * sim.up_filled
    cost_down = sim.avg_down * sim.down_filled
    total_cost = cost_up + cost_down

    print(f"\n=== Backtest sonucu ===")
    print(f"Phase: {sim.phase}")
    print(f"Opener intent: {sim.opener_intent} ({sim.opener_rule})")
    if sim.opener_intent != sim.intent:
        print(f"Final intent (signal_flip): {sim.intent}")
    print(f"\nTrade sayısı: {len(sim.trades)}")
    by_role = {}
    for tr in sim.trades:
        by_role[tr["role"]] = by_role.get(tr["role"], 0) + 1
    print("Role dağılımı:")
    for r, n in sorted(by_role.items(), key=lambda x: -x[1]):
        print(f"  {r:18s} {n}")

    print(f"\nUP filled:   {sim.up_filled:8.2f} share, avg={sim.avg_up:.4f}, cost=${cost_up:.2f}")
    print(f"DOWN filled: {sim.down_filled:8.2f} share, avg={sim.avg_down:.4f}, cost=${cost_down:.2f}")
    print(f"Total cost:  ${total_cost:.2f}")

    if winner != "?":
        payout = sim.up_filled if winner == "Up" else sim.down_filled
        pnl = payout - total_cost
        print(f"\nMarket winner: {winner} (final: up={up_bid_final:.2f} down={down_bid_final:.2f})")
        print(f"Payout (winner shares × $1): ${payout:.2f}")
        print(f"PnL: ${pnl:+.2f}")
        if pnl > 0:
            print(f"\n>>> KAZANÇ: +${pnl:.2f}")
        else:
            print(f"\n>>> KAYIP:  ${pnl:.2f}")
    else:
        # Resolve henüz olmadı — 3 senaryo
        pnl_if_up = sim.up_filled - total_cost
        pnl_if_down = sim.down_filled - total_cost
        # Mid-market satış: pozisyonu son bid fiyatından kapat
        sale_value = sim.up_filled * up_bid_final + sim.down_filled * down_bid_final
        pnl_mid = sale_value - total_cost
        print(f"\nMarket henüz çözümlenmemiş! (final: up_bid={up_bid_final:.2f} down_bid={down_bid_final:.2f})")
        print(f"\nSenaryo PnL'leri:")
        print(f"  UP kazanırsa:    ${pnl_if_up:+.2f}  (UP {sim.up_filled:.0f} share × $1 - cost)")
        print(f"  DOWN kazanırsa:  ${pnl_if_down:+.2f}  (DOWN {sim.down_filled:.0f} share × $1 - cost)")
        print(f"  Mid-market sat:  ${pnl_mid:+.2f}  (her iki taraf son bid'ten satılırsa)")

    print("\n=== Trade dökümü ===")
    print(f"{'t_off':>6} | {'role':18s} | {'outcome':4} | {'price':>6} | {'size':>8} | reason")
    for tr in sim.trades:
        print(f"{tr['t']:>6.0f} | {tr['role']:18s} | {tr['outcome']:4} | "
              f"{tr['price']:>6.3f} | {tr['size']:>8.2f} | {tr['reason']}")


def main():
    if len(sys.argv) < 2:
        print("Usage: backtest_market.py <market_slug> [force_intent]")
        return 1
    slug = sys.argv[1]
    force_intent = sys.argv[2] if len(sys.argv) >= 3 else None
    p = TICKS_DIR / f"{slug}_ticks.json"
    if not p.exists():
        print(f"Tick dosyası yok: {p}")
        return 1
    ticks = json.load(p.open())
    print(f"Market: {slug}")
    print(f"Tick count: {len(ticks)}")
    print(f"Duration: {(ticks[-1]['ts_ms'] - ticks[0]['ts_ms'])/1000:.1f}s")
    print(f"Final: up_bid={ticks[-1]['up_best_bid']:.2f} down_bid={ticks[-1]['down_best_bid']:.2f}")
    if force_intent:
        print(f"FORCE INTENT: {force_intent}")

    sim = Sim(ticks)
    sim.force_intent = force_intent
    sim.run()
    report(sim, ticks)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
