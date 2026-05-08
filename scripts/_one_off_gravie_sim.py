"""Gravie strategy simülasyonu — Bot 71 (hudme.com) tick verileriyle.

Bot 71: btc-updown-5m, dryrun, Bonereaper, 10 USDC/order.
34 session, gerçek tick + winning_outcome verisiyle Gravie davranışı emule edilir.
src/strategy/gravie.rs sabitleri birebir kopyalanır.

Çıktı: data/bot71_gravie_sim.json
"""

from __future__ import annotations

import json
import math
import time
import urllib.request
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parent.parent
CACHE_DIR = ROOT / "data" / "_hudme_bot71_cache"
OUT = ROOT / "data" / "bot71_gravie_sim.json"

API = "https://hudme.com/api"
BOT_ID = 71
ORDER_USDC = 10.0  # Bot 71 config
API_MIN_ORDER_SIZE = 5.0  # default Polymarket

# ── Gravie sabitleri (src/strategy/gravie.rs ile birebir) ─────────────────
TICK_INTERVAL_SECS = 5
BUY_COOLDOWN_MS = 4_000
ENTRY_ASK_CEILING = 0.85
SECOND_LEG_GUARD_MS = 38_000
SECOND_LEG_OPP_TRIGGER = 0.55
T_CUTOFF_SECS = 90.0
BALANCE_REBALANCE = 0.45
REBALANCE_CEILING_MULTIPLIER = 1.20
SUM_AVG_CEILING = 1.20

# ─────────────────────────────────────────────
# Veri çekme (cache'li)
# ─────────────────────────────────────────────


def _http_get_json(url: str, retries: int = 3) -> Any:
    last_err = None
    for i in range(retries):
        try:
            req = urllib.request.Request(url, headers={"User-Agent": "gravie-sim/1.0"})
            with urllib.request.urlopen(req, timeout=30) as resp:
                return json.loads(resp.read())
        except Exception as e:
            last_err = e
            time.sleep(0.5 * (i + 1))
    raise RuntimeError(f"GET {url} failed after {retries} retries: {last_err}")


def fetch_sessions() -> list[dict]:
    cache = CACHE_DIR / "sessions.json"
    if cache.exists():
        return json.loads(cache.read_text())
    out = []
    offset = 0
    while True:
        page = _http_get_json(f"{API}/bots/{BOT_ID}/sessions?limit=20&offset={offset}")
        items = page["items"]
        out.extend(items)
        if len(out) >= page["total"]:
            break
        offset += len(items)
        if not items:
            break
    CACHE_DIR.mkdir(parents=True, exist_ok=True)
    cache.write_text(json.dumps(out, indent=2))
    return out


def fetch_ticks(slug: str) -> list[dict]:
    cache = CACHE_DIR / f"ticks_{slug}.json"
    if cache.exists():
        return json.loads(cache.read_text())
    # Pagination: since_ms=0, limit=800 her sayfa
    out = []
    since_ms = 0
    while True:
        page = _http_get_json(
            f"{API}/bots/{BOT_ID}/sessions/{slug}/ticks?since_ms={since_ms}&limit=800"
        )
        if not page:
            break
        out.extend(page)
        if len(page) < 800:
            break
        since_ms = page[-1]["ts_ms"] + 1
    CACHE_DIR.mkdir(parents=True, exist_ok=True)
    cache.write_text(json.dumps(out))
    return out


# ─────────────────────────────────────────────
# Gravie emulator (Rust kodu ile birebir)
# ─────────────────────────────────────────────


class GravieState:
    def __init__(self) -> None:
        self.phase = "Idle"  # Idle | Active | Stopped
        self.last_acted_secs = 1 << 63
        self.last_buy_ms = 0
        self.first_leg_side: str | None = None  # "Up" | "Down"
        self.first_leg_ms = 0


def opposite(side: str) -> str:
    return "Down" if side == "Up" else "Up"


