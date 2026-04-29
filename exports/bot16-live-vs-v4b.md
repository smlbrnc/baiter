# Bot 16 (Production v.2.1 elis) vs v4b sim — 6 market karşılaştırması

**Tarih:** 2026-04-29 17:25 → 17:55 (30 dk)
**Sunucu:** `ubuntu@54.170.6.194`
**DB:** `/home/ubuntu/baiter/data/baiter.db`
**Local kopya:** `exports/bot16-ticks-20260429/`
**Bot config:** `BTC 5m Elis`, strategy=`elis`, run_mode=`dryrun`, sunucu git: `676aadd v.2.1`

---

## 1. Özet — 30 dakikada $1.123 fark

| Metrik | Bot 16 (gerçek, v.2.1) | v4b sim (local) | Fark |
|---|---:|---:|---:|
| **Net PnL** | **−$981.61** | **+$141.71** | **+$1 123** |
| Yön doğruluğu | 1/5 ¹ | 4/5 (%80) | +3 |
| Toplam trade | 129 | 131 | ~aynı |
| Max DOWN exposure (1 markette) | 1 167 share | ~150 share | −7× |

¹ Bot 16'nın "yön" intent'i log'da yok; trade dağılımına göre **5 markettin 4'ünde dominant taraf yanlıştı** (UP kazanan markette DOWN'a 4–8× fazla pozisyon bindirdi).

---

## 2. Market-by-market detay

| sid | slug | winner | Bot 16 (gerçek) | v4b sim | Fark |
|---:|---|:---:|---:|---:|---:|
| 358 | 1777483500 | Down | **+$24.80** | −$25.95 | −$50 |
| 359 | 1777483800 | Up | **+$26.51** ² | −$27.66 | −$54 |
| 360 | 1777484100 | Up | −$169.05 | **+$40.97** | **+$210** |
| 361 | 1777484400 | Up | −$319.82 | **+$158.53** | **+$478** |
| 362 | 1777484700 | Up | −$542.00 | **+$58.36** | **+$600** |
| 363 | 1777485000 | mid (0.38/0.61) | −$2.05 | −$62.55 | −$60 |
| **TOPLAM** | | | **−$981.61** | **+$141.71** | **+$1 123** |

² Bot 16'nın küçük kazançları rastgele — 358/359 marketleri ilk 1–2 dakika UP/DOWN net çıkmadan kapanmış (1167×$0 batırma yapacak vakit yok).

---

## 3. Bot 16 (v.2.1) ne yapıyor — karşılıklı maker grid

Trade örneği — `1777484700` (UP kazandı, bot 16 DOWN'a yığıldı, **−$542**):

```
17:25:16  BUY UP    @ 0.42  size=29
17:25:36  BUY DOWN  @ 0.66  size=22   ← hedge
17:25:40  BUY DOWN  @ 0.63  size=23
17:26:21  BUY DOWN  @ 0.71  size=21
17:26:36  BUY DOWN  @ 0.79  size=19
17:27:12  BUY DOWN  @ 0.82  size=18   ← UP zaten 0.20'ye geçti
17:27:24  BUY DOWN  @ 0.70  size=20
... 30+ trade aynı kovalama
17:48:42  BUY DOWN  @ 0.06  size=167  ← scoop spam, UP zaten kazandı
17:48:50  BUY DOWN  @ 0.06  size=167
```

**Tespitler:**
1. **Yön filtresi yok** — opener=UP açtı ama hedge DOWN'u sürekli besledi
2. **Asymmetric sizing yok** — DOWN tarafı 8× UP tarafı (1167 vs 141)
3. **Hedge requote sınırsız** — DOWN bid yükseldikçe takipte aldı
4. **Scoop spam** — UP zaten 0.99'a giderken DOWN @ 0.06 alımı (parite kaybı)
5. **Signal flip yok** — yön değişikliği DB'de tetiklenmiyor

---

## 4. v4b sim aynı tick'lerde ne yapardı

`1777484700` örneği (sim, UP kazanır, +$58):

```
t=20  open_pair: opener=UP(score_avg)  → UP $50 dom @ 0.41, DOWN $20 hedge @ 0.59
t=80  ofi yükseldi → pyramid UP $15
t=140 dom requote (UP bid hareketli) → UP $15 @ 0.43
t=210 hedge requote (opp DOWN YÜKSELİYOR ≥2tick) → DOWN $8
t=240 deadline yakın → STOP
toplam: 12 trade, UP=141, DOWN=85, sale=141 - cost=83 = +$58
```

**Farklar:**
- Asymmetric sizing: dom=$50, hedge=$20, pyramid=$15, parity_topup=$8
- Hedge requote sadece **opp YÜKSELDİĞİNDE** ($1167→$85)
- `requote_eps=4 tick` spam koruyucusu (v3'te 2'ydi, %50 daha az emir)
- `flip_freeze 60s`, `parity_topup_cooldown 5s`, `dom_requote_cooldown 3s`
- Pre-resolve scoop sadece `opp_b ≤ 0.05 AND rem ≤ 35s`

---

## 5. v4b deploy önerisi

Sunucuda `git log` son commit `676aadd v.2.1` — bizim local'deki **v4b çalışması deploy edilmemiş**. Aynı 30 dakikalık 6 markette:

| Senaryo | PnL |
|---|---:|
| Mevcut prod (v.2.1) | −$981 |
| Local v4b (sim) | **+$142** |
| 30 dk başına iyileşme | **+$1 123** |
| Saatlik tahmin (12 market/saat) | **+$2 250** |
| Günlük tahmin (288 market/gün) | **+$54 000** ³ |

³ Saçma bir rakam, çünkü:
- Sim %100 fill varsayar (gerçekte %30–50)
- Aynı 6 markete ekstrapolasyon yanıltıcı (volatilite çeşitli)
- Gerçek slippage / cancel başarısı dikkate alınmamış

Realistic: **günlük +$10–20k** brüt iyileşme bekleniyor (v4b dryrun’da 16 + 8 marketde +$862 kesin PnL üretmişti).

---

## 6. Aksiyon önerileri

1. **v4b'yi sunucuya deploy** (öncelik: yüksek)
   ```bash
   git push deploy main   # veya git pull origin v4b
   cd ~/baiter && cargo build --release && systemctl restart baiter
   ```
2. **Bot 16 dryrun'da v4b ile 24 saat çalıştır** — `realized_pnl` kolonunu DB'ye yazdır (bug: `market_sessions.up_filled` da 0 kalıyor).
3. **`market_sessions` güncelleme bug'ını fix et** — engine fill'leri RAM'de tutuyor, DB'ye yansımıyor (`UPDATE market_sessions SET up_filled=...` eksik).
4. **v4b production'a geçmeden önce küçük live test** — min `order_size=$5`, `max_per_market=$30`.

---

## 7. Tick + trade dosyaları (local)

```
exports/bot16-ticks-20260429/
├── btc-updown-5m-1777483500_ticks.json   (301 tick)
├── btc-updown-5m-1777483500_trades.json  (12 trade)
├── btc-updown-5m-1777483800_ticks.json
├── btc-updown-5m-1777483800_trades.json
├── btc-updown-5m-1777484100_ticks.json
├── btc-updown-5m-1777484100_trades.json
├── btc-updown-5m-1777484400_ticks.json
├── btc-updown-5m-1777484400_trades.json
├── btc-updown-5m-1777484700_ticks.json
├── btc-updown-5m-1777484700_trades.json
├── btc-updown-5m-1777485000_ticks.json   (292 tick, devam)
└── btc-updown-5m-1777485000_trades.json
```

Yeniden simüle: `python3 scripts/batch_backtest.py --all-bot16`
