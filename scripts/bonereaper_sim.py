#!/usr/bin/env python3
"""
Bonereaper Simülasyon Scripti
==============================
Polymarket BTC 5m market_ticks verisini kullanarak Bonereaper stratejisini
yeniden oynatır ve botun gerçek kararlarıyla birebir karşılaştırır.

Amaç:
  1) Mevcut botun (135/136/137) karar mekanizmasını Python'da birebir kopyala.
  2) Simülasyon çıktısını DB'deki gerçek trade kayıtlarıyla karşılaştır.
  3) Hangi ticklerde sim ile bot ayrıştı → bunlar gelecekte gerçek bot
     karşılaştırmasının başlangıç noktası olacak.

Kullanım:
  python3 scripts/bonereaper_sim.py --db ./data/baiter.db --bot 135
  python3 scripts/bonereaper_sim.py --db ./data/baiter.db --bot 135 --slug btc-updown-5m-1778658600
  python3 scripts/bonereaper_sim.py --db ./data/baiter.db --bot 135 --all-sessions
  python3 scripts/bonereaper_sim.py --ssh ubuntu@79.125.42.234 --pem ~/Desktop/smlbrnc.pem --bot 135

Seçenekler:
  --db PATH          Yerel SQLite DB yolu
  --ssh HOST         Sunucudan DB indir (scp ile)
  --pem PATH         SSH private key
  --bot INT          Bot ID (135, 136, 137, ...)
  --slug SLUG        Tek oturum (ör. btc-updown-5m-1778658600)
  --all-sessions     Tüm oturumları sim et
  --last N           Son N oturumu sim et (default: 10)
  --verbose          Her kararı detaylı yazdır
"""

import argparse
import math
import os
import sqlite3
import subprocess
import sys
import tempfile
from dataclasses import dataclass, field
from typing import List, Optional, Tuple


# ─────────────────────────────────────────────────────────────────────────────
# PARAMETRELER — config.rs accessor'larını Python'da birebir yansıt
# ─────────────────────────────────────────────────────────────────────────────

@dataclass
class BonereaperParams:
    # bot config
    order_usdc: float = 3.0
    min_price: float = 0.05
    max_price: float = 0.95
    cooldown_threshold_ms: int = 30_000  # bots.cooldown_threshold (kullanılmıyor burada)

    # strateji params (strategy_params JSON → defaults = config.rs accessor defaults)
    buy_cooldown_ms: int = 3_000
    late_winner_secs: int = 180
    late_winner_bid_thr: float = 0.90
    late_winner_usdc: float = 100.0
    lw_max_per_session: int = 20
    imbalance_thr: float = 1000.0
    max_avg_sum: float = 1.0
    first_spread_min: float = 0.02
    size_longshot_usdc: float = 10.0
    size_mid_usdc: float = 25.0
    size_high_usdc: float = 80.0
    loser_min_price: float = 0.01
    loser_scalp_usdc: float = 10.0
    loser_scalp_max_price: float = 0.30
    late_pyramid_secs: int = 150
    winner_size_factor: float = 1.0
    avg_loser_max: float = 0.50

    # market session
    api_min_order_size: float = 5.0

    @staticmethod
    def from_db(row: dict, params_json: dict) -> "BonereaperParams":
        def get(key: str, default, lo=None, hi=None):
            v = params_json.get(key)
            if v is None:
                v = default
            if lo is not None:
                v = max(lo, v)
            if hi is not None:
                v = min(hi, v)
            return v

        buy_cd = get("bonereaper_buy_cooldown_ms", 3_000, 1_000, 60_000)
        lw_secs = min(get("bonereaper_late_winner_secs", 180), 300)
        lw_thr = get("bonereaper_late_winner_bid_thr", 0.90, 0.50, 0.99)
        lw_usdc = get("bonereaper_late_winner_usdc", 100.0, 0.0, 10_000.0)
        lw_max = min(get("bonereaper_lw_max_per_session", 20), 50)
        imb_thr = get("bonereaper_imbalance_thr", 1000.0, 0.0, 10_000.0)
        max_avg_sum = get("bonereaper_max_avg_sum", 1.0, 0.50, 2.00)
        fsm = get("bonereaper_first_spread_min", 0.02, 0.00, 0.20)
        sz_ls = get("bonereaper_size_longshot_usdc", 10.0, 0.0, 10_000.0)
        sz_mid = get("bonereaper_size_mid_usdc", 25.0, 0.0, 10_000.0)
        sz_hi = get("bonereaper_size_high_usdc", 80.0, 0.0, 10_000.0)
        loser_min = get("bonereaper_loser_min_price", 0.01, 0.001, 0.10)
        loser_scalp = get("bonereaper_loser_scalp_usdc", 10.0, 0.0, 50.0)
        loser_scalp_max = get("bonereaper_loser_scalp_max_price", 0.30, 0.05, 0.50)
        lp_secs = min(get("bonereaper_late_pyramid_secs", 150), 300)
        wsf = get("bonereaper_winner_size_factor", 1.0, 1.0, 10.0)
        avg_loser_max = get("bonereaper_avg_loser_max", 0.50, 0.10, 0.95)

        return BonereaperParams(
            order_usdc=row["order_usdc"],
            min_price=row["min_price"],
            max_price=row["max_price"],
            cooldown_threshold_ms=int(row["cooldown_threshold"]),
            buy_cooldown_ms=int(buy_cd),
            late_winner_secs=int(lw_secs),
            late_winner_bid_thr=lw_thr,
            late_winner_usdc=lw_usdc,
            lw_max_per_session=int(lw_max),
            imbalance_thr=imb_thr,
            max_avg_sum=max_avg_sum,
            first_spread_min=fsm,
            size_longshot_usdc=sz_ls,
            size_mid_usdc=sz_mid,
            size_high_usdc=sz_hi,
            loser_min_price=loser_min,
            loser_scalp_usdc=loser_scalp,
            loser_scalp_max_price=loser_scalp_max,
            late_pyramid_secs=int(lp_secs),
            winner_size_factor=wsf,
            avg_loser_max=avg_loser_max,
        )


