export type MarketAsset = "btc" | "eth" | "sol" | "xrp";
export type MarketInterval = "5m" | "15m" | "1h" | "4h";

export const ASSETS: { id: MarketAsset; label: string; logo: string }[] = [
  { id: "btc", label: "BTC", logo: "/BTC.webp" },
  { id: "eth", label: "ETH", logo: "/ETH.webp" },
  { id: "sol", label: "SOL", logo: "/SOL.webp" },
  { id: "xrp", label: "XRP", logo: "/XRP.webp" },
];

export const INTERVALS: { id: MarketInterval; label: string }[] = [
  { id: "5m", label: "5 dk" },
  { id: "15m", label: "15 dk" },
  { id: "1h", label: "1 saat" },
  { id: "4h", label: "4 saat" },
];

export function slugPattern(
  asset: MarketAsset,
  interval: MarketInterval,
): string {
  return `${asset}-updown-${interval}-`;
}

/** Prefix match so full stored slugs still resolve asset/interval. */
export function parseSlugPattern(
  s: string,
): { asset: MarketAsset; interval: MarketInterval } | null {
  const m = s.match(/^(btc|eth|sol|xrp)-updown-(5m|15m|1h|4h)-/);
  if (!m) return null;
  return {
    asset: m[1] as MarketAsset,
    interval: m[2] as MarketInterval,
  };
}

export function assetLogoForSlug(slugPattern: string): string {
  const p = parseSlugPattern(slugPattern);
  const id = p?.asset ?? "btc";
  return ASSETS.find((a) => a.id === id)?.logo ?? ASSETS[0].logo;
}
