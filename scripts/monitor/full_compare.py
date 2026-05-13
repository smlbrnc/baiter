#!/usr/bin/env python3
"""
KAPSAMLI CANLI İZLEME — Real bot vs Bizim Botlar (131/132/134)

Yeni özellikler:
- Per-market tam detaylı tablo (4 bot × tüm metrikler)
- Phase analizi (T-300..240 / T-240..180 / ... / T-10..0)
- Price band breakdown (Scoop/Long/Mid/High/LW)
- arb_mult shot-by-shot doğrulama (beklenen vs gerçek)
- Direction accuracy (her market için real yön vs bot yön)
- Cumulative özet (tüm marketler boyunca toplam istatistik)
- TAKER/MAKER ayrımı (doğru distinct emir sayımı)
- Bug detection (10 kriter)

Çıktı:
- Konsol (renkli özet)
- /tmp/monitor_summary.json (cumulative metrics)
- scripts/monitor/full_report.log (tüm output)
"""
import re, json, time, subprocess, sys, os
from datetime import datetime
from collections import defaultdict

TERM_LOG = "/Users/dorukbirinci/.cursor/projects/Users-dorukbirinci-Desktop-baiter-pro/terminals/6.txt"
SSH_KEY = os.path.expanduser("~/Desktop/smlbrnc.pem")
VPS = "ubuntu@79.125.42.234"
DB = "/home/ubuntu/baiter/data/baiter.db"
CONTROL_BOT = 131
COMPARE_BOTS = [131, 132, 134]
BOT_NAMES = {131:"131-Kons", 132:"132-Agr", 134:"134-LowImb"}
REPORT = "/Users/dorukbirinci/Desktop/baiter-pro/scripts/monitor/full_report.log"
SUMMARY_JSON = "/tmp/monitor_summary.json"

# Renkli çıktı
class C:
    R = "\033[91m"; G = "\033[92m"; Y = "\033[93m"
    B = "\033[94m"; M = "\033[95m"; CY = "\033[96m"
    BOLD = "\033[1m"; END = "\033[0m"

# arb_mult formülü (bonereaper.rs ile birebir aynı, GÜNCEL)
def expected_arb_mult(w_ask, to_end):
    if w_ask >= 0.99:
        if to_end <= 10: return 1.7
        elif to_end <= 30: return 5.7
        elif to_end <= 60: return 5.5
        elif to_end <= 120: return 11.5
        else: return 20.0  # 13x → 20x güncellendi
    elif w_ask >= 0.97:
        if to_end <= 10: return 1.0
        elif to_end <= 30: return 3.7
        elif to_end <= 60: return 6.1
        elif to_end <= 120: return 4.4
        else: return 9.0
    elif w_ask >= 0.95:
        return 4.0 if to_end <= 60 else 2.0
    return 1.0

def log(msg, color=None):
    line = f"[{datetime.now().strftime('%H:%M:%S')}] {msg}"
    if color:
        print(color + line + C.END, flush=True)
    else:
        print(line, flush=True)
    with open(REPORT, "a") as f:
        f.write(line + "\n")

def ssh_query(sql):
    cmd = ["ssh", "-i", SSH_KEY, "-o", "StrictHostKeyChecking=no", VPS,
           f"sqlite3 -separator '|' {DB} \"{sql}\""]
    try:
        return subprocess.check_output(cmd, timeout=20).decode().strip()
    except Exception as e:
        return f"ERR: {e}"

def get_bot_config(bot_id):
    sql = (f"SELECT order_usdc, strategy_params FROM bots WHERE id={bot_id};")
    out = ssh_query(sql)
    if out.startswith("ERR") or not out: return None
    parts = out.split("|", 1)
    if len(parts) < 2: return None
    try: params = json.loads(parts[1])
    except: params = {}
    defaults = {
        "bonereaper_buy_cooldown_ms": 3000,
        "bonereaper_late_winner_secs": 180,
        "bonereaper_late_winner_bid_thr": 0.90,
        "bonereaper_late_winner_usdc": 100.0,
        "bonereaper_lw_max_per_session": 20,
        "bonereaper_imbalance_thr": 1000.0,
        "bonereaper_max_avg_sum": 1.05,
        "bonereaper_first_spread_min": 0.02,
        "bonereaper_size_longshot_usdc": 15.0,
        "bonereaper_size_mid_usdc": 23.0,
        "bonereaper_size_high_usdc": 37.0,
        "bonereaper_loser_scalp_usdc": 10.0,
        "bonereaper_loser_scalp_max_price": 0.30,
    }
    for k, dv in defaults.items():
        if params.get(k) is None: params[k] = dv
    return {"order_usdc": float(parts[0]), "params": params}

