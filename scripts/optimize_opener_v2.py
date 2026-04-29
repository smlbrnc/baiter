#!/usr/bin/env python3
"""24-market opener kural optimizasyonu (v2).

Hipotez: önceki dataset Down-heavy (9/13 = %69), bu yüzden bsi/ofi/cvd
ekstrem değerler "reversion" olarak gözüktü. 24-market combined dataset
20 resolved (Up:8 / Down:12 = %60 Down) → daha dengeli.

Test edilen modlar:
  A. Saf composite (mevcut 5-rule ladder, threshold sweep)
  B. Sadece momentum (rule 4+5; rule 1-3 kapalı)
  C. Reverse mean-reversion (rule 1-3 yön ters çevrilmiş — momentum sinyali olarak)
  D. OFI-only directional (sadece rule 3 + rule 5 fallback)

Çıktı: tablo (mod, doğruluk, hangi marketler yanlış) + en iyi mod.
"""
from __future__ import annotations

import json
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
DATA = ROOT / "exports" / "features-24-markets.json"


def load():
    raw = json.load(DATA.open())
    return [r for r in raw if r.get("winner") in ("Up", "Down")]


# ----------------------------------------------------------------------------
# Karar fonksiyonları (her biri (intent, rule) döner)
# ----------------------------------------------------------------------------

def mode_a_composite(f, p):
    """Mevcut 5-rule ladder."""
    bsi = f["bsi_last"]
    if abs(bsi) > p["bsi_rev"]:
        return ("Down" if bsi > 0 else "Up", "bsi_rev")
    ofi = f["ofi_avg"]
    cvd = f["cvd_last"]
    if abs(ofi) > p["ofi_exh"] and abs(cvd) > p["cvd_exh"]:
        if ofi > 0 and cvd > 0:
            return ("Down", "exhaustion")
        if ofi < 0 and cvd < 0:
            return ("Up", "exhaustion")
    if abs(ofi) > p["ofi_dir"]:
        return ("Up" if ofi > 0 else "Down", "ofi_dir")
    if abs(f["dscore"]) > p["dscore_strong"]:
        return ("Up" if f["dscore"] > 0 else "Down", "momentum")
    return ("Up" if f["score_avg"] >= 5.0 else "Down", "score_avg")


def mode_b_momentum_only(f, p):
    """Rule 1-3 KAPALI; sadece dscore + score_avg."""
    if abs(f["dscore"]) > p["dscore_strong"]:
        return ("Up" if f["dscore"] > 0 else "Down", "momentum")
    return ("Up" if f["score_avg"] >= 5.0 else "Down", "score_avg")


def mode_c_reverse_mr(f, p):
    """Rule 1-3 YÖN TERS — bsi/ofi/cvd momentum sinyali olarak yorumla."""
    bsi = f["bsi_last"]
    if abs(bsi) > p["bsi_rev"]:
        return ("Up" if bsi > 0 else "Down", "bsi_momentum")  # ters
    ofi = f["ofi_avg"]
    cvd = f["cvd_last"]
    if abs(ofi) > p["ofi_exh"] and abs(cvd) > p["cvd_exh"]:
        if ofi > 0 and cvd > 0:
            return ("Up", "exh_momentum")  # ters
        if ofi < 0 and cvd < 0:
            return ("Down", "exh_momentum")  # ters
    if abs(ofi) > p["ofi_dir"]:
        return ("Up" if ofi > 0 else "Down", "ofi_dir")
    if abs(f["dscore"]) > p["dscore_strong"]:
        return ("Up" if f["dscore"] > 0 else "Down", "momentum")
    return ("Up" if f["score_avg"] >= 5.0 else "Down", "score_avg")


def mode_d_ofi_only(f, p):
    """Sadece rule 3 (ofi_dir) + rule 5 fallback."""
    ofi = f["ofi_avg"]
    if abs(ofi) > p["ofi_dir"]:
        return ("Up" if ofi > 0 else "Down", "ofi_dir")
    return ("Up" if f["score_avg"] >= 5.0 else "Down", "score_avg")


