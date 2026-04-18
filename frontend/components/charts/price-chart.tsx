"use client";

import { useMemo, useState } from "react";
import { CartesianGrid, Line, LineChart, XAxis, YAxis } from "recharts";
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
  yesBid: number;
  yesAsk: number;
  noBid: number;
  noAsk: number;
}

const MAX_POINTS = 600;

const chartConfig = {
  yesBid: { label: "YES bid", color: "var(--chart-1)" },
  yesAsk: { label: "YES ask", color: "var(--chart-2)" },
  noBid: { label: "NO bid", color: "oklch(0.58 0.22 352)" },
  noAsk: { label: "NO ask", color: "oklch(0.7 0.17 352)" },
} satisfies ChartConfig;

export function PriceChart({ botId, session }: Props) {
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
      yesBid: ev.yes_best_bid,
      yesAsk: ev.yes_best_ask,
      noBid: ev.no_best_bid,
      noAsk: ev.no_best_ask,
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
        <CardTitle>Price</CardTitle>
        <CardDescription>
          YES bid/ask (yeşil) · NO bid/ask (kırmızı) ·{" "}
          {fmtTickTime(session.start)} → {fmtTickTime(session.end)}
        </CardDescription>
      </CardHeader>
      <CardContent>
        <ChartContainer config={chartConfig} className="h-[300px] w-full">
          <LineChart
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
              domain={[0, 1]}
              tickFormatter={(v) => v.toFixed(2)}
              tickLine={false}
              axisLine={false}
              width={40}
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
            <Line
              type="monotone"
              dataKey="yesBid"
              stroke="var(--color-yesBid)"
              strokeWidth={3}
              dot={false}
              isAnimationActive={false}
            />
            <Line
              type="monotone"
              dataKey="yesAsk"
              stroke="var(--color-yesAsk)"
              strokeWidth={1}
              dot={false}
              isAnimationActive={false}
            />
            <Line
              type="monotone"
              dataKey="noBid"
              stroke="var(--color-noBid)"
              strokeWidth={3}
              dot={false}
              isAnimationActive={false}
            />
            <Line
              type="monotone"
              dataKey="noAsk"
              stroke="var(--color-noAsk)"
              strokeWidth={1}
              dot={false}
              isAnimationActive={false}
            />
          </LineChart>
        </ChartContainer>
      </CardContent>
    </Card>
  );
}
