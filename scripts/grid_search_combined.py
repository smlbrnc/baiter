#!/usr/bin/env python3
"""24-market combined grid search.

Optimize edilen parametreler:
  * hard_stop_avg_sum     [999=off, 1.10, 1.05, 1.00, 0.97]
  * max_requote_per_market [999=off, 50, 30, 20, 15]
  * flip_threshold        [3.0, 4.0, 5.0, 6.0]
  * bsi_rev               [1.5, 2.0, 2.5, 3.0, 100=off]
  * dscore_strong         [0.5, 1.0, 1.5, 2.0]
  * ofi_dir               [0.3, 0.4, 0.5]
  * requote_eps_ticks     [2, 3, 4]

Hedef: max(pnl_resolved); tie-break: max(yon_pct).
"""
from __future__ import annotations

import itertools

from backtest_param import ElisParams, run_all


def grid():
    base = ElisParams()
    grid_axes = {
        "hard_stop_avg_sum": [999.0, 1.10, 1.05, 1.00],
        "max_requote_per_market": [999, 50, 30, 20],
        "flip_threshold": [3.0, 4.0, 5.0, 6.0],
        "bsi_rev": [1.5, 2.0, 2.5, 3.0, 100.0],
        "dscore_strong": [0.5, 1.0, 1.5, 2.0],
        "ofi_dir": [0.3, 0.4, 0.5],
        "requote_eps_ticks": [2.0, 3.0, 4.0],
    }
    keys = list(grid_axes.keys())
    vals = [grid_axes[k] for k in keys]
    total = 1
    for v in vals:
        total *= len(v)
    print(f"Toplam kombinasyon: {total}")
    best = None
    i = 0
    for combo in itertools.product(*vals):
        i += 1
        params_dict = base.__dict__.copy()
        for k, v in zip(keys, combo):
            params_dict[k] = v
        p = ElisParams(**params_dict)
        r = run_all(p)
        score = (r["pnl_resolved"], r["correct"])
        if best is None or score > (best["pnl_resolved"], best["correct"]):
            best = {
                "params": {k: v for k, v in zip(keys, combo)},
                "pnl_resolved": r["pnl_resolved"],
                "correct": r["correct"],
                "n_resolved": r["n_resolved"],
                "pnl_net": r["pnl_net"],
            }
            print(f"[{i}/{total}] yeni en iyi: pnl=${r['pnl_resolved']:+.2f} "
                  f"yön={r['correct']}/{r['n_resolved']} → {best['params']}")
    print(f"\n=== EN İYİ ===")
    print(f"  PnL kesin     : ${best['pnl_resolved']:+.2f}")
    print(f"  Yön           : {best['correct']}/{best['n_resolved']}")
    print(f"  Net           : ${best['pnl_net']:+.2f}")
    print(f"  Params override: {best['params']}")


if __name__ == "__main__":
    grid()
