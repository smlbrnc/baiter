# Faz 13 — Staging E2E Runbook

Amaç: `clob-staging.polymarket.com` üzerinde 1 Harvest botu Live modda çalıştırıp
**1 saat boyunca 3 pencere** (5m × 12 veya 15m × 4) üzerinde doğrulamak.

## 1. Ön hazırlık

1. `.env.example` → `.env` kopyala. Staging satırlarını aç:
   ```
   GAMMA_BASE_URL=https://gamma-api-staging.polymarket.com
   CLOB_BASE_URL=https://clob-staging.polymarket.com
   CLOB_WS_BASE=wss://ws-subscriptions-clob-staging.polymarket.com/ws
   POLY_ADDRESS=0x...
   POLY_API_KEY=...
   POLY_PASSPHRASE=...
   POLY_SECRET=...
   POLYGON_PRIVATE_KEY=...
   ```
2. Staging hesabına minimum **50 USDC** yükle (5 USDC × 10 max order hacmi için buffer).
3. Release build:
   ```bash
   cargo build --release
   cd frontend && npm run build
   ```

## 2. Bot oluştur + başlat

```bash
# 1. Supervisor'ı başlat (arka planda)
./target/release/supervisor 2>&1 | tee supervisor.log &

# 2. Bot yarat (Live mod, harvest, 5m BTC)
curl -X POST http://127.0.0.1:3000/api/bots \
  -H 'Content-Type: application/json' \
  -d '{
    "name":"staging-btc-5m",
    "slug_pattern":"btc-updown-5m-",
    "strategy":"harvest",
    "run_mode":"live",
    "order_usdc":5.0,
    "signal_weight":5.0,
    "auto_start":true
  }'
```

Frontend (`npm run dev` veya `vite build`'ten dist) üzerinden bot sayfasını aç.

## 3. Doğrulama kriterleri

Her pencerede aşağıdaki sırayı izle:

### 3.1 Market Session Start
- [ ] `[[EVENT]] SessionOpened` log satırı supervisor.log'da göründü.
- [ ] Gamma'dan `clobTokenIds` ve `startDate`/`endDate` DB'de `market_sessions`'a yazıldı.
- [ ] Market WS `book` + `best_bid_ask` event'leri akıyor.

### 3.2 DeepTrade Zone (0-30%)
- [ ] Harvest FSM `OpenDualOpen` state'inde, 2 emir REST'e gitti.
- [ ] `POST /order` → `success=true` + `orderID` döndü.
- [ ] REST heartbeat her 5 sn gönderiliyor (`clob heartbeat` log satırı).
- [ ] Binance signal state `connected=true`, `signal_score` 0-10 aralığında.

### 3.3 NormalTrade / AggTrade Zone (30-85%)
- [ ] `avg_sum ≤ 0.98` olunca `ProfitLock`'a geçti, FAK emir atıldı.
- [ ] `User WS trade MATCHED` ile `shares_yes/no` güncellendi.
- [ ] `pnl_snapshots` tablosuna snapshot yazıldı.

### 3.4 StopTrade Zone (≥97%)
- [ ] Yeni emir atılmıyor.
- [ ] `ZoneChanged → StopTrade` event'i frontend SSE'ye düştü.

### 3.5 Market Resolved
- [ ] `market_resolved` event'i geldi; DB'de `market_resolved` satırı var.
- [ ] `SessionResolved` event'i frontend'e düştü.
- [ ] Bot yeni pencere bekleme moduna geçti (veya supervisor durduruldu).

## 4. Metrikleri kaydet

1 saat sonra:
```bash
sqlite3 ./data/baiter.db "SELECT * FROM market_sessions WHERE bot_id=1"
sqlite3 ./data/baiter.db "SELECT COUNT(*), SUM(size*price) FROM trades WHERE bot_id=1"
sqlite3 ./data/baiter.db "SELECT pnl_if_up, pnl_if_down, mtm_pnl, ts_ms FROM pnl_snapshots WHERE bot_id=1 ORDER BY ts_ms DESC LIMIT 3"
```

## 5. Hata senaryoları

- **Bot process crash**: supervisor otomatik yeniden başlatmalı (exponential backoff).
- **CLOB 401/403**: L1/L2 auth header'larını `docs/api/polymarket-clob.md` §auth bölümüne göre tekrar üret.
- **WS disconnect**: `run_ws_loop` otomatik reconnect (1s → 60s backoff). 3 dakikadan uzun kopma uyarı.
- **Heartbeat zaman aşımı**: Açık emirler 10 sn içinde iptal edilir (CLOB policy). Log'u izle.

## 6. Temizlik

```bash
curl -X POST http://127.0.0.1:3000/api/bots/1/stop
curl -X DELETE http://127.0.0.1:3000/api/bots/1
kill $(pgrep -f target/release/supervisor)
```

---

**Not:** Bu runbook operasyonel bir checklist'tir. Credential + canlı staging
hesabı gerektirir; kodu değiştirmez. Başarı kriteri: **3 pencere × 0 panik +
≥1 dolum/pencere + PnL snapshot tutarlılığı**.
