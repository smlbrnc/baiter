#!/usr/bin/env python3
"""Opener kuralı için hyperparam grid search.

10 marketde (1777471800 belirsiz, atlanır → 9 net market) en iyi kuralı bulur.

Multi-feature voting yaklaşımı:
1. BSI extreme reversion: |bsi| > BSI_REV_TH → bsi tersi
2. OFI/CVD exhaustion: |ofi_avg|>OFI_EXH_TH ve |cvd|>CVD_EXH_TH → flow tersi
3. Score+OFI agreement: aynı yöndeyse momentum
4. dscore strong: |dscore|>DSCORE_STRONG → momentum
5. Fallback: score_avg
"""

from __future__ import annotations

import json
from itertools import product
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
TICKS_DIR = ROOT / "exports" / "bot14-ticks-20260429"

MARKETS_LABELED = [
    ("btc-updown-5m-1777467000", "Up"),
    ("btc-updown-5m-1777467300", "Down"),
    # 7467600 belirsiz (final 0.10/0.42)
    ("btc-updown-5m-1777467900", "Down"),
    ("btc-updown-5m-1777468200", "Up"),
    # 7468500 belirsiz (final 0.15/0.25)
    ("btc-updown-5m-1777471200", "Down"),
    # 7471800 belirsiz (final 0.10/0.78)
    ("btc-updown-5m-1777472100", "Up"),
    ("btc-updown-5m-1777473000", "Down"),
    ("btc-updown-5m-1777473900", "Down"),
    ("btc-updown-5m-1777474500", "Down"),
]


def load_ticks(slug):
    p = TICKS_DIR / f"{slug}_ticks.json"
    return json.load(p.open())


def features(ticks, n):
    pre = ticks[:n]
    score_first = pre[0]["signal_score"]
    score_last = pre[-1]["signal_score"]
    dscore = score_last - score_first
    score_avg = sum(t["signal_score"] for t in pre) / len(pre)
    bsi = pre[-1]["bsi"]
    ofi_avg = sum(t["ofi"] for t in pre) / len(pre)
    cvd = pre[-1]["cvd"]
    return {
        "dscore": dscore,
        "score_avg": score_avg,
        "bsi": bsi,
        "ofi_avg": ofi_avg,
        "cvd": cvd,
    }


def predict(features, params):
    """Multi-rule decision"""
    bsi = features["bsi"]
    ofi = features["ofi_avg"]
    cvd = features["cvd"]
    dscore = features["dscore"]
    score_avg = features["score_avg"]

    # 1. BSI extreme reversion
    if abs(bsi) > params["bsi_rev_th"]:
        return ("Down", "bsi_rev") if bsi > 0 else ("Up", "bsi_rev")

    # 2. OFI/CVD exhaustion
    if abs(ofi) > params["ofi_exh_th"] and abs(cvd) > params["cvd_exh_th"]:
        if ofi > 0 and cvd > 0:
            return ("Down", "exhaustion")
        elif ofi < 0 and cvd < 0:
            return ("Up", "exhaustion")
        # opposite sign: skip

    # 3. OFI strong directional (aggressive flow takip)
    if abs(ofi) > params["ofi_dir_th"]:
        return ("Up", "ofi_dir") if ofi > 0 else ("Down", "ofi_dir")

    # 4. dscore strong momentum
    if abs(dscore) > params["dscore_strong"]:
        return ("Up", "momentum") if dscore > 0 else ("Down", "momentum")

    # 5. Fallback: score_avg
    return ("Up", "score_avg") if score_avg >= params["score_neutral"] else ("Down", "score_avg")


def evaluate(params, window):
    correct = 0
    misses = []
    rule_breakdown = {}
    for slug, true_d in MARKETS_LABELED:
        ticks = load_ticks(slug)
        if window > len(ticks):
            continue
        f = features(ticks, window)
        pred, rule = predict(f, params)
        rule_breakdown[rule] = rule_breakdown.get(rule, 0) + 1
        if pred == true_d:
            correct += 1
        else:
            misses.append((slug, true_d, pred, rule, f))
    return correct, misses, rule_breakdown


def main():
    grids = {
        "window": [15, 18, 20, 22, 25, 30],
        "bsi_rev_th": [1.0, 1.5, 2.0, 2.5, 3.0],
        "ofi_exh_th": [0.25, 0.3, 0.35, 0.4, 0.5],
        "cvd_exh_th": [1.5, 2.0, 3.0, 4.0, 5.0],
        "ofi_dir_th": [0.25, 0.3, 0.4, 0.5],
        "dscore_strong": [0.5, 1.0, 1.5, 2.0],
        "score_neutral": [5.0],
    }

    best_score = 0
    best_combos = []
    total = 1
    for k, v in grids.items():
        if k != "window":
            total *= len(v)
    print(f"Grid size: {total} kombinasyon × {len(grids['window'])} pencere = {total * len(grids['window'])}")
    print(f"Toplam market: {len(MARKETS_LABELED)}")
    print()

    for window in grids["window"]:
        for bsi, ofi_e, cvd_e, ofi_d, ds, sn in product(
            grids["bsi_rev_th"],
            grids["ofi_exh_th"],
            grids["cvd_exh_th"],
            grids["ofi_dir_th"],
            grids["dscore_strong"],
            grids["score_neutral"],
        ):
            params = {
                "bsi_rev_th": bsi,
                "ofi_exh_th": ofi_e,
                "cvd_exh_th": cvd_e,
                "ofi_dir_th": ofi_d,
                "dscore_strong": ds,
                "score_neutral": sn,
            }
            score, misses, rules = evaluate(params, window)
            if score > best_score:
                best_score = score
                best_combos = [(window, params, misses, rules)]
            elif score == best_score:
                if len(best_combos) < 10:
                    best_combos.append((window, params, misses, rules))

    print(f"Maksimum doğruluk: {best_score} / {len(MARKETS_LABELED)}")
    print(f"Bu skoru veren kombinasyon sayısı: {len(best_combos)}")
    print()
    for window, params, misses, rules in best_combos[:8]:
        print(f"Window={window}, params={params}")
        print(f"  Rule breakdown: {rules}")
        for slug, true_d, pred, rule, f in misses:
            print(f"  MISS: {slug[-10:]} true={true_d} pred={pred} ({rule}) "
                  f"bsi={f['bsi']:+.2f} ofi={f['ofi_avg']:+.2f} "
                  f"cvd={f['cvd']:+.1f} dscore={f['dscore']:+.2f} avg={f['score_avg']:.2f}")
        print()


if __name__ == "__main__":
    main()