def parse_real_trades(slug):
    """Terminal'den raw trade kayıtları"""
    if not os.path.exists(TERM_LOG): return []
    trades = []
    with open(TERM_LOG) as f:
        for ln in f:
            if "slug="+slug not in ln: continue
            m_t = re.search(r't=([\d\-T:Z]+)', ln)
            m_type = re.search(r'type=(\w+)', ln)
            m_p = re.search(r'price=([\d.]+)', ln)
            m_u = re.search(r'usd=([\d.]+)', ln)
            m_s = re.search(r'shares=([\d.]+)', ln)
            m_side = re.search(r'side=(\w+)', ln)
            if not (m_t and m_p and m_u and m_s): continue
            if m_side and m_side.group(1) != "Buy": continue
            try:
                ts = int(datetime.fromisoformat(m_t.group(1).replace("Z","+00:00")).timestamp())
            except: continue
            trades.append({"ts": ts, "price": float(m_p.group(1)),
                          "usd": float(m_u.group(1)), "sh": float(m_s.group(1)),
                          "type": m_type.group(1) if m_type else "?"})
    trades.sort(key=lambda x: x["ts"])
    return trades

def group_real_distinct(trades, time_window=2):
    """Real bot raw → distinct emirler (TAKER ayrı, MAKER gruplu)"""
    if not trades: return []
    takers = [t for t in trades if t["type"] == "OrdersMatched"]
    makers = sorted([t for t in trades if t["type"] == "OrderFilled"],
                    key=lambda x: (round(x["price"],3), x["ts"]))
    distinct = [{"ts": t["ts"], "price": t["price"], "usd": t["usd"],
                 "sh": t["sh"], "kind": "TAKER"} for t in takers]
    if makers:
        current = [makers[0]]
        for m in makers[1:]:
            if (abs(m["price"] - current[-1]["price"]) < 0.005 and
                abs(m["ts"] - current[-1]["ts"]) <= time_window):
                current.append(m)
            else:
                distinct.append({"ts": min(t["ts"] for t in current),
                                "price": current[0]["price"],
                                "usd": sum(t["usd"] for t in current),
                                "sh": sum(t["sh"] for t in current),
                                "kind": f"MAKER({len(current)})"})
                current = [m]
        if current:
            distinct.append({"ts": min(t["ts"] for t in current),
                            "price": current[0]["price"],
                            "usd": sum(t["usd"] for t in current),
                            "sh": sum(t["sh"] for t in current),
                            "kind": f"MAKER({len(current)})"})
    distinct.sort(key=lambda x: x["ts"])
    return distinct

