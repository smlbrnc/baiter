#!/usr/bin/env python3
"""
Örnek simülasyon: docs/aras.md (Bonereaper / Aras strateji spesifikasyonu).

Tick JSON: her satırda up/down best bid/ask, signal_score, ts_ms.
Epoch dosya adından: btc-updown-5m-<epoch>_ticks.json

Bu script gerçek emir defteri veya fill feed'i yerine basitleştirilmiş modeller kullanır:
- FAK: anında dolar, fill fiyatı = best_ask + tolerans (taker).
- GTC merdiven: bid P'de kalan emir, o tick'te best_ask <= P ise maker fill (o tick'te tam dolma).

Çıktı: faz özeti, pozisyon, tahmini PnL (son kitaba göre kazanan taraf).
"""

from __future__ import annotations

import argparse
import json
import re
import sys
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any, Literal

Outcome = Literal["up", "down"]

# Bölüm 7 defaults
BRACKET_BASE_SIZE = 40.0
LADDER_RUNG_SIZE = 40.0
DIRECTIONAL_BASE_SIZE = 60.0
PAIR_COST_TAKER_GUARD = 1.005
TAKER_PRICE_TOLERANCE = 0.005
LADDER_MIN_GAP = 0.02
DIRECTIONAL_MIN_ABS_TREND = 0.10
DIRECTIONAL_MAX_SKEW = 1500.0
NAKED_DANGER_THRESHOLD = 1000.0
MAX_USDC_EXPOSURE = 2500.0
MAX_NAKED_USD = 500.0
MAX_LOSS_PER_WINDOW = -200.0
DIRECTIONAL_TAKER_SLIPPAGE = 0.01  # Bölüm 5 Faz 3

LADDER_LEVELS = [
    0.45,
    0.40,
    0.35,
    0.30,
    0.25,
    0.20,
    0.17,
    0.15,
    0.13,
    0.10,
    0.07,
    0.05,
    0.03,
    0.02,
    0.01,
]


def parse_epoch_from_path(path: Path) -> int:
    m = re.search(r"btc-updown-5m-(\d+)", path.name)
    if not m:
        raise ValueError(f"Epoch çıkarılamadı: {path}")
    return int(m.group(1))


def t_ep(ts_ms: int, epoch: int) -> float:
    return ts_ms / 1000.0 - float(epoch)


def phase_from_t(t: float) -> str:
    if t < 0:
        return "Idle"
    if t < 30:
        return "TakerBracket"
    if t < 180:
        return "LadderHarvest"
    if t < 280:
        return "DirectionalOverlay"
    if t < 305:
        return "SettlementScoop"
    return "Closed"


def pair_cost(b: dict[str, Any]) -> float:
    return float(b["up_best_ask"]) + float(b["down_best_ask"])


def position_skew(up_sz: float, dn_sz: float) -> float:
    return up_sz - dn_sz


@dataclass
class LadderOrder:
    outcome: Outcome
    price: float
    size: float
    oid: int


@dataclass
class SimState:
    epoch: int
    up_filled: float = 0.0
    up_cost: float = 0.0
    down_filled: float = 0.0
    down_cost: float = 0.0
    ladder_placed: bool = False
    ladder_orders: list[LadderOrder] = field(default_factory=list)
    next_oid: int = 1
    last_stale_ts: float = -1.0
    last_directional_ts: float = -1.0
    directional_cooldown_s: float = 5.0
    halted: bool = False
    halt_reason: str = ""
    events: list[str] = field(default_factory=list)

    def log(self, msg: str) -> None:
        self.events.append(msg)


