// Backend `FrontendEvent` ile birebir eşleşen TS tipleri.
// Backend: src/ipc.rs

export type Outcome = "UP" | "DOWN"
export type Side = "BUY" | "SELL"

export type RunMode = "live" | "dryrun"
export type Strategy = "bonereaper" | "gravie" | "arbitrage" | "binance_latency"

/**
 * `bots.strategy_params` JSON sütunu — backend `config::StrategyParams`.
 * Tüm alanlar opsiyoneldir; `null`/`undefined` → backend `_or_default()` uygular.
 */
export interface StrategyParams {
  /** RTDS Chainlink window-delta sinyali aktif mi. Default true. */
  rtds_enabled?: boolean | null
  /**
   * Composite skorda window_delta payı (0–1). Geri kalan Binance payı.
   * Default 0.70 (window_delta dominant).
   */
  window_delta_weight?: number | null
  /**
   * Sinyal projeksiyon ileri-bakış süresi (sn, 0–30). Backend RTDS velocity'yi
   * bu süreyle çarpıp window_delta'ya ekler → 3-4 sn ileri tahmin.
   * Default 3.0. 0 → projeksiyon kapalı (eski davranış).
   */
  signal_lookahead_secs?: number | null

  // ── Bonereaper ───────────────────────────────────────────────────────────
  // Polymarket "Bonereaper" wallet (0xeebde7a0...) davranış kopyası.
  // Order-book reactive martingale + late winner injection. Sinyal kullanmaz.
  /**
   * Ardışık BUY emirleri arası min bekleme (ms). Real bot ~3-5 sn aralık;
   * default 2000 (~30 trade/dk).
   */
  bonereaper_buy_cooldown_ms?: number | null
  /**
   * Late winner penceresi (sn). T-X anında bid≥thr olan tarafa massive taker
   * BUY. 0 = kural KAPALI. Default 30.
   */
  bonereaper_late_winner_secs?: number | null
  /** Late winner için kazanan tarafın bid eşiği. Default 0.85. */
  bonereaper_late_winner_bid_thr?: number | null
  /**
   * Late winner trade büyüklüğü (USDC notional). Real bot 3 log analizinde
   * big-bet medyan $1000-1300. Default $1000. 0 = kural KAPALI.
   */
  bonereaper_late_winner_usdc?: number | null
  /**
   * Session başına max LW injection sayısı. Real bot 4-5 market'te 1 big-bet
   * (~0.2-0.33/market). Default 1. 0 = sınırsız (eski spam riski).
   */
  bonereaper_lw_max_per_session?: number | null
  /**
   * |up_filled − down_filled| bu eşiği aşarsa weaker side rebalance. Default 100.
   */
  bonereaper_imbalance_thr?: number | null
  /**
   * avg_sum yumuşak cap. `new_avg + opp_avg > X` ise yeni alım yok.
   * Real bot 1.20'ye kadar trade görüldü; default 1.30.
   */
  bonereaper_max_avg_sum?: number | null
  /**
   * İlk emir için minimum |up_bid - down_bid| spread eşiği. Bu eşik aşılmadan
   * BUY ATILMAZ; aşılınca ilk emir yüksek bid tarafına (winner momentum) verilir.
   * Sonraki emirler mevcut akışla devam eder. Default 0.02
   * (bot 101 backtest: ROI %1.41 → %2.56). 0.0 = devre dışı.
   */
  bonereaper_first_spread_min?: number | null
  /** Long-shot bid bucket (bid ≤ 0.30) trade büyüklüğü (USDC). Default 5. */
  bonereaper_size_longshot_usdc?: number | null
  /** Mid bid bucket (0.30 < bid ≤ 0.85) trade büyüklüğü (USDC). Default 10. */
  bonereaper_size_mid_usdc?: number | null
  /** High-confidence bid bucket (bid > 0.85) trade büyüklüğü (USDC). Default 15. */
  bonereaper_size_high_usdc?: number | null

