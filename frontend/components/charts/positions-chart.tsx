"use client"

import { useMemo } from "react"
import { Layers } from "lucide-react"
import { CartesianGrid, Line, LineChart, XAxis, YAxis } from "recharts"
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card"
import { cn } from "@/lib/utils"
import {
  ChartContainer,
  ChartTooltip,
  type ChartConfig,
} from "@/components/ui/chart"
import type { PnLSnapshot } from "@/lib/types"
import {
  CHART_MARGIN_TIGHT,
  CHART_TIME_X_AXIS_LAYOUT,
  fmtTickTime,
  fmtTooltipTime,
  SECTION_LABEL_CLASS,
  SIGNAL_PAIR_CARD_CLASS,
  SIGNAL_PAIR_CONTENT_CLASS,
  SIGNAL_PAIR_HEADER_CLASS,
  timeTicks,
  type SessionRange,
} from "@/lib/chart-utils"

interface Props {
  data: PnLSnapshot[]
  session: SessionRange | null
}

interface Row {
  t: number
  up_filled: number
  down_filled: number
  pair_count: number
  avg_up: number
  avg_down: number
  avg_sum: number
}

const chartConfig = {
  up_filled: { label: "up_filled", color: "oklch(0.58 0.17 155)" },
  down_filled: { label: "down_filled", color: "oklch(0.58 0.2 25)" },
  avg_sum: { label: "avg_sum", color: "oklch(0.55 0.2 285)" },
} satisfies ChartConfig

/** VWAP yoksa 0 kabul (grafikte düz çizgi); dolu olduğunda `avg_up + avg_down`. */
function toRows(snaps: PnLSnapshot[]): Row[] {
  const out: Row[] = []
  for (const p of snaps) {
    const t = Math.floor(p.ts_ms / 1000)
    const au = p.avg_up ?? 0
    const ad = p.avg_down ?? 0
    const row: Row = {
      t,
      up_filled: p.up_filled,
      down_filled: p.down_filled,
      pair_count: p.pair_count,
      avg_up: au,
      avg_down: ad,
      avg_sum: au + ad,
    }
    if (out.length && out[out.length - 1].t === t) {
      out[out.length - 1] = row
    } else {
      out.push(row)
    }
  }
  return out
}

