#!/usr/bin/env python3
"""
DERİN canlı izleme — Real bot vs Bot 131 (kontrol)

Her market için TAM detay:
1. Bot strategy parametreleri (config'den)
2. Trade-by-trade timeline (T-X, outcome, price, USDC, gap)
3. Phase analysis (T-300..240, ..., T-10..0) — fiyat dağılımı
4. avg_sum trajektörü (her 30sn'de avg_sum'ın nasıl değiştiği)
5. LW shot karşılaştırması (timing + büyüklük)
6. Loser scoop karşılaştırması
7. Cooldown/burst pattern
8. arb_mult kontrol — her LW shot için beklenen vs gerçek mult

Bug detection:
- LW MISS, WRONG DIR, AVG_SUM, MEGA SHOT, LATE FIRST, SCOOP MISS, VOLUME LOW
- Yeni: COOLDOWN VIOLATION (2 trade < cooldown_ms)
- Yeni: ARB_MULT MISMATCH (bizim bot bandın katsayısını uygulamamış)
"""
import re, json, time, subprocess, sys, os
from datetime import datetime
from collections import defaultdict

TERM_LOG = "/Users/dorukbirinci/.cursor/projects/Users-dorukbirinci-Desktop-baiter-pro/terminals/6.txt"
SSH_KEY = os.path.expanduser("~/Desktop/smlbrnc.pem")
VPS = "ubuntu@79.125.42.234"
DB = "/home/ubuntu/baiter/data/baiter.db"
CONTROL_BOT = 131  # default + en doğru yön
COMPARE_BOTS = [131, 132, 134]  # 133 yön bug'ı var, hariç
BOT_NAMES = {131:"Konservatif", 132:"Agresif", 134:"LOW_IMB"}
REPORT = "/Users/dorukbirinci/Desktop/baiter-pro/scripts/monitor/deep_report.log"

# arb_mult formülü (bonereaper.rs'den)
def expected_arb_mult(w_ask, to_end):
    if w_ask >= 0.99:
        if to_end <= 10: return 1.7
        elif to_end <= 30: return 5.7
        elif to_end <= 60: return 5.5
        elif to_end <= 120: return 11.5
        else: return 13.0
    elif w_ask >= 0.97:
        if to_end <= 10: return 1.0
        elif to_end <= 30: return 3.7
        elif to_end <= 60: return 6.1
        elif to_end <= 120: return 4.4
        else: return 9.0
    elif w_ask >= 0.95:
        return 4.0 if to_end <= 60 else 2.0
    return 1.0

def log(msg):
    line = f"[{datetime.now().strftime('%H:%M:%S')}] {msg}"
    print(line, flush=True)
    with open(REPORT, "a") as f: f.write(line + "\n")

def ssh_query(sql):
    cmd = ["ssh", "-i", SSH_KEY, "-o", "StrictHostKeyChecking=no", VPS,
           f"sqlite3 -separator '|' {DB} \"{sql}\""]
    try:
        return subprocess.check_output(cmd, timeout=20).decode().strip()
    except Exception as e:
        return f"ERR: {e}"

def get_bot_config(bot_id):
    """Bot config + strategy_params"""
    sql = (f"SELECT order_usdc, min_price, max_price, cooldown_threshold, "
           f"start_offset, strategy_params FROM bots WHERE id={bot_id};")
    out = ssh_query(sql)
    if out.startswith("ERR") or not out: return None
    parts = out.split("|", 5)
    if len(parts) < 6: return None
    try:
        params = json.loads(parts[5])
    except:
        params = {}
    # Defaults from config.rs
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

def parse_real_trades_for_market(slug):
    """Terminal'den tek market trade'leri (zaman sıralı).
    Her kayıt: type=OrderFilled (MAKER fill) veya OrdersMatched (TAKER atomic)"""
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

