#!/usr/bin/env python3
"""Black-box bot reverse-engineering: trade-tick eşleştirme + per-order trigger/role çıkarımı.

Çıktı: exports/blackbox-trades-20260429.csv (her emir bir satır, 30+ kolon).

Yaklaşım:
- Her trade için bisect ile en yakın tick snapshot'ı bul (ts_ms tabanlı).
- Running session-state tut: dom/opp filled, avg_dom/avg_opp, last_pyr_ms, last_score, ...
- Kural-tabanlı candidate trigger ata (öncelik sıralı 9 kural).
- candidate_role: trigger ile bağlantılı rol etiketi.
- confidence: tek kural net eşleşti mi (high/medium/low).

Bu script kanonik eşik *çıkarmaz* — sadece sınıflandırma yapar.
Eşik tahmini için scripts/analyze_correlations.py kullanılır.
"""

from __future__ import annotations

import bisect
import csv
import json
import os
import sys
from dataclasses import dataclass, field
from glob import glob
from pathlib import Path
from typing import Optional

ROOT = Path(__file__).resolve().parent.parent
LOG_GLOB = str(ROOT / "exports" / "polymarket-log-*.json")
TICKS_DIR = ROOT / "exports" / "bot14-ticks-20260429"
OUT_CSV = ROOT / "exports" / "blackbox-trades-20260429.csv"
OUT_PER_MARKET_DIR = ROOT / "exports" / "blackbox-per-market-20260429"

TICK_SIZE = 0.01
SCORE_NEUTRAL = 5.0
SCOOP_OPP_BID_MAX = 0.05
DEADLINE_S = 290
WINDOW_S = 300

# ---------- veri tipleri ----------


@dataclass
class Tick:
    ts_ms: int
    up_bid: float
    up_ask: float
    down_bid: float
    down_ask: float
    score: float
    bsi: float
    ofi: float
    cvd: float


@dataclass
class Trade:
    market: str
    market_start: int
    t_off: float
    side: str
    outcome: str
    size: float
    price: float
    usdc: float


@dataclass
class SessionState:
    intent: Optional[str] = None
    up_filled: float = 0.0
    down_filled: float = 0.0
    avg_up: float = 0.0
    avg_down: float = 0.0
    last_score: Optional[float] = None
    last_trade_t: Optional[float] = None
    last_pyr_t: Optional[float] = None
    last_requote_t: Optional[float] = None
    avg_down_used: bool = False
    last_outcome_price: dict[str, float] = field(default_factory=dict)
    pair_count: int = 0
    last_size_per_outcome: dict[str, float] = field(default_factory=dict)
    predicted_opener: Optional[str] = None
    opener_rule: Optional[str] = None  # "reversion" / "momentum_dscore" / "momentum_score"


# ---------- opener intent predictor (composite signal) ----------

BSI_REVERSION_THRESHOLD = 1.0
DSCORE_DEAD_ZONE = 0.1


def predict_opener_intent(pre_ticks: list[Tick]) -> tuple[str, str]:
    """Composite signal-based opener intent.

    Veriden çıkarılan kural (6/6 doğru):
      1. |bsi| > BSI_REVERSION_THRESHOLD: mean reversion → BSI'nin tersi yön
         Yorum: BSI çok büyük olduğunda piyasada aşırı tek yönlü baskı vardır;
         bot bu trendin geri döneceğine bahis koyar (mean reversion).
      2. |Δscore| > DSCORE_DEAD_ZONE: momentum → Δscore yönü
         Yorum: Skor pre-opener pencerede belirgin yön değiştiriyorsa, bu yön
         gelecekte de süreceği varsayımı (trend following).
      3. else: pre-opener ortalama skor yönü (avg>=5 → Up, <5 → Down)
         Yorum: Sinyaller kararsız → score'un baseline'ını referans al.
    """
    if not pre_ticks:
        return ("Up", "default")
    last = pre_ticks[-1]
    first = pre_ticks[0]
    dscore = last.score - first.score

    if abs(last.bsi) > BSI_REVERSION_THRESHOLD:
        intent = "Down" if last.bsi > 0 else "Up"
        return (intent, "reversion")
    if abs(dscore) > DSCORE_DEAD_ZONE:
        return ("Up" if dscore > 0 else "Down", "momentum_dscore")
    score_avg = sum(t.score for t in pre_ticks) / len(pre_ticks)
    return ("Up" if score_avg >= SCORE_NEUTRAL else "Down", "momentum_score_avg")


# ---------- yükleme ----------


