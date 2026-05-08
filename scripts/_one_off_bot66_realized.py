"""Bot 66 gerçekleşmiş PnL / ROI / winrate hesaplama.

Yöntem: data/new-bot-log.log içindeki:
  - trades  → Σ(size × price) = spent (USDC)
  - activity[type=REDEEM] → usdcSize = resolve sonrası alınan USDC
PnL = redeem - spent (her market için).

Çıktı: data/bot66_realized_pnl.json (per-slug detay + agregat)
"""

from __future__ import annotations

import json
import re
from collections import defaultdict
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parent.parent
SRC = ROOT / "data" / "new-bot-log.log"
OUT = ROOT / "data" / "bot66_realized_pnl.json"


def get_dur(slug: str) -> str | None:
    m = re.match(r".*-updown-(5m|15m|1h|4h)-", slug)
    if m:
        return m.group(1)
    if "-up-or-down-may-" in slug:
        return "1h"
    return None


def get_coin(slug: str) -> str | None:
    m = re.match(r"(btc|eth|sol|xrp)-updown-", slug)
    if m:
        return m.group(1).upper()
    m2 = re.match(r"(bitcoin|ethereum|solana|xrp)-up-or-down", slug)
    if m2:
        return {"bitcoin": "BTC", "ethereum": "ETH", "solana": "SOL", "xrp": "XRP"}[m2.group(1)]
    return None


def main() -> None:
    raw = json.loads(SRC.read_text())

    spent_by_slug: dict[str, float] = defaultdict(float)
    up_size_by_slug: dict[str, float] = defaultdict(float)
    dn_size_by_slug: dict[str, float] = defaultdict(float)
    n_trades_by_slug: dict[str, int] = defaultdict(int)
    title_by_slug: dict[str, str] = {}
    for t in raw["trades"]:
        slug = t["slug"]
        sz = float(t["size"])
        px = float(t["price"])
        spent_by_slug[slug] += sz * px
        n_trades_by_slug[slug] += 1
        title_by_slug.setdefault(slug, t.get("title", ""))
        if t["outcome"] == "Up":
            up_size_by_slug[slug] += sz
        elif t["outcome"] == "Down":
            dn_size_by_slug[slug] += sz

    redeem_by_slug: dict[str, float] = defaultdict(float)
    n_redeems_by_slug: dict[str, int] = defaultdict(int)
    for a in raw["activity"]:
        if a.get("type") == "REDEEM":
            redeem_by_slug[a["slug"]] += float(a["usdcSize"])
            n_redeems_by_slug[a["slug"]] += 1

    markets: list[dict[str, Any]] = []
    for slug, spent in spent_by_slug.items():
        redeem = redeem_by_slug.get(slug, 0.0)
        has_redeem = slug in redeem_by_slug
        up_sz = up_size_by_slug[slug]
        dn_sz = dn_size_by_slug[slug]
        winner: str | None = None
        if has_redeem:
            if up_sz > 0 and abs(redeem - up_sz) < 1.0 and abs(redeem - up_sz) <= abs(redeem - dn_sz):
                winner = "Up"
            elif dn_sz > 0 and abs(redeem - dn_sz) < 1.0:
                winner = "Down"
        markets.append({
            "slug": slug,
            "title": title_by_slug.get(slug, ""),
            "coin": get_coin(slug),
            "dur": get_dur(slug),
            "spent": round(spent, 4),
            "up_size": round(up_sz, 4),
            "dn_size": round(dn_sz, 4),
            "redeem": round(redeem, 4),
            "has_redeem": has_redeem,
            "pnl": round(redeem - spent, 4) if has_redeem else None,
            "winner": winner,
            "n_trades": n_trades_by_slug[slug],
        })

    def aggregate(items: list[dict[str, Any]]) -> dict[str, Any]:
        if not items:
            return {}
        resolved = [m for m in items if m["has_redeem"]]
        spent_total = sum(m["spent"] for m in items)
        spent_res = sum(m["spent"] for m in resolved)
        redeem_total = sum(m["redeem"] for m in resolved)
        pnl_total = redeem_total - spent_res
        wins = [m for m in resolved if (m["pnl"] or 0) > 0]
        losses = [m for m in resolved if (m["pnl"] or 0) < 0]
        roi = pnl_total / spent_res if spent_res else 0.0
        winrate = len(wins) / len(resolved) if resolved else 0.0
        avg_win = sum(m["pnl"] for m in wins) / len(wins) if wins else 0.0
        avg_loss = sum(m["pnl"] for m in losses) / len(losses) if losses else 0.0
        return {
            "n_markets": len(items),
            "n_resolved": len(resolved),
            "n_unresolved": len(items) - len(resolved),
            "spent_total": round(spent_total, 2),
            "spent_resolved": round(spent_res, 2),
            "redeem_resolved": round(redeem_total, 2),
            "pnl": round(pnl_total, 2),
            "roi_pct": round(roi * 100, 4),
            "winrate_pct": round(winrate * 100, 2),
            "wins": len(wins),
            "losses": len(losses),
            "avg_win": round(avg_win, 2),
            "avg_loss": round(avg_loss, 2),
            "avg_pnl_per_market": round(pnl_total / len(resolved), 2) if resolved else 0.0,
            "profit_factor": round(sum(m["pnl"] for m in wins) / abs(sum(m["pnl"] for m in losses)), 4) if losses else None,
        }

    payload: dict[str, Any] = {
        "method": "PnL = REDEEM (activity[type=REDEEM].usdcSize) - spent (Σ trades.size × trades.price)",
        "all": aggregate(markets),
        "by_dur": {dur: aggregate([m for m in markets if m["dur"] == dur]) for dur in ("5m", "15m", "1h", "4h")},
        "by_coin_dur": {
            f"{coin}_{dur}": aggregate([m for m in markets if m["coin"] == coin and m["dur"] == dur])
            for coin in ("BTC", "ETH", "SOL", "XRP")
            for dur in ("5m", "15m", "1h", "4h")
        },
        "markets": markets,
    }

    OUT.write_text(json.dumps(payload, indent=2))
    a = payload["all"]
    print(f"Wrote {OUT}")
    print(f"\nALL: spent ${a['spent_resolved']:,.0f} → redeem ${a['redeem_resolved']:,.0f} → PnL ${a['pnl']:+,.0f}  ROI {a['roi_pct']:+.2f}%  Winrate {a['winrate_pct']:.1f}%")


if __name__ == "__main__":
    main()
