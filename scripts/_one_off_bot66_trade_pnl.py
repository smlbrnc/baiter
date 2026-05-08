"""Bot 66 — TRADE-BASED PnL (REDEEM verisi kullanmadan).

Sadece trades dizisi üzerinden 6 farklı PnL yöntemi:

  M1 = Naive 50/50 EV:           (up_sz + dn_sz)/2 − spent
  M2 = Last-price winner:        son Up px > son Dn px → Up wins
  M3 = Max-price (>=0.85) winner: market içindeki en yüksek fiyat sınıfta
  M4 = Mark-to-Last (MTL):       up_sz×last_up_px + dn_sz×last_dn_px − spent
  M5 = Best-case (pnl_max):      max(up_sz, dn_sz) − spent
  M6 = Worst-case (pnl_min):     min(up_sz, dn_sz) − spent

Çıktı: data/bot66_trade_pnl.json
"""

from __future__ import annotations

import json
import re
from collections import defaultdict
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parent.parent
SRC = ROOT / "data" / "new-bot-log.log"
OUT = ROOT / "data" / "bot66_trade_pnl.json"

THRESHOLD_M3 = 0.85


def get_dur(s: str) -> str | None:
    m = re.match(r".*-updown-(5m|15m|1h|4h)-", s)
    if m:
        return m.group(1)
    if "-up-or-down-may-" in s:
        return "1h"
    return None


def get_coin(s: str) -> str | None:
    m = re.match(r"(btc|eth|sol|xrp)-updown-", s)
    if m:
        return m.group(1).upper()
    m2 = re.match(r"(bitcoin|ethereum|solana|xrp)-up-or-down", s)
    if m2:
        return {"bitcoin": "BTC", "ethereum": "ETH", "solana": "SOL", "xrp": "XRP"}[m2.group(1)]
    return None