def decide_buy(st: GravieState, up_ask: float, dn_ask: float, up_filled: float, dn_filled: float) -> tuple[str, float, str] | None:
    """Returns (dir, price, reason) or None."""
    # ── Rebalance bias ──
    if up_filled > 0.0 and dn_filled > 0.0:
        max_f = max(up_filled, dn_filled)
        min_f = min(up_filled, dn_filled)
        balance = (min_f / max_f) if max_f > 0 else 0.0
        if balance < BALANCE_REBALANCE:
            weak_side = "Up" if up_filled < dn_filled else "Down"
            weak_ask = up_ask if weak_side == "Up" else dn_ask
            if weak_ask > 0.0 and weak_ask <= ENTRY_ASK_CEILING * REBALANCE_CEILING_MULTIPLIER:
                return (weak_side, weak_ask, f"gravie:rebalance:{weak_side.lower()}")

    # ── İkinci leg ──
    if st.first_leg_side is not None:
        opp = opposite(st.first_leg_side)
        opp_filled = up_filled if opp == "Up" else dn_filled
        if opp_filled <= 0.0:
            opp_ask = up_ask if opp == "Up" else dn_ask
            guard_passed = False  # Caller fills this
            opp_cheap = opp_ask > 0.0 and opp_ask <= SECOND_LEG_OPP_TRIGGER
            # NOTE: caller'da guard check yapılır; burada placeholder bırakıp basitleştireceğiz.
            # Aslında çağrıda now_ms - first_leg_ms >= SECOND_LEG_GUARD_MS kontrolü yapacağız.
            # Burada True döner gibi davranıyoruz; gerçek kontrol caller'da.
            return ("__SECOND_LEG_CHECK__", opp_ask, opp)  # type: ignore

    # ── İlk leg / accum: argmin(ask) ──
    if up_ask > 0.0 and (dn_ask <= 0.0 or up_ask <= dn_ask) and up_ask <= ENTRY_ASK_CEILING:
        if st.first_leg_side is None:
            return ("Up", up_ask, "gravie:open:up")
        else:
            return ("Up", up_ask, "gravie:accum:up")
    if dn_ask > 0.0 and dn_ask <= ENTRY_ASK_CEILING:
        if st.first_leg_side is None:
            return ("Down", dn_ask, "gravie:open:down")
        else:
            return ("Down", dn_ask, "gravie:accum:down")
    return None


