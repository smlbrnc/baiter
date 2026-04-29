
# aras.md Ters-Mühendislik Doğrulaması

> Üretildi: `scripts/verify_aras_logs.py` — 6 polymarket-log + 6 tick dosyası
> **Sınırlamalar**: REDEEM eksik piyasalarda PnL share×\$1 yaklaşımıdır; on-chain merge, fee, maker-rebate dahil değildir.

## 1. Header İddiaları (C1–C4)

| ID | Açıklama | Doküman | Gerçek | Sonuç | Not |
|---|---|---|---|---|---|
| C1 | Toplam trade sayısı | 248 | 307 | **MISMATCH** | 307 satır BUY var, 248 aras.md iddiası |
| C2 | Piyasa (epoch) sayısı | 5 | 6 | **MISMATCH** | 6 log dosyası var, 5 aras.md iddiası |
| C3 | Net PnL toplamı (tahmini) | 252.17 | -1118.58 | **MISMATCH** | Redeem eksik piyasalar için share×$1 yaklaşımı; merge/fee yok |
| C4 | Kazanç/kayıp piyasa sayısı | 4W/1L | 2W/4L | **MISMATCH** |  |

## 2. Piyasa-Piyasa Özet

| epoch | n_trades | buy_USDC | redeem_USDC | winner | est_pnl | BRACKET | LADDER | DIRECTIONAL | SCOOP | OTHER | has_redeem |
|---|---|---|---|---|---|---|---|---|---|---|---|
| 1777467000 | 21 | 361.25 | 0.00 | UP | +65.77 * | 6 | 14 | 1 | 0 | 0 | ✗ |
| 1777467300 | 67 | 1188.30 | 1054.59 | DOWN | -133.71 | 5 | 28 | 17 | 16 | 1 | ✓ |
| 1777467600 | 44 | 775.16 | 795.54 | DOWN | +20.38 | 9 | 22 | 13 | 0 | 0 | ✓ |
| 1777467900 | 55 | 1423.31 | 0.00 | DOWN | -14.83 * | 4 | 41 | 10 | 0 | 0 | ✗ |
| 1777468200 | 59 | 3581.92 | 3563.24 | UP | -18.68 | 3 | 19 | 21 | 0 | 1 | ✓ |
| 1777468500 | 61 | 2038.05 | 0.00 | DOWN | -1037.51 * | 11 | 21 | 22 | 6 | 0 | ✗ |

_Toplam BUY USDC: 9367.98 | Toplam REDEEM: 5413.37_
_(*) = REDEEM eksik, PnL share×\$1 yaklaşımı_

## 3. Faz Örnekleri Doğrulaması (C5–C10)

| ID | Açıklama | Beklenen | Gerçek | Sonuç | Not |
|---|---|---|---|---|---|
| C5 | 1777467600 t=4s: 3 UP + 1 DOWN (taker sweep) | UP×3 DOWN×1 | UP×3 DOWN×1 | **MATCH** | Down@0.4751, Up@0.5267, Up@0.5490, Up@0.5500 |
| C6 | 1777467300 t≈160s UP@0.50 fill; t+10s ask≈0.50 | fill var + ask_t10≈0.50 | fill@t=[160] ask_t10=0.500 | **MATCH** | Polymarket ts fill zamanını kaydeder, emir koyuş değil |
| C7 | 1777467300 Faz3 DOWN serisi (t≈196, 242, 262) | 3 büyük DOWN trade | t≈196: 60@0.7400; t≈242: 55@0.7522; t≈262: 154@0.9138 | **MATCH** |  |
| C8 | 1777467300 Faz4 UP scoop (t≈282 toplam≈236+, t≈306 size≈4358) | t282≥236 & t306≥4358 | t282_toplam=355 t306=4358 | **MATCH** | t282 adet=16, t306 adet=1 |
| C9 | 1777467300 son pozisyon: naked_UP≈4629, paired≈1054 | naked_up≈4629 paired≈1054 | naked_up=4629 paired=1055 | **MATCH** | up_shares=5684 dn_shares=1055 |
| C10 | BRACKET_BASE_SIZE gözlemlenen p50 ∈ [38,52] | [38, 52] | p50=40 n=28 | **MATCH** |  |

## 4. Bölüm 8 Kanıt Tablosu Doğrulaması

| Kural | Piyasa | Trade# | Gerçek Veri | Sonuç | Not |
|---|---|---|---|---|---|
| Faz1_taker_sweep | 1777467600 | 2-4 | t=4s UP fiyatlar: ['0.5267', '0.5490', '0.5500'] | **MATCH** |  |
| Faz1_iki_taraf | 1777467600 | 1-4 | t=4s UP×3 DOWN×1 | **MATCH** |  |
| Faz2_GTC_150s | 1777467300 | 26 | t≈160s UP filleri: t=160s @0.5000, t=160s @0.5600 | **MATCH** |  |
| Faz2_simetrik_merdiven | 1777467600 | 25-29 | t=84-92s DOWN filleri: t=86s @0.1600, t=86s @0.1600, t=88s @0.1700, t=88s @0.1865, t=88s @0.2000 | **MATCH** | Toplam DOWN: 5 |
| Faz2_round_levels | 1777467300 | 35-66 | round-level fill sayısı=17 | **MATCH** | 0.10, 0.10, 0.11, 0.11, 0.11, 0.11, 0.11, 0.11, 0.11, 0.11 |
| Faz3_agresif_down | 1777467300 | 50 | t≈262s DOWN@~0.9138: bulundu size=154.0 | **MATCH** |  |
| Faz3_242_sweep | 1777467300 | 44-49 | t≈242s DOWN adet=8 @0.84≈ adet=5 | **MATCH** | 0.7522, 0.7900, 0.7900, 0.8400, 0.8400, 0.8400, 0.8400, 0.8400 |
| Faz4_post_close | 1777467300 | 67 | t≈306s UP@0.01 size≥4000: bulundu size=4358 | **MATCH** |  |
| Faz4_settlement_cluster | 1777467300 | 51-66 | t≈282s fill sayısı=16 (UP@≤0.13: 16) | **MATCH** | fill fiyatlar: ['0.10', '0.10', '0.11', '0.11', '0.11', '0.11', '0.11', '0.11'] |
| Risk_guard_eksik | 1777467300 | 1-13 vs 36-41 | Faz1 DOWN adet=5, Faz2 UP merdiven fill adet=3 | **MATCH** | UP merdiven fiyatlar: ['0.39', '0.40', '0.45'] |

## 5. Kural-Fit Oranı

| Faz | Eşleşen | Toplam | Oran |
|---|---|---|---|
| BRACKET | 38 | 307 | 12.4% |
| LADDER | 145 | 307 | 47.2% |
| DIRECTIONAL | 84 | 307 | 27.4% |
| SCOOP | 38 | 307 | 12.4% |
| OTHER | 2 | 307 | 0.7% |
| **TOPLAM** | **307** | **307** | **100%** |

## 6. docs/aras.md Tutarsızlıklar ve Öneriler


| Satır | Mevcut Metin | Öneri | Dayanak |
|---|---|---|---|
| 4 | `5 ardışık ... 248 trade` | `6 ardışık ... 307 trade` | 6 log dosyası; her biri `trades_count` sahip |
| 5 | `+$252.17 net (4 kazanç / 1 kayıp)` | `yaklaşık -1119 net (2W/4L) *` | `est_pnl` share×\$1 yaklaşımı; bazı REDEEM kayıt eksik |
| 53 | `5 piyasanın 4'ünde` | `6 piyasanın 2'inde` | piyasa bazlı win/loss |
| 994 | `5 BTC UP/DOWN 5m piyasası` | `6 BTC UP/DOWN 5m piyasası` | 6 log dosyası |

_(*) Kesin rakam için eksik REDEEM kayıtlarının on-chain doğrulaması gereklidir._


## 7. Trade-by-Trade Faz Özeti


### Epoch 1777467000 (UP) — 21 trade

