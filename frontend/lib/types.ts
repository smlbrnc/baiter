// Backend `FrontendEvent` ile birebir eşleşen TS tipleri.
// Backend: src/ipc.rs

export type Outcome = "UP" | "DOWN"
export type Side = "BUY" | "SELL"

export type RunMode = "live" | "dryrun"
export type Strategy = "bonereaper" | "gravie"

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
  /** LW shot'ları arası min bekleme (ms). Default 10000. */
  bonereaper_lw_cooldown_ms?: number | null
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

  // ── Gravie (Dual-Balance Accumulator) ────────────────────────────────────
  // avg_up + avg_down < 1 garantisi + her iki tarafta eşit pay birikimi.
  // Sinyal kullanmaz; saf order-book reaktif, BUY-only FAK taker.
  /** Ardışık BUY emirleri arası minimum bekleme (ms). Default: 2000. */
  gravie_buy_cooldown_ms?: number | null
  /**
   * avg_up + avg_down yumuşak tavanı. Bu değerin üstünde yeni emir yok.
   * Default: 0.95.
   */
  gravie_avg_sum_max?: number | null
  /**
   * BUY yapılabilecek maksimum ask fiyatı. Default: 0.99.
   */
  gravie_max_ask?: number | null
  /**
   * Kapanışa bu kadar sn kala yeni emir verilmez. Default: 30.
   */
  gravie_t_cutoff_secs?: number | null
  /**
   * FAK emir başına maksimum share. Düşen fiyatlarda
   * `ceil(usdc/price)` patlamasını önler. 0 = sınırsız. Default: 50.
   */
  gravie_max_fak_size?: number | null
  /**
   * |up_filled − down_filled| bu eşiği aşarsa az olan tarafa rebalance.
   * Default: 5.
   */
  gravie_imb_thr?: number | null
  /**
   * Winner-momentum ilk giriş eşiği. İlk işlemde kazanan tarafın bid'i
   * bu değerin üstünde olmalı; yoksa giriş geciktirilir. Default: 0.65.
   */
  gravie_first_bid_min?: number | null
  /**
   * Loser-scalp bypass eşiği. ask ≤ bu değer ise avg_sum_max gate
   * atlanır; ucuz taraftan pozisyon dengelenir. Default: 0.30.
   */
  gravie_loser_bypass_ask?: number | null
  /**
   * Late Winner injection tetik eşiği (winner bid). `max(up_bid, dn_bid) ≥ X`
   * olduğunda kazanan tarafa büyük taker BUY. Default: 0.88 (Bonereaper ile aynı).
   */
  gravie_lw_bid_thr?: number | null
  /**
   * LW emri USDC çarpanı (`order_usdc × X × lw_mult`). Default: 2.0
   * (Bonereaper `2 × order_usdc` ile aynı).
   */
  gravie_lw_usdc_factor?: number | null
  /** Session başına maksimum LW shot. Default: 30. 0 = sınırsız. */
  gravie_lw_max_per_session?: number | null
  /**
   * Loser tarafta avg fiyat üst sınırı (martingale-down guard). own_avg
   * bu eşiği aşarsa o yöne yeni alım yapılmaz. Default: 0.50.
   */
  gravie_avg_loser_max?: number | null
  /**
   * Loser-scalp boyut çarpanı. `ask ≤ loser_bypass_ask` iken
   * `size = ceil(order_usdc × X / ask)` ile sabit küçük alım. Default: 0.5
   * (Bonereaper ile aynı). 0 = scalp kapalı, mevcut size_multiplier kullanılır.
   */
  gravie_loser_scalp_usdc_factor?: number | null
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

/** `StrategyParams` default'ları — Bot 209 OPT100 ayarları baz alındı. */
export const STRATEGY_PARAMS_DEFAULTS = {
  // Bonereaper
  bonereaper_buy_cooldown_ms: 2000,
  bonereaper_late_winner_secs: 300,   // penceresiz — fiyat bazlı tetikleyici
  bonereaper_late_winner_bid_thr: 0.90,
  bonereaper_late_winner_usdc: 10,    // otomatik: 1 × order_usdc; arb_mult 5×→15× lineer
  bonereaper_lw_max_per_session: 30,
  bonereaper_lw_cooldown_ms: 10000,
  bonereaper_imbalance_thr: 100,      // otomatik: 10 × order_usdc (frontend hidden)
  bonereaper_max_avg_sum: 1.0,
  bonereaper_first_spread_min: 0,     // devre dışı
  bonereaper_size_longshot_usdc: 6,   // bid ≤ 0.30
  bonereaper_size_mid_usdc: 10,       // 0.30 < bid ≤ 0.65
  bonereaper_size_high_usdc: 15,      // 0.65 < bid < 0.90
  // Loser long-shot scalp
  bonereaper_loser_min_price: 0.01,
  bonereaper_loser_scalp_usdc: 5,     // 0.5 × order_usdc
  bonereaper_loser_scalp_max_price: 0.30,
  // Winner pyramid — KAPALI (yanlış yön amplifikasyonu önler)
  bonereaper_late_pyramid_secs: 0,
  bonereaper_winner_size_factor: 1.0,
  // LW burst — KAPALI
  bonereaper_lw_burst_secs: 0,
  bonereaper_lw_burst_usdc: 0,
  // Martingale-down guard
  bonereaper_avg_loser_max: 0.5,
  // Gravie (Dual-Balance Accumulator + Bonereaper-harmonized)
  gravie_buy_cooldown_ms: 2000,
  gravie_avg_sum_max: 1.00,
  gravie_max_ask: 0.99,
  gravie_t_cutoff_secs: 30,
  gravie_max_fak_size: 50,
  gravie_imb_thr: 5,
  gravie_first_bid_min: 0.65,
  gravie_loser_bypass_ask: 0.30,
  gravie_lw_bid_thr: 0.88,
  gravie_lw_usdc_factor: 2.0,
  gravie_lw_max_per_session: 30,
  gravie_avg_loser_max: 0.50,
  gravie_loser_scalp_usdc_factor: 0.5,
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
    bonereaper_lw_cooldown_ms:
      p.bonereaper_lw_cooldown_ms ?? d.bonereaper_lw_cooldown_ms,
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
