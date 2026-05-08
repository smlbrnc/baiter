"""Tek seferlik özet: data/new-bot-log.log -> bot66_analysis.data.ts + analysis_summary.json.

5m + 15m bucket'larındaki market'leri ve tick zincirlerini canvas'a embed edilebilir
JSON formatına özetler. Markdown rapor için ekstra istatistikler de üretir.
"""

from __future__ import annotations

import json
import math
import re
import statistics
from collections import defaultdict
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parent.parent
SRC = ROOT / "data" / "new-bot-log.log"
TS_OUT = ROOT / "bot66_analysis.data.ts"
JSON_OUT = ROOT / "data" / "bot66_analysis_summary.json"

PERIOD_SECONDS = {"5m": 300, "15m": 900, "1h": 3600, "4h": 14400}


def parse_slug(slug: str) -> tuple[str | None, str | None, int | None]:
    m = re.match(r"(btc|eth|sol|xrp)-updown-(5m|15m|1h|4h)-(\d+)$", slug)
    if m:
        return m.group(1).upper(), m.group(2), int(m.group(3))
    m2 = re.match(r"(bitcoin|ethereum|solana|xrp)-up-or-down-may-\d+-\d+-(\d+)am-et$", slug)
    if m2:
        return {"bitcoin": "BTC", "ethereum": "ETH", "solana": "SOL", "xrp": "XRP"}[m2.group(1)], "1h", None
    return None, None, None