def get_bot_trades(slug, bot_id):
    """Bot trade'leri (TAKER ayrı, MAKER gruplu)"""
    sql = (f"SELECT outcome, size, price, ts_ms, trader_side FROM trades "
           f"WHERE bot_id={bot_id} AND market_session_id IN "
           f"(SELECT id FROM market_sessions WHERE bot_id={bot_id} AND slug='{slug}') "
           f"ORDER BY ts_ms;")
    out = ssh_query(sql)
    if out.startswith("ERR"): return []
    raw = []
    for ln in out.split("\n"):
        parts = ln.split("|")
        if len(parts) < 5: continue
        raw.append({"outcome": parts[0], "size": float(parts[1]),
                   "price": float(parts[2]), "ts_ms": int(parts[3]),
                   "side": parts[4]})
    out_list = []
    maker_buf = []
    for t in raw:
        if t["side"] == "TAKER":
            out_list.append({"outcome": t["outcome"], "size": t["size"],
                            "price": t["price"], "ts": t["ts_ms"]//1000,
                            "usd": t["size"]*t["price"], "ts_ms": t["ts_ms"],
                            "side": "TAKER"})
        else:
            maker_buf.append(t)
    maker_buf.sort(key=lambda x: (x["outcome"], round(x["price"],4), x["ts_ms"]))
    if maker_buf:
        current = [maker_buf[0]]
        for m in maker_buf[1:]:
            if (m["outcome"] == current[-1]["outcome"] and
                abs(m["price"] - current[-1]["price"]) < 0.001 and
                abs(m["ts_ms"] - current[-1]["ts_ms"]) <= 2000):
                current.append(m)
            else:
                size = sum(t["size"] for t in current)
                ts_ms = min(t["ts_ms"] for t in current)
                out_list.append({"outcome": current[0]["outcome"], "size": size,
                                "price": current[0]["price"], "ts": ts_ms//1000,
                                "usd": size*current[0]["price"], "ts_ms": ts_ms,
                                "side": f"MAKER({len(current)})"})
                current = [m]
        if current:
            size = sum(t["size"] for t in current)
            ts_ms = min(t["ts_ms"] for t in current)
            out_list.append({"outcome": current[0]["outcome"], "size": size,
                            "price": current[0]["price"], "ts": ts_ms//1000,
                            "usd": size*current[0]["price"], "ts_ms": ts_ms,
                            "side": f"MAKER({len(current)})"})
    out_list.sort(key=lambda x: x["ts_ms"])
    return out_list

# ─────────────────────────────────────────────────────────────────────────
# ANALİZ FONKSİYONLARI
# ─────────────────────────────────────────────────────────────────────────

def phase_breakdown(trades, market_end, ts_field="ts"):
    """Trade'leri T-X bantlarına böl"""
    phases = ["T-300..240","T-240..180","T-180..120","T-120..60",
              "T-60..30","T-30..10","T-10..0"]
    counts = defaultdict(int)
    usdc = defaultdict(float)
    for t in trades:
        to_end = market_end - t[ts_field]
        if to_end > 240: ph = "T-300..240"
        elif to_end > 180: ph = "T-240..180"
        elif to_end > 120: ph = "T-180..120"
        elif to_end > 60: ph = "T-120..60"
        elif to_end > 30: ph = "T-60..30"
        elif to_end > 10: ph = "T-30..10"
        else: ph = "T-10..0"
        counts[ph] += 1
        usdc[ph] += t.get("usd", t.get("usdc", 0))
    return phases, counts, usdc

def price_band_breakdown(trades):
    """Scoop/Long/Mid/High/LW bantlarına böl"""
    bands = ["Scoop $0-0.20", "Long $0.20-0.40", "Mid $0.40-0.65",
             "High $0.65-0.85", "LW $0.85-0.95", "Arb $0.95-0.97",
             "Arb $0.97-0.99", "Arb $0.99+"]
    counts = defaultdict(int)
    usdc = defaultdict(float)
    for t in trades:
        p = t["price"]
        if p < 0.20: b = "Scoop $0-0.20"
        elif p < 0.40: b = "Long $0.20-0.40"
        elif p < 0.65: b = "Mid $0.40-0.65"
        elif p < 0.85: b = "High $0.65-0.85"
        elif p < 0.95: b = "LW $0.85-0.95"
        elif p < 0.97: b = "Arb $0.95-0.97"
        elif p < 0.99: b = "Arb $0.97-0.99"
        else: b = "Arb $0.99+"
        counts[b] += 1
        usdc[b] += t.get("usd", t.get("usdc", 0))
    return bands, counts, usdc

def detect_direction(trades, key="price"):
    """Trade'lerden yön tahmini (high vs low USD ağırlıklı)"""
    high = sum(t.get("usd", t.get("usdc", 0)) for t in trades if t["price"] > 0.55)
    low = sum(t.get("usd", t.get("usdc", 0)) for t in trades if t["price"] < 0.45)
    if high > low * 1.3: return "UP"
    if low > high * 1.3: return "DOWN"
    return "?"

