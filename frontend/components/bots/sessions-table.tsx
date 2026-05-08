"use client"

import { useEffect, useState } from "react"
import Link from "next/link"
import {
  ArrowRight,
  ChevronLeft,
  ChevronRight,
  TrendingUp,
  TrendingDown,
} from "lucide-react"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card"
import { cn } from "@/lib/utils"
import { api } from "@/lib/api"
import type { SessionListItem } from "@/lib/types"

const PAGE_SIZE = 10
const POLL_MS = 5000

function fmtTs(ts: number): string {
  return new Date(ts * 1000).toLocaleString()
}

/**
 * Sayfa yüklendikten sonra `/api/bots/:id/sessions` listesini sayfa sayfa
 * (10'arlı) çeker; arka plan yenileme şu anki sayfayı tazeler.
 * - AbortController: eski istek uçuşta iken yenisi başlarsa iptal eder.
 * - Visibility: sekme arka planda iken polling durur; ön plana gelince gap-fill.
 */
export function SessionsTable({ botId }: { botId: number }) {
  const [items, setItems] = useState<SessionListItem[] | null>(null)
  const [total, setTotal] = useState(0)
  const [totalPnl, setTotalPnl] = useState<number | null>(null)
  const [page, setPage] = useState(0)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    if (!Number.isFinite(botId)) return
    let ctrl: AbortController | null = null
    let timer: ReturnType<typeof setInterval> | null = null

    const reload = async () => {
      if (document.hidden) return
      ctrl?.abort()
      ctrl = new AbortController()
      const { signal } = ctrl
      try {
        const res = await api.botSessions(
          botId,
          PAGE_SIZE,
          page * PAGE_SIZE,
          signal
        )
        if (signal.aborted) return
        setItems(res.items)
        setTotal(res.total)
        setTotalPnl(res.total_pnl ?? null)
        setError(null)
      } catch (e) {
        if (e instanceof Error && e.name === "AbortError") return
        if (!signal.aborted) {
          setError(e instanceof Error ? e.message : "Hata")
        }
      }
    }

    const startTimer = () => {
      if (timer !== null) return
      timer = setInterval(() => void reload(), POLL_MS)
    }
    const stopTimer = () => {
      if (timer !== null) {
        clearInterval(timer)
        timer = null
      }
    }

    const onVisibility = () => {
      if (document.hidden) {
        stopTimer()
        ctrl?.abort()
      } else {
        void reload()
        startTimer()
      }
    }

    void reload()
    document.addEventListener("visibilitychange", onVisibility)
    if (!document.hidden) startTimer()

    return () => {
      stopTimer()
      ctrl?.abort()
      document.removeEventListener("visibilitychange", onVisibility)
    }
  }, [botId, page])

  const list = items ?? []
  const pageCount = Math.max(1, Math.ceil(total / PAGE_SIZE))
  const showingFrom = total === 0 ? 0 : page * PAGE_SIZE + 1
  const showingTo = Math.min(total, page * PAGE_SIZE + list.length)

  return (
    <Card>
      <CardHeader>
        <CardTitle className="flex items-center justify-between gap-2">
          <div className="flex items-center gap-2">
            <span>Geçmiş Marketler</span>
            <Badge variant="secondary" className="font-mono tabular-nums">
              {total}
            </Badge>
          </div>
          {totalPnl != null && (
            <div className="flex flex-col items-end gap-0.5">
              <span className="text-[10px] tracking-wider text-muted-foreground uppercase">
                Toplam K/Z
              </span>
              <span
                className={cn(
                  "font-mono text-sm font-semibold tabular-nums",
                  totalPnl > 0
                    ? "text-emerald-500"
                    : totalPnl < 0
                      ? "text-destructive"
                      : "text-foreground"
                )}
              >
                {totalPnl > 0 ? "+" : ""}
                {totalPnl.toFixed(4)}
              </span>
            </div>
          )}
        </CardTitle>
      </CardHeader>
      <CardContent className="p-0">
        {items === null ? (
          <p className="p-6 text-sm text-muted-foreground">Yükleniyor…</p>
        ) : error && list.length === 0 ? (
          <p className="p-6 text-sm text-destructive">{error}</p>
        ) : list.length === 0 ? (
          <p className="p-6 text-sm text-muted-foreground">
            Henüz session geçmişi yok.
          </p>
        ) : (
          <div className="divide-y">
            {list.map((s) => (
              <Link
                key={s.slug}
                href={`/bots/${botId}/${s.slug}`}
                className={cn(
                  "group flex items-center gap-4 px-4 py-3 transition-colors",
                  s.is_live
                    ? "bg-emerald-500/10 hover:bg-emerald-500/15"
                    : "hover:bg-muted/40"
                )}
              >
                <div className="min-w-0 flex-1">
                  <div className="flex flex-wrap items-center gap-2">
                    <span className="font-mono text-sm break-all">
                      {s.slug}
                    </span>
                    <StateBadge state={s.state} live={s.is_live} />
                  </div>
                  <p className="mt-1 text-xs text-muted-foreground">
                    {fmtTs(s.start_ts)} → {fmtTs(s.end_ts)}
                  </p>
                </div>
                <Stat label="Cost" width="w-20">
                  <span className="font-mono text-xs tabular-nums">
                    ${s.cost_basis.toFixed(2)}
                  </span>
                </Stat>
                <Stat label="Shares" width="w-28">
                  <span className="font-mono text-xs tabular-nums">
                    <span className="text-emerald-500">
                      U{s.up_filled.toFixed(0)}
                    </span>
                    <span className="text-muted-foreground"> · </span>
                    <span className="text-destructive">
                      D{s.down_filled.toFixed(0)}
                    </span>
                  </span>
                </Stat>
                <Stat label="If Up" width="w-24">
                  <PnlValue value={s.pnl_if_up} />
                </Stat>
                <Stat label="If Down" width="w-24">
                  <PnlValue value={s.pnl_if_down} />
                </Stat>
                <Stat label="Realized" width="w-24">
                  {s.winning_outcome && (
                    <WinnerBadge outcome={s.winning_outcome} />
                  )}
                  <PnlValue
                    value={
                      s.winning_outcome?.toLowerCase() === "up"
                        ? s.pnl_if_up
                        : s.winning_outcome?.toLowerCase() === "down"
                          ? s.pnl_if_down
                          : s.realized_pnl
                    }
                    bold
                  />
                </Stat>
                <ArrowRight className="h-4 w-4 shrink-0 text-muted-foreground group-hover:text-foreground" />
              </Link>
            ))}
          </div>
        )}
        {items !== null && total > 0 && (
          <div className="flex items-center justify-between border-t border-border/50 px-4 py-2.5">
            <p className="text-xs text-muted-foreground tabular-nums">
              {showingFrom}–{showingTo} / {total}
            </p>
            <div className="flex items-center gap-2">
              <span className="text-xs text-muted-foreground tabular-nums">
                Sayfa {page + 1} / {pageCount}
              </span>
              <Button
                size="icon"
                variant="outline"
                className="h-7 w-7"
                onClick={() => setPage((p) => Math.max(0, p - 1))}
                disabled={page === 0}
                aria-label="Önceki sayfa"
              >
                <ChevronLeft className="h-4 w-4" />
              </Button>
              <Button
                size="icon"
                variant="outline"
                className="h-7 w-7"
                onClick={() => setPage((p) => Math.min(pageCount - 1, p + 1))}
                disabled={page >= pageCount - 1}
                aria-label="Sonraki sayfa"
              >
                <ChevronRight className="h-4 w-4" />
              </Button>
            </div>
          </div>
        )}
      </CardContent>
    </Card>
  )
}

