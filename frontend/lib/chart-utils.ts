/**
 * Chart ekseni yardımcıları. `start`/`end` Unix **saniye** — backend
 * `SessionOpened.start_ts`/`end_ts` ile aynı birim.
 */
export interface SessionRange {
  start: number;
  end: number;
}

/** `[start, end]` aralığında `count` adet eşit aralıklı tick. */
export function timeTicks(r: SessionRange, count = 6): number[] {
  const step = (r.end - r.start) / (count - 1);
  return Array.from({ length: count }, (_, i) =>
    Math.round(r.start + i * step),
  );
}

/** Unix saniye → `HH:MM:SS` (lokal). */
export function fmtTickTime(t: number): string {
  return new Date(t * 1000).toLocaleTimeString([], {
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
  });
}
