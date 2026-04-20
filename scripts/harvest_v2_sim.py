#!/usr/bin/env python3
"""harvest_v2_sim.py — Harvest v2 stratejisini bot 40'ın gerçek market_ticks
verisi üzerinde simüle eder ve session bazında PnL tablosu üretir.

Üç varyant:
  (a) default        : docs/harvest-v2.md §5–§13 tam spec
  (b) opposite_pyr=N : §16 'opposite_pyramid_enabled=false' önerisi
  (c) failover=Y     : DeepTrade'de fill yoksa NormalTrade'de cancel +
                       tek-taraflı (rising) bid → ardından standard hedge

PnL hesabı Polymarket binary outcome standardı:
  - cost basis  = Σ buy_price × size
  - realized    = Σ winning_shares × $1 − cost  (yalnız market resolve oldu)
  - unrealized  = Σ shares × current_mid − cost (açık pozisyon)
  - no_position : cost == 0 → PnL = 0 (sunk cost yok)

Resolve heuristic (ε = 0.05): son tickte yes_bid > 0.55 → UP win,
yes_bid < 0.45 → DOWN win, else → 'pending' (sadece unrealized rapor).

Çıktılar:
  - stdout: özet + 3 varyant karşılaştırma tablosu
  - data/harvest_v2_sim_per_session.csv  (varyant kolonları yan yana)
  - data/harvest_v2_sim_events.csv       (her tetiklenen aksiyon)
"""

from __future__ import annotations

import csv
import math
import sqlite3
from dataclasses import dataclass, field
from pathlib import Path
from typing import Dict, List, Optional, Tuple

DB_PATH = Path(__file__).resolve().parents[1] / "data" / "baiter.db"
OUT_DIR = Path(__file__).resolve().parents[1] / "data"

BOT_ID = 40

AVG_THRESHOLD = 0.98
ORDER_USDC = 5.0
COOLDOWN_MS = 30_000
MIN_PRICE = 0.05
MAX_PRICE = 0.95
TICK_SIZE = 0.01
API_MIN_ORDER_SIZE = 5.0
SIGNAL_WEIGHT = 10.0
RESOLVE_EPS = 0.05  # yes_bid bandının dışı → resolved sayılır

STATUS_NO_POS = "no_pos"
STATUS_OPEN_UNR = "open_unr"
STATUS_OPEN_RES = "open_res"
STATUS_PAIR_UNR = "pair_unr"
STATUS_PAIR_RES = "pair_res"


def snap(price: float) -> float:
    return round(round(price / TICK_SIZE) * TICK_SIZE, 4)


def clamp(v: float, lo: float, hi: float) -> float:
    return max(lo, min(hi, v))


def zone(now_ms: int, open_ms: int, close_ms: int) -> str:
    if close_ms <= open_ms:
        return "DeepTrade"
    pct = (now_ms - open_ms) / (close_ms - open_ms)
    if pct < 0.10:
        return "DeepTrade"
    if pct < 0.50:
        return "NormalTrade"
    if pct < 0.90:
        return "AggTrade"
    if pct < 0.97:
        return "FakTrade"
    return "StopTrade"


def order_size(price: float) -> float:
    if price <= 0:
        return API_MIN_ORDER_SIZE
    return max(math.ceil(ORDER_USDC / price), API_MIN_ORDER_SIZE)


@dataclass
class Tick:
    ts_ms: int
    yes_bid: float
    yes_ask: float
    no_bid: float
    no_ask: float
    score: float


@dataclass
class OpenOrder:
    side: str       # "Up" | "Down"
    price: float
    size: float
    placed_ms: int
    role: str       # "open" | "hedge" | "avg_down" | "pyramid" | "single_leg_open"


