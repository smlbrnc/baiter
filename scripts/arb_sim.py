#!/usr/bin/env python3
"""
Predictive 1-Tick-Below Bid Arbitraj Simülasyonu
=================================================
Strateji özeti (araştırmadan türetilmiş):
  • Her POLL_INTERVAL_S saniyede bir UP ve DOWN kitabını oku.
  • Her iki taraf için mevcut bid - TICK_SIZE fiyatına GTC limit emir ver.
    ("1 tick altında önceden tahmin")
  • Pair cost < SUM_TARGET ise işlem kur.
  • Bir taraf fill olursa karşı tarafı anında hedge et:
      hedge_price = SUM_TARGET - filled_price
  • Her 2 sn'de stale emirleri iptal, yeni fiyatla yeniden gönder.
  • FLOOR_PRICE ve EARLY_TAKE_PROFIT ile stop-loss / erken çıkış.

Referanslar (araştırma):
  • luoyelittledream (Medium, Mar 2026) — "Polymarket Binary Hedging Arbitrage"
  • PolybaseX/Polymarket-Arbitrage-Trading-Bot (GitHub)
  • 0xFives/Polymarket-Arbitrage-Crypto-Trading-Bot-V3 (GitHub)
"""

from __future__ import annotations

import json
import math
from dataclasses import dataclass, field
from pathlib import Path
from typing import Optional

# ─────────────────────────────────────────────
# PARAMETRELER
# ─────────────────────────────────────────────
TICK_SIZE = 0.01          # Polymarket minimum fiyat adımı
POLL_INTERVAL_S = 2       # Her kaç saniyede emir kontrol et
SUM_TARGET = 0.97         # Hedef pair cost (< 1 = garantili kâr)
SHARES = 40.0             # Her emirde kaç share
FLOOR_PRICE = 0.04        # Stop-loss: bu fiyata düşerse pozisyonu kapat
EARLY_TAKE_PROFIT = 0.15  # Bid %15 yükselirse erken çıkış
LAST_MIN_S = 30           # Son N saniyede farklı mantık (Leg2 deneme)
ORDER_TIMEOUT_S = 8       # Fill gelmezse bu kadar saniyede stale kabul et
MAX_OPEN_LEGS = 3         # Aynı anda açık tek-bacak pozisyon limiti

# Simülasyon fill modeli
# Bir buy emri P'de fill olur eğer gelecekteki TICK'te:
#   best_ask <= P  (biri bize sattı)
#   veya mid <= P  (fiyat P'ye indi, bid tarafı doldu)
FILL_MODEL = "ask_cross"   # "ask_cross" | "bid_reach"

# ─────────────────────────────────────────────
# VERİ YAPILARI
# ─────────────────────────────────────────────

@dataclass
class Tick:
    ts: float   # saniye
    up_bid: float
    up_ask: float
    dn_bid: float
    dn_ask: float

    @property
    def pair_cost_ask(self) -> float:
        return self.up_ask + self.dn_ask

    @property
    def pair_cost_bid(self) -> float:
        return self.up_bid + self.dn_bid

    def ask(self, side: str) -> float:
        return self.up_ask if side == "UP" else self.dn_ask

    def bid(self, side: str) -> float:
        return self.up_bid if side == "UP" else self.dn_bid


@dataclass
class Order:
    side: str        # "UP" | "DOWN"
    price: float
    size: float
    placed_at: float  # ts saniye
    filled_at: Optional[float] = None
    fill_price: Optional[float] = None
    cancelled: bool = False
    is_hedge: bool = False

    def is_stale(self, now: float) -> bool:
        return (now - self.placed_at) >= ORDER_TIMEOUT_S and self.filled_at is None and not self.cancelled


@dataclass
class Position:
    """Açık tek-bacak veya iki-bacaklı pozisyon."""
    leg1: Order
    leg2: Optional[Order] = None
    closed_at: Optional[float] = None
    close_reason: str = ""
    pnl: float = float("nan")


@dataclass
class SimState:
    positions: list[Position] = field(default_factory=list)
    pending_orders: list[Order] = field(default_factory=list)
    last_poll: float = 0.0
    total_invested: float = 0.0
    total_redeemed: float = 0.0
    fills: list[tuple[float, str, float, float]] = field(default_factory=list)  # (ts, side, price, size)
    log: list[str] = field(default_factory=list)


# ─────────────────────────────────────────────
# YARDIMCI
# ─────────────────────────────────────────────

def tick_floor(price: float) -> float:
    """En yakın tick'e yuvarla (aşağı)."""
    return math.floor(price / TICK_SIZE) * TICK_SIZE


