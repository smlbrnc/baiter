"use client";

import { useMemo } from "react";
import { Area, AreaChart, CartesianGrid, XAxis, YAxis } from "recharts";
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
import { fmtTickTime, timeTicks, type SessionRange } from "@/lib/chart-utils";

interface Props {
  data: PnLSnapshot[];
  session: SessionRange | null;
}

interface Row {
  t: number;
  mtm: number;
  fee_total: number;
}

const chartConfig = {
  mtm: { label: "mtm_pnl", color: "var(--chart-3)" },
} satisfies ChartConfig;

function toRows(snaps: PnLSnapshot[]): Row[] {
  const out: Row[] = [];
  for (const p of snaps) {
    const t = Math.floor(p.ts_ms / 1000);
    const row: Row = { t, mtm: p.mtm_pnl, fee_total: p.fee_total };
    if (out.length && out[out.length - 1].t === t) {
      out[out.length - 1] = row;
    } else {
      out.push(row);
    }
  }
  return out;
}

export function PnLChart({ data, session }: Props) {
  const rows = useMemo(() => toRows(data), [data]);

  if (!session) return null;
  const ticks = timeTicks(session);

  return (
    <Card>
      <CardHeader>
        <CardTitle>PnL (mtm)</CardTitle>
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
              content={({ active, payload }) => {
                if (!active || !payload?.length) return null;
                const row = payload[0]?.payload as Row | undefined;
                if (!row) return null;
                return (
                  <div
                    className={cn(
                      "grid min-w-36 gap-1.5 rounded-md border border-border/35 bg-background px-2.5 py-1.5 text-xs shadow-xl",
                    )}
                  >
                    <div className="font-medium">{fmtTickTime(row.t)}</div>
                    <div className="flex justify-between gap-4 leading-none">
                      <span className="text-muted-foreground">mtm_pnl</span>
                      <span className="font-mono font-medium tabular-nums">
                        {row.mtm.toFixed(4)}
                      </span>
                    </div>
                    <div className="flex justify-between gap-4 leading-none">
                      <span className="text-muted-foreground">Fee total</span>
                      <span className="font-mono font-medium tabular-nums">
                        ${row.fee_total.toFixed(4)}
                      </span>
                    </div>
                  </div>
                );
              }}
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
