import type { MarketAsset, MarketInterval } from "@/lib/market"
import { parseSlugPattern } from "@/lib/market"
import type { RunMode, Strategy } from "@/lib/types"

export const STRATEGY_OPTIONS: {
  id: Strategy
  label: string
  description: string
  /** Backend `bot/ctx.rs::load` aktif olmayan stratejiyle bot'u başlatmaz. */
  disabled?: boolean
}[] = [
  {
    id: "bonereaper",
    label: "Bonereaper",
    description:
      "5dk btc-updown için 1sn decision loop; hibrit composite + EMA sinyal yön kararı; Dutch Book + Signal (sabit order_usdc size, avg_sum<1.25); opt-in profit lock. BUY-only, çıkış REDEEM.",
  },
  {
    id: "gravie",
    label: "Gravie",
    description:
      "Bot 66 (Lively-Authenticity) davranış kopyası: 5sn karar tick'i, dual-side BUY-only FAK taker, mid-price civarı ucuz-taraf entry, 38sn second-leg guard, T-90 cutoff, sum_avg ≥ 1.05'te dur, balance < 0.30'da rebalance. Sinyal kullanmaz (saf order book reaktif).",
  },
  {
    id: "arbitrage",
    label: "Arbitrage",
    description:
      "Pure cross-leg sentetik dolar: bid_winner + bid_loser < cost_max (avg_sum<1) iken winner ve loser tarafa eşzamanlı FAK BID. Backtest %100 WR, ROI %4.35 (cost<0.95, mt=5, $100). Yön tahmini yok, matematiksel garanti.",
  },
]

export const RUN_MODE_OPTIONS: {
  id: RunMode
  label: string
  description: string
}[] = [
  {
    id: "dryrun",
    label: "DryRun",
    description: "Emir gönderilmez; güvenli deneme ve log.",
  },
  {
    id: "live",
    label: "Live",
    description: "Gerçek CLOB emirleri; kimlik bilgisi gerekir.",
  },
]

export function defaultBotDisplayName(
  assetLabel: string,
  interval: MarketInterval,
  strategy: Strategy
): string {
  const opt = STRATEGY_OPTIONS.find((o) => o.id === strategy)
  return `${assetLabel} ${interval} ${opt?.label ?? strategy}`
}

export const DEFAULT_MARKET: { asset: MarketAsset; interval: MarketInterval } =
  parseSlugPattern("btc-updown-5m-") ?? {
    asset: "btc",
    interval: "5m",
  }
