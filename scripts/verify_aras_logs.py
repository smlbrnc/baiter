#!/usr/bin/env python3
"""
aras.md Ters-Mühendislik Doğrulama Scripti

Giriş:
  - exports/polymarket-log-btc-updown-5m-<epoch>.json  (trade / redeem logları)
  - exports/bot14-ticks-20260429/btc-updown-5m-<epoch>_ticks.json  (1 sn book + sinyal)
Çıkış:
  - exports/aras-verification.md  (insan-okur rapor)
  - stdout özet

Kural sınıflandırması aras.md Bölüm 5 esas alınarak yapılmıştır.
"""

from __future__ import annotations

import json
import math
import sys
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any

# ─────────────────────────────────────────────
# YAPILANDIRMA
# ─────────────────────────────────────────────
EPOCHS = [1777467000 + 300 * i for i in range(6)]
LADDER_LEVELS = [0.45, 0.40, 0.35, 0.30, 0.25, 0.20, 0.17, 0.15, 0.13,
                 0.10, 0.07, 0.05, 0.03, 0.02, 0.01]

BASE = Path("exports")
TICK_DIR = BASE / "bot14-ticks-20260429"
LOG_DIR = BASE

# aras.md iddia sabitleri
DOC_TRADE_COUNT = 248
DOC_MARKETS = 5
DOC_NET_PNL = 252.17
DOC_WIN_LOSS = (4, 1)

# Sınıflandırma eşikleri (aras.md Bölüm 5 ve 7)
BRACKET_WINDOW = (0, 30)
LADDER_WINDOW = (30, 180)
DIR_WINDOW = (180, 280)
SCOOP_WINDOW = (280, 400)

BRACKET_PRICE_TOL = 0.025        # ask'a göre max fiyat farkı
BRACKET_SIZE_MIN = 25.0
BRACKET_SIZE_MAX = 60.0
LADDER_MAX_DIST = 0.015           # en yakın LADDER seviyesine max mesafe
DIR_MIN_TREND = 0.10              # |ask - 0.5| eşiği
DIR_MIN_SIZE = 40.0
DIR_PRICE_TOL = 0.025
SCOOP_MAX_PRICE = 0.17            # kaybeden taraf ucuz seviyeleri

# ─────────────────────────────────────────────
# YARDIMCI
# ─────────────────────────────────────────────

@dataclass
class Tick:
    ts_ms: int
    up_bid: float
    up_ask: float
    down_bid: float
    down_ask: float
    signal_score: float
    bsi: float
    ofi: float
    cvd: float

    @property
    def pair_cost(self) -> float:
        return self.up_ask + self.down_ask

    @property
    def bid_sum(self) -> float:
        return self.up_bid + self.down_bid

    def ask_for(self, outcome: str) -> float:
        return self.up_ask if outcome == "Up" else self.down_ask

    def bid_for(self, outcome: str) -> float:
        return self.up_bid if outcome == "Up" else self.down_bid


@dataclass
class Trade:
    ts: int           # saniye
    t_off: int        # epoch'tan fark, saniye
    outcome: str      # "Up" | "Down"
    size: float
    price: float
    usdc: float
    tx: str
    idx: int          # sıra (sıfır bazlı, zaman sıralamasına göre)

    phase: str = ""
    phase_reason: str = ""
    tick_pair_cost: float = float("nan")
    tick_ask: float = float("nan")
    tick_signal: float = float("nan")
    dist_to_ladder: float = float("nan")


@dataclass
class MarketResult:
    epoch: int
    trades: list[Trade] = field(default_factory=list)
    up_shares: float = 0.0
    up_cost: float = 0.0
    dn_shares: float = 0.0
    dn_cost: float = 0.0
    redeem_usdc: float = 0.0
    winner: str = "?"      # "UP" | "DOWN" | "?"
    est_pnl: float = float("nan")
    phase_counts: dict[str, int] = field(default_factory=dict)
    has_redeem: bool = False
    notes: list[str] = field(default_factory=list)


# ─────────────────────────────────────────────
# VERİ YÜKLEME
# ─────────────────────────────────────────────

def load_ticks(epoch: int) -> list[Tick]:
    path = TICK_DIR / f"btc-updown-5m-{epoch}_ticks.json"
    raw: list[dict[str, Any]] = json.loads(path.read_text())
    ticks = []
    for r in raw:
        ticks.append(Tick(
            ts_ms=int(r["ts_ms"]),
            up_bid=float(r["up_best_bid"]),
            up_ask=float(r["up_best_ask"]),
            down_bid=float(r["down_best_bid"]),
            down_ask=float(r["down_best_ask"]),
            signal_score=float(r.get("signal_score", 5.0)),
            bsi=float(r.get("bsi", 0.0)),
            ofi=float(r.get("ofi", 0.0)),
            cvd=float(r.get("cvd", 0.0)),
        ))
    return sorted(ticks, key=lambda t: t.ts_ms)


