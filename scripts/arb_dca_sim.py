#!/usr/bin/env python3
"""
DCA Arbitraj Simülasyonu — Fiyat Düştükçe Ortalama Düşür
=========================================================
Strateji:
  • Her POLL_INTERVAL_S (2s) fiyatı kontrol et.
  • UP ve DOWN her taraf için mevcut bid - TICK_SIZE'a yeni emir ver.
  • Fiyat düştükçe yeniden emir ver → ortalama maliyeti düşür (DCA).
  • Her taraf için kümülatif avg_price ve total_shares takip et.
  • HEDGE: UP avg + DOWN avg < SUM_TARGET olduğunda HEDGE emri gönder.
  • Hedge fill olursa pozisyon kilitlendi → garantili kâr.
  • Market başından (t=0) sonuna (t=300s) kesintisiz devam.
  • Tek taraf riski için FLOOR_PRICE ve MAX_USDC_PER_SIDE guard.
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
TICK_SIZE        = 0.01    # Minimum fiyat adımı
POLL_INTERVAL_S  = 2       # Kaç saniyede bir fiyat kontrol
SUM_TARGET       = 0.97    # Hedef pair cost (< 1 = arbitraj)
SHARES_PER_ORDER = 40.0    # Her emir kaç share
MAX_USDC_PER_SIDE= 400.0   # Her tarafta maksimum USDC harcama
FLOOR_PRICE      = 0.03    # Altına düşerse o tarafı dondur
DROP_THRESHOLD   = 0.01    # Bid bu kadar düşünce yeni emir ver (DCA tetik)
MAX_FILL_WINDOW  = 4       # Fill simülasyon toleransı (saniye)

# ─────────────────────────────────────────────
# VERİ YAPILARI
# ─────────────────────────────────────────────

@dataclass
class Tick:
    ts: float
    up_bid: float
    up_ask: float
    dn_bid: float
    dn_ask: float

    def bid(self, side: str) -> float:
        return self.up_bid if side == "UP" else self.dn_bid

    def ask(self, side: str) -> float:
        return self.up_ask if side == "UP" else self.dn_ask

    @property
    def pair_cost_ask(self) -> float:
        return self.up_ask + self.dn_ask


@dataclass
class Fill:
    side: str
    price: float
    shares: float
    ts: float
    is_hedge: bool = False


@dataclass
class SideState:
    """Tek taraf için birikimli pozisyon durumu."""
    side: str
    fills: list[Fill] = field(default_factory=list)
    pending_price: Optional[float] = None   # Açık emir fiyatı
    pending_placed_at: Optional[float] = None
    last_bid_price: float = 0.0             # Son DCA'nın yapıldığı bid fiyatı
    hedged_shares: float = 0.0             # Hedge edilen share miktarı

    @property
    def total_shares(self) -> float:
        return sum(f.shares for f in self.fills if not f.is_hedge)

    @property
    def total_cost(self) -> float:
        return sum(f.price * f.shares for f in self.fills if not f.is_hedge)

    @property
    def avg_price(self) -> float:
        return self.total_cost / self.total_shares if self.total_shares > 0 else 0.0

    @property
    def unhedged_shares(self) -> float:
        return self.total_shares - self.hedged_shares

    @property
    def unhedged_cost(self) -> float:
        return self.avg_price * self.unhedged_shares

    def has_pending(self) -> bool:
        return self.pending_price is not None

    def cancel_pending(self):
        self.pending_price = None
        self.pending_placed_at = None

    def place(self, price: float, ts: float):
        self.pending_price = price
        self.pending_placed_at = ts
        self.last_bid_price = price + TICK_SIZE  # bid fiyatını sakla

    def try_fill(self, tick: Tick) -> Optional[Fill]:
        """Simülasyon: bid fiyatı emir fiyatına inerse fill."""
        if not self.has_pending():
            return None
        current_ask = tick.ask(self.side)
        current_bid = tick.bid(self.side)
        # Fill koşulu: piyasa ask'ı emir fiyatının altına indi (biri bize sattı)
        # ya da bid fiyatı emir fiyatına ulaştı (bid fill)
        if current_ask <= self.pending_price or current_bid >= self.pending_price:
            fill = Fill(
                side=self.side,
                price=self.pending_price,
                shares=SHARES_PER_ORDER,
                ts=tick.ts,
            )
            self.fills.append(fill)
            self.cancel_pending()
            return fill
        return None


@dataclass
class HedgeRecord:
    up_avg: float
    dn_avg: float
    up_shares: float
    dn_shares: float
    pair_cost: float
    placed_at: float
    filled_at: Optional[float] = None
    guaranteed_pnl: Optional[float] = None


@dataclass
class MarketSim:
    epoch: int
    winner: str
    up: SideState = field(default_factory=lambda: SideState("UP"))
    dn: SideState = field(default_factory=lambda: SideState("DOWN"))
    hedges: list[HedgeRecord] = field(default_factory=list)
    last_poll: float = 0.0
    log_lines: list[str] = field(default_factory=list)

    def log(self, msg: str, verbose: bool = True):
        self.log_lines.append(msg)
        if verbose:
            print(msg)

    def state(self, side: str) -> SideState:
        return self.up if side == "UP" else self.dn


# ─────────────────────────────────────────────
# SİMÜLASYON
# ─────────────────────────────────────────────

def run(ticks: list[Tick], epoch: int, winner: str, verbose: bool = True) -> dict:
    sim = MarketSim(epoch=epoch, winner=winner)
    sim.last_poll = ticks[0].ts

    def log(msg): sim.log(msg, verbose)

    log(f"\n{'═'*65}")
    log(f" EPOCH {epoch}  winner={winner}  SUM_TARGET={SUM_TARGET}  DCA_DROP={DROP_THRESHOLD}")
    log(f"{'═'*65}")
    log(f"{'t':>5s} | {'taraf':5s} | {'eylem':20s} | {'price':6s} | "
        f"{'up_avg':6s} | {'dn_avg':6s} | {'pair':5s} | {'note'}")
    log(f"{'-'*85}")

    def fmt(t_off, side, action, price="", note=""):
        up_avg = f"{sim.up.avg_price:.3f}" if sim.up.total_shares > 0 else "  -  "
        dn_avg = f"{sim.dn.avg_price:.3f}" if sim.dn.total_shares > 0 else "  -  "
        pair = f"{sim.up.avg_price+sim.dn.avg_price:.3f}" if (
            sim.up.total_shares > 0 and sim.dn.total_shares > 0) else "  -  "
        px = f"{price:.4f}" if isinstance(price, float) else str(price)
        log(f"{t_off:5d} | {side:5s} | {action:20s} | {px:6s} | "
            f"{up_avg:6s} | {dn_avg:6s} | {pair:5s} | {note}")

    for tk in ticks:
        t_off = int(tk.ts - epoch)

        # ── Fill kontrolü her tick ──────────────────────────────────
        for side in ["UP", "DOWN"]:
            st = sim.state(side)
            fill = st.try_fill(tk)
            if fill:
                fmt(t_off, side, "✓ FILL", fill.price,
                    f"toplam={st.total_shares:.0f}sh avg={st.avg_price:.4f} cost=${st.total_cost:.2f}")

        # ── HEDGE fırsatı: her fill sonrası kontrol ─────────────────
        up_s, dn_s = sim.up, sim.dn
        if up_s.total_shares > 0 and dn_s.total_shares > 0:
            pair_cost = up_s.avg_price + dn_s.avg_price
            up_unhedged = up_s.unhedged_shares
            dn_unhedged = dn_s.unhedged_shares
            hedgeable = min(up_unhedged, dn_unhedged)

            if pair_cost < SUM_TARGET and hedgeable > 0:
                pnl_per_share = 1.0 - pair_cost
                total_pnl = pnl_per_share * hedgeable
                rec = HedgeRecord(
                    up_avg=up_s.avg_price, dn_avg=dn_s.avg_price,
                    up_shares=hedgeable, dn_shares=hedgeable,
                    pair_cost=pair_cost, placed_at=tk.ts,
                    filled_at=tk.ts, guaranteed_pnl=total_pnl
                )
                sim.hedges.append(rec)
                up_s.hedged_shares += hedgeable
                dn_s.hedged_shares += hedgeable
                fmt(t_off, "HEDGE", f"★ ARB LOCKED", pair_cost,
                    f"pair={pair_cost:.4f} +${total_pnl:.2f} ({hedgeable:.0f}sh)")

        # ── POLL: her 2 saniyede emir ver / güncelle ────────────────
        if (tk.ts - sim.last_poll) < POLL_INTERVAL_S:
            continue
        sim.last_poll = tk.ts

        for side in ["UP", "DOWN"]:
            st = sim.state(side)
            current_bid = tk.bid(side)
            current_ask = tk.ask(side)

            # Guard: çok pahalı veya çok ucuz
            if current_bid <= FLOOR_PRICE:
                if st.has_pending():
                    st.cancel_pending()
                continue
            if current_bid >= 0.95:
                continue

            # Guard: maksimum harcama aşıldı mı?
            if st.total_cost >= MAX_USDC_PER_SIDE:
                continue

            # DCA tetik: bid en son işlem bid'inden DROP_THRESHOLD kadar düştü mü?
            # ya da hiç emir olmadı mı?
            entry_price = round(current_bid - TICK_SIZE, 4)  # 1 tick altı

            if st.has_pending():
                # Mevcut emir fiyatından daha iyi fiyat oluştuysa güncelle
                if entry_price < st.pending_price - TICK_SIZE:
                    st.cancel_pending()
                    st.place(entry_price, tk.ts)
                    fmt(t_off, side, "↺ REVISE", entry_price,
                        f"bid={current_bid:.3f} daha_iyi_fiyat")
                continue  # Zaten açık emir var

            # Yeni emir koşulu:
            # (a) Hiç fill yok (ilk giriş)
            # (b) Fiyat son avg'dan DROP_THRESHOLD kadar düştü (DCA)
            # (c) Unhedged pozisyon var ve fiyat daha da düştü
            should_order = False
            reason = ""

            if st.total_shares == 0 and not st.has_pending():
                should_order = True
                reason = "ilk_giriş"
            elif st.total_shares > 0 and entry_price < st.avg_price - DROP_THRESHOLD:
                should_order = True
                reason = f"DCA avg={st.avg_price:.3f}→{entry_price:.3f}"
            elif st.total_shares == 0 and not st.has_pending():
                should_order = True
                reason = "yeniden_giriş"

            if should_order:
                st.place(entry_price, tk.ts)
                fmt(t_off, side, "⊕ EMIR", entry_price,
                    f"bid={current_bid:.3f} {reason}")

    # ── Market kapanışı: settlement PnL ─────────────────────────
    log(f"\n{'-'*65}")
    log(f" SETTLEMENT  t=300s  winner={winner}")
    log(f"{'-'*65}")

    total_guaranteed = sum(h.guaranteed_pnl for h in sim.hedges)
    up_unhedged = sim.up.unhedged_shares
    dn_unhedged = sim.dn.unhedged_shares

    settle_pnl = 0.0
    if winner == "UP":
        settle_pnl += (1.0 - sim.up.avg_price) * up_unhedged if up_unhedged > 0 else 0
        settle_pnl += (0.0 - sim.dn.avg_price) * dn_unhedged if dn_unhedged > 0 else 0
    elif winner == "DOWN":
        settle_pnl += (1.0 - sim.dn.avg_price) * dn_unhedged if dn_unhedged > 0 else 0
        settle_pnl += (0.0 - sim.up.avg_price) * up_unhedged if up_unhedged > 0 else 0

    total_pnl = total_guaranteed + settle_pnl
    total_cost = sim.up.total_cost + sim.dn.total_cost

    log(f" UP  : {sim.up.total_shares:.0f}sh  avg={sim.up.avg_price:.4f}  cost=${sim.up.total_cost:.2f}")
    log(f" DOWN: {sim.dn.total_shares:.0f}sh  avg={sim.dn.avg_price:.4f}  cost=${sim.dn.total_cost:.2f}")
    log(f" Toplam harcama: ${total_cost:.2f}")
    log(f" Garantili arb PnL : ${total_guaranteed:+.2f}  ({len(sim.hedges)} hedge)")
    log(f" Settlement PnL    : ${settle_pnl:+.2f}  "
        f"(hedge_dışı UP={up_unhedged:.0f}sh DN={dn_unhedged:.0f}sh)")
    log(f" TOPLAM PnL        : ${total_pnl:+.2f}")

    return {
        "epoch": epoch,
        "winner": winner,
        "up_shares": sim.up.total_shares,
        "up_avg": sim.up.avg_price,
        "dn_shares": sim.dn.total_shares,
        "dn_avg": sim.dn.avg_price,
        "total_cost": total_cost,
        "arb_count": len(sim.hedges),
        "guaranteed_pnl": total_guaranteed,
        "settle_pnl": settle_pnl,
        "total_pnl": total_pnl,
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

    print("\n" + "="*65)
    print("DCA Arbitraj — Fiyat Düştükçe Ortalama Düşür")
    print(f"POLL={POLL_INTERVAL_S}s | SUM_TARGET={SUM_TARGET} | "
          f"SHARES={SHARES_PER_ORDER} | MAX=${MAX_USDC_PER_SIDE}/taraf")
    print(f"FLOOR={FLOOR_PRICE} | DCA_DROP={DROP_THRESHOLD}")
    print("="*65)

    results = []
    for epoch in EPOCHS:
        ticks = load_ticks(epoch)
        winner = infer_winner(ticks)
        r = run(ticks, epoch, winner, verbose=True)
        results.append(r)

    # ── Global Özet ──────────────────────────────────────────────
    print("\n" + "="*90)
    print("GLOBAL ÖZET")
    print("="*90)
    print(f"{'Epoch':12s}|{'W':3s}|{'UP_sh':6s}|{'UP_avg':6s}|{'DN_sh':6s}|{'DN_avg':6s}|"
          f"{'Maliyet':8s}|{'ARB':3s}|{'Garanti':8s}|{'Settle':8s}|{'TOPLAM':8s}")
    print("-"*90)
    total_cost_all = 0.0
    total_pnl_all = 0.0
    for r in results:
        print(
            f"{r['epoch']:12d}|{r['winner']:3s}|"
            f"{r['up_shares']:6.0f}|{r['up_avg']:6.3f}|"
            f"{r['dn_shares']:6.0f}|{r['dn_avg']:6.3f}|"
            f"${r['total_cost']:7.2f}|"
            f"{r['arb_count']:3d}|"
            f"${r['guaranteed_pnl']:+7.2f}|"
            f"${r['settle_pnl']:+7.2f}|"
            f"${r['total_pnl']:+7.2f}"
        )
        total_cost_all += r["total_cost"]
        total_pnl_all += r["total_pnl"]
    print("="*90)
    roi = 100 * total_pnl_all / total_cost_all if total_cost_all > 0 else 0
    print(f"  Toplam maliyet: ${total_cost_all:.2f}  |  Toplam PnL: ${total_pnl_all:+.2f}  |  ROI: {roi:+.2f}%")

    # ── Parametre Duyarlılığı ─────────────────────────────────────
    print("\n" + "="*65)
    print("DCA STRATEJİSİ NASIL ÇALIŞIR?")
    print("="*65)
    lines = [
        "Adım 1: t=0'dan itibaren her 2 saniyede bid-1tick'e emir ver",
        "Adım 2: Fiyat düşerse yeni emir (DCA) → avg maliyet düşer",
        "Adım 3: UP avg + DOWN avg < 0.97 olduğunda ARB kilitle",
        "         → Her iki taraf dolarsa $0.03/share garantili kâr",
        "Adım 4: Hedge edilmeyen share'ler settlement'a gider",
        "         → Doğru taraf $1, yanlış taraf $0 öder",
        "Adım 5: Net PnL = garantili_arb + settlement_sonucu",
        "",
        "Risk: Çok tek taraflı pozisyon birikirse settlement'ta büyük kayıp",
        "Çözüm: MAX_USDC_PER_SIDE limiti + DCA yalnızca avg altına düşünce",
    ]
    for l in lines:
        print(f"  {l}")


if __name__ == "__main__":
    main()