  // ── Bonereaper - Aşama 3 (loser long-shot scalp) ─────────────────────────
  /**
   * Loser side için minimum bid eşiği (1¢ scalp). Winner tarafı genel
   * `min_price` ile sınırlı. Default 0.01 (real bot 0.01–0.05'te bilet topluyor).
   */
  bonereaper_loser_min_price?: number | null
  /** Loser side scalp USDC notional. Default $1. 0 = scalp KAPALI. */
  bonereaper_loser_scalp_usdc?: number | null
  /**
   * Loser scalp üst bid eşiği. Loser side `bid <= eşik` ise scalp boyutu
   * uygulanır (longshot bucket yerine). Default 0.30 (real bot 0.10-0.30
   * bandında bilet topluyor).
   */
  bonereaper_loser_scalp_max_price?: number | null

  // ── Bonereaper - Aşama 4 (winner pyramid scaling) ────────────────────────
  /**
   * T-X sn'den itibaren winner tarafa size çarpanı uygula. Default 100 sn.
   * 0 = scaling KAPALI.
   */
  bonereaper_late_pyramid_secs?: number | null
  /**
   * Winner tarafı için size çarpanı (T < late_pyramid_secs). Default 2.0
   * (real bot end-game 2-5× büyüklük).
   */
  bonereaper_winner_size_factor?: number | null

  // ── Bonereaper - Aşama 5 (multi-LW burst) ────────────────────────────────
  /** LW burst pencere (sn). T-X kala 2. dalga LW. Default 12. 0 = burst KAPALI. */
  bonereaper_lw_burst_secs?: number | null
  /** LW burst USDC. Default $200 (ana $500 LW'nin yarısı). */
  bonereaper_lw_burst_usdc?: number | null

  // ── Bonereaper - Aşama 6 (martingale-down guard) ─────────────────────────
  /**
   * Loser side avg fiyatı bu eşiği aşarsa o yöne sadece minimal scalp ($1).
   * Pahalı down-pyramid birikimini engeller. Default 0.50.
   */
  bonereaper_avg_loser_max?: number | null

  // ── Arbitrage (pure cross-leg FAK BID, avg_sum<1 garantili) ─────────────
  /** Tick check interval (ms). Default 1000. */
  arbitrage_tick_interval_ms?: number | null
  /** Maks bid_winner+bid_loser. Default 0.95 (%5 marj). 0.99 = fee yiyor. */
  arbitrage_cost_max?: number | null
  /** Order USDC per leg. Default 20. */
  arbitrage_order_usdc?: number | null
  /** Session başına max trade. Default 5. */
  arbitrage_max_trades_per_session?: number | null
  /** Trade'ler arası bekleme (ms). Default 5000. */
  arbitrage_cooldown_ms?: number | null
  /** Entry penceresi (T-X..T-0 arası ara). Default 300 (tüm pencere). */
  arbitrage_entry_window_secs?: number | null

  // ── Binance Latency Arbitrage ───────────────────────────────────────────
  // Polymarket BTC 5dk markete karşı Binance Spot BTC/USDT lag'ini sömürür.
  // Bot 91 backtest (665 session, 64h):
  //   - sig=$50 mt=10 cd=3s → WR %89, NET +$8323, ROI +%4.80, yıllık ~$1.14M
  //   - sig=$80 mt=3 cd=3s → WR %93, ROI +%9.11 (max ROI)
  //   - sig=$50 mt=50 cd=3s → WR %91, NET +$12808 (max NET)
  /** Sinyal eşiği (USD). |delta| ≥ X ise BUY. Default 50. */
  binance_latency_sig_thr_usd?: number | null
  /** Trade'ler arası min bekleme (ms). Default 3000. */
  binance_latency_cooldown_ms?: number | null
  /** Session başına max trade. Default 10. */
  binance_latency_max_trades_per_session?: number | null
  /** Order USDC notional. Default 100. */
  binance_latency_order_usdc?: number | null
  /** Entry penceresi (T-X..T-0). Default 300 (tüm pencere). */
  binance_latency_entry_window_secs?: number | null
  /**
   * Hedge leg notional (USDC). 0 = hedge KAPALI (default; backtest: hedge=$1
   * → NET −$375, hedge=$5 → NET −$2 628). Sadece tek-yön risk azaltmak
   * isteyenler için opt-in.
   */
  binance_latency_hedge_usdc?: number | null
  /**
   * Hedge için karşı taraf bid üst sınırı. Bid bu eşiğin altındaysa FAK BID
   * hedge alınır. Default 0.30.
   */
  binance_latency_hedge_max_bid?: number | null

