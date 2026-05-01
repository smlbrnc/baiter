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
  upSpread: number;
  downSpread: number;
  /** Kümülatif skor: her tick'in skor'u bir öncekine eklenir. */
  cSkor: number;
  /** Binance CVD imbalance ∈ [−1, +1]. */
  imbalance: number;
  /** OKX EMA momentum normalize: clip(bps, −5, +5) / 5 ∈ [−1, +1]. */
  momNorm: number;
}

const chartConfig = {
  upSpread:  { label: "UP spread",   color: "oklch(0.58 0.16 245)" },
  downSpread:{ label: "DOWN spread", color: "oklch(0.58 0.22 310)" },
  cSkor:     { label: "cum.skor",    color: "oklch(0.82 0.18 75)"  },
  imbalance: { label: "imbalance",   color: "oklch(0.68 0.17 155)" },
  momNorm:   { label: "mom (norm)",  color: "oklch(0.72 0.15 295)" },
} satisfies ChartConfig;

const MOM_CAP = 5;

function toRows(ticks: MarketTick[]): { rows: Row[]; cMin: number; cMax: number } {
  const rows: Row[] = [];
  let cumul = 0;
  let cMin = 0;
  let cMax = 0;

  for (const tk of ticks) {
    const t = Math.floor(tk.ts_ms / 1000);
    cumul += tk.skor ?? 0;
    if (cumul < cMin) cMin = cumul;
    if (cumul > cMax) cMax = cumul;

    const row: Row = {
      t,
      upSpread:   Math.max(0, tk.up_best_ask   - tk.up_best_bid),
      downSpread: Math.max(0, tk.down_best_ask  - tk.down_best_bid),
      cSkor:      cumul,
      imbalance:  tk.imbalance ?? 0,
      momNorm:    Math.max(-1, Math.min(1, (tk.momentum_bps ?? 0) / MOM_CAP)),
    };
    if (rows.length && rows[rows.length - 1].t === t) {
      rows[rows.length - 1] = row;
    } else {
      rows.push(row);
    }
  }

  return { rows, cMin, cMax };
}

/** Kümülatif eksen için yuvarlak, simetrik domain. */
function cumulDomain(cMin: number, cMax: number): [number, number] {
  const abs = Math.max(Math.abs(cMin), Math.abs(cMax), 1);
  const pad = abs * 0.15;
  return [-(abs + pad), abs + pad];
}

export function SpreadSignalChart({ data, session }: Props) {
  const { rows, cMin, cMax } = useMemo(() => toRows(data), [data]);
  const ticks = useMemo(
    () => (session ? timeTicks(session) : []),
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [session?.start, session?.end],
  );
  const [dMin, dMax] = useMemo(() => cumulDomain(cMin, cMax), [cMin, cMax]);

  if (!session) return null;

  return (
    <Card>
      <CardHeader>
        <div className="flex items-center justify-between">
          <CardTitle className={cn(SECTION_LABEL_CLASS, "flex items-center gap-1.5")}>
            <Activity className="size-3.5 shrink-0 opacity-80" aria-hidden />
            Spread &amp; Signal
          </CardTitle>
          {/* Legend */}
          <div className="flex items-center gap-3 font-mono text-[9px] text-muted-foreground/70">
            <span className="flex items-center gap-1">
              <span className="inline-block h-0.5 w-5 rounded" style={{ background: "var(--color-cSkor)" }} />
              cum.skor
            </span>
            <span className="flex items-center gap-1">
              <span
                className="inline-block h-px w-4"
                style={{ borderTop: "1.5px dashed var(--color-imbalance)" }}
              />
              imb
            </span>
            <span className="flex items-center gap-1">
              <span
                className="inline-block h-px w-4"
                style={{ borderTop: "1.5px dashed var(--color-momNorm)" }}
              />
              mom
            </span>
          </div>
        </div>
      </CardHeader>
      <CardContent>
        <ChartContainer config={chartConfig} className="h-[240px] w-full">
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

            {/* Sol eksen: spread */}
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

            {/* Sağ eksen: kümülatif skor (dinamik ölçek) */}
            <YAxis
              yAxisId="cumul"
              orientation="right"
              domain={[dMin, dMax]}
              tickFormatter={(v) => Number(v).toFixed(0)}
              tickLine={false}
              axisLine={false}
              width={36}
            />

            {/* Gizli eksen: imbalance / momNorm için [−1, +1] */}
            <YAxis
              yAxisId="signal"
              orientation="right"
              domain={[-1, 1]}
              hide
            />

            {/* Nötr çizgisi (kümülatif sıfır noktası) */}
            <ReferenceLine
              yAxisId="cumul"
              y={0}
              stroke="var(--border)"
              strokeDasharray="4 4"
            />

            <ChartTooltip
              content={
                <ChartTooltipContent
                  formatter={(value, name) => {
                    if (typeof value !== "number") return [value, name];
                    return [value.toFixed(3), name];
                  }}
                  labelFormatter={(_v, p) => {
                    const t = p?.[0]?.payload?.t;
                    return typeof t === "number" ? fmtTooltipTime(t) : "";
                  }}
                />
              }
            />

            {/* Spread barları */}
            <Bar
              yAxisId="spread"
              dataKey="upSpread"
              fill="var(--color-upSpread)"
              fillOpacity={0.55}
              radius={[2, 2, 0, 0]}
              isAnimationActive={false}
            />
            <Bar
              yAxisId="spread"
              dataKey="downSpread"
              fill="var(--color-downSpread)"
              fillOpacity={0.55}
              radius={[2, 2, 0, 0]}
              isAnimationActive={false}
            />

            {/* imbalance (Binance CVD) — ince kesik çizgi */}
            <Line
              yAxisId="signal"
              type="monotone"
              dataKey="imbalance"
              stroke="var(--color-imbalance)"
              strokeWidth={1.5}
              strokeDasharray="4 3"
              dot={false}
              isAnimationActive={false}
            />

            {/* momentum normalize (OKX EMA) — ince kesik çizgi */}
            <Line
              yAxisId="signal"
              type="monotone"
              dataKey="momNorm"
              stroke="var(--color-momNorm)"
              strokeWidth={1.5}
              strokeDasharray="2 3"
              dot={false}
              isAnimationActive={false}
            />

            {/* Kümülatif skor — kalın turuncu çizgi */}
            <Line
              yAxisId="cumul"
              type="monotone"
              dataKey="cSkor"
              stroke="var(--color-cSkor)"
              strokeWidth={2.5}
              dot={false}
              isAnimationActive={false}
            />
          </ComposedChart>
        </ChartContainer>
      </CardContent>
    </Card>
  );
}
