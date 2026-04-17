"use client";

import { useMemo, useState } from "react";
import {
  CartesianGrid,
  Line,
  LineChart,
  ReferenceLine,
  XAxis,
  YAxis,
} from "recharts";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import {
  ChartContainer,
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
  score: number;
}

const MAX_POINTS = 600;

const chartConfig = {
  score: { label: "signal_score", color: "#60a5fa" },
} satisfies ChartConfig;

export function SignalChart({ botId, session }: Props) {
  const [rows, setRows] = useState<Row[]>([]);

  const filter = useMemo(
    () => (ev: FrontendEvent) =>
      ev.kind === "SignalUpdate" && ev.bot_id === botId,
    [botId],
  );

  useEventStream((ev) => {
    if (ev.kind !== "SignalUpdate") return;
    const row: Row = {
      t: Math.floor(ev.ts_ms / 1000),
      score: ev.signal_score,
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
        <CardTitle>Signal (Binance)</CardTitle>
        <CardDescription>
          CVD + BSI(Hawkes) + OFI → 0-10 normalize · 5 nötr
        </CardDescription>
      </CardHeader>
      <CardContent>
        <ChartContainer config={chartConfig} className="h-[180px] w-full">
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
              domain={[0, 10]}
              ticks={[0, 2.5, 5, 7.5, 10]}
              tickLine={false}
              axisLine={false}
              width={32}
            />
            <ReferenceLine y={5} stroke="var(--border)" strokeDasharray="4 4" />
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
            <Line
              type="monotone"
              dataKey="score"
              stroke="var(--color-score)"
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
