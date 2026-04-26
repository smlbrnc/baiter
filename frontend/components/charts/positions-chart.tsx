"use client";

import { useMemo } from "react";
import { Layers } from "lucide-react";
import {
  CartesianGrid,
  Line,
  LineChart,
  XAxis,
  YAxis,
} from "recharts";
import {
  Card,
  CardContent,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { cn } from "@/lib/utils";
import {
  ChartContainer,
  ChartTooltip,
  type ChartConfig,
} from "@/components/ui/chart";
import type { PnLSnapshot } from "@/lib/types";
import {
  CHART_MARGIN_TIGHT,
  CHART_TIME_X_AXIS_LAYOUT,
  fmtTickTime,
  fmtTooltipTime,
  SECTION_LABEL_CLASS,
  SIGNAL_PAIR_CARD_CLASS,
  SIGNAL_PAIR_CONTENT_CLASS,
  SIGNAL_PAIR_HEADER_CLASS,
  timeTicks,
  type SessionRange,
} from "@/lib/chart-utils";

interface Props {
  data: PnLSnapshot[];
  session: SessionRange | null;
}

interface Row {
  t: number;
  up: number;
  down: number;
  /** `avg_up + avg_down`; her iki taraf da boşsa `null` (çizgi kesilsin). */
  avg_sum: number | null;
}

const chartConfig = {
  up: { label: "up_filled", color: "oklch(0.58 0.17 155)" },
  down: { label: "down_filled", color: "oklch(0.58 0.2 25)" },
  avg_sum: { label: "avg_sum", color: "oklch(0.82 0.16 95)" },
} satisfies ChartConfig;

/**
 * `PnLSnapshot` zaman serisini saniye granülerliğine indirger;
 * `up_filled`/`down_filled` her trade fill'inde artar (kümülatif share),
 * dolayısıyla seri doğal olarak monoton non-decreasing step fonksiyonudur.
 * `avg_sum = avg_up + avg_down` (VWAP toplamı) ayrı sağ-eksen üzerinde gösterilir.
 */
function toRows(snaps: PnLSnapshot[]): Row[] {
  const out: Row[] = [];
  for (const p of snaps) {
    const t = Math.floor(p.ts_ms / 1000);
    const au = p.avg_up ?? null;
    const ad = p.avg_down ?? null;
    const avg_sum =
      au !== null || ad !== null ? (au ?? 0) + (ad ?? 0) : null;
    const row: Row = {
      t,
      up: p.up_filled,
      down: p.down_filled,
      avg_sum,
    };
    if (out.length && out[out.length - 1].t === t) {
      out[out.length - 1] = row;
    } else {
      out.push(row);
    }
  }
  return out;
}

/**
 * UP/DOWN pozisyon (filled shares) zaman serisi — her trade fill'inde
 * adımlanır. Tooltip'te `pairs = min(up, down)` türevi de gösterilir.
 */
export function PositionsChart({ data, session }: Props) {
  const rows = useMemo(() => toRows(data), [data]);
  const ticks = useMemo(
    () => (session ? timeTicks(session) : []),
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [session?.start, session?.end],
  );

  if (!session) return null;

  return (
    <Card className={cn(SIGNAL_PAIR_CARD_CLASS, "min-w-0")}>
      <CardHeader className={SIGNAL_PAIR_HEADER_CLASS}>
        <div className="flex min-w-0 items-center gap-1.5">
          <Layers
            className="text-muted-foreground size-3 shrink-0"
            aria-hidden
          />
          <CardTitle
            className={cn(SECTION_LABEL_CLASS, "normal-case tracking-[0.12em]")}
          >
            POSITIONS
          </CardTitle>
        </div>
      </CardHeader>
      <CardContent
        className={cn(
          SIGNAL_PAIR_CONTENT_CLASS,
          "flex min-h-0 flex-1 flex-col",
        )}
      >
        <ChartContainer
          config={chartConfig}
          className="aspect-auto h-full min-h-0 w-full flex-1"
        >
          <LineChart data={rows} margin={CHART_MARGIN_TIGHT}>
            <CartesianGrid vertical={false} strokeDasharray="3 3" />
            <XAxis
              dataKey="t"
              type="number"
              domain={[session.start, session.end]}
              ticks={ticks}
              allowDataOverflow
              tickFormatter={fmtTickTime}
              {...CHART_TIME_X_AXIS_LAYOUT}
            />
            <YAxis
              yAxisId="shares"
              tickFormatter={(v) => formatShares(Number(v))}
              tickLine={false}
              axisLine={false}
              width={40}
              domain={[0, "auto"]}
              allowDecimals={false}
            />
            <YAxis
              yAxisId="price"
              orientation="right"
              tickFormatter={(v) => Number(v).toFixed(3)}
              tickLine={false}
              axisLine={false}
              width={42}
              domain={[0, "auto"]}
            />
            <ChartTooltip
              content={({ active, payload }) => {
                if (!active || !payload?.length) return null;
                const row = payload[0]?.payload as Row | undefined;
                if (!row) return null;
                const pairs = Math.min(row.up, row.down);
                return (
                  <div
                    className={cn(
                      "border-border/35 bg-background grid min-w-40 gap-1.5 rounded-md border px-2.5 py-1.5 text-xs shadow-xl",
                    )}
                  >
                    <div className="font-medium">{fmtTooltipTime(row.t)}</div>
                    <div className="flex justify-between gap-4 leading-none">
                      <span className="text-emerald-700/80 dark:text-emerald-400/90">
                        up_filled
                      </span>
                      <span className="font-mono font-semibold tabular-nums text-emerald-600 dark:text-emerald-400">
                        {fmtShares(row.up)}
                      </span>
                    </div>
                    <div className="flex justify-between gap-4 leading-none">
                      <span className="text-rose-700/80 dark:text-rose-400/90">
                        down_filled
                      </span>
                      <span className="font-mono font-semibold tabular-nums text-rose-600 dark:text-rose-400">
                        {fmtShares(row.down)}
                      </span>
                    </div>
                    <div className="border-border/50 flex justify-between gap-4 border-t pt-1 leading-none">
                      <span className="text-violet-700/80 dark:text-violet-400/90">
                        pairs
                      </span>
                      <span className="font-mono font-semibold tabular-nums text-violet-600 dark:text-violet-400">
                        {fmtShares(pairs)}
                      </span>
                    </div>
                    {row.avg_sum !== null && (
                      <div className="flex justify-between gap-4 leading-none">
                        <span className="text-amber-600/80 dark:text-amber-300/90">
                          avg_sum
                        </span>
                        <span className="font-mono font-semibold tabular-nums text-amber-600 dark:text-amber-300">
                          {row.avg_sum.toFixed(4)}
                        </span>
                      </div>
                    )}
                  </div>
                );
              }}
            />
            <Line
              yAxisId="shares"
              type="stepAfter"
              dataKey="up"
              stroke="var(--color-up)"
              strokeWidth={2}
              dot={false}
              isAnimationActive={false}
            />
            <Line
              yAxisId="shares"
              type="stepAfter"
              dataKey="down"
              stroke="var(--color-down)"
              strokeWidth={2}
              dot={false}
              isAnimationActive={false}
            />
            <Line
              yAxisId="price"
              type="monotone"
              dataKey="avg_sum"
              stroke="var(--color-avg_sum)"
              strokeWidth={2}
              dot={false}
              connectNulls={false}
              isAnimationActive={false}
            />
          </LineChart>
        </ChartContainer>
      </CardContent>
    </Card>
  );
}

/** Y-ekseni etiketi: tam sayıya yakın değerleri integer, kesirlileri 2-haneli göster. */
function formatShares(v: number): string {
  if (!Number.isFinite(v)) return "0";
  return Number.isInteger(v) ? v.toString() : v.toFixed(2);
}

/** Tooltip değerleri: ≥ 1 share için 2 hane, çok küçükse 4 hane (parça fill için). */
function fmtShares(v: number): string {
  if (!Number.isFinite(v)) return "0";
  if (v === 0) return "0";
  return v >= 1 ? v.toFixed(2) : v.toFixed(4);
}