def group_real_distinct_orders(trades, time_window=2):
    """Real bot trade'lerini DISTINCT EMRİE göre grupla:
    - OrdersMatched: her biri tek emir
    - OrderFilled: aynı price + ±time_window saniye → tek maker emri"""
    if not trades: return []
    takers = [t for t in trades if t["type"] == "OrdersMatched"]
    makers = sorted([t for t in trades if t["type"] == "OrderFilled"],
                    key=lambda x: (round(x["price"],3), x["ts"]))
    # Maker grupla
    maker_groups = []
    if makers:
        current = [makers[0]]
        for m in makers[1:]:
            if (abs(m["price"] - current[-1]["price"]) < 0.005 and
                abs(m["ts"] - current[-1]["ts"]) <= time_window):
                current.append(m)
            else:
                maker_groups.append(current)
                current = [m]
        maker_groups.append(current)
    # Her grubu tek emir olarak dön (toplam shares + usd)
    distinct = []
    for t in takers:
        distinct.append({"ts": t["ts"], "price": t["price"],
                        "usd": t["usd"], "sh": t["sh"], "kind": "TAKER"})
    for grp in maker_groups:
        distinct.append({
            "ts": min(t["ts"] for t in grp),
            "price": grp[0]["price"],
            "usd": sum(t["usd"] for t in grp),
            "sh": sum(t["sh"] for t in grp),
            "kind": f"MAKER({len(grp)}fill)",
        })
    distinct.sort(key=lambda x: x["ts"])
    return distinct

def get_bot_trades(slug, bot_id):
    """Bot'un tüm trade'leri (zaman sıralı).
    Doğru gruplama:
    - TAKER kayıtları: her biri AYRI emir (multi-tick race condition mümkün)
    - MAKER kayıtları: aynı (ts ±2sn, outcome, price) → tek emir + çoklu fill"""
    sql = (f"SELECT outcome, size, price, ts_ms, trader_side FROM trades "
           f"WHERE bot_id={bot_id} AND market_session_id IN "
           f"(SELECT id FROM market_sessions WHERE bot_id={bot_id} AND slug='{slug}') "
           f"ORDER BY ts_ms;")
    out = ssh_query(sql)
    if out.startswith("ERR"): return []
    raw = []
    for ln in out.split("\n"):
        if not ln.strip(): continue
        parts = ln.split("|")
        if len(parts) < 5: continue
        raw.append({"outcome": parts[0], "size": float(parts[1]),
                   "price": float(parts[2]), "ts_ms": int(parts[3]),
                   "side": parts[4]})
    # TAKER: her kayıt ayrı, MAKER: grupla
    out_list = []
    maker_buf = []
    for t in raw:
        if t["side"] == "TAKER":
            out_list.append({"outcome": t["outcome"], "size": t["size"],
                            "price": t["price"], "ts": t["ts_ms"]//1000,
                            "usd": t["size"]*t["price"], "ts_ms": t["ts_ms"],
                            "side": "TAKER"})
        else:  # MAKER
            maker_buf.append(t)
    # Maker grupla (ts ±2sn + outcome + price)
    maker_buf.sort(key=lambda x: (x["outcome"], round(x["price"],4), x["ts_ms"]))
    if maker_buf:
        current = [maker_buf[0]]
        for m in maker_buf[1:]:
            if (m["outcome"] == current[-1]["outcome"] and
                abs(m["price"] - current[-1]["price"]) < 0.001 and
                abs(m["ts_ms"] - current[-1]["ts_ms"]) <= 2000):
                current.append(m)
            else:
                # flush
                size = sum(t["size"] for t in current)
                ts_ms = min(t["ts_ms"] for t in current)
                out_list.append({"outcome": current[0]["outcome"], "size": size,
                                "price": current[0]["price"], "ts": ts_ms//1000,
                                "usd": size*current[0]["price"], "ts_ms": ts_ms,
                                "side": f"MAKER({len(current)}fill)"})
                current = [m]
        if current:
            size = sum(t["size"] for t in current)
            ts_ms = min(t["ts_ms"] for t in current)
            out_list.append({"outcome": current[0]["outcome"], "size": size,
                            "price": current[0]["price"], "ts": ts_ms//1000,
                            "usd": size*current[0]["price"], "ts_ms": ts_ms,
                            "side": f"MAKER({len(current)}fill)"})
    out_list.sort(key=lambda x: x["ts_ms"])
    return out_list