# ─────────────────────────────────────────────────────────────────────────────
# METRIKLER
# ─────────────────────────────────────────────────────────────────────────────

@dataclass
class StrategyMetrics:
    up_filled: float = 0.0
    down_filled: float = 0.0
    avg_up: float = 0.0
    avg_down: float = 0.0
    fee_total: float = 0.0

    def ingest_fill(self, outcome: str, price: float, size: float, fee: float):
        if outcome == "UP":
            new_total = self.up_filled + size
            if new_total > 0:
                self.avg_up = (self.avg_up * self.up_filled + price * size) / new_total
            self.up_filled += size
        else:
            new_total = self.down_filled + size
            if new_total > 0:
                self.avg_down = (self.avg_down * self.down_filled + price * size) / new_total
            self.down_filled += size
        self.fee_total += fee

    def pair_count(self) -> float:
        return min(self.up_filled, self.down_filled)

    def avg_sum(self) -> float:
        return self.avg_up + self.avg_down

    def cost_basis(self) -> float:
        return self.avg_up * self.up_filled + self.avg_down * self.down_filled


# ─────────────────────────────────────────────────────────────────────────────
# DURUM MAKİNESİ
# ─────────────────────────────────────────────────────────────────────────────

@dataclass
class BonereaperActive:
    last_buy_ms: int = 0
    last_up_bid: float = 0.0
    last_dn_bid: float = 0.0
    lw_injections: int = 0
    first_done: bool = False


@dataclass
class SimTrade:
    """Simülasyonun ürettiği bir karar."""
    ts_ms: int
    outcome: str       # "UP" | "DOWN"
    price: float
    size: float
    reason: str        # bonereaper:buy:up / lw / scalp / ...
    to_end: float      # kalan saniye
    up_bid: float
    down_bid: float


# ─────────────────────────────────────────────────────────────────────────────
# LOSER SIDE BELİRLEME (bonereaper.rs loser_side)
# ─────────────────────────────────────────────────────────────────────────────

LOSER_SPREAD_MIN = 0.20

def loser_side(up_bid: float, dn_bid: float) -> Optional[str]:
    spread = abs(up_bid - dn_bid)
    if spread < LOSER_SPREAD_MIN:
        return None
    return "DOWN" if up_bid >= dn_bid else "UP"


# ─────────────────────────────────────────────────────────────────────────────
# PIECEWISE LİNEER SIZING (bonereaper_interp_usdc Rust portu)
# ─────────────────────────────────────────────────────────────────────────────

def _interp_usdc(bid: float, p: "BonereaperParams") -> float:
    """3 anchor: longshot@0.30, mid@0.65, high@lw_thr.

    bid <= 0.30 → longshot (sabit)
    0.30 < bid <= 0.65 → longshot → mid lineer
    0.65 < bid < lw_thr → mid → high lineer
    bid >= lw_thr → high (sabit fallback; LW akışı kontrol eder)
    """
    longshot = p.size_longshot_usdc
    mid = p.size_mid_usdc
    high = p.size_high_usdc
    lw_thr = p.late_winner_bid_thr
    if bid <= 0.30:
        return longshot
    if bid <= 0.65:
        t = max(0.0, min(1.0, (bid - 0.30) / 0.35))
        return longshot + (mid - longshot) * t
    if bid < lw_thr:
        span = max(lw_thr - 0.65, 0.01)
        t = max(0.0, min(1.0, (bid - 0.65) / span))
        return mid + (high - mid) * t
    return high


# ─────────────────────────────────────────────────────────────────────────────
# ANA KARAR FONKSİYONU — bonereaper.rs::decide() Python portu
# ─────────────────────────────────────────────────────────────────────────────

DRYRUN_FEE_RATE = 0.0002
POST_LW_WINNER_MAX_BID = 0.70
LW_OPP_AVG_MAX = 0.50
LW_OPP_HIGH_PRICE = 0.40
LW_WINNER_MAX_PRICE = 0.90
BSI_THRESHOLD = 0.30


