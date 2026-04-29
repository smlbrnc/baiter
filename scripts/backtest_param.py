#!/usr/bin/env python3
"""Parametrik Elis backtest — `backtest_market.Sim`'in tüm parametreleri
`ElisParams` dataclass üzerinden geçer; grid search için kullanılır.

Yeni guard'lar:
  * HARD_STOP_AVG_SUM    — avg_up + avg_down ≥ eşik → tüm aksiyon dur
  * MAX_REQUOTE_PER_MARKET — market başına requote_dom + requote_hedge cap
  * MIN_OPP_DECAY_FOR_HEDGE — hedge requote için opp'un yükseliş süresi
"""
from __future__ import annotations

import json
from dataclasses import dataclass, asdict
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
EXPORTS = ROOT / "exports"


@dataclass
class ElisParams:
    """Elis params — v4b default (24-market combined optimize).

    Yön: 17/20 = %85, Kesin PnL: +$862.22 (bot14+15 combined).
    """
    # === COMPOSITE OPENER (v4b default'lar) ===
    pre_opener_ticks: int = 20
    bsi_rev: float = 1.5            # v4b: 2.0→1.5
    ofi_exh: float = 0.4
    cvd_exh: float = 3.0
    ofi_dir: float = 0.3            # v4b: 0.4→0.3
    dscore_strong: float = 1.5      # v4b: 1.0→1.5
    score_neutral: float = 5.0
    # === SIGNAL FLIP ===
    flip_threshold: float = 5.0
    flip_max_count: int = 1
    flip_freeze_opp_s: float = 60.0
    # === SIZING ===
    open_usdc_dom: float = 25.0
    open_usdc_hedge: float = 12.0
    order_usdc_dom: float = 15.0
    order_usdc_hedge: float = 8.0
    pyramid_usdc: float = 30.0
    scoop_usdc: float = 50.0
    max_size: float = 50.0
    # === REQUOTE (v4b: en kritik fix) ===
    tick_size: float = 0.01
    requote_eps_ticks: float = 4.0  # v4b: 2→4 (spam %50+ azaldı)
    requote_cooldown_s: float = 3.0
    # === AVG-DOWN ===
    avg_down_min_edge_ticks: float = 2.3
    # === PYRAMID ===
    pyramid_ofi_min: float = 0.83
    pyramid_score_persist_s: float = 5.0
    pyramid_cooldown_s: float = 3.0
    # === PARITY ===
    parity_min_gap_qty: float = 250.0
    parity_cooldown_s: float = 5.0
    parity_opp_bid_min: float = 0.15
    # === LOCK / DEADLINE / SCOOP ===
    lock_avg_threshold: float = 0.97
    scoop_opp_bid_max: float = 0.05
    scoop_min_remaining_s: float = 35.0
    scoop_cooldown_s: float = 2.0
    deadline_safety_s: float = 8.0
    # === YENİ GUARD'LAR (v4) ===
    hard_stop_avg_sum: float = 999.0  # default kapalı; 1.05 → açık
    max_requote_per_market: int = 999  # default kapalı; 30 → açık


def predict_opener(pre, p: ElisParams):
    last = pre[-1]
    first = pre[0]
    dscore = last["signal_score"] - first["signal_score"]
    score_avg = sum(t["signal_score"] for t in pre) / len(pre)
    bsi = last["bsi"]
    ofi_avg = sum(t["ofi"] for t in pre) / len(pre)
    cvd = last["cvd"]
    if abs(bsi) > p.bsi_rev:
        return ("Down" if bsi > 0 else "Up", "bsi_rev")
    if abs(ofi_avg) > p.ofi_exh and abs(cvd) > p.cvd_exh:
        if ofi_avg > 0 and cvd > 0:
            return ("Down", "exhaustion")
        if ofi_avg < 0 and cvd < 0:
            return ("Up", "exhaustion")
    if abs(ofi_avg) > p.ofi_dir:
        return ("Up" if ofi_avg > 0 else "Down", "ofi_dir")
    if abs(dscore) > p.dscore_strong:
        return ("Up" if dscore > 0 else "Down", "momentum")
    return ("Up" if score_avg >= p.score_neutral else "Down", "score_avg")