  // ── Gravie (Bot 66 davranış kopyası) ─────────────────────────────────────
  /**
   * Karar tick aralığı (sn). Bot 66 ortalama inter-arrival 4-5 sn.
   * Default: 5.
   */
  gravie_tick_interval_secs?: number | null
  /** Ardışık BUY emirleri arası minimum bekleme (ms). Default: 4000. */
  gravie_buy_cooldown_ms?: number | null
  /**
   * Yeni leg açma için ask fiyat tavanı. Bot 66 first entry medyan 0.50,
   * p75 ≈ 0.575 — sıkı kalibrasyon. Default: 0.65.
   */
  gravie_entry_ask_ceiling?: number | null
  /**
   * Second-leg guard süresi (ms). İlk leg sonrası karşı tarafa
   * otomatik geçiş için min bekleme. Bot 66 5m median 38 sn. Default: 38000.
   */
  gravie_second_leg_guard_ms?: number | null
  /**
   * Second-leg karşı taraf fiyat tetikleyicisi — opp_ask bu eşiğin
   * altına inerse guard beklenmeden flip. Bot 66 opp_first_px ≈ 0.50.
   * Default: 0.55.
   */
  gravie_second_leg_opp_trigger?: number | null
  /**
   * Kapanışa bu kadar sn kala yeni emir verme. Bot 66 5m median T-78,
   * %58 ≤ T-90. Default: 90.
   */
  gravie_t_cutoff_secs?: number | null
  /**
   * Balance eşiği — `min/max` bunun altındaysa az tarafa zorunlu rebalance.
   * Default: 0.30 (sim'de 0.45 ile %42 trade rebalance idi; daralt).
   */
  gravie_balance_rebalance?: number | null
  /** Rebalance modunda entry ceiling esneme oranı. Default: 1.20. */
  gravie_rebalance_ceiling_multiplier?: number | null
  /**
   * Sum-avg guard — `avg_up + avg_dn ≥ X` ise yeni emir verme.
   * Default: 1.05 (sim'de 1.20 çok geç oluyor; sıkı tutarak overpay engellenir).
   */
  gravie_sum_avg_ceiling?: number | null
  /**
   * PATCH A — Lose-side ASK cap. `max(up_ask, dn_ask) ≥ X` ise tüm yeni
   * emirler durur. Default 0.85; 1.0 = devre dışı.
   */
  gravie_opp_ask_stop_threshold?: number | null
  /**
   * PATCH C — FAK emir başına maksimum share. Düşen fiyatlarda
   * `ceil(usdc/price)` patlamasını önler. 0 = sınırsız. Default: 50.
   */
  gravie_max_fak_size?: number | null
}

export interface BotRow {
  id: number
  name: string
  slug_pattern: string
  strategy: Strategy
  run_mode: RunMode
  order_usdc: number
  min_price: number
  max_price: number
  cooldown_threshold: number
  start_offset: number
  strategy_params: StrategyParams | null
  state: string
  last_active_ms: number | null
  created_at_ms: number
  updated_at_ms: number
}

export interface LogRow {
  id: number
  bot_id: number | null
  level: string
  message: string
  ts_ms: number
}

