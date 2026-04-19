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
    id: "harvest",
    label: "Harvest",
    description: "Likidite ve fiyat farkından yararlanma odaklı.",
  },
  {
    id: "dutch_book",
    label: "Dutch book (TBD)",
    description: "İki taraflı defter ve dengeleyici emir mantığı — yakında.",
    disabled: true,
  },
  {
    id: "prism",
    label: "Prism (TBD)",
    description: "Sinyal ve ağırlıklarla ince ayarlı model — yakında.",
    disabled: true,
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
