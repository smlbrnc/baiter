"use client";

import { useMemo } from "react";
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
  ChartLegend,
  ChartLegendContent,
  ChartTooltip,
  ChartTooltipContent,
  type ChartConfig,
} from "@/components/ui/chart";
import type { MarketTick } from "@/lib/types";
import { fmtTickTime, timeTicks, type SessionRange } from "@/lib/chart-utils";

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

const chartConfig = {
  yesSpread: { label: "YES spread", color: "var(--chart-1)" },
  noSpread: { label: "NO spread", color: "oklch(0.63 0.2 352)" },
  score: { label: "signal_score", color: "var(--chart-2)" },
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

  if (!session) return null;
  const ticks = timeTicks(session);

  return (
    <Card>
      <CardHeader>
        <CardTitle>Spread &amp; Signal (Binance)</CardTitle>
      </CardHeader>
      <CardContent>
        <ChartContainer config={chartConfig} className="h-[220px] w-full">
          <ComposedChart
            data={rows}
            margin={{ top: 8, right: 12, left: 8, bottom: 8 }}
          >
            <CartesianGrid vertical={false} strokeDasharray="3 3" />
            <XAxis
              dataKey="t"
              type="number"
              domain={[session.start, session.end]}
              ticks={ticks}
              allowDataOverflow
              tickFormatter={fmtTickTime}
              tickLine={false}
              axisLine={false}
              minTickGap={20}
            />
            <YAxis
              yAxisId="spread"
              orientation="left"
              domain={[0, 0.1]}
              ticks={[0, 0.025, 0.05, 0.075, 0.1]}
              tickFormatter={(v) => Number(v).toFixed(2)}
              tickLine={false}
              axisLine={false}
              width={44}
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
                    return typeof t === "number" ? fmtTickTime(t) : "";
                  }}
                />
              }
            />
            <ChartLegend content={<ChartLegendContent />} />
            <Bar
              yAxisId="spread"
              dataKey="yesSpread"
              fill="var(--color-yesSpread)"
              fillOpacity={0.65}
              radius={[2, 2, 0, 0]}
              isAnimationActive={false}
            />
            <Bar
              yAxisId="spread"
              dataKey="noSpread"
              fill="var(--color-noSpread)"
              fillOpacity={0.65}
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
