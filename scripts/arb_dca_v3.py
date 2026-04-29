#!/usr/bin/env python3
"""
DCA + Kademeli Hedge Arbitraj Simülasyonu v3  [TAM TERS]
=========================================================
v2'nin tam tersi mantığı:

  DCA tarafı  (o an price > 0.5 olan, PAHALı/kazanan taraf):
    • Her 2 saniyede bir, o tarafın bid fiyatı DCA_MIN_DROP kadar düştüyse
      bid-1tick'e yeni GTC emir ver (kazanan tarafı ucuzladıkça topla).
    • Fiyat 0.5'in altına düşerse DCA durur.
    • MAX_USD_PER_SIDE limitine kadar devam eder.

  Hedge tarafı (o an price < 0.5 olan, UCUZ/kaybeden taraf):
    • DCA tarafı fill olduğunda karşı tarafa sıralı hedge emri:
        Adım 1: bid - 3×TICK  →  HEDGE_STEP_S saniye bekle
        Adım 2: bid - 2×TICK  →  HEDGE_STEP_S saniye bekle
        Adım 3: bid - 1×TICK  →  fill olmazsa vazgeç
    • Fill olursa pair_cost hesaplanır; < 1.00 ise garantili kâr kilitle.

  Strateji mantığı:
    Kazanan tarafa (0.90 gibi) alım yap → ortalama ~0.85.
    Kaybeden tarafa (0.10 gibi) hedge ver → ortalama ~0.12.
    pair_cost = 0.85 + 0.12 = 0.97 → her share için +$0.03 garantili kâr.
    Üstelik kazanan taraf zaten 1.00'a gideceği için hedge'siz share'ler de kâra geçer.

  Market başlangıcından (t=0) kapanışına (t≈300s) kesintisiz çalışır.
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
TICK             = 0.01    # Minimum fiyat adımı
POLL_S           = 2       # DCA kontrol aralığı (saniye)
DCA_MIN_DROP     = 0.01    # Avg'dan bu kadar düşmeden DCA yapma
SHARES           = 40.0    # Her emir kaç share
MAX_USD_PER_SIDE = 500.0   # Taraf başına maksimum USDC
HEDGE_STEP_S     = 6       # Hedge adım arası bekleme (saniye)
HEDGE_TICKS      = [3, 2, 1]  # bid offset sırası
CHEAP_THRESHOLD  = 0.50    # Bu değerin ÜSTÜNDEKI taraf "DCA tarafı" (v3: ters mantık)

# Fiyat bandı: her iki taraf için çalışma aralığı
# DCA tarafı (pahalı): BAND_LOW < price < BAND_HIGH  → örn. 0.50–0.90
# Hedge tarafı (ucuz): BAND_LOW < price < CHEAP_THRESHOLD → örn. 0.10–0.50
BAND_LOW  = 0.10   # Bu altında işlem yapma (çok ucuz, settlement/kayıp riski)
BAND_HIGH = 0.90   # Bu üstünde DCA yapma (çok pahalı, settlement yakın)

# ─────────────────────────────────────────────
# VERİ YAPILARI
# ─────────────────────────────────────────────

@dataclass
class BookTick:
    ts: float
    up_bid: float; up_ask: float
    dn_bid: float; dn_ask: float

    def bid(self, s: str) -> float: return self.up_bid if s == "UP" else self.dn_bid
    def ask(self, s: str) -> float: return self.up_ask if s == "UP" else self.dn_ask
    def mid(self, s: str) -> float: return (self.bid(s) + self.ask(s)) / 2
    def is_cheap(self, s: str) -> bool: return self.mid(s) < CHEAP_THRESHOLD
    def is_expensive(self, s: str) -> bool: return self.mid(s) > CHEAP_THRESHOLD


@dataclass
class SideBucket:
    side: str
    fills: list[tuple[float, float]] = field(default_factory=list)
    hedged_shares: float = 0.0
    pending_price: Optional[float] = None
    pending_ts: Optional[float] = None

    @property
    def total_shares(self) -> float:  return sum(s for _, s in self.fills)
    @property
    def total_cost(self) -> float:    return sum(p*s for p,s in self.fills)
    @property
    def avg(self) -> float:           return self.total_cost/self.total_shares if self.total_shares else 0.0
    @property
    def unhedged(self) -> float:      return self.total_shares - self.hedged_shares

    def has_pending(self) -> bool: return self.pending_price is not None

    def add_fill(self, px: float, sh: float):
        self.fills.append((px, sh))
        self.pending_price = None
        self.pending_ts = None

    def cancel_pending(self):
        self.pending_price = None
        self.pending_ts = None

    def check_fill(self, tk: BookTick) -> bool:
        if not self.has_pending(): return False
        ask = tk.ask(self.side)
        bid = tk.bid(self.side)
        return ask <= self.pending_price or bid >= self.pending_price


@dataclass
class HedgeJob:
    """Ana taraf fill → hedge tarafında kademeli emir denemesi."""
    main_fill_px: float
    main_fill_ts: float
    step: int = 0             # 0=bid-3, 1=bid-2, 2=bid-1
    step_ts: float = 0.0     # Mevcut adım başlangıcı
    order_px: Optional[float] = None
    done: bool = False

    def advance_step(self):
        self.step += 1
        self.order_px = None

    def exhausted(self) -> bool:
        return self.step >= len(HEDGE_TICKS)

    def check_fill(self, tk: BookTick, side: str) -> bool:
        if self.order_px is None or self.done: return False
        return tk.ask(side) <= self.order_px or tk.bid(side) >= self.order_px


def run(ticks: list[BookTick], epoch: int, winner: str, verbose: bool = True) -> dict:
    up = SideBucket("UP")
    dn = SideBucket("DOWN")
    last_poll = {s: ticks[0].ts for s in ["UP", "DOWN"]}
    hedge_jobs: list[HedgeJob] = []
    locked_arbs: list[dict] = []
    log_lines: list[str] = []

    def out(msg):
        log_lines.append(msg)
        if verbose: print(msg)

    def bucket(s): return up if s == "UP" else dn
    def opp(s): return "DOWN" if s == "UP" else "UP"

    def fmt_row(t_off, side, action, px="", note=""):
        ua = f"{up.avg:.3f}" if up.total_shares else "  .  "
        da = f"{dn.avg:.3f}" if dn.total_shares else "  .  "
        pair = f"{up.avg+dn.avg:.3f}" if (up.total_shares and dn.total_shares) else "  .  "
        out(f"{t_off:4d}|{side:5}|{action:26}|{str(px):6}|{ua:5}|{da:5}|{pair:5}|{note}")

    out(f"\n{'═'*78}")
    out(f" {epoch}  winner={winner}  POLL={POLL_S}s  HEDGE=[bid-{HEDGE_TICKS[0]}→bid-{HEDGE_TICKS[-1]}]  MAX=${MAX_USD_PER_SIDE}/taraf")
    out(f"{'─'*78}")
    out(f"{'t':>4}|{'taraf':5}|{'eylem':26}|{'px':6}|{'up_a':5}|{'dn_a':5}|{'pair':5}|not")
    out(f"{'─'*78}")

    for tk in ticks:
        t = int(tk.ts - epoch)
        for side in ["UP", "DOWN"]:
            bkt = bucket(side)

            # ── DCA Fill kontrolü ────────────────────────────────
            if bkt.check_fill(tk):
                px = bkt.pending_price
                bkt.add_fill(px, SHARES)
                fmt_row(t, side, "✓ DCA FILL", f"{px:.3f}",
                        f"avg={bkt.avg:.4f} {bkt.total_shares:.0f}sh ${bkt.total_cost:.1f}")
                # Hedge görevi başlat
                job = HedgeJob(main_fill_px=px, main_fill_ts=tk.ts, step_ts=tk.ts)
                hedge_jobs.append(job)

        # ── Hedge görevleri ──────────────────────────────────────
        for job in list(hedge_jobs):
            if job.done: continue

            # Hangi taraf fill oldu? → karşısını hedge et
            # Ama hangi fill bu job'a ait? Ana tarafı belirle
            # Basit: UP'un son fill'i mi yoksa DOWN'un mu?
            # → job.main_fill_ts ile karşılaştır
            # Cheap taraf = son fill hangi taraftan geldi?
            # Bunu job'a eklememiz gerekiyor — job'u güncelleyelim
            pass

        # ─── Basitleştirme: tüm hedge_jobs'ı global işle ────────
        # Her job: main_fill hangi taraftan? Belirsiz olduğu için
        # her iki tarafa da tek seferlik hedge job ata
        # Aşağıda yeniden tasarlandı

        # ── Hedge fill kontrolü ──────────────────────────────────
        # hedge_jobs listesindeki her iş için
        for job in list(hedge_jobs):
            if job.done: continue
            hedge_side = job._hedge_side if hasattr(job, "_hedge_side") else None
            if hedge_side is None: continue
            bkt_h = bucket(hedge_side)

            if job.check_fill(tk, hedge_side):
                px = job.order_px
                bkt_h.add_fill(px, SHARES)
                bkt_m = bucket(opp(hedge_side))
                pair_cost = bkt_m.avg + bkt_h.avg
                hedgeable = min(bkt_m.unhedged, bkt_h.unhedged)
                pnl = (1.0 - pair_cost) * hedgeable if (pair_cost < 1.0 and hedgeable > 0) else 0.0
                if pnl > 0:
                    bkt_m.hedged_shares += hedgeable
                    bkt_h.hedged_shares += hedgeable
                    locked_arbs.append({"pair_cost": pair_cost, "shares": hedgeable, "pnl": pnl, "t": t})
                    fmt_row(t, hedge_side, f"★ ARB LOCK adım{job.step}",
                            f"{px:.3f}", f"pair={pair_cost:.4f} +${pnl:.2f} {hedgeable:.0f}sh")
                else:
                    fmt_row(t, hedge_side, f"✓ HEDGE FILL adım{job.step}",
                            f"{px:.3f}", f"pair={pair_cost:.4f} (arb yok)")
                job.done = True
                continue

            # Adım geçişi
            needs_new_order = (job.order_px is None or
                               (tk.ts - job.step_ts) >= HEDGE_STEP_S)
            if needs_new_order:
                if job.exhausted():
                    fmt_row(t, hedge_side, "✗ HEDGE başarısız", "", "tüm adımlar denendi")
                    job.done = True
                    continue
                tick_offset = HEDGE_TICKS[job.step]
                h_bid = tk.bid(hedge_side)
                h_px = round(h_bid - tick_offset * TICK, 4)
                h_px = max(TICK, h_px)
                job.order_px = h_px
                job.step_ts = tk.ts
                label = f"bid-{tick_offset}t"
                fmt_row(t, hedge_side, f"⊞ HEDGE adım{job.step+1} ({label})",
                        f"{h_px:.3f}", f"h_bid={h_bid:.3f}")
                job.step += 1

        # ── POLL: DCA emirleri ───────────────────────────────────
        for side in ["UP", "DOWN"]:
            if (tk.ts - last_poll[side]) < POLL_S:
                continue
            last_poll[side] = tk.ts

            bkt = bucket(side)
            if not tk.is_cheap(side): continue      # Sadece ucuz tarafa DCA
            if bkt.total_cost >= MAX_USD_PER_SIDE:  continue
            if tk.mid(side) <= 0.03:                continue

            cur_bid = tk.bid(side)
            entry = round(cur_bid - TICK, 4)
            entry = max(TICK, entry)

            if bkt.has_pending():
                # Mevcut emirden çok daha iyi fiyat oluştuysa güncelle
                if entry < bkt.pending_price - TICK:
                    bkt.cancel_pending()
                else:
                    continue

            first = bkt.total_shares == 0
            dca_ok = bkt.total_shares > 0 and entry < bkt.avg - DCA_MIN_DROP

            if not first and not dca_ok: continue

            bkt.pending_price = entry
            bkt.pending_ts = tk.ts
            reason = "ilk" if first else f"DCA {bkt.avg:.3f}→{entry:.3f}"
            fmt_row(t, side, f"⊕ DCA EMIR", f"{entry:.3f}",
                    f"bid={cur_bid:.3f} {reason}")

    # ── Settlement ───────────────────────────────────────────────
    total_guaranteed = sum(a["pnl"] for a in locked_arbs)
    up_u = up.unhedged; dn_u = dn.unhedged
    if winner == "UP":
        settle = (1 - up.avg) * up_u - dn.avg * dn_u if up.total_shares and dn.total_shares else \
                 (1 - up.avg) * up_u if up.total_shares else 0.0
    elif winner == "DOWN":
        settle = (1 - dn.avg) * dn_u - up.avg * up_u if up.total_shares and dn.total_shares else \
                 (1 - dn.avg) * dn_u if dn.total_shares else 0.0
    else:
        settle = 0.0

    total_cost = up.total_cost + dn.total_cost
    total_pnl = total_guaranteed + settle

    out(f"\n{'-'*78}")
    out(f" SETTLEMENT  winner={winner}")
    out(f" UP : {up.total_shares:.0f}sh  avg={up.avg:.4f}  ${up.total_cost:.2f}  "
        f"(hedged={up.hedged_shares:.0f} unhedged={up_u:.0f})")
    out(f" DN : {dn.total_shares:.0f}sh  avg={dn.avg:.4f}  ${dn.total_cost:.2f}  "
        f"(hedged={dn.hedged_shares:.0f} unhedged={dn_u:.0f})")
    out(f" Toplam maliyet: ${total_cost:.2f}  |  ARB sayısı: {len(locked_arbs)}")
    out(f" Garantili ARB : ${total_guaranteed:+.2f}")
    out(f" Settlement    : ${settle:+.2f}  (UP={up_u:.0f}sh DN={dn_u:.0f}sh)")
    out(f" TOPLAM PnL    : ${total_pnl:+.2f}")

    return {
        "epoch": epoch, "winner": winner,
        "up_sh": up.total_shares, "up_avg": up.avg, "up_cost": up.total_cost,
        "dn_sh": dn.total_shares, "dn_avg": dn.avg, "dn_cost": dn.total_cost,
        "total_cost": total_cost, "arb_count": len(locked_arbs),
        "guaranteed": total_guaranteed, "settle": settle, "total_pnl": total_pnl,
    }


# ──────────────────────────────────────────────────────────
# DÜZELTME: HedgeJob'a main_side ekle
# ──────────────────────────────────────────────────────────
# Yukarıdaki kod çalıştırmak için küçük bir yeniden tasarım:
# hedge_jobs'ı DCA fill anında doğrudan main_side ile oluştur.

def run_fixed(ticks: list[BookTick], epoch: int, winner: str, verbose: bool = True) -> dict:
    """Düzeltilmiş tam simülasyon — ana/hedge taraf fill'de belirlenir."""

    up = SideBucket("UP")
    dn = SideBucket("DOWN")
    last_poll: dict[str, float] = {"UP": ticks[0].ts, "DOWN": ticks[0].ts}
    log_lines: list[str] = []
    locked_arbs: list[dict] = []

    # Aktif hedge görevleri: her birinin karşı tarafı (hedge_side) biliniyor
    @dataclass
    class Job:
        main_side: str
        hedge_side: str
        main_fill_px: float
        ts: float
        step: int = 0
        step_ts: float = 0.0
        order_px: Optional[float] = None
        done: bool = False

        def exhausted(self): return self.step >= len(HEDGE_TICKS)
        def check_fill(self, tk: BookTick) -> bool:
            if self.order_px is None or self.done: return False
            return tk.ask(self.hedge_side) <= self.order_px or tk.bid(self.hedge_side) >= self.order_px

    jobs: list[Job] = []

    def out(msg):
        log_lines.append(msg)
        if verbose: print(msg)

    def bkt(s): return up if s == "UP" else dn

    def fmt(t_off, side, action, px="", note=""):
        ua = f"{up.avg:.3f}" if up.total_shares else "   . "
        da = f"{dn.avg:.3f}" if dn.total_shares else "   . "
        pair = f"{up.avg+dn.avg:.3f}" if (up.total_shares and dn.total_shares) else "   . "
        out(f"{t_off:4d}|{side:5}|{action:28}|{str(px):6}|{ua:5}|{da:5}|{pair:5}|{note}")

    out(f"\n{'═'*80}")
    out(f" {epoch}  winner={winner}  POLL={POLL_S}s  "
        f"HEDGE=[bid-{HEDGE_TICKS[0]}→bid-{HEDGE_TICKS[-1]}tick, {HEDGE_STEP_S}s]  "
        f"BAND=[{BAND_LOW}–{BAND_HIGH}]  [v3: pahalı DCA]")
    out(f"{'─'*80}")
    out(f"{'t':>4}|{'taraf':5}|{'eylem':28}|{'px':6}|{'up_a':5}|{'dn_a':5}|{'pair':5}|not")
    out(f"{'─'*80}")

    for tk in ticks:
        t = int(tk.ts - epoch)

        # ── Her iki taraf: DCA fill kontrolü ────────────────────
        for side in ["UP", "DOWN"]:
            b = bkt(side)
            if b.check_fill(tk):
                px = b.pending_price
                b.add_fill(px, SHARES)
                fmt(t, side, "✓ DCA FILL", f"{px:.3f}",
                    f"avg={b.avg:.4f} {b.total_shares:.0f}sh ${b.total_cost:.1f}")
                # Hedge görevi: karşı tarafa (ucuz tarafa) sıralı emir
                hedge_s = "DOWN" if side == "UP" else "UP"
                hedge_mid = tk.mid(hedge_s)
                # Sadece ucuz taraf BAND_LOW–CHEAP_THRESHOLD aralığındaysa hedge
                if hedge_mid < CHEAP_THRESHOLD and hedge_mid >= BAND_LOW:
                    j = Job(main_side=side, hedge_side=hedge_s,
                            main_fill_px=px, ts=tk.ts, step_ts=tk.ts)
                    jobs.append(j)

        # ── Hedge görevleri: fill + adım geçişi ─────────────────
        for job in list(jobs):
            if job.done: continue
            hs = job.hedge_side
            b_h = bkt(hs)
            b_m = bkt(job.main_side)

            # Fill kontrolü
            if job.check_fill(tk):
                px = job.order_px
                b_h.add_fill(px, SHARES)
                pair_cost = b_m.avg + b_h.avg
                hedgeable = min(b_m.unhedged, b_h.unhedged)
                pnl = (1.0 - pair_cost) * hedgeable if (pair_cost < 1.0 and hedgeable > 0) else 0.0
                if pnl > 0:
                    b_m.hedged_shares += hedgeable
                    b_h.hedged_shares += hedgeable
                    locked_arbs.append({"pair_cost": pair_cost, "shares": hedgeable, "pnl": pnl, "t": t})
                    fmt(t, hs, f"★ ARB KİLİT (adım {job.step})",
                        f"{px:.3f}", f"pair={pair_cost:.4f} +${pnl:.2f} {hedgeable:.0f}sh")
                else:
                    fmt(t, hs, f"✓ HEDGE fill (adım {job.step})",
                        f"{px:.3f}", f"pair={pair_cost:.4f} kârsız")
                job.done = True
                continue

            # Adım geçişi
            if job.order_px is None or (tk.ts - job.step_ts) >= HEDGE_STEP_S:
                if job.exhausted():
                    fmt(t, hs, "✗ HEDGE başarısız", "", f"tüm adımlar tükendi")
                    job.done = True
                    continue
                # Hedge tarafı bant dışına çıktıysa iptal et
                h_mid = tk.mid(hs)
                if h_mid < BAND_LOW or h_mid >= CHEAP_THRESHOLD:
                    fmt(t, hs, "✗ HEDGE iptal (bant dışı)", "",
                        f"mid={h_mid:.3f} bant=[{BAND_LOW},{CHEAP_THRESHOLD})")
                    job.done = True
                    continue
                tick_offset = HEDGE_TICKS[job.step]
                h_bid = tk.bid(hs)
                h_px = round(h_bid - tick_offset * TICK, 4)
                h_px = max(TICK, min(h_px, 0.99))
                job.order_px = h_px
                job.step_ts = tk.ts
                fmt(t, hs, f"⊞ HEDGE adım{job.step+1} (bid-{tick_offset}t)",
                    f"{h_px:.3f}", f"h_bid={h_bid:.3f}")
                job.step += 1

        # ── POLL: DCA emirleri her 2 saniyede (v3: pahalı tarafa, >0.5) ───────
        for side in ["UP", "DOWN"]:
            if (tk.ts - last_poll[side]) < POLL_S: continue
            last_poll[side] = tk.ts

            b = bkt(side)
            if not tk.is_expensive(side): continue           # Yalnızca PAHALI tarafa DCA (v3)
            if b.total_cost >= MAX_USD_PER_SIDE: continue
            mid = tk.mid(side)
            if mid > BAND_HIGH: continue                     # Üst sınır aşıldı
            if mid < CHEAP_THRESHOLD: continue               # Alt sınır (0.5 altı = ucuz tarafa geçmiş)

            cur_bid = tk.bid(side)
            entry = round(cur_bid - TICK, 4)
            entry = max(TICK, entry)

            if b.has_pending():
                # Mevcut emirden daha iyi fiyat (daha düşük) oluştuysa güncelle
                if entry < b.pending_price - TICK:
                    b.cancel_pending()
                else:
                    continue

            first = b.total_shares == 0
            # v3: pahalı tarafta DCA → fiyat DÜŞTÜKÇE alım yap (avg düşür)
            dca_ok = b.total_shares > 0 and entry < b.avg - DCA_MIN_DROP

            if not first and not dca_ok: continue

            b.pending_price = entry
            b.pending_ts = tk.ts
            reason = "ilk_giriş" if first else f"DCA {b.avg:.3f}→{entry:.3f}"
            fmt(t, side, f"⊕ DCA EMIR", f"{entry:.3f}",
                f"bid={cur_bid:.3f} {reason}")

    # ── Settlement ───────────────────────────────────────────────
    total_guaranteed = sum(a["pnl"] for a in locked_arbs)
    up_u = up.unhedged; dn_u = dn.unhedged
    if winner == "UP":
        settle = (1-up.avg)*up_u - up.avg*(0)*0     # kazanan UP
        settle = (1-up.avg)*up_u + (0-dn.avg)*dn_u
    elif winner == "DOWN":
        settle = (1-dn.avg)*dn_u + (0-up.avg)*up_u
    else:
        settle = 0.0

    total_cost = up.total_cost + dn.total_cost
    total_pnl = total_guaranteed + settle

    out(f"\n{'-'*80}")
    out(f" SETTLEMENT  winner={winner}")
    out(f" UP : {up.total_shares:.0f}sh avg={up.avg:.4f} ${up.total_cost:.2f} "
        f"[hedge={up.hedged_shares:.0f} açık={up_u:.0f}]")
    out(f" DN : {dn.total_shares:.0f}sh avg={dn.avg:.4f} ${dn.total_cost:.2f} "
        f"[hedge={dn.hedged_shares:.0f} açık={dn_u:.0f}]")
    out(f" Toplam maliyet : ${total_cost:.2f}")
    out(f" Garantili ARB  : ${total_guaranteed:+.2f}  ({len(locked_arbs)} işlem)")
    out(f" Settlement     : ${settle:+.2f}  (UP_açık={up_u:.0f}sh  DN_açık={dn_u:.0f}sh)")
    out(f" NET PnL        : ${total_pnl:+.2f}")

    return {
        "epoch": epoch, "winner": winner,
        "up_sh": up.total_shares, "up_avg": up.avg, "up_cost": up.total_cost,
        "dn_sh": dn.total_shares, "dn_avg": dn.avg, "dn_cost": dn.total_cost,
        "total_cost": total_cost, "arb_count": len(locked_arbs),
        "guaranteed": total_guaranteed, "settle": settle, "total_pnl": total_pnl,
    }