def analyze_market(slug, real_raw, bot_data, configs, cumulative):
    """Tek market için DETAYLI analiz"""
    findings = []
    market_start = int(slug.split("-")[-1])
    market_end = market_start + 300
    real_distinct = group_real_distinct(real_raw)
    
    findings.append("")
    findings.append(C.BOLD + C.CY + "═"*82 + C.END)
    findings.append(C.BOLD + C.CY + f"  MARKET: {slug}  (T+0..T+300)" + C.END)
    findings.append(C.BOLD + C.CY + "═"*82 + C.END)
    
    # ─── 1) ÖZET TABLO ───
    if real_raw:
        rt = sum(t["usd"] for t in real_raw)
        rsh = sum(t["sh"] for t in real_raw)
        n_taker = sum(1 for t in real_distinct if t["kind"]=="TAKER")
        n_maker = sum(1 for t in real_distinct if t["kind"].startswith("MAKER"))
        rdir = detect_direction(real_distinct)
        rfirst = market_end - real_distinct[0]["ts"] if real_distinct else 0
    else:
        rt = rsh = n_taker = n_maker = 0; rdir = "?"; rfirst = 0
    
    findings.append(f"\n  {C.BOLD}┌─ ÖZET TABLO ─{C.END}")
    findings.append(f"  │ {'Kim':<14} {'#emir':>6} {'Total$':>9} {'Shares':>8} "
                   f"{'avg_sum':>8} {'maxShot':>8} {'maxP':>5} {'UP/DN':>6} {'dir':>4}")
    findings.append(f"  │ {'─'*14} {'─'*6} {'─'*9} {'─'*8} {'─'*8} {'─'*8} {'─'*5} {'─'*6} {'─'*4}")
    findings.append(f"  │ {C.M+'REAL'+C.END:<22} {len(real_distinct):>6} ${rt:>7.0f} {rsh:>7.0f}sh "
                   f"{'-':>8} {'-':>8} {'-':>5} {'-':>6} {rdir:>4}")
    
    bot_metrics = {}
    for bid in COMPARE_BOTS:
        bt = bot_data.get(bid, [])
        if not bt:
            findings.append(f"  │ {f'Bot {bid}':<14} {'(no trades)':<60}")
            continue
        ut = [t for t in bt if t["outcome"]=="UP"]
        dt = [t for t in bt if t["outcome"]=="DOWN"]
        ush = sum(t["size"] for t in ut); usd = sum(t["usd"] for t in ut)
        dsh = sum(t["size"] for t in dt); dsd = sum(t["usd"] for t in dt)
        avg_u = usd/ush if ush else 0; avg_d = dsd/dsh if dsh else 0
        avg_sum = avg_u + avg_d
        total = usd + dsd
        max_shot = max((t["usd"] for t in bt), default=0)
        max_p = max((t["price"] for t in bt), default=0)
        ratio = ush/dsh if dsh else 99
        our_dir = detect_direction(bt)
        bot_metrics[bid] = {
            "n":len(bt), "total":total, "avg_sum":avg_sum, "max_shot":max_shot,
            "max_p":max_p, "ratio":ratio, "ush":ush, "dsh":dsh, "dir":our_dir,
            "usd":usd, "dsd":dsd
        }
        dir_match = C.G+"✓"+C.END if our_dir == rdir or rdir=="?" else C.R+"✗"+C.END
        findings.append(f"  │ {C.B+'Bot '+str(bid)+C.END+'-'+BOT_NAMES[bid][4:]:<22} {len(bt):>6} "
                       f"${total:>7.0f} {ush+dsh:>7.0f}sh {avg_sum:>7.3f} ${max_shot:>6.0f} "
                       f"{max_p:>5.2f} {ratio:>5.2f}x {our_dir:>3}{dir_match}")
    findings.append(f"  └─")
    
    # ─── 2) UYUM SKORU (Bot 131 vs Real) ───
    if real_distinct and CONTROL_BOT in bot_metrics:
        m = bot_metrics[CONTROL_BOT]
        rt_total = sum(t["usd"] for t in real_distinct)
        r_lw = [t for t in real_distinct if t["price"]>=0.85]
        r_scoop = [t for t in real_distinct if t["price"]<=0.15]
        bt = bot_data[CONTROL_BOT]
        b_lw = sum(1 for t in bt if t["price"]>=0.85)
        b_scoop = sum(1 for t in bt if t["price"]<=0.15)
        r_lw_usd = sum(t["usd"] for t in r_lw)
        b_lw_usd = sum(t["usd"] for t in bt if t["price"]>=0.85)
        
        findings.append(f"\n  {C.BOLD}┌─ UYUM SKORU (Real vs Bot {CONTROL_BOT}) ─{C.END}")
        def uyum_str(real_v, our_v, label, fmt=".0f"):
            if real_v == 0: pct_str = "(div0)"
            else: pct = our_v/real_v*100; pct_str = f"%{pct:.0f}"
            color = ""
            if real_v > 0:
                pct = our_v/real_v*100
                if 80 <= pct <= 130: color = C.G
                elif 50 <= pct < 80 or 130 < pct <= 200: color = C.Y
                else: color = C.R
            r_fmt = f"{real_v:{fmt}}"; o_fmt = f"{our_v:{fmt}}"
            return f"  │ {label:<22} Real {r_fmt:>9} | Bot {o_fmt:>9} | {color}{pct_str}{C.END}"
        findings.append(uyum_str(len(real_distinct), m["n"], "Distinct emir"))
        findings.append(uyum_str(rt_total, m["total"], "Total USDC", fmt=".0f"))
        findings.append(uyum_str(len(r_lw), b_lw, "LW emir #"))
        findings.append(uyum_str(r_lw_usd, b_lw_usd, "LW USDC", fmt=".0f"))
        findings.append(uyum_str(len(r_scoop), b_scoop, "Scoop emir #"))
        findings.append(f"  │ Yön: Real={rdir}, Bot={m['dir']}, "
                       f"{C.G+'EŞLEŞTİ ✓' if m['dir']==rdir or rdir=='?' else C.R+'TERS ✗'}{C.END}")
        findings.append(f"  └─")
    
    # ─── 3) PHASE BREAKDOWN (T-X bantları) ───
    if real_distinct:
        findings.append(f"\n  {C.BOLD}┌─ PHASE DAĞILIMI (T-X bantları) ─{C.END}")
        r_phases, r_pc, r_pu = phase_breakdown(real_distinct, market_end)
        b_phases, b_pc, b_pu = phase_breakdown(bot_data.get(CONTROL_BOT, []), market_end)
        findings.append(f"  │ {'Phase':<14} {'Real#':>6} {'Real$':>8}  | {'Bot131#':>7} {'Bot131$':>9}")
        for ph in r_phases:
            r_n = r_pc.get(ph, 0); r_u = r_pu.get(ph, 0)
            b_n = b_pc.get(ph, 0); b_u = b_pu.get(ph, 0)
            if r_n == 0 and b_n == 0: continue
            findings.append(f"  │ {ph:<14} {r_n:>6} ${r_u:>6.0f}  | {b_n:>7} ${b_u:>7.0f}")
        findings.append(f"  └─")
    
    # ─── 4) PRICE BAND BREAKDOWN ───
    if real_distinct:
        findings.append(f"\n  {C.BOLD}┌─ FİYAT BAND DAĞILIMI ─{C.END}")
        r_bands, r_bc, r_bu = price_band_breakdown(real_distinct)
        b_bc_dict = {}; b_bu_dict = {}
        if CONTROL_BOT in bot_data:
            _, b_bc_dict, b_bu_dict = price_band_breakdown(bot_data[CONTROL_BOT])
        findings.append(f"  │ {'Band':<22} {'Real#':>6} {'Real$':>8}  | {'Bot131#':>7} {'Bot131$':>9}")
        for b in r_bands:
            r_n = r_bc.get(b, 0); r_u = r_bu.get(b, 0)
            bn = b_bc_dict.get(b, 0); bu = b_bu_dict.get(b, 0)
            if r_n == 0 and bn == 0: continue
            findings.append(f"  │ {b:<22} {r_n:>6} ${r_u:>6.0f}  | {bn:>7} ${bu:>7.0f}")
        findings.append(f"  └─")
    
    # ─── 5) ARB_MULT DOĞRULAMA (LW shot başına) ───
    if CONTROL_BOT in bot_data and configs.get(CONTROL_BOT):
        lw_usdc = configs[CONTROL_BOT]["params"]["bonereaper_late_winner_usdc"]
        bt = bot_data[CONTROL_BOT]
        lw_shots = [t for t in bt if t["price"]>=0.95 and t.get("side","").startswith("TAKER")]
        if lw_shots:
            findings.append(f"\n  {C.BOLD}┌─ ARB_MULT DOĞRULAMA (Bot {CONTROL_BOT} TAKER LW shotları, lw_usdc=${lw_usdc:.0f}) ─{C.END}")
            findings.append(f"  │ {'T-X':>5} {'Out':>4} {'Price':>6} {'Size':>6} {'ExpMult':>8} "
                           f"{'ExpSize':>8} {'Sapma%':>8}")
            for t in lw_shots[:8]:
                to_end = market_end - t["ts"]
                em = expected_arb_mult(t["price"], to_end)
                exp_size = lw_usdc * em / t["price"]
                dev = abs(t["size"] - exp_size) / exp_size * 100 if exp_size else 0
                color = C.G if dev < 30 else (C.Y if dev < 100 else C.R)
                findings.append(f"  │ T-{to_end:>3} {t['outcome']:>4} ${t['price']:.3f} "
                               f"{t['size']:>5.0f} {em:>7.2f}x {exp_size:>7.0f} "
                               f"{color}{dev:>6.0f}%{C.END}")
            findings.append(f"  └─")
    
    # ─── 6) TIMELINE (real vs bot 131 ilk 10 emir) ───
    if real_distinct and CONTROL_BOT in bot_data:
        bt = bot_data[CONTROL_BOT]
        findings.append(f"\n  {C.BOLD}┌─ TIMELINE (ilk 10 distinct emir) ─{C.END}")
        findings.append(f"  │ {'#':>3} {'R T-X':>5} {'REAL':<32} | {'O T-X':>5} {'OUR (Bot 131)':<25}")
        for i in range(min(10, max(len(real_distinct), len(bt)))):
            r = real_distinct[i] if i < len(real_distinct) else None
            b = bt[i] if i < len(bt) else None
            r_str = "-"; r_te = "-"
            b_str = "-"; b_te = "-"
            if r:
                r_te_n = market_end - r["ts"]
                r_te = f"T-{r_te_n}"
                r_str = f"${r['price']:.2f} ${r['usd']:.0f} {r['sh']:.0f}sh [{r['kind']}]"
            if b:
                b_te_n = market_end - b["ts"]
                b_te = f"T-{b_te_n}"
                b_str = f"{b['outcome']} ${b['price']:.2f} ${b['usd']:.0f}"
            findings.append(f"  │ {i+1:>3} {r_te:>5} {r_str:<32} | {b_te:>5} {b_str:<25}")
        findings.append(f"  └─")
    
    # ─── 7) BUG CHECKS ───
    is_bug = False
    bug_msg = []
    if CONTROL_BOT in bot_metrics and real_distinct:
        m = bot_metrics[CONTROL_BOT]
        rt_total = sum(t["usd"] for t in real_distinct)
        r_lw_n = len([t for t in real_distinct if t["price"]>=0.85])
        rt_first = market_end - real_distinct[0]["ts"]
        b_first = market_end - bot_data[CONTROL_BOT][0]["ts"] if bot_data[CONTROL_BOT] else 0
        
        # Bug: Yön ters + her iki tarafın hacmi büyük
        if m["dir"] != "?" and rdir != "?" and m["dir"] != rdir and m["total"] > 500 and rt_total > 500:
            bug_msg.append(f"BUG#A WRONG DIR: Real={rdir} Bot={m['dir']} (UP/DN={m['ratio']:.2f})")
            is_bug = True
        # Bug: avg_sum patlama
        normal_bt = [t for t in bot_data[CONTROL_BOT] if t["price"]<0.85]
        if normal_bt:
            n_up = [t for t in normal_bt if t["outcome"]=="UP"]
            n_dn = [t for t in normal_bt if t["outcome"]=="DOWN"]
            n_au = sum(t["usd"] for t in n_up)/sum(t["size"] for t in n_up) if n_up else 0
            n_ad = sum(t["usd"] for t in n_dn)/sum(t["size"] for t in n_dn) if n_dn else 0
            n_sum = n_au + n_ad
            if n_sum > 1.15 and len(normal_bt) > 15:
                bug_msg.append(f"BUG#B NORMAL AVG_SUM: {n_sum:.3f} > 1.15 (LW hariç)")
                is_bug = True
        # Bug: MEGA shot (20x cap = max ~$2020)
        if m["max_shot"] > 5000:
            bug_msg.append(f"BUG#C MEGA SHOT: ${m['max_shot']:.0f} > $5000")
            is_bug = True
        # Bug: LW MISS
        if r_lw_n >= 5 and m["max_p"] < 0.85 and m["total"] > 200:
            bug_msg.append(f"BUG#D LW MISS: Real {r_lw_n} LW, Bot max_p={m['max_p']:.2f}")
            is_bug = True
        # Bug: LATE FIRST
        if rt_first > 250 and b_first < rt_first - 60:
            bug_msg.append(f"BUG#E LATE FIRST: Real T-{rt_first}, Bot T-{b_first}")
            is_bug = True
    
    if bug_msg:
        findings.append("")
        for b in bug_msg:
            findings.append(C.R + C.BOLD + f"  ⚠️  {b}" + C.END)
    
    # CUMULATIVE güncelle
    cumulative["markets"] += 1
    if real_distinct:
        cumulative["real_total_usd"] += sum(t["usd"] for t in real_distinct)
        cumulative["real_total_emir"] += len(real_distinct)
        cumulative["real_lw_emir"] += len([t for t in real_distinct if t["price"]>=0.85])
    for bid, m in bot_metrics.items():
        if bid not in cumulative["bots"]:
            cumulative["bots"][bid] = {"total":0, "n":0, "lw_n":0, "dir_match":0, "dir_total":0}
        cumulative["bots"][bid]["total"] += m["total"]
        cumulative["bots"][bid]["n"] += m["n"]
        cumulative["bots"][bid]["lw_n"] += sum(1 for t in bot_data[bid] if t["price"]>=0.85)
        if rdir != "?" and m["dir"] != "?":
            cumulative["bots"][bid]["dir_total"] += 1
            if rdir == m["dir"]: cumulative["bots"][bid]["dir_match"] += 1
    
    return findings, is_bug