def opp(o):
    return "Down" if o == "Up" else "Up"


def bid(t, o):
    return t["up_best_bid"] if o == "Up" else t["down_best_bid"]


def ask(t, o):
    return t["up_best_ask"] if o == "Up" else t["down_best_ask"]


class Sim:
    def __init__(self, ticks, p: ElisParams):
        self.t = ticks
        self.p = p
        self.t0 = ticks[0]["ts_ms"]
        self.phase = "pending"
        self.intent = None
        self.opener_intent = None
        self.opener_rule = None
        self.opener_score = None
        self.flip_count = 0
        self.flip_freeze_until_s = -999
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
        self.requote_count = 0  # v4: cap
        self.hard_stopped = False  # v4: latch
        self.trades = []

    def t_off_s(self, tick):
        return (tick["ts_ms"] - self.t0) / 1000.0

    def remaining_s(self, tick):
        return 300.0 - self.t_off_s(tick)

    def buy(self, tick, outcome, price, size, role, reason):
        if size < 1:
            return
        size = min(size, self.p.max_size)
        if outcome == "Up":
            new = self.up_filled + size
            self.avg_up = (self.avg_up * self.up_filled + price * size) / new
            self.up_filled = new
        else:
            new = self.down_filled + size
            self.avg_down = (self.avg_down * self.down_filled + price * size) / new
            self.down_filled = new
        self.trades.append({
            "t": self.t_off_s(tick), "role": role, "outcome": outcome,
            "price": price, "size": size, "reason": reason,
        })

    def avg_of(self, o):
        return self.avg_up if o == "Up" else self.avg_down

    def run(self):
        p = self.p
        pre = []
        eps = p.tick_size * p.requote_eps_ticks
        avg_down_edge = p.tick_size * p.avg_down_min_edge_ticks
        for tick in self.t:
            t_s = self.t_off_s(tick)
            if tick["up_best_bid"] == 0.0 and tick["down_best_bid"] == 0.0:
                pre.append(tick)
                continue
            if self.phase == "pending":
                pre.append(tick)
                if len(pre) >= p.pre_opener_ticks:
                    intent, rule = predict_opener(pre[-p.pre_opener_ticks:], p)
                    self.intent = intent
                    self.opener_intent = intent
                    self.opener_rule = rule
                    self.opener_score = tick["signal_score"]
                    self.phase = "managing"
                    self.score_persist_since_s = t_s
                    dom_p = bid(tick, intent)
                    hedge_p = bid(tick, opp(intent))
                    self.buy(tick, intent, dom_p, p.open_usdc_dom / max(dom_p, 0.01),
                             "opener_dom", f"composite={rule}")
                    self.buy(tick, opp(intent), hedge_p, p.open_usdc_hedge / max(hedge_p, 0.01),
                             "opener_hedge", "pair-init")
                    self.last_dom_price = dom_p
                    self.last_hedge_price = hedge_p
                continue
            if self.phase == "done":
                continue

            score = tick["signal_score"]
            dscore_from_open = score - (self.opener_score or score)
            avg_sum = self.avg_up + self.avg_down
            both_filled = self.up_filled > 0 and self.down_filled > 0

            # 0. HARD STOP — v4 yeni guard
            if avg_sum >= p.hard_stop_avg_sum:
                self.hard_stopped = True
                self.phase = "done"
                continue

            # 1. Deadline safety
            if t_s >= 300 - p.deadline_safety_s:
                opp_b = bid(tick, opp(self.intent))
                if opp_b <= p.scoop_opp_bid_max:
                    dom_p2 = ask(tick, self.intent) - p.tick_size
                    if dom_p2 > 0:
                        self.buy(tick, self.intent, dom_p2, 5,
                                 "deadline_scoop", "deadline+scoop")
                self.phase = "done"
                continue

            # 2. Pre-resolve scoop
            opp_b = bid(tick, opp(self.intent))
            if (opp_b <= p.scoop_opp_bid_max
                and self.remaining_s(tick) <= p.scoop_min_remaining_s
                and t_s - self.last_scoop_t_s >= p.scoop_cooldown_s):
                dom_a = ask(tick, self.intent)
                price = max(dom_a - p.tick_size, 0.01)
                self.buy(tick, self.intent, price,
                         p.scoop_usdc / max(price, 0.01),
                         "scoop", f"opp_bid={opp_b:.3f}")
                self.last_scoop_t_s = t_s
                continue

            # 3. Signal flip
            if (abs(dscore_from_open) > p.flip_threshold
                and self.flip_count < p.flip_max_count):
                new_intent = "Up" if dscore_from_open > 0 else "Down"
                if new_intent != self.intent:
                    self.flip_count += 1
                    self.flip_freeze_until_s = t_s + p.flip_freeze_opp_s
                    self.intent = new_intent
                    self.opener_score = score
                    self.avg_down_used = False
                    self.score_persist_since_s = t_s
                    dom_p2 = bid(tick, new_intent)
                    hedge_p2 = bid(tick, opp(new_intent))
                    self.buy(tick, new_intent, dom_p2,
                             (p.order_usdc_dom * 2.0) / max(dom_p2, 0.01),
                             "signal_flip", f"dscore={dscore_from_open:+.2f}")
                    self.buy(tick, opp(new_intent), hedge_p2,
                             (p.order_usdc_hedge * 0.3) / max(hedge_p2, 0.01),
                             "flip_hedge", "flip-pair")
                    self.last_dom_price = dom_p2
                    self.last_hedge_price = hedge_p2
                    continue

            # 4. Lock
            locked = both_filled and avg_sum <= p.lock_avg_threshold
            if locked:
                continue

            # 5. Avg-down
            dom_b = bid(tick, self.intent)
            adom = self.avg_of(self.intent)
            if (not self.avg_down_used and adom > 0
                and dom_b + avg_down_edge <= adom):
                self.avg_down_used = True
                self.buy(tick, self.intent, dom_b,
                         p.order_usdc_dom / max(dom_b, 0.01),
                         "avg_down", f"avg={adom:.3f}")
                self.last_dom_price = dom_b
                continue

            # 6. Pyramid
            if (tick["ofi"] >= p.pyramid_ofi_min
                and (t_s - (self.score_persist_since_s or 0)) >= p.pyramid_score_persist_s
                and (self.last_pyr_t_s is None
                     or (t_s - self.last_pyr_t_s) >= p.pyramid_cooldown_s)
                and abs(dscore_from_open) < 1.0):
                score_dir = "Up" if score >= p.score_neutral else "Down"
                if score_dir == self.intent:
                    self.buy(tick, self.intent, dom_b,
                             p.pyramid_usdc / max(dom_b, 0.01),
                             "pyramid", f"ofi={tick['ofi']:.2f}")
                    self.last_pyr_t_s = t_s
                    self.last_dom_price = dom_b
                    continue

            # 7. Requote dom — v4 cap
            if self.requote_count >= p.max_requote_per_market:
                continue
            if (self.last_dom_price is not None
                and abs(dom_b - self.last_dom_price) >= eps
                and t_s - self.last_requote_dom_t_s >= p.requote_cooldown_s):
                self.buy(tick, self.intent, dom_b,
                         p.order_usdc_dom / max(dom_b, 0.01),
                         "requote_dom", f"drift={dom_b-self.last_dom_price:+.3f}")
                self.last_dom_price = dom_b
                self.last_requote_dom_t_s = t_s
                self.requote_count += 1

            # 8. Hedge requote (sadece artış)
            opp_b2 = bid(tick, opp(self.intent))
            hedge_drift = opp_b2 - (self.last_hedge_price or 0)
            if (self.last_hedge_price is not None
                and hedge_drift >= eps
                and t_s - self.last_requote_hedge_t_s >= p.requote_cooldown_s
                and opp_b2 >= p.parity_opp_bid_min
                and t_s >= self.flip_freeze_until_s
                and self.requote_count < p.max_requote_per_market):
                self.buy(tick, opp(self.intent), opp_b2,
                         p.order_usdc_hedge / max(opp_b2, 0.01),
                         "requote_hedge", f"drift={hedge_drift:+.3f}")
                self.last_hedge_price = opp_b2
                self.last_requote_hedge_t_s = t_s
                self.requote_count += 1

            # 9. Parity
            gap = abs(self.up_filled - self.down_filled)
            opp_b3 = bid(tick, opp(self.intent))
            if (gap > p.parity_min_gap_qty
                and t_s - self.last_parity_t_s >= p.parity_cooldown_s
                and opp_b3 >= p.parity_opp_bid_min
                and t_s >= self.flip_freeze_until_s):
                size = min(gap, 80)
                self.buy(tick, opp(self.intent), opp_b3, size,
                         "parity_topup", f"gap={gap:.0f}")
                self.last_parity_t_s = t_s


