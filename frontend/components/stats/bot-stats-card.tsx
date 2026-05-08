"use client"

import { useState, useEffect, useCallback } from "react"
import { ChevronDown, ChevronUp, TrendingUp, TrendingDown, Minus } from "lucide-react"
import {
  Area,
  AreaChart,
  Bar,
  BarChart,
  CartesianGrid,
  Cell,
  RadialBar,
  RadialBarChart,
  ReferenceLine,
  XAxis,
  YAxis,
} from "recharts"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs"
import {
  ChartContainer,
  ChartTooltip,
  ChartTooltipContent,
  type ChartConfig,
} from "@/components/ui/chart"
import { api } from "@/lib/api"
import type { BotRow, BotStats, SessionTimelineItem } from "@/lib/types"
import { CARD_SHELL_CLASS } from "@/lib/ui-constants"
import { cn } from "@/lib/utils"

// ── Yardımcı bileşenler ───────────────────────────────────────────────────

function fmt(n: number, digits = 2): string {
  return n.toFixed(digits)
}

function fmtPnl(n: number): string {
  return (n >= 0 ? "+" : "") + fmt(n)
}

function PnlText({ value, className }: { value: number; className?: string }) {
  return (
    <span
      className={cn(
        "font-mono tabular-nums",
        value > 0
          ? "text-emerald-500"
          : value < 0
            ? "text-rose-500"
            : "text-muted-foreground",
        className
      )}
    >
      {fmtPnl(value)}
    </span>
  )
}

function WinRateBar({ pct }: { pct: number }) {
  return (
    <div className="h-1.5 w-full overflow-hidden rounded-full bg-emerald-500/15">
      <div
        className="h-full rounded-full bg-emerald-500 transition-all"
        style={{ width: `${Math.min(100, Math.max(0, pct))}%` }}
      />
    </div>
  )
}

const TYPE_LABEL: Record<string, string> = {
  SAF_UP: "SAF UP",
  SAF_DOWN: "SAF DOWN",
  KARMA: "KARMA",
}

const TYPE_COLOR: Record<string, string> = {
  SAF_UP: "var(--chart-1)",
  SAF_DOWN: "var(--chart-3)",
  KARMA: "var(--chart-4)",
}

// ── Grafikler ─────────────────────────────────────────────────────────────

const cumulativeConfig: ChartConfig = {
  cumPnl: { label: "Kümülatif PnL", color: "var(--chart-1)" },
}

function CumulativePnlChart({ data }: { data: SessionTimelineItem[] }) {
  const chartData = data.reduce<{ slug: string; cumPnl: number; pnl: number }[]>(
    (acc, item, i) => {
      const prev = acc[i - 1]?.cumPnl ?? 0
      acc.push({ slug: item.slug.slice(-8), cumPnl: prev + item.mtm_pnl, pnl: item.mtm_pnl })
      return acc
    },
    []
  )
  const isPositive = (chartData.at(-1)?.cumPnl ?? 0) >= 0
  const fillColor = isPositive ? "var(--chart-1)" : "var(--color-destructive)"
  const gradientId = "pnlGrad"

  return (
    <ChartContainer config={cumulativeConfig} className="h-[180px] w-full">
      <AreaChart data={chartData} margin={{ top: 6, right: 4, bottom: 0, left: 0 }}>
        <defs>
          <linearGradient id={gradientId} x1="0" y1="0" x2="0" y2="1">
            <stop offset="5%" stopColor={fillColor} stopOpacity={0.25} />
            <stop offset="95%" stopColor={fillColor} stopOpacity={0.02} />
          </linearGradient>
        </defs>
        <CartesianGrid strokeDasharray="3 3" stroke="var(--border)" strokeOpacity={0.4} />
        <XAxis dataKey="slug" tick={false} axisLine={false} tickLine={false} />
        <YAxis
          tick={{ fontSize: 10, fill: "var(--muted-foreground)" }}
          tickLine={false}
          axisLine={false}
          width={44}
          tickFormatter={(v) => `${v > 0 ? "+" : ""}${v.toFixed(0)}`}
        />
        <ReferenceLine y={0} stroke="var(--muted-foreground)" strokeOpacity={0.4} strokeDasharray="4 2" />
        <ChartTooltip
          content={
            <ChartTooltipContent
              formatter={(v, name) =>
                name === "cumPnl"
                  ? [`${Number(v) >= 0 ? "+" : ""}${Number(v).toFixed(2)} USDC`, "Kümülatif"]
                  : v
              }
            />
          }
        />
        <Area
          type="monotone"
          dataKey="cumPnl"
          stroke={fillColor}
          strokeWidth={2}
          fill={`url(#${gradientId})`}
          dot={false}
          activeDot={{ r: 3 }}
        />
      </AreaChart>
    </ChartContainer>
  )
}