def bonereaper_decide(
    state: Optional[BonereaperActive],
    p: BonereaperParams,
    m: StrategyMetrics,
    tick: dict,
    start_ts: int,
) -> Tuple[Optional[BonereaperActive], Optional[SimTrade]]:
    """
    Returns (new_state, trade_or_none).
    state=None → Idle
    """
    now_ms = tick["ts_ms"]
    up_bid = tick["up_best_bid"]
    up_ask = tick["up_best_ask"]
    dn_bid = tick["down_best_bid"]
    dn_ask = tick["down_best_ask"]
    bsi = tick.get("bsi")
    end_ts = tick["end_ts"]
    to_end = end_ts - now_ms / 1000.0

    # Idle → Active geçişi
    if state is None:
        book_ready = up_bid > 0 and up_ask > 0 and dn_bid > 0 and dn_ask > 0
        if not book_ready:
            return None, None
        st = BonereaperActive(
            last_up_bid=up_bid,
            last_dn_bid=dn_bid,
        )
        return st, None

    st = state

    if to_end < 0:
        return st, None
    if up_bid <= 0 or dn_bid <= 0:
        return st, None

    # ── LATE WINNER ──────────────────────────────────────────────────────────
    lw_secs = float(p.late_winner_secs)
    lw_usdc = p.late_winner_usdc
    lw_thr = p.late_winner_bid_thr
    lw_max = p.lw_max_per_session
    lw_quota_ok = (lw_max == 0) or (st.lw_injections < lw_max)
    lw_active = lw_usdc > 0 and lw_secs > 0 and to_end <= lw_secs

    if lw_quota_ok and lw_active and to_end > 0:
        if up_bid >= dn_bid:
            winner, w_bid, w_ask = "UP", up_bid, up_ask
            opp_filled, opp_avg = m.down_filled, m.avg_down
        else:
            winner, w_bid, w_ask = "DOWN", dn_bid, dn_ask
            opp_filled, opp_avg = m.up_filled, m.avg_up

        if w_bid >= lw_thr and w_ask > 0:
            # arb_mult — bonereaper.rs 2D tablo (price × time)
            if w_ask >= 0.99:
                if to_end <= 10:   arb_mult = 1.7
                elif to_end <= 30: arb_mult = 5.7
                elif to_end <= 60: arb_mult = 5.5
                elif to_end <= 120:arb_mult = 11.5
                else:              arb_mult = 20.0
            elif w_ask >= 0.97:
                if to_end <= 10:   arb_mult = 1.0
                elif to_end <= 30: arb_mult = 3.7
                elif to_end <= 60: arb_mult = 6.1
                elif to_end <= 120:arb_mult = 4.4
                else:              arb_mult = 9.0
            elif w_ask >= 0.95:
                arb_mult = 4.0 if to_end <= 60 else 2.0
            else:
                arb_mult = 1.0

            size = math.ceil(lw_usdc * arb_mult / w_ask)

            # LW opp_avg guard
            lw_blocked = (
                opp_filled > 0 and
                (opp_avg > LW_OPP_AVG_MAX or
                 (opp_avg > LW_OPP_HIGH_PRICE and w_ask > LW_WINNER_MAX_PRICE))
            )

            if not lw_blocked:
                notional = size * w_ask
                if notional >= p.api_min_order_size:
                    trade = SimTrade(
                        ts_ms=now_ms, outcome=winner, price=w_ask,
                        size=size, reason=f"bonereaper:lw:{winner.lower()}",
                        to_end=to_end, up_bid=up_bid, down_bid=dn_bid,
                    )
                    st.last_buy_ms = now_ms
                    st.lw_injections += 1
                    st.last_up_bid = up_bid
                    st.last_dn_bid = dn_bid
                    st.first_done = True
                    return st, trade

    # ── COOLDOWN ─────────────────────────────────────────────────────────────
    if st.last_buy_ms > 0 and (now_ms - st.last_buy_ms) < p.buy_cooldown_ms:
        st.last_up_bid = up_bid
        st.last_dn_bid = dn_bid
        return st, None

    # ── YÖN SEÇİMİ ───────────────────────────────────────────────────────────
    if not st.first_done:
        spread = up_bid - dn_bid
        if abs(spread) < p.first_spread_min:
            st.last_up_bid = up_bid
            st.last_dn_bid = dn_bid
            return st, None
        # BSI primer
        if bsi is not None:
            if bsi >= BSI_THRESHOLD:
                direction = "UP"
            elif bsi <= -BSI_THRESHOLD:
                direction = "DOWN"
            else:
                direction = "UP" if spread > 0 else "DOWN"
        else:
            direction = "UP" if spread > 0 else "DOWN"
    else:
        imb = m.up_filled - m.down_filled
        if abs(imb) > p.imbalance_thr:
            direction = "DOWN" if imb > 0 else "UP"
        else:
            d_up = abs(up_bid - st.last_up_bid)
            d_dn = abs(dn_bid - st.last_dn_bid)
            if d_up == 0 and d_dn == 0:
                direction = "UP" if up_bid >= dn_bid else "DOWN"
            elif d_up >= d_dn:
                direction = "UP"
            else:
                direction = "DOWN"

    st.last_up_bid = up_bid
    st.last_dn_bid = dn_bid

    bid = up_bid if direction == "UP" else dn_bid
    ask = up_ask if direction == "UP" else dn_ask

    if bid <= 0 or ask <= 0:
        return st, None

    # Loser guard
    loser_opt = loser_side(up_bid, dn_bid)
    is_loser_dir = (loser_opt == direction)

    effective_min = min(p.loser_min_price, p.min_price) if is_loser_dir else p.min_price
    if bid < effective_min or bid > p.max_price:
        return st, None

    # Martingale-down guard
    avg_loser_max = p.avg_loser_max
    if direction == "UP":
        cur_filled, cur_avg = m.up_filled, m.avg_up
        opp_filled, opp_avg = m.down_filled, m.avg_down
    else:
        cur_filled, cur_avg = m.down_filled, m.avg_down
        opp_filled, opp_avg = m.up_filled, m.avg_up

    scalp_only = is_loser_dir and cur_filled > 0 and cur_avg > avg_loser_max
    is_scalp_band = is_loser_dir and bid <= p.loser_scalp_max_price and p.loser_scalp_usdc > 0

    if scalp_only and p.loser_scalp_usdc > 0:
        usdc = p.loser_scalp_usdc
    elif is_scalp_band:
        usdc = p.loser_scalp_usdc
    else:
        base = _interp_usdc(bid, p)
        lp_secs = float(p.late_pyramid_secs)
        if not is_loser_dir and lp_secs > 0 and to_end > 0 and to_end <= lp_secs:
            base = base * p.winner_size_factor
        usdc = base

    if usdc <= 0:
        return st, None

    is_any_scalp = scalp_only or is_scalp_band

    # Post-LW winner cap
    if st.lw_injections > 0 and not is_loser_dir and not is_any_scalp and bid > POST_LW_WINNER_MAX_BID:
        return st, None

    # Loser guard (non-scalp)
    if is_loser_dir and not is_any_scalp and bid > p.loser_scalp_max_price:
        st.last_up_bid = up_bid
        st.last_dn_bid = dn_bid
        return st, None

    order_price = ask if is_any_scalp else bid
    size = math.ceil(usdc / order_price)

    # avg_sum soft cap
    if not is_any_scalp and opp_filled > 0:
        new_avg = (cur_avg * cur_filled + order_price * size) / (cur_filled + size) if cur_filled > 0 else order_price
        if new_avg + opp_avg > p.max_avg_sum:
            return st, None

    # Minimum notional
    if size * order_price < p.api_min_order_size:
        return st, None

    if is_any_scalp:
        reason = f"bonereaper:scalp:{direction.lower()}"
    else:
        reason = f"bonereaper:buy:{direction.lower()}"

    trade = SimTrade(
        ts_ms=now_ms, outcome=direction, price=order_price,
        size=size, reason=reason, to_end=to_end,
        up_bid=up_bid, down_bid=dn_bid,
    )
    st.last_buy_ms = now_ms
    st.first_done = True
    return st, trade