def find_ticks_path(slug: str):
    for d in sorted(EXPORTS.glob("bot*-ticks-*")):
        p = d / f"{slug}_ticks.json"
        if p.exists():
            return p
    return None


def all_slugs():
    out = []
    for d in sorted(EXPORTS.glob("bot*-ticks-*")):
        for p in sorted(d.glob("btc-updown-5m-*_ticks.json")):
            out.append(p.stem.replace("_ticks", ""))
    return out


def run_market(slug: str, p: ElisParams):
    path = find_ticks_path(slug)
    if path is None:
        return None
    ticks = json.load(path.open())
    sim = Sim(ticks, p)
    sim.run()
    last = ticks[-1]
    up_b = last["up_best_bid"]
    dn_b = last["down_best_bid"]
    if up_b >= 0.95:
        winner = "Up"
    elif dn_b >= 0.95:
        winner = "Down"
    else:
        winner = "?"
    cost = sim.avg_up * sim.up_filled + sim.avg_down * sim.down_filled
    if winner == "Up":
        pnl = sim.up_filled - cost
    elif winner == "Down":
        pnl = sim.down_filled - cost
    else:
        pnl = sim.up_filled * up_b + sim.down_filled * dn_b - cost
    return {
        "slug": slug, "winner": winner, "intent": sim.intent,
        "opener": sim.opener_intent, "rule": sim.opener_rule,
        "trades": len(sim.trades), "pnl": pnl,
        "hard_stopped": sim.hard_stopped,
        "yon_ok": (winner != "?" and winner == sim.intent),
    }


def run_all(p: ElisParams):
    rows = [run_market(s, p) for s in all_slugs()]
    rows = [r for r in rows if r]
    resolved = [r for r in rows if r["winner"] != "?"]
    correct = sum(1 for r in resolved if r["yon_ok"])
    pnl_resolved = sum(r["pnl"] for r in resolved)
    pnl_mid = sum(r["pnl"] for r in rows if r["winner"] == "?")
    return {
        "params": asdict(p),
        "rows": rows,
        "n_resolved": len(resolved),
        "correct": correct,
        "yon_pct": correct / len(resolved) * 100 if resolved else 0,
        "pnl_resolved": pnl_resolved,
        "pnl_mid": pnl_mid,
        "pnl_net": pnl_resolved + pnl_mid,
    }


if __name__ == "__main__":
    p = ElisParams()
    r = run_all(p)
    print(f"Yön: {r['correct']}/{r['n_resolved']} = %{r['yon_pct']:.0f}")
    print(f"Kesin PnL: ${r['pnl_resolved']:+.2f}")
    print(f"Mid: ${r['pnl_mid']:+.2f}")
    print(f"NET: ${r['pnl_net']:+.2f}")
