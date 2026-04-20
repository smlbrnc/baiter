// Backend `FrontendEvent` ile birebir eşleşen TS tipleri.
// Backend: src/ipc.rs

export type Outcome = "UP" | "DOWN";
export type Side = "BUY" | "SELL";

export type RunMode = "live" | "dryrun";
export type Strategy = "dutch_book" | "harvest" | "prism";

/**
 * `bots.strategy_params` JSON sütunu — backend `config::StrategyParams`.
 * Tüm alanlar opsiyoneldir; `null`/`undefined` → backend `_or_default()` uygular.
 */
export interface StrategyParams {
  /**
   * Harvest hedge avg_threshold türetmek için kullanılan profit-lock oranı
   * (örn. 0.02 → avg_threshold 0.98). Default 0.02.
   */
  harvest_profit_lock_pct?: number | null;
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
  shares_yes: number;
  shares_no: number;
  realized_pnl: number | null;
  pnl_if_up: number | null;
  pnl_if_down: number | null;
  is_live: boolean;
}

/** `/api/bots/:id/sessions` sayfalanmış cevap. */
export interface SessionListResponse {
  items: SessionListItem[];
  total: number;
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
  shares_yes: number;
  shares_no: number;
  realized_pnl: number | null;
  is_live: boolean;
  title: string | null;
  image: string | null;
}

/** `/api/bots/:id/sessions/:slug/ticks` — 1 sn cadence BBA + Binance signal. */
export interface MarketTick {
  yes_best_bid: number;
  yes_best_ask: number;
  no_best_bid: number;
  no_best_ask: number;
  signal_score: number;
  bsi: number;
  ofi: number;
  cvd: number;
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
  shares_yes: number;
  shares_no: number;
  pnl_if_up: number;
  pnl_if_down: number;
  mtm_pnl: number;
  /** Eşleşen YES/NO çift sayısı = min(shares_yes, shares_no). Doc §11. */
  pair_count: number;
  /** YES tarafı VWAP (`avg_up`); eski snapshot yoksa 0. */
  avg_yes?: number | null;
  /** NO tarafı VWAP (`avg_down`). */
  avg_no?: number | null;
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
      yes_token_id: string;
      no_token_id: string;
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
      price: number;
      size: number;
      status: string;
      ts_ms: number;
    }
  | {
      kind: "BestBidAsk";
      bot_id: number;
      yes_best_bid: number;
      yes_best_ask: number;
      no_best_bid: number;
      no_best_ask: number;
      ts_ms: number;
    }
  | {
      kind: "SignalUpdate";
      bot_id: number;
      symbol: string;
      signal_score: number;
      bsi: number;
      ofi: number;
      cvd: number;
      ts_ms: number;
    }
  | {
      kind: "StateChanged";
      bot_id: number;
      state: string;
      ts_ms: number;
    }
  | {
      kind: "Error";
      bot_id: number;
      message: string;
      ts_ms: number;
    };

export interface Credentials {
  poly_address: string;
  poly_api_key: string;
  poly_passphrase: string;
  poly_secret: string;
  polygon_private_key: string;
  signature_type: number;
  funder?: string | null;
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
  credentials?: Credentials;
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
  credentials?: Credentials;
}

/** `StrategyParams` default'ları (`config::StrategyParams::*_or_default`). */
export const STRATEGY_PARAMS_DEFAULTS = {
  harvest_profit_lock_pct: 0.02,
  rtds_enabled: true,
  window_delta_weight: 0.7,
  signal_lookahead_secs: 3.0,
} as const;