| idx | t_off | outcome | size | price | tick_ask | pair_cost | signal | phase | reason |
|---|---|---|---|---|---|---|---|---|---|
| 0 | 6s | Up | 40.00 | 0.5400 | 0.5400 | 1.010 | 5.02 | `BRACKET` | px≈ask(0.5400) sz=40 pc=1.010 |
| 1 | 16s | Up | 18.78 | 0.5400 | 0.5900 | 1.010 | 6.74 | `BRACKET?` | fiyat_ok=False sz_ok=False px=0.5400 ask=0.5900 |
| 2 | 18s | Up | 41.00 | 0.5800 | 0.6500 | 1.010 | 6.88 | `BRACKET?` | fiyat_ok=False sz_ok=True px=0.5800 ask=0.6500 |
| 3 | 22s | Up | 42.00 | 0.5982 | 0.6500 | 1.010 | 7.56 | `BRACKET?` | fiyat_ok=False sz_ok=True px=0.5982 ask=0.6500 |
| 4 | 22s | Up | 43.00 | 0.6400 | 0.6500 | 1.010 | 7.56 | `BRACKET` | px≈ask(0.6500) sz=43 pc=1.010 |
| 5 | 26s | Up | 42.00 | 0.6200 | 0.6300 | 1.010 | 7.65 | `BRACKET` | px≈ask(0.6300) sz=42 pc=1.010 |
| 6 | 38s | Down | 42.00 | 0.3900 | 0.3800 | 1.010 | 7.51 | `LADDER` | level=0.40 dist=0.0100 yakın_ask |
| 7 | 50s | Up | 44.00 | 0.6335 | 0.6700 | 1.010 | 7.76 | `LADDER?` | level_uzak dist=0.1835 nearest=0.45 px=0.6335 |
| 8 | 50s | Up | 10.00 | 0.6500 | 0.6700 | 1.010 | 7.76 | `LADDER?` | level_uzak dist=0.2000 nearest=0.45 px=0.6500 |
| 9 | 50s | Up | 1.58 | 0.6600 | 0.6700 | 1.010 | 7.76 | `LADDER?` | level_uzak dist=0.2100 nearest=0.45 px=0.6600 |
| 10 | 54s | Down | 44.00 | 0.3325 | 0.4000 | 1.010 | 7.72 | `LADDER?` | level_uzak dist=0.0175 nearest=0.35 px=0.3325 |
| 11 | 54s | Down | 42.00 | 0.3526 | 0.4000 | 1.010 | 7.72 | `LADDER` | level=0.35 dist=0.0026 maker_bid_fill |
| 12 | 58s | Down | 41.00 | 0.4188 | 0.4300 | 1.010 | 7.46 | `LADDER?` | level_uzak dist=0.0188 nearest=0.40 px=0.4188 |
| 13 | 58s | Down | 5.02 | 0.4300 | 0.4300 | 1.010 | 7.46 | `LADDER?` | level_uzak dist=0.0200 nearest=0.45 px=0.4300 |
| 14 | 68s | Up | 42.00 | 0.6100 | 0.6600 | 1.010 | 7.76 | `LADDER?` | level_uzak dist=0.1600 nearest=0.45 px=0.6100 |
| 15 | 74s | Down | 42.00 | 0.3700 | 0.3400 | 1.010 | 7.51 | `LADDER?` | level_uzak dist=0.0200 nearest=0.35 px=0.3700 |
| 16 | 90s | Up | 50.00 | 0.6859 | 0.7500 | 1.010 | 8.31 | `LADDER?` | level_uzak dist=0.2359 nearest=0.45 px=0.6859 |
| 17 | 90s | Up | 0.76 | 0.7000 | 0.7500 | 1.010 | 8.31 | `LADDER?` | level_uzak dist=0.2500 nearest=0.45 px=0.7000 |
| 18 | 90s | Up | 51.00 | 0.7200 | 0.7500 | 1.010 | 8.31 | `LADDER?` | level_uzak dist=0.2700 nearest=0.45 px=0.7200 |
| 19 | 108s | Up | 0.90 | 0.8200 | 0.8500 | 1.010 | 9.12 | `LADDER?` | level_uzak dist=0.3700 nearest=0.45 px=0.8200 |
| 20 | 182s | Down | 14.91 | 0.2400 | 0.2200 | 1.010 | 9.28 | `DIRECTIONAL_HEDGE` | t=182s trend=+0.290 sz=15 px=0.2400 ters_yön |

### Epoch 1777467300 (DOWN) — 67 trade