export interface SessionInfo {
  slug: string
  start_ts: number
  end_ts: number
  state: string
  title: string | null
  image: string | null
}

/** `/api/bots/:id/sessions` listesindeki tek satır. */
export interface SessionListItem {
  slug: string
  start_ts: number
  end_ts: number
  state: string
  cost_basis: number
  up_filled: number
  down_filled: number
  realized_pnl: number | null
  pnl_if_up: number | null
  pnl_if_down: number | null
  winning_outcome: string | null
  is_live: boolean
}

/** `/api/bots/:id/sessions` sayfalanmış cevap. */
export interface SessionListResponse {
  items: SessionListItem[]
  total: number
  total_pnl: number | null
  limit: number
  offset: number
}

/** `/api/bots/:id/sessions/:slug` — detay + Gamma cache + position. */
export interface SessionDetail {
  bot_id: number
  slug: string
  start_ts: number
  end_ts: number
  state: string
  cost_basis: number
  fee_total: number
  up_filled: number
  down_filled: number
  realized_pnl: number | null
  is_live: boolean
  title: string | null
  image: string | null
}

/** `/api/bots/:id/sessions/:slug/ticks` — 1 sn cadence BBA + sinyal snapshot. */
export interface MarketTick {
  up_best_bid: number
  up_best_ask: number
  down_best_bid: number
  down_best_ask: number
  /** `skor × 5 + 5 ∈ [0, 10]`; 5.0 = nötr. */
  signal_score: number
  /** Binance CVD imbalance ∈ [−1, +1]. */
  imbalance: number
  /** OKX EMA momentum (bps, kırpılmamış). */
  momentum_bps: number
  /** Birleşik sinyal skoru ∈ [−1, +1]; + = UP, − = DOWN. */
  skor: number
  ts_ms: number
}

/** `/api/bots/:id/sessions/:slug/trades` — DB tarafı `TradeRecord` ile birebir. */
export interface TradeRow {
  trade_id: string
  bot_id: number
  market_session_id: number | null
  market: string | null
  asset_id: string | null
  taker_order_id: string | null
  maker_orders: string | null
  trader_side: string | null
  side: string | null
  outcome: string | null
  size: number
  price: number
  status: string
  fee: number
  ts_ms: number
  raw_payload: string | null
}

export interface PnLSnapshot {
  cost_basis: number
  fee_total: number
  up_filled: number
  down_filled: number
  pnl_if_up: number
  pnl_if_down: number
  mtm_pnl: number
  /** Eşleşen UP/DOWN çift sayısı = min(up_filled, down_filled). Doc §11. */
  pair_count: number
  /** UP tarafı VWAP. */
  avg_up?: number | null
  /** DOWN tarafı VWAP. */
  avg_down?: number | null
  ts_ms: number
}

