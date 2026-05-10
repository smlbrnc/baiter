#!/usr/bin/env python3
"""
Gravie backtest — Window Delta sinyali ile (signal-report.md Section 5.1).

Mevcut signal_score yerine window_delta kullanarak winrate ve ROI hesapla.
"""
import json
import math
import re
import sqlite3
from dataclasses import dataclass, field
from pathlib import Path
from typing import Literal

DB = Path(__file__).resolve().parents[1] / "data" / "baiter.db"
OUT_JSON = Path(__file__).resolve().parents[1] / "data" / "gravie_window_delta_backtest.json"

SideOut = Literal["Up", "Down"]


def window_delta_signal(window_open_mid: float, current_mid: float) -> tuple[float, float]:
    """Signal-report Section 5.1 - Window Delta
    
    Returns: (delta_pct, signal_score 0-10)
    """
    if window_open_mid <= 0:
        return 0.0, 5.0
    
    delta_pct = (current_mid - window_open_mid) / window_open_mid * 100
    direction = 1 if delta_pct > 0 else -1 if delta_pct < 0 else 0
    abs_delta = abs(delta_pct)
    
    # Signal-report Section 5.1 tier sistemi
    if abs_delta > 0.10:
        weight = 7  # decisive
    elif abs_delta > 0.02:
        weight = 5  # strong
    elif abs_delta > 0.005:
        weight = 3  # moderate
    elif abs_delta > 0.001:
        weight = 1  # slight
    else:
        weight = 0
    
    signal = 5.0 + (direction * weight * 0.5)
    return delta_pct, signal


@dataclass
class GravieParams:
    tick_interval_secs: int = 5
    buy_cooldown_ms: int = 4000
    hedge_cooldown_ms: int = 4000
    winner_order_usdc: float = 15.0
    hedge_order_usdc: float = 5.0
    winner_max_price: float = 0.65
    hedge_max_price: float = 0.65
    avg_sum_max: float = 0.80
    signal_up_threshold: float = 6.0    # Window delta > 0.02% → signal > 6.0
    signal_down_threshold: float = 4.0  # Window delta < -0.02% → signal < 4.0
    stability_window: int = 3
    stability_max_std: float = 0.3
    ema_alpha: float = 0.3
    t_cutoff_secs: float = 30.0
    late_winner_pasif_secs: float = 90.0
    max_fak_size: float = 50.0


@dataclass
class Metrics:
    up_filled: float = 0.0
    down_filled: float = 0.0
    avg_up: float = 0.0
    avg_down: float = 0.0
    cost: float = 0.0
    n_winner: int = 0
    n_hedge: int = 0

    def ingest(self, outcome: SideOut, price: float, size: float, is_winner: bool):
        self.cost += price * size
        if is_winner:
            self.n_winner += 1
        else:
            self.n_hedge += 1
        if outcome == "Up":
            nu = self.up_filled + size
            self.avg_up = (self.avg_up * self.up_filled + price * size) / nu if nu > 0 else 0
            self.up_filled = nu
        else:
            nd = self.down_filled + size
            self.avg_down = (self.avg_down * self.down_filled + price * size) / nd if nd > 0 else 0
            self.down_filled = nd


@dataclass
class State:
    last_secs: int = -999999
    last_winner_ms: int = 0
    last_hedge_ms: int = 0
    ema: float | None = None
    hist: list = field(default_factory=list)
    window_open_mid: float | None = None


def try_buy(m: Metrics, side: SideOut, ask: float, usdc: float, p: GravieParams, api_min: float):
    if ask <= 0 or ask > 1.0:
        return None
    size = min(math.ceil(usdc / ask), p.max_fak_size)
    
    own_f = m.up_filled if side == "Up" else m.down_filled
    own_s = m.avg_up * m.up_filled if side == "Up" else m.avg_down * m.down_filled
    opp_f = m.down_filled if side == "Up" else m.up_filled
    opp_s = m.avg_down * m.down_filled if side == "Up" else m.avg_up * m.up_filled
    
    if size <= 0 or size * ask < api_min:
        return None
    
    if opp_f > 0:
        new_avg = (own_s + size * ask) / (own_f + size)
        if new_avg + opp_s / opp_f >= p.avg_sum_max:
            return None
    
    return (ask, size)