# ─────────────────────────────────────────────────────────────────────────────
# DRYRUN FILL — executor.rs mantığı
# Maker (BID) emir → fills when price >= counter ask
# Scalp (ASK) → always fills (ASK >= ASK)
# ─────────────────────────────────────────────────────────────────────────────

def dryrun_will_fill(trade: SimTrade, tick: dict) -> bool:
    """Emir anında fill olur mu? (maker: bid >= karşı ask)"""
    if trade.outcome == "UP":
        counter_ask = tick["up_best_ask"]
    else:
        counter_ask = tick["down_best_ask"]
    return trade.price >= counter_ask


# ─────────────────────────────────────────────────────────────────────────────
# OTURUM SİMÜLASYONU
# ─────────────────────────────────────────────────────────────────────────────

def simulate_session(
    conn: sqlite3.Connection,
    bot_id: int,
    slug: str,
    p: BonereaperParams,
    verbose: bool = False,
) -> dict:
    cur = conn.cursor()

    # Market session bilgisi
    cur.execute(
        """SELECT id, start_ts, end_ts, min_order_size, tick_size
           FROM market_sessions WHERE bot_id=? AND slug=?""",
        (bot_id, slug),
    )
    sess = cur.fetchone()
    if not sess:
        return {"error": f"Oturum bulunamadı: bot={bot_id} slug={slug}"}
    sess_id, start_ts, end_ts, min_order_size, tick_size = sess
    p.api_min_order_size = min_order_size or 5.0

    # Tick verisi
    cur.execute(
        """SELECT ts_ms, up_best_bid, up_best_ask, down_best_bid, down_best_ask,
                  bsi, ofi, cvd, signal_score
           FROM market_ticks
           WHERE market_session_id=? AND bot_id=?
           ORDER BY ts_ms""",
        (sess_id, bot_id),
    )
    ticks_raw = cur.fetchall()
    tick_cols = ["ts_ms","up_best_bid","up_best_ask","down_best_bid","down_best_ask",
                 "bsi","ofi","cvd","signal_score"]
    ticks = []
    for row in ticks_raw:
        d = dict(zip(tick_cols, row))
        d["start_ts"] = start_ts
        d["end_ts"] = end_ts
        ticks.append(d)

    # Gerçek trade'ler — tam liste (gruplu, fill sayısı korunur)
    # Gerçek bot: 9 maker order → hepsi passive fill'de aynı anda → 9 ayrı DB kaydı.
    # Sim da aynı davranışı üretmeli → fill sayısı karşılaştırması için dupe_cnt kullan.
    cur.execute(
        """SELECT ts_ms, outcome, price, size, COUNT(*) dupe_cnt
           FROM trades
           WHERE bot_id=? AND market_session_id=?
           GROUP BY ts_ms, outcome, ROUND(price,4), ROUND(size,0)
           ORDER BY ts_ms""",
        (bot_id, sess_id),
    )
    real_trades_raw = cur.fetchall()
    real_trades = [
        {"ts_ms": r[0], "outcome": r[1], "price": r[2], "size": r[3], "dupe_cnt": r[4]}
        for r in real_trades_raw
    ]
    # ts_ms → real trade lookup
    real_by_ts: dict = {}
    for rt in real_trades:
        real_by_ts.setdefault(rt["ts_ms"], []).append(rt)

    # Replay
    # ─── Gerçek bot akışı (window.rs'den öğrenildi): ─────────────────────────
    # cadence.tick() her 1 sn:
    #   1) run_passive_fills_dryrun → open_orders'ı BBA ile karşılaştır, fill'leri yaz
    #   2) tick::tick → strateji karar, yeni open order'lar eklenebilir
    # book_rx (BBA değişimi): tick::tick çalışır (passive fill YOK)
    # Sonuç: fill_trades (DB kayıtları) = passive fill çıkışları + anında fill'ler.
    # market_ticks 1sn cadence snapshot → sim de 1sn aralıkla çalışır.
    # ─────────────────────────────────────────────────────────────────────────
    state: Optional[BonereaperActive] = None
    metrics = StrategyMetrics()
    fill_trades: List[SimTrade] = []   # DB'de kayıt olan fill'ler (karşılaştırma için)
    decision_trades: List[SimTrade] = []  # Strateji kararları (debug için)

    # Open orders: maker emir resting, sonraki tick'lerde passive fill
    open_orders: List[SimTrade] = []

    for tick in ticks:
        now_ms = tick["ts_ms"]

        # ── 1) Passive fills (cadence.tick → run_passive_fills_dryrun) ──────
        still_open = []
        for oo in open_orders:
            if dryrun_will_fill(oo, tick):
                fill_price = tick["up_best_ask"] if oo.outcome == "UP" else tick["down_best_ask"]
                fee = fill_price * oo.size * DRYRUN_FEE_RATE
                metrics.ingest_fill(oo.outcome, fill_price, oo.size, fee)
                # Fill kaydı: ts_ms = şimdiki tick (fill anı)
                fill_trade = SimTrade(
                    ts_ms=now_ms, outcome=oo.outcome, price=fill_price,
                    size=oo.size, reason=oo.reason + ":passive", to_end=oo.to_end,
                    up_bid=tick["up_best_bid"], down_bid=tick["down_best_bid"],
                )
                fill_trades.append(fill_trade)
                if verbose:
                    print(f"    [PASSIVE FILL] ts={now_ms} {oo.outcome} @{fill_price:.4f} "
                          f"sz={oo.size:.0f} avg_sum={metrics.avg_sum():.3f}")
            else:
                still_open.append(oo)
        open_orders = still_open

        # ── 2) Strateji kararı (cadence.tick → tick::tick) ──────────────────
        state, trade = bonereaper_decide(state, p, metrics, tick, start_ts)

        if trade is None:
            continue

        decision_trades.append(trade)

        # DryRun fill simülasyonu: BUY @ price >= karşı ASK → anında fill
        if dryrun_will_fill(trade, tick):
            fee = trade.price * trade.size * DRYRUN_FEE_RATE
            metrics.ingest_fill(trade.outcome, trade.price, trade.size, fee)
            fill_trades.append(trade)
            if verbose:
                print(f"  SIM FILL  ts={now_ms} {trade.reason} {trade.outcome} "
                      f"@{trade.price:.4f} sz={trade.size:.0f} to_end={trade.to_end:.0f}s "
                      f"avg_sum={metrics.avg_sum():.3f}")
        else:
            # Maker emir → open'a al (passive fill bekleniyor)
            open_orders.append(trade)
            if verbose:
                print(f"  SIM OPEN  ts={now_ms} {trade.reason} {trade.outcome} "
                      f"@{trade.price:.4f} sz={trade.size:.0f} (maker, resting, "
                      f"open_cnt={len(open_orders)})")

    sim_trades = fill_trades  # Karşılaştırma için fill_trades kullan

    # ─── Karşılaştırma ───────────────────────────────────────────────────────
    dupes_total = sum(rt["dupe_cnt"] - 1 for rt in real_trades)

    sim_by_ts: dict = {}
    for st_trade in sim_trades:
        sim_by_ts.setdefault(st_trade.ts_ms, []).append(st_trade)

    all_ts = sorted(set(list(real_by_ts.keys()) + list(sim_by_ts.keys())))
    matches = 0
    only_real = 0
    only_sim = 0
    both_diff = 0
    divergences = []

    for ts in all_ts:
        r_list = real_by_ts.get(ts, [])
        s_list = sim_by_ts.get(ts, [])

        r_count = len(r_list)
        s_count = len(s_list)

        if r_count == 0 and s_count > 0:
            only_sim += s_count
            divergences.append({
                "ts_ms": ts, "type": "SADECE_SIM",
                "sim": [f"{t.outcome}@{t.price:.4f}x{t.size:.0f} ({t.reason})" for t in s_list],
                "real": [],
            })
        elif r_count > 0 and s_count == 0:
            only_real += sum(r["dupe_cnt"] for r in r_list)
            divergences.append({
                "ts_ms": ts, "type": "SADECE_REAL",
                "real": [f"{r['outcome']}@{r['price']:.4f}x{r['size']:.0f}×{r['dupe_cnt']}" for r in r_list],
                "sim": [],
            })
        else:
            # Her ikisi de var: yön + fiyat + sayı eşleşiyor mu?
            # real dupe_cnt ile sim fill count karşılaştır
            def count_map(recs, is_real: bool) -> dict:
                m: dict = {}
                for rec in recs:
                    k = (rec["outcome"] if is_real else rec.outcome,
                         round(rec["price"] if is_real else rec.price, 4))
                    cnt = rec["dupe_cnt"] if is_real else 1
                    m[k] = m.get(k, 0) + cnt
                return m

            r_map = count_map(r_list, True)
            s_map = count_map(s_list, False)

            if r_map == s_map:
                matches += 1
            else:
                both_diff += 1
                divergences.append({
                    "ts_ms": ts, "type": "UYUMSUZ",
                    "real": [f"{r['outcome']}@{r['price']:.4f}x{r['size']:.0f}×{r['dupe_cnt']}" for r in r_list],
                    "sim": [f"{t.outcome}@{t.price:.4f}x{t.size:.0f} ({t.reason})" for t in s_list],
                })

    total_decision_ts = len(all_ts)
    match_rate = matches / total_decision_ts * 100 if total_decision_ts else 0

    # ─── PnL hesabı ─────────────────────────────────────────────────────────
    # Son tick'ten BBA al (session sonu fiyatları)
    last_tick = ticks[-1] if ticks else {}
    last_up_bid = last_tick.get("up_best_bid", 0.0) or 0.0
    last_dn_bid = last_tick.get("down_best_bid", 0.0) or 0.0

    uf = metrics.up_filled
    df = metrics.down_filled
    cb = metrics.cost_basis()
    fee = metrics.fee_total
    pairs = metrics.pair_count()
    imb_up = uf - pairs
    imb_dn = df - pairs

    pnl_if_up   = uf - cb - fee
    pnl_if_down = df - cb - fee
    mtm_pnl     = pairs + imb_up * last_up_bid + imb_dn * last_dn_bid - cb - fee

    # Gerçek bot son PnL snapshot (DB'den)
    cur.execute(
        """SELECT cost_basis, up_filled, down_filled, pnl_if_up, pnl_if_down, mtm_pnl,
                  pair_count, avg_up, avg_down
           FROM pnl_snapshots WHERE market_session_id=? AND bot_id=?
           ORDER BY id DESC LIMIT 1""",
        (sess_id, bot_id),
    )
    snap = cur.fetchone()
    real_pnl = {}
    if snap:
        snap_cols = ["cost_basis","up_filled","down_filled","pnl_if_up","pnl_if_down",
                     "mtm_pnl","pair_count","avg_up","avg_down"]
        real_pnl = dict(zip(snap_cols, snap))

    return {
        "slug": slug,
        "bot_id": bot_id,
        "start_ts": start_ts,
        "end_ts": end_ts,
        "ticks_total": len(ticks),
        "real_trade_ts": len(real_trades),
        "real_trade_records": sum(rt["dupe_cnt"] for rt in real_trades),
        "duplicate_records": dupes_total,
        "sim_trade_ts": len(fill_trades),
        "sim_decisions": len(decision_trades),
        "sim_open_remaining": len(open_orders),
        "sim_up": sum(1 for t in fill_trades if t.outcome == "UP"),
        "sim_down": sum(1 for t in fill_trades if t.outcome == "DOWN"),
        "real_up": sum(1 for t in real_trades if t["outcome"] == "UP"),
        "real_down": sum(1 for t in real_trades if t["outcome"] == "DOWN"),
        "sim_lw": sum(1 for t in fill_trades if "lw" in t.reason),
        "sim_scalp": sum(1 for t in fill_trades if "scalp" in t.reason),
        # PnL
        "sim_cost_basis": round(cb, 4),
        "sim_up_filled": uf,
        "sim_down_filled": df,
        "sim_avg_up": round(metrics.avg_up, 4),
        "sim_avg_down": round(metrics.avg_down, 4),
        "sim_pairs": pairs,
        "sim_fee": round(fee, 4),
        "sim_pnl_if_up": round(pnl_if_up, 4),
        "sim_pnl_if_down": round(pnl_if_down, 4),
        "sim_mtm_pnl": round(mtm_pnl, 4),
        "last_up_bid": last_up_bid,
        "last_dn_bid": last_dn_bid,
        "real_pnl": real_pnl,
        "matches": matches,
        "only_real": only_real,
        "only_sim": only_sim,
        "both_diff": both_diff,
        "match_rate_pct": match_rate,
        "divergences": divergences[:20],  # ilk 20 sapma
    }