| idx | t_off | outcome | size | price | tick_ask | pair_cost | signal | phase | reason |
|---|---|---|---|---|---|---|---|---|---|
| 0 | 10s | Down | 40.00 | 0.5100 | 0.5600 | 1.010 | 4.40 | `BRACKET?` | fiyat_ok=False sz_ok=True px=0.5100 ask=0.5600 |
| 1 | 20s | Down | 42.00 | 0.5900 | 0.6100 | 1.010 | 3.92 | `BRACKET` | px≈ask(0.6100) sz=42 pc=1.010 |
| 2 | 26s | Down | 43.00 | 0.6300 | 0.6800 | 1.010 | 3.58 | `BRACKET?` | fiyat_ok=False sz_ok=True px=0.6300 ask=0.6800 |
| 3 | 26s | Down | 44.00 | 0.6431 | 0.6800 | 1.010 | 3.58 | `BRACKET?` | fiyat_ok=False sz_ok=True px=0.6431 ask=0.6800 |
| 4 | 26s | Down | 45.00 | 0.6600 | 0.6800 | 1.010 | 3.58 | `BRACKET` | px≈ask(0.6800) sz=45 pc=1.010 |
| 5 | 32s | Up | 43.00 | 0.3255 | 0.3600 | 1.010 | 3.58 | `LADDER?` | level_uzak dist=0.0245 nearest=0.35 px=0.3255 |
| 6 | 40s | Down | 47.00 | 0.6700 | 0.6700 | 1.010 | 3.62 | `LADDER?` | level_uzak dist=0.2200 nearest=0.45 px=0.6700 |
| 7 | 50s | Up | 43.00 | 0.3669 | 0.4000 | 1.010 | 3.82 | `LADDER?` | level_uzak dist=0.0169 nearest=0.35 px=0.3669 |
| 8 | 50s | Up | 42.00 | 0.3764 | 0.4000 | 1.010 | 3.82 | `LADDER?` | level_uzak dist=0.0236 nearest=0.40 px=0.3764 |
| 9 | 50s | Up | 42.00 | 0.3782 | 0.4000 | 1.010 | 3.82 | `LADDER?` | level_uzak dist=0.0218 nearest=0.40 px=0.3782 |
| 10 | 60s | Down | 43.00 | 0.6121 | 0.6200 | 1.010 | 4.20 | `LADDER?` | level_uzak dist=0.1621 nearest=0.45 px=0.6121 |
| 11 | 60s | Down | 43.00 | 0.6200 | 0.6200 | 1.010 | 4.20 | `LADDER?` | level_uzak dist=0.1700 nearest=0.45 px=0.6200 |
| 12 | 66s | Up | 43.00 | 0.3500 | 0.3600 | 1.010 | 4.28 | `LADDER` | level=0.35 dist=0.0000 yakın_ask |
| 13 | 78s | Down | 27.79 | 0.6500 | 0.5500 | 1.010 | 3.98 | `LADDER?` | level_uzak dist=0.2000 nearest=0.45 px=0.6500 |
| 14 | 80s | Up | 42.00 | 0.3900 | 0.4600 | 1.010 | 4.28 | `LADDER` | level=0.40 dist=0.0100 maker_bid_fill |
| 15 | 80s | Up | 30.66 | 0.4000 | 0.4600 | 1.010 | 4.28 | `LADDER` | level=0.40 dist=0.0000 maker_bid_fill |
| 16 | 80s | Up | 40.00 | 0.4532 | 0.4600 | 1.010 | 4.28 | `LADDER` | level=0.45 dist=0.0032 yakın_ask |
| 17 | 90s | Up | 40.00 | 0.4772 | 0.4200 | 1.010 | 5.11 | `LADDER?` | level_uzak dist=0.0272 nearest=0.45 px=0.4772 |
| 18 | 92s | Down | 40.00 | 0.5360 | 0.5600 | 1.010 | 5.02 | `LADDER?` | level_uzak dist=0.0860 nearest=0.45 px=0.5360 |
| 19 | 92s | Down | 41.00 | 0.5476 | 0.5600 | 1.010 | 5.02 | `LADDER?` | level_uzak dist=0.0976 nearest=0.45 px=0.5476 |
| 20 | 92s | Down | 41.00 | 0.5500 | 0.5600 | 1.010 | 5.02 | `LADDER?` | level_uzak dist=0.1000 nearest=0.45 px=0.5500 |
| 21 | 132s | Down | 40.00 | 0.5400 | 0.5400 | 1.010 | 6.10 | `LADDER?` | level_uzak dist=0.0900 nearest=0.45 px=0.5400 |
| 22 | 142s | Up | 40.00 | 0.4745 | 0.4900 | 1.010 | 6.14 | `LADDER?` | level_uzak dist=0.0245 nearest=0.45 px=0.4745 |
| 23 | 142s | Up | 40.00 | 0.4800 | 0.4900 | 1.010 | 6.14 | `LADDER?` | level_uzak dist=0.0300 nearest=0.45 px=0.4800 |
| 24 | 160s | Up | 40.00 | 0.5000 | 0.5600 | 1.010 | 6.38 | `LADDER?` | level_uzak dist=0.0500 nearest=0.45 px=0.5000 |
| 25 | 160s | Up | 41.00 | 0.5600 | 0.5600 | 1.010 | 6.38 | `LADDER?` | level_uzak dist=0.1100 nearest=0.45 px=0.5600 |
| 26 | 164s | Down | 30.91 | 0.4591 | 0.5800 | 1.010 | 6.32 | `LADDER` | level=0.45 dist=0.0091 maker_bid_fill |
| 27 | 164s | Down | 41.00 | 0.5600 | 0.5800 | 1.010 | 6.32 | `LADDER?` | level_uzak dist=0.1100 nearest=0.45 px=0.5600 |
| 28 | 166s | Down | 1.89 | 0.4700 | 0.5700 | 1.010 | 6.01 | `LADDER?` | level_uzak dist=0.0200 nearest=0.45 px=0.4700 |
| 29 | 166s | Down | 41.00 | 0.5665 | 0.5700 | 1.010 | 6.01 | `LADDER?` | level_uzak dist=0.1165 nearest=0.45 px=0.5665 |
| 30 | 168s | Up | 40.00 | 0.4300 | 0.4600 | 1.010 | 5.98 | `LADDER?` | level_uzak dist=0.0200 nearest=0.45 px=0.4300 |
| 31 | 170s | Up | 40.00 | 0.4300 | 0.4600 | 1.010 | 5.99 | `LADDER?` | level_uzak dist=0.0200 nearest=0.45 px=0.4300 |
| 32 | 170s | Up | 40.00 | 0.4361 | 0.4600 | 1.010 | 5.99 | `LADDER` | level=0.45 dist=0.0139 maker_bid_fill |
| 33 | 196s | Down | 60.00 | 0.7400 | 0.7500 | 1.010 | 2.17 | `DIRECTIONAL` | t=196s trend=-0.240 sz=60 px=0.7400 |
| 34 | 202s | Up | 49.00 | 0.2600 | 0.3000 | 1.010 | 2.36 | `DIRECTIONAL_HEDGE` | t=202s trend=-0.200 sz=49 px=0.2600 ters_yön |
| 35 | 202s | Up | 45.00 | 0.2742 | 0.3000 | 1.010 | 2.36 | `DIRECTIONAL_HEDGE` | t=202s trend=-0.200 sz=45 px=0.2742 ters_yön |
| 36 | 222s | Up | 42.00 | 0.3056 | 0.2900 | 1.010 | 2.54 | `DIRECTIONAL_HEDGE` | t=222s trend=-0.210 sz=42 px=0.3056 ters_yön |
| 37 | 222s | Up | 36.68 | 0.3127 | 0.2900 | 1.010 | 2.54 | `DIRECTIONAL_HEDGE` | t=222s trend=-0.210 sz=37 px=0.3127 ters_yön |
| 38 | 234s | Up | 53.00 | 0.2200 | 0.2700 | 1.010 | 2.51 | `DIRECTIONAL_HEDGE` | t=234s trend=-0.230 sz=53 px=0.2200 ters_yön |
| 39 | 234s | Up | 51.00 | 0.2200 | 0.2700 | 1.010 | 2.51 | `DIRECTIONAL_HEDGE` | t=234s trend=-0.230 sz=51 px=0.2200 ters_yön |
| 40 | 236s | Up | 47.00 | 0.2109 | 0.2100 | 1.010 | 2.51 | `DIRECTIONAL_HEDGE` | t=236s trend=-0.290 sz=47 px=0.2109 ters_yön |
| 41 | 240s | Down | 55.00 | 0.7522 | 0.8400 | 1.010 | 2.38 | `DIRECTIONAL` | t=240s trend=-0.330 sz=55 px=0.7522 |
| 42 | 240s | Down | 7.00 | 0.7900 | 0.8400 | 1.010 | 2.38 | `DIRECTIONAL?` | t=240s trend=-0.330 sz=7 px=0.7900 trend_küçük=0.330 |
| 43 | 242s | Down | 53.00 | 0.7900 | 0.8300 | 1.010 | 2.39 | `DIRECTIONAL` | t=242s trend=-0.320 sz=53 px=0.7900 |
| 44 | 242s | Down | 8.20 | 0.8400 | 0.8300 | 1.010 | 2.39 | `DIRECTIONAL?` | t=242s trend=-0.320 sz=8 px=0.8400 trend_küçük=0.320 |
| 45 | 242s | Down | 52.50 | 0.8400 | 0.8300 | 1.010 | 2.39 | `DIRECTIONAL` | t=242s trend=-0.320 sz=52 px=0.8400 |
| 46 | 242s | Down | 6.25 | 0.8400 | 0.8300 | 1.010 | 2.39 | `DIRECTIONAL?` | t=242s trend=-0.320 sz=6 px=0.8400 trend_küçük=0.320 |
| 47 | 242s | Down | 6.00 | 0.8400 | 0.8300 | 1.010 | 2.39 | `DIRECTIONAL?` | t=242s trend=-0.320 sz=6 px=0.8400 trend_küçük=0.320 |
| 48 | 242s | Down | 1.05 | 0.8400 | 0.8300 | 1.010 | 2.39 | `DIRECTIONAL?` | t=242s trend=-0.320 sz=1 px=0.8400 trend_küçük=0.320 |
| 49 | 262s | Down | 154.00 | 0.9138 | 0.9100 | 1.010 | 2.96 | `DIRECTIONAL` | t=262s trend=-0.400 sz=154 px=0.9138 |
| 50 | 280s | Up | 50.08 | 0.1000 | 0.2700 | 1.010 | 2.24 | `SCOOP` | t=280s px=0.1000 kaybeden_taraf_ucuz |
| 51 | 282s | Up | 22.22 | 0.1000 | 0.2300 | 1.010 | 2.33 | `SCOOP` | t=282s px=0.1000 kaybeden_taraf_ucuz |
| 52 | 282s | Up | 6.00 | 0.1100 | 0.2300 | 1.010 | 2.33 | `SCOOP` | t=282s px=0.1100 kaybeden_taraf_ucuz |
| 53 | 282s | Up | 10.00 | 0.1100 | 0.2300 | 1.010 | 2.33 | `SCOOP` | t=282s px=0.1100 kaybeden_taraf_ucuz |
| 54 | 282s | Up | 6.00 | 0.1100 | 0.2300 | 1.010 | 2.33 | `SCOOP` | t=282s px=0.1100 kaybeden_taraf_ucuz |
| 55 | 282s | Up | 1.05 | 0.1100 | 0.2300 | 1.010 | 2.33 | `SCOOP` | t=282s px=0.1100 kaybeden_taraf_ucuz |
| 56 | 282s | Up | 30.00 | 0.1100 | 0.2300 | 1.010 | 2.33 | `SCOOP` | t=282s px=0.1100 kaybeden_taraf_ucuz |
| 57 | 282s | Up | 5.00 | 0.1100 | 0.2300 | 1.010 | 2.33 | `SCOOP` | t=282s px=0.1100 kaybeden_taraf_ucuz |
| 58 | 282s | Up | 17.13 | 0.1100 | 0.2300 | 1.010 | 2.33 | `SCOOP` | t=282s px=0.1100 kaybeden_taraf_ucuz |
| 59 | 282s | Up | 5.62 | 0.1100 | 0.2300 | 1.010 | 2.33 | `SCOOP` | t=282s px=0.1100 kaybeden_taraf_ucuz |
| 60 | 282s | Up | 5.40 | 0.1100 | 0.2300 | 1.010 | 2.33 | `SCOOP` | t=282s px=0.1100 kaybeden_taraf_ucuz |
| 61 | 282s | Up | 16.52 | 0.1200 | 0.2300 | 1.010 | 2.33 | `SCOOP` | t=282s px=0.1200 kaybeden_taraf_ucuz |
| 62 | 282s | Up | 5.00 | 0.1200 | 0.2300 | 1.010 | 2.33 | `SCOOP` | t=282s px=0.1200 kaybeden_taraf_ucuz |
| 63 | 282s | Up | 68.18 | 0.1200 | 0.2300 | 1.010 | 2.33 | `SCOOP` | t=282s px=0.1200 kaybeden_taraf_ucuz |
| 64 | 282s | Up | 5.28 | 0.1200 | 0.2300 | 1.010 | 2.33 | `SCOOP` | t=282s px=0.1200 kaybeden_taraf_ucuz |
| 65 | 284s | Up | 102.00 | 0.0700 | 0.1300 | 1.010 | 2.81 | `SCOOP` | t=284s px=0.0700 kaybeden_taraf_ucuz |
| 66 | 306s | Up | 4358.00 | 0.0100 | - | - | - | `OTHER` | tick_eşleşmedi |

### Epoch 1777467600 (DOWN) — 44 trade

