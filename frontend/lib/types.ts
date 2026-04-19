// Backend `FrontendEvent` ile birebir eşleşen TS tipleri.
// Backend: src/ipc.rs

export type Outcome = "UP" | "DOWN";
export type Side = "BUY" | "SELL";

export type RunMode = "live" | "dryrun";
export type Strategy = "dutch_book" | "harvest" | "prism";

export interface BotRow {
  id: number;
  name: string;
  slug_pattern: string;
  strategy: Strategy;
  run_mode: RunMode;
  order_usdc: number;
  signal_weight: number;
  min_price: number;
  max_price: number;
  cooldown_threshold: number;
  strategy_params: Record<string, unknown> | null;
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
  signal_weight: number;
  min_price: number;
  max_price: number;
  cooldown_threshold: number;
  strategy_params?: Record<string, unknown>;
  credentials?: Credentials;
  auto_start?: boolean;
}