def load_ticks(market: str) -> list[Tick]:
    p = TICKS_DIR / f"{market}_ticks.json"
    raw = json.loads(p.read_text())
    out = [
        Tick(
            ts_ms=int(t["ts_ms"]),
            up_bid=float(t["up_best_bid"]),
            up_ask=float(t["up_best_ask"]),
            down_bid=float(t["down_best_bid"]),
            down_ask=float(t["down_best_ask"]),
            score=float(t["signal_score"]),
            bsi=float(t["bsi"]),
            ofi=float(t["ofi"]),
            cvd=float(t["cvd"]),
        )
        for t in raw
    ]
    out.sort(key=lambda x: x.ts_ms)
    return out


def load_trades(log_path: str) -> tuple[str, int, list[Trade]]:
    d = json.loads(Path(log_path).read_text())
    by_slug = (d.get("by_slug") or [{}])[0]
    market = by_slug.get("slug", "?")
    market_start = int(market.split("-")[-1])
    raw = sorted(
        d.get("trades", []),
        key=lambda t: (t["timestamp"], t.get("transactionHash", "")),
    )
    trades = []
    for t in raw:
        ts = int(t["timestamp"])
        size = float(t.get("size", 0))
        price = float(t.get("price", 0))
        trades.append(
            Trade(
                market=market,
                market_start=market_start,
                t_off=ts - market_start,
                side=t.get("side", "?"),
                outcome=t.get("outcome", "?"),
                size=size,
                price=price,
                usdc=float(t.get("usdcSize", size * price)),
            )
        )
    return market, market_start, trades


# ---------- tick eşleştirme ----------


def find_tick_at(ticks: list[Tick], ts_ms: int) -> Optional[Tick]:
    """En son ts_ms <= verilen ts_ms olan tick'i döner (snapshot-style)."""
    if not ticks:
        return None
    keys = [t.ts_ms for t in ticks]
    idx = bisect.bisect_right(keys, ts_ms) - 1
    if idx < 0:
        return ticks[0]
    return ticks[idx]


def find_prev_tick(ticks: list[Tick], ref: Tick, lookback_ms: int = 1000) -> Optional[Tick]:
    keys = [t.ts_ms for t in ticks]
    idx = bisect.bisect_right(keys, ref.ts_ms - lookback_ms) - 1
    if idx < 0:
        return None
    return ticks[idx]


# ---------- candidate trigger sınıflandırma ----------


def classify(
    trade: Trade,
    state: SessionState,
    tick: Tick,
    prev_tick: Optional[Tick],
    is_first_trade_in_market: bool,
) -> tuple[str, str, str]:
    """
    Returns (trigger, role, confidence)

    Öncelik sıralı kural zinciri (önce eşleşen kazanır):
      1. deadline_cleanup    (t_off >= 290)
      2. pre_resolve_scoop   (opp_bid <= 0.05 AND t_off >= 240)
      3. signal_open         (ilk emir)
      4. signal_flip         (önceki dom != bu trade outcome AND |dscore| > 1.0)
      5. avg_down_edge       (dom side AND price < avg_dom - 1 tick AND !avg_down_used)
      6. pyramid_signal      (dom side AND gap_to_prev_score_change_s < 10 AND ofi > 0.5 AND size >= prev_size)
      7. price_drift         (aynı outcome'a önceki emir vardı, |Δprice| >= 1 tick)
      8. parity_gap          (opp side AND |dom_filled-opp_filled| > min_gap)
      9. unknown
    """
    dscore = tick.score - state.last_score if state.last_score is not None else 0.0

    opp_bid = tick.down_bid if trade.outcome == "Up" else tick.up_bid

    # 1. deadline cleanup
    if trade.t_off >= DEADLINE_S:
        return ("deadline_cleanup", "cleanup", "high")

    # 2. pre-resolve scoop
    if opp_bid <= SCOOP_OPP_BID_MAX and trade.t_off >= 240:
        return ("pre_resolve_scoop", "scoop", "high")

    # 3. signal_open
    if is_first_trade_in_market:
        return ("signal_open", "opener_dom", "high")

    # 4. signal_flip (önceki dom intent'ten farklı outcome'a büyük score sıçramasıyla geçiş)
    if state.intent and trade.outcome != state.intent and abs(dscore) > 1.0:
        return ("signal_flip", "opener_dom", "high")

    is_dom = state.intent == trade.outcome
    avg_dom = state.avg_up if state.intent == "Up" else state.avg_down

    # 5. avg_down_edge (dom side, daha iyi fiyat — alış için "daha düşük" demek)
    if (
        is_dom
        and not state.avg_down_used
        and avg_dom > 0
        and trade.price + TICK_SIZE <= avg_dom
    ):
        return ("avg_down_edge", "avg_down", "high")

    # 6. pyramid_signal
    gap_score = (trade.t_off - (state.last_trade_t or trade.t_off))
    prev_size = state.last_size_per_outcome.get(trade.outcome, 0.0)
    if (
        is_dom
        and gap_score < 10
        and tick.ofi >= 0.5
        and trade.size >= prev_size * 0.9  # eşit/üst boyut
        and trade.size >= 30  # küçük tail emirleri pyramid sayma
    ):
        return ("pyramid_signal", "pyramid_dom", "medium")

    # 7. price_drift (aynı outcome, fiyat değişti)
    last_price = state.last_outcome_price.get(trade.outcome)
    if last_price is not None and abs(trade.price - last_price) >= TICK_SIZE:
        role = "requote_dom" if is_dom else "requote_hedge"
        return ("price_drift", role, "medium")

    # 8. parity_gap (opp side)
    if not is_dom:
        gap_qty = abs(state.up_filled - state.down_filled)
        if gap_qty > 5.0:
            return ("parity_gap", "hedge_topup", "medium")
        return ("parity_gap", "opener_hedge", "low")

    return ("unknown", "unknown", "low")


