"use client"

import { memo, useCallback, useState } from "react"
import Link from "next/link"
import Image from "next/image"
import { CircleStop, Loader2, Play, Trash2 } from "lucide-react"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { CardDescription, CardTitle } from "@/components/ui/card"
import { api } from "@/lib/api"
import { assetLogoForSlug } from "@/lib/market"
import type { BotRow } from "@/lib/types"
import { CARD_SHELL_CLASS, HEADER_RADIAL_GRADIENT } from "@/lib/ui-constants"
import { cn } from "@/lib/utils"

const listBadge = cn(
  "h-4 min-h-4 rounded-sm px-1.5 py-0 text-[10px] leading-none font-normal"
)

function StateBadge({ state }: { state: string }) {
  switch (state) {
    case "RUNNING":
      return (
        <Badge
          className={cn(
            listBadge,
            "border-transparent bg-emerald-500/15 text-emerald-600 dark:text-emerald-400"
          )}
        >
          {state}
        </Badge>
      )
    case "STOPPED":
      return (
        <Badge variant="secondary" className={listBadge}>
          {state}
        </Badge>
      )
    case "CRASHED":
      return (
        <Badge variant="destructive" className={listBadge}>
          {state}
        </Badge>
      )
    default:
      return (
        <Badge variant="outline" className={listBadge}>
          {state}
        </Badge>
      )
  }
}

function ModeBadge({ mode }: { mode: "live" | "dryrun" }) {
  if (mode === "live") {
    return (
      <Badge
        className={cn(
          listBadge,
          "border-transparent bg-primary/15 text-primary uppercase"
        )}
      >
        live
      </Badge>
    )
  }
  return (
    <Badge
      className={cn(
        listBadge,
        "border-transparent bg-amber-500/15 text-amber-700 uppercase dark:text-amber-400"
      )}
    >
      dryrun
    </Badge>
  )
}

/**
 * Tek bir bot kartı — React.memo ile sarılır; yalnız ilgili prop'lar
 * değiştiğinde yeniden render edilir (diğer botların aksiyon/state
 * değişimlerinden etkilenmez).
 */
const BotCard = memo(function BotCard({
  bot,
  isPending,
  onStart,
  onStop,
  onDelete,
}: {
  bot: BotRow
  isPending: boolean
  onStart: (bot: BotRow) => void
  onStop: (bot: BotRow) => void
  onDelete: (bot: BotRow) => void
}) {
  return (
    <article className={cn(CARD_SHELL_CLASS, "@container min-w-0")}>
      <Link
        href={`/bots/${bot.id}`}
        className="group relative block overflow-hidden border-b border-border/45 bg-gradient-to-br from-muted/35 via-background to-background px-3 py-2.5 no-underline transition-colors outline-none hover:from-muted/45 hover:via-background focus-visible:ring-2 focus-visible:ring-ring/50 focus-visible:ring-offset-0 sm:px-4 sm:py-3"
      >
        <div
          aria-hidden
          className="pointer-events-none absolute inset-0 z-0 opacity-[0.35]"
          style={{ backgroundImage: HEADER_RADIAL_GRADIENT }}
        />
        <div className="relative z-10 flex gap-3">
          <div
            className="relative size-9 shrink-0 overflow-hidden rounded-md sm:size-10"
            aria-hidden
          >
            <Image
              src={assetLogoForSlug(bot.slug_pattern)}
              alt=""
              fill
              className="object-contain"
              sizes="(max-width: 640px) 36px, 40px"
            />
          </div>
          <div className="min-w-0 flex-1 space-y-1">
            <span className="block truncate font-heading text-sm font-semibold tracking-tight text-foreground transition-colors group-hover:text-primary sm:text-base">
              {bot.name}
            </span>
            <div className="flex flex-wrap items-center gap-1">
              <Badge variant="outline" className={cn(listBadge, "uppercase")}>
                {bot.strategy}
              </Badge>
              <ModeBadge mode={bot.run_mode} />
              <StateBadge state={bot.state} />
            </div>
          </div>
        </div>
      </Link>

      <div className="space-y-2 px-3 py-2.5 sm:px-4 sm:py-3">
        <p className="text-center font-mono text-[11px] leading-snug break-words text-muted-foreground sm:text-xs">
          {bot.slug_pattern}
        </p>
        <div className="space-y-2 rounded-md border border-border/40 bg-muted/20 p-2.5 shadow-xs sm:p-3">
          <p className="text-[10px] font-medium tracking-wide text-muted-foreground uppercase">
            Settings
          </p>
          <dl className="grid grid-cols-1 gap-2">
            <div className="flex items-baseline justify-between gap-3 sm:block sm:space-y-0">
              <dt className="text-[11px] text-muted-foreground sm:text-xs">
                Order (USDC)
              </dt>
              <dd className="text-right font-mono text-xs font-medium text-foreground tabular-nums sm:text-left sm:text-sm">
                ${bot.order_usdc.toFixed(2)}
              </dd>
            </div>
          </dl>
        </div>
      </div>

      <div className="flex items-center justify-between gap-2 border-t border-border/45 px-3 py-2 sm:px-4">
        {bot.state === "RUNNING" ? (
          <Button
            size="sm"
            variant="secondary"
            title="Stop"
            aria-label="Stop bot"
            className="shadow-xs"
            disabled={isPending}
            onClick={() => onStop(bot)}
          >
            {isPending ? <Loader2 className="animate-spin" /> : <CircleStop />}
            Stop
          </Button>
        ) : (
          <Button
            size="sm"
            title="Start"
            aria-label="Start bot"
            className="shadow-xs"
            disabled={isPending}
            onClick={() => onStart(bot)}
          >
            {isPending ? <Loader2 className="animate-spin" /> : <Play />}
            Start
          </Button>
        )}
        <Button
          size="icon-sm"
          variant="destructive"
          title="Delete"
          aria-label="Delete bot"
          className="shadow-xs"
          disabled={isPending}
          onClick={() => onDelete(bot)}
        >
          {isPending ? <Loader2 className="animate-spin" /> : <Trash2 />}
        </Button>
      </div>
    </article>
  )
})