def load_trades(epoch: int) -> tuple[list[Trade], float, bool]:
    """Trade'leri ve redeem'i yükle; (trades, redeem_usdc, has_redeem) döndür."""
    path = LOG_DIR / f"polymarket-log-btc-updown-5m-{epoch}.json"
    log: dict[str, Any] = json.loads(path.read_text())
    acts = log.get("activity", [])
    buys = sorted(
        [a for a in acts if a.get("type") == "TRADE" and a.get("side") == "BUY"],
        key=lambda a: (int(a["timestamp"]), float(a.get("price", 0)))
    )
    trades = []
    for idx, a in enumerate(buys):
        ts = int(a["timestamp"])
        trades.append(Trade(
            ts=ts,
            t_off=ts - epoch,
            outcome=str(a.get("outcome", "")),
            size=float(a["size"]),
            price=float(a["price"]),
            usdc=float(a["usdcSize"]),
            tx=str(a.get("transactionHash", "")),
            idx=idx,
        ))
    redeem_entries = [a for a in acts if a.get("type") == "REDEEM"]
    redeem_usdc = sum(float(a["usdcSize"]) for a in redeem_entries)
    has_redeem = len(redeem_entries) > 0
    return trades, redeem_usdc, has_redeem


# ─────────────────────────────────────────────
# TICK EŞLEŞTİRME
# ─────────────────────────────────────────────

def find_tick(ticks: list[Tick], trade_ts: int, window_s: int = 2) -> Tick | None:
    """Trade zaman damgasına en yakın tick'i döndür (±window_s)."""
    target_ms = trade_ts * 1000
    best: Tick | None = None
    best_dist = window_s * 1000 + 1
    for tk in ticks:
        d = abs(tk.ts_ms - target_ms)
        if d < best_dist:
            best_dist = d
            best = tk
    return best if best_dist <= window_s * 1000 else None


# ─────────────────────────────────────────────
# SINIFLANDIRMA
# ─────────────────────────────────────────────

def nearest_ladder(price: float) -> float:
    return min(LADDER_LEVELS, key=lambda L: abs(price - L))


def classify(trade: Trade, tick: Tick | None, up_sh: float, dn_sh: float) -> tuple[str, str]:
    """(phase_label, reason) döndür."""
    t = trade.t_off
    oc = trade.outcome   # "Up" | "Down"
    px = trade.price
    sz = trade.size

    if tick is None:
        return "OTHER", "tick_eşleşmedi"

    ask = tick.ask_for(oc)
    trend = tick.up_ask - 0.5

    # ── FAZ 1: BRACKET ──────────────────────────────────────────────────────
    if BRACKET_WINDOW[0] <= t < BRACKET_WINDOW[1]:
        if ask <= 0:
            return "BRACKET", "book_yeni_açıldı(ask=0)"
        price_ok = abs(px - ask) <= BRACKET_PRICE_TOL
        size_ok = BRACKET_SIZE_MIN <= sz <= BRACKET_SIZE_MAX
        pc_ok = tick.pair_cost <= 1.02
        reasons = []
        if price_ok: reasons.append(f"px≈ask({ask:.4f})")
        if size_ok:  reasons.append(f"sz={sz:.0f}")
        if pc_ok:    reasons.append(f"pc={tick.pair_cost:.3f}")
        if price_ok and size_ok:
            return "BRACKET", " ".join(reasons)
        return "BRACKET?", f"fiyat_ok={price_ok} sz_ok={size_ok} px={px:.4f} ask={ask:.4f}"

    # ── FAZ 2: LADDER ───────────────────────────────────────────────────────
    if LADDER_WINDOW[0] <= t < LADDER_WINDOW[1]:
        nl = nearest_ladder(px)
        dist = abs(px - nl)
        price_below_ask = px < ask - 0.01 or ask <= 0  # maker bid doldu
        if dist <= LADDER_MAX_DIST:
            verb = "maker_bid_fill" if price_below_ask else "yakın_ask"
            return "LADDER", f"level={nl:.2f} dist={dist:.4f} {verb}"
        # Merdiven seviyesine uzak ama LADDER penceresinde
        return "LADDER?", f"level_uzak dist={dist:.4f} nearest={nl:.2f} px={px:.4f}"

    # ── FAZ 3: DIRECTIONAL ─────────────────────────────────────────────────
    if DIR_WINDOW[0] <= t < DIR_WINDOW[1]:
        abs_trend = abs(trend)
        right_dir = (trend > 0 and oc == "Up") or (trend < 0 and oc == "Down")
        price_ok = ask > 0 and abs(px - ask) <= DIR_PRICE_TOL
        reasons = [f"t={t}s", f"trend={trend:+.3f}", f"sz={sz:.0f}", f"px={px:.4f}"]
        if abs_trend >= DIR_MIN_TREND and right_dir and sz >= DIR_MIN_SIZE:
            return "DIRECTIONAL", " ".join(reasons)
        if abs_trend >= DIR_MIN_TREND and not right_dir:
            # ters yön — hedge olabilir (pair cost dengesi)
            return "DIRECTIONAL_HEDGE", " ".join(reasons + ["ters_yön"])
        return "DIRECTIONAL?", " ".join(reasons + [f"trend_küçük={abs_trend:.3f}"])

    # ── FAZ 4: SCOOP / SETTLEMENT ───────────────────────────────────────────
    if t >= SCOOP_WINDOW[0]:
        if px <= SCOOP_MAX_PRICE:
            return "SCOOP", f"t={t}s px={px:.4f} kaybeden_taraf_ucuz"
        if px >= 0.85:
            return "SCOOP_WINNER", f"t={t}s px={px:.4f} kazanan_taraf_pahalı"
        return "SCOOP?", f"t={t}s px={px:.4f} belirsiz"

    return "OTHER", f"t={t}s dışı"


