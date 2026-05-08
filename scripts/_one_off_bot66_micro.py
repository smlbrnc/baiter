"""Bot 66 mikro-davranış analizi — 6 kriter.

1. Eşik değeri (entry için fiyat tavanı)
2. Sizing fonksiyonu (trade size dağılımı, blok analizi)
3. Second-leg gevşemesi (karşı tarafa ilk geçişe kadar süre)
4. Cancel-replace ritmi / GTC vs FAK (zamansal yoğunluk paterni)
5. Same-second multi-fill (FAK kanıtı)
6. T-cutoff kesinliği (son trade ile close arası dağılım)

Sadece 5m + 15m bucket'larında çalışır (kullanıcı talebine uygun).
"""

from __future__ import annotations

import json
import re
import statistics
from collections import Counter, defaultdict
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parent.parent
SRC = ROOT / "data" / "bot66_analysis_summary.json"
LOG = ROOT / "data" / "new-bot-log.log"
OUT = ROOT / "data" / "bot66_micro_analysis.json"

PERIOD_SECONDS = {"5m": 300, "15m": 900}


def parse_slug(slug: str) -> tuple[str | None, str | None, int | None]:
    m = re.match(r"(btc|eth|sol|xrp)-updown-(5m|15m)-(\d+)$", slug)
    if m:
        return m.group(1).upper(), m.group(2), int(m.group(3))
    return None, None, None


def histogram(values: list[float], bins: list[float]) -> list[int]:
    counts = [0] * (len(bins) - 1)
    for v in values:
        placed = False
        for i in range(len(bins) - 1):
            if bins[i] <= v < bins[i + 1]:
                counts[i] += 1
                placed = True
                break
        if not placed and v >= bins[-1]:
            counts[-1] += 1
    return counts


def percentiles(values: list[float], ps: list[float]) -> dict[str, float]:
    if not values:
        return {f"p{p}": 0.0 for p in ps}
    s = sorted(values)
    out = {}
    for p in ps:
        k = (len(s) - 1) * (p / 100.0)
        f = int(k)
        c = min(f + 1, len(s) - 1)
        out[f"p{int(p)}"] = round(s[f] + (s[c] - s[f]) * (k - f), 4)
    return out


