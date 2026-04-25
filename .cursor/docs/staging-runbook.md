# Faz 13 — Staging E2E Runbook

Amaç: `clob-staging.polymarket.com` üzerinde 1 Harvest botu Live modda çalıştırıp
**1 saat boyunca 3 pencere** (5m × 12 veya 15m × 4) üzerinde doğrulamak.

## 1. Ön hazırlık

1. `.env.example` → `.env` kopyala. Staging satırlarını aç (prod satırlarını yorumla):
   ```
   GAMMA_BASE_URL=https://gamma-api-staging.polymarket.com
   CLOB_BASE_URL=https://clob-staging.polymarket.com
   CLOB_WS_BASE=wss://ws-subscriptions-clob-staging.polymarket.com/ws
   ```
   `PORT` (varsayılan **3000**), `DB_PATH`, `BOT_BINARY`, `HEARTBEAT_DIR` ihtiyaca göre ayarlanır.
2. **Live kimlik:** Bot `RunMode::Live` iken Polymarket kimlik bilgileri SQLite'tan okunur — önce `bot_credentials` (bota özel), yoksa `global_credentials` (Settings sayfasında türetilen). `.env` içindeki `POLY_*` satırları bot süreci tarafından kullanılmaz. UI'dan `/settings` veya `/bots/new` ile kaydet; ikisi de boşsa `MissingCredentials` ile başlatma reddedilir.
3. Staging hesabına minimum **50 USDC** yükle (5 USDC × 10 max order hacmi için buffer).
4. Release build:
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
    "auto_start":true,
    "credentials":{
      "poly_address":"0x…",
      "poly_api_key":"…",
      "poly_passphrase":"…",
      "poly_secret":"…",
      "polygon_private_key":"0x…",
      "signature_type":0
    }
  }'
```

`signature_type` **1** veya **2** ise `funder` adresi zorunludur (`src/bot/ctx.rs` doğrulaması).

Frontend: `cd frontend && npm run dev` (Next.js, varsayılan **5173**; API için `PORT=3000` supervisor + gerekirse `next` proxy veya `NEXT_PUBLIC_` API tabanı env ile ayarlanır).

## 3. Doğrulama kriterleri

Her pencerede aşağıdaki sırayı izle:

### 3.1 Market Session Start
- [ ] `[[EVENT]] SessionOpened` log satırı supervisor.log'da göründü.
- [ ] Gamma'dan `clobTokenIds` ve `startDate`/`endDate` DB'de `market_sessions`'a yazıldı.
- [ ] Market WS `book` + `best_bid_ask` event'leri akıyor.

### 3.2 DeepTrade bölgesi (`zone_pct` &lt; 10 %)
- [ ] Harvest FSM `OpenDual{deadline}` aşamasında iki planlı GTC için ardışık **`POST /order`** çağrıları gitti (`Decision::PlaceOrders` → `engine::executor::place_batch`).
- [ ] Her `POST /order` → `success=true` + `orderID` döndü.
- [ ] REST heartbeat her 5 sn gönderiliyor (`clob heartbeat` log satırı).
- [ ] Binance signal state `connected=true`, `signal_score` 0-10 aralığında.

### 3.3 NormalTrade / AggTrade (`zone_pct` 10 %–90 %)
- [ ] `avg_sum ≤ 0.98` olunca `ProfitLock`'a geçti, FAK emir atıldı.
- [ ] `User WS trade MATCHED` ile `shares_yes/no` güncellendi.
- [ ] `pnl_snapshots` tablosuna snapshot yazıldı.

### 3.4 StopTrade Zone (≥97%)
- [ ] Yeni emir atılmıyor.
- [ ] `ZoneChanged` (`zone` içinde `StopTrade`) event'i frontend SSE'ye düştü (`src/ipc.rs` + `src/bot/zone.rs`).

### 3.5 Market Resolved
- [ ] `market_resolved` event'i geldi; DB'de `market_resolved` satırı var.
- [ ] `SessionResolved` event'i frontend'e düştü.
- [ ] Bot yeni pencere bekleme moduna geçti (veya supervisor durduruldu).

## 4. Metrikleri kaydet

1 saat sonra (`<BOT_ID>`'yi oluşturma yanıtındaki `id` ile değiştir):
```bash
sqlite3 ./data/baiter.db "SELECT * FROM market_sessions WHERE bot_id=<BOT_ID>"
sqlite3 ./data/baiter.db "SELECT COUNT(*), SUM(size*price) FROM trades WHERE bot_id=<BOT_ID>"
sqlite3 ./data/baiter.db "SELECT pnl_if_up, pnl_if_down, mtm_pnl, pair_count, ts_ms FROM pnl_snapshots WHERE bot_id=<BOT_ID> ORDER BY ts_ms DESC LIMIT 3"
```

## 5. Hata senaryoları

- **Bot process crash**: `src/supervisor.rs::run_bot_with_backoff` — çıkış kodu ≠ 0 iken **üst sınır 60 sn** olacak şekilde exponential backoff (`1s → 2s → 4s → …`); deneme sayısı sabitlenmemiş, kullanıcı `stop` ile döngüyü keser. `FAILED` DB state'i kullanılmaz; temiz çıkışta `STOPPED` yazılır.
- **CLOB 401/403**: L1/L2 auth header'larını `docs/api/polymarket-clob.md` §auth bölümüne göre tekrar üret.
- **WS disconnect**: `run_ws_loop` otomatik reconnect (1s → 60s backoff). 3 dakikadan uzun kopma uyarı.
- **Heartbeat zaman aşımı**: Açık emirler 10 sn içinde iptal edilir (CLOB policy). Log'u izle.

## 6. Temizlik

```bash
curl -X POST http://127.0.0.1:3000/api/bots/<BOT_ID>/stop
curl -X DELETE http://127.0.0.1:3000/api/bots/<BOT_ID>
kill $(pgrep -f target/release/supervisor)
```

---

**Not:** Bu runbook operasyonel bir checklist'tir. Credential + canlı staging
hesabı gerektirir; kodu değiştirmez. Başarı kriteri: **3 pencere × 0 panik +
≥1 dolum/pencere + PnL snapshot tutarlılığı**.