# ---------- state güncellemesi ----------


def update_state(state: SessionState, trade: Trade, trigger: str) -> None:
    if trigger == "signal_open":
        state.intent = trade.outcome
        state.pair_count = 1
    elif trigger == "signal_flip":
        state.intent = trade.outcome
        state.avg_down_used = False  # flip sonrası avg_down sıfırlanır
    elif trigger == "avg_down_edge":
        state.avg_down_used = True

    side_sign = 1.0 if trade.side == "BUY" else -1.0
    if trade.outcome == "Up":
        if side_sign > 0:
            new_total = state.up_filled + trade.size
            if new_total > 0:
                state.avg_up = (state.avg_up * state.up_filled + trade.price * trade.size) / new_total
        state.up_filled = max(0.0, state.up_filled + side_sign * trade.size)
    else:
        if side_sign > 0:
            new_total = state.down_filled + trade.size
            if new_total > 0:
                state.avg_down = (state.avg_down * state.down_filled + trade.price * trade.size) / new_total
        state.down_filled = max(0.0, state.down_filled + side_sign * trade.size)

    state.last_outcome_price[trade.outcome] = trade.price
    state.last_size_per_outcome[trade.outcome] = trade.size
    state.last_trade_t = trade.t_off

    if trigger == "pyramid_signal":
        state.last_pyr_t = trade.t_off


# ---------- ana akış ----------


