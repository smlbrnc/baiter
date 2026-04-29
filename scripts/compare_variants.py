#!/usr/bin/env python3
"""v3 vs v4 vs ara varyantlar — combined 24-market sonuçlarını karşılaştır."""
from backtest_param import ElisParams, run_all

variants = {
    "v3 (mevcut default)": ElisParams(),
    "v3 + requote_eps=4": ElisParams(requote_eps_ticks=4.0),
    "v3 + requote_eps=4 + bsi_rev=1.5": ElisParams(requote_eps_ticks=4.0, bsi_rev=1.5),
    "v4a (flip=6)": ElisParams(flip_threshold=6.0, requote_eps_ticks=4.0,
                                bsi_rev=1.5, dscore_strong=1.5, ofi_dir=0.3),
    "v4b (flip=5, ofi_dir=0.3)": ElisParams(flip_threshold=5.0, requote_eps_ticks=4.0,
                                              bsi_rev=1.5, dscore_strong=1.5, ofi_dir=0.3),
    "v4c (flip=4)": ElisParams(flip_threshold=4.0, requote_eps_ticks=4.0,
                                bsi_rev=1.5, dscore_strong=1.5, ofi_dir=0.3),
    "v4 + hard_stop=1.05": ElisParams(flip_threshold=6.0, requote_eps_ticks=4.0,
                                        bsi_rev=1.5, dscore_strong=1.5, ofi_dir=0.3,
                                        hard_stop_avg_sum=1.05),
    "v4 + max_requote=20": ElisParams(flip_threshold=6.0, requote_eps_ticks=4.0,
                                        bsi_rev=1.5, dscore_strong=1.5, ofi_dir=0.3,
                                        max_requote_per_market=20),
}

print(f"{'variant':40s} | {'yön':>10s} | {'kesin PnL':>11s} | {'mid':>10s} | {'net':>10s}")
print("-" * 95)
for name, p in variants.items():
    r = run_all(p)
    print(f"{name:40s} | {r['correct']:>3d}/{r['n_resolved']:<3d}={r['yon_pct']:>3.0f}% | "
          f"${r['pnl_resolved']:>+9.2f} | ${r['pnl_mid']:>+8.2f} | ${r['pnl_net']:>+8.2f}")