# ─────────────────────────────────────────────
# VERİ YÜKLEME
# ─────────────────────────────────────────────

def load_ticks(epoch: int) -> list[BookTick]:
    p = Path(f"exports/bot14-ticks-20260429/btc-updown-5m-{epoch}_ticks.json")
    return [BookTick(ts=r["ts_ms"]/1000., up_bid=float(r["up_best_bid"]),
                     up_ask=float(r["up_best_ask"]), dn_bid=float(r["down_best_bid"]),
                     dn_ask=float(r["down_best_ask"])) for r in json.loads(p.read_text())]


def discover_epochs(folder: str = "exports/bot14-ticks-20260429") -> list[int]:
    """Klasördeki tüm _ticks.json dosyalarından epoch listesi üretir."""
    import re
    epochs = []
    for p in sorted(Path(folder).glob("*_ticks.json")):
        m = re.search(r"btc-updown-5m-(\d+)_ticks", p.name)
        if m:
            epochs.append(int(m.group(1)))
    return epochs


def infer_winner(ticks: list[BookTick]) -> str:
    for tk in reversed(ticks[-30:]):
        if tk.up_ask >= 0.80 and tk.dn_ask <= 0.25: return "UP"
        if tk.dn_ask >= 0.80 and tk.up_ask <= 0.25: return "DOWN"
    return "?"