def check_fill(order: Order, tick: Tick) -> bool:
    """Simülasyon fill kontrolü."""
    if order.cancelled or order.filled_at is not None:
        return False
    ask = tick.ask(order.side)
    bid = tick.bid(order.side)
    if FILL_MODEL == "ask_cross":
        return ask <= order.price
    elif FILL_MODEL == "bid_reach":
        return bid >= order.price
    return False


# ─────────────────────────────────────────────
# ANA SİMÜLASYON
# ─────────────────────────────────────────────

def run_arb_sim(ticks: list[Tick], epoch: int, winner: str, verbose: bool = True) -> dict:
    st = SimState()
    st.last_poll = ticks[0].ts
    open_single_legs: list[Position] = []   # sadece leg1 filled, leg2 bekliyor

    def log(msg: str):
        if verbose:
            print(msg)
        st.log.append(msg)

    log(f"\n{'='*60}")
    log(f"Epoch {epoch}  |  SUM_TARGET={SUM_TARGET}  |  SHARES={SHARES}")
    log(f"{'='*60}")

    for i, tk in enumerate(ticks):
        t_off = int(tk.ts - epoch)

        # ── Fill kontrolü ─────────────────────────────────────────
        for order in list(st.pending_orders):
            if check_fill(order, tk):
                order.filled_at = tk.ts
                order.fill_price = order.price
                st.fills.append((tk.ts, order.side, order.price, order.size))
                cost = order.price * order.size
                st.total_invested += cost
                log(f"  ✓ FILL  t={t_off:3d}s {order.side:4s} @{order.price:.4f} x{order.size:.0f} "
                    f"({'hedge' if order.is_hedge else 'açılış'}) cost=${cost:.2f}")

        # ── Tek-bacak pozisyonları: hedge durumlarını güncelle ─────
        for pos in list(open_single_legs):
            leg1 = pos.leg1
            if leg1.filled_at is None or pos.leg2 is not None:
                continue
            # Leg1 fill, leg2 henüz konmamış → hedge ver
            hedge_price = tick_floor(SUM_TARGET - leg1.fill_price)
            hedge_price = max(FLOOR_PRICE + TICK_SIZE, hedge_price)
            opp = "DOWN" if leg1.side == "UP" else "UP"
            hedge = Order(side=opp, price=hedge_price, size=leg1.size,
                          placed_at=tk.ts, is_hedge=True)
            pos.leg2 = hedge
            st.pending_orders.append(hedge)
            log(f"  → HEDGE t={t_off:3d}s {opp:4s} @{hedge_price:.4f} "
                f"(pair_cost={leg1.fill_price+hedge_price:.3f})")

        # ── Hedge fill → pozisyon kapat ────────────────────────────
        for pos in list(open_single_legs):
            if pos.leg2 and pos.leg2.filled_at is not None and pos.closed_at is None:
                pair_cost = pos.leg1.fill_price + pos.leg2.fill_price
                profit = (1.0 - pair_cost) * pos.leg1.size
                pos.pnl = profit
                pos.closed_at = tk.ts
                pos.close_reason = "HEDGED_COMPLETE"
                open_single_legs.remove(pos)
                st.positions.append(pos)
                log(f"  ★ ARB t={t_off:3d}s  pair_cost={pair_cost:.4f}  "
                    f"garantili_kâr=${profit:.2f}")

        # ── Floor price / erken çıkış kontrolü ────────────────────
        remaining = (epoch + 300) - tk.ts
        is_last_min = remaining <= LAST_MIN_S

        for pos in list(open_single_legs):
            if pos.closed_at is not None:
                continue
            leg1 = pos.leg1
            if leg1.filled_at is None:
                continue
            hold_bid = tk.bid(leg1.side)
            profit_line = leg1.fill_price * (1 + EARLY_TAKE_PROFIT)
            need_close = False
            reason = ""
            if hold_bid <= FLOOR_PRICE:
                need_close = True; reason = "FLOOR_PRICE"
            elif not is_last_min and hold_bid >= profit_line:
                need_close = True; reason = "EARLY_TAKE_PROFIT"
            elif is_last_min:
                if winner == leg1.side:
                    pos.pnl = (1.0 - leg1.fill_price) * leg1.size
                    pos.close_reason = "SETTLEMENT_WIN"
                else:
                    pos.pnl = (0.0 - leg1.fill_price) * leg1.size
                    pos.close_reason = "SETTLEMENT_LOSS"
                pos.closed_at = tk.ts
                open_single_legs.remove(pos)
                st.positions.append(pos)
                if pos.leg2:
                    pos.leg2.cancelled = True
                log(f"  {'✓' if pos.pnl > 0 else '✗'} SETTLE t={t_off:3d}s {leg1.side} "
                    f"pnl=${pos.pnl:.2f} ({pos.close_reason})")
                continue
            if need_close:
                close_price = hold_bid
                pos.pnl = (close_price - leg1.fill_price) * leg1.size
                pos.closed_at = tk.ts
                pos.close_reason = reason
                open_single_legs.remove(pos)
                st.positions.append(pos)
                if pos.leg2:
                    pos.leg2.cancelled = True
                log(f"  {'↑' if pos.pnl > 0 else '↓'} EXIT t={t_off:3d}s {leg1.side} @{close_price:.4f} "
                    f"pnl=${pos.pnl:.2f} ({reason})")

        # ── Stale emir temizleme ───────────────────────────────────
        for order in list(st.pending_orders):
            if order.is_stale(tk.ts) and not order.cancelled:
                order.cancelled = True
                st.pending_orders.remove(order)

        # ── POLL: her POLL_INTERVAL_S saniyede yeni emir ──────────
        if (tk.ts - st.last_poll) < POLL_INTERVAL_S:
            continue

        st.last_poll = tk.ts
        open_count = len(open_single_legs)

        if open_count >= MAX_OPEN_LEGS:
            continue   # Mevcut tek-bacak pozisyon limiti aşıldı

        # Pair cost fırsatı var mı?
        # Eğer bid fiyatlarının 1 tick altına emirler koyarsak
        up_entry = tick_floor(tk.up_bid - TICK_SIZE)
        dn_entry = tick_floor(tk.dn_bid - TICK_SIZE)
        proj_pair_cost = up_entry + dn_entry

        # Arb penceresi: mevcut ask'tan da kontrol (ask < SUM_TARGET)
        ask_pair = tk.pair_cost_ask
        bid_pair = tk.pair_cost_bid
        entry_pair = proj_pair_cost  # bid-1tick ile girilirse

        if entry_pair >= SUM_TARGET or ask_pair >= 1.00:
            # Düz arb yok, ama tek taraf ucuzlamış mı?
            # UP ucuz: up_ask < 0.45, DOWN ask < 0.45
            up_cheap = tk.up_ask < 0.45
            dn_cheap = tk.dn_ask < 0.45
            if not (up_cheap or dn_cheap):
                continue

        # Her iki taraf için emir ver
        orders_to_place = []
        for side, entry in [("UP", up_entry), ("DOWN", dn_entry)]:
            if entry < FLOOR_PRICE:
                continue
            if entry > 0.95:
                continue  # Pahalı tarafa bid koyma
            # Mevcut açık emirlerde aynı taraf var mı?
            already = any(
                o.side == side and o.filled_at is None and not o.cancelled
                for o in st.pending_orders
            )
            if already:
                continue
            orders_to_place.append(Order(side=side, price=entry, size=SHARES, placed_at=tk.ts))

        for order in orders_to_place:
            st.pending_orders.append(order)
            pos = Position(leg1=order)
            open_single_legs.append(pos)
            log(f"  ⊕ ORDER t={t_off:3d}s {order.side:4s} @{order.price:.4f} "
                f"(bid={tk.bid(order.side):.4f} ask={tk.ask(order.side):.4f} "
                f"pair_ask={ask_pair:.3f})")

    # Kapanmamış pozisyonlar → settlement
    for pos in list(open_single_legs):
        if pos.closed_at is not None:
            continue
        leg1 = pos.leg1
        if leg1.filled_at is not None:
            pos.pnl = (1.0 - leg1.fill_price) * leg1.size if winner == leg1.side else \
                      (-leg1.fill_price) * leg1.size
            pos.close_reason = "SETTLE_END"
            pos.closed_at = ticks[-1].ts
        st.positions.append(pos)

    # ── Özet ──────────────────────────────────────────────────────
    total_fills = len([o for o in st.fills])
    closed = [p for p in st.positions if not math.isnan(p.pnl)]
    total_pnl = sum(p.pnl for p in closed)
    arb_wins = [p for p in closed if p.close_reason == "HEDGED_COMPLETE"]
    settle_wins = [p for p in closed if "WIN" in p.close_reason]
    settle_loss = [p for p in closed if "LOSS" in p.close_reason]

    log(f"\n{'─'*60}")
    log(f"Epoch {epoch} | winner={winner}")
    log(f"Toplam emir fill: {total_fills}")
    log(f"Garantili arb (iki bacak): {len(arb_wins)}  "
        f"avg_pair_cost={sum(p.leg1.fill_price+p.leg2.fill_price for p in arb_wins)/len(arb_wins):.4f}"
        if arb_wins else "Garantili arb: 0")
    log(f"Settlement kazanç: {len(settle_wins)}  kayıp: {len(settle_loss)}")
    log(f"Toplam tahmini PnL: ${total_pnl:+.2f}")

    return {
        "epoch": epoch,
        "winner": winner,
        "fills": total_fills,
        "arb_count": len(arb_wins),
        "settle_wins": len(settle_wins),
        "settle_loss": len(settle_loss),
        "total_pnl": total_pnl,
        "positions": closed,
    }


