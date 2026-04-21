"use client";

import { useMemo } from "react";
import { Activity } from "lucide-react";
import {
  Bar,
  CartesianGrid,
  ComposedChart,
  Line,
  ReferenceLine,
  XAxis,
  YAxis,
} from "recharts";
import {
  Card,
  CardContent,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import {
  ChartContainer,
  ChartTooltip,
  ChartTooltipContent,
  type ChartConfig,
} from "@/components/ui/chart";
import type { MarketTick } from "@/lib/types";
import { cn } from "@/lib/utils";
import {
  CHART_MARGIN_TIGHT,
  CHART_TIME_X_AXIS_LAYOUT,
  fmtTickTime,
  fmtTooltipTime,
  SECTION_LABEL_CLASS,
  timeTicks,
  type SessionRange,
} from "@/lib/chart-utils";

interface Props {
  data: MarketTick[];
  session: SessionRange | null;
}

interface Row {
  t: number;
  yesSpread: number;
  noSpread: number;
  score: number;
}

/** Tema chart-* hepsi yeşil aile; burada seri başına ayırt edici renkler. */
const chartConfig = {
  yesSpread: { label: "YES spread", color: "oklch(0.58 0.16 245)" },
  noSpread: { label: "NO spread", color: "oklch(0.58 0.22 310)" },
  score: { label: "signal_score", color: "oklch(0.78 0.16 75)" },
} satisfies ChartConfig;

function toRows(ticks: MarketTick[]): Row[] {
  const out: Row[] = [];
  for (const tk of ticks) {
    const t = Math.floor(tk.ts_ms / 1000);
    const row: Row = {
      t,
      yesSpread: Math.max(0, tk.yes_best_ask - tk.yes_best_bid),
      noSpread: Math.max(0, tk.no_best_ask - tk.no_best_bid),
      score: tk.signal_score,
    };
    if (out.length && out[out.length - 1].t === t) {
      out[out.length - 1] = row;
    } else {
      out.push(row);
    }
  }
  return out;
}

export function SpreadSignalChart({ data, session }: Props) {
  const rows = useMemo(() => toRows(data), [data]);
  const ticks = useMemo(
    () => (session ? timeTicks(session) : []),
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [session?.start, session?.end],
  );

  if (!session) return null;

  return (
    <Card>
      <CardHeader>
        <CardTitle
          className={cn(SECTION_LABEL_CLASS, "flex items-center gap-1.5")}
        >
          <Activity className="size-3.5 shrink-0 opacity-80" aria-hidden />
          Spread &amp; Signal (Binance)
        </CardTitle>
      </CardHeader>
      <CardContent>
        <ChartContainer config={chartConfig} className="h-[220px] w-full">
          <ComposedChart data={rows} margin={CHART_MARGIN_TIGHT}>
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
              yAxisId="spread"
              orientation="left"
              domain={[0, 0.1]}
              ticks={[0, 0.025, 0.05, 0.075, 0.1]}
              tickFormatter={(v) => Number(v).toFixed(2)}
              tickLine={false}
              axisLine={false}
              width={40}
              allowDataOverflow
            />
            <YAxis
              yAxisId="signal"
              orientation="right"
              domain={[0, 10]}
              ticks={[0, 2.5, 5, 7.5, 10]}
              tickLine={false}
              axisLine={false}
              width={36}
            />
            <ReferenceLine
              yAxisId="signal"
              y={5}
              stroke="var(--border)"
              strokeDasharray="4 4"
            />
            <ChartTooltip
              content={
                <ChartTooltipContent
                  labelFormatter={(_v, p) => {
                    const t = p?.[0]?.payload?.t;
                    return typeof t === "number" ? fmtTooltipTime(t) : "";
                  }}
                />
              }
            />
            <Bar
              yAxisId="spread"
              dataKey="yesSpread"
              fill="var(--color-yesSpread)"
              fillOpacity={0.72}
              radius={[2, 2, 0, 0]}
              isAnimationActive={false}
            />
            <Bar
              yAxisId="spread"
              dataKey="noSpread"
              fill="var(--color-noSpread)"
              fillOpacity={0.72}
              radius={[2, 2, 0, 0]}
              isAnimationActive={false}
            />
            <Line
              yAxisId="signal"
              type="monotone"
              dataKey="score"
              stroke="var(--color-score)"
              strokeWidth={2}
              dot={false}
              isAnimationActive={false}
            />
          </ComposedChart>
        </ChartContainer>
      </CardContent>
    </Card>
  );
}
