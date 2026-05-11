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
      "OB-reaktif martingale + fiyat-bazlı winner injection. Winner ask $0.99'a gelince (zaman bağımsız) $200/shot × 20 = $4000 cap atar; loser tarafa $0.10-$0.20 lottery scalp toplar. BUY-only, çıkış REDEEM.",
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
  {
    id: "binance_latency",
    label: "Binance Latency",
    description:
      "Binance Spot BTC/USDT mid fiyat lag arbitrajı. Session başında BTC mid snapshot, her tick |delta|≥sig_thr ise BUY (delta>0 → UP, <0 → DOWN). Bot 91 backtest (665 session): sig=$50 mt=10 cd=3s → WR %89, ROI +%4.80, yıllık ~$1.14M. Yüksek WR'li directional latency arbitrajı.",
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