def main() -> int:
    OUT_PER_MARKET_DIR.mkdir(parents=True, exist_ok=True)

    log_files = sorted(glob(LOG_GLOB))
    if not log_files:
        print(f"No log files found at {LOG_GLOB}", file=sys.stderr)
        return 1

    rows: list[dict[str, str]] = []

    for log_path in log_files:
        market, market_start, trades = load_trades(log_path)
        if not trades:
            print(f"  skip {market}: no trades")
            continue
        ticks = load_ticks(market)
        if not ticks:
            print(f"  skip {market}: no ticks")
            continue

        state = SessionState()
        per_market_rows: list[dict[str, str]] = []

        # Pre-opener pencere: ilk emirden ÖNCE ki tüm tick'ler
        first_trade_ts = trades[0].t_off + market_start
        pre_opener_ticks = [t for t in ticks if t.ts_ms / 1000 < first_trade_ts]
        if pre_opener_ticks:
            predicted, rule = predict_opener_intent(pre_opener_ticks)
            state.predicted_opener = predicted
            state.opener_rule = rule

        for idx, tr in enumerate(trades):
            tick = find_tick_at(ticks, (tr.t_off + market_start) * 1000 + 999)
            if tick is None:
                continue
            if state.last_score is None:
                # ilk trade'in tick'i ile referans olarak senkronize et
                state.last_score = tick.score

            prev_tick = find_prev_tick(ticks, tick, lookback_ms=1000)
            dscore = tick.score - (state.last_score or tick.score)

            trigger, role, conf = classify(
                tr,
                state,
                tick,
                prev_tick,
                is_first_trade_in_market=(idx == 0),
            )

            dom_filled_now = state.up_filled if state.intent == "Up" else state.down_filled
            opp_filled_now = state.down_filled if state.intent == "Up" else state.up_filled
            avg_dom_now = state.avg_up if state.intent == "Up" else state.avg_down
            avg_opp_now = state.avg_down if state.intent == "Up" else state.avg_up
            avg_sum = (state.avg_up + state.avg_down) if (state.up_filled > 0 and state.down_filled > 0) else 0.0
            imb = 0.0
            if dom_filled_now + opp_filled_now > 0:
                imb = (dom_filled_now - opp_filled_now) / (dom_filled_now + opp_filled_now)

            row = {
                "market": market,
                "t_off": f"{tr.t_off:.0f}",
                "idx": str(idx),
                "side": tr.side,
                "outcome": tr.outcome,
                "size": f"{tr.size:.4f}",
                "price": f"{tr.price:.4f}",
                "usdc": f"{tr.usdc:.4f}",
                "score": f"{tick.score:.4f}",
                "dscore": f"{dscore:+.4f}",
                "ofi": f"{tick.ofi:+.4f}",
                "bsi": f"{tick.bsi:+.4f}",
                "cvd": f"{tick.cvd:+.4f}",
                "up_bid": f"{tick.up_bid:.3f}",
                "up_ask": f"{tick.up_ask:.3f}",
                "down_bid": f"{tick.down_bid:.3f}",
                "down_ask": f"{tick.down_ask:.3f}",
                "bid_sum": f"{tick.up_bid + tick.down_bid:.3f}",
                "ask_sum": f"{tick.up_ask + tick.down_ask:.3f}",
                "intent_before": state.intent or "",
                "dom_filled": f"{dom_filled_now:.2f}",
                "opp_filled": f"{opp_filled_now:.2f}",
                "avg_dom": f"{avg_dom_now:.4f}",
                "avg_opp": f"{avg_opp_now:.4f}",
                "avg_sum": f"{avg_sum:.4f}",
                "imbalance": f"{imb:+.3f}",
                "gap_to_prev_trade_s": f"{(tr.t_off - state.last_trade_t):.0f}" if state.last_trade_t is not None else "",
                "candidate_trigger": trigger,
                "candidate_role": role,
                "confidence": conf,
                "predicted_opener": state.predicted_opener or "",
                "opener_rule": state.opener_rule or "",
                "opener_match": "" if idx != 0 else (
                    "match" if state.predicted_opener == tr.outcome else "miss"
                ),
            }
            rows.append(row)
            per_market_rows.append(row)

            update_state(state, tr, trigger)
            state.last_score = tick.score

        # per-market markdown dump
        per_md = OUT_PER_MARKET_DIR / f"{market}.md"
        write_per_market(per_md, market, market_start, per_market_rows)

    OUT_CSV.parent.mkdir(parents=True, exist_ok=True)
    if rows:
        keys = list(rows[0].keys())
        with OUT_CSV.open("w", newline="") as f:
            w = csv.DictWriter(f, fieldnames=keys)
            w.writeheader()
            w.writerows(rows)

    # özet
    from collections import Counter

    by_market = Counter(r["market"] for r in rows)
    by_trigger = Counter(r["candidate_trigger"] for r in rows)
    by_role = Counter(r["candidate_role"] for r in rows)
    by_conf = Counter(r["confidence"] for r in rows)
    print(f"\n[OK] {len(rows)} trades classified across {len(by_market)} markets")
    print(f"  CSV: {OUT_CSV.relative_to(ROOT)}")
    print(f"  per-market: {OUT_PER_MARKET_DIR.relative_to(ROOT)}/")
    print("\n=== Trigger dağılımı ===")
    for k, v in by_trigger.most_common():
        print(f"  {k:24s} {v:4d}")
    print("\n=== Role dağılımı ===")
    for k, v in by_role.most_common():
        print(f"  {k:18s} {v:4d}")
    print("\n=== Confidence ===")
    for k, v in by_conf.most_common():
        print(f"  {k:8s} {v:4d}")

    return 0


def write_per_market(path: Path, market: str, market_start: int, rows: list[dict[str, str]]) -> None:
    if not rows:
        return
    lines = [f"# {market} (start={market_start}, {len(rows)} trades)\n"]
    cols = [
        ("t_off", 4),
        ("side", 4),
        ("outcome", 4),
        ("size", 8),
        ("price", 6),
        ("score", 7),
        ("dscore", 7),
        ("ofi", 7),
        ("bsi", 8),
        ("up_bid", 6),
        ("down_bid", 8),
        ("avg_dom", 7),
        ("avg_sum", 7),
        ("imbalance", 7),
        ("intent_before", 6),
        ("candidate_trigger", 18),
        ("candidate_role", 14),
        ("confidence", 6),
    ]
    header = " | ".join(f"{c:<{w}}" for c, w in cols)
    sep = " | ".join("-" * w for _, w in cols)
    lines.append(f"```\n{header}\n{sep}")
    for r in rows:
        lines.append(" | ".join(f"{r.get(c,''):<{w}}" for c, w in cols))
    lines.append("```\n")

    from collections import Counter

    tr = Counter(r["candidate_trigger"] for r in rows)
    lines.append("## Trigger dağılımı")
    for k, v in tr.most_common():
        lines.append(f"- `{k}`: {v}")
    path.write_text("\n".join(lines))


if __name__ == "__main__":
    raise SystemExit(main())