export type FrontendEvent =
  | {
      kind: "BotStarted"
      bot_id: number
      name: string
      slug: string
      ts_ms: number
    }
  | {
      kind: "BotStopped"
      bot_id: number
      ts_ms: number
      reason: string
    }
  | {
      kind: "SessionOpened"
      bot_id: number
      slug: string
      start_ts: number
      end_ts: number
      up_token_id: string
      down_token_id: string
    }
  | {
      kind: "SessionResolved"
      bot_id: number
      slug: string
      winning_outcome: string
      ts_ms: number
    }
  | {
      kind: "OrderPlaced"
      bot_id: number
      order_id: string
      outcome: Outcome
      side: Side
      price: number
      size: number
      order_type: string
      ts_ms: number
    }
  | {
      kind: "OrderCanceled"
      bot_id: number
      order_id: string
      ts_ms: number
    }
  | {
      kind: "Fill"
      bot_id: number
      trade_id: string
      outcome: Outcome
      side: Side
      price: number
      size: number
      status: string
      ts_ms: number
    }
  | {
      /** 1 sn cadence book + sinyal snapshot'ı; session slug ile eşleştirilir. */
      kind: "TickSnapshot"
      bot_id: number
      slug: string
      up_best_bid: number
      up_best_ask: number
      down_best_bid: number
      down_best_ask: number
      /** `skor × 5 + 5 ∈ [0, 10]`; 5.0 = nötr. */
      signal_score: number
      /** Binance CVD imbalance ∈ [−1, +1]. */
      imbalance: number
      /** OKX EMA momentum (bps, kırpılmamış). */
      momentum_bps: number
      /** Birleşik sinyal skoru ∈ [−1, +1]; + = UP, − = DOWN. */
      skor: number
      ts_ms: number
    }
  | {
      /** 1 sn cadence PnL snapshot; REST polling yerine kullanılır. */
      kind: "PnlUpdate"
      bot_id: number
      slug: string
      cost_basis: number
      fee_total: number
      up_filled: number
      down_filled: number
      pnl_if_up: number
      pnl_if_down: number
      mtm_pnl: number
      pair_count: number
      avg_up?: number | null
      avg_down?: number | null
      ts_ms: number
    }
  | {
      /**
       * Alis profit-lock tetiklendi (`PositionOpen → Locked`).
       * `lock_method`: `"taker_fak"` | `"passive_hedge_fill"` | `"symmetric_fill"`.
       */
      kind: "ProfitLocked"
      bot_id: number
      slug: string
      avg_up: number
      avg_down: number
      expected_profit: number
      lock_method: string
      ts_ms: number
    }
  | {
      kind: "Error"
      bot_id: number
      message: string
      ts_ms: number
    }

/**
 * Polymarket kimlik girişi — kullanıcı yalnızca PK + signature_type +
 * (funder) verir. Backend Polymarket'ten L1 EIP-712 ile
 * `apiKey/secret/passphrase` türetir ve tam credential'ı saklar.
 */
export interface CredentialsInput {
  /** Polygon EOA private key (`0x...` veya çıplak hex). */
  private_key: string
  /** 0 = EOA, 1 = POLY_PROXY, 2 = POLY_GNOSIS_SAFE. */
  signature_type: number
  /** `signature_type ∈ {1,2}` ise zorunlu (proxy/safe adresi). */
  funder?: string | null
  /** EIP-712 nonce (Polymarket tek nonce kullanır). Default 0. */
  nonce?: number
}

export interface CreateBotReq {
  name: string
  slug_pattern: string
  strategy: Strategy
  run_mode: RunMode
  order_usdc: number
  min_price: number
  max_price: number
  cooldown_threshold: number
  start_offset: number
  strategy_params?: StrategyParams
  credentials?: CredentialsInput
  auto_start?: boolean
}

/**
 * PATCH /api/bots/:id — bot ayarlarını günceller (yalnızca STOPPED).
 *
 * `slug_pattern` ve `strategy` immutable; bot oluşturulurken belirlenir,
 * sonradan değiştirilemez (yeniden oluşturulması gerekir).
 */
export interface UpdateBotReq {
  name: string
  run_mode: RunMode
  order_usdc: number
  min_price: number
  max_price: number
  cooldown_threshold: number
  start_offset: number
  strategy_params?: StrategyParams
  credentials?: CredentialsInput
}

/**
 * GET /api/settings/credentials yanıtı — display only.
 * Hassas alanlar (PK, L2 secret, apiKey, passphrase) hiçbir zaman
 * döndürülmez; yalnızca türetilmiş `poly_address`, sig_type, funder
 * meta'sı ve "kayıt var mı?" durumu döner.
 */
export interface GlobalCredentials {
  poly_address: string | null
  signature_type: number
  funder: string | null
  has_credentials: boolean
  updated_at_ms: number | null
}

