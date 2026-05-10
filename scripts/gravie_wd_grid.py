#!/usr/bin/env python3
"""Window Delta + Sıkı threshold grid search"""
import json, math, re, sqlite3
from dataclasses import dataclass, field
from pathlib import Path

DB = Path(__file__).resolve().parents[1] / "data" / "baiter.db"

def window_delta_signal(wom, cm):
    if wom <= 0: return 0.0, 5.0
    d = (cm - wom) / wom * 100
    dr = 1 if d > 0 else -1 if d < 0 else 0
    ad = abs(d)
    if ad > 0.10: w = 7
    elif ad > 0.02: w = 5
    elif ad > 0.005: w = 3
    elif ad > 0.001: w = 1
    else: w = 0
    return d, 5.0 + dr * w * 0.5

@dataclass
class P:
    up_thr: float = 7.0
    dn_thr: float = 3.0
    std_max: float = 0.2
    avg_max: float = 0.80
    tick: int = 5
    cd_ms: int = 4000
    w_usdc: float = 15.0
    h_usdc: float = 5.0
    max_px: float = 0.65
    t_cut: float = 30.0
    late: float = 90.0
    ema_a: float = 0.3
    fak: float = 50.0

@dataclass
class M:
    up: float = 0.0
    down: float = 0.0
    avg_up: float = 0.0
    avg_down: float = 0.0
    cost: float = 0.0
    def add(s, o, px, sz):
        s.cost += px * sz
        if o == "Up":
            nu = s.up + sz
            s.avg_up = (s.avg_up * s.up + px * sz) / nu if nu > 0 else 0
            s.up = nu
        else:
            nd = s.down + sz
            s.avg_down = (s.avg_down * s.down + px * sz) / nd if nd > 0 else 0
            s.down = nd