| idx | t_off | outcome | size | price | tick_ask | pair_cost | signal | phase | reason |
|---|---|---|---|---|---|---|---|---|---|
| 0 | 4s | Down | 40.00 | 0.4751 | 0.5600 | 1.010 | 3.50 | `BRACKET?` | fiyat_ok=False sz_ok=True px=0.4751 ask=0.5600 |
| 1 | 4s | Up | 40.00 | 0.5267 | 0.4500 | 1.010 | 3.50 | `BRACKET?` | fiyat_ok=False sz_ok=True px=0.5267 ask=0.4500 |
| 2 | 4s | Up | 41.00 | 0.5490 | 0.4500 | 1.010 | 3.50 | `BRACKET?` | fiyat_ok=False sz_ok=True px=0.5490 ask=0.4500 |
| 3 | 4s | Up | 42.00 | 0.5500 | 0.4500 | 1.010 | 3.50 | `BRACKET?` | fiyat_ok=False sz_ok=True px=0.5500 ask=0.4500 |
| 4 | 6s | Down | 6.94 | 0.5400 | 0.5600 | 1.010 | 3.41 | `BRACKET?` | fiyat_ok=True sz_ok=False px=0.5400 ask=0.5600 |
| 5 | 10s | Up | 40.00 | 0.4750 | 0.6000 | 1.010 | 3.84 | `BRACKET?` | fiyat_ok=False sz_ok=True px=0.4750 ask=0.6000 |
| 6 | 10s | Down | 40.00 | 0.5300 | 0.4100 | 1.010 | 3.84 | `BRACKET?` | fiyat_ok=False sz_ok=True px=0.5300 ask=0.4100 |
| 7 | 12s | Up | 40.00 | 0.4840 | 0.6400 | 1.010 | 4.19 | `BRACKET?` | fiyat_ok=False sz_ok=True px=0.4840 ask=0.6400 |
| 8 | 12s | Up | 4.35 | 0.5700 | 0.6400 | 1.010 | 4.19 | `BRACKET?` | fiyat_ok=False sz_ok=False px=0.5700 ask=0.6400 |
| 9 | 32s | Up | 49.00 | 0.6938 | 0.7400 | 1.010 | 5.43 | `LADDER?` | level_uzak dist=0.2438 nearest=0.45 px=0.6938 |
| 10 | 32s | Up | 50.00 | 0.7000 | 0.7400 | 1.010 | 5.43 | `LADDER?` | level_uzak dist=0.2500 nearest=0.45 px=0.7000 |
| 11 | 38s | Up | 55.00 | 0.7400 | 0.6500 | 1.010 | 5.51 | `LADDER?` | level_uzak dist=0.2900 nearest=0.45 px=0.7400 |
| 12 | 40s | Down | 35.00 | 0.2457 | 0.4500 | 1.010 | 4.69 | `LADDER` | level=0.25 dist=0.0043 maker_bid_fill |
| 13 | 40s | Up | 47.42 | 0.7663 | 0.5600 | 1.010 | 4.69 | `LADDER?` | level_uzak dist=0.3163 nearest=0.45 px=0.7663 |
| 14 | 40s | Up | 4.35 | 0.7700 | 0.5600 | 1.010 | 4.69 | `LADDER?` | level_uzak dist=0.3200 nearest=0.45 px=0.7700 |
| 15 | 54s | Up | 41.00 | 0.5530 | 0.6500 | 1.010 | 4.61 | `LADDER?` | level_uzak dist=0.1030 nearest=0.45 px=0.5530 |
| 16 | 54s | Up | 10.00 | 0.5850 | 0.6500 | 1.010 | 4.61 | `LADDER?` | level_uzak dist=0.1350 nearest=0.45 px=0.5850 |
| 17 | 54s | Up | 9.71 | 0.6100 | 0.6500 | 1.010 | 4.61 | `LADDER?` | level_uzak dist=0.1600 nearest=0.45 px=0.6100 |
| 18 | 54s | Up | 7.95 | 0.6100 | 0.6500 | 1.010 | 4.61 | `LADDER?` | level_uzak dist=0.1600 nearest=0.45 px=0.6100 |
| 19 | 54s | Up | 5.13 | 0.6200 | 0.6500 | 1.010 | 4.61 | `LADDER?` | level_uzak dist=0.1700 nearest=0.45 px=0.6200 |
| 20 | 56s | Up | 31.72 | 0.6200 | 0.6400 | 1.010 | 4.73 | `LADDER?` | level_uzak dist=0.1700 nearest=0.45 px=0.6200 |
| 21 | 56s | Up | 5.13 | 0.6200 | 0.6400 | 1.010 | 4.73 | `LADDER?` | level_uzak dist=0.1700 nearest=0.45 px=0.6200 |
| 22 | 72s | Up | 36.00 | 0.7903 | 0.8500 | 1.010 | 8.57 | `LADDER?` | level_uzak dist=0.3403 nearest=0.45 px=0.7903 |
| 23 | 72s | Up | 78.00 | 0.8500 | 0.8500 | 1.010 | 8.57 | `LADDER?` | level_uzak dist=0.4000 nearest=0.45 px=0.8500 |
| 24 | 86s | Down | 68.00 | 0.1600 | 0.2600 | 1.010 | 8.17 | `LADDER` | level=0.17 dist=0.0100 maker_bid_fill |
| 25 | 86s | Down | 74.00 | 0.1600 | 0.2600 | 1.010 | 8.17 | `LADDER` | level=0.17 dist=0.0100 maker_bid_fill |
| 26 | 88s | Down | 2.02 | 0.1700 | 0.2600 | 1.010 | 7.86 | `LADDER` | level=0.17 dist=0.0000 maker_bid_fill |
| 27 | 88s | Down | 23.14 | 0.1865 | 0.2600 | 1.010 | 7.86 | `LADDER` | level=0.20 dist=0.0135 maker_bid_fill |
| 28 | 88s | Down | 45.00 | 0.2000 | 0.2600 | 1.010 | 7.86 | `LADDER` | level=0.20 dist=0.0000 maker_bid_fill |
| 29 | 100s | Up | 56.00 | 0.7500 | 0.7600 | 1.010 | 8.85 | `LADDER?` | level_uzak dist=0.3000 nearest=0.45 px=0.7500 |
| 30 | 106s | Up | 60.00 | 0.7839 | 0.8200 | 1.010 | 9.15 | `LADDER?` | level_uzak dist=0.3339 nearest=0.45 px=0.7839 |
| 31 | 208s | Down | 44.78 | 0.0900 | 0.1100 | 1.010 | 7.80 | `DIRECTIONAL_HEDGE` | t=208s trend=+0.400 sz=45 px=0.0900 ters_yön |
| 32 | 212s | Down | 83.00 | 0.1336 | 0.1700 | 1.010 | 7.16 | `DIRECTIONAL_HEDGE` | t=212s trend=+0.340 sz=83 px=0.1336 ters_yön |
| 33 | 214s | Down | 78.00 | 0.1500 | 0.1600 | 1.010 | 6.82 | `DIRECTIONAL_HEDGE` | t=214s trend=+0.350 sz=78 px=0.1500 ters_yön |
| 34 | 228s | Down | 68.00 | 0.1509 | 0.1500 | 1.010 | 5.68 | `DIRECTIONAL_HEDGE` | t=228s trend=+0.360 sz=68 px=0.1509 ters_yön |
| 35 | 246s | Down | 26.00 | 0.1658 | 0.2000 | 1.010 | 5.19 | `DIRECTIONAL_HEDGE` | t=246s trend=+0.310 sz=26 px=0.1658 ters_yön |
| 36 | 254s | Down | 0.67 | 0.2100 | 0.4500 | 1.010 | 4.14 | `DIRECTIONAL?` | t=254s trend=+0.060 sz=1 px=0.2100 trend_küçük=0.060 |
| 37 | 254s | Down | 17.99 | 0.2200 | 0.4500 | 1.010 | 4.14 | `DIRECTIONAL?` | t=254s trend=+0.060 sz=18 px=0.2200 trend_küçük=0.060 |
| 38 | 254s | Down | 52.00 | 0.2451 | 0.4500 | 1.010 | 4.14 | `DIRECTIONAL?` | t=254s trend=+0.060 sz=52 px=0.2451 trend_küçük=0.060 |
| 39 | 254s | Down | 51.00 | 0.2500 | 0.4500 | 1.010 | 4.14 | `DIRECTIONAL?` | t=254s trend=+0.060 sz=51 px=0.2500 trend_küçük=0.060 |
| 40 | 266s | Up | 41.00 | 0.5144 | 0.5900 | 1.010 | 4.39 | `DIRECTIONAL?` | t=266s trend=+0.090 sz=41 px=0.5144 trend_küçük=0.090 |
| 41 | 266s | Up | 42.00 | 0.5264 | 0.5900 | 1.010 | 4.39 | `DIRECTIONAL?` | t=266s trend=+0.090 sz=42 px=0.5264 trend_küçük=0.090 |
| 42 | 270s | Down | 40.00 | 0.5080 | 0.4200 | 1.040 | 4.22 | `DIRECTIONAL_HEDGE` | t=270s trend=+0.120 sz=40 px=0.5080 ters_yön |
| 43 | 278s | Up | 42.00 | 0.6200 | 0.7300 | 1.010 | 5.03 | `DIRECTIONAL` | t=278s trend=+0.230 sz=42 px=0.6200 |

### Epoch 1777467900 (DOWN) — 55 trade