def main() -> None:
    raw = json.loads(LOG.read_text())
    trades = raw["trades"]

    by_slug: dict[str, list[dict]] = defaultdict(list)
    for t in trades:
        slug = t["slug"]
        coin, dur, open_ts = parse_slug(slug)
        if dur not in ("5m", "15m"):
            continue
        by_slug[slug].append({
            "ts": int(t["timestamp"]),
            "outcome": t["outcome"],
            "size": float(t["size"]),
            "price": float(t["price"]),
            "tx": t.get("transactionHash", ""),
            "open_ts": open_ts,
            "dur": dur,
            "coin": coin,
        })

    print(f"Slugs (5m+15m): {len(by_slug)}")
    print(f"Total trades:   {sum(len(v) for v in by_slug.values())}")

    out: dict[str, Any] = {}

    # ─── 1. EŞİK DEĞERİ — entry price ceiling ────────────────────────────────
    # Hangi fiyat aralıklarında BUY ediyor? İlk trade'lerin fiyat dağılımı.
    print("\n=== 1. ENTRY PRICE THRESHOLD ===")
    entry_prices_5m_first = []
    entry_prices_15m_first = []
    entry_prices_5m_all = []
    entry_prices_15m_all = []
    cap_breaches_5m = 0  # >= 0.95
    cap_breaches_15m = 0
    over_50_5m = 0
    over_50_15m = 0
    over_70_5m = 0
    over_70_15m = 0
    for slug, ts in by_slug.items():
        ts_sorted = sorted(ts, key=lambda x: x["ts"])
        first_per_outcome: dict[str, dict] = {}
        for tr in ts_sorted:
            if tr["outcome"] in ("Up", "Down") and tr["outcome"] not in first_per_outcome:
                first_per_outcome[tr["outcome"]] = tr
        target = entry_prices_5m_first if ts[0]["dur"] == "5m" else entry_prices_15m_first
        for tr in first_per_outcome.values():
            target.append(tr["price"])
        for tr in ts_sorted:
            if tr["outcome"] not in ("Up", "Down"):
                continue
            if tr["dur"] == "5m":
                entry_prices_5m_all.append(tr["price"])
                if tr["price"] >= 0.95:
                    cap_breaches_5m += 1
                if tr["price"] > 0.5:
                    over_50_5m += 1
                if tr["price"] > 0.7:
                    over_70_5m += 1
            else:
                entry_prices_15m_all.append(tr["price"])
                if tr["price"] >= 0.95:
                    cap_breaches_15m += 1
                if tr["price"] > 0.5:
                    over_50_15m += 1
                if tr["price"] > 0.7:
                    over_70_15m += 1

    bins_px = [0.0, 0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9, 1.01]
    out["threshold"] = {
        "first_entry_per_outcome_5m": {
            "n": len(entry_prices_5m_first),
            "mean": round(statistics.mean(entry_prices_5m_first), 4) if entry_prices_5m_first else None,
            "median": round(statistics.median(entry_prices_5m_first), 4) if entry_prices_5m_first else None,
            "max": round(max(entry_prices_5m_first), 4) if entry_prices_5m_first else None,
            **percentiles(entry_prices_5m_first, [25, 50, 75, 90, 95, 99]),
            "histogram": dict(zip([f"{b:.1f}-{bins_px[i+1]:.1f}" for i, b in enumerate(bins_px[:-1])], histogram(entry_prices_5m_first, bins_px))),
        },
        "first_entry_per_outcome_15m": {
            "n": len(entry_prices_15m_first),
            "mean": round(statistics.mean(entry_prices_15m_first), 4) if entry_prices_15m_first else None,
            "median": round(statistics.median(entry_prices_15m_first), 4) if entry_prices_15m_first else None,
            "max": round(max(entry_prices_15m_first), 4) if entry_prices_15m_first else None,
            **percentiles(entry_prices_15m_first, [25, 50, 75, 90, 95, 99]),
            "histogram": dict(zip([f"{b:.1f}-{bins_px[i+1]:.1f}" for i, b in enumerate(bins_px[:-1])], histogram(entry_prices_15m_first, bins_px))),
        },
        "all_trades_5m": {
            "n": len(entry_prices_5m_all),
            "mean": round(statistics.mean(entry_prices_5m_all), 4) if entry_prices_5m_all else None,
            "median": round(statistics.median(entry_prices_5m_all), 4) if entry_prices_5m_all else None,
            **percentiles(entry_prices_5m_all, [25, 50, 75, 90, 95, 99]),
            "trades_over_0_50": over_50_5m,
            "trades_over_0_70": over_70_5m,
            "trades_over_0_95": cap_breaches_5m,
        },
        "all_trades_15m": {
            "n": len(entry_prices_15m_all),
            "mean": round(statistics.mean(entry_prices_15m_all), 4) if entry_prices_15m_all else None,
            "median": round(statistics.median(entry_prices_15m_all), 4) if entry_prices_15m_all else None,
            **percentiles(entry_prices_15m_all, [25, 50, 75, 90, 95, 99]),
            "trades_over_0_50": over_50_15m,
            "trades_over_0_70": over_70_15m,
            "trades_over_0_95": cap_breaches_15m,
        },
    }
    print(f"  5m first entries — n={len(entry_prices_5m_first)} mean={out['threshold']['first_entry_per_outcome_5m']['mean']} max={out['threshold']['first_entry_per_outcome_5m']['max']}")
    print(f"  15m first entries — n={len(entry_prices_15m_first)} mean={out['threshold']['first_entry_per_outcome_15m']['mean']} max={out['threshold']['first_entry_per_outcome_15m']['max']}")
    print(f"  5m all trades >0.95: {cap_breaches_5m}/{len(entry_prices_5m_all)}, >0.70: {over_70_5m}")
    print(f"  15m all trades >0.95: {cap_breaches_15m}/{len(entry_prices_15m_all)}, >0.70: {over_70_15m}")

    # ─── 2. SIZING FONKSİYONU — trade size dağılımı ──────────────────────────
    print("\n=== 2. SIZING FUNCTION ===")
    sizes_5m = [tr["size"] for ts in by_slug.values() for tr in ts if ts[0]["dur"] == "5m"]
    sizes_15m = [tr["size"] for ts in by_slug.values() for tr in ts if ts[0]["dur"] == "15m"]
    # Coin bazında
    sizes_by_coin: dict[str, list[float]] = defaultdict(list)
    notional_by_coin: dict[str, list[float]] = defaultdict(list)
    for slug, ts in by_slug.items():
        for tr in ts:
            sizes_by_coin[tr["coin"]].append(tr["size"])
            notional_by_coin[tr["coin"]].append(tr["size"] * tr["price"])

    bins_sz = [0, 1, 5, 10, 20, 50, 100, 200, 500, 1000, 5000]
    out["sizing"] = {
        "all_5m": {
            "n": len(sizes_5m),
            "mean": round(statistics.mean(sizes_5m), 2),
            "median": round(statistics.median(sizes_5m), 2),
            "stdev": round(statistics.stdev(sizes_5m), 2) if len(sizes_5m) > 1 else 0,
            "min": round(min(sizes_5m), 2),
            "max": round(max(sizes_5m), 2),
            **percentiles(sizes_5m, [10, 25, 50, 75, 90, 95, 99]),
            "histogram": dict(zip([f"{bins_sz[i]}-{bins_sz[i+1]}" for i in range(len(bins_sz)-1)], histogram(sizes_5m, bins_sz))),
        },
        "all_15m": {
            "n": len(sizes_15m),
            "mean": round(statistics.mean(sizes_15m), 2),
            "median": round(statistics.median(sizes_15m), 2),
            "stdev": round(statistics.stdev(sizes_15m), 2) if len(sizes_15m) > 1 else 0,
            "min": round(min(sizes_15m), 2),
            "max": round(max(sizes_15m), 2),
            **percentiles(sizes_15m, [10, 25, 50, 75, 90, 95, 99]),
            "histogram": dict(zip([f"{bins_sz[i]}-{bins_sz[i+1]}" for i in range(len(bins_sz)-1)], histogram(sizes_15m, bins_sz))),
        },
        "per_coin": {
            coin: {
                "n": len(sizes),
                "mean_size": round(statistics.mean(sizes), 2),
                "median_size": round(statistics.median(sizes), 2),
                "max_size": round(max(sizes), 2),
                "mean_usdc": round(statistics.mean(notional_by_coin[coin]), 2),
                "median_usdc": round(statistics.median(notional_by_coin[coin]), 2),
                "max_usdc": round(max(notional_by_coin[coin]), 2),
            }
            for coin, sizes in sizes_by_coin.items()
        },
    }
    print(f"  5m sizes — mean={out['sizing']['all_5m']['mean']} median={out['sizing']['all_5m']['median']} max={out['sizing']['all_5m']['max']}")
    print(f"  15m sizes — mean={out['sizing']['all_15m']['mean']} median={out['sizing']['all_15m']['median']} max={out['sizing']['all_15m']['max']}")

    # ─── 3. SECOND-LEG GEVŞEMESİ ─────────────────────────────────────────────
    # Bir slug'da ilk trade tek bir outcome'ta (örn Down) gerçekleşir.
    # İkinci leg = ilk karşı tarafa BUY emri. Aradaki süre + bu sırada karşı tarafın
    # son fiyatı / kendi tarafın son fiyatı. Karşı taraf ne kadar düşmüş?
    print("\n=== 3. SECOND-LEG OPENING ===")
    second_leg_records = []
    for slug, ts in by_slug.items():
        ts_sorted = sorted(ts, key=lambda x: x["ts"])
        if not ts_sorted:
            continue
        first = ts_sorted[0]
        if first["outcome"] not in ("Up", "Down"):
            continue
        first_side = first["outcome"]
        opp = "Up" if first_side == "Down" else "Down"
        # First opp trade
        opp_first = next((tr for tr in ts_sorted if tr["outcome"] == opp), None)
        if not opp_first:
            continue
        # Last same-side trade BEFORE opp_first
        same_before_opp = [tr for tr in ts_sorted if tr["ts"] <= opp_first["ts"] and tr["outcome"] == first_side]
        if not same_before_opp:
            continue
        last_same = same_before_opp[-1]
        delay = opp_first["ts"] - first["ts"]
        # px movement of own side from first to opp_first
        px_first = first["price"]
        px_last_same = last_same["price"]
        px_opp_first = opp_first["price"]
        # ratio: implied complement check
        # if same-side price went UP a lot before bot flipped, that's "guard": waited for own side to be expensive
        own_side_movement = px_last_same - px_first
        # Trades in between (counting same-side accumulation)
        in_between = [tr for tr in ts_sorted if first["ts"] < tr["ts"] < opp_first["ts"]]
        same_in_between = sum(1 for tr in in_between if tr["outcome"] == first_side)
        second_leg_records.append({
            "slug": slug,
            "dur": first["dur"],
            "first_side": first_side,
            "delay_sec": delay,
            "same_side_first_px": px_first,
            "same_side_last_px_before_flip": px_last_same,
            "opp_first_px": px_opp_first,
            "own_movement": round(own_side_movement, 4),
            "n_same_before_flip": same_in_between + 1,
        })

    # Bucket by dur
    by_dur_sl: dict[str, list[dict]] = defaultdict(list)
    for r in second_leg_records:
        by_dur_sl[r["dur"]].append(r)

    delay_bins = [0, 5, 10, 20, 30, 60, 120, 300, 600, 900]
    out["second_leg"] = {}
    for dur in ("5m", "15m"):
        items = by_dur_sl[dur]
        if not items:
            continue
        delays = [r["delay_sec"] for r in items]
        own_moves = [r["own_movement"] for r in items]
        opp_pxs = [r["opp_first_px"] for r in items]
        out["second_leg"][dur] = {
            "n": len(items),
            "delay_sec": {
                "mean": round(statistics.mean(delays), 1),
                "median": round(statistics.median(delays), 1),
                "min": min(delays),
                "max": max(delays),
                **percentiles(delays, [25, 50, 75, 90, 95]),
                "histogram": dict(zip([f"{delay_bins[i]}-{delay_bins[i+1]}" for i in range(len(delay_bins)-1)], histogram(delays, delay_bins))),
            },
            "own_side_movement": {
                "mean": round(statistics.mean(own_moves), 4),
                "median": round(statistics.median(own_moves), 4),
                **percentiles(own_moves, [25, 50, 75]),
                "n_positive": sum(1 for v in own_moves if v > 0),
                "n_negative": sum(1 for v in own_moves if v < 0),
            },
            "opp_first_px": {
                "mean": round(statistics.mean(opp_pxs), 4),
                "median": round(statistics.median(opp_pxs), 4),
                **percentiles(opp_pxs, [25, 50, 75, 90]),
            },
        }
        print(f"  {dur} second-leg: n={len(items)} delay_med={statistics.median(delays):.1f}s opp_first_px_med={statistics.median(opp_pxs):.3f}")

    # ─── 4 & 5. CANCEL-REPLACE / FAK KANITI: aynı saniye multi-fill ─────────
    print("\n=== 4-5. SAME-SECOND MULTI-FILL (FAK evidence) ===")
    same_sec_groups_5m = []
    same_sec_groups_15m = []
    same_sec_per_slug: dict[str, list[int]] = defaultdict(list)
    for slug, ts in by_slug.items():
        ts_sorted = sorted(ts, key=lambda x: x["ts"])
        sec_groups: dict[int, list[dict]] = defaultdict(list)
        for tr in ts_sorted:
            sec_groups[tr["ts"]].append(tr)
        for sec, group in sec_groups.items():
            if ts[0]["dur"] == "5m":
                same_sec_groups_5m.append(len(group))
            else:
                same_sec_groups_15m.append(len(group))
            if len(group) > 1:
                same_sec_per_slug[slug].append(len(group))

    # Tek fill mi yoksa multi-fill yoğunluğu ne?
    def group_stats(groups: list[int]) -> dict:
        if not groups:
            return {}
        cnt = Counter(groups)
        total_fills = sum(g for g in groups)
        multi = sum(g for g in groups if g > 1)
        return {
            "total_seconds_with_trades": len(groups),
            "total_trades": total_fills,
            "single_fill_seconds": cnt[1],
            "multi_fill_seconds": sum(c for n, c in cnt.items() if n > 1),
            "max_fills_per_second": max(groups),
            "fills_in_multi_seconds": multi,
            "ratio_trades_in_multi": round(multi / total_fills, 4) if total_fills else 0,
            "histogram": {f"{n}_fills": c for n, c in sorted(cnt.items()) if c > 0},
        }

    out["multi_fill"] = {
        "5m": group_stats(same_sec_groups_5m),
        "15m": group_stats(same_sec_groups_15m),
    }
    print(f"  5m: {out['multi_fill']['5m']}")
    print(f"  15m: {out['multi_fill']['15m']}")

    # Cancel-replace ritmi: ardışık trade'ler arasındaki süre dağılımı
    # GTC olsa: pasif emir, fill geldiğinde tek bir hash, geniş dağılımlı süreler
    # FAK olsa: hızlı ardışık fill'ler, çok kısa süreler (saniye altı eşitliği gözlemlenemez ama yakın)
    print("\n=== 4. CANCEL-REPLACE RHYTHM ===")
    inter_arrivals_5m = []
    inter_arrivals_15m = []
    for slug, ts in by_slug.items():
        ts_sorted = sorted(ts, key=lambda x: x["ts"])
        for i in range(1, len(ts_sorted)):
            dt = ts_sorted[i]["ts"] - ts_sorted[i-1]["ts"]
            if ts_sorted[0]["dur"] == "5m":
                inter_arrivals_5m.append(dt)
            else:
                inter_arrivals_15m.append(dt)

    arrival_bins = [0, 1, 2, 3, 5, 10, 20, 30, 60, 120, 300, 900]
    out["cancel_replace_rhythm"] = {}
    for label, arr in (("5m", inter_arrivals_5m), ("15m", inter_arrivals_15m)):
        if not arr:
            continue
        out["cancel_replace_rhythm"][label] = {
            "n": len(arr),
            "mean": round(statistics.mean(arr), 2),
            "median": round(statistics.median(arr), 2),
            "n_zero_seconds": sum(1 for v in arr if v == 0),
            "n_within_1s": sum(1 for v in arr if v <= 1),
            "n_within_3s": sum(1 for v in arr if v <= 3),
            "n_over_30s": sum(1 for v in arr if v > 30),
            **percentiles(arr, [10, 25, 50, 75, 90, 95]),
            "histogram": dict(zip([f"{arrival_bins[i]}-{arrival_bins[i+1]}s" for i in range(len(arrival_bins)-1)], histogram(arr, arrival_bins))),
        }
        print(f"  {label}: median {statistics.median(arr):.1f}s, zero={out['cancel_replace_rhythm'][label]['n_zero_seconds']}, ≤1s={out['cancel_replace_rhythm'][label]['n_within_1s']}, >30s={out['cancel_replace_rhythm'][label]['n_over_30s']}")

    # ─── 6. T-CUTOFF ─────────────────────────────────────────────────────────
    print("\n=== 6. T-CUTOFF (last trade to close) ===")
    cutoff_5m = []
    cutoff_15m = []
    for slug, ts in by_slug.items():
        ts_sorted = sorted(ts, key=lambda x: x["ts"])
        last_ts = ts_sorted[-1]["ts"]
        open_ts = ts_sorted[0]["open_ts"]
        if open_ts is None:
            continue
        period = PERIOD_SECONDS.get(ts_sorted[0]["dur"])
        if not period:
            continue
        close_ts = open_ts + period
        cutoff = close_ts - last_ts
        if ts_sorted[0]["dur"] == "5m":
            cutoff_5m.append(cutoff)
        else:
            cutoff_15m.append(cutoff)

    cutoff_bins = [-30, 0, 5, 10, 15, 30, 45, 60, 90, 120, 180, 300, 900]
    out["t_cutoff"] = {}
    for label, vals in (("5m", cutoff_5m), ("15m", cutoff_15m)):
        if not vals:
            continue
        n_pre_close = sum(1 for v in vals if v <= 0)
        n_within_15 = sum(1 for v in vals if 0 < v <= 15)
        n_within_30 = sum(1 for v in vals if 0 < v <= 30)
        n_within_60 = sum(1 for v in vals if 0 < v <= 60)
        n_within_90 = sum(1 for v in vals if 0 < v <= 90)
        n_within_120 = sum(1 for v in vals if 0 < v <= 120)
        out["t_cutoff"][label] = {
            "n": len(vals),
            "mean": round(statistics.mean(vals), 1),
            "median": round(statistics.median(vals), 1),
            "min": min(vals),
            "max": max(vals),
            **percentiles(vals, [10, 25, 50, 75, 90, 95]),
            "n_after_close": n_pre_close,
            "n_within_15s": n_within_15,
            "n_within_30s": n_within_30,
            "n_within_60s": n_within_60,
            "n_within_90s": n_within_90,
            "n_within_120s": n_within_120,
            "histogram": dict(zip([f"{cutoff_bins[i]}-{cutoff_bins[i+1]}" for i in range(len(cutoff_bins)-1)], histogram(vals, cutoff_bins))),
        }
        print(f"  {label}: median {statistics.median(vals):.0f}s, ≤30s={n_within_30}, ≤60s={n_within_60}, ≤90s={n_within_90}, ≤120s={n_within_120}")

    OUT.write_text(json.dumps(out, indent=2))
    print(f"\nWrote {OUT}")


if __name__ == "__main__":
    main()
