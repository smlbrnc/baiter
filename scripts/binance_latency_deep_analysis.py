#!/usr/bin/env python3
"""Binance Latency Arbitrage — derin analiz.

Eklemeler:
  1. Geniş grid (8x6 = 48 parametre kombinasyonu)
  2. Per-session detay (hangi market kazandı/kaybetti)
  3. Saatlik dağılım (hangi saatlerde daha karlı)
  4. BTC volatility ile korelasyon
  5. Çoklu bot cross-validation
  6. Direction accuracy by delta size
  7. Optimum entry timing (T-X analizi)
  8. Win/Loss istatistikleri (avg win, avg loss, profit factor)
"""
import argparse
import math
import sqlite3
import sys
import time
from urllib.request import urlopen, Request
from urllib.error import URLError, HTTPError
import json
from collections import defaultdict
from datetime import datetime, timezone

BINANCE_KLINES_URL = "https://api.binance.com/api/v3/klines"
FEE_RATE = 0.0002
MIN_PRICE = 0.10
MAX_PRICE = 0.95


def fetch_binance_btc_klines(start_ms: int, end_ms: int, interval: str = "1s"):
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
        last_close = data[-1][6]
        cur = int(last_close) + 1
        time.sleep(0.05)
        if len(data) < 1000:
            break
    return all_klines