def pass_hard_guards(book: dict[str, Any], st: SimState, t: float) -> bool:
    if st.halted:
        return False
    # exposure USDC ~ maliyet (basit)
    exposure = st.up_cost + st.down_cost
    if exposure > MAX_USDC_EXPOSURE:
        st.halted = True
        st.halt_reason = "MAX_USDC_EXPOSURE"
        return False
    skew = position_skew(st.up_filled, st.down_filled)
    up_avg = st.up_cost / st.up_filled if st.up_filled > 0 else 0.0
    dn_avg = st.down_cost / st.down_filled if st.down_filled > 0 else 0.0
    naked_up = max(0.0, skew)
    naked_dn = max(0.0, -skew)
    naked_usd = naked_up * up_avg + naked_dn * dn_avg
    if naked_usd > MAX_NAKED_USD * 2:  # simülasyonda gevşek
        st.halted = True
        st.halt_reason = "MAX_NAKED_USD"
        return False
    # tahmini kayıp (hangi taraf kaybederse kötü senaryo)
    worst = min(
        _pnl_if_winner(st, "up"),
        _pnl_if_winner(st, "down"),
    )
    if worst < MAX_LOSS_PER_WINDOW:
        st.halted = True
        st.halt_reason = "MAX_LOSS_PER_WINDOW"
        return False
    return True


def avg_pair_cost(st: SimState) -> float:
    if st.up_filled <= 0 or st.down_filled <= 0:
        return float("nan")
    up_avg = st.up_cost / st.up_filled
    dn_avg = st.down_cost / st.down_filled
    return up_avg + dn_avg


def _pnl_if_winner(st: SimState, winner: Outcome) -> float:
    """aras.md 2.3 guaranteed + directional."""
    n_pairs = min(st.up_filled, st.down_filled)
    apc = avg_pair_cost(st)
    if apc != apc or n_pairs <= 0:
        g = 0.0
    else:
        g = n_pairs * (1.0 - apc)
    up_avg = st.up_cost / st.up_filled if st.up_filled > 0 else 0.0
    dn_avg = st.down_cost / st.down_filled if st.down_filled > 0 else 0.0
    nu = max(0.0, st.up_filled - st.down_filled)
    nd = max(0.0, st.down_filled - st.up_filled)
    if winner == "up":
        d_pnl = nu * (1.0 - up_avg) - nd * dn_avg
    else:
        d_pnl = nd * (1.0 - dn_avg) - nu * up_avg
    return g + d_pnl


def infer_winner_from_book(book: dict[str, Any]) -> Outcome:
    ua = float(book["up_best_ask"])
    da = float(book["down_best_ask"])
    # çözünürlük civarı: kazanan ~1, kaybeden ~0
    if ua >= 0.9 and da <= 0.15:
        return "up"
    if da >= 0.9 and ua <= 0.15:
        return "down"
    # belirsiz: bid'lere bak
    ub = float(book["up_best_bid"])
    db = float(book["down_best_bid"])
    return "up" if ub >= db else "down"


def should_take_bracket(book: dict[str, Any], st: SimState, t: float) -> bool:
    if t < 0 or t >= 30:
        return False
    if float(book["up_best_ask"]) <= 0 or float(book["down_best_ask"]) <= 0:
        return False
    if pair_cost(book) > PAIR_COST_TAKER_GUARD:
        return False
    if st.up_filled >= BRACKET_BASE_SIZE and st.down_filled >= BRACKET_BASE_SIZE:
        return False
    sc = float(book.get("signal_score", 5.0))
    if not (2.5 <= sc <= 7.5):
        return False
    return True


def place_taker_bracket(book: dict[str, Any], st: SimState, t: float) -> None:
    """FAK: eksik tarafı bir seferde doldur (40'a kadar)."""
    up_ask = float(book["up_best_ask"])
    dn_ask = float(book["down_best_ask"])
    if st.up_filled < BRACKET_BASE_SIZE:
        sz = min(BRACKET_BASE_SIZE - st.up_filled, BRACKET_BASE_SIZE)
        fill = up_ask + TAKER_PRICE_TOLERANCE
        st.up_filled += sz
        st.up_cost += sz * fill
        st.log(f"t={t:.1f}s FAK BUY UP {sz} @ {fill:.4f} (bracket)")
    if st.down_filled < BRACKET_BASE_SIZE:
        sz = min(BRACKET_BASE_SIZE - st.down_filled, BRACKET_BASE_SIZE)
        fill = dn_ask + TAKER_PRICE_TOLERANCE
        st.down_filled += sz
        st.down_cost += sz * fill
        st.log(f"t={t:.1f}s FAK BUY DOWN {sz} @ {fill:.4f} (bracket)")


