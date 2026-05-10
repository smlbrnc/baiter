#!/usr/bin/env python3
"""
Gravie V3 (ASYM) — Sinyal-yönlü asimetrik dual-side backtest.

Signal-report.md dokümanı ile uyumlu sinyal kalitesi testi:
- EMA smoothed signal_score
- Stability filter (son N tick std < threshold)
- avg_sum guard (matematiksel arbitraj garantisi)
- Late-window winner pasif

Çıktı: data/gravie_sim_backtest.json
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
OUT_JSON = ROOT / "data" / "gravie_sim_backtest.json"

LOG_PATHS = [
    ROOT / "data" / "genel.log",
    ROOT / "data" / "genel2.log",
    ROOT / "data" / "gercekbotlog.log",
]

WL = {"5m": 300, "15m": 900, "1h": 3600, "4h": 14400}
SideOut = Literal["Up", "Down"]


def parse_market_slug(slug: str) -> tuple[str | None, str | None, int | None]:
    m = re.match(r"(btc|eth|sol|xrp)-updown-(5m|15m|1h|4h)-(\d+)", slug or "")
    if not m:
        return None, None, None
    return m.group(1), m.group(2), int(m.group(3))


# === Gravie V3 ASYM defaults (src/config.rs GravieParams) ===
@dataclass
class GravieParams:
    tick_interval_secs: int = 5
    buy_cooldown_ms: int = 4000      # winner cooldown
    hedge_cooldown_ms: int = 4000    # hedge cooldown
    winner_order_usdc: float = 15.0
    hedge_order_usdc: float = 5.0
    winner_max_price: float = 0.65
    hedge_max_price: float = 0.65
    avg_sum_max: float = 0.80
    signal_up_threshold: float = 5.5
    signal_down_threshold: float = 4.5
    stability_window: int = 3
    stability_max_std: float = 0.5
    ema_alpha: float = 0.3
    t_cutoff_secs: float = 30.0
    late_winner_pasif_secs: float = 90.0
    max_fak_size: float = 50.0
    max_size_per_side: float = 0.0  # 0 = unlimited


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
    """FAK BUY emri planla. avg_sum kontrolü. Return (price, size) or None."""
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

    # avg_sum gating
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
    """Tek tick kararı. Return: (state, metrics, [(outcome, price, size, reason), ...])"""
    trades: list[tuple[SideOut, float, float, str]] = []

    # T-cutoff
    if to_end <= p.t_cutoff_secs:
        return st, m, trades

    rel_secs = (now_ms // 1000) - start_ts

    # tick_interval gate
    if rel_secs % p.tick_interval_secs != 0:
        return st, m, trades
    if rel_secs == st.last_acted_secs:
        return st, m, trades
    st.last_acted_secs = rel_secs

    # OB check
    if up_ask <= 0 or down_ask <= 0:
        return st, m, trades
    if signal_score <= 0:
        return st, m, trades

    # EMA smoothing
    centered = signal_score - 5.0
    smoothed_centered = centered if st.ema_state is None else (
        p.ema_alpha * centered + (1.0 - p.ema_alpha) * st.ema_state
    )
    st.ema_state = smoothed_centered
    smoothed = smoothed_centered + 5.0

    # Stability filter
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

    # Signal direction
    if smoothed > p.signal_up_threshold:
        winner: SideOut = "Up"
    elif smoothed < p.signal_down_threshold:
        winner = "Down"
    else:
        return st, m, trades

    hedge: SideOut = "Down" if winner == "Up" else "Up"

    # Late-window: winner BUY engeli
    winner_allowed = p.late_winner_pasif_secs <= 0 or to_end > p.late_winner_pasif_secs

    # Winner BUY
    winner_ask = up_ask if winner == "Up" else down_ask
    if winner_allowed and winner_ask > 0 and winner_ask <= p.winner_max_price:
        if now_ms - st.last_winner_buy_ms >= p.buy_cooldown_ms:
            res = try_buy(m, winner, winner_ask, p.winner_order_usdc, p, api_min)
            if res:
                px, sz = res
                m.ingest_buy(winner, px, sz, is_winner=True)
                st.last_winner_buy_ms = now_ms
                trades.append((winner, px, sz, f"gravie:winner:{winner.lower()}"))

    # Hedge BUY (only if winner side has position)
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
    ticks: list[tuple[int, float, float, float, float, float]],  # ts_ms, ub, ua, db, da, signal
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


def load_logs() -> tuple[dict[str, list[dict]], dict[str, SideOut | None]]:
    by_slug: dict[str, list[dict]] = defaultdict(list)
    activities_redeem: dict[str, list[float]] = defaultdict(list)
    for path in LOG_PATHS:
        if not path.exists():
            continue
        with open(path) as f:
            d = json.load(f)
        for t in d.get("trades", []):
            by_slug[t["slug"]].append(t)
        for a in d.get("activity", []):
            if a.get("type") == "REDEEM" and a.get("slug"):
                activities_redeem[a["slug"]].append(float(a.get("usdcSize", 0)))

    winner: dict[str, SideOut | None] = {}
    for slug, trades in by_slug.items():
        ups = sum(x["size"] for x in trades if x.get("outcome") == "Up")
        dns = sum(x["size"] for x in trades if x.get("outcome") == "Down")
        rds = activities_redeem.get(slug, [])
        payout = sum(rds)
        if not rds or ups + dns <= 0:
            winner[slug] = None
            continue
        if abs(payout - ups) < 0.6:
            winner[slug] = "Up"
        elif abs(payout - dns) < 0.6:
            winner[slug] = "Down"
        else:
            winner[slug] = None

    return dict(by_slug), winner


def proxy_winner_from_row(r: tuple[Any, ...]) -> SideOut:
    _ts, ub, ua, db, da, *_rest = r
    um = (float(ub) + float(ua)) / 2.0
    dm = (float(db) + float(da)) / 2.0
    return "Up" if um >= dm else "Down"


def main() -> None:
    params = GravieParams()
    real_by_slug, winner_by_slug = load_logs()

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

    sim_results: list[dict[str, Any]] = []
    signal_accuracy: dict[str, dict[str, int]] = {
        "5m": {"total": 0, "correct": 0},
        "15m": {"total": 0, "correct": 0},
        "1h": {"total": 0, "correct": 0},
        "4h": {"total": 0, "correct": 0},
    }

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

        # Proxy winner from last OB
        last_row = rows[-1] if rows else None
        proxy_win = proxy_winner_from_row(last_row) if last_row else None
        real_win = winner_by_slug.get(slug)
        winner = real_win or proxy_win

        # PnL calculation
        pnl = None
        if winner:
            payout = m.up_filled if winner == "Up" else m.down_filled
            pnl = payout - m.cost

        # Signal accuracy: sinyal doğru tarafa mı yönlendirdi?
        if winner and n_tr > 0:
            winner_trades = [t for t in trades if "winner" in t["reason"]]
            if winner_trades:
                first_winner = winner_trades[0]
                pred_dir = "Up" if "up" in first_winner["reason"] else "Down"
                signal_accuracy[wl_str]["total"] += 1
                if pred_dir == winner:
                    signal_accuracy[wl_str]["correct"] += 1

        avg_sum = m.avg_up + m.avg_down if m.up_filled > 0 and m.down_filled > 0 else None
        dual = m.up_filled > 0 and m.down_filled > 0

        sim_results.append({
            "session_id": sid,
            "bot_id": s["bot_id"],
            "slug": slug,
            "asset": asset,
            "window": wl_str,
            "n_ticks": len(ticks),
            "n_sim_trades": n_tr,
            "n_winner_buys": m.n_winner_buys,
            "n_hedge_buys": m.n_hedge_buys,
            "sim_cost": round(m.cost, 2),
            "sim_up": round(m.up_filled, 2),
            "sim_down": round(m.down_filled, 2),
            "avg_sum": round(avg_sum, 4) if avg_sum else None,
            "dual_side": dual,
            "winner": winner,
            "pnl": round(pnl, 2) if pnl is not None else None,
        })

    # Aggregations
    total_cost = sum(r["sim_cost"] for r in sim_results)
    total_pnl_known = sum(r["pnl"] for r in sim_results if r["pnl"] is not None)
    wins = sum(1 for r in sim_results if r["pnl"] is not None and r["pnl"] > 0)
    losses = sum(1 for r in sim_results if r["pnl"] is not None and r["pnl"] <= 0)
    dual_count = sum(1 for r in sim_results if r["dual_side"])
    arb_count = sum(1 for r in sim_results if r["avg_sum"] and r["avg_sum"] < 1.0)

    # By window
    by_window: dict[str, dict[str, Any]] = {}
    for wl in ["5m", "15m", "1h", "4h"]:
        subset = [r for r in sim_results if r["window"] == wl]
        if not subset:
            continue
        wl_cost = sum(r["sim_cost"] for r in subset)
        wl_pnl = sum(r["pnl"] for r in subset if r["pnl"] is not None)
        wl_wins = sum(1 for r in subset if r["pnl"] is not None and r["pnl"] > 0)
        wl_losses = sum(1 for r in subset if r["pnl"] is not None and r["pnl"] <= 0)
        sig_acc = signal_accuracy[wl]
        by_window[wl] = {
            "sessions": len(subset),
            "cost": round(wl_cost, 2),
            "pnl": round(wl_pnl, 2),
            "roi_pct": round(wl_pnl / wl_cost * 100, 2) if wl_cost > 0 else 0,
            "wins": wl_wins,
            "losses": wl_losses,
            "winrate_pct": round(wl_wins / (wl_wins + wl_losses) * 100, 2) if wl_wins + wl_losses > 0 else 0,
            "signal_accuracy_pct": round(sig_acc["correct"] / sig_acc["total"] * 100, 2) if sig_acc["total"] > 0 else None,
        }

    # By asset
    by_asset: dict[str, dict[str, Any]] = {}
    for asset in ["btc", "eth", "sol", "xrp"]:
        subset = [r for r in sim_results if r["asset"] == asset]
        if not subset:
            continue
        a_cost = sum(r["sim_cost"] for r in subset)
        a_pnl = sum(r["pnl"] for r in subset if r["pnl"] is not None)
        a_wins = sum(1 for r in subset if r["pnl"] is not None and r["pnl"] > 0)
        a_losses = sum(1 for r in subset if r["pnl"] is not None and r["pnl"] <= 0)
        by_asset[asset] = {
            "sessions": len(subset),
            "cost": round(a_cost, 2),
            "pnl": round(a_pnl, 2),
            "roi_pct": round(a_pnl / a_cost * 100, 2) if a_cost > 0 else 0,
            "wins": a_wins,
            "losses": a_losses,
            "winrate_pct": round(a_wins / (a_wins + a_losses) * 100, 2) if a_wins + a_losses > 0 else 0,
        }

    out = {
        "meta": {
            "db": str(DB),
            "gravie_params": params.__dict__,
            "signal_report_alignment": {
                "ema_smoothing": "signal-report §9.1 — EMA fusion",
                "stability_filter": "signal-report §5.3 — variance gating",
                "avg_sum_guard": "signal-report §7.1 — complete-set arbitrage",
                "late_window_pasif": "signal-report §12.1 — T-10s sweet spot uyumu",
            },
        },
        "summary": {
            "total_sessions": len(sim_results),
            "total_trades": sum(r["n_sim_trades"] for r in sim_results),
            "total_cost_usdc": round(total_cost, 2),
            "total_pnl_usdc": round(total_pnl_known, 2),
            "roi_pct": round(total_pnl_known / total_cost * 100, 2) if total_cost > 0 else 0,
            "wins": wins,
            "losses": losses,
            "winrate_pct": round(wins / (wins + losses) * 100, 2) if wins + losses > 0 else 0,
            "dual_side_sessions": dual_count,
            "arbitrage_sessions_avg_sum_lt_1": arb_count,
        },
        "by_window": by_window,
        "by_asset": by_asset,
        "signal_accuracy_by_window": {
            wl: {
                "total": signal_accuracy[wl]["total"],
                "correct": signal_accuracy[wl]["correct"],
                "accuracy_pct": round(signal_accuracy[wl]["correct"] / signal_accuracy[wl]["total"] * 100, 2)
                if signal_accuracy[wl]["total"] > 0 else None,
            }
            for wl in ["5m", "15m", "1h", "4h"]
        },
        "top_10_profitable": sorted(
            [r for r in sim_results if r["pnl"] is not None],
            key=lambda x: x["pnl"],
            reverse=True,
        )[:10],
        "top_10_losers": sorted(
            [r for r in sim_results if r["pnl"] is not None],
            key=lambda x: x["pnl"],
        )[:10],
        "all_results": sim_results,
    }

    OUT_JSON.parent.mkdir(parents=True, exist_ok=True)
    with open(OUT_JSON, "w") as f:
        json.dump(out, f, indent=2)

    print(json.dumps({
        "sessions": out["summary"]["total_sessions"],
        "total_cost": out["summary"]["total_cost_usdc"],
        "total_pnl": out["summary"]["total_pnl_usdc"],
        "roi_pct": out["summary"]["roi_pct"],
        "winrate_pct": out["summary"]["winrate_pct"],
        "by_window": out["by_window"],
        "signal_accuracy": out["signal_accuracy_by_window"],
    }, indent=2))


if __name__ == "__main__":
    main()
