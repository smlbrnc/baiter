"use client";

import { useCallback, useEffect, useMemo, useState } from "react";
import { useParams, useRouter } from "next/navigation";
import { ArrowLeft, CircleStop, Play } from "lucide-react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent } from "@/components/ui/card";
import {
  MetricsPanel,
  type LiveMetrics,
} from "@/components/bots/metrics-panel";
import { PnLWidget } from "@/components/bots/pnl-widget";
import { TradesTable } from "@/components/bots/trades-table";
import { PriceChart } from "@/components/charts/price-chart";
import { PnLChart } from "@/components/charts/pnl-chart";
import { SpreadSignalChart } from "@/components/charts/spread-signal-chart";
import { BotSettingsCards } from "@/components/bots/bot-settings-cards";
import { api } from "@/lib/api";
import { useBot, useEventStream, useHistoryStream } from "@/lib/hooks";
import type {
  FrontendEvent,
  MarketTick,
  PnLSnapshot,
  SessionDetail,
} from "@/lib/types";

export default function MarketDetailPage() {
  const { id, slug } = useParams<{ id: string; slug: string }>();
  const botId = Number(id);
  const { bot } = useBot(Number.isFinite(botId) ? botId : null);

  const [detail, setDetail] = useState<SessionDetail | null>(null);
  const [loaded, setLoaded] = useState(false);

  useEffect(() => {
    if (!Number.isFinite(botId) || !slug) return;
    let cancelled = false;
    const reload = () =>
      api
        .sessionDetail(botId, slug)
        .then((d) => {
          if (cancelled) return;
          setDetail(d);
          setLoaded(true);
        })
        .catch(() => {
          if (!cancelled) setLoaded(true);
        });
    reload();
    const t = setInterval(reload, 5000);
    return () => {
      cancelled = true;
      clearInterval(t);
    };
  }, [botId, slug]);

  const isLive = detail?.is_live ?? false;

  const fetchTicks = useCallback(
    (sinceMs: number) => api.sessionTicks(botId, slug, sinceMs),
    [botId, slug],
  );
  const fetchPnl = useCallback(
    (sinceMs: number) => api.sessionPnlHistory(botId, slug, sinceMs),
    [botId, slug],
  );

  const ticks = useHistoryStream<MarketTick>({
    fetchInitial: fetchTicks,
    isLive,
    pollMs: 1500,
  });
  const pnlHistory = useHistoryStream<PnLSnapshot>({
    fetchInitial: fetchPnl,
    isLive,
    pollMs: 1500,
  });

  const sessionRange = useMemo(
    () =>
      detail ? { start: detail.start_ts, end: detail.end_ts } : null,
    [detail],
  );

  const [metrics, setMetrics] = useState<LiveMetrics>({
    lastBestBidAsk: null,
    lastSignal: null,
    lastFill: null,
  });

  const filter = useMemo(
    () => (ev: FrontendEvent) => "bot_id" in ev && ev.bot_id === botId,
    [botId],
  );

  useEventStream((ev) => {
    if (!isLive) return;
    switch (ev.kind) {
      case "BestBidAsk":
        setMetrics((m) => ({ ...m, lastBestBidAsk: ev }));
        break;
      case "SignalUpdate":
        setMetrics((m) => ({ ...m, lastSignal: ev }));
        break;
      case "Fill":
        setMetrics((m) => ({ ...m, lastFill: ev }));
        break;
    }
  }, filter);

  if (!loaded) {
    return (
      <div className="space-y-6">
        <div className="space-y-4">
          <BackButton />
          <p className="text-muted-foreground text-sm">Yükleniyor…</p>
        </div>
        {bot && <BotSettingsCards bot={bot} />}
      </div>
    );
  }

  if (!detail) {
    return (
      <div className="space-y-6">
          <BackButton />
        {bot && <BotSettingsCards bot={bot} />}
        <Card>
          <CardContent className="text-muted-foreground p-6 text-sm">
            Bu slug için session bulunamadı:{" "}
            <span className="font-mono">{slug}</span>
          </CardContent>
        </Card>
      </div>
    );
  }

  return (
    <div className="space-y-6">
      <div className="flex flex-col gap-4 lg:flex-row lg:items-start lg:justify-between">
        <div className="flex min-w-0 items-start gap-3">
          <BackButton />
          {detail.image && (
            // eslint-disable-next-line @next/next/no-img-element
            <img
              src={detail.image}
              alt=""
              className="h-12 w-12 shrink-0 rounded-md object-cover"
            />
          )}
          <div className="min-w-0 space-y-2">
            {detail.title && (
              <h1 className="font-heading truncate text-2xl font-semibold tracking-tight">
                {detail.title}
              </h1>
            )}
            <div className="flex flex-wrap items-center gap-x-2 gap-y-1.5">
              <p className="text-muted-foreground font-mono text-xs break-all">
                {detail.slug}
              </p>
              {isLive ? (
                <Badge className="border-transparent bg-emerald-500/15 text-emerald-600 dark:text-emerald-400">
                  LIVE
                </Badge>
              ) : (
                <Badge variant="outline">{detail.state}</Badge>
              )}
            </div>
          </div>
        </div>
        {bot && (
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
        )}
      </div>

      {bot && <BotSettingsCards bot={bot} />}

      <PnLWidget
        pnl={pnlHistory[pnlHistory.length - 1] ?? null}
        session={sessionRange}
      />

      {isLive && <MetricsPanel m={metrics} />}

      <PriceChart data={ticks} session={sessionRange} />
      <SpreadSignalChart data={ticks} session={sessionRange} />
      <PnLChart data={pnlHistory} session={sessionRange} />

      <TradesTable botId={botId} slug={slug} isLive={isLive} />
    </div>
  );
}

function BackButton() {
  const router = useRouter();
  return (
    <Button
      variant="outline"
      size="icon"
      className="shrink-0"
      onClick={() => router.back()}
    >
      <ArrowLeft />
    </Button>
  );
}