def klines_to_lookup(klines):
    return {int(k[0]) // 1000: float(k[4]) for k in klines}


def get_btc(price_lookup, ts_sec, drift=5):
    for d in range(drift):
        if ts_sec - d in price_lookup:
            return price_lookup[ts_sec - d]
        if ts_sec + d in price_lookup:
            return price_lookup[ts_sec + d]
    return None


def winner_of(con, bot_id, sess):
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


def sim_session(
    con, bot_id, sess, btc_lookup,
    entry_window_secs, entry_threshold_usd, order_usdc,
    api_min_order_size=5.0,
    multi_trade=False,  # True: aynı session'da birden fazla trade
    cooldown_secs=10,
):
    """Tek session simulasyon. multi_trade=True ise her cooldown sonrası yeniden tetik."""
    w = winner_of(con, bot_id, sess)
    if w is None:
        return None
    sm = con.execute(
        "SELECT start_ts, end_ts FROM market_sessions WHERE id=?", (sess,)
    ).fetchone()
    start_ts, end_ts = sm[0], sm[1]

    btc_open = get_btc(btc_lookup, start_ts)
    if btc_open is None:
        return None

    ticks = con.execute(
        "SELECT ts_ms, up_best_bid, up_best_ask, down_best_bid, down_best_ask "
        "FROM market_ticks WHERE bot_id=? AND market_session_id=? ORDER BY ts_ms",
        (bot_id, sess),
    ).fetchall()

    trades = []
    last_entry_ts = 0

    for ts_ms, ub, ua, db, da in ticks:
        ts_sec = ts_ms // 1000
        sec_to_end = end_ts - ts_sec
        if sec_to_end <= 0:
            break
        if sec_to_end > entry_window_secs:
            continue
        if ts_sec - last_entry_ts < cooldown_secs:
            continue
        btc_now = get_btc(btc_lookup, ts_sec)
        if btc_now is None:
            continue
        delta = btc_now - btc_open
        if abs(delta) < entry_threshold_usd:
            continue
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
        if ask >= 0.99:
            continue
        size = math.ceil(order_usdc / ask)
        cost_t = size * ask
        if cost_t < api_min_order_size:
            continue
        # PnL
        if trade_dir == w:
            pnl = size * 1.0 - cost_t
        else:
            pnl = -cost_t
        fees = cost_t * FEE_RATE
        trades.append(dict(
            ts_sec=ts_sec, dir=trade_dir, ask=ask, size=size, cost=cost_t,
            pnl=pnl, fees=fees, delta=delta, sec_to_end=sec_to_end,
        ))
        last_entry_ts = ts_sec
        if not multi_trade:
            break

    if not trades:
        return dict(sess=sess, w=w, n_trades=0, pnl=0.0, cost=0.0, fees=0.0)

    return dict(
        sess=sess, w=w, n_trades=len(trades),
        pnl=sum(t["pnl"] for t in trades),
        cost=sum(t["cost"] for t in trades),
        fees=sum(t["fees"] for t in trades),
        first_trade=trades[0],
        all_trades=trades,
        start_ts=start_ts,
    )


def aggregate(con, bot_id, sessions, btc_lookup, win, thr, ord_usdc,
              multi_trade=False):
    triggered_sessions = 0
    no_trigger = 0
    no_btc = 0
    wins = losses = 0
    tot_cost = tot_pnl = tot_fee = 0.0
    n_total_trades = 0
    win_pnl = []
    loss_pnl = []
    per_sess = []
    hourly = defaultdict(lambda: {"n": 0, "pnl": 0, "wins": 0})
    for s in sessions:
        r = sim_session(con, bot_id, s, btc_lookup, win, thr, ord_usdc,
                        multi_trade=multi_trade)
        if r is None:
            no_btc += 1
            continue
        if r["n_trades"] == 0:
            no_trigger += 1
            continue
        triggered_sessions += 1
        tot_cost += r["cost"]
        tot_pnl += r["pnl"]
        tot_fee += r["fees"]
        n_total_trades += r["n_trades"]
        if r["pnl"] > 0:
            wins += 1
            win_pnl.append(r["pnl"])
        else:
            losses += 1
            loss_pnl.append(r["pnl"])
        per_sess.append(r)
        # Hourly
        hr = datetime.fromtimestamp(r["start_ts"], tz=timezone.utc).strftime("%H")
        hourly[hr]["n"] += 1
        hourly[hr]["pnl"] += r["pnl"]
        if r["pnl"] > 0:
            hourly[hr]["wins"] += 1

    avg_win = sum(win_pnl) / len(win_pnl) if win_pnl else 0.0
    avg_loss = sum(loss_pnl) / len(loss_pnl) if loss_pnl else 0.0
    profit_factor = (
        sum(win_pnl) / abs(sum(loss_pnl)) if loss_pnl and sum(loss_pnl) != 0 else 999
    )
    return dict(
        triggered=triggered_sessions, no_trigger=no_trigger, no_btc=no_btc,
        wins=wins, losses=losses, n_trades=n_total_trades,
        cost=tot_cost, pnl=tot_pnl, fee=tot_fee,
        net=tot_pnl - tot_fee,
        roi=100 * (tot_pnl - tot_fee) / max(1, tot_cost),
        wr=100 * wins / max(1, wins + losses),
        avg_win=avg_win, avg_loss=avg_loss, profit_factor=profit_factor,
        per_sess=per_sess, hourly=hourly,
    )


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("bot_id", type=int)
    ap.add_argument("db", nargs="?", default="/home/ubuntu/baiter/data/baiter.db")
    args = ap.parse_args()

    con = sqlite3.connect(args.db)
    sessions = [
        r[0] for r in con.execute(
            "SELECT id FROM market_sessions WHERE bot_id=? ORDER BY id", (args.bot_id,)
        ).fetchall()
    ]
    span = con.execute(
        "SELECT MIN(start_ts), MAX(end_ts) FROM market_sessions WHERE bot_id=?",
        (args.bot_id,),
    ).fetchone()
    span_start, span_end = span
    span_hours = (span_end - span_start) / 3600

    print("=" * 100)
    print(f"BINANCE LATENCY DEEP ANALYSIS — Bot {args.bot_id}")
    print(f"  {len(sessions)} session, {span_hours:.1f} saat veri")
    print("=" * 100)

    print("\nBinance API'den BTCUSDT 1s klines indiriliyor...")
    klines = fetch_binance_btc_klines(span_start * 1000, span_end * 1000, "1s")
    btc_lookup = klines_to_lookup(klines)
    print(f"  Toplam: {len(klines)} kline\n")

    if not btc_lookup:
        return

    # === GENİŞ GRID ===
    windows = [15, 30, 45, 60, 90, 120, 180, 240]
    thresholds = [10, 20, 30, 50, 80, 120]
    order_usdc = 5

    print(f"\n[Grid] {len(windows)}x{len(thresholds)} = {len(windows)*len(thresholds)} senaryo "
          f"(order=${order_usdc}, single trade per session)\n")
    print(f"{'Window':>7} | " + " | ".join(f"Δ>${t:<3}" for t in thresholds))
    print("-" * (10 + 11 * len(thresholds)))

    grid = {}
    for w in windows:
        row = []
        for thr in thresholds:
            r = aggregate(con, args.bot_id, sessions, btc_lookup, w, thr, order_usdc)
            grid[(w, thr)] = r
            cell = f"R{r['roi']:+5.1f}%/W{r['wr']:.0f}%"
            row.append(cell)
        print(f"T-{w:>3}s | " + " | ".join(f"{c:<10}" for c in row))

    # En iyi senaryolar
    print("\n[NET TOP 10]")
    sorted_grid = sorted(grid.items(), key=lambda x: x[1]["net"], reverse=True)
    for i, ((w, thr), r) in enumerate(sorted_grid[:10], 1):
        print(f"  {i:>2}. T-{w}s + Δ>${thr:<4} → NET=${r['net']:+7.2f}  "
              f"ROI={r['roi']:+6.2f}%  WR={r['wr']:5.1f}%  trig={r['triggered']:>3}  "
              f"PF={r['profit_factor']:.2f}")

    print("\n[ROI TOP 10 (en az 20 trigger)]")
    valid = [(k, v) for k, v in grid.items() if v["triggered"] >= 20]
    sorted_roi = sorted(valid, key=lambda x: x[1]["roi"], reverse=True)
    for i, ((w, thr), r) in enumerate(sorted_roi[:10], 1):
        print(f"  {i:>2}. T-{w}s + Δ>${thr:<4} → ROI={r['roi']:+6.2f}%  "
              f"NET=${r['net']:+7.2f}  WR={r['wr']:5.1f}%  trig={r['triggered']:>3}  "
              f"avg_win=${r['avg_win']:+5.2f}  avg_loss=${r['avg_loss']:+5.2f}")

    # === EN İYİ SENARYO İÇİN DERİN ANALİZ ===
    if not sorted_roi:
        print("\nYeterli trigger yok, derin analiz atlandı.")
        return
    best_w, best_thr = sorted_roi[0][0]
    best = sorted_roi[0][1]
    print(f"\n{'=' * 100}")
    print(f"EN İYİ SENARYO DETAY: T-{best_w}s + Δ>${best_thr}")
    print(f"{'=' * 100}\n")

    print("[Genel istatistik]")
    print(f"  Trade sayısı:          {best['n_trades']}")
    print(f"  Trigger sessions:      {best['triggered']}/{len(sessions)}")
    print(f"  Wins / Losses:         {best['wins']} / {best['losses']}")
    print(f"  Winrate:               {best['wr']:.2f}%")
    print(f"  Avg win:               ${best['avg_win']:+.2f}")
    print(f"  Avg loss:              ${best['avg_loss']:+.2f}")
    print(f"  Profit factor:         {best['profit_factor']:.2f}")
    print(f"  Total cost:            ${best['cost']:.2f}")
    print(f"  Total PnL:             ${best['pnl']:+.2f}")
    print(f"  Fee:                   ${best['fee']:.4f}")
    print(f"  NET:                   ${best['net']:+.2f}")
    print(f"  ROI:                   {best['roi']:+.2f}%")

    # Saatlik dağılım
    print("\n[Saatlik dağılım — UTC]")
    print(f"  {'Saat':>5} | {'n':>3} | {'win':>3} {'WR%':>5} | {'PnL':>9}")
    for hr in sorted(best["hourly"].keys()):
        h = best["hourly"][hr]
        wr_h = 100 * h["wins"] / max(1, h["n"])
        marker = " 🔥" if h["pnl"] > 5 else ""
        print(f"  {hr:>5} | {h['n']:>3} | {h['wins']:>3} {wr_h:>5.1f} | "
              f"{h['pnl']:>+9.2f}{marker}")

    # Win/Loss session breakdown (top 5 win, top 5 loss)
    print("\n[En kârlı 5 trade]")
    sorted_sess = sorted(best["per_sess"], key=lambda x: x["pnl"], reverse=True)
    for r in sorted_sess[:5]:
        ft = r["first_trade"]
        print(f"  sess {r['sess']:>5} | {r['w']:>4} winner | {ft['dir']:>4} BUY @ ${ft['ask']:.3f} "
              f"size={ft['size']} | T-{ft['sec_to_end']:>3}s | Δ=${ft['delta']:+7.2f} "
              f"| pnl=${r['pnl']:+.2f}")

    print("\n[En zararlı 5 trade]")
    for r in sorted_sess[-5:]:
        ft = r["first_trade"]
        print(f"  sess {r['sess']:>5} | {r['w']:>4} winner | {ft['dir']:>4} BUY @ ${ft['ask']:.3f} "
              f"size={ft['size']} | T-{ft['sec_to_end']:>3}s | Δ=${ft['delta']:+7.2f} "
              f"| pnl=${r['pnl']:+.2f}")

    # Multi-trade vs single
    print("\n[Single vs Multi trade per session karşılaştırma]")
    single = best  # zaten single
    multi = aggregate(con, args.bot_id, sessions, btc_lookup,
                      best_w, best_thr, order_usdc, multi_trade=True)
    print(f"  Single trade: trades={single['n_trades']:>3} cost=${single['cost']:>7.2f} "
          f"NET=${single['net']:+7.2f} ROI={single['roi']:+5.2f}% WR={single['wr']:.1f}%")
    print(f"  Multi trade:  trades={multi['n_trades']:>3} cost=${multi['cost']:>7.2f} "
          f"NET=${multi['net']:+7.2f} ROI={multi['roi']:+5.2f}% WR={multi['wr']:.1f}%")

    # Order size etkisi
    print("\n[Order size etkisi (T-{}s + Δ>${})]".format(best_w, best_thr))
    print(f"  {'order':>7} | {'trig':>4} {'cost':>8} {'NET':>8} {'ROI%':>6}")
    for ord_size in [5, 10, 20, 50, 100]:
        r2 = aggregate(con, args.bot_id, sessions, btc_lookup,
                       best_w, best_thr, ord_size)
        print(f"  ${ord_size:>5} | {r2['triggered']:>4} {r2['cost']:>8.2f} "
              f"{r2['net']:>+8.2f} {r2['roi']:>+6.2f}")


if __name__ == "__main__":
    main()