@dataclass
class SimResult:
    session_id: int
    ticks: int
    opened: bool = False
    open_side: Optional[str] = None
    avg_down_count: int = 0
    pyramid_count: int = 0
    hedge_replaces: int = 0
    failover_triggered: bool = False
    pair_complete: bool = False
    final_zone: str = ""
    resolved: bool = False
    win_side: Optional[str] = None       # "Up" | "Down" | None (pending)
    shares_yes: float = 0.0
    shares_no: float = 0.0
    avg_yes: float = 0.0
    avg_no: float = 0.0
    cost: float = 0.0                    # Σ size×price (cost basis)
    pair_count: float = 0.0
    pair_avg_cost: float = 0.0           # raporlama için (doc §3 formülü ≠ PnL)
    mark_value: float = 0.0
    resolved_value: float = 0.0
    pnl_unrealized: float = 0.0          # mark - cost (açık pozisyonda anlamlı)
    pnl_realized: float = 0.0            # resolved_value - cost (sadece resolved)
    pnl_final: float = 0.0               # resolved varsa realized, yoksa unrealized
    status: str = STATUS_NO_POS
    events: List[Tuple[int, str]] = field(default_factory=list)


def fetch_sessions(conn: sqlite3.Connection) -> List[Tuple[int, int, int, str]]:
    cur = conn.execute(
        "SELECT id, start_ts*1000, end_ts*1000, slug "
        "FROM market_sessions WHERE bot_id=? ORDER BY id",
        (BOT_ID,),
    )
    return cur.fetchall()


def fetch_ticks(conn: sqlite3.Connection, session_id: int) -> List[Tick]:
    cur = conn.execute(
        "SELECT ts_ms, yes_best_bid, yes_best_ask, no_best_bid, no_best_ask, signal_score "
        "FROM market_ticks WHERE bot_id=? AND market_session_id=? ORDER BY ts_ms",
        (BOT_ID, session_id),
    )
    return [Tick(*row) for row in cur.fetchall()]


def passive_fill(order: OpenOrder, t: Tick) -> Optional[float]:
    """Bot her zaman BUY. Buy taker eşiği = ask. Limit ≥ ask → fill@order.price (maker)."""
    ask = t.yes_ask if order.side == "Up" else t.no_ask
    if ask > 0 and order.price >= ask:
        return order.price
    return None