def main() -> None:
    raw = json.loads(SRC.read_text())
    per_slug: dict[str, list[dict]] = defaultdict(list)
    for t in raw["trades"]:
        per_slug[t["slug"]].append(t)

    markets: list[dict[str, Any]] = []
    for slug, ts in per_slug.items():
        dur = get_dur(slug)
        coin = get_coin(slug)
        if dur is None:
            continue
        ts_sorted = sorted(ts, key=lambda x: int(x["timestamp"]))
        ups = [t for t in ts_sorted if t["outcome"] == "Up"]
        dns = [t for t in ts_sorted if t["outcome"] == "Down"]
        up_sz = sum(float(t["size"]) for t in ups)
        dn_sz = sum(float(t["size"]) for t in dns)
        up_usdc = sum(float(t["size"]) * float(t["price"]) for t in ups)
        dn_usdc = sum(float(t["size"]) * float(t["price"]) for t in dns)
        spent = up_usdc + dn_usdc

        last_up_px = float(ups[-1]["price"]) if ups else 0.0
        last_dn_px = float(dns[-1]["price"]) if dns else 0.0
        max_up_px = max((float(t["price"]) for t in ups), default=0.0)
        max_dn_px = max((float(t["price"]) for t in dns), default=0.0)

        m1 = (up_sz + dn_sz) / 2 - spent
        if last_up_px > last_dn_px:
            m2 = up_sz - spent
            m2_winner = "Up"
        elif last_dn_px > last_up_px:
            m2 = dn_sz - spent
            m2_winner = "Down"
        else:
            m2 = None
            m2_winner = None
        if max_up_px >= THRESHOLD_M3 and max_up_px > max_dn_px:
            m3 = up_sz - spent
            m3_winner = "Up"
        elif max_dn_px >= THRESHOLD_M3 and max_dn_px > max_up_px:
            m3 = dn_sz - spent
            m3_winner = "Down"
        else:
            m3 = None
            m3_winner = None
        m4 = up_sz * last_up_px + dn_sz * last_dn_px - spent
        m5 = max(up_sz, dn_sz) - spent
        m6 = (min(up_sz, dn_sz) - spent) if (up_sz > 0 and dn_sz > 0) else (max(up_sz, dn_sz) - spent)

        markets.append({
            "slug": slug,
            "coin": coin,
            "dur": dur,
            "spent": round(spent, 4),
            "up_size": round(up_sz, 4),
            "dn_size": round(dn_sz, 4),
            "last_up_px": round(last_up_px, 4),
            "last_dn_px": round(last_dn_px, 4),
            "max_up_px": round(max_up_px, 4),
            "max_dn_px": round(max_dn_px, 4),
            "pnl_m1_5050": round(m1, 4),
            "pnl_m2_lastpx": round(m2, 4) if m2 is not None else None,
            "pnl_m2_winner": m2_winner,
            "pnl_m3_maxpx": round(m3, 4) if m3 is not None else None,
            "pnl_m3_winner": m3_winner,
            "pnl_m4_mtl": round(m4, 4),
            "pnl_m5_best": round(m5, 4),
            "pnl_m6_worst": round(m6, 4),
        })

    def aggregate(items: list[dict[str, Any]], key: str) -> dict[str, Any]:
        valid = [m for m in items if m.get(key) is not None]
        if not valid:
            return {"n": 0}
        spent = sum(m["spent"] for m in valid)
        pnl = sum(m[key] for m in valid)
        wins = sum(1 for m in valid if m[key] > 0)
        losses = sum(1 for m in valid if m[key] < 0)
        roi = (pnl / spent * 100) if spent else 0.0
        winrate = (wins / len(valid) * 100) if valid else 0.0
        wins_pnl = [m[key] for m in valid if m[key] > 0]
        losses_pnl = [m[key] for m in valid if m[key] < 0]
        return {
            "n": len(valid),
            "spent": round(spent, 2),
            "pnl": round(pnl, 2),
            "roi_pct": round(roi, 4),
            "winrate_pct": round(winrate, 2),
            "wins": wins,
            "losses": losses,
            "avg_win": round(sum(wins_pnl) / len(wins_pnl), 2) if wins_pnl else 0.0,
            "avg_loss": round(sum(losses_pnl) / len(losses_pnl), 2) if losses_pnl else 0.0,
        }

    methods = [
        ("M1_5050_EV", "pnl_m1_5050"),
        ("M2_LastPx_Winner", "pnl_m2_lastpx"),
        ("M3_MaxPx_Winner", "pnl_m3_maxpx"),
        ("M4_MarkToLast", "pnl_m4_mtl"),
        ("M5_BestCase", "pnl_m5_best"),
        ("M6_WorstCase", "pnl_m6_worst"),
    ]

    payload: dict[str, Any] = {
        "method_descriptions": {
            "M1_5050_EV": "Naive: her sonuç eşit olasılıkla (50/50). PnL = (up_sz + dn_sz)/2 − spent",
            "M2_LastPx_Winner": "Bot'un son Up trade px > son Down trade px ise Up wins; payout = o side'ın size'ı",
            "M3_MaxPx_Winner": f"Market içindeki en yüksek fiyat ≥ {THRESHOLD_M3} olan side wins (else: kayıt dışı)",
            "M4_MarkToLast": "Mark-to-last: pozisyonu son trade fiyatından kapasaydı değeri. PnL = up_sz×last_up_px + dn_sz×last_dn_px − spent",
            "M5_BestCase": "İdealist üst sınır: en büyük side kazandı varsayımı (pnl_max)",
            "M6_WorstCase": "Pesimist alt sınır: en küçük side kazandı varsayımı (pnl_min)",
        },
        "all": {name: aggregate(markets, key) for name, key in methods},
        "by_dur": {
            dur: {name: aggregate([m for m in markets if m["dur"] == dur], key) for name, key in methods}
            for dur in ("5m", "15m", "1h", "4h")
        },
        "by_coin_dur": {
            f"{coin}_{dur}": {name: aggregate([m for m in markets if m["coin"] == coin and m["dur"] == dur], key) for name, key in methods}
            for coin in ("BTC", "ETH", "SOL", "XRP")
            for dur in ("5m", "15m", "1h", "4h")
        },
        "markets": markets,
    }

    OUT.write_text(json.dumps(payload, indent=2))
    print(f"Wrote {OUT}\n")
    print(f"{'METHOD':<22} {'n':>4} {'PnL':>12} {'ROI':>9} {'Winrate':>9}")
    for name, key in methods:
        a = payload["all"][name]
        print(f"  {name:<20} {a['n']:>4} ${a['pnl']:>+11,.2f} {a['roi_pct']:>+7.2f}% {a['winrate_pct']:>7.1f}%")


if __name__ == "__main__":
    main()
