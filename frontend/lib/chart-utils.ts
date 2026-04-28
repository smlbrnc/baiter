/**
 * Chart ekseni yardımcıları. `start`/`end` Unix **saniye** — backend
 * `SessionOpened.start_ts`/`end_ts` ile aynı birim.
 */
export interface SessionRange {
  start: number;
  end: number;
}

/** `MarketZone::from_pct` geçişleri: Deep|Normal|Agg|Fak bitişleri (Stop öncesi). */
export const ZONE_PCT_BOUNDARIES = [0.1, 0.75, 0.9, 0.98] as const;

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

/**
 * Unix saniye → `HH:MM:SS` — **America/New_York (ET)** (tooltip için saniye dahil).
 * X ekseni `fmtTickTime` ile HH:MM kalır; tooltip kullanıcıya tam saniye doğruluğu verir.
 */
export function fmtTooltipTime(t: number): string {
  const d = new Date(t * 1000);
  const parts = new Intl.DateTimeFormat("en-US", {
    timeZone: "America/New_York",
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
    hour12: false,
  }).formatToParts(d);
  const v = (type: Intl.DateTimeFormatPartTypes) =>
    parts.find((p) => p.type === type)?.value ?? "00";
  return `${v("hour").padStart(2, "0")}:${v("minute")}:${v("second")}`;
}

/** Chart / tablo bölüm başlıkları — `BotSettingsCards` `CardDescription` ile aynı ölçü ve stil. */
export const SECTION_LABEL_CLASS =
  "font-sans text-[10px] font-normal leading-snug tracking-wider text-muted-foreground uppercase";

/**
 * `BinanceSignalPanel` + `PositionsChart` yan yana — kart gövdesi ve padding aynı.
 * Grid `items-stretch` ile satır yüksekliği eşitlenir; grafik alanı `flex-1` ile dolar.
 */
export const SIGNAL_PAIR_CARD_CLASS =
  "gap-0 py-0 h-full min-h-0 flex flex-col";
export const SIGNAL_PAIR_HEADER_CLASS =
  "flex flex-row items-center justify-between space-y-0 px-3.5 pb-2 pt-3";
export const SIGNAL_PAIR_CONTENT_CLASS = "px-3.5 pb-3.5 pt-0";

/**
 * Recharts dış margin — X altı `margin.bottom` yerine eksen `height` ile sıkı tutulur.
 * `PriceChart` üstte zone etiketleri için `CHART_MARGIN_PRICE` kullan.
 */
export const CHART_MARGIN_TIGHT = {
  top: 6,
  right: 8,
  left: 2,
  bottom: 0,
} as const;

/** `PriceChart` — üstte `ReferenceLine` zone etiketleri için ekstra boşluk. */
export const CHART_MARGIN_PRICE = {
  top: 22,
  right: 8,
  left: 2,
  bottom: 0,
} as const;

/** Tüm zaman (X) eksenlerinde ortak düzen — Recharts varsayılan geniş alt bandı daraltır. */
export const CHART_TIME_X_AXIS_LAYOUT = {
  tickLine: false,
  axisLine: false,
  minTickGap: 20,
  height: 18,
  tickMargin: 4,
  tick: { fontSize: 10, className: "fill-muted-foreground" },
} as const;

/** Eski sabit; yalnızca tek başına sabit yükseklik gerektiğinde kullanılabilir. */
export const PNL_ROW_CHART_HEIGHT_CLASS = "h-[140px]";
