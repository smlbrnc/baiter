"use client";

import { useMemo, useState } from "react";
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
import { useEventStream } from "@/lib/hooks";
import type { FrontendEvent } from "@/lib/types";
import { fmtTickTime, timeTicks, type SessionRange } from "@/lib/chart-utils";

interface Props {
  botId: number;
  session: SessionRange | null;
}

interface Row {
  t: number;
  yesSpread: number;
  noSpread: number;
  score: number;
}

const MAX_POINTS = 600;

const chartConfig = {
  yesSpread: { label: "YES spread", color: "var(--chart-1)" },
  noSpread: { label: "NO spread", color: "oklch(0.63 0.2 352)" },
  score: { label: "signal_score", color: "var(--chart-2)" },
} satisfies ChartConfig;

function trimRows(rows: Row[]): Row[] {
  if (rows.length <= MAX_POINTS) return rows;
  return rows.slice(rows.length - MAX_POINTS);
}

export function SpreadSignalChart({ botId, session }: Props) {
  const [rows, setRows] = useState<Row[]>([]);

  const filter = useMemo(
    () => (ev: FrontendEvent) =>
      ev.bot_id === botId &&
      (ev.kind === "BestBidAsk" || ev.kind === "SignalUpdate"),
    [botId],
  );

  useEventStream((ev) => {
    if (ev.kind === "BestBidAsk") {
      const t = Math.floor(ev.ts_ms / 1000);
      setRows((prev) => {
        const i = prev.findIndex((r) => r.t === t);
        const last = prev[prev.length - 1];
        const patch = {
          yesSpread: Math.max(0, ev.yes_best_ask - ev.yes_best_bid),
          noSpread: Math.max(0, ev.no_best_ask - ev.no_best_bid),
        };
        if (i >= 0) {
          const next = [...prev];
          next[i] = { ...next[i], ...patch, t };
          return trimRows(next);
        }
        const row: Row = {
          t,
          ...patch,
          score: last?.score ?? 5,
        };
        return trimRows([...prev, row]);
      });
      return;
    }
    if (ev.kind === "SignalUpdate") {
      const t = Math.floor(ev.ts_ms / 1000);
      setRows((prev) => {
        const i = prev.findIndex((r) => r.t === t);
        const last = prev[prev.length - 1];
        const patch = { score: ev.signal_score };
        if (i >= 0) {
          const next = [...prev];
          next[i] = { ...next[i], ...patch, t };
          return trimRows(next);
        }
        const row: Row = {
          t,
          yesSpread: last?.yesSpread ?? 0,
          noSpread: last?.noSpread ?? 0,
          ...patch,
        };
        return trimRows([...prev, row]);
      });
    }
  }, filter);

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
