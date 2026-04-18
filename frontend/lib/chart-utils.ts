/**
 * Chart ekseni yardımcıları. `start`/`end` Unix **saniye** — backend
 * `SessionOpened.start_ts`/`end_ts` ile aynı birim.
 */
export interface SessionRange {
  start: number;
  end: number;
}

/** `MarketZone::from_pct` geçişleri: Deep|Normal|Agg|Fak bitişleri (Stop öncesi). */
export const ZONE_PCT_BOUNDARIES = [0.1, 0.5, 0.9, 0.97] as const;

/**
 * Sınır çizgisinin üst etiketi: çizgiyi geçince başlayan bölge
 * (DeepTrade öncesi ayrı çizgi yok; ilk çizgi NormalTrade başlangıcı).
 */
export const ZONE_BOUNDARY_LABELS: readonly string[] = [
  "NormalTrade",
  "AggTrade",
  "FakTrade",
  "StopTrade",
];

/** Session penceresinde zone sınırlarının unix saniye zamanları (x ekseni). */
export function zoneBoundaryTimes(r: SessionRange): number[] {
  const d = r.end - r.start;
  if (d <= 0) return [];
  return ZONE_PCT_BOUNDARIES.map((pct) => r.start + pct * d);
}

/** `[start, end]` aralığında `count` adet eşit aralıklı tick. */
export function timeTicks(r: SessionRange, count = 6): number[] {
  const step = (r.end - r.start) / (count - 1);
  return Array.from({ length: count }, (_, i) =>
    Math.round(r.start + i * step),
  );
}

/**
 * Unix saniye → `HH:MM` — **America/New_York (ET)** (saniye yok; x ekseni).
 * Polymarket market başlığındaki “…3:25PM-3:30PM ET” ile aynı saat dilimi.
 */
export function fmtTickTime(t: number): string {
  const d = new Date(t * 1000);
  const parts = new Intl.DateTimeFormat("en-US", {
    timeZone: "America/New_York",
    hour: "2-digit",
    minute: "2-digit",
    hour12: false,
  }).formatToParts(d);
  const v = (type: Intl.DateTimeFormatPartTypes) =>
    parts.find((p) => p.type === type)?.value ?? "00";
  return `${v("hour").padStart(2, "0")}:${v("minute")}`;
}