# ─────────────────────────────────────────────────────────────────────────────
# DB YÜKLEME
# ─────────────────────────────────────────────────────────────────────────────

def load_bot_params(conn: sqlite3.Connection, bot_id: int) -> Optional[BonereaperParams]:
    import json
    cur = conn.cursor()
    cur.execute(
        "SELECT id, name, order_usdc, min_price, max_price, cooldown_threshold, "
        "strategy_params FROM bots WHERE id=?",
        (bot_id,),
    )
    row = cur.fetchone()
    if not row:
        return None
    cols = ["id","name","order_usdc","min_price","max_price","cooldown_threshold","strategy_params"]
    d = dict(zip(cols, row))
    params_json = json.loads(d.get("strategy_params") or "{}")
    # null değerleri temizle
    params_json = {k: v for k, v in params_json.items() if v is not None}
    return BonereaperParams.from_db(d, params_json)


def load_sessions(conn: sqlite3.Connection, bot_id: int, last_n: int) -> List[str]:
    cur = conn.cursor()
    cur.execute(
        "SELECT slug FROM market_sessions WHERE bot_id=? ORDER BY start_ts DESC LIMIT ?",
        (bot_id, last_n),
    )
    return [r[0] for r in cur.fetchall()]


def download_db(ssh_host: str, pem: Optional[str]) -> str:
    tmp = tempfile.mktemp(suffix=".db")
    cmd = ["scp"]
    if pem:
        cmd += ["-i", pem]
    cmd += ["-o", "StrictHostKeyChecking=accept-new"]
    cmd += [f"{ssh_host}:/home/ubuntu/baiter/data/baiter.db", tmp]
    print(f"  DB indiriliyor: {' '.join(cmd)}")
    subprocess.run(cmd, check=True)
    return tmp