# ─────────────────────────────────────────────
# ANA
# ─────────────────────────────────────────────

def main():
    EPOCHS = discover_epochs()
    print("="*80)
    print("DCA v3 — TAM TERS: Pahalı taraf DCA, Ucuz tarafa kademeli hedge")
    print(f"POLL={POLL_S}s | DCA_MIN_DROP={DCA_MIN_DROP} | HEDGE adım süresi={HEDGE_STEP_S}s")
    print(f"SHARES={SHARES} | MAX_USD/taraf=${MAX_USD_PER_SIDE}")
    print(f"BAND: DCA [{CHEAP_THRESHOLD}–{BAND_HIGH}]  |  HEDGE [{BAND_LOW}–{CHEAP_THRESHOLD}]")
    print(f"Market sayısı: {len(EPOCHS)}")
    print("="*80)

    results = []
    for epoch in EPOCHS:
        tks = load_ticks(epoch)
        wnr = infer_winner(tks)
        results.append(run_fixed(tks, epoch, wnr, verbose=False))   # verbose=False → sadece özet

    print("\n" + "="*100)
    print("GLOBAL ÖZET  (16 Market)")
    print("="*100)
    print(f"{'Epoch':12}|{'W':4}|{'UP_sh':6}|{'UP_avg':7}|{'DN_sh':6}|{'DN_avg':7}|"
          f"{'Maliyet':8}|{'ARB':4}|{'Garanti':9}|{'Settle':9}|{'TOPLAM':9}|{'ROI':6}")
    print("-"*100)
    tot_c = tot_p = 0.0
    wins = losses = 0
    for r in results:
        up_w = "✓" if r["winner"] == "UP" else " "
        dn_w = "✓" if r["winner"] == "DOWN" else " "
        roi_m = 100*r["total_pnl"]/r["total_cost"] if r["total_cost"] else 0
        sign = "+" if r["total_pnl"] >= 0 else ""
        if r["total_pnl"] >= 0: wins += 1
        else: losses += 1
        print(f"{r['epoch']:12}|{r['winner']:4}|"
              f"{r['up_sh']:5.0f}{up_w}|{r['up_avg']:7.3f}|"
              f"{r['dn_sh']:5.0f}{dn_w}|{r['dn_avg']:7.3f}|"
              f"${r['total_cost']:7.2f}|{r['arb_count']:4}|"
              f"${r['guaranteed']:+8.2f}|${r['settle']:+8.2f}|"
              f"${r['total_pnl']:+8.2f}|{roi_m:+5.1f}%")
        tot_c += r["total_cost"]; tot_p += r["total_pnl"]
    print("="*100)
    roi = 100*tot_p/tot_c if tot_c else 0
    print(f"  Toplam yatırım : ${tot_c:.2f}")
    print(f"  Net PnL        : ${tot_p:+.2f}")
    print(f"  Genel ROI      : {roi:+.2f}%")
    print(f"  Kârlı / Zararlı: {wins}W / {losses}L")

    print("\n" + "="*80)
    print("STRATEJİ ÖZET (v3: TAM TERS)")
    print("="*80)
    print(f"  DCA tarafı  : {CHEAP_THRESHOLD} < price < {BAND_HIGH}  → her {POLL_S}s'de bid-1tick emir, avg düşünce DCA")
    print(f"  Hedge tarafı: {BAND_LOW} < price < {CHEAP_THRESHOLD} → bid-3t → {HEDGE_STEP_S}s → bid-2t → {HEDGE_STEP_S}s → bid-1t")
    print(f"  Bant dışı   : price < {BAND_LOW} veya price > {BAND_HIGH} → işlem yok (settlement yakın)")
    print(f"  ARB kilidi  : pair_cost < 1.00 → garantili kâr; unhedged kazanan share 1.00'a gider")
    print(f"  Risk        : Hedge edilemeyen pahalı-taraf share → taraf kaybederse değer sıfır")


if __name__ == "__main__":
    main()
