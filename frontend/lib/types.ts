// Backend `FrontendEvent` ile birebir eşleşen TS tipleri.
// Backend: src/ipc.rs

export type Outcome = "UP" | "DOWN";
export type Side = "BUY" | "SELL";

export type RunMode = "live" | "dryrun";
export type Strategy = "alis" | "elis" | "bonereaper";

/**
 * `bots.strategy_params` JSON sütunu — backend `config::StrategyParams`.
 * Tüm alanlar opsiyoneldir; `null`/`undefined` → backend `_or_default()` uygular.
 */
export interface StrategyParams {
  /**
   * Profit-lock canonical eşiği için kullanılan oran
   * (örn. 0.02 → avg_threshold 0.98). Default 0.02.
   */
  profit_lock_pct?: number | null;
  /** RTDS Chainlink window-delta sinyali aktif mi. Default true. */
  rtds_enabled?: boolean | null;
  /**
   * Composite skorda window_delta payı (0–1). Geri kalan Binance payı.
   * Default 0.70 (window_delta dominant).
   */
  window_delta_weight?: number | null;
  /**
   * Sinyal projeksiyon ileri-bakış süresi (sn, 0–30). Backend RTDS velocity'yi
   * bu süreyle çarpıp window_delta'ya ekler → 3-4 sn ileri tahmin.
   * Default 3.0. 0 → projeksiyon kapalı (eski davranış).
   */
  signal_lookahead_secs?: number | null;
  /**
   * Alis: opener GTC fiyat delta'sı (`best_ask + delta`). Skordan bağımsız,
   * sabit; skor sadece yön belirler. Default 0.01.
   */
  open_delta?: number | null;
  /**
   * Alis: AggTrade pyramid taker FAK delta'sı (`best_ask + delta`).
   * Default 0.015.
   */
  pyramid_agg_delta?: number | null;
  /**
   * Alis: FakTrade pyramid taker FAK delta'sı (daha agresif).
   * Default 0.025.
   */
  pyramid_fak_delta?: number | null;
  /**
   * Alis: pyramid emir başına USDC. `null` → opener `order_usdc` ile aynı.
   */
  pyramid_usdc?: number | null;

  // ── Elis Dutch Book Bid Loop (docs/gabagool.md) ──────────────────────────
  /** Taraf başına temel emir büyüklüğü (share). Default 20. */
  elis_max_buy_order_size?: number | null;
  /** Loop süresi: emir → iptal arası (ms). Default 2000. */
  elis_trade_cooldown_ms?: number | null;
  /** Pencere kapanmadan bu kadar saniye önce döngü durur. Default 30. */
  elis_stop_before_end_secs?: number | null;
  /**
   * P4 Improvement threshold: avg pair cost bu kadar düşmüyorsa emir verilmez.
   * min_improvement ≥ tick + slippage + fee/size. Default 0.005.
   */
  elis_min_improvement?: number | null;
  /**
   * P5 Vol filter: bid-ask spread bu eşiği aşarsa OB ince sayılır, NoOp.
   * Default 0.05.
   */
  elis_vol_threshold?: number | null;
  /**
   * P5 BSI filter: |BSI| bu eşiği aşarsa karşı taraf engellenir.
   * Default 0.50.
   */
  elis_bsi_filter_threshold?: number | null;
  /**
   * P2 Lock threshold: avg_up + avg_down bu değerin altına düşünce pozisyon
   * kilitli sayılır ve yeni emir verilmez. Default 0.98.
   */
  elis_lock_threshold?: number | null;
  /**
   * P6 Stale cleanup: emirler bu süreden (ms) eskiyse zorla iptal edilir.
   * Default 30000.
   */
  elis_max_order_age_ms?: number | null;

  // ── Bonereaper ───────────────────────────────────────────────────────────
  /**
   * Signal emirlerinde taker (ask) kullanılsın mı? Default true — live'da
   * anında fill. `false` ise best_bid'den maker GTC emir verilir.
   */
  bonereaper_signal_taker?: boolean | null;
  /**
   * Profit-lock için imbalance eşiği (share). |up_filled − down_filled| bu
   * değerin altında VE her iki tarafta da fill varsa profit_lock aktifse
   * yeni emir durur. Default 50.
   */
  bonereaper_profit_lock_imbalance?: number | null;
  /**
   * Signal yön onayı için kaç ardışık tick gerekli? K=1 (default) → anlık
   * karar (real bot uyumlu). K=2+ → yeni yön için K ardışık tick onayı.
   */
  bonereaper_signal_persistence_k?: number | null;
  /**
   * Polymarket UP_bid sinyalinin yön kararındaki ağırlığı [0, 1].
   * Hibrit formül: `signal × (1-w) + market × w`. 0 = sadece Binance/OKX;
   * 0.7 (default) = Polymarket dominant.
   */
  bonereaper_signal_w_market?: number | null;
  /**
   * Composite skor EMA smoothing α ∈ (0, 1]. 1.0 (default) = smoothing yok
   * (real bot uyumlu, anlık tepki). 0.5 → daha yumuşak ama yön değişiminde
   * gecikme.
   */
  bonereaper_signal_ema_alpha?: number | null;
  /**
   * Profit lock: aktif ise her iki tarafta da fill oluşup imbalance
   * `bonereaper_profit_lock_imbalance` altına düştüğünde yeni emir durur.
   * Pozisyon korunur. Default: false.
   */
  bonereaper_profit_lock?: boolean | null;
}

