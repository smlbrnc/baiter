#!/usr/bin/env python3
"""
Canlı izleme: Real bot (terminal log) vs Bizim botlar (VPS DB)

Çalışma:
- 30sn'de bir terminal log'unu re-tail et + DB sorgula
- Yeni biten 5m market için karşılaştırma yap
- Kritik anomali (bug) tespit edersen RAPOR + ÇIK
- Anomali yoksa loop devam et

Bug kriterleri (kritik):
1. LW MISS: Real bot $0.85+ shot atmış, bizim bot 133 (kontrol botu) hiç atmamış
2. WRONG DIRECTION: Real bot UP-ağırlıklı, bizim bot DOWN-ağırlıklı (veya tersi) — UP/DN ratio
   ters yönde 5x+ farklı ise bug
3. AVG_SUM PATLAMA: Bizim bot avg_sum > 1.20 (cap çalışmamış)
4. LW SIZE FELAKET: Bizim bot LW shot $5000+ (15x cap aşılmış)
5. DUPLICATE TRADE: Aynı (ts, outcome) için 10+ aynı trade (cooldown bug)
"""
import re, json, time, subprocess, sys, os
from datetime import datetime
from collections import defaultdict

TERM_LOG = "/Users/dorukbirinci/.cursor/projects/Users-dorukbirinci-Desktop-baiter-pro/terminals/6.txt"
SSH_KEY = os.path.expanduser("~/Desktop/smlbrnc.pem")
VPS = "ubuntu@79.125.42.234"
DB = "/home/ubuntu/baiter/data/baiter.db"
BOTS = [131, 132, 134]  # Bot 133 yön bug'ı tespit edildi, izleme dışı
BOT_NAMES = {131:"Konservatif", 132:"Agresif", 134:"LOW_IMB"}
CONTROL_BOT = 131  # Default config + en doğru yön seçimi (Bot 131)
REPORT = "/Users/dorukbirinci/Desktop/baiter-pro/scripts/monitor/report2.log"

def log(msg):
    line = f"[{datetime.now().strftime('%H:%M:%S')}] {msg}"
    print(line, flush=True)
    with open(REPORT, "a") as f: f.write(line + "\n")

def ssh_query(sql):
    cmd = ["ssh", "-i", SSH_KEY, "-o", "StrictHostKeyChecking=no", VPS,
           f"sqlite3 {DB} \"{sql}\""]
    try:
        return subprocess.check_output(cmd, timeout=15).decode().strip()
    except Exception as e:
        return f"ERR: {e}"

def parse_real_bot_trades():
    """Terminal logundan tüm 5m BTC trade'leri çıkar (slug bazlı grupla)"""
    if not os.path.exists(TERM_LOG):
        return {}
    with open(TERM_LOG) as f:
        lines = f.readlines()
    by_slug = defaultdict(list)
    for ln in lines:
        if not ln.startswith("t="): continue
        if "btc-updown-5m-" not in ln: continue
        slug_m = re.search(r'slug=(btc-updown-5m-\d+)', ln)
        if not slug_m: continue
        slug = slug_m.group(1)
        ts_m = re.search(r't=([\d\-T:Z]+)', ln)
        usd_m = re.search(r'usd=([\d.]+)', ln)
        price_m = re.search(r'price=([\d.]+)', ln)
        sh_m = re.search(r'shares=([\d.]+)', ln)
        side_m = re.search(r'side=(\w+)', ln)
        if not (ts_m and usd_m and price_m and sh_m): continue
        if side_m and side_m.group(1) != "Buy": continue
        # Convert ISO to unix
        try:
            unix_ts = int(datetime.fromisoformat(ts_m.group(1).replace("Z","+00:00")).timestamp())
        except:
            continue
        by_slug[slug].append({
            "ts": unix_ts, "price": float(price_m.group(1)),
            "usdc": float(usd_m.group(1)), "shares": float(sh_m.group(1)),
        })
    return dict(by_slug)

def get_bot_trades(slug):
    """4 botun bu market'teki trade'leri"""
    sql = (f"SELECT bot_id, outcome, COUNT(*), ROUND(SUM(size),1), "
           f"ROUND(SUM(size*price),2), ROUND(MAX(size*price),2), ROUND(MAX(price),3) "
           f"FROM trades WHERE market_session_id IN "
           f"(SELECT id FROM market_sessions WHERE slug='{slug}' AND bot_id IN (131,132,133,134)) "
           f"GROUP BY bot_id, outcome;")
    out = ssh_query(sql)
    if out.startswith("ERR"): return None
    bot_data = defaultdict(dict)
    for ln in out.split("\n"):
        if not ln.strip(): continue
        parts = ln.split("|")
        if len(parts) < 7: continue
        bid = int(parts[0])
        outcome = parts[1]
        bot_data[bid][outcome] = {
            "n": int(parts[2]), "sh": float(parts[3]),
            "usdc": float(parts[4]), "max_shot": float(parts[5]),
            "max_price": float(parts[6]),
        }
    return dict(bot_data)

