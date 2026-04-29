#!/usr/bin/env python3
"""24 marketten pre-opener feature'ları + final winner çıkarır.

Output: `exports/features-24-markets.json`
Format: list of {slug, winner, pre: {dscore, score_avg, bsi_last, ofi_avg, cvd_last}}

Winner kuralı: final tick'te `up_best_bid >= 0.95` → Up; `<= 0.05` → Down; aksi → "?".
"""
from __future__ import annotations

import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
EXPORTS = ROOT / "exports"
PRE_TICKS = 20


def first_active_tick_idx(ticks: list) -> int:
    """İlk `up_best_bid > 0` olan tick'in indeksi."""
    for i, t in enumerate(ticks):
        if t["up_best_bid"] > 0.0 or t["down_best_bid"] > 0.0:
            return i
    return 0


def extract_one(slug: str, ticks: list) -> dict:
    start = first_active_tick_idx(ticks)
    pre = ticks[start : start + PRE_TICKS]
    if len(pre) < PRE_TICKS:
        return {"slug": slug, "error": "tick too short"}

    last_score = pre[-1]["signal_score"]
    first_score = pre[0]["signal_score"]
    score_avg = sum(t["signal_score"] for t in pre) / PRE_TICKS
    bsi_last = pre[-1]["bsi"]
    ofi_avg = sum(t["ofi"] for t in pre) / PRE_TICKS
    cvd_last = pre[-1]["cvd"]
    dscore = last_score - first_score

    final = ticks[-1]
    up_b = final["up_best_bid"]
    dn_b = final["down_best_bid"]
    if up_b >= 0.95:
        winner = "Up"
    elif dn_b >= 0.95:
        winner = "Down"
    else:
        winner = "?"

    return {
        "slug": slug,
        "winner": winner,
        "final_up_bid": up_b,
        "final_dn_bid": dn_b,
        "pre": {
            "dscore": dscore,
            "score_avg": score_avg,
            "bsi_last": bsi_last,
            "ofi_avg": ofi_avg,
            "cvd_last": cvd_last,
        },
    }


def main():
    out = []
    for d in sorted(EXPORTS.glob("bot*-ticks-*")):
        for p in sorted(d.glob("btc-updown-5m-*_ticks.json")):
            slug = p.stem.replace("_ticks", "")
            ticks = json.load(p.open())
            r = extract_one(slug, ticks)
            r["bot_dir"] = d.name
            out.append(r)

    out_path = EXPORTS / "features-24-markets.json"
    out_path.write_text(json.dumps(out, indent=2))
    print(f"Yazıldı: {out_path}  ({len(out)} market)")
    resolved = [r for r in out if r.get("winner") in ("Up", "Down")]
    print(f"Resolved: {len(resolved)} (Up: {sum(1 for r in resolved if r['winner']=='Up')}, "
          f"Down: {sum(1 for r in resolved if r['winner']=='Down')})")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
