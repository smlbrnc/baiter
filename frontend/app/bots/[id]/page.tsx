"use client";

import { useCallback, useEffect, useMemo, useState } from "react";
import { useParams, useRouter } from "next/navigation";
import { ArrowLeft, CircleStop, Play } from "lucide-react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { LogStream } from "@/components/bots/log-stream";
import { PnLWidget } from "@/components/bots/pnl-widget";
import {
  MetricsPanel,
  type LiveMetrics,
} from "@/components/bots/metrics-panel";
import { PriceChart } from "@/components/charts/price-chart";
import { PnLChart } from "@/components/charts/pnl-chart";
import { SpreadSignalChart } from "@/components/charts/spread-signal-chart";
import { api } from "@/lib/api";
import { useBot, useEventStream } from "@/lib/hooks";
import type { FrontendEvent, SessionInfo } from "@/lib/types";

export default function BotDetailPage() {
  const { id } = useParams<{ id: string }>();
  const router = useRouter();
  const botId = Number(id);
  const { bot, pnl } = useBot(Number.isFinite(botId) ? botId : null);

  const [metrics, setMetrics] = useState<LiveMetrics>({
    lastBestBidAsk: null,
    lastSignal: null,
    lastZone: null,
    lastFill: null,
  });
  const [session, setSession] = useState<SessionInfo | null>(null);

  const refreshSession = useCallback(() => {
    if (!Number.isFinite(botId)) return;
    api
      .botSession(botId)
      .then((s) => setSession(s ?? null))
      .catch(() => {});
  }, [botId]);

  useEffect(() => {
    refreshSession();
  }, [refreshSession]);

  const sessionRange = useMemo(
    () => (session ? { start: session.start_ts, end: session.end_ts } : null),
    [session],
  );

  const filter = useMemo(
    () => (ev: FrontendEvent) => "bot_id" in ev && ev.bot_id === botId,
    [botId],
  );

  useEventStream((ev) => {
    switch (ev.kind) {
      case "BestBidAsk":
        setMetrics((m) => ({ ...m, lastBestBidAsk: ev }));
        break;
      case "SignalUpdate":
        setMetrics((m) => ({ ...m, lastSignal: ev }));
        break;
      case "ZoneChanged":
        setMetrics((m) => ({ ...m, lastZone: ev }));
        break;
      case "Fill":
        setMetrics((m) => ({ ...m, lastFill: ev }));
        break;
      case "SessionOpened":
        refreshSession();
        break;
    }
  }, filter);

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
          <div className="flex min-w-0 items-center gap-3">
            {session?.image && (
              <img
                src={session.image}
                alt=""
                className="h-12 w-12 shrink-0 rounded-md object-cover"
              />
            )}
            <div className="min-w-0 space-y-2">
              {session?.title && (
                <h1 className="font-heading truncate text-2xl font-semibold tracking-tight">
                  {session.title}
                </h1>
              )}
              <div className="flex min-w-0 flex-wrap items-center gap-x-2 gap-y-1.5">
                <p className="text-muted-foreground min-w-0 shrink font-mono text-xs break-all sm:max-w-[min(100%,42rem)] sm:break-normal sm:truncate">
                  {session?.slug ?? bot.slug_pattern}
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

      <Card>
        <CardHeader>
          <CardTitle>Özet</CardTitle>
          <CardDescription className="font-mono">
            {bot.slug_pattern}
          </CardDescription>
        </CardHeader>
        <CardContent className="grid grid-cols-2 gap-3 text-sm sm:grid-cols-4">
          <Item label="Order USDC" value={`$${bot.order_usdc.toFixed(2)}`} />
          <Item label="Signal weight" value={String(bot.signal_weight)} />
          <Item
            label="Last active"
            value={
              bot.last_active_ms
                ? new Date(bot.last_active_ms).toLocaleTimeString()
                : "-"
            }
          />
          <Item
            label="Created"
            value={new Date(bot.created_at_ms).toLocaleString()}
          />
        </CardContent>
      </Card>

      <MetricsPanel m={metrics} />

      <PnLWidget pnl={pnl ?? null} />

      <PriceChart botId={botId} session={sessionRange} />
      <SpreadSignalChart botId={botId} session={sessionRange} />
      <PnLChart botId={botId} session={sessionRange} />

      <LogStream botId={botId} />
    </div>
  );
}

function Item({ label, value }: { label: string; value: string }) {
  return (
    <div className="flex flex-col gap-1">
      <span className="text-muted-foreground text-xs font-medium tracking-wide uppercase">
        {label}
      </span>
      <span className="font-mono text-sm leading-snug">{value}</span>
    </div>
  );
}