export function BotList({
  bots,
  onChanged,
  patch,
  remove,
}: {
  bots: BotRow[]
  onChanged: () => void
  patch: (id: number, partial: Partial<BotRow>) => void
  remove: (id: number) => void
}) {
  const [pending, setPending] = useState<Record<number, boolean>>({})

  const setPendingFor = (id: number, val: boolean) =>
    setPending((p) => ({ ...p, [id]: val }))

  const doStart = useCallback(
    async (bot: BotRow) => {
      if (pending[bot.id]) return
      const prevState = bot.state
      patch(bot.id, { state: "RUNNING" })
      setPendingFor(bot.id, true)
      try {
        await api.startBot(bot.id)
      } catch {
        patch(bot.id, { state: prevState })
      } finally {
        setPendingFor(bot.id, false)
      }
    },
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [patch, pending]
  )

  const doStop = useCallback(
    async (bot: BotRow) => {
      if (pending[bot.id]) return
      const prevState = bot.state
      patch(bot.id, { state: "STOPPED" })
      setPendingFor(bot.id, true)
      try {
        await api.stopBot(bot.id)
      } catch {
        patch(bot.id, { state: prevState })
      } finally {
        setPendingFor(bot.id, false)
      }
    },
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [patch, pending]
  )

  const doDelete = useCallback(
    async (bot: BotRow) => {
      if (pending[bot.id]) return
      if (!confirm(`Bot #${bot.id} silinsin mi?`)) return
      remove(bot.id)
      setPendingFor(bot.id, true)
      try {
        await api.deleteBot(bot.id)
        onChanged()
      } catch {
        onChanged() // hata durumunda reload ile geri getir
      } finally {
        setPendingFor(bot.id, false)
      }
    },
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [remove, onChanged, pending]
  )

  if (bots.length === 0) {
    return (
      <div className={CARD_SHELL_CLASS}>
        <div className="border-b border-border/45 bg-gradient-to-br from-muted/35 via-background to-background px-3 py-3 sm:px-4">
          <CardTitle className="font-heading text-sm font-semibold tracking-tight sm:text-base">
            Bots
          </CardTitle>
          <CardDescription className="mt-0.5 text-xs text-muted-foreground sm:text-sm">
            No bots yet. Create one from New bot.
          </CardDescription>
        </div>
      </div>
    )
  }

  return (
    <div className="grid grid-cols-1 gap-3 md:grid-cols-2 xl:grid-cols-3">
      {bots.map((b) => (
        <BotCard
          key={b.id}
          bot={b}
          isPending={pending[b.id] ?? false}
          onStart={doStart}
          onStop={doStop}
          onDelete={doDelete}
        />
      ))}
    </div>
  )
}
