#!/usr/bin/env python3
"""
4-Hipotez Birebir Uyum Simülasyonu
==================================
6 markette gerçek bot vs 4 hipotezin trade davranışı karşılaştırması.

H0: Mevcut bot 383 (basamaklı spread + Δbid yön + tek emir, 40/25/8)
H1: Lineer spread sizing (40 → linear → 8)
H2: Çift emir (spread >= 0.40 winner + loser ikili)
H3: Yön seçim winner-bias (spread > 0.30 her zaman winner)
H4: Küçük lot (20/15/8) + Winner-bias + Çift emir (en kapsamlı)

Veri: data/bonereaper10/btc-updown-5m-{ts}.json + /tmp/bot383_all.tsv (DB)
"""
import json
import math
import os
from collections import defaultdict
from typing import Dict, List, Tuple, Optional

WALLET = "0xeebde7a0e019a63e6b476eb425505b7b3e6eba30"
DATA_DIR = "data/bonereaper10"

MARKETS = [
    1778948400, 1778948700, 1778949000,
    1778949300, 1778951400, 1778952000,
]

# Market kazananları (kullanıcı verdi: 1778952000 → UP; diğerleri PnL hesabında
# her iki senaryo için de hesaplanır, asıl uyum metriği bağımsız)
KNOWN_WINNERS = {1778952000: "Up"}


# ─── Veri yükleme ────────────────────────────────────────────────────
def load_real_trades(market_ts: int) -> List[dict]:
    fname = f"{DATA_DIR}/btc-updown-5m-{market_ts}.json"
    if not os.path.exists(fname):
        return []
    with open(fname) as f:
        data = json.load(f)
    raw = []
    seen = set()
    for src in ['trades', 'activity']:
        for t in data.get(src, []):
            if not isinstance(t, dict):
                continue
            if t.get('proxyWallet', '').lower() != WALLET:
                continue
            if t.get('side', '').upper() != 'BUY':
                continue
            k = (t.get('transactionHash'), t.get('asset'),
                 t.get('size'), t.get('price'), t.get('timestamp'))
            if k in seen:
                continue
            seen.add(k)
            oi = t.get('outcomeIndex')
            oc = t.get('outcome', '?')
            if oc == '?' and oi is not None:
                oc = 'Down' if oi == 1 else 'Up'
            raw.append({
                'tx': t.get('transactionHash', ''),
                'asset': t.get('asset', ''),
                'oc': oc,
                'price': float(t.get('price', 0) or 0),
                'size': float(t.get('size', 0) or 0),
                'usdc': float(t.get('size', 0) or 0) * float(t.get('price', 0) or 0),
                'ts': int(t.get('timestamp', 0) or 0),
            })
    raw.sort(key=lambda x: x['ts'])
    return raw


def load_bot_trades(market_ts: int) -> List[dict]:
    """VPS DB'den çekilmiş bot 383 trade'lerini market filtrele."""
    if not os.path.exists('/tmp/bot383_all.tsv'):
        return []
    bot = []
    with open('/tmp/bot383_all.tsv') as f:
        for line in f:
            line = line.strip()
            if not line or line.startswith('Error'):
                continue
            parts = line.split('|')
            if len(parts) < 7:
                continue
            try:
                bot.append({
                    'bot_id': int(parts[0]),
                    'market': parts[1],
                    'oc': 'Up' if parts[2].upper() == 'UP' else 'Down',
                    'price': float(parts[3]),
                    'size': float(parts[4]),
                    'usdc': float(parts[5]),
                    'ts_ms': int(parts[6]),
                })
            except (ValueError, IndexError):
                pass
    # Pencere filtresi: market_ts ≤ ts_s < market_ts + 300
    lo_ms = market_ts * 1000
    hi_ms = (market_ts + 300) * 1000
    return [t for t in bot if lo_ms <= t['ts_ms'] < hi_ms]