| idx | t_off | outcome | size | price | tick_ask | pair_cost | signal | phase | reason |
|---|---|---|---|---|---|---|---|---|---|
| 0 | 16s | Up | 40.00 | 0.4718 | 0.5900 | 1.020 | 5.30 | `BRACKET?` | fiyat_ok=False sz_ok=True px=0.4718 ask=0.5900 |
| 1 | 16s | Up | 40.00 | 0.4900 | 0.5900 | 1.020 | 5.30 | `BRACKET?` | fiyat_ok=False sz_ok=True px=0.4900 ask=0.5900 |
| 2 | 20s | Up | 40.00 | 0.4951 | 0.5600 | 1.010 | 5.71 | `BRACKET?` | fiyat_ok=False sz_ok=True px=0.4951 ask=0.5600 |
| 3 | 24s | Down | 25.69 | 0.4639 | 0.5200 | 1.010 | 5.31 | `BRACKET?` | fiyat_ok=False sz_ok=True px=0.4639 ask=0.5200 |
| 4 | 42s | Down | 40.00 | 0.4651 | 0.6900 | 1.010 | 2.36 | `LADDER?` | level_uzak dist=0.0151 nearest=0.45 px=0.4651 |
| 5 | 42s | Down | 6.63 | 0.5400 | 0.6900 | 1.010 | 2.36 | `LADDER?` | level_uzak dist=0.0900 nearest=0.45 px=0.5400 |
| 6 | 42s | Down | 23.56 | 0.5800 | 0.6900 | 1.010 | 2.36 | `LADDER?` | level_uzak dist=0.1300 nearest=0.45 px=0.5800 |
| 7 | 42s | Down | 9.33 | 0.5800 | 0.6900 | 1.010 | 2.36 | `LADDER?` | level_uzak dist=0.1300 nearest=0.45 px=0.5800 |
| 8 | 42s | Down | 5.71 | 0.5800 | 0.6900 | 1.010 | 2.36 | `LADDER?` | level_uzak dist=0.1300 nearest=0.45 px=0.5800 |
| 9 | 42s | Down | 2.38 | 0.5800 | 0.6900 | 1.010 | 2.36 | `LADDER?` | level_uzak dist=0.1300 nearest=0.45 px=0.5800 |
| 10 | 68s | Down | 65.00 | 0.7978 | 0.8200 | 1.010 | 1.07 | `LADDER?` | level_uzak dist=0.3478 nearest=0.45 px=0.7978 |
| 11 | 70s | Down | 63.00 | 0.7884 | 0.8300 | 1.010 | 0.92 | `LADDER?` | level_uzak dist=0.3384 nearest=0.45 px=0.7884 |
| 12 | 70s | Down | 74.00 | 0.8088 | 0.8300 | 1.010 | 0.92 | `LADDER?` | level_uzak dist=0.3588 nearest=0.45 px=0.8088 |
| 13 | 76s | Up | 71.00 | 0.1603 | 0.2500 | 1.010 | 1.29 | `LADDER` | level=0.17 dist=0.0097 maker_bid_fill |
| 14 | 82s | Up | 3.53 | 0.2200 | 0.2600 | 1.010 | 1.23 | `LADDER?` | level_uzak dist=0.0200 nearest=0.20 px=0.2200 |
| 15 | 84s | Up | 52.00 | 0.2300 | 0.2100 | 1.010 | 1.33 | `LADDER?` | level_uzak dist=0.0200 nearest=0.25 px=0.2300 |
| 16 | 84s | Up | 53.00 | 0.2300 | 0.2100 | 1.010 | 1.33 | `LADDER?` | level_uzak dist=0.0200 nearest=0.25 px=0.2300 |
| 17 | 86s | Down | 38.00 | 0.7563 | 0.8000 | 1.010 | 1.22 | `LADDER?` | level_uzak dist=0.3063 nearest=0.45 px=0.7563 |
| 18 | 86s | Down | 17.00 | 0.7600 | 0.8000 | 1.010 | 1.22 | `LADDER?` | level_uzak dist=0.3100 nearest=0.45 px=0.7600 |
| 19 | 86s | Down | 58.00 | 0.7700 | 0.8000 | 1.010 | 1.22 | `LADDER?` | level_uzak dist=0.3200 nearest=0.45 px=0.7700 |
| 20 | 88s | Up | 2.55 | 0.2000 | 0.2000 | 1.010 | 1.13 | `LADDER` | level=0.20 dist=0.0000 yakın_ask |
| 21 | 88s | Up | 1.25 | 0.2000 | 0.2000 | 1.010 | 1.13 | `LADDER` | level=0.20 dist=0.0000 yakın_ask |
| 22 | 88s | Up | 2.50 | 0.2000 | 0.2000 | 1.010 | 1.13 | `LADDER` | level=0.20 dist=0.0000 yakın_ask |
| 23 | 88s | Down | 60.00 | 0.7700 | 0.8100 | 1.010 | 1.13 | `LADDER?` | level_uzak dist=0.3200 nearest=0.45 px=0.7700 |
| 24 | 102s | Up | 56.00 | 0.2248 | 0.3400 | 1.020 | 1.35 | `LADDER?` | level_uzak dist=0.0248 nearest=0.20 px=0.2248 |
| 25 | 102s | Up | 55.00 | 0.2377 | 0.3400 | 1.020 | 1.35 | `LADDER` | level=0.25 dist=0.0123 maker_bid_fill |
| 26 | 102s | Up | 53.00 | 0.2500 | 0.3400 | 1.020 | 1.35 | `LADDER` | level=0.25 dist=0.0000 maker_bid_fill |
| 27 | 110s | Up | 11.43 | 0.3700 | 0.4400 | 1.010 | 2.78 | `LADDER?` | level_uzak dist=0.0200 nearest=0.35 px=0.3700 |
| 28 | 110s | Up | 19.52 | 0.3800 | 0.4400 | 1.010 | 2.78 | `LADDER?` | level_uzak dist=0.0200 nearest=0.40 px=0.3800 |
| 29 | 110s | Up | 20.39 | 0.3900 | 0.4400 | 1.010 | 2.78 | `LADDER` | level=0.40 dist=0.0100 maker_bid_fill |
| 30 | 110s | Up | 12.11 | 0.4000 | 0.4400 | 1.010 | 2.78 | `LADDER` | level=0.40 dist=0.0000 maker_bid_fill |
| 31 | 110s | Up | 29.89 | 0.4000 | 0.4400 | 1.010 | 2.78 | `LADDER` | level=0.40 dist=0.0000 maker_bid_fill |
| 32 | 110s | Up | 41.00 | 0.4200 | 0.4400 | 1.010 | 2.78 | `LADDER?` | level_uzak dist=0.0200 nearest=0.40 px=0.4200 |
| 33 | 126s | Down | 45.00 | 0.6694 | 0.7000 | 1.010 | 2.89 | `LADDER?` | level_uzak dist=0.2194 nearest=0.45 px=0.6694 |
| 34 | 136s | Up | 43.00 | 0.3680 | 0.3400 | 1.010 | 5.42 | `LADDER?` | level_uzak dist=0.0180 nearest=0.35 px=0.3680 |
| 35 | 140s | Down | 15.00 | 0.6467 | 0.6400 | 1.010 | 5.38 | `LADDER?` | level_uzak dist=0.1967 nearest=0.45 px=0.6467 |
| 36 | 142s | Up | 20.00 | 0.3250 | 0.3800 | 1.010 | 5.53 | `LADDER?` | level_uzak dist=0.0250 nearest=0.35 px=0.3250 |
| 37 | 152s | Down | 50.00 | 0.7164 | 0.8000 | 1.010 | 2.00 | `LADDER?` | level_uzak dist=0.2664 nearest=0.45 px=0.7164 |
| 38 | 152s | Down | 55.00 | 0.7517 | 0.8000 | 1.010 | 2.00 | `LADDER?` | level_uzak dist=0.3017 nearest=0.45 px=0.7517 |
| 39 | 162s | Down | 78.00 | 0.8398 | 0.8900 | 1.010 | 1.31 | `LADDER?` | level_uzak dist=0.3898 nearest=0.45 px=0.8398 |
| 40 | 168s | Down | 136.00 | 0.9000 | 0.9400 | 1.010 | 0.96 | `LADDER?` | level_uzak dist=0.4500 nearest=0.45 px=0.9000 |
| 41 | 172s | Down | 74.17 | 0.9188 | 0.9200 | 1.010 | 0.84 | `LADDER?` | level_uzak dist=0.4688 nearest=0.45 px=0.9188 |
| 42 | 172s | Down | 26.22 | 0.9300 | 0.9200 | 1.010 | 0.84 | `LADDER?` | level_uzak dist=0.4800 nearest=0.45 px=0.9300 |
| 43 | 172s | Down | 123.99 | 0.9300 | 0.9200 | 1.010 | 0.84 | `LADDER?` | level_uzak dist=0.4800 nearest=0.45 px=0.9300 |
| 44 | 172s | Down | 3.79 | 0.9300 | 0.9200 | 1.010 | 0.84 | `LADDER?` | level_uzak dist=0.4800 nearest=0.45 px=0.9300 |
| 45 | 190s | Up | 131.43 | 0.0672 | 0.0800 | 1.010 | 0.89 | `DIRECTIONAL_HEDGE` | t=190s trend=-0.420 sz=131 px=0.0672 ters_yön |
| 46 | 190s | Up | 8.57 | 0.0700 | 0.0800 | 1.010 | 0.89 | `DIRECTIONAL_HEDGE` | t=190s trend=-0.420 sz=9 px=0.0700 ters_yön |
| 47 | 190s | Up | 14.00 | 0.0700 | 0.0800 | 1.010 | 0.89 | `DIRECTIONAL_HEDGE` | t=190s trend=-0.420 sz=14 px=0.0700 ters_yön |
| 48 | 242s | Up | 260.00 | 0.0318 | 0.1800 | 1.010 | 3.54 | `DIRECTIONAL_HEDGE` | t=242s trend=-0.320 sz=260 px=0.0318 ters_yön |
| 49 | 242s | Up | 211.00 | 0.0400 | 0.1800 | 1.010 | 3.54 | `DIRECTIONAL_HEDGE` | t=242s trend=-0.320 sz=211 px=0.0400 ters_yön |
| 50 | 262s | Up | 5.00 | 0.0600 | 0.1100 | 1.010 | 4.97 | `DIRECTIONAL_HEDGE` | t=262s trend=-0.390 sz=5 px=0.0600 ters_yön |
| 51 | 262s | Up | 5.11 | 0.0600 | 0.1100 | 1.010 | 4.97 | `DIRECTIONAL_HEDGE` | t=262s trend=-0.390 sz=5 px=0.0600 ters_yön |
| 52 | 268s | Down | 136.00 | 0.9200 | 0.9300 | 1.010 | 4.78 | `DIRECTIONAL` | t=268s trend=-0.420 sz=136 px=0.9200 |
| 53 | 268s | Down | 177.00 | 0.9240 | 0.9300 | 1.010 | 4.78 | `DIRECTIONAL` | t=268s trend=-0.420 sz=177 px=0.9240 |
| 54 | 276s | Up | 62.54 | 0.0654 | 0.0500 | 1.010 | 4.90 | `DIRECTIONAL_HEDGE` | t=276s trend=-0.450 sz=63 px=0.0654 ters_yön |

