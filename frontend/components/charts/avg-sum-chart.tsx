"use client";

import { useMemo } from "react";
import { Ratio } from "lucide-react";
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
  avg_up: number;
  avg_down: number;
}

const chartConfig = {
  avg_up: { label: "avg_up", color: "oklch(0.58 0.17 155)" },
  avg_down: { label: "avg_down", color: "oklch(0.58 0.2 25)" },
} satisfies ChartConfig;

function toRows(snaps: PnLSnapshot[]): Row[] {
  const out: Row[] = [];
  for (const p of snaps) {
    const t = Math.floor(p.ts_ms / 1000);
    const row: Row = {
      t,
      avg_up: p.avg_yes ?? 0,
      avg_down: p.avg_no ?? 0,
    };
    if (out.length && out[out.length - 1].t === t) {
      out[out.length - 1] = row;
    } else {
      out.push(row);
    }
  }
  return out;
}

/** `avg_up` / `avg_down` (YES/NO VWAP) zaman serisi — `AVG SUM = avg_up + avg_down` tooltip'te. */
export function AvgSumChart({ data, session }: Props) {
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
          <Ratio
            className="text-muted-foreground size-3 shrink-0"
            aria-hidden
          />
          <CardTitle
            className={cn(SECTION_LABEL_CLASS, "normal-case tracking-[0.12em]")}
          >
            AVG SUM
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
              tickFormatter={(v) => Number(v).toFixed(3)}
              tickLine={false}
              axisLine={false}
              width={40}
              domain={[0, "auto"]}
            />
            <ChartTooltip
              content={({ active, payload }) => {
                if (!active || !payload?.length) return null;
                const row = payload[0]?.payload as Row | undefined;
                if (!row) return null;
                const sum = row.avg_up + row.avg_down;
                return (
                  <div
                    className={cn(
                      "border-border/35 bg-background grid min-w-40 gap-1.5 rounded-md border px-2.5 py-1.5 text-xs shadow-xl",
                    )}
                  >
                    <div className="font-medium">{fmtTooltipTime(row.t)}</div>
                    <div className="flex justify-between gap-4 leading-none">
                      <span className="text-emerald-700/80 dark:text-emerald-400/90">
                        avg_up
                      </span>
                      <span className="font-mono font-semibold tabular-nums text-emerald-600 dark:text-emerald-400">
                        {row.avg_up.toFixed(4)}
                      </span>
                    </div>
                    <div className="flex justify-between gap-4 leading-none">
                      <span className="text-rose-700/80 dark:text-rose-400/90">
                        avg_down
                      </span>
                      <span className="font-mono font-semibold tabular-nums text-rose-600 dark:text-rose-400">
                        {row.avg_down.toFixed(4)}
                      </span>
                    </div>
                    <div className="border-border/50 flex justify-between gap-4 border-t pt-1 leading-none">
                      <span className="text-violet-700/80 dark:text-violet-400/90">
                        avg_sum
                      </span>
                      <span className="font-mono font-semibold tabular-nums text-violet-600 dark:text-violet-400">
                        {sum.toFixed(4)}
                      </span>
                    </div>
                  </div>
                );
              }}
            />
            <Line
              type="monotone"
              dataKey="avg_up"
              stroke="var(--color-avg_up)"
              strokeWidth={2}
              dot={false}
              isAnimationActive={false}
            />
            <Line
              type="monotone"
              dataKey="avg_down"
              stroke="var(--color-avg_down)"
              strokeWidth={2}
              dot={false}
              isAnimationActive={false}
            />
          </LineChart>
        </ChartContainer>
      </CardContent>
    </Card>
  );
}
