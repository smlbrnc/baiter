#!/usr/bin/env python3
"""Per-market detay raporu: trade tablosu + emir-bazlı yorum + sinyal sparkline.

Çıktı: exports/blackbox-per-market-20260429.md (tek dosya, tüm marketler).
"""

from __future__ import annotations

import csv
import json
from collections import Counter, defaultdict
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
CSV_PATH = ROOT / "exports" / "blackbox-trades-20260429.csv"
TICKS_DIR = ROOT / "exports" / "bot14-ticks-20260429"
LOG_DIR = ROOT / "exports"
OUT = ROOT / "exports" / "blackbox-per-market-20260429.md"

REDEEMED = {
    "btc-updown-5m-1777467300": 1054.59,
    "btc-updown-5m-1777467600": 795.54,
    "btc-updown-5m-1777468200": 3563.24,
}

BLOCKS = "▁▂▃▄▅▆▇█"


def sparkline(values: list[float], lo: float | None = None, hi: float | None = None) -> str:
    if not values:
        return ""
    lo = lo if lo is not None else min(values)
    hi = hi if hi is not None else max(values)
    span = hi - lo if hi > lo else 1.0
    out = []
    for v in values:
        n = (v - lo) / span
        idx = max(0, min(len(BLOCKS) - 1, int(n * (len(BLOCKS) - 1))))
        out.append(BLOCKS[idx])
    return "".join(out)


def load_csv() -> list[dict[str, str]]:
    return list(csv.DictReader(CSV_PATH.open()))


def load_ticks(market: str) -> list[dict]:
    p = TICKS_DIR / f"{market}_ticks.json"
    return json.loads(p.read_text())


def fnum(s: str) -> float:
    if s == "" or s is None:
        return float("nan")
    return float(s)


def trade_comment(r: dict[str, str]) -> str:
    t = r["candidate_trigger"]
    out = r["outcome"]
    score = fnum(r["score"])
    dscore = fnum(r["dscore"])
    ofi = fnum(r["ofi"])
    bsi = fnum(r["bsi"])
    intent = r.get("intent_before", "")
    is_dom = intent == out

    if t == "signal_open":
        return f"İlk emir — score={score:.2f}, intent={out}; bsi={bsi:+.2f}, ofi={ofi:+.2f}"
    if t == "signal_flip":
        return f"Yön değişti: {intent}→{out} (Δscore={dscore:+.2f}); eski dom emirler iptal edilmiş olmalı"
    if t == "price_drift":
        side = "dom" if is_dom else "hedge"
        return f"Fiyat hareketi → {side} requote (price={r['price']})"
    if t == "avg_down_edge":
        avg = fnum(r["avg_dom"])
        edge = (avg - fnum(r["price"])) / 0.01 if avg > 0 else 0
        return f"Avg-down: dom avg={avg:.3f}, fiyat={r['price']} ({edge:.1f} tick aşağı)"
    if t == "pyramid_signal":
        return f"Pyramid (dom={out}): ofi={ofi:.2f}, score={score:.2f} (trend güçlü)"
    if t == "parity_gap":
        gap = abs(fnum(r["dom_filled"]) - fnum(r["opp_filled"]))
        return f"Hedge top-up: |dom-opp|={gap:.0f} share"
    if t == "pre_resolve_scoop":
        return f"Pre-resolve scoop: opp_bid çok düşük, kazanan tarafa ucuz hisse"
    if t == "deadline_cleanup":
        return f"Deadline cleanup (t={r['t_off']}s, son saniyeler)"
    if t == "unknown":
        return f"Sınıflandırılamadı (review gerekli) — score={score:.2f}, dscore={dscore:+.2f}, intent_before={intent}"
    return ""


