import type { MarketAsset, MarketInterval } from "@/lib/market";
import { parseSlugPattern } from "@/lib/market";
import type { RunMode, Strategy } from "@/lib/types";

export const STRATEGY_OPTIONS: {
  id: Strategy;
  label: string;
  description: string;
  /** Backend `bot/ctx.rs::load` aktif olmayan stratejiyle bot'u başlatmaz. */
  disabled?: boolean;
}[] = [
  {
    id: "alis",
    label: "Alis",
    description:
      "Profit-lock öncelikli FSM: DeepTrade opener çifti, NormalTrade avg-down, AggTrade/FakTrade trend-pyramid; FAK ile aktif lock.",
  },
  {
    id: "elis",
    label: "Elis",
    description:
      "Dutch Book Bid Loop: up_bid + down_bid < $1.00 koşulunda her iki tarafı bid fiyatından al; 2sn döngüde dolmayanları biriktir, sonraki emirlere ekle.",
  },
  {
    id: "bonereaper",
    label: "Bonereaper",
    description:
      "5dk btc-updown için 1sn decision loop; hibrit composite + EMA sinyal yön kararı; Dutch Book + Signal (sabit order_usdc size, avg_sum<1.25); opt-in profit lock. BUY-only, çıkış REDEEM.",
  },
];

export const RUN_MODE_OPTIONS: {
  id: RunMode;
  label: string;
  description: string;
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
];

export function defaultBotDisplayName(
  assetLabel: string,
  interval: MarketInterval,
  strategy: Strategy,
): string {
  const opt = STRATEGY_OPTIONS.find((o) => o.id === strategy);
  return `${assetLabel} ${interval} ${opt?.label ?? strategy}`;
}

export const DEFAULT_MARKET: { asset: MarketAsset; interval: MarketInterval } =
  parseSlugPattern("btc-updown-5m-") ?? {
    asset: "btc",
    interval: "5m",
  };