# ─── Tick timeline (saniye-bazlı bid snapshot) ──────────────────────
def build_tick_timeline(real_trades: List[dict], market_ts: int) -> List[dict]:
    """Real trade'lerden saniye-bazlı (up_bid, dn_bid) timeline oluştur."""
    if not real_trades:
        return []
    by_ts = defaultdict(lambda: {'Up': [], 'Down': []})
    for t in real_trades:
        by_ts[t['ts']][t['oc']].append(t['price'])
    timeline = []
    last_up = 0.0
    last_dn = 0.0
    end_ts = market_ts + 300
    for sec in range(market_ts, end_ts + 1):
        if sec in by_ts:
            ups = by_ts[sec]['Up']
            dns = by_ts[sec]['Down']
            if ups:
                last_up = max(ups)
            if dns:
                last_dn = max(dns)
        if last_up > 0 and last_dn > 0:
            timeline.append({
                'ts': sec,
                'up_bid': last_up,
                'dn_bid': last_dn,
                't_end': end_ts - sec,
                'up_ask': min(last_up + 0.005, 0.999),
                'dn_ask': min(last_dn + 0.005, 0.999),
            })
    return timeline


def determine_winner(real_trades: List[dict]) -> Tuple[str, str, float]:
    """Avg fiyat üzerinden winner/loser ve spread."""
    om = defaultdict(list)
    for o in real_trades:
        om[o['oc']].append(o)
    if len(om) < 2:
        return ('Up', 'Down', 0.0)
    avgs = {}
    for oc, lst in om.items():
        total_sh = sum(o['size'] for o in lst)
        if total_sh > 0:
            avgs[oc] = sum(o['price'] * o['size'] for o in lst) / total_sh
    winner = max(avgs, key=avgs.get)
    loser = min(avgs, key=avgs.get)
    return (winner, loser, avgs[winner] - avgs[loser])


