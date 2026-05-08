"use client"

import { useMemo } from "react"
import { BarChart2, TrendingDown, TrendingUp, Minus } from "lucide-react"
import { Badge } from "@/components/ui/badge"
import { BotStatsCard } from "@/components/stats/bot-stats-card"
import { useBots } from "@/lib/hooks"
import { CARD_SHELL_CLASS, HEADER_RADIAL_GRADIENT } from "@/lib/ui-constants"
import { cn } from "@/lib/utils"

// ── Özet kart ────────────────────────────────────────────────────────────

function OverviewTile({
  label,
  value,
  sub,
  trend,
}: {
  label: string
  value: React.ReactNode
  sub?: string
  trend?: "up" | "down" | "neutral"
}) {
  const Icon =
    trend === "up"
      ? TrendingUp
      : trend === "down"
        ? TrendingDown
        : Minus

  const iconColor =
    trend === "up"
      ? "text-emerald-500"
      : trend === "down"
        ? "text-rose-500"
        : "text-muted-foreground"

  return (
    <div className="flex flex-col gap-2 rounded-md border border-border/50 bg-card px-4 py-3 shadow-xs">
      <div className="flex items-center justify-between">
        <span className="text-[10px] uppercase tracking-widest text-muted-foreground">{label}</span>
        <Icon className={cn("size-3.5", iconColor)} aria-hidden />
      </div>
      <div className="text-xl font-semibold leading-none">{value}</div>
      {sub && <p className="text-[11px] text-muted-foreground">{sub}</p>}
    </div>
  )
}

// ── Sayfa ─────────────────────────────────────────────────────────────────

export default function StatsPage() {
  const { bots } = useBots()

  const activeBotCount = bots.length
  const runningCount = useMemo(() => bots.filter((b) => b.state === "RUNNING").length, [bots])

  return (
    <div className="space-y-5">
      {/* Header */}
      <header className={CARD_SHELL_CLASS}>
        <div className="relative overflow-hidden bg-gradient-to-br from-muted/35 via-background to-background px-4 py-4 sm:px-5 sm:py-4">
          <div
            aria-hidden
            className="pointer-events-none absolute inset-0 z-0 opacity-[0.35]"
            style={{ backgroundImage: HEADER_RADIAL_GRADIENT }}
          />
          <div className="relative z-10 flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between sm:gap-4">
            <div className="min-w-0 space-y-1">
              <div className="flex flex-wrap items-center gap-x-2 gap-y-1">
                <BarChart2 className="size-5 text-primary" aria-hidden />
                <h1 className="font-heading text-xl font-semibold tracking-tight text-foreground sm:text-2xl">
                  İstatistikler
                </h1>
                <Badge
                  variant="secondary"
                  className="px-1.5 py-0 text-[10px] leading-none font-normal"
                >
                  {activeBotCount} bot
                </Badge>
              </div>
              <p className="max-w-xl text-sm leading-snug text-muted-foreground sm:text-[13px]">
                {activeBotCount === 0
                  ? "Henüz bot yok."
                  : `${activeBotCount} bot · ${runningCount} çalışıyor — MTM PnL anlık değerler (gerçekleşmemiş).`}
              </p>
            </div>
          </div>
        </div>
      </header>

      {/* Bot kartları */}
      {bots.length === 0 ? (
        <div className={cn(CARD_SHELL_CLASS, "flex flex-col items-center gap-3 px-6 py-12 text-center")}>
          <BarChart2 className="size-8 text-muted-foreground/40" aria-hidden />
          <p className="text-sm text-muted-foreground">İstatistik görmek için önce bir bot oluştur.</p>
        </div>
      ) : (
        <div className="space-y-3">
          {bots.map((bot) => (
            <BotStatsCard key={bot.id} bot={bot} />
          ))}
        </div>
      )}
    </div>
  )
}
