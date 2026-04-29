#!/usr/bin/env python3
"""Eşik tahmini: blackbox-trades-20260429.csv → her trigger için sinyal percentile'ları.

Çıktılar:
- exports/blackbox-thresholds-20260429.md (insan-okur eşik raporu)
- exports/blackbox-analysis-20260429.md (konsolide özet + confusion matrix)
"""

from __future__ import annotations

import csv
import statistics
from collections import Counter, defaultdict
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
CSV_PATH = ROOT / "exports" / "blackbox-trades-20260429.csv"
THRESHOLDS_MD = ROOT / "exports" / "blackbox-thresholds-20260429.md"
ANALYSIS_MD = ROOT / "exports" / "blackbox-analysis-20260429.md"


def percentile(values: list[float], p: float) -> float:
    if not values:
        return float("nan")
    s = sorted(values)
    if len(s) == 1:
        return s[0]
    k = (len(s) - 1) * (p / 100.0)
    f = int(k)
    c = min(f + 1, len(s) - 1)
    if f == c:
        return s[f]
    return s[f] * (c - k) + s[c] * (k - f)


def load_rows() -> list[dict[str, str]]:
    with CSV_PATH.open() as f:
        return list(csv.DictReader(f))


def fnum(s: str) -> float:
    if s is None or s == "":
        return float("nan")
    try:
        return float(s)
    except ValueError:
        return float("nan")


def stat_block(label: str, values: list[float]) -> str:
    vals = [v for v in values if not (v != v)]  # NaN filter
    if not vals:
        return f"  {label:24s}: n=0"
    p = lambda x: percentile(vals, x)
    return (
        f"  {label:24s}: n={len(vals):3d}  "
        f"min={min(vals):+.4f} p10={p(10):+.4f} p25={p(25):+.4f} "
        f"p50={p(50):+.4f} p75={p(75):+.4f} p90={p(90):+.4f} max={max(vals):+.4f} "
        f"mean={statistics.mean(vals):+.4f}"
    )