def show_cumulative(cumulative):
    """Tüm marketler boyunca özet"""
    if cumulative["markets"] == 0: return []
    out = []
    out.append("")
    out.append(C.BOLD + C.M + "═"*82 + C.END)
    out.append(C.BOLD + C.M + f"  CUMULATIVE ÖZET ({cumulative['markets']} market)" + C.END)
    out.append(C.BOLD + C.M + "═"*82 + C.END)
    out.append(f"  Real bot total: ${cumulative['real_total_usd']:.0f}, "
              f"{cumulative['real_total_emir']} emir, {cumulative['real_lw_emir']} LW")
    out.append("")
    out.append(f"  {'Bot':<14} {'Total$':>9} {'Vol_Uyum':>9} {'Emir':>6} {'Emir_Uyum':>10} "
              f"{'LW#':>5} {'LW_Uyum':>9} {'YönDoğr':>9}")
    for bid in COMPARE_BOTS:
        b = cumulative["bots"].get(bid, {"total":0,"n":0,"lw_n":0,"dir_match":0,"dir_total":0})
        vol_pct = b["total"]/cumulative["real_total_usd"]*100 if cumulative["real_total_usd"] else 0
        emir_pct = b["n"]/cumulative["real_total_emir"]*100 if cumulative["real_total_emir"] else 0
        lw_pct = b["lw_n"]/cumulative["real_lw_emir"]*100 if cumulative["real_lw_emir"] else 0
        dir_acc = b["dir_match"]/b["dir_total"]*100 if b["dir_total"] else 0
        c1 = C.G if 80<=vol_pct<=130 else (C.Y if 50<=vol_pct<200 else C.R)
        c2 = C.G if 80<=emir_pct<=130 else (C.Y if 50<=emir_pct<200 else C.R)
        c3 = C.G if 80<=lw_pct<=130 else (C.Y if 50<=lw_pct<200 else C.R)
        c4 = C.G if dir_acc>=70 else (C.Y if dir_acc>=50 else C.R)
        out.append(f"  Bot {bid}-{BOT_NAMES[bid][4:]:<10} ${b['total']:>7.0f} "
                  f"{c1}%{vol_pct:>6.0f}{C.END} {b['n']:>6} {c2}%{emir_pct:>7.0f}{C.END} "
                  f"{b['lw_n']:>5} {c3}%{lw_pct:>6.0f}{C.END} {c4}%{dir_acc:>6.0f}{C.END}")
    return out