def main() -> None:
    raw = json.loads(SRC.read_text())
    trades = raw["trades"]

    per_slug: dict[str, dict[str, Any]] = defaultdict(
        lambda: {"trades": [], "title": "", "icon": "", "coin": None, "dur": None, "open_ts": None}
    )
    for t in trades:
        slug = t["slug"]
        rec = per_slug[slug]
        rec["title"] = t.get("title", "")
        rec["icon"] = t.get("icon", "")
        coin, dur, open_ts = parse_slug(slug)
        if coin:
            rec["coin"] = coin
            rec["dur"] = dur
            rec["open_ts"] = open_ts
        rec["trades"].append(
            {
                "ts": int(t["timestamp"]),
                "outcome": t["outcome"],
                "size": float(t["size"]),
                "price": float(t["price"]),
            }
        )

    def aggregate(slug: str, rec: dict[str, Any]) -> dict[str, Any] | None:
        ups = [tr for tr in rec["trades"] if tr["outcome"] == "Up"]
        dns = [tr for tr in rec["trades"] if tr["outcome"] == "Down"]
        if not ups and not dns:
            return None
        up_sz = sum(tr["size"] for tr in ups)
        dn_sz = sum(tr["size"] for tr in dns)
        up_usdc = sum(tr["size"] * tr["price"] for tr in ups)
        dn_usdc = sum(tr["size"] * tr["price"] for tr in dns)
        spent = up_usdc + dn_usdc
        avg_up = up_usdc / up_sz if up_sz else 0.0
        avg_dn = dn_usdc / dn_sz if dn_sz else 0.0
        sum_avg = avg_up + avg_dn if (up_sz and dn_sz) else None
        balance = (
            min(up_sz, dn_sz) / max(up_sz, dn_sz) if (up_sz and dn_sz) else 0.0
        )
        all_sorted = sorted(rec["trades"], key=lambda tr: tr["ts"])
        first_ts = all_sorted[0]["ts"]
        last_ts = all_sorted[-1]["ts"]
        first_side = "Up" if all_sorted[0]["outcome"] == "Up" else "Down" if all_sorted[0]["outcome"] == "Down" else None
        open_ts = rec["open_ts"]
        period = PERIOD_SECONDS.get(rec["dur"] or "", 0)
        close_ts = open_ts + period if open_ts else None
        time_from_open = (first_ts - open_ts) if open_ts else None
        time_to_close = (close_ts - last_ts) if close_ts else None
        n_trades = len(rec["trades"])
        min_payout = min(up_sz, dn_sz) if (up_sz and dn_sz) else 0.0
        max_payout = max(up_sz, dn_sz)
        pnl_min = min_payout - spent if (up_sz and dn_sz) else (max_payout - spent)
        pnl_max = max_payout - spent
        return {
            "slug": slug,
            "title": rec["title"],
            "icon": rec["icon"],
            "coin": rec["coin"],
            "dur": rec["dur"],
            "open_ts": open_ts,
            "close_ts": close_ts,
            "first_ts": first_ts,
            "last_ts": last_ts,
            "time_from_open": time_from_open,
            "time_to_close": time_to_close,
            "n_trades": n_trades,
            "n_up": len(ups),
            "n_dn": len(dns),
            "up_size": round(up_sz, 4),
            "dn_size": round(dn_sz, 4),
            "up_usdc": round(up_usdc, 4),
            "dn_usdc": round(dn_usdc, 4),
            "spent": round(spent, 4),
            "avg_up_price": round(avg_up, 4) if up_sz else None,
            "avg_dn_price": round(avg_dn, 4) if dn_sz else None,
            "sum_avg_price": round(sum_avg, 4) if sum_avg is not None else None,
            "balance": round(balance, 4),
            "first_side": first_side,
            "min_payout": round(min_payout, 4),
            "max_payout": round(max_payout, 4),
            "pnl_min": round(pnl_min, 4),
            "pnl_max": round(pnl_max, 4),
        }

    aggregates: list[dict[str, Any]] = []
    for slug, rec in per_slug.items():
        agg = aggregate(slug, rec)
        if agg:
            aggregates.append(agg)

    by_dur: dict[str, list[dict[str, Any]]] = defaultdict(list)
    for a in aggregates:
        if a["dur"]:
            by_dur[a["dur"]].append(a)

    def safe_mean(vals: list[float]) -> float | None:
        return round(statistics.mean(vals), 4) if vals else None

    def bucket_summary(items: list[dict[str, Any]]) -> dict[str, Any]:
        two_sided = [x for x in items if x["n_up"] > 0 and x["n_dn"] > 0]
        if not two_sided:
            return {}
        spent_total = sum(x["spent"] for x in items)
        sum_avg_vals = [x["sum_avg_price"] for x in two_sided if x["sum_avg_price"] is not None]
        bal_vals = [x["balance"] for x in two_sided]
        first_open_vals = [x["time_from_open"] for x in two_sided if x["time_from_open"] is not None]
        last_close_vals = [x["time_to_close"] for x in two_sided if x["time_to_close"] is not None]
        n_trades_vals = [x["n_trades"] for x in two_sided]
        pnl_min_vals = [x["pnl_min"] for x in two_sided]
        pnl_max_vals = [x["pnl_max"] for x in two_sided]
        first_side_counts = {"Up": 0, "Down": 0}
        for x in two_sided:
            if x["first_side"] in first_side_counts:
                first_side_counts[x["first_side"]] += 1
        arb_count = sum(1 for v in sum_avg_vals if v < 1.0)
        guaranteed_profit = sum(1 for x in two_sided if x["pnl_min"] >= 0)
        return {
            "n_markets": len(items),
            "n_two_sided": len(two_sided),
            "spent_total": round(spent_total, 2),
            "spent_avg": safe_mean([x["spent"] for x in two_sided]),
            "sum_avg_mean": safe_mean(sum_avg_vals),
            "sum_avg_median": round(statistics.median(sum_avg_vals), 4) if sum_avg_vals else None,
            "balance_mean": safe_mean(bal_vals),
            "first_open_mean": safe_mean(first_open_vals),
            "last_close_mean": safe_mean(last_close_vals),
            "n_trades_mean": safe_mean(n_trades_vals),
            "pnl_min_mean": safe_mean(pnl_min_vals),
            "pnl_max_mean": safe_mean(pnl_max_vals),
            "arb_count": arb_count,
            "arb_ratio": round(arb_count / len(two_sided), 4),
            "guaranteed_profit_count": guaranteed_profit,
            "first_side_counts": first_side_counts,
        }

    duration_summary = {dur: bucket_summary(items) for dur, items in by_dur.items()}

    by_coin_dur: dict[tuple[str, str], list[dict[str, Any]]] = defaultdict(list)
    for a in aggregates:
        if a["coin"] and a["dur"]:
            by_coin_dur[(a["coin"], a["dur"])].append(a)

    coin_dur_summary: list[dict[str, Any]] = []
    for (coin, dur), items in sorted(by_coin_dur.items()):
        two = [x for x in items if x["n_up"] > 0 and x["n_dn"] > 0]
        if not two:
            continue
        coin_dur_summary.append(
            {
                "coin": coin,
                "dur": dur,
                "n_markets": len(items),
                "n_two_sided": len(two),
                "n_trades_total": sum(x["n_trades"] for x in items),
                "spent_total": round(sum(x["spent"] for x in items), 2),
                "spent_avg": round(statistics.mean(x["spent"] for x in two), 2),
                "sum_avg_mean": round(statistics.mean(x["sum_avg_price"] for x in two), 4),
                "balance_mean": round(statistics.mean(x["balance"] for x in two), 4),
                "first_side_up": sum(1 for x in two if x["first_side"] == "Up"),
                "first_side_dn": sum(1 for x in two if x["first_side"] == "Down"),
                "arb_count": sum(1 for x in two if x["sum_avg_price"] is not None and x["sum_avg_price"] < 1.0),
                "guaranteed_profit": sum(1 for x in two if x["pnl_min"] >= 0),
            }
        )

    def slim_market(a: dict[str, Any]) -> dict[str, Any]:
        return {
            k: a[k]
            for k in (
                "slug",
                "title",
                "coin",
                "dur",
                "open_ts",
                "close_ts",
                "first_ts",
                "last_ts",
                "time_from_open",
                "time_to_close",
                "n_trades",
                "n_up",
                "n_dn",
                "up_size",
                "dn_size",
                "up_usdc",
                "dn_usdc",
                "spent",
                "avg_up_price",
                "avg_dn_price",
                "sum_avg_price",
                "balance",
                "first_side",
                "min_payout",
                "max_payout",
                "pnl_min",
                "pnl_max",
            )
        }

    markets_5m = sorted(
        [slim_market(a) for a in aggregates if a["dur"] == "5m"],
        key=lambda x: x["open_ts"] or 0,
    )
    markets_15m = sorted(
        [slim_market(a) for a in aggregates if a["dur"] == "15m"],
        key=lambda x: x["open_ts"] or 0,
    )

    def tick_chain(slug: str) -> list[dict[str, Any]]:
        rec = per_slug[slug]
        ts_sorted = sorted(rec["trades"], key=lambda tr: tr["ts"])
        cum_up = 0.0
        cum_dn = 0.0
        spent = 0.0
        out = []
        for tr in ts_sorted:
            if tr["outcome"] == "Up":
                cum_up += tr["size"]
            elif tr["outcome"] == "Down":
                cum_dn += tr["size"]
            spent += tr["size"] * tr["price"]
            out.append(
                {
                    "ts": tr["ts"],
                    "outcome": tr["outcome"],
                    "size": round(tr["size"], 4),
                    "price": round(tr["price"], 4),
                    "cum_up": round(cum_up, 4),
                    "cum_dn": round(cum_dn, 4),
                    "spent": round(spent, 4),
                }
            )
        return out

    chains_5m = {m["slug"]: tick_chain(m["slug"]) for m in markets_5m}
    chains_15m = {m["slug"]: tick_chain(m["slug"]) for m in markets_15m}

    case_study_slug = "eth-updown-5m-1778242200"
    case_study = {
        "slug": case_study_slug,
        "agg": next((a for a in aggregates if a["slug"] == case_study_slug), None),
        "chain": tick_chain(case_study_slug) if case_study_slug in per_slug else [],
    }

    sum_avg_5m = [m["sum_avg_price"] for m in markets_5m if m["sum_avg_price"] is not None]
    sum_avg_15m = [m["sum_avg_price"] for m in markets_15m if m["sum_avg_price"] is not None]
    bal_5m = [m["balance"] for m in markets_5m if m["balance"] > 0]
    bal_15m = [m["balance"] for m in markets_15m if m["balance"] > 0]
    open_5m = [m["time_from_open"] for m in markets_5m if m["time_from_open"] is not None]
    open_15m = [m["time_from_open"] for m in markets_15m if m["time_from_open"] is not None]
    close_5m = [m["time_to_close"] for m in markets_5m if m["time_to_close"] is not None]
    close_15m = [m["time_to_close"] for m in markets_15m if m["time_to_close"] is not None]

    def histogram(values: list[float], bins: list[float]) -> list[int]:
        counts = [0] * (len(bins) - 1)
        for v in values:
            for i in range(len(bins) - 1):
                if bins[i] <= v < bins[i + 1]:
                    counts[i] += 1
                    break
            else:
                if math.isclose(v, bins[-1]):
                    counts[-1] += 1
        return counts

    sum_avg_bins = [round(0.6 + 0.05 * i, 2) for i in range(0, 17)]
    bal_bins = [round(0.0 + 0.05 * i, 2) for i in range(0, 21)]
    open_bins = [0, 15, 30, 60, 90, 120, 180, 240, 300, 600, 900]
    close_bins = [0, 15, 30, 60, 90, 120, 180, 240, 300, 600, 900]

    histograms = {
        "sum_avg_bins": sum_avg_bins,
        "sum_avg_5m": histogram(sum_avg_5m, sum_avg_bins),
        "sum_avg_15m": histogram(sum_avg_15m, sum_avg_bins),
        "balance_bins": bal_bins,
        "balance_5m": histogram(bal_5m, bal_bins),
        "balance_15m": histogram(bal_15m, bal_bins),
        "open_bins": open_bins,
        "open_5m": histogram(open_5m, open_bins),
        "open_15m": histogram(open_15m, open_bins),
        "close_bins": close_bins,
        "close_5m": histogram(close_5m, close_bins),
        "close_15m": histogram(close_15m, close_bins),
    }

    overall = {
        "exported_at_utc": raw.get("exported_at_utc"),
        "window_start_unix": raw.get("window_start_unix"),
        "window_end_unix": raw.get("window_end_unix"),
        "wallet": "0xb55fa1296e6ec55d0ce53d93b9237389f11764d4",
        "pseudonym": "Lively-Authenticity",
        "total_trades_in_log": len(trades),
        "total_buy": sum(1 for t in trades if t["side"] == "BUY"),
        "total_sell": sum(1 for t in trades if t["side"] == "SELL"),
        "distinct_slugs": len({t["slug"] for t in trades}),
        "distinct_slugs_two_sided": sum(1 for a in aggregates if a["n_up"] > 0 and a["n_dn"] > 0),
        "distinct_slugs_only_up": sum(1 for a in aggregates if a["n_up"] > 0 and a["n_dn"] == 0),
        "distinct_slugs_only_dn": sum(1 for a in aggregates if a["n_up"] == 0 and a["n_dn"] > 0),
    }

    payload = {
        "overall": overall,
        "duration_summary": duration_summary,
        "coin_dur_summary": coin_dur_summary,
        "markets_5m": markets_5m,
        "markets_15m": markets_15m,
        "chains_5m": chains_5m,
        "chains_15m": chains_15m,
        "case_study": case_study,
        "histograms": histograms,
    }

    JSON_OUT.write_text(json.dumps(payload, indent=2))
    print(f"Wrote {JSON_OUT}  ({JSON_OUT.stat().st_size} bytes)")

    canvas_payload = {
        "overall": overall,
        "duration_summary": duration_summary,
        "coin_dur_summary": coin_dur_summary,
        "markets_5m": markets_5m,
        "markets_15m": markets_15m,
        "chains_5m": chains_5m,
        "chains_15m": chains_15m,
        "case_study": case_study,
        "histograms": histograms,
    }
    ts_text = (
        "// AUTO-GENERATED by scripts/_one_off_bot66_summarize.py — do not edit.\n"
        "// Source: data/new-bot-log.log\n"
        "export const BOT66_DATA = "
        + json.dumps(canvas_payload, indent=2)
        + " as const;\n"
        "\n"
        "export type Bot66Data = typeof BOT66_DATA;\n"
    )
    TS_OUT.write_text(ts_text)
    print(f"Wrote {TS_OUT}  ({TS_OUT.stat().st_size} bytes)")


if __name__ == "__main__":
    main()
