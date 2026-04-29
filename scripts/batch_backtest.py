#!/usr/bin/env python3
"""Birden çok markete `backtest_market.Sim`'i koşturur ve özet tablo basar.

Kullanım:
    python3 scripts/batch_backtest.py <slug1> <slug2> ...
    python3 scripts/batch_backtest.py --all-bot15
"""
from __future__ import annotations

import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from backtest_market import Sim, find_ticks_path, TICKS_DIRS


def run_one(slug: str):
    p = find_ticks_path(slug)
    if p is None:
        return {"slug": slug, "error": "tick file not found"}
    ticks = json.load(p.open())
    sim = Sim(ticks)
    sim.run()

    last = ticks[-1]
    up_bid_final = last["up_best_bid"]
    down_bid_final = last["down_best_bid"]
    if up_bid_final >= 0.95:
        winner = "Up"
    elif down_bid_final >= 0.95:
        winner = "Down"
    else:
        winner = "?"

    cost = sim.avg_up * sim.up_filled + sim.avg_down * sim.down_filled
    if winner == "Up":
        pnl = sim.up_filled - cost
        pnl_str = f"{pnl:+.2f}"
        pnl_val = pnl
    elif winner == "Down":
        pnl = sim.down_filled - cost
        pnl_str = f"{pnl:+.2f}"
        pnl_val = pnl
    else:
        pnl_if_up = sim.up_filled - cost
        pnl_if_down = sim.down_filled - cost
        sale = sim.up_filled * up_bid_final + sim.down_filled * down_bid_final - cost
        pnl_str = f"UP{pnl_if_up:+.0f}/DN{pnl_if_down:+.0f}/mid{sale:+.0f}"
        pnl_val = sale

    yon_ok = (winner != "?" and winner == sim.intent)

    flip_marker = "→" + sim.intent if sim.opener_intent != sim.intent else ""

    return {
        "slug": slug,
        "winner": winner,
        "opener": f"{sim.opener_intent}({sim.opener_rule}){flip_marker}",
        "trades": len(sim.trades),
        "up": sim.up_filled,
        "dn": sim.down_filled,
        "cost": cost,
        "pnl_str": pnl_str,
        "pnl_val": pnl_val,
        "final": f"up={up_bid_final:.2f} dn={down_bid_final:.2f}",
        "yon_ok": yon_ok,
    }


def main():
    args = sys.argv[1:]
    if not args:
        print("Usage: batch_backtest.py <slug>... | --all-bot15 | --all-bot14")
        return 1

    if args[0] == "--all-bot15":
        d = next((x for x in TICKS_DIRS if x.name.startswith("bot15-")), None)
        if d is None:
            print("bot15-* klasörü yok")
            return 1
        slugs = sorted(p.stem.replace("_ticks", "") for p in d.glob("btc-updown-5m-*_ticks.json"))
    elif args[0] == "--all-bot14":
        d = next((x for x in TICKS_DIRS if x.name.startswith("bot14-")), None)
        slugs = sorted(p.stem.replace("_ticks", "") for p in d.glob("btc-updown-5m-*_ticks.json"))
    else:
        slugs = args

    print(f"{'slug':30s} | {'winner':6s} | {'opener':32s} | {'trd':>4} | {'PnL':>20s} | yön")
    print("-" * 110)
    results = []
    pnl_total_resolved = 0.0
    pnl_total_mid = 0.0
    correct = 0
    resolved = 0
    for slug in slugs:
        r = run_one(slug)
        if "error" in r:
            print(f"{slug:30s} | ERROR: {r['error']}")
            continue
        print(f"{r['slug']:30s} | {r['winner']:6s} | {r['opener']:32s} | "
              f"{r['trades']:>4} | {r['pnl_str']:>20s} | "
              f"{'✓' if r['yon_ok'] else ('-' if r['winner'] == '?' else '✗')}")
        if r["winner"] != "?":
            resolved += 1
            pnl_total_resolved += r["pnl_val"]
            if r["yon_ok"]:
                correct += 1
        else:
            pnl_total_mid += r["pnl_val"]
        results.append(r)

    print("-" * 110)
    if resolved > 0:
        print(f"Yön doğruluğu : {correct}/{resolved} = %{correct/resolved*100:.0f}")
        print(f"Kesin PnL     : ${pnl_total_resolved:+.2f}")
    print(f"Belirsiz mid  : ${pnl_total_mid:+.2f}")
    print(f"NET PnL       : ${pnl_total_resolved + pnl_total_mid:+.2f}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