# ─────────────────────────────────────────────────────────────────────────────
# ÇIKTI FORMATI
# ─────────────────────────────────────────────────────────────────────────────

def print_session_result(r: dict, verbose_div: bool = False):
    if "error" in r:
        print(f"  ❌ HATA: {r['error']}")
        return

    mr = r["match_rate_pct"]
    icon = "✅" if mr >= 80 else ("⚠️" if mr >= 50 else "❌")
    print(f"\n{'─'*60}")
    print(f"  {icon}  {r['slug']}  (bot={r['bot_id']})")
    print(f"  Tick sayısı       : {r['ticks_total']}")
    print(f"  Gerçek karar anı  : {r['real_trade_ts']}  "
          f"(toplam kayıt: {r['real_trade_records']}, duplikat: {r['duplicate_records']})")
    print(f"  Sim fill sayısı   : {r['sim_trade_ts']}  "
          f"(UP:{r['sim_up']} DN:{r['sim_down']} LW:{r['sim_lw']} scalp:{r['sim_scalp']}) "
          f"| kararlar:{r.get('sim_decisions','-')} açık_emir:{r.get('sim_open_remaining','-')}")
    print(f"  Real UP/DOWN      : {r['real_up']} / {r['real_down']}")
    print(f"  Eşleşme oranı     : {mr:.1f}%  "
          f"(match={r['matches']} sadece_real={r['only_real']} "
          f"sadece_sim={r['only_sim']} uyumsuz={r['both_diff']})")
    # PnL
    cb = r.get("sim_cost_basis", 0)
    uf = r.get("sim_up_filled", 0)
    df = r.get("sim_down_filled", 0)
    fee = r.get("sim_fee", 0)
    pnl_up   = r.get("sim_pnl_if_up", 0)
    pnl_down = r.get("sim_pnl_if_down", 0)
    mtm      = r.get("sim_mtm_pnl", 0)
    avg_up   = r.get("sim_avg_up", 0)
    avg_dn   = r.get("sim_avg_down", 0)
    pairs    = r.get("sim_pairs", 0)
    rp       = r.get("real_pnl", {})

    print(f"  ── SIM PnL ──────────────────────────────────────────────────")
    print(f"  Cost basis        : {cb:.2f}$   fee: {fee:.2f}$")
    print(f"  UP : {uf:6.0f}sh  avg={avg_up:.4f}   DOWN: {df:6.0f}sh  avg={avg_dn:.4f}   pairs={pairs:.0f}")
    print(f"  pnl_if_UP         : {pnl_up:+.2f}$")
    print(f"  pnl_if_DOWN       : {pnl_down:+.2f}$")
    print(f"  MTM PnL           : {mtm:+.2f}$  (last_up_bid={r.get('last_up_bid',0):.2f} last_dn_bid={r.get('last_dn_bid',0):.2f})")
    if rp:
        print(f"  ── REAL BOT PnL (DB son snapshot) ──")
        print(f"  Real cost_basis   : {rp.get('cost_basis',0):.2f}$")
        print(f"  Real UP/DN fills  : {rp.get('up_filled',0):.0f}sh / {rp.get('down_filled',0):.0f}sh")
        print(f"  Real pnl_if_UP    : {rp.get('pnl_if_up',0):+.2f}$")
        print(f"  Real pnl_if_DOWN  : {rp.get('pnl_if_down',0):+.2f}$")
        print(f"  Real MTM PnL      : {rp.get('mtm_pnl',0):+.2f}$")

    if verbose_div and r["divergences"]:
        print(f"\n  --- İlk {len(r['divergences'])} SAPMA ---")
        for dv in r["divergences"]:
            ts_s = dv["ts_ms"] / 1000.0
            print(f"  [{dv['type']}] ts={dv['ts_ms']} ({ts_s:.0f}s)")
            if dv["real"]:
                print(f"    Real : {', '.join(dv['real'])}")
            if dv["sim"]:
                print(f"    Sim  : {', '.join(dv['sim'])}")