def sim_session(ticks, start_ts, end_ts, p: GravieParams, api_min: float = 1.0):
    m = Metrics()
    st = State()
    
    for ts_ms, ub, ua, db, da in ticks:
        ub, ua, db, da = float(ub), float(ua), float(db), float(da)
        
        # Window open mid hesapla (ilk geçerli tick)
        if st.window_open_mid is None and ub > 0 and ua > 0 and db > 0 and da > 0:
            up_mid = (ub + ua) / 2
            dn_mid = (db + da) / 2
            st.window_open_mid = (up_mid + (1 - dn_mid)) / 2
            continue
        
        if st.window_open_mid is None:
            continue
        
        to_end = float(end_ts) - ts_ms / 1000.0
        if to_end <= p.t_cutoff_secs:
            continue
        
        rel = (ts_ms // 1000) - start_ts
        if rel % p.tick_interval_secs != 0 or rel == st.last_secs:
            continue
        st.last_secs = rel
        
        if ua <= 0 or da <= 0:
            continue
        
        # Current mid ve Window Delta hesapla
        up_mid = (ub + ua) / 2
        dn_mid = (db + da) / 2
        current_mid = (up_mid + (1 - dn_mid)) / 2
        
        delta_pct, wd_signal = window_delta_signal(st.window_open_mid, current_mid)
        
        # EMA smoothing
        centered = wd_signal - 5.0
        st.ema = centered if st.ema is None else p.ema_alpha * centered + (1 - p.ema_alpha) * st.ema
        smoothed = st.ema + 5.0
        
        # Stability filter
        if len(st.hist) >= p.stability_window:
            st.hist.pop(0)
        st.hist.append(smoothed)
        if len(st.hist) < p.stability_window:
            continue
        mean = sum(st.hist) / len(st.hist)
        std = math.sqrt(sum((x - mean) ** 2 for x in st.hist) / len(st.hist))
        if std > p.stability_max_std:
            continue
        
        # Signal direction
        if smoothed > p.signal_up_threshold:
            winner: SideOut = "Up"
        elif smoothed < p.signal_down_threshold:
            winner = "Down"
        else:
            continue
        
        hedge: SideOut = "Down" if winner == "Up" else "Up"
        
        # Late-window
        winner_allowed = p.late_winner_pasif_secs <= 0 or to_end > p.late_winner_pasif_secs
        
        # Winner BUY
        w_ask = ua if winner == "Up" else da
        if winner_allowed and w_ask > 0 and w_ask <= p.winner_max_price:
            if ts_ms - st.last_winner_ms >= p.buy_cooldown_ms:
                res = try_buy(m, winner, w_ask, p.winner_order_usdc, p, api_min)
                if res:
                    px, sz = res
                    m.ingest(winner, px, sz, True)
                    st.last_winner_ms = ts_ms
        
        # Hedge BUY
        w_filled = m.up_filled if winner == "Up" else m.down_filled
        h_ask = ua if hedge == "Up" else da
        if h_ask > 0 and h_ask <= p.hedge_max_price and w_filled > 0:
            if ts_ms - st.last_hedge_ms >= p.hedge_cooldown_ms:
                res = try_buy(m, hedge, h_ask, p.hedge_order_usdc, p, api_min)
                if res:
                    px, sz = res
                    m.ingest(hedge, px, sz, False)
                    st.last_hedge_ms = ts_ms
    
    return m


def proxy_winner(rows):
    if not rows:
        return None
    last = rows[-1]
    um = (float(last[1]) + float(last[2])) / 2.0
    dm = (float(last[3]) + float(last[4])) / 2.0
    return "Up" if um >= dm else "Down"


def main():
    params = GravieParams()
    
    conn = sqlite3.connect(DB)
    cur = conn.cursor()
    cur.execute("""
        SELECT ms.id, ms.slug, ms.start_ts, ms.end_ts,
               COALESCE(ms.min_order_size, 1.0) AS min_order_size
        FROM market_sessions ms
        JOIN bots b ON b.id = ms.bot_id
        WHERE b.strategy = 'gravie'
        ORDER BY ms.id
    """)
    sessions = cur.fetchall()
    
    results = []
    
    for sid, slug, st, et, api_min in sessions:
        m = re.match(r"(btc|eth|sol|xrp)-updown-(5m|15m|1h|4h)-(\d+)", slug or "")
        if not m:
            continue
        
        cur.execute("""
            SELECT ts_ms, up_best_bid, up_best_ask, down_best_bid, down_best_ask
            FROM market_ticks WHERE market_session_id = ?
            ORDER BY ts_ms
        """, (sid,))
        rows = cur.fetchall()
        if len(rows) < 10:
            continue
        
        ticks = [(r[0], r[1], r[2], r[3], r[4]) for r in rows]
        met = sim_session(ticks, st, et, params, float(api_min))
        
        winner = proxy_winner(rows)
        if not winner:
            continue
        
        pnl = None
        if met.cost > 0:
            payout = met.up_filled if winner == "Up" else met.down_filled
            pnl = payout - met.cost
        
        dual = met.up_filled > 0 and met.down_filled > 0
        avg_sum = met.avg_up + met.avg_down if dual else None
        
        results.append({
            "slug": slug,
            "cost": round(met.cost, 2),
            "up": round(met.up_filled, 2),
            "down": round(met.down_filled, 2),
            "winner": winner,
            "pnl": round(pnl, 2) if pnl else None,
            "dual": dual,
            "avg_sum": round(avg_sum, 4) if avg_sum else None,
            "n_trades": met.n_winner + met.n_hedge,
        })
    
    # Summary
    with_pnl = [r for r in results if r["pnl"] is not None]
    total_cost = sum(r["cost"] for r in with_pnl)
    total_pnl = sum(r["pnl"] for r in with_pnl)
    wins = sum(1 for r in with_pnl if r["pnl"] > 0)
    losses = sum(1 for r in with_pnl if r["pnl"] <= 0)
    dual_cnt = sum(1 for r in with_pnl if r["dual"])
    single_cnt = sum(1 for r in with_pnl if not r["dual"])
    
    # Dual vs Single winrate
    dual_wins = sum(1 for r in with_pnl if r["dual"] and r["pnl"] > 0)
    dual_losses = sum(1 for r in with_pnl if r["dual"] and r["pnl"] <= 0)
    single_wins = sum(1 for r in with_pnl if not r["dual"] and r["pnl"] > 0)
    single_losses = sum(1 for r in with_pnl if not r["dual"] and r["pnl"] <= 0)
    
    summary = {
        "params": params.__dict__,
        "sessions": len(with_pnl),
        "cost": round(total_cost, 2),
        "pnl": round(total_pnl, 2),
        "roi_pct": round(total_pnl / total_cost * 100, 2) if total_cost > 0 else 0,
        "wins": wins,
        "losses": losses,
        "winrate_pct": round(wins / (wins + losses) * 100, 2) if wins + losses > 0 else 0,
        "dual_sessions": dual_cnt,
        "single_sessions": single_cnt,
        "dual_winrate_pct": round(dual_wins / (dual_wins + dual_losses) * 100, 2) if dual_wins + dual_losses > 0 else 0,
        "single_winrate_pct": round(single_wins / (single_wins + single_losses) * 100, 2) if single_wins + single_losses > 0 else 0,
    }
    
    out = {"summary": summary, "results": results}
    
    with open(OUT_JSON, "w") as f:
        json.dump(out, f, indent=2)
    
    print("=== WINDOW DELTA BACKTEST SONUÇLARI ===")
    print(f"Sessions: {summary['sessions']}")
    print(f"Cost: ${summary['cost']:,.2f}")
    print(f"PnL: ${summary['pnl']:,.2f}")
    print(f"ROI: {summary['roi_pct']}%")
    print(f"Winrate: {summary['winrate_pct']}% ({wins}W/{losses}L)")
    print()
    print(f"Dual-side: {dual_cnt} sessions, {summary['dual_winrate_pct']}% WR")
    print(f"Tek taraflı: {single_cnt} sessions, {summary['single_winrate_pct']}% WR")


if __name__ == "__main__":
    main()