# ─────────────────────────────────────────────
# VERİ YÜKLEME
# ─────────────────────────────────────────────

def load_ticks(epoch: int) -> list[Tick]:
    path = Path(f"exports/bot14-ticks-20260429/btc-updown-5m-{epoch}_ticks.json")
    raw = json.loads(path.read_text())
    return [Tick(
        ts=r["ts_ms"] / 1000.0,
        up_bid=float(r["up_best_bid"]),
        up_ask=float(r["up_best_ask"]),
        dn_bid=float(r["down_best_bid"]),
        dn_ask=float(r["down_best_ask"]),
    ) for r in raw]


def infer_winner(ticks: list[Tick]) -> str:
    for tk in reversed(ticks[-30:]):
        if tk.up_ask >= 0.80 and tk.dn_ask <= 0.25:
            return "UP"
        if tk.dn_ask >= 0.80 and tk.up_ask <= 0.25:
            return "DOWN"
    return "?"


# ─────────────────────────────────────────────
# ÇALIŞTIRICISI
# ─────────────────────────────────────────────

def main():
    EPOCHS = [1777467000 + 300 * i for i in range(6)]

    print("\n" + "="*70)
    print("Predictive 1-Tick-Below Bid Arbitraj Simülasyonu")
    print(f"SUM_TARGET={SUM_TARGET}  TICK={TICK_SIZE}  POLL={POLL_INTERVAL_S}s  SHARES={SHARES}")
    print("="*70)

    all_results = []
    for epoch in EPOCHS:
        ticks = load_ticks(epoch)
        winner = infer_winner(ticks)
        result = run_arb_sim(ticks, epoch, winner, verbose=True)
        all_results.append(result)

    # Global özet
    print("\n" + "="*70)
    print("GLOBAL ÖZET")
    print("="*70)
    print(f"{'Epoch':12s} | {'winner':6s} | {'fills':5s} | {'arb':3s} | {'W/L':5s} | {'PnL':>10s}")
    print("-"*55)
    total_pnl = 0.0
    for r in all_results:
        wl = f"{r['settle_wins']}W/{r['settle_loss']}L"
        print(f"{r['epoch']:12d} | {r['winner']:6s} | {r['fills']:5d} | "
              f"{r['arb_count']:3d} | {wl:5s} | ${r['total_pnl']:>+8.2f}")
        total_pnl += r["total_pnl"]
    print("-"*55)
    print(f"{'TOPLAM':12s} | {'':6s} | {'':5s} | {'':3s} | {'':5s} | ${total_pnl:>+8.2f}")

    # Strateji notları
    print("\n" + "="*70)
    print("STRATEJİ NOTLARI (araştırmadan)")
    print("="*70)
    notes = [
        "1. Bid-1tick maker stratejisi: fiyat çekildiğinde ucuz fill → garantili arb",
        f"2. SUM_TARGET={SUM_TARGET}: her iki bacak dolduğunda ${1-SUM_TARGET:.2f}/share net kâr",
        "3. Leg2 hedge: leg1 fill fiyatına göre karşı tarafı SUM_TARGET - leg1 fiyatına ver",
        f"4. FLOOR_PRICE={FLOOR_PRICE}: bu fiyata düşerse zarar kes",
        f"5. EARLY_TAKE_PROFIT={EARLY_TAKE_PROFIT*100:.0f}%: bu kadar yükselirse erken çıkış",
        f"6. Son {LAST_MIN_S}s: Leg2 denemesi durdurulur, settlement beklenir",
        "7. On-chain latency için emir timeout ve retry mekanizması gerekli (canlıda)",
        "8. Polymarket CLOB API: py-clob-client (resmi Python SDK) ile entegrasyon",
    ]
    for n in notes:
        print(f"  {n}")


if __name__ == "__main__":
    main()