def print_summary(results: List[dict], p: BonereaperParams):
    good = [r for r in results if "error" not in r]
    if not good:
        return
    avg_mr = sum(r["match_rate_pct"] for r in good) / len(good)
    total_real = sum(r["real_trade_ts"] for r in good)
    total_sim  = sum(r["sim_trade_ts"] for r in good)
    total_dupes = sum(r["duplicate_records"] for r in good)

    # PnL toplamları
    total_cb       = sum(r.get("sim_cost_basis", 0) for r in good)
    total_fee      = sum(r.get("sim_fee", 0) for r in good)
    total_uf       = sum(r.get("sim_up_filled", 0) for r in good)
    total_df       = sum(r.get("sim_down_filled", 0) for r in good)
    total_pnl_up   = sum(r.get("sim_pnl_if_up", 0) for r in good)
    total_pnl_down = sum(r.get("sim_pnl_if_down", 0) for r in good)
    total_mtm      = sum(r.get("sim_mtm_pnl", 0) for r in good)
    total_lw       = sum(r.get("sim_lw", 0) for r in good)
    total_scalp    = sum(r.get("sim_scalp", 0) for r in good)

    # Real bot PnL toplamları
    real_cb   = sum(r.get("real_pnl", {}).get("cost_basis", 0) for r in good)
    real_pup  = sum(r.get("real_pnl", {}).get("pnl_if_up", 0) for r in good)
    real_pdn  = sum(r.get("real_pnl", {}).get("pnl_if_down", 0) for r in good)
    real_mtm  = sum(r.get("real_pnl", {}).get("mtm_pnl", 0) for r in good)

    print(f"\n{'═'*60}")
    print("  ÖZET")
    print(f"  Oturum sayısı      : {len(good)}")
    print(f"  Ort. eşleşme oranı : {avg_mr:.1f}%")
    print(f"  Toplam gerçek karar: {total_real}  (duplikat kayıt: {total_dupes})")
    print(f"  Toplam sim fill    : {total_sim}")
    print()
    print(f"  ── SIM PnL TOPLAM ─────────────────────────────────────────")
    print(f"  Toplam cost basis  : {total_cb:+.2f}$")
    print(f"  Toplam fee         : {total_fee:+.2f}$")
    print(f"  Toplam UP fills    : {total_uf:.0f} sh")
    print(f"  Toplam DOWN fills  : {total_df:.0f} sh")
    print(f"  Toplam LW fill     : {total_lw}")
    print(f"  Toplam scalp fill  : {total_scalp}")
    print(f"  pnl_if_ALL_UP      : {total_pnl_up:+.2f}$")
    print(f"  pnl_if_ALL_DOWN    : {total_pnl_down:+.2f}$")
    print(f"  MTM PnL (kümülatif): {total_mtm:+.2f}$")
    print()
    print(f"  ── REAL BOT PnL TOPLAM (DB) ────────────────────────────────")
    print(f"  Real cost basis    : {real_cb:+.2f}$")
    print(f"  Real pnl_if_ALL_UP : {real_pup:+.2f}$")
    print(f"  Real pnl_if_ALL_DN : {real_pdn:+.2f}$")
    print(f"  Real MTM PnL (küm) : {real_mtm:+.2f}$")
    print(f"\n  Bot parametreleri  :")
    print(f"    buy_cooldown_ms   = {p.buy_cooldown_ms}")
    print(f"    late_winner_secs  = {p.late_winner_secs}")
    print(f"    late_winner_bid_thr= {p.late_winner_bid_thr}")
    print(f"    late_winner_usdc  = {p.late_winner_usdc}")
    print(f"    lw_max_per_session= {p.lw_max_per_session}")
    print(f"    max_avg_sum       = {p.max_avg_sum}")
    print(f"    imbalance_thr     = {p.imbalance_thr}")
    print(f"    size_longshot@0.30= {p.size_longshot_usdc}  (anchor)")
    print(f"    size_mid@0.65    = {p.size_mid_usdc}  (anchor, lineer interp)")
    print(f"    size_high@lw_thr = {p.size_high_usdc}  (anchor, lineer interp)")
    print(f"    loser_scalp_usdc  = {p.loser_scalp_usdc}")
    print(f"    loser_scalp_max_p = {p.loser_scalp_max_price}")
    print(f"    max_price         = {p.max_price}")