def simulate(
    session_id: int,
    open_ms: int,
    close_ms: int,
    ticks: List[Tick],
    opposite_pyramid: bool = True,
    single_leg_failover: bool = False,
) -> SimResult:
    res = SimResult(session_id=session_id, ticks=len(ticks))
    if not ticks:
        return res

    state = "Pending"
    filled_side: Optional[str] = None
    open_orders: List[OpenOrder] = []
    hedge_order: Optional[OpenOrder] = None
    last_avg_ms = 0
    last_fill_price: Dict[str, float] = {"Up": 0.0, "Down": 0.0}
    avg_side: Dict[str, float] = {"Up": 0.0, "Down": 0.0}
    shares: Dict[str, float] = {"Up": 0.0, "Down": 0.0}
    cost = 0.0
    prev_zone = "DeepTrade"

    def vwap_update(side: str, price: float, size: float):
        old_q = shares[side]
        old_v = avg_side[side] * old_q
        new_q = old_q + size
        avg_side[side] = (old_v + price * size) / new_q if new_q > 0 else 0
        shares[side] = new_q
        last_fill_price[side] = price

    def record_fill(side: str, price: float, size: float, ts_ms: int, role: str):
        nonlocal cost, filled_side, state, hedge_order
        cost += size * price
        vwap_update(side, price, size)
        res.events.append((ts_ms, f"FILL {role} {side} {size:.0f}@{price:.3f}"))

        if state in ("OpenPair", "OpenPair_Retry") and role in ("open", "single_leg_open"):
            filled_side = side
            state = "PositionOpen"
            # Failover akışında hedge yok → fill anında konur (Soru cevabı = B)
            if role == "single_leg_open":
                hp_raw = AVG_THRESHOLD - avg_side[filled_side]
                hp = clamp(snap(hp_raw), MIN_PRICE, MAX_PRICE)
                if MIN_PRICE <= hp <= MAX_PRICE:
                    h_side = "Down" if filled_side == "Up" else "Up"
                    sz = order_size(hp)
                    hedge_order = OpenOrder(h_side, hp, sz, ts_ms, "hedge")
                    res.events.append((ts_ms, f"FAILOVER_HEDGE {h_side}@{hp:.3f}({sz:.0f})"))

        # Pair complete (imbalance < min order)
        if shares["Up"] > 0 and shares["Down"] > 0 and abs(shares["Up"] - shares["Down"]) < API_MIN_ORDER_SIZE:
            state = "PairComplete"
            res.pair_complete = True

    def fire_failover(t: Tick):
        nonlocal open_orders, hedge_order, state, last_avg_ms
        # cancel mevcut OpenPair emirleri
        for o in open_orders:
            res.events.append((t.ts_ms, f"FAILOVER_CANCEL {o.role} {o.side}"))
        open_orders = []
        if hedge_order is not None:
            res.events.append((t.ts_ms, f"FAILOVER_CANCEL {hedge_order.role} {hedge_order.side}"))
            hedge_order = None
        # rising tarafı belirle
        if t.yes_bid > 0.5 + RESOLVE_EPS:
            rising = "Up"
            price = snap(t.yes_bid)
        elif t.yes_bid < 0.5 - RESOLVE_EPS:
            rising = "Down"
            price = snap(t.no_bid)
        else:
            res.events.append((t.ts_ms, f"FAILOVER_SKIP yes_bid={t.yes_bid:.3f}"))
            state = "Pending"
            return
        if not (MIN_PRICE <= price <= MAX_PRICE) or price <= 0:
            res.events.append((t.ts_ms, f"FAILOVER_SKIP_PRICE {rising}@{price:.3f}"))
            state = "Pending"
            return
        sz = order_size(price)
        open_orders.append(OpenOrder(rising, price, sz, t.ts_ms, "single_leg_open"))
        state = "OpenPair_Retry"
        res.failover_triggered = True
        last_avg_ms = t.ts_ms
        res.events.append((t.ts_ms, f"FAILOVER_OPEN {rising}@{price:.3f}({sz:.0f})"))

    for t in ticks:
        z = zone(t.ts_ms, open_ms, close_ms)
        res.final_zone = z

        # 0. Failover tetik: DeepTrade → NormalTrade ve hâlâ fill yok
        if (single_leg_failover
                and prev_zone == "DeepTrade" and z == "NormalTrade"
                and state == "OpenPair" and filled_side is None and cost == 0):
            fire_failover(t)
        prev_zone = z

        # 1. StopTrade
        if z == "StopTrade":
            if open_orders or hedge_order:
                res.events.append((t.ts_ms, f"STOP cancel_all open={len(open_orders)} hedge={bool(hedge_order)}"))
            open_orders = []
            hedge_order = None
            if state in ("OpenPair", "OpenPair_Retry", "PositionOpen", "HedgeUpdating"):
                state = "Done"
            continue

        # 2. Pasif fill kontrolü
        if hedge_order is not None:
            fp = passive_fill(hedge_order, t)
            if fp is not None:
                record_fill(hedge_order.side, hedge_order.price, hedge_order.size, t.ts_ms, "hedge")
                hedge_order = None

        new_open: List[OpenOrder] = []
        for o in open_orders:
            fp = passive_fill(o, t)
            if fp is not None:
                record_fill(o.side, o.price, o.size, t.ts_ms, o.role)
                # avg/pyramid sonrası hedge re-place
                if o.role in ("avg_down", "pyramid") and state == "PositionOpen" and not res.pair_complete:
                    new_hp = clamp(snap(AVG_THRESHOLD - avg_side[filled_side]), MIN_PRICE, MAX_PRICE)
                    if hedge_order is not None:
                        res.hedge_replaces += 1
                        hedge_order = None
                    imb = abs(shares["Up"] - shares["Down"])
                    if imb >= API_MIN_ORDER_SIZE and MIN_PRICE <= new_hp <= MAX_PRICE:
                        h_side = "Down" if filled_side == "Up" else "Up"
                        hedge_order = OpenOrder(h_side, new_hp, imb, t.ts_ms, "hedge")
                        res.events.append((t.ts_ms, f"HEDGE_REPLACE {h_side} {imb:.0f}@{new_hp:.3f}"))
            else:
                if t.ts_ms - o.placed_ms >= COOLDOWN_MS:
                    res.events.append((t.ts_ms, f"CANCEL stale {o.role} {o.side}"))
                    continue
                new_open.append(o)
        open_orders = new_open

        # 3. State machine kararları
        if state == "Pending":
            if t.yes_bid > 0 and t.no_bid > 0:
                yes_spread = max(0.0, t.yes_ask - t.yes_bid)
                no_spread = max(0.0, t.no_ask - t.no_bid)
                if SIGNAL_WEIGHT == 0 or abs(t.score - 5.0) < 1e-9:
                    open_side = "Up"
                    open_price_raw = t.yes_bid
                elif t.score > 5.0:
                    open_side = "Up"
                    open_price_raw = t.yes_ask + (t.score - 5.0) / 5.0 * yes_spread
                else:
                    open_side = "Down"
                    open_price_raw = t.no_ask + (5.0 - t.score) / 5.0 * no_spread
                op = clamp(snap(open_price_raw), MIN_PRICE, MAX_PRICE)
                hp = clamp(snap(AVG_THRESHOLD - op), MIN_PRICE, MAX_PRICE)
                h_side = "Down" if open_side == "Up" else "Up"
                op_size = order_size(op)
                hp_size = order_size(hp)
                opener = OpenOrder(open_side, op, op_size, t.ts_ms, "open")
                hedger = OpenOrder(h_side, hp, hp_size, t.ts_ms, "hedge")
                res.events.append((t.ts_ms, f"OPENPAIR open={open_side}@{op:.3f}({op_size:.0f}) hedge={h_side}@{hp:.3f}({hp_size:.0f}) score={t.score:.2f}"))
                state = "OpenPair"
                res.opened = True
                res.open_side = open_side
                if (fp := passive_fill(opener, t)) is not None:
                    record_fill(open_side, op, op_size, t.ts_ms, "open")
                else:
                    open_orders.append(opener)
                if (fp := passive_fill(hedger, t)) is not None:
                    record_fill(h_side, hp, hp_size, t.ts_ms, "hedge")
                else:
                    hedge_order = hedger

        elif state == "OpenPair_Retry" and filled_side is None:
            # Failover: fill yoksa cooldown sonrası re-place
            has_sl = any(o.role == "single_leg_open" for o in open_orders)
            cooldown_ok = t.ts_ms - last_avg_ms >= COOLDOWN_MS
            if not has_sl and cooldown_ok and z in ("NormalTrade", "AggTrade", "FakTrade"):
                if t.yes_bid > 0.5 + RESOLVE_EPS:
                    rising = "Up"
                    price = snap(t.yes_bid)
                elif t.yes_bid < 0.5 - RESOLVE_EPS:
                    rising = "Down"
                    price = snap(t.no_bid)
                else:
                    rising = None
                if rising and MIN_PRICE <= price <= MAX_PRICE and price > 0:
                    sz = order_size(price)
                    open_orders.append(OpenOrder(rising, price, sz, t.ts_ms, "single_leg_open"))
                    last_avg_ms = t.ts_ms
                    res.events.append((t.ts_ms, f"FAILOVER_REPLACE {rising}@{price:.3f}({sz:.0f})"))

        elif state == "PositionOpen" and filled_side is not None:
            if z == "NormalTrade":
                ask_side = t.yes_ask if filled_side == "Up" else t.no_ask
                bid_side = t.yes_bid if filled_side == "Up" else t.no_bid
                cooldown_ok = t.ts_ms - last_avg_ms >= COOLDOWN_MS
                avg_open = any(o.role == "avg_down" and o.side == filled_side for o in open_orders)
                if (cooldown_ok and not avg_open
                        and ask_side > 0 and ask_side < avg_side[filled_side]
                        and MIN_PRICE <= bid_side <= MAX_PRICE):
                    price = snap(bid_side)
                    sz = order_size(price)
                    open_orders.append(OpenOrder(filled_side, price, sz, t.ts_ms, "avg_down"))
                    last_avg_ms = t.ts_ms
                    res.avg_down_count += 1
                    res.events.append((t.ts_ms, f"AVG_DOWN {filled_side}@{price:.3f}({sz:.0f}) avg={avg_side[filled_side]:.3f}"))
            elif z in ("AggTrade", "FakTrade"):
                rising = "Up" if t.yes_bid > 0.5 else "Down"
                cooldown_ok = t.ts_ms - last_avg_ms >= COOLDOWN_MS
                pyr_open = any(o.role == "pyramid" and o.side == rising for o in open_orders)
                if cooldown_ok and not pyr_open and (opposite_pyramid or rising == filled_side):
                    ask_rising = t.yes_ask if rising == "Up" else t.no_ask
                    spread_rising = max(0.0, (t.yes_ask - t.yes_bid) if rising == "Up" else (t.no_ask - t.no_bid))
                    delta = abs((t.score - 5.0) / 5.0 * spread_rising) if SIGNAL_WEIGHT > 0 else 0.0
                    if rising == filled_side:
                        trend_ok = ask_rising > last_fill_price[rising]
                    else:
                        trend_ok = ask_rising > 0
                    if trend_ok and ask_rising > 0:
                        price = clamp(snap(ask_rising + delta), MIN_PRICE, MAX_PRICE)
                        sz = order_size(price)
                        open_orders.append(OpenOrder(rising, price, sz, t.ts_ms, "pyramid"))
                        last_avg_ms = t.ts_ms
                        res.pyramid_count += 1
                        res.events.append((t.ts_ms, f"PYRAMID {rising}@{price:.3f}({sz:.0f})"))

    # 4. Final değerleme — Polymarket PnL
    last = ticks[-1]
    res.shares_yes = shares["Up"]
    res.shares_no = shares["Down"]
    res.avg_yes = avg_side["Up"]
    res.avg_no = avg_side["Down"]
    res.cost = cost
    pc = min(shares["Up"], shares["Down"])
    res.pair_count = pc
    if pc > 0:
        res.pair_avg_cost = (avg_side["Up"] + avg_side["Down"]) / 2.0

    # mark-to-market
    yes_mid = (last.yes_bid + last.yes_ask) / 2.0 if last.yes_ask > 0 else last.yes_bid
    no_mid = (last.no_bid + last.no_ask) / 2.0 if last.no_ask > 0 else last.no_bid
    res.mark_value = shares["Up"] * yes_mid + shares["Down"] * no_mid

    # Resolve heuristic (yes_bid bandının dışı)
    if last.yes_bid > 0.5 + RESOLVE_EPS:
        res.resolved = True
        res.win_side = "Up"
        res.resolved_value = shares["Up"] * 1.0
    elif last.yes_bid < 0.5 - RESOLVE_EPS:
        res.resolved = True
        res.win_side = "Down"
        res.resolved_value = shares["Down"] * 1.0
    else:
        res.resolved = False
        res.win_side = None
        res.resolved_value = 0.0

    # PnL
    res.pnl_unrealized = res.mark_value - cost
    res.pnl_realized = (res.resolved_value - cost) if res.resolved else 0.0

    # Status + final
    if cost == 0:
        res.status = STATUS_NO_POS
        res.pnl_final = 0.0
    elif res.pair_complete:
        res.status = STATUS_PAIR_RES if res.resolved else STATUS_PAIR_UNR
        res.pnl_final = res.pnl_realized if res.resolved else res.pnl_unrealized
    else:
        res.status = STATUS_OPEN_RES if res.resolved else STATUS_OPEN_UNR
        res.pnl_final = res.pnl_realized if res.resolved else res.pnl_unrealized

    return res


