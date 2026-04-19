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

/** Pencere uzunluğu saniye cinsinden — backend `Interval::seconds` ile birebir. */
export function intervalSecs(interval: MarketInterval): number {
  switch (interval) {
    case "5m":
      return 5 * 60;
    case "15m":
      return 15 * 60;
    case "1h":
      return 60 * 60;
    case "4h":
      return 4 * 60 * 60;
  }
}

/**
 * UI önizleme — `bots.start_offset`'i uygulayarak resolved tam slug üretir.
 * DB'ye gönderilen body hâlâ `slugPattern(...)` prefix'idir; backend start anında
 * aynı hesabı tekrar yapar (`parse_slug_or_prefix_with_offset`).
 */
export function previewSlug(
  asset: MarketAsset,
  interval: MarketInterval,
  startOffset: number,
  nowMs: number = Date.now(),
): string {
  const secs = intervalSecs(interval);
  const snap = Math.floor(nowMs / 1000 / secs) * secs;
  const ts = snap + Math.max(0, Math.trunc(startOffset)) * secs;
  return `${asset}-updown-${interval}-${ts}`;
}

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