# ─────────────────────────────────────────────────────────────────────────────
# MAIN
# ─────────────────────────────────────────────────────────────────────────────

def main():
    ap = argparse.ArgumentParser(description="Bonereaper simülasyon + karşılaştırma")
    ap.add_argument("--db", help="Yerel SQLite DB yolu")
    ap.add_argument("--ssh", help="SSH host (örn. ubuntu@79.125.42.234)")
    ap.add_argument("--pem", help="SSH PEM key yolu")
    ap.add_argument("--bot", type=int, required=True, help="Bot ID (ör. 135)")
    ap.add_argument("--slug", help="Tek oturum slug")
    ap.add_argument("--all-sessions", action="store_true", help="Tüm oturumları sim et")
    ap.add_argument("--last", type=int, default=10, help="Son N oturum (default: 10)")
    ap.add_argument("--verbose", action="store_true", help="Her kararı yazdır")
    ap.add_argument("--show-divergences", action="store_true", help="Sapmaları göster")
    ap.add_argument(
        "--max-avg-sum",
        type=float,
        default=None,
        help="bonereaper_max_avg_sum geçersiz kıl",
    )
    ap.add_argument("--lw-usdc", type=float, default=None, help="bonereaper_late_winner_usdc geçersiz kıl")
    ap.add_argument("--lw-max", type=int, default=None, help="bonereaper_lw_max_per_session geçersiz kıl")
    ap.add_argument("--imbalance-thr", type=float, default=None, help="bonereaper_imbalance_thr geçersiz kıl")
    ap.add_argument("--lw-secs", type=int, default=None, help="bonereaper_late_winner_secs geçersiz kıl")
    args = ap.parse_args()

    # DB yolu
    db_path = args.db
    tmp_db = None
    if db_path is None:
        if args.ssh:
            print("Sunucudan DB indiriliyor...")
            db_path = download_db(args.ssh, args.pem)
            tmp_db = db_path
        else:
            # Varsayılan: proje içindeki DB
            candidates = [
                os.path.join(os.path.dirname(__file__), "..", "data", "baiter.db"),
                "./data/baiter.db",
            ]
            for c in candidates:
                if os.path.exists(c):
                    db_path = c
                    break
            if not db_path:
                print("❌ DB bulunamadı. --db veya --ssh kullanın.", file=sys.stderr)
                sys.exit(1)

    print(f"DB: {db_path}")
    conn = sqlite3.connect(db_path)

    try:
        p = load_bot_params(conn, args.bot)
        if p is None:
            print(f"❌ Bot {args.bot} bulunamadı.", file=sys.stderr)
            sys.exit(1)
        if args.max_avg_sum is not None:
            p.max_avg_sum = float(args.max_avg_sum)
        if args.lw_usdc is not None:
            p.late_winner_usdc = float(args.lw_usdc)
        if args.lw_max is not None:
            p.lw_max_per_session = int(args.lw_max)
        if args.imbalance_thr is not None:
            p.imbalance_thr = float(args.imbalance_thr)
        if args.lw_secs is not None:
            p.late_winner_secs = int(args.lw_secs)
        print(f"Bot {args.bot} yüklendi: anchors {p.size_longshot_usdc}/"
              f"{p.size_mid_usdc}/{p.size_high_usdc}$ (ls/mid/hi) / "
              f"lw={p.late_winner_usdc}$@{p.late_winner_bid_thr} / "
              f"cd={p.buy_cooldown_ms}ms / max_avg={p.max_avg_sum} / "
              f"lw_max={p.lw_max_per_session} / imb_thr={p.imbalance_thr}")

        # Oturum listesi
        if args.slug:
            slugs = [args.slug]
        elif args.all_sessions:
            cur = conn.cursor()
            cur.execute(
                "SELECT slug FROM market_sessions WHERE bot_id=? ORDER BY start_ts",
                (args.bot,),
            )
            slugs = [r[0] for r in cur.fetchall()]
        else:
            slugs = load_sessions(conn, args.bot, args.last)
            slugs = list(reversed(slugs))  # eskiden yeniye

        print(f"{len(slugs)} oturum simüle edilecek...\n")

        results = []
        for slug in slugs:
            r = simulate_session(conn, args.bot, slug, p, verbose=args.verbose)
            print_session_result(r, verbose_div=args.show_divergences or args.verbose)
            results.append(r)

        print_summary(results, p)

    finally:
        conn.close()
        if tmp_db and os.path.exists(tmp_db):
            os.unlink(tmp_db)


if __name__ == "__main__":
    main()