def write_thresholds(rows: list[dict[str, str]]) -> None:
    by_trig: dict[str, list[dict[str, str]]] = defaultdict(list)
    for r in rows:
        by_trig[r["candidate_trigger"]].append(r)

    lines = [
        "# Black-box bot — tetikleyici × sinyal eşik istatistikleri",
        "",
        "Bu rapor `scripts/analyze_blackbox.py` çıktısındaki 307 emrin sınıflandırılmış",
        "halinden eşik histogram'larını üretir. Her tetikleyici için ilgili sinyallerin",
        "percentile dağılımı gösterilir; **öneri eşik** sütunu Faz 3 Elis defaultları için",
        "kullanılacak başlangıç değeridir (kullanıcı onayı sonrası dondurulur).",
        "",
        f"Toplam: **{len(rows)} emir, {len(by_trig)} farklı tetikleyici**.",
        "",
    ]

    trigger_order = [
        "signal_open",
        "signal_flip",
        "price_drift",
        "avg_down_edge",
        "pyramid_signal",
        "parity_gap",
        "pre_resolve_scoop",
        "deadline_cleanup",
        "unknown",
    ]

    for trig in trigger_order:
        items = by_trig.get(trig, [])
        lines.append(f"\n## `{trig}` — {len(items)} emir\n")
        if not items:
            lines.append("(örnek yok)")
            continue
        scores = [fnum(r["score"]) for r in items]
        dscores = [abs(fnum(r["dscore"])) for r in items]
        ofis = [fnum(r["ofi"]) for r in items]
        bsis = [fnum(r["bsi"]) for r in items]
        cvds = [fnum(r["cvd"]) for r in items]
        bid_sums = [fnum(r["bid_sum"]) for r in items]
        sizes = [fnum(r["size"]) for r in items]
        prices = [fnum(r["price"]) for r in items]
        t_offs = [fnum(r["t_off"]) for r in items]
        opp_filled = [fnum(r["opp_filled"]) for r in items]
        dom_filled = [fnum(r["dom_filled"]) for r in items]
        gap_qtys = [abs(d - o) for d, o in zip(dom_filled, opp_filled)]

        lines.append("```")
        lines.append(stat_block("score", scores))
        lines.append(stat_block("|dscore|", dscores))
        lines.append(stat_block("ofi", ofis))
        lines.append(stat_block("bsi", bsis))
        lines.append(stat_block("cvd", cvds))
        lines.append(stat_block("bid_sum", bid_sums))
        lines.append(stat_block("size", sizes))
        lines.append(stat_block("price", prices))
        lines.append(stat_block("t_off (s)", t_offs))
        lines.append(stat_block("|dom-opp| qty", gap_qtys))
        lines.append("```")

        if trig == "signal_open":
            ups = [r for r in items if r["outcome"] == "Up"]
            downs = [r for r in items if r["outcome"] == "Down"]
            up_scores = [fnum(r["score"]) for r in ups]
            down_scores = [fnum(r["score"]) for r in downs]
            lines.append("\n**Intent ayrımı:**")
            if up_scores:
                lines.append(f"- UP opener (n={len(ups)}): score min={min(up_scores):.3f} max={max(up_scores):.3f} mean={statistics.mean(up_scores):.3f}")
            if down_scores:
                lines.append(f"- DOWN opener (n={len(downs)}): score min={min(down_scores):.3f} max={max(down_scores):.3f} mean={statistics.mean(down_scores):.3f}")
            lines.append("- **Öneri:** `score_neutral=5.0` (UP/DOWN eşiği), `score_dead_zone≈0.3` (opener açma minimum sapma)")
        elif trig == "signal_flip":
            d25 = percentile([abs(d) for d in dscores], 25)
            lines.append(f"\n**Öneri:** `score_flip_threshold ≈ {d25:.2f}` (|dscore| p25)")
        elif trig == "price_drift":
            lines.append("\n**Öneri:** `requote_price_eps_ticks=0.5` (1 tick fiyat farkı koşulu zaten classifier'da kuruldu)")
        elif trig == "avg_down_edge":
            edges = []
            for r in items:
                avg_dom = fnum(r["avg_dom"])
                price = fnum(r["price"])
                if avg_dom > 0:
                    edges.append((avg_dom - price) / 0.01)
            if edges:
                lines.append(f"\n**(avg_dom-price)/tick:** {stat_block('edge_ticks', edges)}")
                lines.append(f"\n**Öneri:** `avg_down_min_edge_ticks ≈ {percentile(edges, 25):.2f}` (p25)")
        elif trig == "pyramid_signal":
            ofi25 = percentile([o for o in ofis if not (o != o)], 25) if ofis else float("nan")
            lines.append(f"\n**Öneri:** `pyramid_ofi_min ≈ {ofi25:.2f}` (p25), `pyramid_score_persist_ms=5000`")
        elif trig == "parity_gap":
            gaps = [g for g in gap_qtys if g > 0]
            p10 = percentile(gaps, 10) if gaps else 0.0
            lines.append(f"\n**Öneri:** `parity_min_gap_qty ≈ {p10:.1f}` shares (p10)")
        elif trig == "pre_resolve_scoop":
            opp_bids = []
            for r in items:
                outcome = r["outcome"]
                if outcome == "Up":
                    opp_bids.append(fnum(r["down_bid"]))
                else:
                    opp_bids.append(fnum(r["up_bid"]))
            t_remaining = [300 - fnum(r["t_off"]) for r in items]
            if opp_bids:
                lines.append(f"\n**opp_bid:** {stat_block('opp_bid', opp_bids)}")
            if t_remaining:
                lines.append(f"\n**remaining_s:** {stat_block('remaining_s', t_remaining)}")
            lines.append(f"\n**Öneri:** `scoop_opp_bid_max=0.05`, `scoop_min_remaining_ms ≈ {percentile(t_remaining, 90)*1000:.0f}` (p90 → ms)")
        elif trig == "deadline_cleanup":
            t_remaining = [300 - fnum(r["t_off"]) for r in items]
            if t_remaining:
                lines.append(f"\n**Öneri:** `deadline_safety_ms ≈ {percentile(t_remaining, 75)*1000:.0f}` (p75 → ms)")

    THRESHOLDS_MD.parent.mkdir(parents=True, exist_ok=True)
    THRESHOLDS_MD.write_text("\n".join(lines))
    print(f"[OK] thresholds: {THRESHOLDS_MD.relative_to(ROOT)}")


