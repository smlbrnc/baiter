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
      "UP/DOWN maker bid ile spread arbitrajı; envanter dengesi, profit-lock, momentum (composite skor) ve StopTrade’de sadece hedge.",
  },
  {
    id: "aras",
    label: "Aras",
    description:
      "DCA + Kademeli Hedge Arbitrajı: pahalı tarafı (>0.50) 2 sn'de bir DCA ile ortalar, fill'de ucuz tarafa bid-3t→bid-2t→bid-1t kademeli GTC hedge açar; avg_up+avg_down<1.00 → garantili kâr kilidi.",
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
