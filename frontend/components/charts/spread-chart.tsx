"use client";

import { useMemo, useState } from "react";
import { Bar, BarChart, CartesianGrid, XAxis, YAxis } from "recharts";
import {
  Card,
  CardContent,
  CardDescription,
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
}

const MAX_POINTS = 600;

const chartConfig = {
  yesSpread: { label: "YES spread", color: "var(--chart-1)" },
  noSpread: { label: "NO spread", color: "oklch(0.63 0.2 352)" },
} satisfies ChartConfig;

export function SpreadChart({ botId, session }: Props) {
  const [rows, setRows] = useState<Row[]>([]);

  const filter = useMemo(
    () => (ev: FrontendEvent) =>
      ev.kind === "BestBidAsk" && ev.bot_id === botId,
    [botId],
  );

  useEventStream((ev) => {
    if (ev.kind !== "BestBidAsk") return;
    const row: Row = {
      t: Math.floor(ev.ts_ms / 1000),
      yesSpread: Math.max(0, ev.yes_best_ask - ev.yes_best_bid),
      noSpread: Math.max(0, ev.no_best_ask - ev.no_best_bid),
    };
    setRows((prev) => {
      const next =
        prev.length && prev[prev.length - 1].t === row.t
          ? [...prev.slice(0, -1), row]
          : [...prev, row];
      return next.length > MAX_POINTS
        ? next.slice(next.length - MAX_POINTS)
        : next;
    });
  }, filter);

  if (!session) return null;
  const ticks = timeTicks(session);

  return (
    <Card>
      <CardHeader>
        <CardTitle>Spread</CardTitle>
        <CardDescription>
          YES (yeşil) ve NO (kırmızı) spread histogramı
        </CardDescription>
      </CardHeader>
      <CardContent>
        <ChartContainer config={chartConfig} className="h-[200px] w-full">
          <BarChart
            data={rows}
            margin={{ top: 8, right: 8, left: 8, bottom: 8 }}
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
              tickFormatter={(v) => Number(v).toFixed(3)}
              tickLine={false}
              axisLine={false}
              width={44}
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
              dataKey="yesSpread"
              fill="var(--color-yesSpread)"
              fillOpacity={0.7}
              radius={[2, 2, 0, 0]}
              isAnimationActive={false}
            />
            <Bar
              dataKey="noSpread"
              fill="var(--color-noSpread)"
              fillOpacity={0.7}
              radius={[2, 2, 0, 0]}
              isAnimationActive={false}
            />
          </BarChart>
        </ChartContainer>
      </CardContent>
    </Card>
  );
}