# ----------------------------------------------------------------------------
# Test runner
# ----------------------------------------------------------------------------

DEFAULT_PARAMS = {
    "bsi_rev": 2.0,
    "ofi_exh": 0.4,
    "cvd_exh": 3.0,
    "ofi_dir": 0.4,
    "dscore_strong": 1.0,
}


def evaluate(mode_fn, mode_name, data, params=None):
    if params is None:
        params = DEFAULT_PARAMS
    correct = 0
    total = len(data)
    rule_stats = {}  # rule -> (correct, total)
    wrong = []
    for r in data:
        intent, rule = mode_fn(r["pre"], params)
        ok = intent == r["winner"]
        if ok:
            correct += 1
        rs = rule_stats.setdefault(rule, [0, 0])
        rs[0] += 1 if ok else 0
        rs[1] += 1
        if not ok:
            wrong.append(f"{r['slug']}({rule}:{intent}≠{r['winner']})")
    return {
        "mode": mode_name,
        "correct": correct,
        "total": total,
        "pct": correct / total * 100,
        "rule_stats": rule_stats,
        "wrong": wrong,
        "params": params,
    }


def print_result(r):
    print(f"\n=== {r['mode']} → {r['correct']}/{r['total']} = %{r['pct']:.0f} ===")
    print(f"  params: {r['params']}")
    print("  rule breakdown:")
    for rule, (ok, tot) in sorted(r["rule_stats"].items(), key=lambda x: -x[1][1]):
        print(f"    {rule:20s} {ok}/{tot} ({ok/tot*100:.0f}%)")
    if r["wrong"]:
        print(f"  yanlış marketler: {len(r['wrong'])}")
        for w in r["wrong"]:
            print(f"    ✗ {w}")


def main():
    data = load()
    print(f"Toplam resolved market: {len(data)}")
    print(f"  Up: {sum(1 for r in data if r['winner']=='Up')} "
          f"Down: {sum(1 for r in data if r['winner']=='Down')}")

    # 4 modu default parametrelerle test
    modes = [
        (mode_a_composite, "A. Composite (mevcut 5-rule)"),
        (mode_b_momentum_only, "B. Sadece momentum (rule 1-3 kapalı)"),
        (mode_c_reverse_mr, "C. Reverse-MR (bsi/cvd momentum yönünde)"),
        (mode_d_ofi_only, "D. OFI-only (rule 3 + fallback)"),
    ]
    results = [evaluate(fn, name, data) for fn, name in modes]
    for r in results:
        print_result(r)

    # En iyi modu seç
    best = max(results, key=lambda x: x["pct"])
    print(f"\n{'='*60}")
    print(f"EN İYİ MOD: {best['mode']} → %{best['pct']:.0f}")
    print(f"{'='*60}")

    # En iyi mod için threshold sweep (eğer rule kullanıyorsa)
    if "Composite" in best["mode"] or "Reverse" in best["mode"]:
        print("\n--- En iyi mod için threshold sweep ---")
        best_fn = mode_a_composite if "Composite" in best["mode"] else mode_c_reverse_mr
        best_pct = best["pct"]
        best_params = best["params"]
        for bsi_rev in [1.5, 2.0, 2.5, 3.0, 4.0, 5.0, 100.0]:  # 100 = bsi rule'u kapat
            for ofi_dir in [0.3, 0.4, 0.5, 0.6, 100.0]:  # 100 = ofi_dir kapat
                for dscore in [0.5, 1.0, 1.5, 2.0]:
                    p = {
                        "bsi_rev": bsi_rev,
                        "ofi_exh": 0.4,
                        "cvd_exh": 3.0,
                        "ofi_dir": ofi_dir,
                        "dscore_strong": dscore,
                    }
                    r = evaluate(best_fn, "sweep", data, p)
                    if r["pct"] > best_pct:
                        best_pct = r["pct"]
                        best_params = p
                        print(f"  yeni en iyi: %{r['pct']:.0f} at {p}")
        print(f"\nFinal best params: {best_params} → %{best_pct:.0f}")


if __name__ == "__main__":
    raise SystemExit(main())