/** `StrategyParams` default'ları (`config::StrategyParams::*_or_default`). */
export const STRATEGY_PARAMS_DEFAULTS = {
  // Bonereaper (realbot.log 3472-trade analizi: fiyat-bazlı LW, T-161s'de bile
  // winner ask $0.99'a gelince atla; küçük lot × 20 = $4000 cap/market)
  bonereaper_buy_cooldown_ms: 2000,
  bonereaper_late_winner_secs: 300,   // penceresiz — fiyat bazlı tetikleyici
  bonereaper_late_winner_bid_thr: 0.88, // winner bid $0.88+ = loser ~$0.11
  bonereaper_late_winner_usdc: 200,   // küçük lot × 20 = $4000 cap
  bonereaper_lw_max_per_session: 5,  // 5 × $200 = $1000 max LW cap
  bonereaper_imbalance_thr: 1000, // salınım önleme: 0 osc vs thr=200'de 14 osc
  bonereaper_max_avg_sum: 1.05,
  bonereaper_first_spread_min: 0.02,
  bonereaper_size_longshot_usdc: 8,   // bid ≤ 0.30, real avg $7.18
  bonereaper_size_mid_usdc: 20,       // 0.30 < bid ≤ 0.65, 40sh×$0.50=$20 (real mode=40sh)
  bonereaper_size_high_usdc: 40,      // bid>0.65 → ceil(40/bid): $0.75=54sh ≈ real 58sh
  // Loser long-shot scalp ($0.10–$0.20 bant, realbot $40–$450/market)
  bonereaper_loser_min_price: 0.01,
  bonereaper_loser_scalp_usdc: 5,
  bonereaper_loser_scalp_max_price: 0.20,
  // Winner pyramid (T-150s'den erken accumulation)
  bonereaper_late_pyramid_secs: 150,
  bonereaper_winner_size_factor: 2.0,  // $40×2=$80 @ $0.90 = ~89sh ≈ real 87sh ✓
  // LW burst — KAPALI (realbot ayrı burst wave kullanmıyor)
  bonereaper_lw_burst_secs: 0,
  bonereaper_lw_burst_usdc: 0,
  // Martingale-down guard
  bonereaper_avg_loser_max: 0.5,
  // Arbitrage (Bot 108 backtest optimum: cost<0.95, mt=5, $100 → WR %100)
  arbitrage_tick_interval_ms: 1000,
  arbitrage_cost_max: 0.95,
  arbitrage_order_usdc: 20,
  arbitrage_max_trades_per_session: 5,
  arbitrage_cooldown_ms: 5000,
  arbitrage_entry_window_secs: 300,
  // Binance Latency (Bot 91 backtest profil B: WR %89, NET +$8323, yıllık ~$1.14M)
  binance_latency_sig_thr_usd: 50,
  binance_latency_cooldown_ms: 3000,
  binance_latency_max_trades_per_session: 10,
  binance_latency_order_usdc: 100,
  binance_latency_entry_window_secs: 300,
  binance_latency_hedge_usdc: 0,
  binance_latency_hedge_max_bid: 0.30,
  // Gravie (Bot 66 davranış kopyası — optimum kalibre)
  gravie_tick_interval_secs: 5,
  gravie_buy_cooldown_ms: 4000,
  gravie_entry_ask_ceiling: 0.65,
  gravie_second_leg_guard_ms: 38000,
  gravie_second_leg_opp_trigger: 0.55,
  gravie_t_cutoff_secs: 90,
  gravie_balance_rebalance: 0.3,
  gravie_rebalance_ceiling_multiplier: 1.2,
  gravie_sum_avg_ceiling: 1.05,
  gravie_opp_ask_stop_threshold: 0.85,
  gravie_max_fak_size: 50,
} as const

/**
 * Bonereaper alanları `null`/eksikken UI'da `STRATEGY_PARAMS_DEFAULTS` gösterilir;
 * bu değerler `strategy_params` içine yazılmazsa API'ye `{}` gider ve backend
 * `unwrap_or(2000)` gibi farklı default kullanır. Seçim / kayıt öncesi merge et.
 */
