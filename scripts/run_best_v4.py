#!/usr/bin/env python3
"""Grid-search'ten gelen v4 en iyi parametreyi 24 marketde tek tek raporla."""
from backtest_param import ElisParams, run_all, run_market, all_slugs


BEST = ElisParams(
    flip_threshold=6.0,
    bsi_rev=1.5,
    dscore_strong=1.5,
    ofi_dir=0.3,
    requote_eps_ticks=4.0,
    # diğerleri default (hard_stop / max_requote OFF; çünkü requote_eps=4 yeterli)
)


def main():
    print(f"=== Elis v4 — 24 market combined ===\n")
    print(f"{'slug':30s} | {'win':4s} | {'opener':30s} | {'flip':5s} | "
          f"{'trd':>4s} | {'PnL':>10s} | yön")
    print("-" * 105)
    pnl_resolved = 0.0
    pnl_mid = 0.0
    correct = 0
    resolved = 0
    rule_stats = {}
    for slug in all_slugs():
        r = run_market(slug, BEST)
        flip_str = "→" + r["intent"] if r["intent"] != r["opener"] else ""
        opener_full = f"{r['opener']}({r['rule']}){flip_str}"
        pnl_str = f"{r['pnl']:+.2f}" if r["winner"] != "?" else f"mid {r['pnl']:+.0f}"
        ok_str = "✓" if r["yon_ok"] else ("-" if r["winner"] == "?" else "✗")
        print(f"{r['slug']:30s} | {r['winner']:4s} | {opener_full:30s} | "
              f"{r['intent'] if flip_str else '':5s} | "
              f"{r['trades']:>4} | {pnl_str:>10s} | {ok_str}")
        if r["winner"] != "?":
            resolved += 1
            pnl_resolved += r["pnl"]
            if r["yon_ok"]:
                correct += 1
            rs = rule_stats.setdefault(r["rule"], [0, 0])
            rs[1] += 1
            if r["yon_ok"]:
                rs[0] += 1
        else:
            pnl_mid += r["pnl"]

    print("-" * 105)
    print(f"\n=== ÖZET (Elis v4) ===")
    print(f"  Yön doğruluğu: {correct}/{resolved} = %{correct/resolved*100:.0f}")
    print(f"  Kesin PnL    : ${pnl_resolved:+.2f}")
    print(f"  Belirsiz mid : ${pnl_mid:+.2f}")
    print(f"  NET PnL      : ${pnl_resolved + pnl_mid:+.2f}")

    print(f"\n  Rule-bazlı doğruluk:")
    for rule, (ok, tot) in sorted(rule_stats.items(), key=lambda x: -x[1][1]):
        print(f"    {rule:15s} {ok}/{tot} ({ok/tot*100:.0f}%)")


if __name__ == "__main__":
    main()
