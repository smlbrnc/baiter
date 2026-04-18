"use client";

import { useEffect, useState } from "react";
import { Area, AreaChart, CartesianGrid, XAxis, YAxis } from "recharts";
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
import { api } from "@/lib/api";
import { fmtTickTime, timeTicks, type SessionRange } from "@/lib/chart-utils";

interface Props {
  botId: number;
  session: SessionRange | null;
  pollMs?: number;
}

interface Row {
  t: number;
  mtm: number;
}

const MAX_POINTS = 600;

const chartConfig = {
  mtm: { label: "mtm_pnl", color: "var(--chart-3)" },
} satisfies ChartConfig;

export function PnLChart({ botId, session, pollMs = 1000 }: Props) {
  const [rows, setRows] = useState<Row[]>([]);

  useEffect(() => {
    let cancelled = false;
    let last = -1;
    const tick = async () => {
      const p = await api.botPnl(botId);
      if (cancelled || !p || p.ts_ms === last) return;
      last = p.ts_ms;
      const row: Row = { t: Math.floor(p.ts_ms / 1000), mtm: p.mtm_pnl };
      setRows((prev) => {
        const next =
          prev.length && prev[prev.length - 1].t === row.t
            ? [...prev.slice(0, -1), row]
            : [...prev, row];
        return next.length > MAX_POINTS
          ? next.slice(next.length - MAX_POINTS)
          : next;
      });
    };
    tick();
    const t = setInterval(tick, pollMs);
    return () => {
      cancelled = true;
      clearInterval(t);
    };
  }, [botId, pollMs]);

  if (!session) return null;
  const ticks = timeTicks(session);

  return (
    <Card>
      <CardHeader>
        <CardTitle>PnL (mtm)</CardTitle>
        <CardDescription>1 sn polling</CardDescription>
      </CardHeader>
      <CardContent>
        <ChartContainer config={chartConfig} className="h-[200px] w-full">
          <AreaChart
            data={rows}
            margin={{ top: 8, right: 8, left: 8, bottom: 8 }}
          >
            <defs>
              <linearGradient id="mtmFill" x1="0" y1="0" x2="0" y2="1">
                <stop
                  offset="0%"
                  stopColor="var(--color-mtm)"
                  stopOpacity={0.5}
                />
                <stop
                  offset="100%"
                  stopColor="var(--color-mtm)"
                  stopOpacity={0}
                />
              </linearGradient>
            </defs>
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
              width={48}
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
            <Area
              type="monotone"
              dataKey="mtm"
              stroke="var(--color-mtm)"
              strokeWidth={2}
              fill="url(#mtmFill)"
              isAnimationActive={false}
            />
          </AreaChart>
        </ChartContainer>
      </CardContent>
    </Card>
  );
}