def sim(ticks, st, et, p):
    m = M()
    wom, ema, hist = None, None, []
    ls, lwm, lhm = -999999, 0, 0
    for ts, ub, ua, db, da in ticks:
        ub, ua, db, da = float(ub), float(ua), float(db), float(da)
        if wom is None and ub > 0 and ua > 0 and db > 0 and da > 0:
            wom = ((ub + ua) / 2 + (1 - (db + da) / 2)) / 2
            continue
        if wom is None: continue
        te = float(et) - ts / 1000.0
        if te <= p.t_cut: continue
        rel = (ts // 1000) - st
        if rel % p.tick != 0 or rel == ls: continue
        ls = rel
        if ua <= 0 or da <= 0: continue
        cm = ((ub + ua) / 2 + (1 - (db + da) / 2)) / 2
        _, wd = window_delta_signal(wom, cm)
        c = wd - 5.0
        ema = c if ema is None else p.ema_a * c + (1 - p.ema_a) * ema
        sm = ema + 5.0
        if len(hist) >= 3: hist.pop(0)
        hist.append(sm)
        if len(hist) < 3: continue
        std = math.sqrt(sum((x - sum(hist)/3)**2 for x in hist)/3)
        if std > p.std_max: continue
        if sm > p.up_thr: w = "Up"
        elif sm < p.dn_thr: w = "Down"
        else: continue
        h = "Down" if w == "Up" else "Up"
        wok = p.late <= 0 or te > p.late
        wa = ua if w == "Up" else da
        if wok and wa > 0 and wa <= p.max_px and ts - lwm >= p.cd_ms:
            sz = min(math.ceil(p.w_usdc / wa), p.fak)
            of, os = (m.up, m.avg_up * m.up) if w == "Up" else (m.down, m.avg_down * m.down)
            opf, ops = (m.down, m.avg_down * m.down) if w == "Up" else (m.up, m.avg_up * m.up)
            ok = True
            if opf > 0:
                na = (os + sz * wa) / (of + sz)
                if na + ops / opf >= p.avg_max: ok = False
            if ok and sz > 0:
                m.add(w, wa, sz)
                lwm = ts
        wf = m.up if w == "Up" else m.down
        ha = ua if h == "Up" else da
        if ha > 0 and ha <= p.max_px and wf > 0 and ts - lhm >= p.cd_ms:
            sz = min(math.ceil(p.h_usdc / ha), p.fak)
            of, os = (m.up, m.avg_up * m.up) if h == "Up" else (m.down, m.avg_down * m.down)
            opf, ops = (m.down, m.avg_down * m.down) if h == "Up" else (m.up, m.avg_up * m.up)
            ok = True
            if opf > 0:
                na = (os + sz * ha) / (of + sz)
                if na + ops / opf >= p.avg_max: ok = False
            if ok and sz > 0:
                m.add(h, ha, sz)
                lhm = ts
    return m

def pw(rows):
    if not rows: return None
    last = rows[-1]
    um = (float(last[1]) + float(last[2])) / 2
    dm = (float(last[3]) + float(last[4])) / 2
    return "Up" if um >= dm else "Down"

def main():
    conn = sqlite3.connect(DB)
    cur = conn.cursor()
    cur.execute("SELECT ms.id, ms.slug, ms.start_ts, ms.end_ts FROM market_sessions ms JOIN bots b ON b.id = ms.bot_id WHERE b.strategy = 'gravie' ORDER BY ms.id")
    sessions = cur.fetchall()

    configs = [
        ("decisive", P(up_thr=7.5, dn_thr=2.5, std_max=0.15)),
        ("strong", P(up_thr=7.0, dn_thr=3.0, std_max=0.20)),
        ("moderate", P(up_thr=6.5, dn_thr=3.5, std_max=0.25)),
        ("tight_arb", P(up_thr=7.0, dn_thr=3.0, avg_max=0.75)),
        ("max_dual", P(up_thr=6.0, dn_thr=4.0, avg_max=0.85, std_max=0.30)),
        ("ultra_tight", P(up_thr=8.0, dn_thr=2.0, std_max=0.10)),
    ]

    results = {}
    for name, p in configs:
        tc, tp, ws, ls = 0, 0, 0, 0
        dw, dl, sw, sl = 0, 0, 0, 0
        for sid, slug, st, et in sessions:
            if not re.match(r"(btc|eth|sol|xrp)-updown-(5m|15m|1h|4h)-\d+", slug or ""): continue
            cur.execute("SELECT ts_ms, up_best_bid, up_best_ask, down_best_bid, down_best_ask FROM market_ticks WHERE market_session_id = ? ORDER BY ts_ms", (sid,))
            rows = cur.fetchall()
            if len(rows) < 10: continue
            m = sim(rows, st, et, p)
            if m.cost <= 0: continue
            winner = pw(rows)
            if not winner: continue
            payout = m.up if winner == "Up" else m.down
            pnl = payout - m.cost
            tc += m.cost
            tp += pnl
            dual = m.up > 0 and m.down > 0
            if pnl > 0:
                ws += 1
                if dual: dw += 1
                else: sw += 1
            else:
                ls += 1
                if dual: dl += 1
                else: sl += 1
        wr = ws / (ws + ls) * 100 if ws + ls > 0 else 0
        dwr = dw / (dw + dl) * 100 if dw + dl > 0 else 0
        swr = sw / (sw + sl) * 100 if sw + sl > 0 else 0
        results[name] = {
            "cost": round(tc, 2), "pnl": round(tp, 2),
            "roi": round(tp / tc * 100, 2) if tc > 0 else 0,
            "wr": round(wr, 2), "n": ws + ls,
            "dual_wr": round(dwr, 2), "single_wr": round(swr, 2),
            "dual_n": dw + dl, "single_n": sw + sl,
        }

    print("=== WINDOW DELTA + SIKI THRESHOLD ===")
    print()
    for name, r in results.items():
        print(f"{name:12s}: ROI={r['roi']:+6.2f}% | WR={r['wr']:5.2f}% | Dual={r['dual_wr']:5.2f}%({r['dual_n']}) | Single={r['single_wr']:5.2f}%({r['single_n']})")

if __name__ == "__main__":
    main()