def fmt_pnl(v: float) -> str:
    return f"{v:+8.3f}"


def main() -> None:
    OUT_DIR.mkdir(parents=True, exist_ok=True)
    conn = sqlite3.connect(DB_PATH)
    sessions = fetch_sessions(conn)
    print(f"Bot 40 sessions: {len(sessions)}\n")

    a_results: List[SimResult] = []   # default
    b_results: List[SimResult] = []   # opposite_pyramid=False
    c_results: List[SimResult] = []   # single_leg_failover=True
    for sid, open_ms, close_ms, _ in sessions:
        ticks = fetch_ticks(conn, sid)
        a_results.append(simulate(sid, open_ms, close_ms, ticks, True, False))
        b_results.append(simulate(sid, open_ms, close_ms, ticks, False, False))
        c_results.append(simulate(sid, open_ms, close_ms, ticks, True, True))

    # CSV: per session, üç varyant yan yana
    per_csv = OUT_DIR / "harvest_v2_sim_per_session.csv"
    with per_csv.open("w", newline="") as f:
        w = csv.writer(f)
        w.writerow([
            "session_id", "ticks", "final_zone", "resolved", "win_side",
            # default (a)
            "a_status", "a_opened", "a_failover", "a_avgD", "a_pyr", "a_hRpl",
            "a_pair", "a_cost", "a_shares_yes", "a_shares_no",
            "a_avg_yes", "a_avg_no", "a_mark", "a_pnl_unr", "a_pnl_real", "a_pnl_final",
            # b: opposite_pyramid=False
            "b_status", "b_pair", "b_cost", "b_pnl_final",
            # c: failover=True
            "c_status", "c_failover", "c_pair", "c_cost", "c_pnl_final",
        ])
        for a, b, c in zip(a_results, b_results, c_results):
            w.writerow([
                a.session_id, a.ticks, a.final_zone, int(a.resolved), a.win_side or "",
                a.status, int(a.opened), int(a.failover_triggered),
                a.avg_down_count, a.pyramid_count, a.hedge_replaces,
                int(a.pair_complete),
                f"{a.cost:.4f}", f"{a.shares_yes:.2f}", f"{a.shares_no:.2f}",
                f"{a.avg_yes:.4f}", f"{a.avg_no:.4f}",
                f"{a.mark_value:.4f}", f"{a.pnl_unrealized:.4f}",
                f"{a.pnl_realized:.4f}", f"{a.pnl_final:.4f}",
                b.status, int(b.pair_complete), f"{b.cost:.4f}", f"{b.pnl_final:.4f}",
                c.status, int(c.failover_triggered), int(c.pair_complete),
                f"{c.cost:.4f}", f"{c.pnl_final:.4f}",
            ])

    # CSV: events (3 varyantı bir arada, varyant prefix'iyle)
    ev_csv = OUT_DIR / "harvest_v2_sim_events.csv"
    with ev_csv.open("w", newline="") as f:
        w = csv.writer(f)
        w.writerow(["variant", "session_id", "ts_ms", "event"])
        for variant, lst in (("a", a_results), ("b", b_results), ("c", c_results)):
            for r in lst:
                for ts, ev in r.events:
                    w.writerow([variant, r.session_id, ts, ev])

    # Stdout: tek tablo (default varyantı)
    print("Default (a) — docs/harvest-v2.md spec")
    print(f"{'sid':>4} {'zone':>10} {'res':>4} {'win':>4} {'stat':>10} {'fo':>3} {'pair':>4} "
          f"{'cost':>7} {'unr':>9} {'real':>9} {'final':>9}")
    print("-" * 95)
    for r in a_results:
        print(f"{r.session_id:>4} {r.final_zone:>10} {('Y' if r.resolved else 'N'):>4} "
              f"{(r.win_side or '-'):>4} {r.status:>10} "
              f"{('Y' if r.failover_triggered else '-'):>3} "
              f"{('Y' if r.pair_complete else 'N'):>4} {r.cost:>7.2f} "
              f"{fmt_pnl(r.pnl_unrealized)} {fmt_pnl(r.pnl_realized)} {fmt_pnl(r.pnl_final)}")
    print("-" * 95)

    def agg(lst: List[SimResult]) -> dict:
        return {
            "n": len(lst),
            "no_pos": sum(1 for r in lst if r.status == STATUS_NO_POS),
            "open_unr": sum(1 for r in lst if r.status == STATUS_OPEN_UNR),
            "open_res": sum(1 for r in lst if r.status == STATUS_OPEN_RES),
            "pair_unr": sum(1 for r in lst if r.status == STATUS_PAIR_UNR),
            "pair_res": sum(1 for r in lst if r.status == STATUS_PAIR_RES),
            "failovers": sum(1 for r in lst if r.failover_triggered),
            "cost": sum(r.cost for r in lst),
            "pnl_unr": sum(r.pnl_unrealized for r in lst),
            "pnl_real": sum(r.pnl_realized for r in lst),
            "pnl_final": sum(r.pnl_final for r in lst),
        }

    aA = agg(a_results); aB = agg(b_results); aC = agg(c_results)

    print()
    print(f"{'metric':>25} {'(a) default':>14} {'(b) no_opp_pyr':>16} {'(c) failover':>14}")
    print("-" * 75)
    rows = [
        ("sessions",          f"{aA['n']}", f"{aB['n']}", f"{aC['n']}"),
        ("status: no_pos",    f"{aA['no_pos']}", f"{aB['no_pos']}", f"{aC['no_pos']}"),
        ("status: open_unr",  f"{aA['open_unr']}", f"{aB['open_unr']}", f"{aC['open_unr']}"),
        ("status: open_res",  f"{aA['open_res']}", f"{aB['open_res']}", f"{aC['open_res']}"),
        ("status: pair_unr",  f"{aA['pair_unr']}", f"{aB['pair_unr']}", f"{aC['pair_unr']}"),
        ("status: pair_res",  f"{aA['pair_res']}", f"{aB['pair_res']}", f"{aC['pair_res']}"),
        ("failovers tetiklendi", f"{aA['failovers']}", f"{aB['failovers']}", f"{aC['failovers']}"),
        ("cost basis (USDC)",  f"{aA['cost']:.2f}", f"{aB['cost']:.2f}", f"{aC['cost']:.2f}"),
        ("Σ pnl_unrealized",   f"{aA['pnl_unr']:+.3f}", f"{aB['pnl_unr']:+.3f}", f"{aC['pnl_unr']:+.3f}"),
        ("Σ pnl_realized",     f"{aA['pnl_real']:+.3f}", f"{aB['pnl_real']:+.3f}", f"{aC['pnl_real']:+.3f}"),
        ("Σ pnl_final",        f"{aA['pnl_final']:+.3f}", f"{aB['pnl_final']:+.3f}", f"{aC['pnl_final']:+.3f}"),
    ]
    for k, va, vb, vc in rows:
        print(f"{k:>25} {va:>14} {vb:>16} {vc:>14}")

    print()
    print("PnL standardı: Polymarket binary outcome")
    print("  cost      = Σ buy_price × size")
    print("  realized  = Σ winning_shares × $1 − cost   (yalnız resolved markets)")
    print("  unrealized= Σ shares × current_mid − cost  (açık pozisyon)")
    print("  no_pos    = cost == 0 → pnl_final = 0     (sunk cost yok)")
    print(f"\nDetails: {per_csv}")
    print(f"Events:  {ev_csv}")


if __name__ == "__main__":
    main()
