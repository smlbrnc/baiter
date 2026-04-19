"use client";

import { useParams, useRouter } from "next/navigation";
import {
  ArrowLeft,
  CircleStop,
  LineChart,
  Play,
  ScrollText,
  Settings as SettingsIcon,
} from "lucide-react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  Tabs,
  TabsContent,
  TabsList,
  TabsTrigger,
} from "@/components/ui/tabs";
import { BotSettingsCards } from "@/components/bots/bot-settings-cards";
import { LogStream } from "@/components/bots/log-stream";
import { SessionsTable } from "@/components/bots/sessions-table";
import { BotSettingsEditForm } from "@/components/bots/bot-settings-edit-form";
import { api } from "@/lib/api";
import { useBot } from "@/lib/hooks";

export default function BotSummaryPage() {
  const { id } = useParams<{ id: string }>();
  const router = useRouter();
  const botId = Number(id);
  const { bot } = useBot(Number.isFinite(botId) ? botId : null);

  if (!bot) {
    return (
      <div className="space-y-4">
        <Button variant="ghost" onClick={() => router.back()}>
          <ArrowLeft />
          Geri
        </Button>
        <p className="text-muted-foreground text-sm">Yükleniyor…</p>
      </div>
    );
  }

  return (
    <div className="space-y-6">
      <div className="flex flex-col gap-4 lg:flex-row lg:items-start lg:justify-between">
        <div className="flex min-w-0 flex-col gap-3 sm:flex-row sm:items-center sm:gap-3">
          <Button
            variant="outline"
            size="icon"
            className="shrink-0"
            onClick={() => router.back()}
          >
            <ArrowLeft />
          </Button>
          <div className="min-w-0 space-y-2">
            <h1 className="font-heading truncate text-2xl font-semibold tracking-tight">
              {bot.name}
            </h1>
            <div className="flex min-w-0 flex-wrap items-center gap-x-2 gap-y-1.5">
              <p className="text-muted-foreground min-w-0 shrink font-mono text-xs break-all">
                {bot.slug_pattern}
              </p>
              <div className="flex shrink-0 flex-wrap items-center gap-2">
                <Badge variant="outline">{bot.strategy}</Badge>
                <Badge
                  className={
                    bot.run_mode === "live"
                      ? "border-transparent bg-primary/15 text-primary"
                      : "border-transparent bg-amber-500/15 text-amber-700 dark:text-amber-400"
                  }
                >
                  {bot.run_mode}
                </Badge>
                <Badge
                  className={
                    bot.state === "RUNNING"
                      ? "border-transparent bg-emerald-500/15 text-emerald-600 dark:text-emerald-400"
                      : "border-transparent bg-secondary text-secondary-foreground"
                  }
                >
                  {bot.state}
                </Badge>
              </div>
            </div>
          </div>
        </div>
        <div className="flex shrink-0 flex-wrap gap-2">
          {bot.state === "RUNNING" ? (
            <Button
              size="lg"
              variant="secondary"
              onClick={async () => {
                try {
                  await api.stopBot(bot.id);
                } catch {
                  /* yut */
                }
              }}
            >
              <CircleStop />
              Durdur
            </Button>
          ) : (
            <Button
              size="lg"
              onClick={async () => {
                try {
                  await api.startBot(bot.id);
                } catch {
                  /* yut */
                }
              }}
            >
              <Play />
              Başlat
            </Button>
          )}
        </div>
      </div>

      <Tabs defaultValue="markets" className="w-full">
        <div className="border-border/50 border-b">
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

        <TabsContent value="markets" className="mt-6 space-y-6">
          <BotSettingsCards bot={bot} />
          <SessionsTable botId={botId} />
        </TabsContent>

        <TabsContent value="logs" className="mt-6">
          <LogStream botId={botId} />
        </TabsContent>

        <TabsContent value="settings" className="mt-6">
          <BotSettingsEditForm key={bot.id} bot={bot} />
        </TabsContent>
      </Tabs>
    </div>
  );
}

/**
 * Underlined tab tetikleyici — line variant ile uyumlu, ikon + etiket.
 * Active state'te tab'in altında foreground renginde ince çubuk belirir
 * (TabsTrigger'in `after:` pseudo-element'i `data-active`'de açılır).
 */
function TabTrigger({
  value,
  icon,
  label,
}: {
  value: string;
  icon: React.ReactNode;
  label: string;
}) {
  return (
    <TabsTrigger
      value={value}
      className="!flex-none gap-2 px-3 text-sm font-medium [&_svg]:size-4"
    >
      {icon}
      {label}
    </TabsTrigger>
  );
}