def detailed_market_analysis(slug, real_trades, bot_trades_dict, bot_configs):
    """Tek market için DETAY analiz"""
    findings = []
    market_start = int(slug.split("-")[-1])
    market_end = market_start + 300
    
    findings.append("\n" + "═"*78)
    findings.append(f"   MARKET: {slug}  ({market_start} → {market_end})")
    findings.append("═"*78)
    
    # ─── REAL BOT ÖZET (TAKER + MAKER ayrımıyla) ───
    real_distinct = group_real_distinct_orders(real_trades) if real_trades else []
    if real_trades:
        rt = sum(t["usd"] for t in real_trades)
        rsh = sum(t["sh"] for t in real_trades)
        rmin_p = min(t["price"] for t in real_trades)
        rmax_p = max(t["price"] for t in real_trades)
        rfirst = market_end - real_trades[0]["ts"]
        rlast = market_end - real_trades[-1]["ts"]
        rlw = [t for t in real_distinct if t["price"]>=0.85]
        rscoop = [t for t in real_distinct if t["price"]<=0.15]
        n_taker = sum(1 for t in real_distinct if t["kind"]=="TAKER")
        n_maker = sum(1 for t in real_distinct if t["kind"].startswith("MAKER"))
        # Yön tahmini (USD ağırlıklı)
        rh = sum(t["usd"] for t in real_distinct if t["price"]>0.55)
        rl = sum(t["usd"] for t in real_distinct if t["price"]<0.45)
        rdir = "UP" if rh > rl*1.3 else ("DOWN" if rl > rh*1.3 else "?")
        findings.append(f"\n REAL BOT: {len(real_trades)} fill → {len(real_distinct)} distinct emir "
                       f"(TAKER {n_taker} + MAKER {n_maker})")
        findings.append(f"  Total: ${rt:.0f}, {rsh:.0f} sh, fiyat ${rmin_p:.2f}-${rmax_p:.2f}")
        findings.append(f"  Window: T-{rfirst} → T-{rlast}, dir={rdir}")
        findings.append(f"  LW emir: {len(rlw)}, ${sum(t['usd'] for t in rlw):.0f}")
        findings.append(f"  Scoop emir: {len(rscoop)}, ${sum(t['usd'] for t in rscoop):.0f}")
    else:
        findings.append(f"\n REAL BOT: trade verisi yok")
    
    # ─── BİZİM BOTLAR (DISTINCT shot bazlı) ───
    findings.append("")
    for bid in COMPARE_BOTS:
        bt = bot_trades_dict.get(bid, [])  # zaten distinct shot (get_bot_trades içinde gruplanmış)
        cfg = bot_configs.get(bid, {})
        params = cfg.get("params", {})
        
        if not bt:
            findings.append(f" Bot {bid} ({BOT_NAMES[bid]}): trade yok")
            continue
        
        ut = [t for t in bt if t["outcome"]=="UP"]
        dt = [t for t in bt if t["outcome"]=="DOWN"]
        ush = sum(t["size"] for t in ut); usd = sum(t["usd"] for t in ut)
        dsh = sum(t["size"] for t in dt); dsd = sum(t["usd"] for t in dt)
        avg_u = usd/ush if ush else 0
        avg_d = dsd/dsh if dsh else 0
        avg_sum = avg_u + avg_d
        total = usd + dsd
        max_shot = max((t["usd"] for t in bt), default=0)
        max_p = max((t["price"] for t in bt), default=0)
        ratio = ush/dsh if dsh else 99
        first_to_end = market_end - bt[0]["ts"]
        last_to_end = market_end - bt[-1]["ts"]
        
        lw_count = sum(1 for t in bt if t["price"]>=0.85)
        scoop_count = sum(1 for t in bt if t["price"]<=0.15)
        
        findings.append(f" Bot {bid} ({BOT_NAMES[bid]}):")
        findings.append(f"   PARAMS: order=${cfg.get('order_usdc',0):.0f} "
                       f"lw_usdc=${params['bonereaper_late_winner_usdc']:.0f} "
                       f"lw_max={params['bonereaper_lw_max_per_session']} "
                       f"imb_thr={params['bonereaper_imbalance_thr']:.0f} "
                       f"first_spread={params['bonereaper_first_spread_min']:.2f}")
        findings.append(f"   Sizes: ls=${params['bonereaper_size_longshot_usdc']:.0f} "
                       f"mid=${params['bonereaper_size_mid_usdc']:.0f} "
                       f"hi=${params['bonereaper_size_high_usdc']:.0f} "
                       f"scoop=${params['bonereaper_loser_scalp_usdc']:.0f}@{params['bonereaper_loser_scalp_max_price']:.2f} "
                       f"cd={params['bonereaper_buy_cooldown_ms']}ms")
        findings.append(f"   DISTINCT SHOTS: {len(bt)}, ${total:.0f}, "
                       f"UP {ush:.0f}sh@${avg_u:.3f}={ratio:.2f}x DN {dsh:.0f}sh@${avg_d:.3f}")
        findings.append(f"   avg_sum={avg_sum:.3f}, max_shot=${max_shot:.0f}, max_p={max_p:.2f}, "
                       f"LW#{lw_count} Scoop#{scoop_count}")
        findings.append(f"   Window: T-{first_to_end} → T-{last_to_end}")
        
        # UYUM SKORU (kontrol botu için)
        if bid == CONTROL_BOT and real_trades:
            r_total = sum(t["usd"] for t in real_trades)
            r_distinct_n = len(real_distinct)
            r_lw = len([t for t in real_distinct if t["price"]>=0.85])
            r_scoop = len([t for t in real_distinct if t["price"]<=0.15])
            findings.append(f"   ━━ UYUM (Real vs Bot {bid}) ━━")
            findings.append(f"     Distinct emir: {r_distinct_n} vs {len(bt)} ({len(bt)/r_distinct_n*100 if r_distinct_n else 0:.0f}%)")
            findings.append(f"     Total USDC: ${r_total:.0f} vs ${total:.0f} ({total/r_total*100 if r_total else 0:.0f}%)")
            findings.append(f"     LW shot: {r_lw} vs {lw_count} ({lw_count/r_lw*100 if r_lw else 0:.0f}%)")
            findings.append(f"     Scoop shot: {r_scoop} vs {scoop_count} ({scoop_count/r_scoop*100 if r_scoop else 0:.0f}%)")
        
        # ─── DETAYLI BUG CHECKS (kontrol botu için, DISTINCT emir bazlı) ───
        if bid != CONTROL_BOT: continue
        if not real_distinct: continue
        
        rt_total = sum(t["usd"] for t in real_distinct)
        rt_lw_n = len(rlw)
        rt_scoop_n = len(rscoop)
        rt_dir = rdir
        rt_first = market_end - real_distinct[0]["ts"]
        
        # BUG#1: LW MISS
        if rt_lw_n >= 5 and max_p < 0.85 and total > 200:
            findings.append(f"   ⚠️  BUG#1 LW MISS: Real {rt_lw_n} LW, bot max_p={max_p:.2f}")
            return findings, True
        
        # BUG#2: WRONG DIRECTION
        if total > 200 and rt_total > 200 and rt_dir != "?":
            our_dir = "UP" if ratio > 2.5 else ("DOWN" if ratio < 0.4 else "?")
            if our_dir != "?" and rt_dir != our_dir:
                findings.append(f"   ⚠️  BUG#2 WRONG DIR: Real={rt_dir}, bot={our_dir} (ratio={ratio:.2f})")
                return findings, True
        
        # BUG#3: AVG_SUM > 1.30 (LW exempt olduğu için 1.05 cap aşılması normal)
        # Sadece NORMAL trade'ler için cap kontrol et
        normal_trades = [t for t in bt if t["price"] < 0.85]
        if normal_trades:
            n_up = [t for t in normal_trades if t["outcome"]=="UP"]
            n_dn = [t for t in normal_trades if t["outcome"]=="DOWN"]
            n_avg_u = sum(t["usd"] for t in n_up)/sum(t["size"] for t in n_up) if n_up else 0
            n_avg_d = sum(t["usd"] for t in n_dn)/sum(t["size"] for t in n_dn) if n_dn else 0
            normal_sum = n_avg_u + n_avg_d
            if normal_sum > 1.10 and len(normal_trades) > 15:
                findings.append(f"   ⚠️  BUG#3 NORMAL AVG_SUM: {normal_sum:.3f} > 1.10 "
                              f"(LW hariç UP=${n_avg_u:.3f} + DN=${n_avg_d:.3f})")
                return findings, True
        
        # BUG#4: MEGA SHOT > $5000
        if max_shot > 5000:
            findings.append(f"   ⚠️  BUG#4 MEGA SHOT: ${max_shot:.0f} > $5000")
            return findings, True
        
        # BUG#5: LATE FIRST (60s+ gecikme)
        if rt_first > 250 and first_to_end < rt_first - 60:
            findings.append(f"   ⚠️  BUG#5 LATE FIRST: Real T-{rt_first}, bot T-{first_to_end} ({rt_first-first_to_end}s gecikme)")
            return findings, True
        
        # BUG#6: SCOOP MISS
        if rt_scoop_n >= 10 and scoop_count == 0:
            findings.append(f"   ⚠️  BUG#6 SCOOP MISS: Real {rt_scoop_n} scoop, bot 0")
            return findings, True
        
        # BUG#7: VOLUME LOW (< %10 of real)
        if total < rt_total * 0.10 and rt_total > 500:
            findings.append(f"   ⚠️  BUG#7 VOLUME LOW: ${total:.0f} < %10 of ${rt_total:.0f}")
            return findings, True
        
        # BUG#8: COOLDOWN VIOLATION (distinct shot bazlı, gap_ms ile)
        # bt zaten gruplanmış (maker fill'ler birleşmiş), sadece distinct shotlar arası gap'e bak
        cd_ms = params['bonereaper_buy_cooldown_ms']
        cd_violations = 0
        for i in range(1, len(bt)):
            gap_ms = bt[i]["ts_ms"] - bt[i-1]["ts_ms"]
            # Aynı saniyede ardışık çoklu shot = LW burst, normal değil ama bug değil
            if gap_ms < cd_ms * 0.5 and gap_ms > 0:
                # Sadece normal trade'ler arası (LW exempt — LW kendi cooldown'una sahip)
                if bt[i]["price"] < 0.85 and bt[i-1]["price"] < 0.85:
                    cd_violations += 1
        if cd_violations >= 5:
            findings.append(f"   ⚠️  BUG#8 COOLDOWN VIOLATION: {cd_violations} distinct shot "
                          f"arası gap < cd={cd_ms}ms/2 (LW hariç)")
            return findings, True
        
        # BUG#9: ARB_MULT MISMATCH (sadece TAKER tek emirler için kontrol)
        # MAKER fill grupları toplam size taşır, mult kontrolüne uygun değil
        for t in bt:
            if t["price"] < 0.95: continue
            if not t.get("side", "").startswith("TAKER"): continue
            to_end = market_end - t["ts"]
            expected_mult = expected_arb_mult(t["price"], to_end)
            base_lw = params['bonereaper_late_winner_usdc']
            expected_size = base_lw * expected_mult / t["price"]
            actual_size = t["size"]
            if expected_size > 0:
                deviation = abs(actual_size - expected_size) / expected_size
                if deviation > 0.5 and actual_size > 100:
                    findings.append(f"   ⚠️  BUG#9 ARB_MULT MISMATCH: T-{to_end}s {t['outcome']} "
                                  f"@${t['price']:.2f} TAKER expected mult={expected_mult}x size={expected_size:.0f}, "
                                  f"actual size={actual_size:.0f}")
                    return findings, True
    
    # ─── KARŞILAŞTIRMA TIMELINE (DISTINCT emirler) ───
    if real_distinct and bot_trades_dict.get(CONTROL_BOT):
        bt = bot_trades_dict[CONTROL_BOT]
        findings.append(f"\n  TIMELINE (Real distinct emir vs Bot {CONTROL_BOT} distinct shot, ilk 12):")
        findings.append(f"  {'idx':>3} {'R T-X':>5} {'REAL':<28} | {'O T-X':>5} {'OUR':<25}")
        for i in range(min(12, max(len(real_distinct), len(bt)))):
            r_str = "-"; r_te_str = "-"
            b_str = "-"; b_te_str = "-"
            if i < len(real_distinct):
                r = real_distinct[i]
                r_te = market_end - r["ts"]
                r_te_str = f"T-{r_te}"
                r_str = f"${r['price']:.2f} ${r['usd']:.0f} {r['sh']:.0f}sh [{r['kind']}]"
            if i < len(bt):
                b = bt[i]
                b_te = market_end - b["ts"]
                b_te_str = f"T-{b_te}"
                b_str = f"{b['outcome']} ${b['price']:.2f} ${b['usd']:.0f}"
            findings.append(f"  {i+1:>3} {r_te_str:>5} {r_str:<28} | {b_te_str:>5} {b_str:<25}")
    
    return findings, False