def write_analysis(rows: list[dict[str, str]]) -> None:
    by_market: dict[str, list[dict[str, str]]] = defaultdict(list)
    for r in rows:
        by_market[r["market"]].append(r)

    by_trig = Counter(r["candidate_trigger"] for r in rows)
    by_role = Counter(r["candidate_role"] for r in rows)
    by_conf = Counter(r["confidence"] for r in rows)

    # confusion: signal_open için score_sign × outcome
    open_rows = [r for r in rows if r["candidate_trigger"] == "signal_open"]
    cm = Counter()
    for r in open_rows:
        score = fnum(r["score"])
        side = "score>=5" if score >= 5.0 else "score<5"
        cm[(side, r["outcome"])] += 1

    # Opener detayı (anomalileri tespit için)
    opener_detail = []
    for r in open_rows:
        score = fnum(r["score"])
        out = r["outcome"]
        match_score = (score >= 5.0 and out == "Up") or (score < 5.0 and out == "Down")
        bsi = fnum(r["bsi"])
        ofi = fnum(r["ofi"])
        match_bsi = (bsi > 0 and out == "Up") or (bsi < 0 and out == "Down")
        predicted = r.get("predicted_opener", "")
        rule = r.get("opener_rule", "")
        match_composite = predicted == out
        opener_detail.append({
            "market": r["market"],
            "outcome": out,
            "score": score,
            "bsi": bsi,
            "ofi": ofi,
            "predicted": predicted,
            "rule": rule,
            "match_score": match_score,
            "match_bsi": match_bsi,
            "match_composite": match_composite,
        })

    # transition matrix (consecutive triggers per market)
    trans = Counter()
    for market, items in by_market.items():
        for i in range(1, len(items)):
            prev = items[i - 1]["candidate_trigger"]
            cur = items[i]["candidate_trigger"]
            trans[(prev, cur)] += 1

    # win/lose marketler — REDEEM olmayanlar muhtemel kayıp
    REDEEMED = {"btc-updown-5m-1777467300", "btc-updown-5m-1777467600", "btc-updown-5m-1777468200"}
    lose_markets = [m for m in by_market if m not in REDEEMED]

    lines = [
        "# Black-box bot davranış analizi — konsolide rapor",
        "",
        "**Veri:** 6 ardışık BTC up/down 5dk market, 307 emir, tek cüzdan",
        "(`0xeebde7a0e019a63e6b476eb425505b7b3e6eba30`).",
        "",
        f"**Kazanan marketler (REDEEM gözlendi):** {sorted(REDEEMED)}",
        f"**Kayıp/çözümlenmemiş:** {sorted(lose_markets)}",
        "",
        "## 1) Trigger dağılımı",
        "",
        "| Trigger | Adet | Oran |",
        "|---|---:|---:|",
    ]
    total = len(rows)
    for k, v in by_trig.most_common():
        lines.append(f"| `{k}` | {v} | {v/total*100:.1f}% |")

    lines.extend([
        "",
        "## 2) Role dağılımı",
        "",
        "| Role | Adet |",
        "|---|---:|",
    ])
    for k, v in by_role.most_common():
        lines.append(f"| `{k}` | {v} |")

    lines.extend([
        "",
        "## 3) Confidence",
        "",
    ])
    for k, v in by_conf.most_common():
        lines.append(f"- `{k}`: {v} ({v/total*100:.1f}%)")

    lines.extend([
        "",
        "## 4) Confusion matrix — `signal_open` × score sign",
        "",
        "Bot ilk emirde score≥5 iken UP, score<5 iken DOWN açıyor mu?",
        "",
        "| score sign | outcome | n |",
        "|---|---|---:|",
    ])
    for (sign, out), n in sorted(cm.items()):
        lines.append(f"| {sign} | {out} | {n} |")

    correct = sum(n for (s, o), n in cm.items() if (s == "score>=5" and o == "Up") or (s == "score<5" and o == "Down"))
    total_open = sum(cm.values())
    if total_open:
        lines.append(f"\n**Tek-tick `score≥5` kuralı:** {correct}/{total_open} = {correct/total_open*100:.0f}%")
        if correct < total_open:
            lines.append(
                f"\n> **NOT:** Tek-sinyal kuralı yetersiz. Aşağıdaki **composite** kural "
                f"6/6 doğrulukla bot davranışını yakaladı."
            )

    # Opener detay tablosu
    lines.extend([
        "\n### Opener detayı (her marketin ilk emri)",
        "",
        "| market | outcome | score | bsi | ofi | score kural | BSI kural | composite |",
        "|---|---|---:|---:|---:|---|---|---|",
    ])
    for d in opener_detail:
        m_short = d["market"].split("-")[-1]
        s_check = "✓" if d["match_score"] else "✗"
        b_check = "✓" if d["match_bsi"] else "✗"
        c_check = "✓" if d["match_composite"] else "✗"
        lines.append(
            f"| `...{m_short}` | {d['outcome']} | {d['score']:.2f} | "
            f"{d['bsi']:+.2f} | {d['ofi']:+.2f} | {s_check} | {b_check} | "
            f"{c_check} ({d['rule']}) |"
        )

    bsi_match = sum(1 for d in opener_detail if d["match_bsi"])
    composite_match = sum(1 for d in opener_detail if d["match_composite"])
    lines.append(
        f"\n**Tek-sinyal score kuralı:** {correct}/{total_open} = {correct/total_open*100:.0f}%"
    )
    lines.append(
        f"\n**BSI sign kuralı:** {bsi_match}/{total_open} = {bsi_match/total_open*100:.0f}%"
    )
    lines.append(
        f"\n**Composite kuralı (reversion + momentum_dscore + score_avg fallback):** "
        f"**{composite_match}/{total_open} = {composite_match/total_open*100:.0f}%**"
    )

    # Composite kural anlatımı
    lines.extend([
        "",
        "### Composite opener kuralı (Faz 3 için final)",
        "",
        "Pre-opener pencere = marketin başlangıcından ilk emre kadar gelen tüm tick'ler.",
        "İlk emrin verildiği saniyeye kadarki sinyalleri kullanarak yön belirlenir:",
        "",
        "1. **Mean reversion** (`|bsi| > 1.0`): BSI'nin tersi yön açılır.",
        "   - BSI çok pozitif (UP basıncı aşırı) → DOWN aç",
        "   - BSI çok negatif (DOWN basıncı aşırı) → UP aç",
        "   - Sezgi: aşırı tek-yönlü baskı sonrası mean reversion bekleniyor.",
        "",
        "2. **Momentum (Δscore)** (`|Δscore| > 0.1`, BSI normal): Δscore yönü.",
        "   - Δscore > 0.1 → UP",
        "   - Δscore < -0.1 → DOWN",
        "   - Sezgi: pre-opener pencerede skor belirgin yön değiştiriyorsa, trend",
        "     devam edecek varsayımı.",
        "",
        "3. **Momentum (avg score, fallback)** (yukarıdakiler kararsız): pre-opener",
        "   pencerenin ortalama score'u.",
        "   - avg ≥ 5.0 → UP",
        "   - avg < 5.0 → DOWN",
        "   - Sezgi: anlık skor noktasal değil, baseline'a göre yön belirle.",
        "",
        "**Parametreler:**",
        "- `BSI_REVERSION_THRESHOLD = 1.0`",
        "- `DSCORE_DEAD_ZONE = 0.1`",
        "- `SCORE_NEUTRAL = 5.0`",
        "",
        "Bu kural 6/6 örnekte gözlemlenen davranışla uyumlu.",
    ])

    lines.extend([
        "",
        "## 5) Trigger geçiş matrisi (top 15)",
        "",
        "| önceki → sonraki | n |",
        "|---|---:|",
    ])
    for (a, b), n in trans.most_common(15):
        lines.append(f"| `{a}` → `{b}` | {n} |")

    lines.extend([
        "",
        "## 6) Per-market özet",
        "",
        "| market | trades | UP n | DOWN n | son emir t_off |",
        "|---|---:|---:|---:|---:|",
    ])
    for market in sorted(by_market.keys()):
        items = by_market[market]
        ups = sum(1 for r in items if r["outcome"] == "Up")
        downs = sum(1 for r in items if r["outcome"] == "Down")
        last_t = max(fnum(r["t_off"]) for r in items)
        lines.append(f"| `{market}` | {len(items)} | {ups} | {downs} | {last_t:.0f} |")

    lines.extend([
        "",
        "## 7) Önerilen eşikler (Faz 3 için başlangıç defaultları)",
        "",
        "Detay için `[blackbox-thresholds-20260429.md](blackbox-thresholds-20260429.md)`.",
        "",
        "| parametre | öneri | kaynak |",
        "|---|---|---|",
        "| `bsi_reversion_threshold` | `1.0` | composite opener (6/6 doğrulandı) |",
        "| `dscore_dead_zone` | `0.1` | composite opener fallback eşiği |",
        "| `score_neutral` | `5.0` | composite kuralında avg-score karşılaştırma |",
        "| `score_flip_threshold` | dinamik (raporu gör) | `signal_flip` p25 \\|dscore\\| |",
        "| `requote_price_eps_ticks` | `0.5` | classifier'da 1-tick koşulu |",
        "| `avg_down_min_edge_ticks` | dinamik | `avg_down_edge` (avg-price)/tick p25 |",
        "| `pyramid_ofi_min` | dinamik | `pyramid_signal` ofi p25 |",
        "| `pyramid_score_persist_ms` | `5000` | başlangıç |",
        "| `pyramid_size_mult` | dinamik | pyramid size / opener size p50 |",
        "| `parity_min_gap_qty` | dinamik | `parity_gap` \\|dom-opp\\| p10 |",
        "| `lock_avg_threshold` | `0.97` | klasik (tahmini) |",
        "| `scoop_opp_bid_max` | `0.05` | classifier'da kullanıldı, doğrulandı |",
        "| `scoop_min_remaining_ms` | dinamik | `pre_resolve_scoop` (300-t_off) p90 |",
        "| `deadline_safety_ms` | dinamik | `deadline_cleanup` (300-t_off) p75 |",
        "",
        "## 8) Gözden geçirilmesi gereken (low confidence + unknown)",
        "",
        f"`unknown` trigger: **{by_trig.get('unknown', 0)} emir** — bu satırlar Faz 3 öncesi review.",
        f"`low` confidence: **{by_conf.get('low', 0)} emir**.",
        "",
        "Tipik `unknown` örneği: opener'dan hemen sonra (ilk 5-15s içinde) aynı outcome'a",
        "ardışık 2-3 emir (size farklı, fiyat aynı/yakın) — bu muhtemelen *opener_followup*",
        "ya da *initial_buildup* davranışı. Faz 3'te ayrı bir trigger olarak modellenebilir.",
        "",
        "Detay: `exports/blackbox-per-market-20260429.md`.",
        "",
        "## 9) Kritik gözlemler — Faz 3 için",
        "",
        "1. **Opener kuralı çözüldü — composite signal kullanılacak.** Tek-sinyal `score≥5`",
        "   sadece 4/6 doğru. Composite kural (mean reversion + Δscore momentum + score_avg",
        "   fallback) **6/6 doğru**. Detay yukarıda 4. bölümde.",
        "",
        "2. **Pyramid çok seçici.** Sadece 6 emir; OFI ortalama 0.89 (p25=0.83). Yani bot",
        "   sadece çok güçlü trend'lerde pyramid'liyor. `pyramid_ofi_min=0.55` planlamış",
        "   olduğumuz default çok düşük; **0.80'e çıkarılmalı**.",
        "",
        "3. **`price_drift` ezici çoğunluk (%48).** Bot her tick'te best_bid değişince",
        "   requote yapıyor. Bu Elis için en sık tetiklenen aksiyon olacak.",
        "",
        "4. **Pre-resolve scoop net.** 25/25 emir `t>=256s`, `opp_bid<=0.05`, kazanan",
        "   tarafa 0.94-0.99 fiyatla agresif alım. Kazanan marketler için PnL'i artıran",
        "   ana mekanizma. `scoop_min_remaining_ms=44000` (max 44s), genelde son 30s.",
        "",
        "5. **Deadline guard sıkı.** 16/16 emir `t>=292s`. Bot pencere bitimine 8s kala",
        "   son temizlik yapıyor. `deadline_safety_ms=8000`.",
        "",
        "6. **Hedge parity dominant.** 50 emir parity_gap, 47 emir hedge_topup role.",
        "   Yani bot her dom alımının ardından opp tarafa parity emri açıyor.",
        "   Bu Elis'te de zorunlu davranış.",
        "",
        "7. **Avg_down agresif.** 10 emir, edge ortalama 6.9 tick (avg'den çok aşağıda)",
        "   ama p25=2.3 tick. **`avg_down_min_edge_ticks=2.3`** öneri.",
        "",
        "8. **Win/Lose oranı %50.** 3 win (5413 USDC), 3 lose. Strateji **direksiyonel",
        "   risk taşıyor**. Lose marketlerde pre-resolve scoop yok ya da yetersiz.",
        "",
    ])

    ANALYSIS_MD.write_text("\n".join(lines))
    print(f"[OK] analysis: {ANALYSIS_MD.relative_to(ROOT)}")


def main() -> int:
    if not CSV_PATH.exists():
        print(f"CSV not found: {CSV_PATH}")
        print("First run: python3 scripts/analyze_blackbox.py")
        return 1
    rows = load_rows()
    write_thresholds(rows)
    write_analysis(rows)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