export interface BotRow {
  id: number;
  name: string;
  slug_pattern: string;
  strategy: Strategy;
  run_mode: RunMode;
  order_usdc: number;
  min_price: number;
  max_price: number;
  cooldown_threshold: number;
  start_offset: number;
  strategy_params: StrategyParams | null;
  state: string;
  last_active_ms: number | null;
  created_at_ms: number;
  updated_at_ms: number;
}

export interface LogRow {
  id: number;
  bot_id: number | null;
  level: string;
  message: string;
  ts_ms: number;
}

export interface SessionInfo {
  slug: string;
  start_ts: number;
  end_ts: number;
  state: string;
  title: string | null;
  image: string | null;
}

/** `/api/bots/:id/sessions` listesindeki tek satır. */
export interface SessionListItem {
  slug: string;
  start_ts: number;
  end_ts: number;
  state: string;
  cost_basis: number;
  up_filled: number;
  down_filled: number;
  realized_pnl: number | null;
  pnl_if_up: number | null;
  pnl_if_down: number | null;
  winning_outcome: string | null;
  is_live: boolean;
}

/** `/api/bots/:id/sessions` sayfalanmış cevap. */
export interface SessionListResponse {
  items: SessionListItem[];
  total: number;
  total_pnl: number | null;
  limit: number;
  offset: number;
}

/** `/api/bots/:id/sessions/:slug` — detay + Gamma cache + position. */
export interface SessionDetail {
  bot_id: number;
  slug: string;
  start_ts: number;
  end_ts: number;
  state: string;
  cost_basis: number;
  fee_total: number;
  up_filled: number;
  down_filled: number;
  realized_pnl: number | null;
  is_live: boolean;
  title: string | null;
  image: string | null;
}

/** `/api/bots/:id/sessions/:slug/ticks` — 1 sn cadence BBA + sinyal snapshot. */
export interface MarketTick {
  up_best_bid: number;
  up_best_ask: number;
  down_best_bid: number;
  down_best_ask: number;
  /** `skor × 5 + 5 ∈ [0, 10]`; 5.0 = nötr. */
  signal_score: number;
  /** Binance CVD imbalance ∈ [−1, +1]. */
  imbalance: number;
  /** OKX EMA momentum (bps, kırpılmamış). */
  momentum_bps: number;
  /** Birleşik sinyal skoru ∈ [−1, +1]; + = UP, − = DOWN. */
  skor: number;
  ts_ms: number;
}

/** `/api/bots/:id/sessions/:slug/trades` — DB tarafı `TradeRecord` ile birebir. */
export interface TradeRow {
  trade_id: string;
  bot_id: number;
  market_session_id: number | null;
  market: string | null;
  asset_id: string | null;
  taker_order_id: string | null;
  maker_orders: string | null;
  trader_side: string | null;
  side: string | null;
  outcome: string | null;
  size: number;
  price: number;
  status: string;
  fee: number;
  ts_ms: number;
  raw_payload: string | null;
}

export interface PnLSnapshot {
  cost_basis: number;
  fee_total: number;
  up_filled: number;
  down_filled: number;
  pnl_if_up: number;
  pnl_if_down: number;
  mtm_pnl: number;
  /** Eşleşen UP/DOWN çift sayısı = min(up_filled, down_filled). Doc §11. */
  pair_count: number;
  /** UP tarafı VWAP. */
  avg_up?: number | null;
  /** DOWN tarafı VWAP. */
  avg_down?: number | null;
  ts_ms: number;
}