/** UP/DOWN kontrat (sol eksen) + `avg_sum = avg_up + avg_down` (sağ eksen). */
export function PositionsChart({ data, session }: Props) {
  const rows = useMemo(() => toRows(data), [data])
  const ticks = useMemo(
    () => (session ? timeTicks(session) : []),
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [session?.start, session?.end]
  )

  if (!session) return null

  const margin = { ...CHART_MARGIN_TIGHT, right: 44 }

  return (
    <Card className={cn(SIGNAL_PAIR_CARD_CLASS, "min-w-0")}>
      <CardHeader className={SIGNAL_PAIR_HEADER_CLASS}>
        <div className="flex min-w-0 items-center gap-1.5">
          <Layers
            className="size-3 shrink-0 text-muted-foreground"
            aria-hidden
          />
          <CardTitle
            className={cn(SECTION_LABEL_CLASS, "tracking-[0.12em] normal-case")}
          >
            POSITIONS
          </CardTitle>
        </div>
      </CardHeader>
      <CardContent
        className={cn(
          SIGNAL_PAIR_CONTENT_CLASS,
          "flex min-h-0 flex-1 flex-col"
        )}
      >
        <ChartContainer
          config={chartConfig}
          className="aspect-auto h-full min-h-0 w-full flex-1"
        >
          <LineChart data={rows} margin={margin}>
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
              yAxisId="left"
              tickFormatter={(v) =>
                Number.isInteger(v) ? String(v) : Number(v).toFixed(1)
              }
              tickLine={false}
              axisLine={false}
              width={44}
              domain={[0, "auto"]}
            />
            <YAxis
              yAxisId="right"
              orientation="right"
              tickFormatter={(v) => Number(v).toFixed(3)}
              tickLine={false}
              axisLine={false}
              width={40}
              domain={[0, "auto"]}
            />
            <ChartTooltip
              content={({ active, payload }) => {
                if (!active || !payload?.length) return null
                const row = payload[0]?.payload as Row | undefined
                if (!row) return null
                const imb = row.up_filled - row.down_filled
                return (
                  <div
                    className={cn(
                      "grid min-w-40 gap-1.5 rounded-md border border-border/35 bg-background px-2.5 py-1.5 text-xs shadow-xl"
                    )}
                  >
                    <div className="font-medium">{fmtTooltipTime(row.t)}</div>
                    <div className="flex justify-between gap-4 leading-none">
                      <span className="text-emerald-700/80 dark:text-emerald-400/90">
                        up_filled
                      </span>
                      <span className="font-mono font-semibold text-emerald-600 tabular-nums dark:text-emerald-400">
                        {row.up_filled.toFixed(2)}
                      </span>
                    </div>
                    <div className="flex justify-between gap-4 leading-none">
                      <span className="text-rose-700/80 dark:text-rose-400/90">
                        down_filled
                      </span>
                      <span className="font-mono font-semibold text-rose-600 tabular-nums dark:text-rose-400">
                        {row.down_filled.toFixed(2)}
                      </span>
                    </div>
                    <div className="flex justify-between gap-4 leading-none">
                      <span className="text-muted-foreground">imbalance</span>
                      <span className="font-mono font-semibold tabular-nums">
                        {imb >= 0 ? "+" : ""}
                        {imb.toFixed(2)}
                      </span>
                    </div>
                    <div className="flex justify-between gap-4 border-t border-border/50 pt-1 leading-none">
                      <span className="text-violet-700/80 dark:text-violet-400/90">
                        pair_count
                      </span>
                      <span className="font-mono font-semibold text-violet-600 tabular-nums dark:text-violet-400">
                        {row.pair_count.toFixed(2)}
                      </span>
                    </div>
                    <div className="space-y-1 border-t border-border/50 pt-1">
                      <div className="flex justify-between gap-4 leading-none">
                        <span className="text-emerald-700/85 dark:text-emerald-400/85">
                          avg_up
                        </span>
                        <span className="font-mono font-semibold text-emerald-600/90 tabular-nums dark:text-emerald-400/90">
                          {row.avg_up.toFixed(4)}
                        </span>
                      </div>
                      <div className="flex justify-between gap-4 leading-none">
                        <span className="text-rose-700/85 dark:text-rose-400/85">
                          avg_down
                        </span>
                        <span className="font-mono font-semibold text-rose-600/90 tabular-nums dark:text-rose-400/90">
                          {row.avg_down.toFixed(4)}
                        </span>
                      </div>
                      <div className="flex justify-between gap-4 leading-none">
                        <span className="text-violet-700/80 dark:text-violet-400/90">
                          avg_sum
                        </span>
                        <span className="font-mono font-semibold text-violet-600 tabular-nums dark:text-violet-400">
                          {row.avg_sum.toFixed(4)}
                        </span>
                      </div>
                    </div>
                  </div>
                )
              }}
            />
            <Line
              yAxisId="left"
              type="monotone"
              dataKey="up_filled"
              stroke="var(--color-up_filled)"
              strokeWidth={2}
              dot={false}
              isAnimationActive={false}
            />
            <Line
              yAxisId="left"
              type="monotone"
              dataKey="down_filled"
              stroke="var(--color-down_filled)"
              strokeWidth={2}
              dot={false}
              isAnimationActive={false}
            />
            <Line
              yAxisId="right"
              type="monotone"
              dataKey="avg_sum"
              stroke="var(--color-avg_sum)"
              strokeWidth={2}
              dot={false}
              isAnimationActive={false}
            />
          </LineChart>
        </ChartContainer>
      </CardContent>
    </Card>
  )
}
