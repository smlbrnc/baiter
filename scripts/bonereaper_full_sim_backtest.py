#!/usr/bin/env python3
"""
Bonereaper — tüm market oturumlarında tick simülasyonu + sinyal backtest.

- Yerel data/baiter.db: market_ticks + market_sessions (strategy=bonereaper)
- data/*.log: gerçek trade + REDEEM (kazanan taraf türetimi)
- Motor: src/strategy/bonereaper.rs ile aynı default parametreler (Python)

Çıktı: data/bonereaper_sim_backtest.json
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
OUT_JSON = ROOT / "data" / "bonereaper_sim_backtest.json"

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


# === Bonereaper defaults (src/config.rs StrategyParams) ===
@dataclass
class BonereaperParams:
    buy_cooldown_ms: int = 8000
    late_winner_secs: int = 30
    late_winner_bid_thr: float = 0.85
    late_winner_usdc: float = 1000.0
    lw_max_per_session: int = 1
    imbalance_thr: float = 100.0
    max_avg_sum: float = 1.30
    size_longshot: float = 15.0
    size_mid: float = 25.0
    size_high: float = 30.0
    min_price: float = 0.05
    max_price: float = 0.95


@dataclass
class Metrics:
    up_filled: float = 0.0
    down_filled: float = 0.0
    avg_up: float = 0.0
    avg_down: float = 0.0
    cost: float = 0.0

    def ingest_buy(self, outcome: SideOut, price: float, size: float) -> None:
        self.cost += price * size
        if outcome == "Up":
            nu = self.up_filled + size
            self.avg_up = (self.avg_up * self.up_filled + price * size) / nu if nu > 0 else 0.0
            self.up_filled = nu
        else:
            nd = self.down_filled + size
            self.avg_down = (self.avg_down * self.down_filled + price * size) / nd if nd > 0 else 0.0
            self.down_filled = nd


@dataclass
class ActiveState:
    last_buy_ms: int = 0
    last_up_bid: float = 0.0
    last_dn_bid: float = 0.0
    lw_injections: int = 0


@dataclass
class SimResult:
    slug: str
    session_id: int
    bot_id: int
    n_ticks: int
    n_sim_trades: int
    sim_cost: float
    sim_up: float
    sim_down: float
    avg_sum_end: float
    ticks_ok: bool = True
    error: str | None = None


def decide_buy(
    st: ActiveState,
    m: Metrics,
    p: BonereaperParams,
    *,
    now_ms: int,
    up_bid: float,
    up_ask: float,
    down_bid: float,
    down_ask: float,
    to_end: float,
    api_min: float,
) -> tuple[ActiveState, Metrics, SideOut | None, float, float, str | None]:
    """Tek tick kararı. Dönüş: yeni state, yeni metrics, outcome|None, price, size, reason."""
    book_ready = up_bid > 0 and up_ask > 0 and down_bid > 0 and down_ask > 0
    if not book_ready or up_bid <= 0 or down_bid <= 0:
        return st, m, None, 0.0, 0.0, None

    # --- Late winner ---
    lw_ok = p.late_winner_usdc > 0 and p.late_winner_secs > 0 and 0 < to_end <= float(p.late_winner_secs)
    lw_quota = p.lw_max_per_session == 0 or st.lw_injections < p.lw_max_per_session
    if lw_ok and lw_quota:
        if up_bid >= down_bid:
            winner: SideOut = "Up"
            w_bid, w_ask = up_bid, up_ask
        else:
            winner = "Down"
            w_bid, w_ask = down_bid, down_ask
        if w_bid >= p.late_winner_bid_thr and w_ask > 0:
            sz = math.ceil(p.late_winner_usdc / w_ask)
            if sz * w_ask >= api_min:
                st = ActiveState(
                    last_buy_ms=now_ms,
                    last_up_bid=up_bid,
                    last_dn_bid=down_bid,
                    lw_injections=st.lw_injections + 1,
                )
                m.ingest_buy(winner, w_ask, float(sz))
                return st, m, winner, w_ask, float(sz), "lw"

    # --- Cooldown ---
    if st.last_buy_ms > 0 and now_ms - st.last_buy_ms < p.buy_cooldown_ms:
        st = ActiveState(
            last_buy_ms=st.last_buy_ms,
            last_up_bid=up_bid,
            last_dn_bid=down_bid,
            lw_injections=st.lw_injections,
        )
        return st, m, None, 0.0, 0.0, None

    imb = m.up_filled - m.down_filled
    if abs(imb) > p.imbalance_thr:
        dir_o: SideOut = "Down" if imb > 0 else "Up"
    else:
        d_up = abs(up_bid - st.last_up_bid)
        d_dn = abs(down_bid - st.last_dn_bid)
        if d_up == 0.0 and d_dn == 0.0:
            dir_o = "Up" if up_bid >= down_bid else "Down"
        elif d_up >= d_dn:
            dir_o = "Up"
        else:
            dir_o = "Down"

    st = ActiveState(
        last_buy_ms=st.last_buy_ms,
        last_up_bid=up_bid,
        last_dn_bid=down_bid,
        lw_injections=st.lw_injections,
    )

    bid = up_bid if dir_o == "Up" else down_bid
    ask = up_ask if dir_o == "Up" else down_ask
    if bid <= 0 or ask <= 0:
        return st, m, None, 0.0, 0.0, None
    if bid < p.min_price or bid > p.max_price:
        return st, m, None, 0.0, 0.0, None

    if bid <= 0.30:
        usdc = p.size_longshot
    elif bid <= 0.85:
        usdc = p.size_mid
    else:
        usdc = p.size_high
    if usdc <= 0:
        return st, m, None, 0.0, 0.0, None
    sz = math.ceil(usdc / ask)

    cur_f, cur_a, opp_f, opp_a = (
        (m.up_filled, m.avg_up, m.down_filled, m.avg_down)
        if dir_o == "Up"
        else (m.down_filled, m.avg_down, m.up_filled, m.avg_up)
    )
    if opp_f > 0.0:
        new_avg = (cur_a * cur_f + ask * sz) / (cur_f + sz) if cur_f > 0 else ask
        if new_avg + opp_a > p.max_avg_sum:
            return st, m, None, 0.0, 0.0, None

    if sz * ask < api_min:
        return st, m, None, 0.0, 0.0, None

    st = ActiveState(
        last_buy_ms=now_ms,
        last_up_bid=up_bid,
        last_dn_bid=down_bid,
        lw_injections=st.lw_injections,
    )
    m.ingest_buy(dir_o, ask, float(sz))
    return st, m, dir_o, ask, float(sz), "buy"


def run_session_sim(
    ticks: list[tuple[int, float, float, float, float]],
    start_ts: int,
    end_ts: int,
    api_min: float,
    p: BonereaperParams | None = None,
) -> tuple[Metrics, int, ActiveState]:
    p = p or BonereaperParams()
    if not ticks:
        return Metrics(), 0, ActiveState()

    m = Metrics()
    n_trades = 0
    # Idle -> Active: ilk geçerli book
    st = ActiveState()
    active = False
    for ts_ms, ub, ua, db, da in ticks:
        if not active:
            if ub > 0 and ua > 0 and db > 0 and da > 0:
                st = ActiveState(0, ub, db, 0)
                active = True
            continue
        to_end = float(end_ts) - ts_ms / 1000.0
        st, m, o, _px, _sz, _reason = decide_buy(
            st, m, p,
            now_ms=ts_ms,
            up_bid=ub, up_ask=ua, down_bid=db, down_ask=da,
            to_end=to_end,
            api_min=api_min,
        )
        if o is not None:
            n_trades += 1
    return m, n_trades, st


def load_logs() -> tuple[dict[str, list[dict]], dict[str, SideOut | None], dict[str, list[dict]]]:
    """slug -> trades list; slug -> winner from REDEEM; slug -> redeems"""
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

    redeem_lists = {k: [{"usdcSize": x} for x in v] for k, v in activities_redeem.items()}
    return dict(by_slug), winner, redeem_lists


def proxy_winner_from_row(r: tuple[Any, ...]) -> SideOut:
    """Son OB satırından yön: UP mid > DOWN mid → 'Up' (kâğıt üzerinde yaklaşık kazanan)."""
    _ts, ub, ua, db, da, *_rest = r
    um = (float(ub) + float(ua)) / 2.0
    dm = (float(db) + float(da)) / 2.0
    return "Up" if um >= dm else "Down"


def pick_row_near_ms(rows: list[tuple[Any, ...]], target_ms: int) -> tuple[Any, ...] | None:
    if not rows:
        return None
    best = min(rows, key=lambda r: abs(int(r[0]) - target_ms))
    return best


def main() -> None:
    params = BonereaperParams()
    real_by_slug, winner_by_slug, _redeems = load_logs()
    # Gerçek metrikler (log)
    real_summary: dict[str, Any] = {}
    for slug, trades in real_by_slug.items():
        a, w, mts = parse_market_slug(slug)
        if a is None:
            continue
        ups = sum(t["size"] for t in trades if t["outcome"] == "Up")
        dns = sum(t["size"] for t in trades if t["outcome"] == "Down")
        cost = sum(t["price"] * t["size"] for t in trades)
        wn = winner_by_slug.get(slug)
        pnl = None
        if wn == "Up":
            pnl = ups - cost
        elif wn == "Down":
            pnl = dns - cost
        dual = ups > 0 and dns > 0
        u_cost = sum(t["price"] * t["size"] for t in trades if t["outcome"] == "Up")
        d_cost = sum(t["price"] * t["size"] for t in trades if t["outcome"] == "Down")
        avg_sum = (u_cost / ups + d_cost / dns) if dual and ups and dns else None
        real_summary[slug] = {
            "n_trades": len(trades),
            "cost": cost,
            "up_sh": ups,
            "down_sh": dns,
            "winner": wn,
            "pnl_if_resolved": pnl,
            "avg_sum": avg_sum,
        }

    conn = sqlite3.connect(DB)
    conn.row_factory = sqlite3.Row
    cur = conn.cursor()
    cur.execute(
        """
        SELECT ms.id, ms.bot_id, ms.slug, ms.start_ts, ms.end_ts,
               COALESCE(ms.min_order_size, 1.0) AS min_order_size
        FROM market_sessions ms
        JOIN bots b ON b.id = ms.bot_id
        WHERE b.strategy = 'bonereaper'
        ORDER BY ms.id
        """
    )
    sessions = [dict(r) for r in cur.fetchall()]

    sim_results: list[dict[str, Any]] = []
    signal_tests: list[dict[str, Any]] = []

    # Sinyal backtest — iki hedef: (A) REDEEM kazananı, (B) son tick proxy kazananı
    def reset_sig() -> dict[str, int]:
        return {k: 0 for k in ("tot", "ok_sc", "ok_bs", "ok_cb", "ok_of", "ok_cv")}

    sig_redeem = reset_sig()
    sig_same_snapshot = reset_sig()  # uyarı: sinyal ile aynı OB satırı — sadece iç tutarlılık
    sig_predict_T60 = reset_sig()  # T-60s civarı sinyal → son OB proxy kazananı
    sig_predict_mid = reset_sig()  # pencere ortası sinyal → son OB proxy

    for s in sessions:
        sid = s["id"]
        slug = s["slug"]
        cur.execute(
            """
            SELECT ts_ms, up_best_bid, up_best_ask, down_best_bid, down_best_ask,
                   signal_score, bsi, ofi, cvd
            FROM market_ticks WHERE market_session_id = ?
            ORDER BY ts_ms
            """,
            (sid,),
        )
        rows = cur.fetchall()
        ticks = [(r[0], float(r[1]), float(r[2]), float(r[3]), float(r[4])) for r in rows]
        tail = [r for r in rows if r[0] >= s["end_ts"] * 1000 - 15_000] if rows else []
        last_ev = tail[-1] if tail else (rows[-1] if rows else None)
        mid_t = int((s["start_ts"] + s["end_ts"]) / 2 * 1000)
        mid_row = pick_row_near_ms(list(rows), mid_t) if rows else None
        t60_row = pick_row_near_ms(list(rows), s["end_ts"] * 1000 - 60_000) if rows else None

        api_min = max(1.0, float(s["min_order_size"]))
        m, n_tr, _st = run_session_sim(ticks, s["start_ts"], s["end_ts"], api_min, params)
        avg_end = (m.avg_up + m.avg_down) if (m.up_filled > 0 and m.down_filled > 0) else 0.0
        sim_results.append({
            "session_id": sid,
            "bot_id": s["bot_id"],
            "slug": slug,
            "n_ticks": len(ticks),
            "n_sim_trades": n_tr,
            "sim_cost": round(m.cost, 2),
            "sim_up": round(m.up_filled, 2),
            "sim_down": round(m.down_filled, 2),
            "avg_sum_end": round(avg_end, 4) if avg_end else None,
        })

        wn_redeem = winner_by_slug.get(slug)
        proxy_final = proxy_winner_from_row(last_ev) if last_ev else None
        proxy_mid_target = proxy_winner_from_row(mid_row) if mid_row else None

        def tally(
            bucket: dict[str, int],
            truth: SideOut | None,
            ev: tuple[Any, ...] | None,
        ) -> None:
            if truth is None or ev is None:
                return
            _ts, _ub, _ua, _db, _da, score, bsi, ofi, cvd = ev
            pred_sc = float(score) > 5.0
            pred_bs = float(bsi) > 0.0
            pred_cb = pred_sc and pred_bs
            pred_of = float(ofi) > 0.0
            pred_cv = float(cvd) > 0.0
            act_up = truth == "Up"
            bucket["tot"] += 1
            if pred_sc == act_up:
                bucket["ok_sc"] += 1
            if pred_bs == act_up:
                bucket["ok_bs"] += 1
            if pred_cb == act_up:
                bucket["ok_cb"] += 1
            if pred_of == act_up:
                bucket["ok_of"] += 1
            if pred_cv == act_up:
                bucket["ok_cv"] += 1

        if last_ev:
            tally(sig_redeem, wn_redeem, last_ev)
            if proxy_final:
                tally(sig_same_snapshot, proxy_final, last_ev)
                if t60_row:
                    tally(sig_predict_T60, proxy_final, t60_row)
                if mid_row:
                    tally(sig_predict_mid, proxy_final, mid_row)

        if last_ev and (wn_redeem is not None or proxy_final):
            ts_ms, _ub, _ua, _db, _da, score, bsi, ofi, cvd = last_ev
            signal_tests.append({
                "slug": slug,
                "session_id": sid,
                "winner_redeem_log": wn_redeem,
                "proxy_winner_last_ob": proxy_final,
                "proxy_winner_mid_ob": proxy_mid_target,
                "signal_score": round(float(score), 4),
                "bsi": round(float(bsi), 4),
                "ofi": round(float(ofi), 4),
                "cvd": round(float(cvd), 4),
            })

    # Log ile session eşleştir: aynı slug'da gerçek vs sim (ilk session)
    by_slug_sessions = defaultdict(list)
    for r in sim_results:
        by_slug_sessions[r["slug"]].append(r)

    compare_rows: list[dict[str, Any]] = []
    for slug, reals in real_summary.items():
        sess_list = by_slug_sessions.get(slug)
        if not sess_list:
            continue
        sim0 = sess_list[0]
        compare_rows.append({
            "slug": slug,
            "real_n_trades": reals["n_trades"],
            "sim_n_trades": sim0["n_sim_trades"],
            "real_cost": round(reals["cost"], 2),
            "sim_cost": sim0["sim_cost"],
            "real_winner": reals["winner"],
            "real_pnl": round(reals["pnl_if_resolved"], 2) if reals["pnl_if_resolved"] is not None else None,
        })

    def acc_dict(b: dict[str, int]) -> dict[str, Any]:
        t = max(1, b["tot"])
        return {
            "n": b["tot"],
            "score_gt5": round(b["ok_sc"] / t, 4),
            "bsi_gt0": round(b["ok_bs"] / t, 4),
            "score_and_bsi": round(b["ok_cb"] / t, 4),
            "ofi_gt0": round(b["ok_of"] / t, 4),
            "cvd_gt0": round(b["ok_cv"] / t, 4),
        }

    # Sim PnL — önce REDEEM (log+slug eşleşmesi), yoksa proxy_final session başına
    sim_pnl_redeem: list[float] = []
    sim_pnl_proxy: list[float] = []
    slug_to_proxy: dict[str, SideOut] = {}
    for st in signal_tests:
        slug_to_proxy[st["slug"]] = st["proxy_winner_last_ob"] or "Up"

    for r in sim_results:
        slug = r["slug"]
        cost = r["sim_cost"]
        up_s, down_s = r["sim_up"], r["sim_down"]
        wn = winner_by_slug.get(slug)
        if wn is not None:
            pay = up_s if wn == "Up" else down_s
            sim_pnl_redeem.append(pay - cost)
        pw = slug_to_proxy.get(slug)
        if pw is not None:
            pay2 = up_s if pw == "Up" else down_s
            sim_pnl_proxy.append(pay2 - cost)

    out = {
        "meta": {
            "db": str(DB),
            "log_files": [str(p) for p in LOG_PATHS if p.exists()],
            "bonereaper_params": params.__dict__,
            "note_tr": (
                "Simülasyon motoru bonereaper.rs defaults ile uyumludur. "
                "REDEEM sadece log slug'ları ile kesişince doludur; "
                "proxy_winner: son 15s içindeki son OB satırında UP_mid vs DOWN_mid."
            ),
            "note": (
                "same_snapshot: sinyal ve 'truth' aynı OB satırından — tahmin değil, iç tutarlılık. "
                "predict_T60 / predict_mid: truth=son 15s son OB mid'ine göre proxy kazanan."
            ),
        },
        "simulation_totals": {
            "sessions": len(sim_results),
            "sum_sim_trades": int(sum(x["n_sim_trades"] for x in sim_results)),
            "median_sim_trades_per_session": sorted([x["n_sim_trades"] for x in sim_results])[len(sim_results) // 2] if sim_results else 0,
            "sum_sim_cost_usdc": round(sum(x["sim_cost"] for x in sim_results), 2),
        },
        "signal_backtest": {
            "truth_redeem_from_logs_last15s_ob": acc_dict(sig_redeem),
            "same_snapshot_signal_vs_mid_truth_CAUTION": acc_dict(sig_same_snapshot),
            "predict_T60s_row_truth_final_ob_proxy": acc_dict(sig_predict_T60),
            "predict_mid_window_row_truth_final_ob_proxy": acc_dict(sig_predict_mid),
        },
        "real_log_markets_parsed": len(real_summary),
        "compare_real_vs_sim_sample": compare_rows[:80],
        "compare_summary": {
            "n_compare": len(compare_rows),
            "median_real_trades": sorted([x["real_n_trades"] for x in compare_rows])[len(compare_rows) // 2] if compare_rows else 0,
            "median_sim_trades": sorted([x["sim_n_trades"] for x in compare_rows])[len(compare_rows) // 2] if compare_rows else 0,
            "sum_real_cost": round(sum(x["real_cost"] for x in compare_rows), 2),
            "sum_sim_cost": round(sum(x["sim_cost"] for x in compare_rows), 2),
        },
        "sim_pnl_redeem_winner": {
            "n": len(sim_pnl_redeem),
            "sum_usdc": round(sum(sim_pnl_redeem), 2),
            "wins": sum(1 for x in sim_pnl_redeem if x > 0),
        },
        "sim_pnl_proxy_winner": {
            "n": len(sim_pnl_proxy),
            "sum_usdc": round(sum(sim_pnl_proxy), 2),
            "wins": sum(1 for x in sim_pnl_proxy if x > 0),
        },
        "signal_tests_detail": signal_tests[:200],
        "all_sim_results": sim_results,
    }

    OUT_JSON.parent.mkdir(parents=True, exist_ok=True)
    with open(OUT_JSON, "w") as f:
        json.dump(out, f, indent=2)
    print(json.dumps({
        "sessions_simulated": out["simulation_totals"]["sessions"],
        "simulation_totals": out["simulation_totals"],
        "signal_backtest": out["signal_backtest"],
        "compare_summary": out["compare_summary"],
        "sim_pnl": {k: out[k] for k in ("sim_pnl_redeem_winner", "sim_pnl_proxy_winner")},
    }, indent=2))


if __name__ == "__main__":
    main()