def get_recent_completed():
    now = int(time.time())
    sql = (f"SELECT DISTINCT slug FROM market_sessions "
           f"WHERE bot_id={CONTROL_BOT} AND end_ts < {now} AND end_ts > {now-1800} "
           f"ORDER BY end_ts DESC;")
    out = ssh_query(sql)
    if out.startswith("ERR"): return []
    return [s.strip() for s in out.split("\n") if s.strip().startswith("btc-updown-5m-")]

def main():
    log("█"*82, C.BOLD)
    log("KAPSAMLI CANLI İZLEME — Real bot vs Bot 131/132/134", C.BOLD + C.CY)
    log("█"*82, C.BOLD)
    log("")
    
    # Bot config'leri
    log("Bot config'leri:")
    configs = {}
    for bid in COMPARE_BOTS:
        c = get_bot_config(bid)
        if c:
            configs[bid] = c
            p = c["params"]
            log(f"  Bot {bid} ({BOT_NAMES[bid]}): order=${c['order_usdc']:.0f} "
                f"lw=${p['bonereaper_late_winner_usdc']:.0f} max={p['bonereaper_lw_max_per_session']} "
                f"imb={p['bonereaper_imbalance_thr']:.0f} avgcap={p['bonereaper_max_avg_sum']:.2f}")
    log("")
    log(f"arb_mult formülü: $0.99+×T>120 = {C.BOLD}20x{C.END} (güncel), $0.97-0.99×T-60..30 = 6.1x")
    log("")
    
    cumulative = {"markets": 0, "real_total_usd": 0, "real_total_emir": 0,
                  "real_lw_emir": 0, "bots": {}}
    seen = set()
    iteration = 0
    
    while True:
        iteration += 1
        log(C.B + f"\n{'─'*82}\n┃ İterasyon #{iteration} @ {datetime.now().strftime('%H:%M:%S')}" + C.END)
        
        completed = get_recent_completed()
        new_markets = [m for m in completed if m not in seen]
        
        if not new_markets:
            log(f"  Yeni biten yok ({len(seen)} izlenmiş, {cumulative['markets']} analiz edildi)")
        else:
            log(C.G + f"  {len(new_markets)} yeni market" + C.END)
            for slug in new_markets:
                seen.add(slug)
                real_raw = parse_real_trades(slug)
                bot_data = {bid: get_bot_trades(slug, bid) for bid in COMPARE_BOTS}
                findings, is_bug = analyze_market(slug, real_raw, bot_data, configs, cumulative)
                for f in findings: log(f)
                
                if is_bug:
                    for ln in show_cumulative(cumulative): log(ln)
                    log("")
                    log(C.R + C.BOLD + "█"*82 + C.END)
                    log(C.R + C.BOLD + "  KRİTİK BUG TESPİT EDİLDİ — DURDU" + C.END)
                    log(C.R + C.BOLD + "█"*82 + C.END)
                    with open(SUMMARY_JSON, "w") as f: json.dump(cumulative, f, indent=2, default=str)
                    sys.exit(1)
            
            # Her market sonrası cumulative özet göster
            for ln in show_cumulative(cumulative): log(ln)
            with open(SUMMARY_JSON, "w") as f: json.dump(cumulative, f, indent=2, default=str)
        
        time.sleep(45)

if __name__ == "__main__":
    main()
