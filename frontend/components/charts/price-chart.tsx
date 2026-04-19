"use client";

import { useMemo } from "react";
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
  CardAction,
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
import {
  fmtTickTime,
  timeTicks,
  ZONE_BOUNDARY_LABELS,
  zoneBoundaryTimes,
  type SessionRange,
} from "@/lib/chart-utils";

interface Props {
  data: MarketTick[];
  session: SessionRange | null;
}

interface Row {
  t: number;
  yesBid: number;
  yesAsk: number;
  noBid: number;
  noAsk: number;
}

const chartConfig = {
  yesBid: { label: "YES bid", color: "var(--chart-1)" },
  yesAsk: { label: "YES ask", color: "var(--chart-2)" },
  noBid: { label: "NO bid", color: "oklch(0.58 0.22 352)" },
  noAsk: { label: "NO ask", color: "oklch(0.7 0.17 352)" },
} satisfies ChartConfig;

function fmtPx(v: number | undefined): string {
  if (v == null || Number.isNaN(v)) return "—";
  return v.toFixed(4);
}

/** MarketTick[] → Row[] (saniye granülaritesinde tekilleştirme). */
function toRows(ticks: MarketTick[]): Row[] {
  const out: Row[] = [];
  for (const tk of ticks) {
    const t = Math.floor(tk.ts_ms / 1000);
    const row: Row = {
      t,
      yesBid: tk.yes_best_bid,
      yesAsk: tk.yes_best_ask,
      noBid: tk.no_best_bid,
      noAsk: tk.no_best_ask,
    };
    if (out.length && out[out.length - 1].t === t) {
      out[out.length - 1] = row;
    } else {
      out.push(row);
    }
  }
  return out;
}

export function PriceChart({ data, session }: Props) {
  const rows = useMemo(() => toRows(data), [data]);

  if (!session) return null;
  const ticks = timeTicks(session);
  const zoneLines = zoneBoundaryTimes(session);
  const latest = rows.length > 0 ? rows[rows.length - 1] : null;

  return (
    <Card>
      <CardHeader>
        <CardTitle>Price</CardTitle>
        <CardAction className="flex flex-wrap items-end justify-end gap-x-5 gap-y-2">
          {(
            [
              { key: "yesBid", label: "YES bid" },
              { key: "yesAsk", label: "YES ask" },
              { key: "noBid", label: "NO bid" },
              { key: "noAsk", label: "NO ask" },
            ] as const
          ).map(({ key, label }) => {
            const color = chartConfig[key].color;
            return (
              <div key={key} className="text-right">
                <div className="text-muted-foreground text-[10px] leading-tight">
                  {label}
                </div>
                <div
                  className="font-mono text-sm tabular-nums"
                  style={{ color }}
                >
                  {fmtPx(latest?.[key])}
                </div>
              </div>
            );
          })}
        </CardAction>
      </CardHeader>
      <CardContent>
        <ChartContainer config={chartConfig} className="h-[220px] w-full">
          <LineChart
            data={rows}
            margin={{ top: 22, right: 8, left: 8, bottom: 8 }}
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
            {zoneLines.map((x, i) => (
              <ReferenceLine
                key={`zone-${x}`}
                x={x}
                stroke="var(--color-muted-foreground)"
                strokeDasharray="4 3"
                strokeWidth={1}
                ifOverflow="visible"
                label={{
                  value: ZONE_BOUNDARY_LABELS[i] ?? "",
                  position: "top",
                  fill: "var(--color-muted-foreground)",
                  fontSize: 10,
                  offset: 6,
                }}
              />
            ))}
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