def market_section(market: str, rows: list[dict[str, str]], ticks: list[dict]) -> str:
    lines = []
    market_start = int(market.split("-")[-1])
    redeem = REDEEMED.get(market)
    status = f"WIN +{redeem:.2f} USDC" if redeem else "LOSE / unresolved"

    lines.append(f"## {market} — {status}")
    lines.append("")
    lines.append(f"- {len(rows)} emir, son emir t_off={max(int(r['t_off']) for r in rows)}s")
    ups = sum(1 for r in rows if r["outcome"] == "Up")
    downs = sum(1 for r in rows if r["outcome"] == "Down")
    lines.append(f"- UP={ups}, DOWN={downs}")
    open_row = next((r for r in rows if r["candidate_trigger"] == "signal_open"), rows[0])
    lines.append(f"- Opener: t={open_row['t_off']}s, {open_row['outcome']}, score={open_row['score']}, bsi={open_row['bsi']}, ofi={open_row['ofi']}")

    flips = [r for r in rows if r["candidate_trigger"] == "signal_flip"]
    if flips:
        lines.append(f"- {len(flips)} signal_flip:")
        for f in flips:
            lines.append(f"  - t={f['t_off']}s {f['intent_before']}→{f['outcome']} (Δscore={f['dscore']})")

    trig_count = Counter(r["candidate_trigger"] for r in rows)
    lines.append("- Trigger dağılımı: " + ", ".join(f"`{k}`={v}" for k, v in trig_count.most_common()))

    # Sparkline: signal_score ve up_bid (her saniye)
    lines.append("")
    lines.append("### Sinyal trendi (5dk pencere)")
    lines.append("")
    scores = [t["signal_score"] for t in ticks]
    up_bids = [t["up_best_bid"] for t in ticks]
    down_bids = [t["down_best_bid"] for t in ticks]
    ofis = [t["ofi"] for t in ticks]
    lines.append("```")
    lines.append(f"signal_score [0-10]   : {sparkline(scores, 0, 10)}")
    lines.append(f"up_best_bid  [0-1]    : {sparkline(up_bids, 0, 1)}")
    lines.append(f"down_best_bid [0-1]   : {sparkline(down_bids, 0, 1)}")
    lines.append(f"ofi          [-1,1]   : {sparkline(ofis, -1, 1)}")
    lines.append("```")

    # Emir tablosu
    lines.append("")
    lines.append("### Emir-bazlı detay")
    lines.append("")
    cols = [
        ("t_off", 5),
        ("outcome", 4),
        ("size", 8),
        ("price", 6),
        ("score", 6),
        ("dscore", 7),
        ("ofi", 7),
        ("up_bid", 6),
        ("down_bid", 8),
        ("avg_dom", 7),
        ("imbalance", 9),
        ("candidate_trigger", 18),
        ("conf", 4),
    ]
    header = " | ".join(f"{c:<{w}}" for c, w in cols)
    sep = " | ".join("-" * w for _, w in cols)
    lines.append("```")
    lines.append(header)
    lines.append(sep)
    for r in rows:
        cells = []
        for c, w in cols:
            if c == "conf":
                v = r.get("confidence", "")
            else:
                v = r.get(c, "")
            cells.append(f"{v:<{w}}")
        lines.append(" | ".join(cells))
    lines.append("```")

    # Yorumlar
    lines.append("")
    lines.append("### Emir yorumları")
    lines.append("")
    for r in rows:
        comment = trade_comment(r)
        lines.append(f"- **t={r['t_off']}s** ({r['outcome']} sz={r['size']} pr={r['price']}, `{r['candidate_trigger']}`): {comment}")

    return "\n".join(lines)


def main() -> int:
    rows = load_csv()
    by_market: dict[str, list[dict[str, str]]] = defaultdict(list)
    for r in rows:
        by_market[r["market"]].append(r)

    out_lines = [
        "# Black-box bot — Per-market emir-bazlı detay",
        "",
        "Her marketin tüm emirleri tek tek incelendi: trigger, role, sinyal değerleri,",
        "ve insan-okur yorum. Sparkline'lar (▁..█) tick verisini özetler.",
        "",
        "**Veri:** `exports/blackbox-trades-20260429.csv` (307 emir, 6 market).",
        "",
    ]

    for market in sorted(by_market.keys()):
        ticks = load_ticks(market)
        out_lines.append(market_section(market, by_market[market], ticks))
        out_lines.append("\n---\n")

    OUT.write_text("\n".join(out_lines))
    print(f"[OK] {OUT.relative_to(ROOT)} ({sum(len(v) for v in by_market.values())} emir, {len(by_market)} market)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