const sessionBarConfig: ChartConfig = {
  mtm_pnl: { label: "Session PnL" },
}

function SessionBarChart({ data }: { data: SessionTimelineItem[] }) {
  const last100 = data.slice(-100)
  return (
    <ChartContainer config={sessionBarConfig} className="h-[160px] w-full">
      <BarChart data={last100} margin={{ top: 4, right: 4, bottom: 0, left: 0 }}>
        <CartesianGrid strokeDasharray="3 3" stroke="var(--border)" strokeOpacity={0.4} vertical={false} />
        <XAxis dataKey="slug" tick={false} axisLine={false} tickLine={false} />
        <YAxis
          tick={{ fontSize: 10, fill: "var(--muted-foreground)" }}
          tickLine={false}
          axisLine={false}
          width={44}
          tickFormatter={(v) => `${v > 0 ? "+" : ""}${v.toFixed(0)}`}
        />
        <ReferenceLine y={0} stroke="var(--muted-foreground)" strokeOpacity={0.4} />
        <ChartTooltip
          content={
            <ChartTooltipContent
              formatter={(v, _name, props) => {
                const item = props.payload as SessionTimelineItem
                return [
                  `${Number(v) >= 0 ? "+" : ""}${Number(v).toFixed(2)} USDC`,
                  item?.position_type ?? "PnL",
                ]
              }}
            />
          }
        />
        <Bar dataKey="mtm_pnl" radius={[2, 2, 0, 0]} maxBarSize={12}>
          {last100.map((entry, idx) => (
            <Cell
              key={idx}
              fill={entry.mtm_pnl >= 0 ? "var(--chart-1)" : "var(--color-destructive)"}
              fillOpacity={0.85}
            />
          ))}
        </Bar>
      </BarChart>
    </ChartContainer>
  )
}

function TypeRadialChart({ data }: { data: BotStats["by_type"] }) {
  const chartData = data.map((d) => ({
    name: TYPE_LABEL[d.position_type] ?? d.position_type,
    winrate: Math.round(d.winrate_pct),
    fill: TYPE_COLOR[d.position_type] ?? "var(--chart-5)",
  }))
  const radialConfig: ChartConfig = Object.fromEntries(
    chartData.map((d) => [d.name, { label: d.name, color: d.fill }])
  )

  return (
    <ChartContainer config={radialConfig} className="h-[180px] w-full">
      <RadialBarChart
        data={chartData}
        innerRadius="30%"
        outerRadius="90%"
        startAngle={90}
        endAngle={-270}
        barSize={14}
      >
        <RadialBar dataKey="winrate" background={{ fill: "var(--muted)", opacity: 0.3 }} cornerRadius={4}>
          {chartData.map((entry, idx) => (
            <Cell key={idx} fill={entry.fill} />
          ))}
        </RadialBar>
        <ChartTooltip
          content={
            <ChartTooltipContent formatter={(v) => [`%${v}`, "Win Rate"]} />
          }
        />
      </RadialBarChart>
    </ChartContainer>
  )
}

// ── Tip Kutuları ─────────────────────────────────────────────────────────

