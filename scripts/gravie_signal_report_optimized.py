#!/usr/bin/env python3
"""
Gravie — signal-report.md dokümanına göre optimize edilmiş backtest.

Değişiklikler:
1. Signal threshold: 5.5/4.5 → 6.0/4.0 (daha kararlı sinyaller)
2. avg_sum guard: 0.80 → 0.85 (daha sıkı arbitraj)
3. Stability filter: std < 0.3 (daha az gürültülü marketler)
4. Late-window pasif: 90s → 60s (signal-report T-10s sweet spot)
"""
from __future__ import annotations

import json
import math
import re
import sqlite3
from collections import defaultdict
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any, Literal

ROOT = Path(__file__).resolve().parents[1]
DB = ROOT / "data" / "baiter.db"
OUT_JSON = ROOT / "data" / "gravie_optimized_backtest.json"

SideOut = Literal["Up", "Down"]


def parse_market_slug(slug: str) -> tuple[str | None, str | None, int | None]:
    m = re.match(r"(btc|eth|sol|xrp)-updown-(5m|15m|1h|4h)-(\d+)", slug or "")
    if not m:
        return None, None, None
    return m.group(1), m.group(2), int(m.group(3))


@dataclass
class GravieParams:
    tick_interval_secs: int = 5
    buy_cooldown_ms: int = 4000
    hedge_cooldown_ms: int = 4000
    winner_order_usdc: float = 15.0
    hedge_order_usdc: float = 5.0
    winner_max_price: float = 0.65
    hedge_max_price: float = 0.65
    avg_sum_max: float = 0.85        # OPTIMIZED: 0.80 → 0.85
    signal_up_threshold: float = 6.0  # OPTIMIZED: 5.5 → 6.0
    signal_down_threshold: float = 4.0 # OPTIMIZED: 4.5 → 4.0
    stability_window: int = 3
    stability_max_std: float = 0.3    # OPTIMIZED: 0.5 → 0.3
    ema_alpha: float = 0.3
    t_cutoff_secs: float = 30.0
    late_winner_pasif_secs: float = 60.0  # OPTIMIZED: 90 → 60
    max_fak_size: float = 50.0
    max_size_per_side: float = 0.0


@dataclass
class Metrics:
    up_filled: float = 0.0
    down_filled: float = 0.0
    avg_up: float = 0.0
    avg_down: float = 0.0
    cost: float = 0.0
    n_winner_buys: int = 0
    n_hedge_buys: int = 0

    def ingest_buy(self, outcome: SideOut, price: float, size: float, is_winner: bool) -> None:
        self.cost += price * size
        if is_winner:
            self.n_winner_buys += 1
        else:
            self.n_hedge_buys += 1
        if outcome == "Up":
            nu = self.up_filled + size
            self.avg_up = (self.avg_up * self.up_filled + price * size) / nu if nu > 0 else 0.0
            self.up_filled = nu
        else:
            nd = self.down_filled + size
            self.avg_down = (self.avg_down * self.down_filled + price * size) / nd if nd > 0 else 0.0
            self.down_filled = nd


@dataclass
class GravieActive:
    last_acted_secs: int = -999999
    last_winner_buy_ms: int = 0
    last_hedge_buy_ms: int = 0
    ema_state: float | None = None
    signal_history: list[float] = field(default_factory=list)


def try_buy(
    m: Metrics,
    side: SideOut,
    ask: float,
    order_usdc: float,
    p: GravieParams,
    api_min: float,
) -> tuple[float, float] | None:
    if ask <= 0.0 or ask > 1.0:
        return None
    raw_size = math.ceil(order_usdc / ask)
    size = min(raw_size, p.max_fak_size) if p.max_fak_size > 0 else raw_size

    own_filled = m.up_filled if side == "Up" else m.down_filled
    own_spent = m.avg_up * m.up_filled if side == "Up" else m.avg_down * m.down_filled
    opp_filled = m.down_filled if side == "Up" else m.up_filled
    opp_spent = m.avg_down * m.down_filled if side == "Up" else m.avg_up * m.up_filled

    if p.max_size_per_side > 0:
        size = min(size, max(0.0, p.max_size_per_side - own_filled))
    if size <= 0 or size * ask < api_min:
        return None

    if opp_filled > 0:
        new_own_avg = (own_spent + size * ask) / (own_filled + size) if own_filled + size > 0 else ask
        opp_avg = opp_spent / opp_filled
        if new_own_avg + opp_avg >= p.avg_sum_max:
            return None

    return (ask, size)