export function mergeBonereaperStrategyDefaults(
  params?: StrategyParams | null
): StrategyParams {
  const p = params ?? {}
  const d = STRATEGY_PARAMS_DEFAULTS
  return {
    ...p,
    bonereaper_buy_cooldown_ms:
      p.bonereaper_buy_cooldown_ms ?? d.bonereaper_buy_cooldown_ms,
    bonereaper_late_winner_secs:
      p.bonereaper_late_winner_secs ?? d.bonereaper_late_winner_secs,
    bonereaper_late_winner_bid_thr:
      p.bonereaper_late_winner_bid_thr ?? d.bonereaper_late_winner_bid_thr,
    bonereaper_late_winner_usdc:
      p.bonereaper_late_winner_usdc ?? d.bonereaper_late_winner_usdc,
    bonereaper_lw_max_per_session:
      p.bonereaper_lw_max_per_session ?? d.bonereaper_lw_max_per_session,
    bonereaper_imbalance_thr:
      p.bonereaper_imbalance_thr ?? d.bonereaper_imbalance_thr,
    bonereaper_max_avg_sum:
      p.bonereaper_max_avg_sum ?? d.bonereaper_max_avg_sum,
    bonereaper_first_spread_min:
      p.bonereaper_first_spread_min ?? d.bonereaper_first_spread_min,
    bonereaper_size_longshot_usdc:
      p.bonereaper_size_longshot_usdc ?? d.bonereaper_size_longshot_usdc,
    bonereaper_size_mid_usdc:
      p.bonereaper_size_mid_usdc ?? d.bonereaper_size_mid_usdc,
    bonereaper_size_high_usdc:
      p.bonereaper_size_high_usdc ?? d.bonereaper_size_high_usdc,
    bonereaper_loser_min_price:
      p.bonereaper_loser_min_price ?? d.bonereaper_loser_min_price,
    bonereaper_loser_scalp_usdc:
      p.bonereaper_loser_scalp_usdc ?? d.bonereaper_loser_scalp_usdc,
    bonereaper_loser_scalp_max_price:
      p.bonereaper_loser_scalp_max_price ?? d.bonereaper_loser_scalp_max_price,
    bonereaper_late_pyramid_secs:
      p.bonereaper_late_pyramid_secs ?? d.bonereaper_late_pyramid_secs,
    bonereaper_winner_size_factor:
      p.bonereaper_winner_size_factor ?? d.bonereaper_winner_size_factor,
    bonereaper_lw_burst_secs:
      p.bonereaper_lw_burst_secs ?? d.bonereaper_lw_burst_secs,
    bonereaper_lw_burst_usdc:
      p.bonereaper_lw_burst_usdc ?? d.bonereaper_lw_burst_usdc,
    bonereaper_avg_loser_max:
      p.bonereaper_avg_loser_max ?? d.bonereaper_avg_loser_max,
  }
}

// ── Bot İstatistikleri ────────────────────────────────────────────────────

export interface PositionTypeStats {
  position_type: "SAF_UP" | "SAF_DOWN" | "KARMA"
  total: number
  winning: number
  losing: number
  winrate_pct: number
  avg_pnl: number
  total_pnl: number
  total_cost: number
  roi_pct: number
}

export interface SessionTimelineItem {
  session_id: number
  slug: string
  mtm_pnl: number
  cost_basis: number
  roi_pct: number
  position_type: "SAF_UP" | "SAF_DOWN" | "KARMA"
  ts_ms: number
}

export interface BotStats {
  total_sessions: number
  winning: number
  losing: number
  winrate_pct: number
  total_mtm_pnl: number
  total_cost_basis: number
  roi_pct: number
  total_fee: number
  avg_session_pnl: number
  best_session_pnl: number
  worst_session_pnl: number
  total_trades: number
  by_type: PositionTypeStats[]
  sessions_timeline: SessionTimelineItem[]
}