function TypeBox({ stat }: { stat: BotStats["by_type"][number] }) {
  const label = TYPE_LABEL[stat.position_type] ?? stat.position_type
  const color = TYPE_COLOR[stat.position_type] ?? "var(--chart-5)"
  const isKarma = stat.position_type === "KARMA"

  return (
    <div
      className="flex flex-col gap-2 rounded-md border border-border/50 bg-muted/20 px-3 py-2.5"
      style={{ borderLeftColor: color, borderLeftWidth: 2 }}
    >
      <div className="flex items-center justify-between gap-2">
        <span className="text-[10px] font-medium uppercase tracking-widest text-muted-foreground">
          {label}
        </span>
        <Badge
          variant="outline"
          className={cn(
            "h-4 rounded-sm px-1.5 py-0 text-[10px] font-normal",
            stat.winrate_pct >= 80
              ? "border-transparent bg-emerald-500/15 text-emerald-600 dark:text-emerald-400"
              : isKarma && stat.winrate_pct < 50
                ? "border-transparent bg-rose-500/15 text-rose-600 dark:text-rose-400"
                : "border-transparent bg-amber-500/15 text-amber-700 dark:text-amber-400"
          )}
        >
          %{fmt(stat.winrate_pct, 0)} WR
        </Badge>
      </div>
      <WinRateBar pct={stat.winrate_pct} />
      <div className="flex items-center justify-between">
        <span className="text-[11px] text-muted-foreground">{stat.total} session</span>
        <PnlText value={stat.total_pnl} className="text-xs font-semibold" />
      </div>
      <div className="flex items-center justify-between">
        <span className="text-[10px] text-muted-foreground">ROI</span>
        <span
          className={cn(
            "font-mono text-[11px] tabular-nums",
            stat.roi_pct >= 0 ? "text-emerald-500" : "text-rose-500"
          )}
        >
          {stat.roi_pct >= 0 ? "+" : ""}
          {fmt(stat.roi_pct)}%
        </span>
      </div>
    </div>
  )
}

// ── MetricPill ─────────────────────────────────────────────────────────────

function MetricPill({
  label,
  children,
}: {
  label: string
  children: React.ReactNode
}) {
  return (
    <div className="flex flex-col gap-0.5">
      <span className="text-[10px] uppercase tracking-widest text-muted-foreground">{label}</span>
      <div className="text-sm font-semibold">{children}</div>
    </div>
  )
}

// ── Ana kart ─────────────────────────────────────────────────────────────

interface BotStatsCardProps {
  bot: BotRow
}

