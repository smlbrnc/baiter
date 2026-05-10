#!/usr/bin/env python3
"""
Signal-report.md dokümanına göre sinyal accuracy testi.

Karşılaştırma:
1. Mevcut signal_score
2. Window Delta (Section 5.1)
3. Combined Fusion (Section 9.1)
"""
import json
import math
import re
import sqlite3
from pathlib import Path

DB = Path(__file__).resolve().parents[1] / "data" / "baiter.db"


def window_delta_signal(window_open_mid, current_mid):
    """Signal-report Section 5.1 - Window Delta hesaplama
    
    >0.10% = decisive (weight 7)
    >0.02% = strong (weight 5)
    >0.005% = moderate (weight 3)
    >0.001% = slight (weight 1)
    """
    if window_open_mid <= 0:
        return 0, 5.0
    
    delta_pct = (current_mid - window_open_mid) / window_open_mid * 100
    direction = 1 if delta_pct > 0 else -1 if delta_pct < 0 else 0
    abs_delta = abs(delta_pct)
    
    if abs_delta > 0.10:
        weight = 7
    elif abs_delta > 0.02:
        weight = 5
    elif abs_delta > 0.005:
        weight = 3
    elif abs_delta > 0.001:
        weight = 1
    else:
        weight = 0
    
    signal = 5.0 + (direction * weight * 0.5)
    return delta_pct, signal


def combined_signal(window_delta_sig, bsi, ofi, cvd):
    """Signal-report Section 9.1 - Fusion Engine"""
    w_delta = 5.0
    w_bsi = 1.5
    w_ofi = 1.0
    w_cvd = 1.0
    
    delta_norm = (window_delta_sig - 5.0) / 3.5
    bsi_norm = max(-1, min(1, bsi))
    ofi_norm = max(-1, min(1, ofi))
    cvd_norm = max(-1, min(1, cvd))
    
    total_weight = w_delta + w_bsi + w_ofi + w_cvd
    combined = (w_delta * delta_norm + w_bsi * bsi_norm + 
                w_ofi * ofi_norm + w_cvd * cvd_norm) / total_weight
    
    return 5.0 + combined * 4.0


def proxy_winner(rows):
    if not rows:
        return None
    last = rows[-1]
    um = (float(last[5]) + float(last[6])) / 2.0
    dm = (float(last[7]) + float(last[8])) / 2.0
    return "Up" if um >= dm else "Down"


def main():
    conn = sqlite3.connect(DB)
    cur = conn.cursor()
    cur.execute("""
        SELECT ms.id, ms.slug, ms.start_ts, ms.end_ts
        FROM market_sessions ms
        JOIN bots b ON b.id = ms.bot_id
        WHERE b.strategy = 'gravie'
        ORDER BY ms.id
    """)
    sessions = cur.fetchall()

    results = {
        "current_signal": {"correct": 0, "total": 0, "details": []},
        "window_delta": {"correct": 0, "total": 0, "details": []},
        "combined": {"correct": 0, "total": 0, "details": []},
        "wd_strong": {"correct": 0, "total": 0, "details": []},
        "wd_decisive": {"correct": 0, "total": 0, "details": []},
    }

    for sid, slug, st, et in sessions:
        m = re.match(r"(btc|eth|sol|xrp)-updown-(5m|15m|1h|4h)-(\d+)", slug or "")
        if not m:
            continue
        
        cur.execute("""
            SELECT ts_ms, signal_score, bsi, ofi, cvd,
                   up_best_bid, up_best_ask, down_best_bid, down_best_ask
            FROM market_ticks WHERE market_session_id = ?
            ORDER BY ts_ms
        """, (sid,))
        rows = cur.fetchall()
        if len(rows) < 10:
            continue
        
        winner = proxy_winner(rows)
        if not winner:
            continue
        
        # Window open mid
        window_open_mid = None
        for r in rows[:20]:
            ub, ua, db, da = float(r[5]), float(r[6]), float(r[7]), float(r[8])
            if ub > 0 and ua > 0 and db > 0 and da > 0:
                up_mid = (ub + ua) / 2
                dn_mid = (db + da) / 2
                window_open_mid = (up_mid + (1 - dn_mid)) / 2
                break
        
        if window_open_mid is None:
            continue
        
        # T-60s sinyal
        target_ms = et * 1000 - 60000
        best_row = min(rows, key=lambda r: abs(int(r[0]) - target_ms))
        
        ts, sig_score, bsi, ofi, cvd, ub, ua, db, da = best_row
        if float(ub) <= 0 or float(ua) <= 0:
            continue
        
        up_mid = (float(ub) + float(ua)) / 2
        dn_mid = (float(db) + float(da)) / 2
        current_mid = (up_mid + (1 - dn_mid)) / 2
        
        delta_pct, wd_signal = window_delta_signal(window_open_mid, current_mid)
        comb_signal = combined_signal(wd_signal, float(bsi), float(ofi), float(cvd))
        
        pred_current = "Up" if float(sig_score) > 5.0 else "Down"
        pred_wd = "Up" if wd_signal > 5.0 else "Down"
        pred_comb = "Up" if comb_signal > 5.0 else "Down"
        
        # Current signal
        results["current_signal"]["total"] += 1
        if pred_current == winner:
            results["current_signal"]["correct"] += 1
        
        # Window delta
        results["window_delta"]["total"] += 1
        if pred_wd == winner:
            results["window_delta"]["correct"] += 1
        
        # Combined
        results["combined"]["total"] += 1
        if pred_comb == winner:
            results["combined"]["correct"] += 1
        
        # Window delta strong (|delta| > 0.02%)
        if abs(delta_pct) > 0.02:
            results["wd_strong"]["total"] += 1
            if pred_wd == winner:
                results["wd_strong"]["correct"] += 1
        
        # Window delta decisive (|delta| > 0.10%)
        if abs(delta_pct) > 0.10:
            results["wd_decisive"]["total"] += 1
            if pred_wd == winner:
                results["wd_decisive"]["correct"] += 1

    print("=== SİNYAL ACCURACY KARŞILAŞTIRMASI ===")
    print("(Signal-report.md Section 5.1 & 9.1)")
    print()
    for name, data in results.items():
        if data["total"] > 0:
            acc = data["correct"] / data["total"] * 100
            print(f"{name:20s}: {data['correct']:3d}/{data['total']:3d} = {acc:.1f}%")
    
    print()
    print("Açıklamalar:")
    print("  current_signal  : Mevcut signal_score (bsi+ofi+cvd bazlı)")
    print("  window_delta    : Signal-report Section 5.1 formülü")
    print("  combined        : Window delta + bsi + ofi + cvd fusion")
    print("  wd_strong       : Sadece |delta| > 0.02% olan sinyaller")
    print("  wd_decisive     : Sadece |delta| > 0.10% olan sinyaller")


if __name__ == "__main__":
    main()