# ─────────────────────────────────────────────
# KAZANAN TAHMİNİ
# ─────────────────────────────────────────────

def infer_winner(ticks: list[Tick]) -> str:
    # Son 30 tick üzerinden — bir taraf ≥0.85, diğer taraf ≤0.20
    for tk in reversed(ticks[-30:]):
        ua, da = tk.up_ask, tk.down_ask
        if ua <= 0.20 and da >= 0.85:
            return "DOWN"
        if da <= 0.20 and ua >= 0.85:
            return "UP"
    # Son tick — gevşek eşik
    last = ticks[-1]
    if last.up_ask >= 0.80 and last.down_ask <= 0.25:
        return "UP"
    if last.down_ask >= 0.80 and last.up_ask <= 0.25:
        return "DOWN"
    return "?"


# ─────────────────────────────────────────────
# PIYASA DOĞRULAMA
# ─────────────────────────────────────────────

def verify_market(epoch: int) -> MarketResult:
    ticks = load_ticks(epoch)
    trades, redeem_usdc, has_redeem = load_trades(epoch)
    res = MarketResult(epoch=epoch)
    res.redeem_usdc = redeem_usdc
    res.has_redeem = has_redeem
    res.winner = infer_winner(ticks)

    # Tick haritası: saniye → tick (ms'yi saniyeye düşür)
    tick_by_sec: dict[int, Tick] = {}
    for tk in ticks:
        sec = tk.ts_ms // 1000
        if sec not in tick_by_sec:
            tick_by_sec[sec] = tk

    for trade in trades:
        # Tick eşleştirme: tam saniye veya ±2
        tk: Tick | None = None
        for delta in range(0, 3):
            for sign in (0, -1, 1):
                candidate = tick_by_sec.get(trade.ts + sign * delta)
                if candidate:
                    tk = candidate
                    break
            if tk:
                break

        up_sh_before = res.up_shares
        dn_sh_before = res.dn_shares

        phase, reason = classify(trade, tk, res.up_shares, res.dn_shares)
        trade.phase = phase
        trade.phase_reason = reason
        if tk:
            trade.tick_pair_cost = tk.pair_cost
            trade.tick_ask = tk.ask_for(trade.outcome)
            trade.tick_signal = tk.signal_score
            trade.dist_to_ladder = abs(trade.price - nearest_ladder(trade.price))

        # Pozisyon güncellemesi
        if trade.outcome == "Up":
            res.up_shares += trade.size
            res.up_cost += trade.usdc
        else:
            res.dn_shares += trade.size
            res.dn_cost += trade.usdc

        res.trades.append(trade)

    # Faz sayımı
    from collections import Counter
    base_phase = [t.phase.split("?")[0].split("_HEDGE")[0] for t in res.trades]
    res.phase_counts = dict(Counter(base_phase))

    # PnL tahmini
    total_cost = res.up_cost + res.dn_cost
    if res.winner == "UP":
        win_sh = res.up_shares
    elif res.winner == "DOWN":
        win_sh = res.dn_shares
    else:
        win_sh = float("nan")

    if math.isnan(win_sh):
        res.est_pnl = float("nan")
    elif has_redeem:
        # Eğer REDEEM kaydı varsa onu kullan; kazanan payları $1'den hesapla
        res.est_pnl = redeem_usdc - total_cost
    else:
        # REDEEM yok: kazanan share * $1 − maliyet (merge/fee dahil değil)
        res.est_pnl = win_sh - total_cost

    return res


# ─────────────────────────────────────────────
# DOKÜMAN İDDİALARINI KONTROL ET (C1–C9)
# ─────────────────────────────────────────────