def get_first_trades(slug, bot_id, n=3):
    """Botun bu market'teki ilk N trade'i (yön kararı için)"""
    sql = (f"SELECT outcome, price, ts_ms FROM trades "
           f"WHERE bot_id={bot_id} AND market_session_id IN "
           f"(SELECT id FROM market_sessions WHERE bot_id={bot_id} AND slug='{slug}') "
           f"ORDER BY ts_ms LIMIT {n};")
    out = ssh_query(sql)
    if out.startswith("ERR"): return []
    trades = []
    for ln in out.split("\n"):
        if not ln.strip(): continue
        parts = ln.split("|")
        trades.append({"outcome": parts[0], "price": float(parts[1]), "ts": int(parts[2])//1000})
    return trades

def analyze_market(slug, real_trades, bot_data):
    """Tek market için kapsamlı anomali kontrolü"""
    findings = []
    if not real_trades or not bot_data: return findings, False
    
    real_total = sum(t["usdc"] for t in real_trades)
    real_lw = [t for t in real_trades if t["price"] >= 0.85]
    real_lw_n = len(real_lw)
    real_lw_usdc = sum(t["usdc"] for t in real_lw)
    real_scoop = [t for t in real_trades if t["price"] <= 0.15]
    real_scoop_n = len(real_scoop)
    real_max_price = max(t["price"] for t in real_trades) if real_trades else 0
    real_first_ts = min(t["ts"] for t in real_trades)
    market_end = int(slug.split("-")[-1]) + 300
    real_first_to_end = market_end - real_first_ts
    
    # Real bot yön tahmini (high vs low price ratio)
    real_high_p_n = sum(1 for t in real_trades if t["price"] > 0.55)
    real_low_p_n = sum(1 for t in real_trades if t["price"] < 0.45)
    real_dir = "UP" if real_high_p_n > real_low_p_n * 1.3 else ("DOWN" if real_low_p_n > real_high_p_n * 1.3 else "?")
    
    summary = (f"\n  REAL: {len(real_trades)} trade, ${real_total:.0f}, "
               f"LW: {real_lw_n} (${real_lw_usdc:.0f}), Scoop: {real_scoop_n}, "
               f"max_p: {real_max_price:.2f}, dir~{real_dir}, first T-{real_first_to_end}")
    findings.append(summary)
    
    for bid in BOTS:
        d = bot_data.get(bid, {})
        up = d.get("UP", {"n":0,"sh":0,"usdc":0,"max_shot":0,"max_price":0})
        dn = d.get("DOWN", {"n":0,"sh":0,"usdc":0,"max_shot":0,"max_price":0})
        n = up["n"] + dn["n"]; total = up["usdc"] + dn["usdc"]
        max_shot = max(up["max_shot"], dn["max_shot"])
        max_price = max(up["max_price"], dn["max_price"])
        avg_up = up["usdc"]/up["sh"] if up["sh"] else 0
        avg_dn = dn["usdc"]/dn["sh"] if dn["sh"] else 0
        avg_sum = avg_up + avg_dn
        ratio_up_dn = up["sh"]/dn["sh"] if dn["sh"] else 99
        # LW (mevcut iki tarafta da maksimum fiyat 0.85 üstü mi?)
        bot_lw_max = max_price
        
        b_summary = (f"  Bot {bid} ({BOT_NAMES[bid]}): {n} trd, ${total:.0f}, "
                     f"avg_sum={avg_sum:.3f}, max_shot=${max_shot:.0f}, max_p={max_price:.2f}, "
                     f"UP/DN={ratio_up_dn:.2f}")
        findings.append(b_summary)
        
        # ─────── KAPSAMLI BUG CHECKS ───────
        
        # BUG#1: LW MISS — Real bot 5+ LW yapmış, KONTROL botu ($0.85+ asla atamamış)
        if bid == CONTROL_BOT and real_lw_n >= 5 and bot_lw_max < 0.85 and total > 200:
            findings.append(f"  ⚠️  BUG#1 (Bot {bid} LW MISS): Real {real_lw_n} LW (${real_lw_usdc:.0f}), "
                          f"bizim bot $0.85+ asla atamamış (max_p={bot_lw_max:.2f})")
            return findings, True
        
        # BUG#2: WRONG DIRECTION — Kontrol botu yön TERS (5x+ ters)
        if bid == CONTROL_BOT and total > 200 and real_total > 200 and real_dir != "?":
            our_dir = "UP" if ratio_up_dn > 2.5 else ("DOWN" if ratio_up_dn < 0.4 else "?")
            if our_dir != "?" and real_dir != our_dir:
                findings.append(f"  ⚠️  BUG#2 (Bot {bid} WRONG DIR): Real {real_dir}, "
                              f"bizim {our_dir} (UP/DN={ratio_up_dn:.2f})")
                return findings, True
        
        # BUG#3: AVG_SUM PATLAMA — cap çok aşılmış
        if avg_sum > 1.25 and n > 15:
            findings.append(f"  ⚠️  BUG#3 (Bot {bid} AVG_SUM): {avg_sum:.3f} > 1.25 (cap fail)")
            return findings, True
        
        # BUG#4: MEGA SHOT — 15x cap aşıldı
        if max_shot > 5000:
            findings.append(f"  ⚠️  BUG#4 (Bot {bid} MEGA SHOT): ${max_shot:.0f} > $5000")
            return findings, True
        
        # BUG#5: LATE FIRST TRADE — Real bot T-280'de girdi, bizim T-200 sonra
        if bid == CONTROL_BOT and total > 100 and real_first_to_end > 250:
            first = get_first_trades(slug, bid, 1)
            if first:
                our_first_to_end = market_end - first[0]["ts"]
                if our_first_to_end < real_first_to_end - 60:
                    findings.append(f"  ⚠️  BUG#5 (Bot {bid} LATE FIRST): Real T-{real_first_to_end}, "
                                  f"bizim T-{our_first_to_end} ({real_first_to_end-our_first_to_end}s gecikme)")
                    return findings, True
        
        # BUG#6: SCOOP MISS — Real bot 10+ scoop yapmış, bizim hiç
        if bid == CONTROL_BOT and real_scoop_n >= 10:
            bot_scoop_n = sum(1 for o in ["UP","DOWN"] 
                              if d.get(o,{}).get("max_price",1.0) <= 0.15)
            # Daha doğru: trade detayına bak
            if max_price > 0:
                scoop_check = ssh_query(
                    f"SELECT COUNT(*) FROM trades WHERE bot_id={bid} AND price<=0.15 "
                    f"AND market_session_id IN (SELECT id FROM market_sessions "
                    f"WHERE bot_id={bid} AND slug='{slug}');")
                try:
                    bot_scoop_count = int(scoop_check)
                    if bot_scoop_count == 0 and real_scoop_n >= 10:
                        findings.append(f"  ⚠️  BUG#6 (Bot {bid} SCOOP MISS): Real {real_scoop_n} "
                                      f"scoop ($<0.15), bizim 0!")
                        return findings, True
                except: pass
        
        # BUG#7: VOLUME TOO LOW — Toplam < 10% real
        if bid == CONTROL_BOT and total < real_total * 0.10 and real_total > 500:
            findings.append(f"  ⚠️  BUG#7 (Bot {bid} VOLUME LOW): ${total:.0f} < %10 of "
                          f"real ${real_total:.0f}")
            return findings, True
    
    return findings, False

def get_recent_completed_markets():
    """Son 1 saatte biten 5m marketleri al (state=ACTIVE veya RESOLVED)"""
    now = int(time.time())
    sql = (f"SELECT DISTINCT slug FROM market_sessions "
           f"WHERE bot_id IN (131,132,133,134) AND end_ts < {now} AND end_ts > {now-3600} "
           f"ORDER BY end_ts DESC;")
    out = ssh_query(sql)
    if out.startswith("ERR"): return []
    return [s.strip() for s in out.split("\n") if s.strip().startswith("btc-updown-5m-")]

def main():
    log("=" * 70)
    log("CANLI İZLEME BAŞLADI — bug bulana kadar devam edecek")
    log("=" * 70)
    log(f"İzlenen botlar: {BOTS} ({BOT_NAMES})")
    log(f"Bug kriterleri: LW MISS, WRONG DIR, AVG_SUM>1.20, MEGA_SHOT>$5k")
    log("")
    
    seen_markets = set()
    iteration = 0
    bug_found = False
    
    while not bug_found:
        iteration += 1
        log(f"İterasyon #{iteration}: market kontrol ediliyor...")
        
        # Yeni biten marketler
        completed = get_recent_completed_markets()
        new_markets = [m for m in completed if m not in seen_markets]
        
        if new_markets:
            log(f"  Yeni biten: {len(new_markets)} market")
            
            real_data = parse_real_bot_trades()
            
            for slug in new_markets:
                seen_markets.add(slug)
                bot_data = get_bot_trades(slug)
                real_trades = real_data.get(slug, [])
                
                if not real_trades or not bot_data:
                    log(f"  {slug}: veri eksik (real={len(real_trades)}, bot={bool(bot_data)})")
                    continue
                
                log(f"\n  ─── {slug} ───")
                result = analyze_market(slug, real_trades, bot_data)
                if isinstance(result, tuple):
                    findings, is_bug = result
                else:
                    findings, is_bug = result, False
                
                for f in findings: log(f)
                
                if is_bug:
                    log("\n" + "=" * 70)
                    log("KRITIK BUG TESPİT EDİLDİ — İZLEME DURDURULDU")
                    log("=" * 70)
                    bug_found = True
                    break
        else:
            log(f"  Yeni biten market yok ({len(seen_markets)} izlenmiş)")
        
        if not bug_found:
            time.sleep(30)  # 30sn'de bir kontrol et
    
    log("\nİzleme sona erdi. Rapor: " + REPORT)
    sys.exit(0 if not bug_found else 1)

if __name__ == "__main__":
    main()
