#!/usr/bin/env python3
"""Binance Latency Arbitrage backtest.

Strateji:
  1. Polymarket 5dk BTC market açılırken `window_open_btc = Binance BTC fiyatı`
  2. Her tick T anında: `current_btc = Binance BTC @ T`
  3. `delta_btc = current_btc - window_open_btc`
  4. Final winner = `UP if window_close_btc > window_open_btc else DOWN`
  5. Tetik koşulu: T_to_end <= entry_window_secs && |delta_btc| >= entry_threshold_usd
  6. Yön: delta_btc > 0 ise UP, < 0 ise DOWN
  7. BUY: Polymarket'te o yönün ASK fiyatından (market_ticks tablosundan)
  8. PnL: size × (1.0 if direction==winner else 0.0) - cost

Veri kaynakları:
  - Binance API: GET /api/v3/klines?symbol=BTCUSDT&interval=1s&startTime&endTime
  - Polymarket: market_ticks tablosu (DB'den)
  - Winner kuralı: canonical bid > 0.95 (UI ile birebir)
"""
import argparse
import math
import sqlite3
import sys
import time
from urllib.request import urlopen, Request
from urllib.error import URLError, HTTPError
import json

BINANCE_KLINES_URL = "https://api.binance.com/api/v3/klines"
FEE_RATE = 0.0002
MIN_PRICE = 0.10
MAX_PRICE = 0.95


def fetch_binance_btc_klines(start_ms: int, end_ms: int, interval: str = "1s"):
    """Binance public API'den BTC/USDT klines çek. Otomatik batch (max 1000 per req)."""
    all_klines = []
    cur = start_ms
    while cur < end_ms:
        url = (
            f"{BINANCE_KLINES_URL}?symbol=BTCUSDT&interval={interval}"
            f"&startTime={cur}&endTime={end_ms}&limit=1000"
        )
        try:
            req = Request(url, headers={"User-Agent": "Mozilla/5.0"})
            with urlopen(req, timeout=10) as resp:
                data = json.loads(resp.read().decode("utf-8"))
        except (URLError, HTTPError, TimeoutError) as e:
            print(f"  [warn] Binance API hata: {e}", file=sys.stderr)
            time.sleep(1)
            continue
        if not data:
            break
        all_klines.extend(data)
        last_close = data[-1][6]  # close_time
        cur = int(last_close) + 1
        time.sleep(0.05)  # rate limit nezaketi
        if len(data) < 1000:
            break
    return all_klines