export function BotStatsCard({ bot }: BotStatsCardProps) {
  const [open, setOpen] = useState(false)
  const [stats, setStats] = useState<BotStats | null>(null)
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const loadStats = useCallback(async () => {
    if (stats) return
    setLoading(true)
    setError(null)
    try {
      const s = await api.botStats(bot.id)
      setStats(s)
    } catch (e) {
      setError(e instanceof Error ? e.message : "Yüklenemedi")
    } finally {
      setLoading(false)
    }
  }, [bot.id, stats])

  useEffect(() => {
    if (open) loadStats()
  }, [open, loadStats])

  const pnlIcon =
    stats && stats.total_mtm_pnl > 0 ? (
      <TrendingUp className="size-3.5 text-emerald-500" />
    ) : stats && stats.total_mtm_pnl < 0 ? (
      <TrendingDown className="size-3.5 text-rose-500" />
    ) : (
      <Minus className="size-3.5 text-muted-foreground" />
    )

  return (
    <div className={CARD_SHELL_CLASS}>
      {/* Kart başlığı — her zaman görünür */}
      <button
        onClick={() => setOpen((v) => !v)}
        className="group w-full text-left"
        aria-expanded={open}
      >
        <div className="flex items-center gap-3 px-4 py-3 transition-colors hover:bg-muted/30">
          {/* Sol: isim + badge'ler */}
          <div className="min-w-0 flex-1">
            <div className="flex flex-wrap items-center gap-1.5">
              <span className="font-heading text-sm font-semibold text-foreground group-hover:text-primary">
                {bot.name}
              </span>
              <Badge variant="outline" className="h-4 rounded-sm px-1.5 py-0 text-[10px] font-normal uppercase">
                {bot.strategy}
              </Badge>
              <Badge
                className={cn(
                  "h-4 rounded-sm px-1.5 py-0 text-[10px] font-normal uppercase border-transparent",
                  bot.run_mode === "live"
                    ? "bg-primary/15 text-primary"
                    : "bg-amber-500/15 text-amber-700 dark:text-amber-400"
                )}
              >
                {bot.run_mode}
              </Badge>
              <Badge
                className={cn(
                  "h-4 rounded-sm px-1.5 py-0 text-[10px] font-normal border-transparent",
                  bot.state === "RUNNING"
                    ? "bg-emerald-500/15 text-emerald-600 dark:text-emerald-400"
                    : bot.state === "CRASHED"
                      ? "bg-destructive/15 text-destructive"
                      : "bg-secondary text-secondary-foreground"
                )}
              >
                {bot.state}
              </Badge>
            </div>
            <p className="mt-0.5 font-mono text-[11px] text-muted-foreground">{bot.slug_pattern}</p>
          </div>

          {/* Sağ: özet metrikler */}
          {stats && (
            <div className="hidden shrink-0 items-center gap-4 sm:flex">
              <div className="flex items-center gap-1">
                {pnlIcon}
                <PnlText value={stats.total_mtm_pnl} className="text-sm font-semibold" />
                <span className="text-[10px] text-muted-foreground">USDC</span>
              </div>
              <div className="text-right">
                <div
                  className={cn(
                    "font-mono text-sm font-semibold tabular-nums",
                    stats.winrate_pct >= 60 ? "text-emerald-500" : "text-rose-500"
                  )}
                >
                  %{fmt(stats.winrate_pct, 0)}
                </div>
                <div className="text-[10px] text-muted-foreground">WR</div>
              </div>
              <div className="text-right">
                <div
                  className={cn(
                    "font-mono text-sm font-semibold tabular-nums",
                    stats.roi_pct >= 0 ? "text-emerald-500" : "text-rose-500"
                  )}
                >
                  {stats.roi_pct >= 0 ? "+" : ""}
                  {fmt(stats.roi_pct)}%
                </div>
                <div className="text-[10px] text-muted-foreground">ROI</div>
              </div>
            </div>
          )}

          {/* Expand ikonu */}
          <div className="shrink-0 text-muted-foreground">
            {open ? <ChevronUp className="size-4" /> : <ChevronDown className="size-4" />}
          </div>
        </div>
      </button>

      {/* Genişletilmiş içerik */}
      {open && (
        <div className="border-t border-border/50">
          {loading && (
            <div className="flex items-center justify-center py-10 text-sm text-muted-foreground">
              Yükleniyor…
            </div>
          )}
          {error && (
            <div className="px-4 py-4 text-sm text-destructive">{error}</div>
          )}
          {stats && (
            <div className="space-y-4 p-4">
              {/* Özet metrikler */}
              <div className="grid grid-cols-2 gap-3 sm:grid-cols-4 lg:grid-cols-6">
                <MetricPill label="Session">
                  <span className="font-mono tabular-nums">{stats.total_sessions}</span>
                </MetricPill>
                <MetricPill label="Kazanan">
                  <span className="font-mono tabular-nums text-emerald-500">{stats.winning}</span>
                </MetricPill>
                <MetricPill label="Kaybeden">
                  <span className="font-mono tabular-nums text-rose-500">{stats.losing}</span>
                </MetricPill>
                <MetricPill label="Trade">
                  <span className="font-mono tabular-nums">{stats.total_trades}</span>
                </MetricPill>
                <MetricPill label="En İyi">
                  <PnlText value={stats.best_session_pnl} />
                </MetricPill>
                <MetricPill label="En Kötü">
                  <PnlText value={stats.worst_session_pnl} />
                </MetricPill>
              </div>

              {/* Pozisyon tipi kutuları */}
              {stats.by_type.length > 0 && (
                <div className="grid grid-cols-1 gap-2 sm:grid-cols-3">
                  {stats.by_type.map((t) => (
                    <TypeBox key={t.position_type} stat={t} />
                  ))}
                </div>
              )}

              {/* Tabs: Grafikler | Sessions */}
              <Tabs defaultValue="charts">
                <TabsList className="h-8 w-full border-b border-border/50 bg-transparent p-0">
                  <TabsTrigger
                    value="charts"
                    className="h-8 rounded-none border-b-2 border-transparent px-3 text-xs data-[state=active]:border-primary data-[state=active]:text-foreground"
                  >
                    Grafikler
                  </TabsTrigger>
                  <TabsTrigger
                    value="sessions"
                    className="h-8 rounded-none border-b-2 border-transparent px-3 text-xs data-[state=active]:border-primary data-[state=active]:text-foreground"
                  >
                    Session Listesi
                  </TabsTrigger>
                </TabsList>

                <TabsContent value="charts" className="mt-4 space-y-4">
                  {stats.sessions_timeline.length === 0 ? (
                    <p className="py-6 text-center text-sm text-muted-foreground">
                      Henüz session verisi yok.
                    </p>
                  ) : (
                    <>
                      <div className="grid grid-cols-1 gap-4 lg:grid-cols-2">
                        <div className="space-y-1">
                          <p className="text-[10px] uppercase tracking-widest text-muted-foreground">
                            Kümülatif PnL
                          </p>
                          <CumulativePnlChart data={stats.sessions_timeline} />
                        </div>
                        <div className="space-y-1">
                          <p className="text-[10px] uppercase tracking-widest text-muted-foreground">
                            Pozisyon Dağılımı (Win Rate %)
                          </p>
                          <div className="flex items-center gap-4">
                            <TypeRadialChart data={stats.by_type} />
                            <div className="flex flex-col gap-2">
                              {stats.by_type.map((t) => (
                                <div key={t.position_type} className="flex items-center gap-2">
                                  <div
                                    className="size-2.5 rounded-full"
                                    style={{ background: TYPE_COLOR[t.position_type] ?? "var(--chart-5)" }}
                                  />
                                  <span className="text-[11px] text-muted-foreground">
                                    {TYPE_LABEL[t.position_type]} — %{fmt(t.winrate_pct, 0)}
                                  </span>
                                </div>
                              ))}
                            </div>
                          </div>
                        </div>
                      </div>
                      <div className="space-y-1">
                        <p className="text-[10px] uppercase tracking-widest text-muted-foreground">
                          Session PnL (Son {Math.min(100, stats.sessions_timeline.length)})
                        </p>
                        <SessionBarChart data={stats.sessions_timeline} />
                      </div>
                    </>
                  )}
                </TabsContent>

                <TabsContent value="sessions" className="mt-4">
                  <SessionTable data={stats.sessions_timeline} />
                </TabsContent>
              </Tabs>
            </div>
          )}
        </div>
      )}
    </div>
  )
}