### Epoch 1777468200 (UP) — 59 trade

| idx | t_off | outcome | size | price | tick_ask | pair_cost | signal | phase | reason |
|---|---|---|---|---|---|---|---|---|---|
| 0 | 12s | Down | 40.00 | 0.4800 | 0.4600 | 1.010 | 4.08 | `BRACKET` | px≈ask(0.4600) sz=40 pc=1.010 |
| 1 | 14s | Up | 40.00 | 0.5372 | 0.5500 | 1.010 | 4.20 | `BRACKET` | px≈ask(0.5500) sz=40 pc=1.010 |
| 2 | 14s | Up | 41.00 | 0.5400 | 0.5500 | 1.010 | 4.20 | `BRACKET` | px≈ask(0.5500) sz=41 pc=1.010 |
| 3 | 54s | Down | 40.00 | 0.4900 | 0.4900 | 1.010 | 6.09 | `LADDER?` | level_uzak dist=0.0400 nearest=0.45 px=0.4900 |
| 4 | 64s | Up | 40.00 | 0.5248 | 0.5800 | 1.010 | 6.24 | `LADDER?` | level_uzak dist=0.0748 nearest=0.45 px=0.5248 |
| 5 | 70s | Down | 41.00 | 0.4215 | 0.4600 | 1.010 | 6.56 | `LADDER?` | level_uzak dist=0.0215 nearest=0.40 px=0.4215 |
| 6 | 70s | Down | 40.00 | 0.4462 | 0.4600 | 1.010 | 6.56 | `LADDER` | level=0.45 dist=0.0038 maker_bid_fill |
| 7 | 72s | Up | 41.00 | 0.5463 | 0.5700 | 1.010 | 6.58 | `LADDER?` | level_uzak dist=0.0963 nearest=0.45 px=0.5463 |
| 8 | 82s | Up | 42.00 | 0.5888 | 0.6000 | 1.010 | 6.81 | `LADDER?` | level_uzak dist=0.1388 nearest=0.45 px=0.5888 |
| 9 | 92s | Down | 40.00 | 0.4500 | 0.4500 | 1.010 | 4.94 | `LADDER` | level=0.45 dist=0.0000 yakın_ask |
| 10 | 94s | Down | 41.00 | 0.4200 | 0.4400 | 1.010 | 5.06 | `LADDER?` | level_uzak dist=0.0200 nearest=0.40 px=0.4200 |
| 11 | 98s | Down | 40.00 | 0.4303 | 0.4400 | 1.010 | 4.87 | `LADDER?` | level_uzak dist=0.0197 nearest=0.45 px=0.4303 |
| 12 | 104s | Down | 40.00 | 0.4438 | 0.3100 | 1.020 | 5.61 | `LADDER` | level=0.45 dist=0.0062 yakın_ask |
| 13 | 104s | Down | 40.00 | 0.4500 | 0.3100 | 1.020 | 5.61 | `LADDER` | level=0.45 dist=0.0000 yakın_ask |
| 14 | 106s | Up | 41.00 | 0.5200 | 0.7400 | 1.010 | 6.76 | `LADDER?` | level_uzak dist=0.0700 nearest=0.45 px=0.5200 |
| 15 | 106s | Up | 40.00 | 0.5200 | 0.7400 | 1.010 | 6.76 | `LADDER?` | level_uzak dist=0.0700 nearest=0.45 px=0.5200 |
| 16 | 114s | Up | 63.00 | 0.7872 | 0.8200 | 1.010 | 7.77 | `LADDER?` | level_uzak dist=0.3372 nearest=0.45 px=0.7872 |
| 17 | 126s | Up | 30.00 | 0.8300 | 0.8200 | 1.010 | 7.84 | `LADDER?` | level_uzak dist=0.3800 nearest=0.45 px=0.8300 |
| 18 | 126s | Up | 83.00 | 0.8498 | 0.8200 | 1.010 | 7.84 | `LADDER?` | level_uzak dist=0.3998 nearest=0.45 px=0.8498 |
| 19 | 128s | Down | 65.00 | 0.1700 | 0.1800 | 1.010 | 7.60 | `LADDER` | level=0.17 dist=0.0000 yakın_ask |
| 20 | 128s | Down | 62.00 | 0.1806 | 0.1800 | 1.010 | 7.60 | `LADDER` | level=0.17 dist=0.0106 yakın_ask |
| 21 | 128s | Down | 60.00 | 0.1995 | 0.1800 | 1.010 | 7.60 | `LADDER` | level=0.20 dist=0.0005 yakın_ask |
| 22 | 268s | Up | 5.00 | 0.9900 | 0.9910 | 1.001 | 8.11 | `DIRECTIONAL?` | t=268s trend=+0.491 sz=5 px=0.9900 trend_küçük=0.491 |
| 23 | 268s | Up | 100.00 | 0.9900 | 0.9910 | 1.001 | 8.11 | `DIRECTIONAL` | t=268s trend=+0.491 sz=100 px=0.9900 |
| 24 | 268s | Up | 26.00 | 0.9900 | 0.9910 | 1.001 | 8.11 | `DIRECTIONAL?` | t=268s trend=+0.491 sz=26 px=0.9900 trend_küçük=0.491 |
| 25 | 268s | Up | 2.08 | 0.9900 | 0.9910 | 1.001 | 8.11 | `DIRECTIONAL?` | t=268s trend=+0.491 sz=2 px=0.9900 trend_küçük=0.491 |
| 26 | 268s | Up | 1166.59 | 0.9900 | 0.9910 | 1.001 | 8.11 | `DIRECTIONAL` | t=268s trend=+0.491 sz=1167 px=0.9900 |
| 27 | 268s | Up | 31.46 | 0.9900 | 0.9910 | 1.001 | 8.11 | `DIRECTIONAL?` | t=268s trend=+0.491 sz=31 px=0.9900 trend_küçük=0.491 |
| 28 | 270s | Up | 100.00 | 0.9900 | 0.9910 | 1.001 | 8.14 | `DIRECTIONAL` | t=270s trend=+0.491 sz=100 px=0.9900 |
| 29 | 270s | Up | 132.02 | 0.9900 | 0.9910 | 1.001 | 8.14 | `DIRECTIONAL` | t=270s trend=+0.491 sz=132 px=0.9900 |
| 30 | 270s | Up | 1.13 | 0.9900 | 0.9910 | 1.001 | 8.14 | `DIRECTIONAL?` | t=270s trend=+0.491 sz=1 px=0.9900 trend_küçük=0.491 |
| 31 | 270s | Up | 1.06 | 0.9900 | 0.9910 | 1.001 | 8.14 | `DIRECTIONAL?` | t=270s trend=+0.491 sz=1 px=0.9900 trend_küçük=0.491 |
| 32 | 270s | Up | 1.01 | 0.9900 | 0.9910 | 1.001 | 8.14 | `DIRECTIONAL?` | t=270s trend=+0.491 sz=1 px=0.9900 trend_küçük=0.491 |
| 33 | 272s | Up | 34.16 | 0.9900 | 1.0000 | 1.001 | 9.12 | `DIRECTIONAL?` | t=272s trend=+0.500 sz=34 px=0.9900 trend_küçük=0.500 |
| 34 | 272s | Up | 1.17 | 0.9900 | 1.0000 | 1.001 | 9.12 | `DIRECTIONAL?` | t=272s trend=+0.500 sz=1 px=0.9900 trend_küçük=0.500 |
| 35 | 272s | Up | 657.07 | 0.9900 | 1.0000 | 1.001 | 9.12 | `DIRECTIONAL` | t=272s trend=+0.500 sz=657 px=0.9900 |
| 36 | 272s | Up | 214.16 | 0.9900 | 1.0000 | 1.001 | 9.12 | `DIRECTIONAL` | t=272s trend=+0.500 sz=214 px=0.9900 |
| 37 | 274s | Up | 108.15 | 0.9900 | 1.0000 | 1.010 | 9.35 | `DIRECTIONAL` | t=274s trend=+0.500 sz=108 px=0.9900 |
| 38 | 274s | Up | 30.44 | 0.9900 | 1.0000 | 1.010 | 9.35 | `DIRECTIONAL?` | t=274s trend=+0.500 sz=30 px=0.9900 trend_küçük=0.500 |
| 39 | 274s | Up | 84.36 | 0.9900 | 1.0000 | 1.010 | 9.35 | `DIRECTIONAL` | t=274s trend=+0.500 sz=84 px=0.9900 |
| 40 | 278s | Up | 1.98 | 0.9900 | 1.0000 | 1.010 | 9.35 | `DIRECTIONAL?` | t=278s trend=+0.500 sz=2 px=0.9900 trend_küçük=0.500 |
| 41 | 278s | Up | 1.04 | 0.9900 | 1.0000 | 1.010 | 9.35 | `DIRECTIONAL?` | t=278s trend=+0.500 sz=1 px=0.9900 trend_küçük=0.500 |
| 42 | 278s | Up | 100.00 | 0.9900 | 1.0000 | 1.010 | 9.35 | `DIRECTIONAL` | t=278s trend=+0.500 sz=100 px=0.9900 |
| 43 | 284s | Up | 12.00 | 0.9900 | 0.9990 | 1.008 | 9.37 | `SCOOP_WINNER` | t=284s px=0.9900 kazanan_taraf_pahalı |
| 44 | 292s | Up | 1.06 | 0.9900 | 1.0000 | 1.001 | 9.38 | `SCOOP_WINNER` | t=292s px=0.9900 kazanan_taraf_pahalı |
| 45 | 292s | Up | 1.06 | 0.9900 | 1.0000 | 1.001 | 9.38 | `SCOOP_WINNER` | t=292s px=0.9900 kazanan_taraf_pahalı |
| 46 | 292s | Up | 1.06 | 0.9900 | 1.0000 | 1.001 | 9.38 | `SCOOP_WINNER` | t=292s px=0.9900 kazanan_taraf_pahalı |
| 47 | 292s | Up | 1.06 | 0.9900 | 1.0000 | 1.001 | 9.38 | `SCOOP_WINNER` | t=292s px=0.9900 kazanan_taraf_pahalı |
| 48 | 292s | Up | 1.06 | 0.9900 | 1.0000 | 1.001 | 9.38 | `SCOOP_WINNER` | t=292s px=0.9900 kazanan_taraf_pahalı |
| 49 | 292s | Up | 1.50 | 0.9900 | 1.0000 | 1.001 | 9.38 | `SCOOP_WINNER` | t=292s px=0.9900 kazanan_taraf_pahalı |
| 50 | 292s | Up | 1.06 | 0.9900 | 1.0000 | 1.001 | 9.38 | `SCOOP_WINNER` | t=292s px=0.9900 kazanan_taraf_pahalı |
| 51 | 292s | Up | 1.06 | 0.9900 | 1.0000 | 1.001 | 9.38 | `SCOOP_WINNER` | t=292s px=0.9900 kazanan_taraf_pahalı |
| 52 | 292s | Up | 1.06 | 0.9900 | 1.0000 | 1.001 | 9.38 | `SCOOP_WINNER` | t=292s px=0.9900 kazanan_taraf_pahalı |
| 53 | 292s | Up | 1.06 | 0.9900 | 1.0000 | 1.001 | 9.38 | `SCOOP_WINNER` | t=292s px=0.9900 kazanan_taraf_pahalı |
| 54 | 292s | Up | 1.87 | 0.9900 | 1.0000 | 1.001 | 9.38 | `SCOOP_WINNER` | t=292s px=0.9900 kazanan_taraf_pahalı |
| 55 | 292s | Up | 1.06 | 0.9900 | 1.0000 | 1.001 | 9.38 | `SCOOP_WINNER` | t=292s px=0.9900 kazanan_taraf_pahalı |
| 56 | 292s | Up | 1.06 | 0.9900 | 1.0000 | 1.001 | 9.38 | `SCOOP_WINNER` | t=292s px=0.9900 kazanan_taraf_pahalı |
| 57 | 292s | Up | 275.27 | 0.9900 | 1.0000 | 1.001 | 9.38 | `SCOOP_WINNER` | t=292s px=0.9900 kazanan_taraf_pahalı |
| 58 | 304s | Up | 1.06 | 0.9900 | - | - | - | `OTHER` | tick_eşleşmedi |