def verify_doc_claims(results: list[MarketResult], all_ticks: dict[int, list[Tick]]) -> list[dict]:
    claims = []

    def claim(cid, desc, actual, expected, result, note=""):
        claims.append({"id": cid, "desc": desc, "expected": expected,
                        "actual": actual, "result": result, "note": note})

    total_trades = sum(len(r.trades) for r in results)
    # C1 trade sayısı
    claim("C1", "Toplam trade sayısı",
          total_trades, DOC_TRADE_COUNT,
          "MATCH" if total_trades == DOC_TRADE_COUNT else "MISMATCH",
          note="307 satır BUY var, 248 aras.md iddiası")

    # C2 piyasa sayısı
    claim("C2", "Piyasa (epoch) sayısı",
          len(results), DOC_MARKETS,
          "MATCH" if len(results) == DOC_MARKETS else "MISMATCH",
          note="6 log dosyası var, 5 aras.md iddiası")

    # C3 net PnL
    total_pnl = sum(r.est_pnl for r in results if not math.isnan(r.est_pnl))
    pnl_diff = abs(total_pnl - DOC_NET_PNL)
    claim("C3", "Net PnL toplamı (tahmini)",
          round(total_pnl, 2), DOC_NET_PNL,
          "PARTIAL" if pnl_diff < 100 else "MISMATCH",
          note="Redeem eksik piyasalar için share×$1 yaklaşımı; merge/fee yok")

    # C4 kazanç/kayıp sayısı
    win_count = sum(1 for r in results if not math.isnan(r.est_pnl) and r.est_pnl > 0)
    loss_count = sum(1 for r in results if not math.isnan(r.est_pnl) and r.est_pnl < 0)
    claim("C4", "Kazanç/kayıp piyasa sayısı",
          f"{win_count}W/{loss_count}L", f"{DOC_WIN_LOSS[0]}W/{DOC_WIN_LOSS[1]}L",
          "MATCH" if (win_count, loss_count) == DOC_WIN_LOSS else "MISMATCH")

    # C5: 1777467600 t=4s → UP×3 + DOWN×1 aynı saniyede
    r7600 = next((r for r in results if r.epoch == 1777467600), None)
    if r7600:
        t4 = [t for t in r7600.trades if t.t_off == 4]
        up_at_4 = [t for t in t4 if t.outcome == "Up"]
        dn_at_4 = [t for t in t4 if t.outcome == "Down"]
        ok = len(up_at_4) == 3 and len(dn_at_4) == 1
        claim("C5", "1777467600 t=4s: 3 UP + 1 DOWN (taker sweep)",
              f"UP×{len(up_at_4)} DOWN×{len(dn_at_4)}", "UP×3 DOWN×1",
              "MATCH" if ok else "MISMATCH",
              note=", ".join(f"{t.outcome}@{t.price:.4f}" for t in t4))

    # C6: 1777467300 t≈160s UP@0.50 fill
    r7300 = next((r for r in results if r.epoch == 1777467300), None)
    if r7300:
        t160 = [t for t in r7300.trades if 158 <= t.t_off <= 165 and t.outcome == "Up" and abs(t.price - 0.50) < 0.01]
        # aras.md: t+10s'de UP ask = 0.50 → bid konulmuş, t+160s'de fill
        ticks_7300 = all_ticks[1777467300]
        t10_ticks = [tk for tk in ticks_7300 if 8 <= (tk.ts_ms // 1000 - 1777467300) <= 12]
        ask_at_t10 = t10_ticks[0].up_ask if t10_ticks else float("nan")
        ok = len(t160) > 0 and abs(ask_at_t10 - 0.50) < 0.02
        claim("C6", "1777467300 t≈160s UP@0.50 fill; t+10s ask≈0.50",
              f"fill@t={[t.t_off for t in t160]} ask_t10={ask_at_t10:.3f}",
              "fill var + ask_t10≈0.50",
              "MATCH" if ok else "PARTIAL",
              note="Polymarket ts fill zamanını kaydeder, emir koyuş değil")

    # C7: 1777467300 t∈{196,242,262} agresif DOWN
    if r7300:
        rows = {}
        for t_target, size_thresh in [(196, 40), (242, 40), (262, 100)]:
            hits = [t for t in r7300.trades
                    if abs(t.t_off - t_target) <= 3 and t.outcome == "Down" and t.size >= size_thresh]
            rows[t_target] = hits
        ok = all(len(v) > 0 for v in rows.values())
        detail = "; ".join(
            f"t≈{k}: " + (f"{rows[k][0].size:.0f}@{rows[k][0].price:.4f}" if rows[k] else "YOK")
            for k in sorted(rows)
        )
        claim("C7", "1777467300 Faz3 DOWN serisi (t≈196, 242, 262)",
              detail, "3 büyük DOWN trade",
              "MATCH" if ok else "MISMATCH")

    # C8: 1777467300 t∈{282,284,306} UP scoop (size 236,102,4358)
    if r7300:
        t282 = [t for t in r7300.trades if abs(t.t_off - 282) <= 3 and t.outcome == "Up"]
        t306 = [t for t in r7300.trades if abs(t.t_off - 306) <= 5 and t.outcome == "Up"]
        t282_size = sum(t.size for t in t282)
        t306_size = sum(t.size for t in t306)
        ok282 = t282_size > 200
        ok306 = any(t.size > 4000 for t in t306)
        claim("C8", "1777467300 Faz4 UP scoop (t≈282 toplam≈236+, t≈306 size≈4358)",
              f"t282_toplam={t282_size:.0f} t306={t306_size:.0f}",
              "t282≥236 & t306≥4358",
              "MATCH" if (ok282 and ok306) else "PARTIAL",
              note=f"t282 adet={len(t282)}, t306 adet={len(t306)}")

    # C9: 1777467300 son pozisyon — naked UP ≈ 4629, paired ≈ 1054
    if r7300:
        naked_up = max(0.0, r7300.up_shares - r7300.dn_shares)
        paired = min(r7300.up_shares, r7300.dn_shares)
        ok = abs(naked_up - 4629) < 200 and abs(paired - 1054) < 100
        claim("C9", "1777467300 son pozisyon: naked_UP≈4629, paired≈1054",
              f"naked_up={naked_up:.0f} paired={paired:.0f}",
              "naked_up≈4629 paired≈1054",
              "MATCH" if ok else "PARTIAL",
              note=f"up_shares={r7300.up_shares:.0f} dn_shares={r7300.dn_shares:.0f}")

    # C10: BRACKET_BASE_SIZE gözlemlenen değerler 40-50
    bracket_sizes = []
    for r in results:
        for t in r.trades:
            if t.phase.startswith("BRACKET") and BRACKET_SIZE_MIN <= t.size <= BRACKET_SIZE_MAX:
                bracket_sizes.append(t.size)
    if bracket_sizes:
        p50 = sorted(bracket_sizes)[len(bracket_sizes) // 2]
        ok = 38 <= p50 <= 52
        claim("C10", "BRACKET_BASE_SIZE gözlemlenen p50 ∈ [38,52]",
              f"p50={p50:.0f} n={len(bracket_sizes)}",
              "[38, 52]",
              "MATCH" if ok else "MISMATCH")

    return claims


# ─────────────────────────────────────────────
# BÖLÜM 8 KANIT TABLOSU DOĞRULAMA
# ─────────────────────────────────────────────

def verify_section8(results: list[MarketResult]) -> list[dict]:
    """aras.md Bölüm 8 kanıt tablosunun satır bazlı doğrulaması."""
    rows = []

    def row(rule, epoch, trade_ref, detail, status, note=""):
        rows.append({"rule": rule, "epoch": epoch, "trade_ref": trade_ref,
                     "detail": detail, "status": status, "note": note})

    r7600 = next((r for r in results if r.epoch == 1777467600), None)
    r7300 = next((r for r in results if r.epoch == 1777467300), None)

    # Satır 782: "Faz 1: çoklu seviye taker sweep | 1777467600 | 2-4 | UP @0.5267, 0.55, 0.549"
    if r7600:
        t4_up = sorted([t for t in r7600.trades if t.t_off == 4 and t.outcome == "Up"],
                       key=lambda t: t.price)
        prices = [f"{t.price:.4f}" for t in t4_up]
        expected_prices = {0.5267, 0.55, 0.549}
        found = any(abs(t.price - p) < 0.002 for t in t4_up for p in expected_prices)
        row("Faz1_taker_sweep", 1777467600, "2-4",
            f"t=4s UP fiyatlar: {prices}",
            "MATCH" if found and len(t4_up) >= 3 else "MISMATCH")

    # Satır 783: "Faz 1: aynı saniye iki taraf | t=4s UP×3 + DOWN×1"
    if r7600:
        t4_all = [t for t in r7600.trades if t.t_off == 4]
        up4 = len([t for t in t4_all if t.outcome == "Up"])
        dn4 = len([t for t in t4_all if t.outcome == "Down"])
        row("Faz1_iki_taraf", 1777467600, "1-4",
            f"t=4s UP×{up4} DOWN×{dn4}",
            "MATCH" if up4 == 3 and dn4 == 1 else "MISMATCH")

    # Satır 784: "Faz 2: GTC bid 150s+ bekleme | t+160s UP@0.50"
    if r7300:
        t160_fill = [t for t in r7300.trades if 155 <= t.t_off <= 165 and t.outcome == "Up"]
        found = any(abs(t.price - 0.50) < 0.01 for t in t160_fill)
        detail = ", ".join(f"t={t.t_off}s @{t.price:.4f}" for t in t160_fill) or "YOK"
        row("Faz2_GTC_150s", 1777467300, "26",
            f"t≈160s UP filleri: {detail}",
            "MATCH" if found else "MISMATCH")

    # Satır 785: "Faz 2: simetrik iki taraflı merdiven | 7600 t+86-88s DOWN @0.16,0.17,0.20"
    if r7600:
        t8x_dn = [t for t in r7600.trades if 84 <= t.t_off <= 92 and t.outcome == "Down"]
        ladder_hits = [t for t in t8x_dn if any(abs(t.price - L) < 0.015 for L in [0.16, 0.17, 0.20])]
        detail = ", ".join(f"t={t.t_off}s @{t.price:.4f}" for t in t8x_dn) or "YOK"
        row("Faz2_simetrik_merdiven", 1777467600, "25-29",
            f"t=84-92s DOWN filleri: {detail}",
            "MATCH" if len(ladder_hits) >= 2 else "MISMATCH",
            note=f"Toplam DOWN: {len(t8x_dn)}")

    # Satır 786: "Faz 2: round level | 7300 #35-66 @0.10,0.11,0.12,0.07,0.01"
    if r7300:
        round_levels = {0.01, 0.07, 0.10, 0.11, 0.12}
        round_hits = [t for t in r7300.trades if any(abs(t.price - L) < 0.005 for L in round_levels)]
        detail = f"round-level fill sayısı={len(round_hits)}"
        row("Faz2_round_levels", 1777467300, "35-66",
            detail,
            "MATCH" if len(round_hits) >= 5 else "MISMATCH",
            note=", ".join(f"{t.price:.2f}" for t in round_hits[:10]))

    # Satır 787: "Faz 3: DOWN@0.9138 size=154 t+262s"
    if r7300:
        t262 = [t for t in r7300.trades
                if abs(t.t_off - 262) <= 3 and t.outcome == "Down" and abs(t.price - 0.9138) < 0.005]
        row("Faz3_agresif_down", 1777467300, "50",
            f"t≈262s DOWN@~0.9138: {'bulundu size=' + str(t262[0].size) if t262 else 'YOK'}",
            "MATCH" if t262 else "MISMATCH")

    # Satır 788: "Faz 3: ardışık ask sweep t+242s DOWN@0.84 ×5"
    if r7300:
        t242 = [t for t in r7300.trades if abs(t.t_off - 242) <= 3 and t.outcome == "Down"]
        found_84 = [t for t in t242 if abs(t.price - 0.84) < 0.02]
        row("Faz3_242_sweep", 1777467300, "44-49",
            f"t≈242s DOWN adet={len(t242)} @0.84≈ adet={len(found_84)}",
            "MATCH" if len(t242) >= 3 else "MISMATCH",
            note=", ".join(f"{t.price:.4f}" for t in t242))

    # Satır 789: "Faz 4: post-close t+306s UP@0.01 size=4358"
    if r7300:
        t306 = [t for t in r7300.trades
                if abs(t.t_off - 306) <= 5 and t.outcome == "Up" and t.size > 4000]
        row("Faz4_post_close", 1777467300, "67",
            f"t≈306s UP@0.01 size≥4000: {'bulundu size=' + str(int(t306[0].size)) if t306 else 'YOK'}",
            "MATCH" if t306 else "MISMATCH")

    # Satır 790: "Faz 4: t+282s'de 16 ardışık fill @0.10-0.12"
    if r7300:
        t282 = [t for t in r7300.trades if abs(t.t_off - 282) <= 3]
        up282 = [t for t in t282 if t.outcome == "Up" and t.price <= 0.13]
        row("Faz4_settlement_cluster", 1777467300, "51-66",
            f"t≈282s fill sayısı={len(t282)} (UP@≤0.13: {len(up282)})",
            "MATCH" if len(t282) >= 10 else "MISMATCH",
            note=f"fill fiyatlar: {[f'{t.price:.2f}' for t in t282[:8]]}")

    # Satır 791: "Risk guard eksik: DOWN Faz1 → UP merdiven kontrol dışı fill"
    if r7300:
        faz1_dn = [t for t in r7300.trades if t.t_off < 30 and t.outcome == "Down"]
        faz2_up = [t for t in r7300.trades if 30 <= t.t_off < 100 and t.outcome == "Up"
                   and any(abs(t.price - L) < 0.015 for L in [0.50, 0.45, 0.40])]
        row("Risk_guard_eksik", 1777467300, "1-13 vs 36-41",
            f"Faz1 DOWN adet={len(faz1_dn)}, Faz2 UP merdiven fill adet={len(faz2_up)}",
            "MATCH" if len(faz1_dn) >= 3 and len(faz2_up) >= 1 else "MISMATCH",
            note=f"UP merdiven fiyatlar: {[f'{t.price:.2f}' for t in faz2_up]}")

    return rows


# ─────────────────────────────────────────────
# RAPOR YAZMA
# ─────────────────────────────────────────────

def write_report(results: list[MarketResult], claims: list[dict], sec8: list[dict]) -> str:
    lines: list[str] = []

    def h(level: int, text: str):
        lines.append(f"\n{'#' * level} {text}\n")

    def ln(text: str = ""):
        lines.append(text)

    h(1, "aras.md Ters-Mühendislik Doğrulaması")
    ln("> Üretildi: `scripts/verify_aras_logs.py` — 6 polymarket-log + 6 tick dosyası")
    ln("> **Sınırlamalar**: REDEEM eksik piyasalarda PnL share×\\$1 yaklaşımıdır; "
       "on-chain merge, fee, maker-rebate dahil değildir.")

    # 1. Header iddiaları
    h(2, "1. Header İddiaları (C1–C4)")
    header_cols = ["ID", "Açıklama", "Doküman", "Gerçek", "Sonuç", "Not"]
    ln("| " + " | ".join(header_cols) + " |")
    ln("|" + "|".join("---" for _ in header_cols) + "|")
    for c in claims[:4]:
        ln(f"| {c['id']} | {c['desc']} | {c['expected']} | {c['actual']} | **{c['result']}** | {c['note']} |")

    # 2. Piyasa-piyasa özet
    h(2, "2. Piyasa-Piyasa Özet")
    total_buy = sum(r.up_cost + r.dn_cost for r in results)
    total_redeem = sum(r.redeem_usdc for r in results)
    mkt_cols = ["epoch", "n_trades", "buy_USDC", "redeem_USDC", "winner", "est_pnl",
                "BRACKET", "LADDER", "DIRECTIONAL", "SCOOP", "OTHER", "has_redeem"]
    ln("| " + " | ".join(mkt_cols) + " |")
    ln("|" + "|".join("---" for _ in mkt_cols) + "|")
    for r in results:
        pc = r.phase_counts
        pnl_str = f"{r.est_pnl:+.2f}" if not math.isnan(r.est_pnl) else "n/a"
        est_note = "" if r.has_redeem else " *"
        ln(f"| {r.epoch} | {len(r.trades)} | {r.up_cost+r.dn_cost:.2f} | "
           f"{r.redeem_usdc:.2f} | {r.winner} | {pnl_str}{est_note} | "
           f"{pc.get('BRACKET',0)} | {pc.get('LADDER',0)} | "
           f"{pc.get('DIRECTIONAL',0)} | {pc.get('SCOOP',0)} | "
           f"{pc.get('OTHER',0)+pc.get('BRACKET?',0)+pc.get('LADDER?',0)+pc.get('DIRECTIONAL?',0)+pc.get('SCOOP?',0)} | "
           f"{'✓' if r.has_redeem else '✗'} |")
    ln(f"\n_Toplam BUY USDC: {total_buy:.2f} | Toplam REDEEM: {total_redeem:.2f}_")
    ln("_(*) = REDEEM eksik, PnL share×\\$1 yaklaşımı_")

    # 3. Bireysel claim doğrulaması (C5+)
    h(2, "3. Faz Örnekleri Doğrulaması (C5–C10)")
    cl_cols = ["ID", "Açıklama", "Beklenen", "Gerçek", "Sonuç", "Not"]
    ln("| " + " | ".join(cl_cols) + " |")
    ln("|" + "|".join("---" for _ in cl_cols) + "|")
    for c in claims[4:]:
        ln(f"| {c['id']} | {c['desc']} | {c['expected']} | {c['actual']} | **{c['result']}** | {c['note']} |")

    # 4. Bölüm 8 kanıt tablosu
    h(2, "4. Bölüm 8 Kanıt Tablosu Doğrulaması")
    s8_cols = ["Kural", "Piyasa", "Trade#", "Gerçek Veri", "Sonuç", "Not"]
    ln("| " + " | ".join(s8_cols) + " |")
    ln("|" + "|".join("---" for _ in s8_cols) + "|")
    for r in sec8:
        ln(f"| {r['rule']} | {r['epoch']} | {r['trade_ref']} | "
           f"{r['detail']} | **{r['status']}** | {r['note']} |")

    # 5. Kural-fit oranı
    h(2, "5. Kural-Fit Oranı")
    all_phases = [t.phase for r in results for t in r.trades]
    from collections import Counter
    cnt = Counter(all_phases)
    total = len(all_phases)
    clean = {"BRACKET": 0, "LADDER": 0, "DIRECTIONAL": 0, "SCOOP": 0, "OTHER": 0}
    for ph, n in cnt.items():
        base = ph.split("?")[0].split("_HEDGE")[0].split("_WINNER")[0]
        if base in clean:
            clean[base] += n
        else:
            clean["OTHER"] += n
    fit_cols = ["Faz", "Eşleşen", "Toplam", "Oran"]
    ln("| " + " | ".join(fit_cols) + " |")
    ln("|" + "|".join("---" for _ in fit_cols) + "|")
    for ph, n in clean.items():
        pct = 100.0 * n / total if total else 0
        ln(f"| {ph} | {n} | {total} | {pct:.1f}% |")
    ln(f"| **TOPLAM** | **{total}** | **{total}** | **100%** |")

    # 6. Tutarsızlıklar / Öneriler
    h(2, "6. docs/aras.md Tutarsızlıklar ve Öneriler")
    total_trades_actual = sum(len(r.trades) for r in results)
    total_pnl_actual = sum(r.est_pnl for r in results if not math.isnan(r.est_pnl))
    win_c = sum(1 for r in results if not math.isnan(r.est_pnl) and r.est_pnl > 0)
    loss_c = sum(1 for r in results if not math.isnan(r.est_pnl) and r.est_pnl < 0)
    ln(f"""
| Satır | Mevcut Metin | Öneri | Dayanak |
|---|---|---|---|
| 4 | `5 ardışık ... 248 trade` | `6 ardışık ... {total_trades_actual} trade` | 6 log dosyası; her biri `trades_count` sahip |
| 5 | `+$252.17 net (4 kazanç / 1 kayıp)` | `yaklaşık {total_pnl_actual:+.0f} net ({win_c}W/{loss_c}L) *` | `est_pnl` share×\\$1 yaklaşımı; bazı REDEEM kayıt eksik |
| 53 | `5 piyasanın 4'ünde` | `6 piyasanın {win_c}'inde` | piyasa bazlı win/loss |
| 994 | `5 BTC UP/DOWN 5m piyasası` | `6 BTC UP/DOWN 5m piyasası` | 6 log dosyası |

_(*) Kesin rakam için eksik REDEEM kayıtlarının on-chain doğrulaması gereklidir._
""")

    # 7. Her piyasanın trade-by-trade özeti
    h(2, "7. Trade-by-Trade Faz Özeti")
    for r in results:
        h(3, f"Epoch {r.epoch} ({r.winner}) — {len(r.trades)} trade")
        t_cols = ["idx", "t_off", "outcome", "size", "price", "tick_ask", "pair_cost", "signal", "phase", "reason"]
        ln("| " + " | ".join(t_cols) + " |")
        ln("|" + "|".join("---" for _ in t_cols) + "|")
        for t in r.trades:
            pc = f"{t.tick_pair_cost:.3f}" if not math.isnan(t.tick_pair_cost) else "-"
            ask = f"{t.tick_ask:.4f}" if not math.isnan(t.tick_ask) else "-"
            sig = f"{t.tick_signal:.2f}" if not math.isnan(t.tick_signal) else "-"
            ln(f"| {t.idx} | {t.t_off}s | {t.outcome} | {t.size:.2f} | {t.price:.4f} | "
               f"{ask} | {pc} | {sig} | `{t.phase}` | {t.phase_reason} |")

    ln("\n---")
    ln("*Üretildi: scripts/verify_aras_logs.py v1.0*")

    return "\n".join(lines)


# ─────────────────────────────────────────────
# ANA GİRİŞ
# ─────────────────────────────────────────────

def main() -> None:
    all_ticks: dict[int, list[Tick]] = {}
    results: list[MarketResult] = []

    for epoch in EPOCHS:
        print(f"[verify] epoch={epoch} ...", end=" ", flush=True)
        ticks = load_ticks(epoch)
        all_ticks[epoch] = ticks
        res = verify_market(epoch)
        results.append(res)
        print(f"trades={len(res.trades)} winner={res.winner} est_pnl={res.est_pnl:+.2f}")

    claims = verify_doc_claims(results, all_ticks)
    sec8 = verify_section8(results)

    # Rapor çıktısı
    report_text = write_report(results, claims, sec8)
    out_path = BASE / "aras-verification.md"
    out_path.write_text(report_text, encoding="utf-8")
    print(f"\n[verify] Rapor yazıldı: {out_path}")

    # Özet: MATCH/MISMATCH/PARTIAL
    print("\n──── Claim özeti ────")
    for c in claims:
        print(f"  {c['id']:4s} {c['result']:10s} {c['desc']}")
    print("\n──── Bölüm 8 özeti ────")
    for s in sec8:
        print(f"  {s['status']:10s} {s['rule']}")


if __name__ == "__main__":
    main()
