"use client"

import { useMemo } from "react"
import { CandlestickChart } from "lucide-react"
import {
  CartesianGrid,
  Line,
  LineChart,
  ReferenceLine,
  XAxis,
  YAxis,
} from "recharts"
import {
  Card,
  CardAction,
  CardContent,
  CardHeader,
  CardTitle,
} from "@/components/ui/card"
import {
  ChartContainer,
  ChartTooltip,
  ChartTooltipContent,
  type ChartConfig,
} from "@/components/ui/chart"
import type { MarketTick } from "@/lib/types"
import { cn } from "@/lib/utils"
import {
  CHART_MARGIN_PRICE,
  CHART_TIME_X_AXIS_LAYOUT,
  fmtTickTime,
  fmtTooltipTime,
  SECTION_LABEL_CLASS,
  timeTicks,
  ZONE_BOUNDARY_LABELS,
  zoneBoundaryTimes,
  type SessionRange,
} from "@/lib/chart-utils"

/* ─── Bot bid formula (mirrors src/strategy/harvest/dual.rs) ─────────────
 *   composite = backend signal_score (RTDS + Binance harmanı; 5 = nötr)
 *
 *   Doğrudan olasılık eşlemesi (orderbook bağımsız):
 *     up_price   = clamp(snap(composite / 10),       MIN, MAX)
 *     down_price = clamp(snap(1 − composite / 10),   MIN, MAX)
 *
 *   composite=5 → 0.50/0.50; composite=10 → 0.95/0.05; composite=0 → 0.05/0.95.
 *   1 − up = down simetrisi (clamp sınırları dışında bozulabilir).
 */
const TICK = 0.01
const MIN_PRICE = 0.05
const MAX_PRICE = 0.95
const snap = (p: number) => Math.round(p / TICK) * TICK
const clampPrice = (p: number) =>
  Math.min(MAX_PRICE, Math.max(MIN_PRICE, snap(p)))
function botBids(composite: number) {
  const upRaw = Math.max(0, Math.min(1, composite / 10))
  const downRaw = 1 - upRaw
  return {
    upBotBid: clampPrice(upRaw),
    downBotBid: clampPrice(downRaw),
  }
}

interface Props {
  data: MarketTick[]
  session: SessionRange | null
}

interface Row {
  t: number
  upBid: number
  upAsk: number
  downBid: number
  downAsk: number
  upBotBid: number
  downBotBid: number
}

const chartConfig = {
  upBid: { label: "UP bid", color: "var(--chart-1)" },
  upAsk: { label: "UP ask", color: "var(--chart-2)" },
  downBid: { label: "DOWN bid", color: "oklch(0.58 0.22 352)" },
  downAsk: { label: "DOWN ask", color: "oklch(0.7 0.17 352)" },
  upBotBid: { label: "UP bot bid", color: "oklch(0.80 0.20 145)" },
  downBotBid: { label: "DOWN bot bid", color: "oklch(0.75 0.20 25)" },
} satisfies ChartConfig

function fmtPx(v: number | undefined): string {
  if (v == null || Number.isNaN(v)) return "—"
  return v.toFixed(4)
}

/** MarketTick[] → Row[] (saniye granülaritesinde tekilleştirme). */
function toRows(ticks: MarketTick[]): Row[] {
  const out: Row[] = []
  for (const tk of ticks) {
    const t = Math.floor(tk.ts_ms / 1000)
    const { upBotBid, downBotBid } = botBids(tk.signal_score)
    const row: Row = {
      t,
      upBid: tk.up_best_bid,
      upAsk: tk.up_best_ask,
      downBid: tk.down_best_bid,
      downAsk: tk.down_best_ask,
      upBotBid,
      downBotBid,
    }
    if (out.length && out[out.length - 1].t === t) {
      out[out.length - 1] = row
    } else {
      out.push(row)
    }
  }
  return out
}

export function PriceChart({ data, session }: Props) {
  const rows = useMemo(() => toRows(data), [data])
  const ticks = useMemo(
    () => (session ? timeTicks(session) : []),
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [session?.start, session?.end]
  )
  const zoneLines = useMemo(
    () => (session ? zoneBoundaryTimes(session) : []),
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [session?.start, session?.end]
  )

  if (!session) return null
  const latest = rows.length > 0 ? rows[rows.length - 1] : null

  return (
    <Card>
      <CardHeader>
        <CardTitle
          className={cn(SECTION_LABEL_CLASS, "flex items-center gap-1.5")}
        >
          <CandlestickChart
            className="size-3.5 shrink-0 opacity-80"
            aria-hidden
          />
          Price
        </CardTitle>
        <CardAction className="flex flex-wrap items-end justify-end gap-x-4 gap-y-2">
          {(
            [
              { key: "upBid", label: "UP bid" },
              { key: "upAsk", label: "UP ask" },
              { key: "downBid", label: "DOWN bid" },
              { key: "downAsk", label: "DOWN ask" },
              { key: "upBotBid", label: "UP bot bid" },
              { key: "downBotBid", label: "DOWN bot bid" },
            ] as const
          ).map(({ key, label }) => {
            const color = chartConfig[key].color
            const isBot = key === "upBotBid" || key === "downBotBid"
            return (
              <div key={key} className="text-right">
                <div
                  className={cn(
                    "text-[10px] leading-tight",
                    isBot ? "text-muted-foreground/60" : "text-muted-foreground"
                  )}
                >
                  {label}
                </div>
                <div
                  className="font-mono text-sm tabular-nums"
                  style={{ color }}
                >
                  {fmtPx(latest?.[key])}
                </div>
              </div>
            )
          })}
        </CardAction>
      </CardHeader>
      <CardContent>
        <ChartContainer config={chartConfig} className="h-[220px] w-full">
          <LineChart data={rows} margin={CHART_MARGIN_PRICE}>
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
                    const t = p?.[0]?.payload?.t
                    return typeof t === "number" ? fmtTooltipTime(t) : ""
                  }}
                />
              }
            />
            <Line
              type="monotone"
              dataKey="upBid"
              stroke="var(--color-upBid)"
              strokeWidth={3}
              dot={false}
              isAnimationActive={false}
            />
            <Line
              type="monotone"
              dataKey="upAsk"
              stroke="var(--color-upAsk)"
              strokeWidth={1}
              dot={false}
              isAnimationActive={false}
            />
            <Line
              type="monotone"
              dataKey="downBid"
              stroke="var(--color-downBid)"
              strokeWidth={3}
              dot={false}
              isAnimationActive={false}
            />
            <Line
              type="monotone"
              dataKey="downAsk"
              stroke="var(--color-downAsk)"
              strokeWidth={1}
              dot={false}
              isAnimationActive={false}
            />
            <Line
              type="monotone"
              dataKey="upBotBid"
              stroke="var(--color-upBotBid)"
              strokeWidth={1.5}
              strokeDasharray="5 3"
              dot={false}
              isAnimationActive={false}
            />
            <Line
              type="monotone"
              dataKey="downBotBid"
              stroke="var(--color-downBotBid)"
              strokeWidth={1.5}
              strokeDasharray="5 3"
              dot={false}
              isAnimationActive={false}
            />
          </LineChart>
        </ChartContainer>
      </CardContent>
    </Card>
  )
}