def build_ladder_orders(book: dict[str, Any], st: SimState) -> None:
    up_ask = float(book["up_best_ask"])
    dn_ask = float(book["down_best_ask"])
    for level in LADDER_LEVELS:
        if up_ask > 0 and level < up_ask - LADDER_MIN_GAP:
            st.ladder_orders.append(
                LadderOrder("up", level, LADDER_RUNG_SIZE, st.next_oid)
            )
            st.next_oid += 1
        if dn_ask > 0 and level < dn_ask - LADDER_MIN_GAP:
            st.ladder_orders.append(
                LadderOrder("down", level, LADDER_RUNG_SIZE, st.next_oid)
            )
            st.next_oid += 1
    st.ladder_placed = True
    st.log(f"Merdiven kuruldu: {len(st.ladder_orders)} GTC emir")


def refresh_stale_ladder(book: dict[str, Any], st: SimState, t: float) -> None:
    """Her 10s: spec'teki stale iptal (fiyat uçmuşsa)."""
    if st.last_stale_ts < 0:
        st.last_stale_ts = t
        return
    if t - st.last_stale_ts < 10.0:
        return
    st.last_stale_ts = t
    up_ask = float(book["up_best_ask"])
    dn_ask = float(book["down_best_ask"])
    kept: list[LadderOrder] = []
    for o in st.ladder_orders:
        if o.outcome == "up":
            opp = dn_ask
        else:
            opp = up_ask
        if abs(o.price - opp) > 0.40:
            st.log(f"t={t:.1f}s stale cancel ladder {o.outcome} @ {o.price:.2f}")
            continue
        kept.append(o)
    st.ladder_orders = kept


def try_ladder_fills(book: dict[str, Any], st: SimState, t: float) -> None:
    """Maker: best_ask limit seviyemize düştüğünde fill."""
    up_ask = float(book["up_best_ask"])
    dn_ask = float(book["down_best_ask"])
    remaining: list[LadderOrder] = []
    for o in st.ladder_orders:
        if o.size <= 0:
            continue
        ask = up_ask if o.outcome == "up" else dn_ask
        # Ask bizim bid'e kadar düştü veya altına (satıcılar bize çarpıyor)
        if ask > 0 and ask <= o.price:
            fill_px = min(o.price, ask)
            st.log(f"t={t:.1f}s GTC fill {o.outcome.upper()} {o.size} @ {fill_px:.4f} (ladder)")
            if o.outcome == "up":
                st.up_filled += o.size
                st.up_cost += o.size * fill_px
            else:
                st.down_filled += o.size
                st.down_cost += o.size * fill_px
            continue
        remaining.append(o)
    st.ladder_orders = remaining


def needs_directional(book: dict[str, Any], st: SimState, t: float) -> Outcome | None:
    if t < 180 or t >= 280:
        return None
    trend = float(book["up_best_ask"]) - 0.5
    if abs(trend) < DIRECTIONAL_MIN_ABS_TREND:
        return None
    skew = position_skew(st.up_filled, st.down_filled)
    if abs(skew) >= DIRECTIONAL_MAX_SKEW:
        return None
    naked_dir: Outcome = "up" if skew > 0 else "down"
    book_dir: Outcome = "up" if trend > 0 else "down"
    if naked_dir != book_dir:
        return None
    return book_dir


def aggression(book: dict[str, Any]) -> float:
    trend = abs(float(book["up_best_ask"]) - 0.5)
    a = (trend - DIRECTIONAL_MIN_ABS_TREND) / 0.40
    return max(0.3, min(1.0, a))


def place_directional(book: dict[str, Any], st: SimState, t: float, direction: Outcome) -> None:
    if st.last_directional_ts >= 0 and t - st.last_directional_ts < st.directional_cooldown_s:
        return
    agg = aggression(book)
    sz = DIRECTIONAL_BASE_SIZE * agg
    if direction == "up":
        ask = float(book["up_best_ask"])
        fill = ask + DIRECTIONAL_TAKER_SLIPPAGE
        st.up_filled += sz
        st.up_cost += sz * fill
        st.log(f"t={t:.1f}s FAK BUY UP {sz:.1f} @ {fill:.4f} (directional agg={agg:.2f})")
    else:
        ask = float(book["down_best_ask"])
        fill = ask + DIRECTIONAL_TAKER_SLIPPAGE
        st.down_filled += sz
        st.down_cost += sz * fill
        st.log(f"t={t:.1f}s FAK BUY DOWN {sz:.1f} @ {fill:.4f} (directional agg={agg:.2f})")
    st.last_directional_ts = t


