"use client"

import { useCallback, useState } from "react"
import { useParams } from "next/navigation"
import {
  CircleStop,
  LineChart,
  Loader2,
  Play,
  ScrollText,
  Settings as SettingsIcon,
} from "lucide-react"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
  BotDetailHeader,
  PageBackButton,
} from "@/components/bots/bot-detail-header"
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs"
import { BotSettingsCards } from "@/components/bots/bot-settings-cards"
import { LogStream } from "@/components/bots/log-stream"
import { SessionsTable } from "@/components/bots/sessions-table"
import { BotSettingsEditForm } from "@/components/bots/bot-settings-edit-form"
import { api } from "@/lib/api"
import { useBot } from "@/lib/hooks"

export default function BotSummaryPage() {
  const { id } = useParams<{ id: string }>()
  const botId = Number(id)
  const { bot, mutate } = useBot(Number.isFinite(botId) ? botId : null)
  const [pending, setPending] = useState(false)

  const doStart = useCallback(async () => {
    if (!bot || pending) return
    const prevState = bot.state
    mutate({ state: "RUNNING" })
    setPending(true)
    try {
      await api.startBot(bot.id)
    } catch {
      mutate({ state: prevState })
    } finally {
      setPending(false)
    }
  }, [bot, pending, mutate])

  const doStop = useCallback(async () => {
    if (!bot || pending) return
    const prevState = bot.state
    mutate({ state: "STOPPED" })
    setPending(true)
    try {
      await api.stopBot(bot.id)
    } catch {
      mutate({ state: prevState })
    } finally {
      setPending(false)
    }
  }, [bot, pending, mutate])

  if (!bot) {
    return (
      <div className="flex flex-col gap-3">
        <PageBackButton />
        <p className="text-sm text-muted-foreground">Yükleniyor…</p>
      </div>
    )
  }

  const badgeBase =
    "h-5 border px-1.5 text-[10px] font-semibold uppercase tracking-wide"

  return (
    <div className="space-y-4">
      <BotDetailHeader
        title={bot.name}
        subtitle={bot.slug_pattern}
        badges={
          <>
            <Badge variant="outline" className={badgeBase}>
              {bot.strategy}
            </Badge>
            <Badge
              className={`${badgeBase} border-transparent ${
                bot.run_mode === "live"
                  ? "bg-primary/15 text-primary"
                  : "bg-amber-500/15 text-amber-700 dark:text-amber-400"
              }`}
            >
              {bot.run_mode}
            </Badge>
            <Badge
              className={`${badgeBase} border-transparent ${
                bot.state === "RUNNING"
                  ? "bg-emerald-500/15 text-emerald-600 dark:text-emerald-400"
                  : "bg-secondary text-secondary-foreground"
              }`}
            >
              {bot.state}
            </Badge>
          </>
        }
        actions={
          bot.state === "RUNNING" ? (
            <Button
              size="sm"
              variant="secondary"
              className="gap-1.5"
              disabled={pending}
              onClick={doStop}
            >
              {pending ? (
                <Loader2 className="size-4 animate-spin" />
              ) : (
                <CircleStop className="size-4" />
              )}
              Durdur
            </Button>
          ) : (
            <Button
              size="sm"
              className="gap-1.5"
              disabled={pending}
              onClick={doStart}
            >
              {pending ? (
                <Loader2 className="size-4 animate-spin" />
              ) : (
                <Play className="size-4" />
              )}
              Başlat
            </Button>
          )
        }
      />

      <Tabs defaultValue="markets" className="w-full">
        <div className="border-b border-border/50">
          <TabsList
            variant="line"
            className="!h-10 w-full justify-start gap-1 rounded-none !p-0"
          >
            <TabTrigger value="markets" icon={<LineChart />} label="Markets" />
            <TabTrigger value="logs" icon={<ScrollText />} label="Logs" />
            <TabTrigger
              value="settings"
              icon={<SettingsIcon />}
              label="Settings"
            />
          </TabsList>
        </div>

        <TabsContent value="markets" className="mt-4 space-y-4">
          <BotSettingsCards bot={bot} />
          <SessionsTable botId={botId} />
        </TabsContent>

        <TabsContent value="logs" className="mt-4">
          <LogStream botId={botId} />
        </TabsContent>

        <TabsContent value="settings" className="mt-4">
          <BotSettingsEditForm key={bot.id} bot={bot} />
        </TabsContent>
      </Tabs>
    </div>
  )
}

/**
 * Underlined tab tetikleyici — line variant ile uyumlu, ikon + etiket.
 */
function TabTrigger({
  value,
  icon,
  label,
}: {
  value: string
  icon: React.ReactNode
  label: string
}) {
  return (
    <TabsTrigger
      value={value}
      className="!flex-none gap-2 px-3 text-sm font-medium [&_svg]:size-4"
    >
      {icon}
      {label}
    </TabsTrigger>
  )
}