export type FrontendEvent =
  | {
      kind: "BotStarted";
      bot_id: number;
      name: string;
      slug: string;
      ts_ms: number;
    }
  | {
      kind: "BotStopped";
      bot_id: number;
      ts_ms: number;
      reason: string;
    }
  | {
      kind: "SessionOpened";
      bot_id: number;
      slug: string;
      start_ts: number;
      end_ts: number;
      up_token_id: string;
      down_token_id: string;
    }
  | {
      kind: "SessionResolved";
      bot_id: number;
      slug: string;
      winning_outcome: string;
      ts_ms: number;
    }
  | {
      kind: "OrderPlaced";
      bot_id: number;
      order_id: string;
      outcome: Outcome;
      side: Side;
      price: number;
      size: number;
      order_type: string;
      ts_ms: number;
    }
  | {
      kind: "OrderCanceled";
      bot_id: number;
      order_id: string;
      ts_ms: number;
    }
  | {
      kind: "Fill";
      bot_id: number;
      trade_id: string;
      outcome: Outcome;
      side: Side;
      price: number;
      size: number;
      status: string;
      ts_ms: number;
    }
  | {
      /** 1 sn cadence book + sinyal snapshot'ı; session slug ile eşleştirilir. */
      kind: "TickSnapshot";
      bot_id: number;
      slug: string;
      up_best_bid: number;
      up_best_ask: number;
      down_best_bid: number;
      down_best_ask: number;
      /** `skor × 5 + 5 ∈ [0, 10]`; 5.0 = nötr. */
      signal_score: number;
      /** Binance CVD imbalance ∈ [−1, +1]. */
      imbalance: number;
      /** OKX EMA momentum (bps, kırpılmamış). */
      momentum_bps: number;
      /** Birleşik sinyal skoru ∈ [−1, +1]; + = UP, − = DOWN. */
      skor: number;
      ts_ms: number;
    }
  | {
      /** 1 sn cadence PnL snapshot; REST polling yerine kullanılır. */
      kind: "PnlUpdate";
      bot_id: number;
      slug: string;
      cost_basis: number;
      fee_total: number;
      up_filled: number;
      down_filled: number;
      pnl_if_up: number;
      pnl_if_down: number;
      mtm_pnl: number;
      pair_count: number;
      avg_up?: number | null;
      avg_down?: number | null;
      ts_ms: number;
    }
  | {
      /**
       * Alis profit-lock tetiklendi (`PositionOpen → Locked`).
       * `lock_method`: `"taker_fak"` | `"passive_hedge_fill"` | `"symmetric_fill"`.
       */
      kind: "ProfitLocked";
      bot_id: number;
      slug: string;
      avg_up: number;
      avg_down: number;
      expected_profit: number;
      lock_method: string;
      ts_ms: number;
    }
  | {
      kind: "Error";
      bot_id: number;
      message: string;
      ts_ms: number;
    };

/**
 * Polymarket kimlik girişi — kullanıcı yalnızca PK + signature_type +
 * (funder) verir. Backend Polymarket'ten L1 EIP-712 ile
 * `apiKey/secret/passphrase` türetir ve tam credential'ı saklar.
 */
export interface CredentialsInput {
  /** Polygon EOA private key (`0x...` veya çıplak hex). */
  private_key: string;
  /** 0 = EOA, 1 = POLY_PROXY, 2 = POLY_GNOSIS_SAFE. */
  signature_type: number;
  /** `signature_type ∈ {1,2}` ise zorunlu (proxy/safe adresi). */
  funder?: string | null;
  /** EIP-712 nonce (Polymarket tek nonce kullanır). Default 0. */
  nonce?: number;
}

export interface CreateBotReq {
  name: string;
  slug_pattern: string;
  strategy: Strategy;
  run_mode: RunMode;
  order_usdc: number;
  min_price: number;
  max_price: number;
  cooldown_threshold: number;
  start_offset: number;
  strategy_params?: StrategyParams;
  credentials?: CredentialsInput;
  auto_start?: boolean;
}

/**
 * PATCH /api/bots/:id — bot ayarlarını günceller (yalnızca STOPPED).
 *
 * `slug_pattern` ve `strategy` immutable; bot oluşturulurken belirlenir,
 * sonradan değiştirilemez (yeniden oluşturulması gerekir).
 */
export interface UpdateBotReq {
  name: string;
  run_mode: RunMode;
  order_usdc: number;
  min_price: number;
  max_price: number;
  cooldown_threshold: number;
  start_offset: number;
  strategy_params?: StrategyParams;
  credentials?: CredentialsInput;
}

/**
 * GET /api/settings/credentials yanıtı — display only.
 * Hassas alanlar (PK, L2 secret, apiKey, passphrase) hiçbir zaman
 * döndürülmez; yalnızca türetilmiş `poly_address`, sig_type, funder
 * meta'sı ve "kayıt var mı?" durumu döner.
 */
export interface GlobalCredentials {
  poly_address: string | null;
  signature_type: number;
  funder: string | null;
  has_credentials: boolean;
  updated_at_ms: number | null;
}

/** `StrategyParams` default'ları (`config::StrategyParams::*_or_default`). */
export const STRATEGY_PARAMS_DEFAULTS = {
  profit_lock_pct: 0.02,
  open_delta: 0.01,
  pyramid_agg_delta: 0.015,
  pyramid_fak_delta: 0.025,
  // Elis
  elis_max_buy_order_size: 20,
  elis_trade_cooldown_ms: 4000,
  elis_stop_before_end_secs: 30,
  elis_min_improvement: 0.005,
  elis_vol_threshold: 0.05,
  elis_bsi_filter_threshold: 0.50,
  elis_lock_threshold: 0.98,
  elis_max_order_age_ms: 30000,
  // Bonereaper
  bonereaper_signal_taker: true,
  bonereaper_profit_lock_imbalance: 50,
  bonereaper_signal_persistence_k: 1,
  bonereaper_signal_w_market: 0.7,
  bonereaper_signal_ema_alpha: 1.0,
  bonereaper_profit_lock: false,
} as const;