def decide_gravie(
    st: GravieActive,
    m: Metrics,
    p: GravieParams,
    *,
    now_ms: int,
    start_ts: int,
    up_bid: float,
    up_ask: float,
    down_bid: float,
    down_ask: float,
    signal_score: float,
    to_end: float,
    api_min: float,
) -> tuple[GravieActive, Metrics, list[tuple[SideOut, float, float, str]]]:
    trades: list[tuple[SideOut, float, float, str]] = []

    if to_end <= p.t_cutoff_secs:
        return st, m, trades

    rel_secs = (now_ms // 1000) - start_ts

    if rel_secs % p.tick_interval_secs != 0:
        return st, m, trades
    if rel_secs == st.last_acted_secs:
        return st, m, trades
    st.last_acted_secs = rel_secs

    if up_ask <= 0 or down_ask <= 0:
        return st, m, trades
    if signal_score <= 0:
        return st, m, trades

    centered = signal_score - 5.0
    smoothed_centered = centered if st.ema_state is None else (
        p.ema_alpha * centered + (1.0 - p.ema_alpha) * st.ema_state
    )
    st.ema_state = smoothed_centered
    smoothed = smoothed_centered + 5.0

    if p.stability_window > 0:
        if len(st.signal_history) >= p.stability_window:
            st.signal_history.pop(0)
        st.signal_history.append(smoothed)
        if len(st.signal_history) < p.stability_window:
            return st, m, trades
        n = len(st.signal_history)
        mean = sum(st.signal_history) / n
        var = sum((x - mean) ** 2 for x in st.signal_history) / n
        if math.sqrt(var) > p.stability_max_std:
            return st, m, trades

    if smoothed > p.signal_up_threshold:
        winner: SideOut = "Up"
    elif smoothed < p.signal_down_threshold:
        winner = "Down"
    else:
        return st, m, trades

    hedge: SideOut = "Down" if winner == "Up" else "Up"

    winner_allowed = p.late_winner_pasif_secs <= 0 or to_end > p.late_winner_pasif_secs

    winner_ask = up_ask if winner == "Up" else down_ask
    if winner_allowed and winner_ask > 0 and winner_ask <= p.winner_max_price:
        if now_ms - st.last_winner_buy_ms >= p.buy_cooldown_ms:
            res = try_buy(m, winner, winner_ask, p.winner_order_usdc, p, api_min)
            if res:
                px, sz = res
                m.ingest_buy(winner, px, sz, is_winner=True)
                st.last_winner_buy_ms = now_ms
                trades.append((winner, px, sz, f"gravie:winner:{winner.lower()}"))

    winner_filled = m.up_filled if winner == "Up" else m.down_filled
    hedge_ask = up_ask if hedge == "Up" else down_ask
    if hedge_ask > 0 and hedge_ask <= p.hedge_max_price and winner_filled > 0:
        if now_ms - st.last_hedge_buy_ms >= p.hedge_cooldown_ms:
            res = try_buy(m, hedge, hedge_ask, p.hedge_order_usdc, p, api_min)
            if res:
                px, sz = res
                m.ingest_buy(hedge, px, sz, is_winner=False)
                st.last_hedge_buy_ms = now_ms
                trades.append((hedge, px, sz, f"gravie:hedge:{hedge.lower()}"))

    return st, m, trades


def run_session_sim(
    ticks: list[tuple[int, float, float, float, float, float]],
    start_ts: int,
    end_ts: int,
    api_min: float,
    p: GravieParams | None = None,
) -> tuple[Metrics, int, GravieActive, list[dict]]:
    p = p or GravieParams()
    if not ticks:
        return Metrics(), 0, GravieActive(), []

    m = Metrics()
    st = GravieActive()
    all_trades: list[dict] = []

    for ts_ms, ub, ua, db, da, sig in ticks:
        to_end = float(end_ts) - ts_ms / 1000.0
        st, m, trades = decide_gravie(
            st, m, p,
            now_ms=ts_ms,
            start_ts=start_ts,
            up_bid=ub, up_ask=ua, down_bid=db, down_ask=da,
            signal_score=sig,
            to_end=to_end,
            api_min=api_min,
        )
        for outcome, px, sz, reason in trades:
            all_trades.append({
                "ts_ms": ts_ms, "outcome": outcome, "price": round(px, 4),
                "size": round(sz, 2), "reason": reason,
            })

    return m, len(all_trades), st, all_trades


def proxy_winner_from_row(r: tuple[Any, ...]) -> SideOut:
    _ts, ub, ua, db, da, *_rest = r
    um = (float(ub) + float(ua)) / 2.0
    dm = (float(db) + float(da)) / 2.0
    return "Up" if um >= dm else "Down"


def run_comparison(params_list: list[tuple[str, GravieParams]]) -> dict:
    conn = sqlite3.connect(DB)
    conn.row_factory = sqlite3.Row
    cur = conn.cursor()
    cur.execute(
        """
        SELECT ms.id, ms.bot_id, ms.slug, ms.start_ts, ms.end_ts,
               COALESCE(ms.min_order_size, 1.0) AS min_order_size
        FROM market_sessions ms
        JOIN bots b ON b.id = ms.bot_id
        WHERE b.strategy = 'gravie'
        ORDER BY ms.id
        """
    )
    sessions = [dict(r) for r in cur.fetchall()]

    results_by_profile: dict[str, dict] = {}

    for profile_name, params in params_list:
        sim_results = []
        signal_correct = 0
        signal_total = 0

        for s in sessions:
            sid = s["id"]
            slug = s["slug"]
            asset, wl_str, _ = parse_market_slug(slug)
            if not asset or not wl_str:
                continue

            cur.execute(
                """
                SELECT ts_ms, up_best_bid, up_best_ask, down_best_bid, down_best_ask, signal_score
                FROM market_ticks WHERE market_session_id = ?
                ORDER BY ts_ms
                """,
                (sid,),
            )
            rows = cur.fetchall()
            ticks = [
                (r[0], float(r[1]), float(r[2]), float(r[3]), float(r[4]), float(r[5] or 5.0))
                for r in rows
            ]

            api_min = max(1.0, float(s["min_order_size"]))
            m, n_tr, _st, trades = run_session_sim(ticks, s["start_ts"], s["end_ts"], api_min, params)

            last_row = rows[-1] if rows else None
            proxy_win = proxy_winner_from_row(last_row) if last_row else None
            winner = proxy_win

            pnl = None
            if winner:
                payout = m.up_filled if winner == "Up" else m.down_filled
                pnl = payout - m.cost

            if winner and n_tr > 0:
                winner_trades = [t for t in trades if "winner" in t["reason"]]
                if winner_trades:
                    first_winner = winner_trades[0]
                    pred_dir = "Up" if "up" in first_winner["reason"] else "Down"
                    signal_total += 1
                    if pred_dir == winner:
                        signal_correct += 1

            avg_sum = m.avg_up + m.avg_down if m.up_filled > 0 and m.down_filled > 0 else None
            dual = m.up_filled > 0 and m.down_filled > 0

            sim_results.append({
                "slug": slug, "cost": round(m.cost, 2), "pnl": round(pnl, 2) if pnl else None,
                "dual": dual, "avg_sum": round(avg_sum, 4) if avg_sum else None,
            })

        total_cost = sum(r["cost"] for r in sim_results)
        total_pnl = sum(r["pnl"] for r in sim_results if r["pnl"] is not None)
        wins = sum(1 for r in sim_results if r["pnl"] and r["pnl"] > 0)
        losses = sum(1 for r in sim_results if r["pnl"] is not None and r["pnl"] <= 0)
        dual_cnt = sum(1 for r in sim_results if r["dual"])
        arb_cnt = sum(1 for r in sim_results if r["avg_sum"] and r["avg_sum"] < 1.0)

        results_by_profile[profile_name] = {
            "params": params.__dict__,
            "sessions": len(sim_results),
            "cost": round(total_cost, 2),
            "pnl": round(total_pnl, 2),
            "roi_pct": round(total_pnl / total_cost * 100, 2) if total_cost > 0 else 0,
            "wins": wins,
            "losses": losses,
            "winrate_pct": round(wins / (wins + losses) * 100, 2) if wins + losses > 0 else 0,
            "signal_accuracy_pct": round(signal_correct / signal_total * 100, 2) if signal_total > 0 else 0,
            "dual_sessions": dual_cnt,
            "arb_sessions": arb_cnt,
        }

    return results_by_profile


def main() -> None:
    profiles = [
        ("baseline", GravieParams(
            signal_up_threshold=5.5, signal_down_threshold=4.5,
            avg_sum_max=0.80, stability_max_std=0.5, late_winner_pasif_secs=90.0
        )),
        ("optimized_v1", GravieParams(
            signal_up_threshold=6.0, signal_down_threshold=4.0,
            avg_sum_max=0.85, stability_max_std=0.3, late_winner_pasif_secs=60.0
        )),
        ("tight_signal", GravieParams(
            signal_up_threshold=6.5, signal_down_threshold=3.5,
            avg_sum_max=0.80, stability_max_std=0.2, late_winner_pasif_secs=90.0
        )),
        ("loose_arb", GravieParams(
            signal_up_threshold=5.5, signal_down_threshold=4.5,
            avg_sum_max=0.95, stability_max_std=0.5, late_winner_pasif_secs=90.0
        )),
        ("aggressive", GravieParams(
            signal_up_threshold=5.2, signal_down_threshold=4.8,
            avg_sum_max=0.90, stability_max_std=0.6, late_winner_pasif_secs=120.0
        )),
    ]

    results = run_comparison(profiles)

    out = {
        "comparison": results,
        "best_profile": max(results.items(), key=lambda x: x[1]["roi_pct"])[0],
        "signal_report_reference": {
            "section_4.2": "BTC 5m threshold: 0.30%/60s → 3-8 sinyal/gün, %62-69 win rate",
            "section_5.1": "Window delta 5-7x ağırlık, confidence divisor = 7",
            "section_7.1": "avg_sum < 1.0 = complete-set arbitrage",
            "section_12.1": "T-10s sweet spot, T-5s çok geç",
        },
    }

    with open(OUT_JSON, "w") as f:
        json.dump(out, f, indent=2)

    print(json.dumps(results, indent=2))


if __name__ == "__main__":
    main()