def directional_safety_check(book: dict[str, Any], st: SimState, t: float) -> bool:
    """Bölüm 6.4 — geçiş penceresi."""
    if t < 175.0 or t > 200.0:
        return True
    skew = position_skew(st.up_filled, st.down_filled)
    book_dir = float(book["up_best_ask"]) - 0.5
    if skew > NAKED_DANGER_THRESHOLD and book_dir < -0.10:
        return False
    if skew < -NAKED_DANGER_THRESHOLD and book_dir > 0.10:
        return False
    return True


def run_ticks(ticks: list[dict[str, Any]], epoch: int) -> SimState:
    st = SimState(epoch=epoch)
    for book in ticks:
        ts_ms = int(book["ts_ms"])
        t = t_ep(ts_ms, epoch)
        ph = phase_from_t(t)

        if not pass_hard_guards(book, st, t):
            break
        if not directional_safety_check(book, st, t):
            st.log(f"t={t:.1f}s directional_safety_check fail (ladder risk freeze)")
            # dondurma: yeni yönlü ve merdiven fill'ini durdurmadan sadece uyarı — spec "halt ladder side"
            pass

        if ph == "TakerBracket" and should_take_bracket(book, st, t):
            place_taker_bracket(book, st, t)

        elif ph == "LadderHarvest":
            if not st.ladder_placed:
                build_ladder_orders(book, st)
            refresh_stale_ladder(book, st, t)
            try_ladder_fills(book, st, t)

        elif ph == "DirectionalOverlay":
            refresh_stale_ladder(book, st, t)
            try_ladder_fills(book, st, t)
            if directional_safety_check(book, st, t):
                d = needs_directional(book, st, t)
                if d is not None:
                    place_directional(book, st, t, d)

        elif ph == "SettlementScoop":
            refresh_stale_ladder(book, st, t)
            try_ladder_fills(book, st, t)

        prev_t = t

    return st


def load_ticks(path: Path) -> tuple[int, list[dict[str, Any]]]:
    epoch = parse_epoch_from_path(path)
    raw = path.read_text(encoding="utf-8")
    data = json.loads(raw)
    if not isinstance(data, list):
        raise ValueError("Tick dosyası JSON array bekleniyor")
    return epoch, data


def main() -> None:
    ap = argparse.ArgumentParser(description="Aras (Bonereaper) strateji tick simülasyonu")
    ap.add_argument(
        "ticks_json",
        nargs="+",
        type=Path,
        help="btc-updown-5m-*_ticks.json dosyaları",
    )
    args = ap.parse_args()

    for path in args.ticks_json:
        if not path.is_file():
            print(f"SKIP (yok): {path}", file=sys.stderr)
            continue
        epoch, ticks = load_ticks(path)
        st = run_ticks(ticks, epoch)
        last = ticks[-1]
        winner = infer_winner_from_book(last)
        pnl = _pnl_if_winner(st, winner)

        print("=" * 72)
        print(path.name)
        print(f"  epoch={epoch}  ticks={len(ticks)}  inferred_winner={winner.upper()}")
        print(
            f"  UP:   size={st.up_filled:.2f}  avg={st.up_cost/st.up_filled if st.up_filled else 0:.4f}"
        )
        print(
            f"  DOWN: size={st.down_filled:.2f}  avg={st.down_cost/st.down_filled if st.down_filled else 0:.4f}"
        )
        print(f"  skew={position_skew(st.up_filled, st.down_filled):.2f}  halted={st.halted} {st.halt_reason!r}")
        apc = avg_pair_cost(st)
        print(f"  avg_pair_cost={apc:.4f}" if apc == apc else "  avg_pair_cost=n/a")
        print(f"  est_pnl_if_{winner}=${pnl:.2f}")
        if st.events:
            print("  --- son olaylar (max 12) ---")
            for e in st.events[-12:]:
                print(f"    {e}")


if __name__ == "__main__":
    main()