def get_recent_completed():
    now = int(time.time())
    sql = (f"SELECT DISTINCT slug FROM market_sessions "
           f"WHERE bot_id={CONTROL_BOT} AND end_ts < {now} AND end_ts > {now-1800} "
           f"ORDER BY end_ts DESC;")
    out = ssh_query(sql)
    if out.startswith("ERR"): return []
    return [s.strip() for s in out.split("\n") if s.strip().startswith("btc-updown-5m-")]

def main():
    log("█"*78)
    log("DERİN İZLEME — Real bot vs Bot 131 (kontrol)")
    log("█"*78)
    
    # Bot config'leri çek
    log("Bot config'leri yükleniyor...")
    bot_configs = {}
    for bid in COMPARE_BOTS:
        cfg = get_bot_config(bid)
        if cfg:
            bot_configs[bid] = cfg
            p = cfg["params"]
            log(f"  Bot {bid} ({BOT_NAMES[bid]}): order=${cfg['order_usdc']:.0f} "
                f"lw=${p['bonereaper_late_winner_usdc']:.0f} "
                f"max={p['bonereaper_lw_max_per_session']} "
                f"imb={p['bonereaper_imbalance_thr']:.0f} "
                f"avgcap={p['bonereaper_max_avg_sum']:.2f}")
    log("")
    log(f"Bug kriterleri (9): LW MISS, WRONG DIR, AVG_SUM>1.25, MEGA SHOT>$5k,")
    log(f"                    LATE FIRST(60s+), SCOOP MISS, VOLUME LOW(<%10),")
    log(f"                    COOLDOWN VIOLATION, ARB_MULT MISMATCH(>%50)")
    log("")
    
    seen = set()
    iteration = 0
    
    while True:
        iteration += 1
        log(f"\n{'─'*78}\n┃ İterasyon #{iteration} @ {datetime.now().strftime('%H:%M:%S')}")
        
        completed = get_recent_completed()
        new_markets = [m for m in completed if m not in seen]
        
        if not new_markets:
            log(f"  Yeni biten yok ({len(seen)} izlenmiş)")
        else:
            log(f"  {len(new_markets)} yeni market: {', '.join(new_markets)}")
            
            for slug in new_markets:
                seen.add(slug)
                
                real_trades = parse_real_trades_for_market(slug)
                bot_trades = {bid: get_bot_trades(slug, bid) for bid in COMPARE_BOTS}
                
                findings, is_bug = detailed_market_analysis(slug, real_trades, bot_trades, bot_configs)
                for f in findings: log(f)
                
                if is_bug:
                    log("\n" + "█"*78)
                    log("KRİTİK BUG TESPİT EDİLDİ — DURDU")
                    log("█"*78)
                    sys.exit(1)
        
        time.sleep(45)  # 45sn'de bir kontrol

if __name__ == "__main__":
    main()