def klines_to_price_lookup(klines):
    """Klines → {ts_ms: close_price} dict."""
    lookup = {}
    for k in klines:
        ts_ms = int(k[0])  # open_time
        close = float(k[4])
        lookup[ts_ms // 1000] = close  # ts_sec → close (yaklaşık)
    return lookup


def get_btc_at(price_lookup: dict, ts_sec: int, fallback_max_drift: int = 5):
    """ts_sec'e en yakın BTC fiyatı. ±N saniye drift'e izin ver."""
    for d in range(fallback_max_drift):
        if ts_sec - d in price_lookup:
            return price_lookup[ts_sec - d]
        if ts_sec + d in price_lookup:
            return price_lookup[ts_sec + d]
    return None


def winner_of(con, bot_id, sess) -> str | None:
    """Canonical: bid > 0.95."""
    r = con.execute(
        "SELECT up_best_bid, down_best_bid FROM market_ticks "
        "WHERE bot_id=? AND market_session_id=? ORDER BY ts_ms DESC LIMIT 1",
        (bot_id, sess),
    ).fetchone()
    if not r or r[0] is None:
        return None
    ub, db = r[0] or 0.0, r[1] or 0.0
    if ub > 0.95:
        return "UP"
    if db > 0.95:
        return "DOWN"
    return None


def sim_session_latency(
    con,
    bot_id,
    sess,
    btc_lookup,
    entry_window_secs: int,
    entry_threshold_usd: float,
    order_usdc: float,
    api_min_order_size: float = 5.0,
):
    """Tek session simulasyonu — Binance latency stratejisi."""
    w = winner_of(con, bot_id, sess)
    if w is None:
        return None
    sm = con.execute(
        "SELECT start_ts, end_ts FROM market_sessions WHERE id=?", (sess,)
    ).fetchone()
    start_ts = sm[0]
    end_ts = sm[1]

    # Window açılışında BTC
    btc_open = get_btc_at(btc_lookup, start_ts)
    if btc_open is None:
        return None

    # Tick'leri al
    ticks = con.execute(
        "SELECT ts_ms, up_best_bid, up_best_ask, down_best_bid, down_best_ask "
        "FROM market_ticks WHERE bot_id=? AND market_session_id=? ORDER BY ts_ms",
        (bot_id, sess),
    ).fetchall()

    triggered = False
    trade_dir = None
    entry_ask = None
    entry_ts = None
    for ts_ms, ub, ua, db, da in ticks:
        ts_sec = ts_ms // 1000
        sec_to_end = end_ts - ts_sec
        if sec_to_end <= 0:
            break
        if sec_to_end > entry_window_secs:
            continue
        # Trigger penceresinde — BTC delta kontrolü
        btc_now = get_btc_at(btc_lookup, ts_sec)
        if btc_now is None:
            continue
        delta = btc_now - btc_open
        if abs(delta) < entry_threshold_usd:
            continue
        # Yön: delta yönü
        if delta > 0:
            trade_dir = "UP"
            ask = ua
            bid = ub
        else:
            trade_dir = "DOWN"
            ask = da
            bid = db
        if ask <= 0 or bid < MIN_PRICE or bid > MAX_PRICE:
            continue
        # Fiyat çok yüksekse atla (zaten kazanmış)
        if ask >= 0.99:
            continue
        # Order size
        size = math.ceil(order_usdc / ask)
        cost_t = size * ask
        if cost_t < api_min_order_size:
            continue
        triggered = True
        entry_ask = ask
        entry_ts = ts_sec
        break  # tek atış per session

    if not triggered:
        return dict(sess=sess, w=w, triggered=False, pnl=0.0, cost=0.0,
                    fees=0.0, dir=None, entry_ask=None)

    # PnL hesabı
    size = math.ceil(order_usdc / entry_ask)
    cost = size * entry_ask
    fees = cost * FEE_RATE
    if trade_dir == w:
        pnl = size * 1.0 - cost  # winner share = $1
    else:
        pnl = -cost

    return dict(
        sess=sess, w=w, triggered=True, pnl=pnl, cost=cost, fees=fees,
        dir=trade_dir, entry_ask=entry_ask, entry_ts=entry_ts,
        btc_open=btc_open, btc_at_entry=get_btc_at(btc_lookup, entry_ts),
    )


def aggregate_scenario(con, bot_id, sessions, btc_lookup,
                       entry_window_secs, entry_threshold_usd, order_usdc):
    triggered = no_trigger = wins = losses = 0
    tot_cost = tot_pnl = tot_fee = 0.0
    no_btc = 0
    correct_dir = 0
    for s in sessions:
        r = sim_session_latency(
            con, bot_id, s, btc_lookup,
            entry_window_secs, entry_threshold_usd, order_usdc,
        )
        if r is None:
            no_btc += 1
            continue
        if not r["triggered"]:
            no_trigger += 1
            continue
        triggered += 1
        tot_cost += r["cost"]
        tot_pnl += r["pnl"]
        tot_fee += r["fees"]
        if r["pnl"] > 0:
            wins += 1
            correct_dir += 1
        else:
            losses += 1
    n = wins + losses
    return dict(
        triggered=triggered, no_trigger=no_trigger, no_btc=no_btc,
        wins=wins, losses=losses,
        cost=tot_cost, pnl=tot_pnl, fee=tot_fee,
        net=tot_pnl - tot_fee,
        roi=100 * (tot_pnl - tot_fee) / max(1, tot_cost),
        wr=100 * wins / max(1, n),
        dir_acc=100 * correct_dir / max(1, triggered),
    )


def main():
    ap = argparse.ArgumentParser(description="Binance Latency Arbitrage backtest")
    ap.add_argument("bot_id", type=int)
    ap.add_argument("db", nargs="?", default="/home/ubuntu/baiter/data/baiter.db")
    args = ap.parse_args()

    con = sqlite3.connect(args.db)
    sessions = [
        r[0] for r in con.execute(
            "SELECT id FROM market_sessions WHERE bot_id=? ORDER BY id", (args.bot_id,)
        ).fetchall()
    ]

    # Bot süresinin başı/sonu
    span = con.execute(
        "SELECT MIN(start_ts), MAX(end_ts) FROM market_sessions WHERE bot_id=?",
        (args.bot_id,),
    ).fetchone()
    if not span or span[0] is None:
        print(f"Bot {args.bot_id}: session yok")
        return
    span_start, span_end = span
    span_hours = (span_end - span_start) / 3600
    print("=" * 95)
    print(f"BOT {args.bot_id} — {len(sessions)} session, {span_hours:.1f} saat veri")
    print(f"  Span: {span_start} → {span_end} (UTC unix)")
    print("=" * 95)

    print("\nBinance API'den BTCUSDT 1s klines indiriliyor...")
    klines = fetch_binance_btc_klines(span_start * 1000, span_end * 1000, "1s")
    btc_lookup = klines_to_price_lookup(klines)
    print(f"  Toplam: {len(klines)} kline, {len(btc_lookup)} unique saniye")

    if not btc_lookup:
        print("Binance verisi alınamadı, çıkılıyor.")
        return

    # Senaryo grid: entry_window × entry_threshold × order_usdc
    print()
    print(f"{'Senaryo':<46} {'trig':>4} {'WR%':>5} {'cost':>9} "
          f"{'pnl':>9} {'NET':>9} {'ROI%':>7} {'dir%':>5}")
    print("-" * 100)

    scenarios = [
        # (etiket, entry_window_secs, entry_threshold_usd, order_usdc)
        ("T-30s + |Δ|>$10 + $5",  30, 10, 5),
        ("T-30s + |Δ|>$30 + $5",  30, 30, 5),
        ("T-30s + |Δ|>$50 + $5",  30, 50, 5),
        ("T-30s + |Δ|>$80 + $5",  30, 80, 5),
        ("T-30s + |Δ|>$80 + $20", 30, 80, 20),
        ("T-15s + |Δ|>$50 + $5",  15, 50, 5),
        ("T-15s + |Δ|>$80 + $5",  15, 80, 5),
        ("T-15s + |Δ|>$100 + $5", 15, 100, 5),
        ("T-15s + |Δ|>$100 + $20",15, 100, 20),
        ("T-60s + |Δ|>$30 + $5",  60, 30, 5),
        ("T-60s + |Δ|>$80 + $5",  60, 80, 5),
        ("T-60s + |Δ|>$120 + $5", 60, 120, 5),
        ("T-90s + |Δ|>$50 + $5",  90, 50, 5),
        ("T-120s + |Δ|>$80 + $5", 120, 80, 5),
        ("T-180s + |Δ|>$100 + $5",180, 100, 5),
    ]

    results = []
    for label, win, thr, ord_usdc in scenarios:
        r = aggregate_scenario(con, args.bot_id, sessions, btc_lookup,
                               win, thr, ord_usdc)
        results.append((label, r))
        print(f"{label:<46} {r['triggered']:>4} {r['wr']:>5.1f} {r['cost']:>9,.2f} "
              f"{r['pnl']:>+9,.2f} {r['net']:>+9,.2f} {r['roi']:>+7.2f} {r['dir_acc']:>5.1f}")

    # Sıralama
    print()
    print("[NET TOP 5]")
    for i, (label, r) in enumerate(
        sorted(results, key=lambda x: x[1]["net"], reverse=True)[:5], 1
    ):
        marker = " ⭐" if i == 1 else ""
        print(f"  {i}. {label:<48} NET=${r['net']:+,.2f}  ROI={r['roi']:+.2f}%  "
              f"trig={r['triggered']}/{len(sessions)}{marker}")

    print()
    print("[ROI TOP 5 (en az 20 trade)]")
    valid = [(l, r) for l, r in results if r["triggered"] >= 20]
    for i, (label, r) in enumerate(
        sorted(valid, key=lambda x: x[1]["roi"], reverse=True)[:5], 1
    ):
        marker = " ⭐" if i == 1 else ""
        print(f"  {i}. {label:<48} ROI={r['roi']:+.2f}%  NET=${r['net']:+,.2f}  "
              f"trig={r['triggered']} WR={r['wr']:.1f}%{marker}")


if __name__ == "__main__":
    main()