# ─── Hipotez simülatörü ──────────────────────────────────────────────
def simulate_hypothesis(
    timeline: List[dict],
    cfg: dict,
    hypothesis: str,
    market_ts: int,
) -> List[dict]:
    """
    Bot karar döngüsünü simüle et. Her saniye:
      1. LW kontrol (bid >= lw_thr, t_end <= lw_secs, quota+cd OK)
      2. Cooldown kontrol
      3. Yön seçim (Δbid veya winner-bias)
      4. Sizing (basamaklı / lineer / çift emir)
    """
    end_ts = market_ts + 300
    trades = []
    last_buy = -9999
    last_lw = -9999
    lw_count = 0
    up_sh = dn_sh = up_paid = dn_paid = 0.0
    prev_up = 0.0
    prev_dn = 0.0
    first_done = False

    # Yön seçim için imbalance threshold (sadece info)
    BSI_THR = 0.30

    for tick in timeline:
        ts = tick['ts']
        t_end = tick['t_end']
        up_bid = tick['up_bid']
        dn_bid = tick['dn_bid']
        up_ask = tick['up_ask']
        dn_ask = tick['dn_ask']
        ob_spread = abs(up_bid - dn_bid)

        if t_end < 0:
            continue

        # ── LATE WINNER ─────────────────────────────────────────────
        lw_quota = (cfg['lw_max'] == 0 or lw_count < cfg['lw_max'])
        lw_cd_ok = (ts - last_lw) >= cfg['lw_cd']
        lw_active = (cfg['lw_usdc'] > 0 and t_end <= cfg['lw_secs'])
        if lw_quota and lw_active and lw_cd_ok and t_end > 0:
            if up_bid >= dn_bid:
                w_oc, w_bid, w_ask = 'Up', up_bid, up_ask
            else:
                w_oc, w_bid, w_ask = 'Down', dn_bid, dn_ask
            if w_bid >= cfg['lw_thr'] and w_ask > 0:
                lo = cfg['lw_thr']
                arb = max(5.0, min(10.0, 5 + 5 * (w_ask - lo) / (0.99 - lo)))
                sz = math.ceil(cfg['lw_usdc'] * arb / w_ask)
                trades.append({
                    'ts': ts, 'oc': w_oc, 'price': w_ask,
                    'size': sz, 'usdc': sz * w_ask,
                    'type': 'LW', 't_end': t_end,
                })
                last_buy = ts
                last_lw = ts
                lw_count += 1
                first_done = True
                if w_oc == 'Up':
                    up_sh += sz
                    up_paid += sz * w_ask
                else:
                    dn_sh += sz
                    dn_paid += sz * w_ask
                continue

        # ── COOLDOWN ────────────────────────────────────────────────
        if (ts - last_buy) < cfg['buy_cd']:
            prev_up = up_bid
            prev_dn = dn_bid
            continue

        # ── YÖN SEÇİMİ ──────────────────────────────────────────────
        # H3 / H4 — Winner-Bias: spread büyükse winner
        if hypothesis in ('H3', 'H4') and ob_spread > 0.30:
            dir_oc = 'Up' if up_bid >= dn_bid else 'Down'
        elif not first_done:
            # İlk emir spread gate + yön
            spread_diff = up_bid - dn_bid
            if abs(spread_diff) < cfg['first_spread_min']:
                prev_up = up_bid
                prev_dn = dn_bid
                continue
            dir_oc = 'Up' if spread_diff > 0 else 'Down'
        else:
            d_up = abs(up_bid - prev_up) if prev_up > 0 else 0
            d_dn = abs(dn_bid - prev_dn) if prev_dn > 0 else 0
            if d_up == 0 and d_dn == 0:
                dir_oc = 'Up' if up_bid >= dn_bid else 'Down'
            elif d_up >= d_dn:
                dir_oc = 'Up'
            else:
                dir_oc = 'Down'

        bid = up_bid if dir_oc == 'Up' else dn_bid
        ask = up_ask if dir_oc == 'Up' else dn_ask
        if bid <= 0 or ask <= 0 or bid > 0.999:
            prev_up = up_bid
            prev_dn = dn_bid
            continue

        # min_price filtresi (loser_min_price = 0.01)
        loser_oc_now = 'Up' if up_bid < dn_bid else 'Down'  # düşük bidli
        is_loser = (dir_oc == loser_oc_now and ob_spread >= 0.20)
        eff_min = cfg['loser_min_price'] if is_loser else 0.05
        if bid < eff_min:
            prev_up = up_bid
            prev_dn = dn_bid
            continue

        # avg_loser_max guard
        if is_loser:
            cur_avg = (up_paid / up_sh) if (dir_oc == 'Up' and up_sh > 0) else \
                      (dn_paid / dn_sh) if (dir_oc == 'Down' and dn_sh > 0) else 0
            scalp_only = cur_avg > cfg['avg_loser_max']
        else:
            scalp_only = False

        # ── SIZING ──────────────────────────────────────────────────
        if scalp_only and cfg['scalp'] > 0:
            sz = math.ceil(cfg['scalp'] / ask)
            usdc = cfg['scalp']
        else:
            # H1 — Lineer spread sizing
            if hypothesis == 'H1':
                lo_thr = cfg['sp_lo']
                hi_thr = cfg['sp_hi']
                sh_lo = cfg['sh_lo']
                sh_const = cfg['sh_const']
                if ob_spread <= lo_thr:
                    shares = sh_lo
                elif ob_spread >= hi_thr:
                    shares = sh_const
                else:
                    # Lineer interp
                    t = (ob_spread - lo_thr) / (hi_thr - lo_thr)
                    shares = sh_lo + (sh_const - sh_lo) * t
                shares = max(1.0, round(shares))
            elif hypothesis == 'H4':
                # H4: Küçük lot — gerçek bot her bantta 8-15sh kullanıyor
                if ob_spread < cfg['sp_lo']:
                    shares = 20.0  # H0'da 40sh, biz 20sh
                elif ob_spread < cfg['sp_hi']:
                    shares = 15.0  # H0'da 25sh, biz 15sh
                else:
                    shares = cfg['sh_const']  # 8sh
            else:
                # H0/H2/H3 → basamaklı
                if ob_spread < cfg['sp_lo']:
                    shares = cfg['sh_lo']
                elif ob_spread < cfg['sp_hi']:
                    shares = cfg['sh_mid']
                else:
                    shares = cfg['sh_const']
            sz = shares
            usdc = sz * ask

        if usdc < 0.3:
            prev_up = up_bid
            prev_dn = dn_bid
            continue

        # Ana emir
        trades.append({
            'ts': ts, 'oc': dir_oc, 'price': ask,
            'size': sz, 'usdc': sz * ask,
            'type': 'normal', 't_end': t_end,
        })
        last_buy = ts
        first_done = True
        if dir_oc == 'Up':
            up_sh += sz
            up_paid += sz * ask
        else:
            dn_sh += sz
            dn_paid += sz * ask

        # H2 / H4 — Çift emir: spread >= force_thr iken loser yönüne ek lot
        if hypothesis in ('H2', 'H4') and ob_spread >= cfg.get('force_both_thr', 0.40):
            loser_dir = 'Down' if dir_oc == 'Up' else 'Up'
            loser_bid = dn_bid if loser_dir == 'Down' else up_bid
            loser_ask = dn_ask if loser_dir == 'Down' else up_ask
            if loser_bid > 0.05:  # min eşiği
                # Loser yönüne sabit küçük lot
                l_sh = cfg.get('force_both_loser_sh', 8.0)
                l_usdc = l_sh * loser_ask
                if l_usdc >= 0.3:
                    trades.append({
                        'ts': ts, 'oc': loser_dir, 'price': loser_ask,
                        'size': l_sh, 'usdc': l_usdc,
                        'type': 'loser_pair', 't_end': t_end,
                    })
                    if loser_dir == 'Up':
                        up_sh += l_sh
                        up_paid += l_sh * loser_ask
                    else:
                        dn_sh += l_sh
                        dn_paid += l_sh * loser_ask

        prev_up = up_bid
        prev_dn = dn_bid

    return trades