function Stat({
  label,
  width,
  children,
}: {
  label: string
  width: string
  children: React.ReactNode
}) {
  return (
    <div
      className={cn(
        "hidden shrink-0 flex-col items-end gap-0.5 text-right md:flex",
        width
      )}
    >
      <span className="text-[10px] tracking-wider text-muted-foreground uppercase">
        {label}
      </span>
      {children}
    </div>
  )
}

function PnlValue({ value, bold }: { value: number | null; bold?: boolean }) {
  if (value == null) {
    return (
      <span className="font-mono text-xs text-muted-foreground tabular-nums">
        —
      </span>
    )
  }
  return (
    <span
      className={cn(
        "font-mono tabular-nums",
        bold ? "text-sm" : "text-xs",
        value > 0
          ? "text-emerald-500"
          : value < 0
            ? "text-destructive"
            : "text-foreground"
      )}
    >
      {value.toFixed(4)}
    </span>
  )
}

function WinnerBadge({ outcome }: { outcome: string }) {
  const isUp = outcome.toLowerCase() === "up"
  const isDown = outcome.toLowerCase() === "down"
  return (
    <span
      className={cn(
        "flex items-center gap-0.5 font-mono text-[10px] font-semibold tracking-wider uppercase",
        isUp
          ? "text-emerald-500"
          : isDown
            ? "text-destructive"
            : "text-muted-foreground"
      )}
    >
      {isUp ? (
        <TrendingUp className="h-3 w-3" />
      ) : isDown ? (
        <TrendingDown className="h-3 w-3" />
      ) : null}
      {outcome}
    </span>
  )
}

function StateBadge({ state, live }: { state: string; live: boolean }) {
  if (live) {
    return (
      <Badge className="border-transparent bg-emerald-500/15 text-emerald-600 dark:text-emerald-400">
        LIVE
      </Badge>
    )
  }
  if (state === "RESOLVED") {
    return (
      <Badge variant="outline" className="text-muted-foreground">
        RESOLVED
      </Badge>
    )
  }
  return null
}