def gravie_decide(st: GravieState, ctx: dict) -> tuple[str, float, str] | None:
    """Tick için Gravie kararı. ctx: now_ms, start_ts (sec), to_end (sec),
       up_best_ask, dn_best_ask, up_filled, dn_filled, sum_avg."""
    to_end = ctx["to_end"]
    now_ms = ctx["now_ms"]
    rel_secs = (now_ms // 1000) - ctx["start_ts"]
    up_ask = ctx["up_best_ask"]
    dn_ask = ctx["dn_best_ask"]
    up_filled = ctx["up_filled"]
    dn_filled = ctx["dn_filled"]
    sum_avg = ctx["sum_avg"]

    if st.phase == "Stopped":
        return None
    if st.phase == "Idle":
        book_ready = up_ask > 0.0 and dn_ask > 0.0 and ctx["up_best_bid"] > 0.0 and ctx["dn_best_bid"] > 0.0
        if not book_ready:
            return None
        st.phase = "Active"
        # Pozisyon mirası
        if up_filled > 0.0 and up_filled >= dn_filled:
            st.first_leg_side = "Up"
        elif dn_filled > 0.0:
            st.first_leg_side = "Down"
        return None

    # ── Active ──
    if to_end <= T_CUTOFF_SECS:
        st.phase = "Stopped"
        return None
    if rel_secs % TICK_INTERVAL_SECS != 0:
        return None
    if rel_secs == st.last_acted_secs:
        return None
    st.last_acted_secs = rel_secs

    if up_ask <= 0.0 or dn_ask <= 0.0:
        return None
    if st.last_buy_ms > 0 and now_ms - st.last_buy_ms < BUY_COOLDOWN_MS:
        return None
    if up_filled > 0.0 and dn_filled > 0.0 and sum_avg >= SUM_AVG_CEILING:
        return None

    # ── Karar (decide_buy logic, second-leg guard'ı burada uygula) ──
    # 1) Rebalance bias
    if up_filled > 0.0 and dn_filled > 0.0:
        max_f = max(up_filled, dn_filled)
        min_f = min(up_filled, dn_filled)
        balance = (min_f / max_f) if max_f > 0 else 0.0
        if balance < BALANCE_REBALANCE:
            weak_side = "Up" if up_filled < dn_filled else "Down"
            weak_ask = up_ask if weak_side == "Up" else dn_ask
            if weak_ask > 0.0 and weak_ask <= ENTRY_ASK_CEILING * REBALANCE_CEILING_MULTIPLIER:
                return (weak_side, weak_ask, f"gravie:rebalance:{weak_side.lower()}")

    # 2) İkinci leg (first_leg_side dolu, opp_filled = 0)
    if st.first_leg_side is not None:
        opp = opposite(st.first_leg_side)
        opp_filled = up_filled if opp == "Up" else dn_filled
        if opp_filled <= 0.0:
            opp_ask = up_ask if opp == "Up" else dn_ask
            guard_passed = (now_ms - st.first_leg_ms) >= SECOND_LEG_GUARD_MS
            opp_cheap = opp_ask > 0.0 and opp_ask <= SECOND_LEG_OPP_TRIGGER
            if (guard_passed or opp_cheap) and opp_ask > 0.0 and opp_ask <= ENTRY_ASK_CEILING:
                return (opp, opp_ask, f"gravie:flip:{opp.lower()}")
            # First leg accumulation (eğer hâlâ ucuzsa)
            first_ask = up_ask if st.first_leg_side == "Up" else dn_ask
            if first_ask > 0.0 and first_ask <= ENTRY_ASK_CEILING:
                return (st.first_leg_side, first_ask, f"gravie:accum:{st.first_leg_side.lower()}")
            return None

    # 3) İlk leg / accum: argmin(ask)
    if up_ask > 0.0 and (dn_ask <= 0.0 or up_ask <= dn_ask) and up_ask <= ENTRY_ASK_CEILING:
        if st.first_leg_side is None:
            return ("Up", up_ask, "gravie:open:up")
        else:
            return ("Up", up_ask, "gravie:accum:up")
    if dn_ask > 0.0 and dn_ask <= ENTRY_ASK_CEILING:
        if st.first_leg_side is None:
            return ("Down", dn_ask, "gravie:open:down")
        else:
            return ("Down", dn_ask, "gravie:accum:down")
    return None


# ─────────────────────────────────────────────
# Session simülasyonu
# ─────────────────────────────────────────────


def simulate_session(session: dict, ticks: list[dict]) -> dict:
    slug = session["slug"]
    start_ts = session["start_ts"]
    end_ts = session["end_ts"]
    winner = session.get("winning_outcome")  # "Up" | "Down" | None

    st = GravieState()
    up_filled = 0.0
    dn_filled = 0.0
    up_usdc = 0.0
    dn_usdc = 0.0
    trades: list[dict] = []

    for tick in ticks:
        now_ms = tick["ts_ms"]
        to_end = end_ts - (now_ms // 1000)
        sum_avg = 0.0
        if up_filled > 0.0 and dn_filled > 0.0:
            sum_avg = (up_usdc / up_filled) + (dn_usdc / dn_filled)
        ctx = {
            "now_ms": now_ms,
            "start_ts": start_ts,
            "to_end": to_end,
            "up_best_bid": tick["up_best_bid"],
            "up_best_ask": tick["up_best_ask"],
            "dn_best_bid": tick["down_best_bid"],
            "dn_best_ask": tick["down_best_ask"],
            "up_filled": up_filled,
            "dn_filled": dn_filled,
            "sum_avg": sum_avg,
        }
        decision = gravie_decide(st, ctx)
        if decision is None:
            continue
        side, price, reason = decision
        # FAK fill: immediate at ask
        size = math.ceil(ORDER_USDC / price) if price > 0 else 0
        if size <= 0 or size * price < API_MIN_ORDER_SIZE:
            continue
        st.last_buy_ms = now_ms
        if side == "Up":
            up_filled += size
            up_usdc += size * price
        else:
            dn_filled += size
            dn_usdc += size * price
        if st.first_leg_side is None:
            st.first_leg_side = side
            st.first_leg_ms = now_ms
        trades.append({
            "ts_ms": now_ms,
            "side": side,
            "price": price,
            "size": size,
            "reason": reason,
        })

    spent = up_usdc + dn_usdc
    payout = 0.0
    if winner == "Up":
        payout = up_filled
    elif winner == "Down":
        payout = dn_filled
    pnl = payout - spent
    avg_up = up_usdc / up_filled if up_filled > 0 else 0
    avg_dn = dn_usdc / dn_filled if dn_filled > 0 else 0

    return {
        "slug": slug,
        "winner": winner,
        "n_trades": len(trades),
        "up_filled": round(up_filled, 4),
        "dn_filled": round(dn_filled, 4),
        "spent": round(spent, 4),
        "payout": round(payout, 4),
        "pnl": round(pnl, 4),
        "avg_up": round(avg_up, 4),
        "avg_dn": round(avg_dn, 4),
        "sum_avg": round(avg_up + avg_dn, 4) if up_filled > 0 and dn_filled > 0 else None,
        "balance": round(min(up_filled, dn_filled) / max(up_filled, dn_filled), 4) if max(up_filled, dn_filled) > 0 else 0,
        "first_side": ("Up" if up_filled >= dn_filled and up_filled > 0 else ("Down" if dn_filled > 0 else None)),
        "trades": trades,
        # Bonereaper karşılaştırma
        "bonereaper_pnl": session["pnl_if_up"] if winner == "Up" else (session["pnl_if_down"] if winner == "Down" else 0),
        "bonereaper_cost": session["cost_basis"],
    }


# ─────────────────────────────────────────────
# Main
# ─────────────────────────────────────────────


def main() -> None:
    print("Fetching sessions...")
    sessions = fetch_sessions()
    print(f"  → {len(sessions)} sessions")

    results = []
    for i, sess in enumerate(sessions, 1):
        slug = sess["slug"]
        print(f"  [{i}/{len(sessions)}] {slug} ...", end=" ")
        ticks = fetch_ticks(slug)
        if not ticks:
            print("no ticks, skip")
            continue
        sim = simulate_session(sess, ticks)
        results.append(sim)
        print(f"trades={sim['n_trades']:3d} spent=${sim['spent']:>7.2f} pnl=${sim['pnl']:>+7.2f} winner={sim['winner']}")

    # Aggregate
    total_spent = sum(r["spent"] for r in results)
    total_pnl = sum(r["pnl"] for r in results)
    total_bone_pnl = sum(r["bonereaper_pnl"] for r in results)
    total_bone_cost = sum(r["bonereaper_cost"] for r in results)
    resolved = [r for r in results if r["winner"] in ("Up", "Down")]
    wins = [r for r in resolved if r["pnl"] > 0]
    losses = [r for r in resolved if r["pnl"] < 0]
    bone_wins = [r for r in resolved if r["bonereaper_pnl"] > 0]
    bone_losses = [r for r in resolved if r["bonereaper_pnl"] < 0]

    summary = {
        "n_sessions_total": len(sessions),
        "n_sessions_simulated": len(results),
        "n_resolved": len(resolved),
        "gravie": {
            "spent": round(total_spent, 2),
            "pnl": round(total_pnl, 2),
            "roi_pct": round(total_pnl / total_spent * 100, 4) if total_spent else 0,
            "winrate_pct": round(len(wins) / len(resolved) * 100, 2) if resolved else 0,
            "wins": len(wins),
            "losses": len(losses),
            "avg_win": round(sum(r["pnl"] for r in wins) / len(wins), 2) if wins else 0,
            "avg_loss": round(sum(r["pnl"] for r in losses) / len(losses), 2) if losses else 0,
            "n_trades_total": sum(r["n_trades"] for r in results),
            "avg_trades_per_session": round(sum(r["n_trades"] for r in results) / len(results), 1) if results else 0,
        },
        "bonereaper_actual": {
            "spent": round(total_bone_cost, 2),
            "pnl": round(total_bone_pnl, 2),
            "roi_pct": round(total_bone_pnl / total_bone_cost * 100, 4) if total_bone_cost else 0,
            "winrate_pct": round(len(bone_wins) / len(resolved) * 100, 2) if resolved else 0,
            "wins": len(bone_wins),
            "losses": len(bone_losses),
        },
    }

    payload = {
        "bot_id": BOT_ID,
        "generated_at_utc": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
        "gravie_constants": {
            "TICK_INTERVAL_SECS": TICK_INTERVAL_SECS,
            "BUY_COOLDOWN_MS": BUY_COOLDOWN_MS,
            "ENTRY_ASK_CEILING": ENTRY_ASK_CEILING,
            "SECOND_LEG_GUARD_MS": SECOND_LEG_GUARD_MS,
            "SECOND_LEG_OPP_TRIGGER": SECOND_LEG_OPP_TRIGGER,
            "T_CUTOFF_SECS": T_CUTOFF_SECS,
            "BALANCE_REBALANCE": BALANCE_REBALANCE,
            "REBALANCE_CEILING_MULTIPLIER": REBALANCE_CEILING_MULTIPLIER,
            "SUM_AVG_CEILING": SUM_AVG_CEILING,
            "ORDER_USDC": ORDER_USDC,
        },
        "summary": summary,
        "sessions": results,
    }
    OUT.write_text(json.dumps(payload, indent=2))
    print(f"\nWrote {OUT}")
    print("\n=== SUMMARY ===")
    print(f"  Gravie:     spent ${summary['gravie']['spent']:>9,.2f}  PnL ${summary['gravie']['pnl']:>+9,.2f}  ROI {summary['gravie']['roi_pct']:>+6.2f}%  WR {summary['gravie']['winrate_pct']:>5.1f}%  ({summary['gravie']['wins']}W/{summary['gravie']['losses']}L)")
    print(f"  Bonereaper: spent ${summary['bonereaper_actual']['spent']:>9,.2f}  PnL ${summary['bonereaper_actual']['pnl']:>+9,.2f}  ROI {summary['bonereaper_actual']['roi_pct']:>+6.2f}%  WR {summary['bonereaper_actual']['winrate_pct']:>5.1f}%  ({summary['bonereaper_actual']['wins']}W/{summary['bonereaper_actual']['losses']}L)")


if __name__ == "__main__":
    main()