// ── Session Tablosu ───────────────────────────────────────────────────────

function SessionTable({ data }: { data: SessionTimelineItem[] }) {
  const sorted = [...data].sort((a, b) => b.ts_ms - a.ts_ms).slice(0, 50)

  if (sorted.length === 0) {
    return (
      <p className="py-6 text-center text-sm text-muted-foreground">Session bulunamadı.</p>
    )
  }

  return (
    <div className="overflow-hidden rounded-md border border-border/50">
      <div className="divide-y divide-border/40">
        {sorted.map((s) => (
          <div key={s.session_id} className="flex items-center gap-3 px-3 py-2 hover:bg-muted/30">
            <span className="flex-1 font-mono text-[11px] text-muted-foreground">{s.slug}</span>
            <Badge
              variant="outline"
              className="h-4 shrink-0 rounded-sm px-1.5 py-0 text-[10px] font-normal"
              style={{ color: TYPE_COLOR[s.position_type] ?? undefined, borderColor: "transparent" }}
            >
              {TYPE_LABEL[s.position_type] ?? s.position_type}
            </Badge>
            <span
              className={cn(
                "w-20 text-right font-mono text-[11px] tabular-nums",
                s.roi_pct >= 0 ? "text-emerald-500" : "text-rose-500"
              )}
            >
              {s.roi_pct >= 0 ? "+" : ""}
              {fmt(s.roi_pct)}%
            </span>
            <PnlText value={s.mtm_pnl} className="w-20 text-right text-xs" />
          </div>
        ))}
      </div>
    </div>
  )
}