# ─── Kıyaslama metrikleri ─────────────────────────────────────────────
def band_summary(trades: List[dict]) -> Dict[Tuple[float, float], dict]:
    """Her 0.10'luk bantta n, P50_sh, P50_usdc."""
    bands = [(0.01, 0.10), (0.10, 0.20), (0.20, 0.30),
             (0.30, 0.40), (0.40, 0.50), (0.50, 0.60),
             (0.60, 0.70), (0.70, 0.80), (0.80, 0.99)]
    out = {}
    for lo, hi in bands:
        sub = [o for o in trades if lo <= o['price'] < hi]
        if not sub:
            out[(lo, hi)] = {'n': 0, 'p50_sh': 0, 'p50_usdc': 0,
                              'total_usdc': 0, 'up_sh': 0, 'dn_sh': 0,
                              'up_usdc': 0, 'dn_usdc': 0}
            continue
        n = len(sub)
        sh_sorted = sorted(o['size'] for o in sub)
        u_sorted = sorted(o['usdc'] for o in sub)
        out[(lo, hi)] = {
            'n': n,
            'p50_sh': sh_sorted[n // 2],
            'p50_usdc': u_sorted[n // 2],
            'total_usdc': sum(o['usdc'] for o in sub),
            'up_sh': sum(o['size'] for o in sub if o['oc'] == 'Up'),
            'dn_sh': sum(o['size'] for o in sub if o['oc'] == 'Down'),
            'up_usdc': sum(o['usdc'] for o in sub if o['oc'] == 'Up'),
            'dn_usdc': sum(o['usdc'] for o in sub if o['oc'] == 'Down'),
        }
    return out


def calc_pnl(trades: List[dict], winner: str) -> dict:
    loser = 'Down' if winner == 'Up' else 'Up'
    total = sum(o['usdc'] for o in trades)
    w_sh = sum(o['size'] for o in trades if o['oc'] == winner)
    revenue = w_sh * 1.0
    fee = revenue * 0.02
    return {
        'total': total,
        'revenue': revenue,
        'fee': fee,
        'net': revenue - fee - total,
        'roi_pct': (revenue - fee - total) / total * 100 if total > 0 else 0,
    }


def band_distance(real_band: dict, sim_band: dict) -> float:
    """İki band özeti arasındaki USDC P50 farkının karekök ortalaması."""
    bands = list(real_band.keys())
    diffs = []
    for b in bands:
        r = real_band[b]['p50_usdc']
        s = sim_band[b]['p50_usdc']
        if r == 0 and s == 0:
            continue
        diffs.append((s - r) ** 2)
    if not diffs:
        return 0.0
    return math.sqrt(sum(diffs) / len(diffs))


# ─── Main ────────────────────────────────────────────────────────────
def main():
    # Konfig — bot 383'ün şu anki ayarlarına göre
    base_cfg = {
        'buy_cd': 4.0,
        'sh_const': 8.0,
        'sp_lo': 0.15, 'sp_hi': 0.40,
        'sh_lo': 40.0, 'sh_mid': 25.0,
        'lw_thr': 0.85, 'lw_secs': 180, 'lw_max': 5,
        'lw_cd': 8.0, 'lw_usdc': 2.3,
        'first_spread_min': 0.02,
        'loser_min_price': 0.01,
        'avg_loser_max': 0.60,
        'scalp': 1.0,
        # H2 ek params
        'force_both_thr': 0.40,
        'force_both_loser_sh': 8.0,
    }

    HYPOTHESES = ['H0', 'H1', 'H2', 'H3', 'H4']

    # Sonuç toplayıcı
    summary = {h: {'total_usdc': 0, 'total_n': 0, 'pnl_total': 0, 'band_err_total': 0}
                for h in HYPOTHESES + ['REAL']}

    print("=" * 110)
    print("4-HİPOTEZ SİMÜLASYONU — 6 Market Birebir Uyum Analizi")
    print("=" * 110)

    market_results = []

    for mkt_ts in MARKETS:
        real = load_real_trades(mkt_ts)
        bot_real = load_bot_trades(mkt_ts)
        if not real:
            continue

        timeline = build_tick_timeline(real, mkt_ts)
        winner, loser, _ = determine_winner(real)
        # Override known winner
        if mkt_ts in KNOWN_WINNERS:
            winner = KNOWN_WINNERS[mkt_ts]
            loser = 'Down' if winner == 'Up' else 'Up'

        real_band = band_summary(real)
        real_pnl = calc_pnl(real, winner)
        real_total = sum(o['usdc'] for o in real)

        print(f"\n{'─'*110}")
        print(f"MARKET btc-updown-5m-{mkt_ts}  |  Winner={winner}  |  Tick sayısı: {len(timeline)}")
        print(f"  GERÇEK BOT: {len(real)} emir, ${real_total:.2f}, Net PnL: ${real_pnl['net']:+.2f} (ROI %{real_pnl['roi_pct']:+.1f})")
        print(f"  BOT 383 (canlı): {len(bot_real)} emir, ${sum(o['usdc'] for o in bot_real):.2f}")

        summary['REAL']['total_usdc'] += real_total
        summary['REAL']['total_n'] += len(real)
        summary['REAL']['pnl_total'] += real_pnl['net']

        market_row = {
            'mkt': mkt_ts, 'winner': winner,
            'real_n': len(real), 'real_usdc': real_total, 'real_pnl': real_pnl['net'],
        }

        for h in HYPOTHESES:
            sim_trades = simulate_hypothesis(timeline, base_cfg, h, mkt_ts)
            sim_band = band_summary(sim_trades)
            sim_pnl = calc_pnl(sim_trades, winner)
            sim_total = sum(o['usdc'] for o in sim_trades)
            band_err = band_distance(real_band, sim_band)

            print(f"\n  [{h}] {len(sim_trades):>4} emir  ${sim_total:>7.2f}  "
                  f"({sim_total/real_total:.2f}x)  Net PnL: ${sim_pnl['net']:+8.2f}  "
                  f"Bant Hata: {band_err:>5.2f}")

            summary[h]['total_usdc'] += sim_total
            summary[h]['total_n'] += len(sim_trades)
            summary[h]['pnl_total'] += sim_pnl['net']
            summary[h]['band_err_total'] += band_err

            market_row[f'{h}_n'] = len(sim_trades)
            market_row[f'{h}_usdc'] = sim_total
            market_row[f'{h}_pnl'] = sim_pnl['net']
            market_row[f'{h}_err'] = band_err

        market_results.append(market_row)

    # ─── Genel Tablo ────────────────────────────────────────────────
    print("\n\n" + "=" * 110)
    print("GENEL ÖZET — 6 Market Toplamı")
    print("=" * 110)
    real_t = summary['REAL']['total_usdc']
    real_n = summary['REAL']['total_n']
    real_pnl = summary['REAL']['pnl_total']

    # H4 için loser ek lot küçük (8sh → 12sh)
    h4_loser_sh = 12.0  # gerçek 0.20-0.30 P50=15sh, 0.30-0.40=29sh ortalaması
    base_cfg['force_both_loser_sh'] = 8.0  # H2 için
    # H4 için override loser lot (eğer dinamik yapılırsa)

    print(f"\n  {'Hipotez':<10}  {'n':>5}  {'USDC':>10}  {'Oran':>7}  {'Net PnL':>10}  {'Bant Hata Avg':>14}  {'Toplam Skor':>12}")
    print("  " + "-" * 78)
    print(f"  {'GERÇEK':<10}  {real_n:>5}  ${real_t:>8.2f}  {'1.00x':>7}  ${real_pnl:>+8.2f}  {'0.00 (ref)':>14}  {'0 (ref)':>12}")

    best_h = None
    best_score = float('inf')
    for h in HYPOTHESES:
        s_t = summary[h]['total_usdc']
        s_n = summary[h]['total_n']
        s_pnl = summary[h]['pnl_total']
        avg_err = summary[h]['band_err_total'] / len(MARKETS)
        usdc_ratio = s_t / real_t if real_t > 0 else 0
        # Toplam skor: |usdc_oran-1| * 50 + bant_hata + |pnl_fark|/10
        score = abs(usdc_ratio - 1) * 50 + avg_err + abs(s_pnl - real_pnl) / 10
        if score < best_score:
            best_score = score
            best_h = h
        marker = " ← EN UYUMLU" if False else ""
        print(f"  {h:<10}  {s_n:>5}  ${s_t:>8.2f}  {usdc_ratio:>5.2f}x  ${s_pnl:>+8.2f}  {avg_err:>14.2f}  {score:>12.1f}")

    print(f"\n  → EN UYUMLU HIPOTEZ: {best_h}  (skor: {best_score:.1f})")

    # ─── Bant Bazında Detay (en iyi H için) ─────────────────────────
    print("\n\n" + "=" * 110)
    print(f"BANT BAZINDA DETAY — Tüm hipotezler vs Gerçek")
    print("=" * 110)
    print(f"\n  {'Bant':<12}  {'GERÇEK':>22}  {'H0':>22}  {'H3':>22}  {'H4':>22}")
    print(f"  {'':12}  {'n / shP50 / $P50':>22}  {'n / shP50 / $P50':>22}  {'n / shP50 / $P50':>22}  {'n / shP50 / $P50':>22}")
    print("  " + "-" * 110)

    bands = [(0.01, 0.10), (0.10, 0.20), (0.20, 0.30),
             (0.30, 0.40), (0.40, 0.50), (0.50, 0.60),
             (0.60, 0.70), (0.70, 0.80), (0.80, 0.99)]

    # Her bant için tüm market toplamları
    real_bands = defaultdict(list)
    sim_bands = {h: defaultdict(list) for h in HYPOTHESES}

    for mkt_ts in MARKETS:
        real = load_real_trades(mkt_ts)
        if not real:
            continue
        timeline = build_tick_timeline(real, mkt_ts)
        for lo, hi in bands:
            for o in real:
                if lo <= o['price'] < hi:
                    real_bands[(lo, hi)].append(o)
        for h in HYPOTHESES:
            sim_trades = simulate_hypothesis(timeline, base_cfg, h, mkt_ts)
            for lo, hi in bands:
                for o in sim_trades:
                    if lo <= o['price'] < hi:
                        sim_bands[h][(lo, hi)].append(o)

    for lo, hi in bands:
        rr = real_bands.get((lo, hi), [])
        if rr:
            sh_s = sorted(o['size'] for o in rr)
            u_s = sorted(o['usdc'] for o in rr)
            n = len(rr)
            real_str = f"{n:>3} {sh_s[n//2]:>5.0f}sh ${u_s[n//2]:>5.1f}"
        else:
            real_str = "  -    -    -"

        sim_strs = {}
        for h in HYPOTHESES:
            sl = sim_bands[h].get((lo, hi), [])
            if sl:
                sh_s = sorted(o['size'] for o in sl)
                u_s = sorted(o['usdc'] for o in sl)
                n = len(sl)
                sim_strs[h] = f"{n:>3} {sh_s[n//2]:>5.0f}sh ${u_s[n//2]:>5.1f}"
            else:
                sim_strs[h] = "  -    -    -"

        print(f"  {lo:.2f}-{hi:.2f}    {real_str:>22}  {sim_strs.get('H0','-'):>22}  {sim_strs.get('H3','-'):>22}  {sim_strs.get('H4','-'):>22}")


if __name__ == '__main__':
    main()