### Epoch 1777468500 (DOWN) — 61 trade

| idx | t_off | outcome | size | price | tick_ask | pair_cost | signal | phase | reason |
|---|---|---|---|---|---|---|---|---|---|
| 0 | 20s | Down | 40.00 | 0.5025 | 0.4800 | 1.010 | 6.32 | `BRACKET` | px≈ask(0.4800) sz=40 pc=1.010 |
| 1 | 24s | Up | 40.00 | 0.4957 | 0.5900 | 1.010 | 6.51 | `BRACKET?` | fiyat_ok=False sz_ok=True px=0.4957 ask=0.5900 |
| 2 | 26s | Up | 5.68 | 0.5600 | 0.6000 | 1.010 | 7.27 | `BRACKET?` | fiyat_ok=False sz_ok=False px=0.5600 ask=0.6000 |
| 3 | 28s | Up | 35.78 | 0.5384 | 0.6200 | 1.010 | 7.27 | `BRACKET?` | fiyat_ok=False sz_ok=True px=0.5384 ask=0.6200 |
| 4 | 28s | Up | 9.67 | 0.5800 | 0.6200 | 1.010 | 7.27 | `BRACKET?` | fiyat_ok=False sz_ok=False px=0.5800 ask=0.6200 |
| 5 | 28s | Up | 5.00 | 0.5800 | 0.6200 | 1.010 | 7.27 | `BRACKET?` | fiyat_ok=False sz_ok=False px=0.5800 ask=0.6200 |
| 6 | 28s | Up | 2.50 | 0.5800 | 0.6200 | 1.010 | 7.27 | `BRACKET?` | fiyat_ok=False sz_ok=False px=0.5800 ask=0.6200 |
| 7 | 28s | Up | 11.90 | 0.5800 | 0.6200 | 1.010 | 7.27 | `BRACKET?` | fiyat_ok=False sz_ok=False px=0.5800 ask=0.6200 |
| 8 | 28s | Up | 9.33 | 0.5800 | 0.6200 | 1.010 | 7.27 | `BRACKET?` | fiyat_ok=False sz_ok=False px=0.5800 ask=0.6200 |
| 9 | 28s | Up | 2.57 | 0.5800 | 0.6200 | 1.010 | 7.27 | `BRACKET?` | fiyat_ok=False sz_ok=False px=0.5800 ask=0.6200 |
| 10 | 28s | Up | 42.00 | 0.5900 | 0.6200 | 1.010 | 7.27 | `BRACKET?` | fiyat_ok=False sz_ok=True px=0.5900 ask=0.6200 |
| 11 | 36s | Up | 45.00 | 0.6656 | 0.6900 | 1.010 | 7.63 | `LADDER?` | level_uzak dist=0.2156 nearest=0.45 px=0.6656 |
| 12 | 36s | Up | 46.00 | 0.6700 | 0.6900 | 1.010 | 7.63 | `LADDER?` | level_uzak dist=0.2200 nearest=0.45 px=0.6700 |
| 13 | 40s | Up | 49.00 | 0.6900 | 0.7300 | 1.010 | 7.89 | `LADDER?` | level_uzak dist=0.2400 nearest=0.45 px=0.6900 |
| 14 | 42s | Up | 52.00 | 0.7100 | 0.5900 | 1.010 | 8.00 | `LADDER?` | level_uzak dist=0.2600 nearest=0.45 px=0.7100 |
| 15 | 74s | Up | 40.00 | 0.4800 | 0.5800 | 1.010 | 6.83 | `LADDER?` | level_uzak dist=0.0300 nearest=0.45 px=0.4800 |
| 16 | 74s | Up | 40.00 | 0.4800 | 0.5800 | 1.010 | 6.83 | `LADDER?` | level_uzak dist=0.0300 nearest=0.45 px=0.4800 |
| 17 | 74s | Up | 0.93 | 0.5200 | 0.5800 | 1.010 | 6.83 | `LADDER?` | level_uzak dist=0.0700 nearest=0.45 px=0.5200 |
| 18 | 84s | Down | 40.00 | 0.4500 | 0.4900 | 1.010 | 7.18 | `LADDER` | level=0.45 dist=0.0000 maker_bid_fill |
| 19 | 84s | Down | 31.57 | 0.4579 | 0.4900 | 1.010 | 7.18 | `LADDER` | level=0.45 dist=0.0079 maker_bid_fill |
| 20 | 118s | Down | 20.00 | 0.4600 | 0.5800 | 1.010 | 6.01 | `LADDER` | level=0.45 dist=0.0100 maker_bid_fill |
| 21 | 118s | Down | 35.00 | 0.4800 | 0.5800 | 1.010 | 6.01 | `LADDER?` | level_uzak dist=0.0300 nearest=0.45 px=0.4800 |
| 22 | 132s | Up | 19.00 | 0.4900 | 0.6000 | 1.010 | 6.85 | `LADDER?` | level_uzak dist=0.0400 nearest=0.45 px=0.4900 |
| 23 | 132s | Up | 0.70 | 0.5000 | 0.6000 | 1.010 | 6.85 | `LADDER?` | level_uzak dist=0.0500 nearest=0.45 px=0.5000 |
| 24 | 140s | Up | 43.00 | 0.6300 | 0.6400 | 1.010 | 7.26 | `LADDER?` | level_uzak dist=0.1800 nearest=0.45 px=0.6300 |
| 25 | 142s | Up | 46.00 | 0.6300 | 0.6400 | 1.010 | 7.28 | `LADDER?` | level_uzak dist=0.1800 nearest=0.45 px=0.6300 |
| 26 | 142s | Up | 44.00 | 0.6300 | 0.6400 | 1.010 | 7.28 | `LADDER?` | level_uzak dist=0.1800 nearest=0.45 px=0.6300 |
| 27 | 156s | Up | 51.00 | 0.7002 | 0.6800 | 1.010 | 7.60 | `LADDER?` | level_uzak dist=0.2502 nearest=0.45 px=0.7002 |
| 28 | 158s | Down | 51.00 | 0.2500 | 0.3500 | 1.010 | 7.41 | `LADDER` | level=0.25 dist=0.0000 maker_bid_fill |
| 29 | 166s | Up | 46.00 | 0.6600 | 0.7100 | 1.010 | 7.34 | `LADDER?` | level_uzak dist=0.2100 nearest=0.45 px=0.6600 |
| 30 | 166s | Up | 49.00 | 0.6772 | 0.7100 | 1.010 | 7.34 | `LADDER?` | level_uzak dist=0.2272 nearest=0.45 px=0.6772 |
| 31 | 172s | Down | 44.00 | 0.3112 | 0.3800 | 1.010 | 6.41 | `LADDER` | level=0.30 dist=0.0112 maker_bid_fill |
| 32 | 182s | Up | 16.13 | 0.6900 | 0.7300 | 1.010 | 6.62 | `DIRECTIONAL?` | t=182s trend=+0.230 sz=16 px=0.6900 trend_küçük=0.230 |
| 33 | 194s | Up | 58.00 | 0.7600 | 0.8300 | 1.010 | 7.17 | `DIRECTIONAL` | t=194s trend=+0.330 sz=58 px=0.7600 |
| 34 | 196s | Up | 11.11 | 0.7900 | 0.8400 | 1.010 | 7.23 | `DIRECTIONAL?` | t=196s trend=+0.340 sz=11 px=0.7900 trend_küçük=0.340 |
| 35 | 196s | Up | 41.49 | 0.7900 | 0.8400 | 1.010 | 7.23 | `DIRECTIONAL` | t=196s trend=+0.340 sz=41 px=0.7900 |
| 36 | 196s | Up | 5.00 | 0.7900 | 0.8400 | 1.010 | 7.23 | `DIRECTIONAL?` | t=196s trend=+0.340 sz=5 px=0.7900 trend_küçük=0.340 |
| 37 | 196s | Up | 71.00 | 0.7940 | 0.8400 | 1.010 | 7.23 | `DIRECTIONAL` | t=196s trend=+0.340 sz=71 px=0.7940 |
| 38 | 232s | Down | 12.60 | 0.1300 | 0.1500 | 1.010 | 6.29 | `DIRECTIONAL_HEDGE` | t=232s trend=+0.360 sz=13 px=0.1300 ters_yön |
| 39 | 232s | Down | 14.00 | 0.1300 | 0.1500 | 1.010 | 6.29 | `DIRECTIONAL_HEDGE` | t=232s trend=+0.360 sz=14 px=0.1300 ters_yön |
| 40 | 232s | Down | 22.99 | 0.1300 | 0.1500 | 1.010 | 6.29 | `DIRECTIONAL_HEDGE` | t=232s trend=+0.360 sz=23 px=0.1300 ters_yön |
| 41 | 232s | Down | 8.00 | 0.1300 | 0.1500 | 1.010 | 6.29 | `DIRECTIONAL_HEDGE` | t=232s trend=+0.360 sz=8 px=0.1300 ters_yön |
| 42 | 232s | Down | 14.00 | 0.1300 | 0.1500 | 1.010 | 6.29 | `DIRECTIONAL_HEDGE` | t=232s trend=+0.360 sz=14 px=0.1300 ters_yön |
| 43 | 232s | Down | 14.00 | 0.1300 | 0.1500 | 1.010 | 6.29 | `DIRECTIONAL_HEDGE` | t=232s trend=+0.360 sz=14 px=0.1300 ters_yön |
| 44 | 232s | Down | 1.15 | 0.1300 | 0.1500 | 1.010 | 6.29 | `DIRECTIONAL_HEDGE` | t=232s trend=+0.360 sz=1 px=0.1300 ters_yön |
| 45 | 232s | Down | 1.25 | 0.1300 | 0.1500 | 1.010 | 6.29 | `DIRECTIONAL_HEDGE` | t=232s trend=+0.360 sz=1 px=0.1300 ters_yön |
| 46 | 232s | Down | 78.00 | 0.1400 | 0.1500 | 1.010 | 6.29 | `DIRECTIONAL_HEDGE` | t=232s trend=+0.360 sz=78 px=0.1400 ters_yön |
| 47 | 232s | Up | 122.00 | 0.8956 | 0.8600 | 1.010 | 6.29 | `DIRECTIONAL` | t=232s trend=+0.360 sz=122 px=0.8956 |
| 48 | 232s | Up | 154.00 | 0.9209 | 0.8600 | 1.010 | 6.29 | `DIRECTIONAL` | t=232s trend=+0.360 sz=154 px=0.9209 |
| 49 | 242s | Down | 58.00 | 0.2100 | 0.2000 | 1.010 | 5.86 | `DIRECTIONAL_HEDGE` | t=242s trend=+0.310 sz=58 px=0.2100 ters_yön |
| 50 | 256s | Up | 50.41 | 0.9400 | 0.9500 | 1.010 | 6.04 | `DIRECTIONAL` | t=256s trend=+0.450 sz=50 px=0.9400 |
| 51 | 264s | Up | 344.00 | 0.9634 | 0.9800 | 1.010 | 6.07 | `DIRECTIONAL` | t=264s trend=+0.480 sz=344 px=0.9634 |
| 52 | 264s | Up | 510.00 | 0.9733 | 0.9800 | 1.010 | 6.07 | `DIRECTIONAL` | t=264s trend=+0.480 sz=510 px=0.9733 |
| 53 | 274s | Down | 344.00 | 0.0200 | 0.1300 | 1.010 | 5.39 | `DIRECTIONAL_HEDGE` | t=274s trend=+0.380 sz=344 px=0.0200 ters_yön |
| 54 | 282s | Down | 35.12 | 0.1300 | 0.0900 | 1.010 | 7.13 | `SCOOP` | t=282s px=0.1300 kaybeden_taraf_ucuz |
| 55 | 282s | Down | 11.49 | 0.1300 | 0.0900 | 1.010 | 7.13 | `SCOOP` | t=282s px=0.1300 kaybeden_taraf_ucuz |
| 56 | 282s | Down | 5.75 | 0.1300 | 0.0900 | 1.010 | 7.13 | `SCOOP` | t=282s px=0.1300 kaybeden_taraf_ucuz |
| 57 | 282s | Down | 34.47 | 0.1300 | 0.0900 | 1.010 | 7.13 | `SCOOP` | t=282s px=0.1300 kaybeden_taraf_ucuz |
| 58 | 282s | Down | 1.15 | 0.1300 | 0.0900 | 1.010 | 7.13 | `SCOOP` | t=282s px=0.1300 kaybeden_taraf_ucuz |
| 59 | 282s | Down | 83.00 | 0.1400 | 0.0900 | 1.010 | 7.13 | `SCOOP` | t=282s px=0.1400 kaybeden_taraf_ucuz |
| 60 | 282s | Up | 122.00 | 0.8765 | 0.9200 | 1.010 | 7.13 | `SCOOP_WINNER` | t=282s px=0.8765 kazanan_taraf_pahalı |

---
*Üretildi: scripts/verify_aras_logs.py v1.0*