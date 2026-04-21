"use client";

import { useCallback, useEffect, useMemo, useState } from "react";
import { useParams } from "next/navigation";
import { CircleStop, Play } from "lucide-react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent } from "@/components/ui/card";
import {
  BotDetailHeader,
  PageBackButton,
  type SessionMarketProgress,
} from "@/components/bots/bot-detail-header";
import { PnLWidget } from "@/components/bots/pnl-widget";
import { TradesTable } from "@/components/bots/trades-table";
import { PriceChart } from "@/components/charts/price-chart";
import { AvgSumChart } from "@/components/charts/avg-sum-chart";
import { BinanceSignalPanel } from "@/components/charts/binance-signal-panel";
import { SpreadSignalChart } from "@/components/charts/spread-signal-chart";
import { BotSettingsCards } from "@/components/bots/bot-settings-cards";
import { api } from "@/lib/api";
import { useBot, useHistoryStream } from "@/lib/hooks";
import type { MarketTick, PnLSnapshot, SessionDetail } from "@/lib/types";

export default function MarketDetailPage() {
  const { id, slug } = useParams<{ id: string; slug: string }>();
  const botId = Number(id);
  const { bot } = useBot(Number.isFinite(botId) ? botId : null, 5000);

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
    const t = setInterval(reload, 10_000);
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
    pollMs: 3000,
    maxItems: 800,
  });
  const pnlHistory = useHistoryStream<PnLSnapshot>({
    fetchInitial: fetchPnl,
    isLive,
    pollMs: 3000,
    maxItems: 500,
  });

  const sessionRange = useMemo(
    () =>
      detail ? { start: detail.start_ts, end: detail.end_ts } : null,
    [detail],
  );

  const [tickSec, setTickSec] = useState(() => Math.floor(Date.now() / 1000));
  useEffect(() => {
    if (!isLive || !sessionRange) return;
    setTickSec(Math.floor(Date.now() / 1000));
    const id = setInterval(() => setTickSec(Math.floor(Date.now() / 1000)), 1000);
    return () => clearInterval(id);
  }, [isLive, sessionRange]);

  const marketProgress = useMemo((): SessionMarketProgress | null => {
    if (!sessionRange || sessionRange.end <= sessionRange.start) return null;
    const last = pnlHistory[pnlHistory.length - 1];
    let tSec: number;
    if (last) {
      tSec = isLive
        ? Math.max(last.ts_ms / 1000, tickSec)
        : last.ts_ms / 1000;
    } else if (isLive) {
      tSec = tickSec;
    } else {
      tSec = sessionRange.start;
    }
    const span = sessionRange.end - sessionRange.start;
    const pct = Math.min(
      100,
      Math.max(0, ((tSec - sessionRange.start) / span) * 100),
    );
    const fmtHm = (ts: number) =>
      new Date(ts * 1000).toLocaleTimeString([], {
        hour: "2-digit",
        minute: "2-digit",
      });
    const centerLabel = new Date(tSec * 1000).toLocaleTimeString([], {
      hour: "2-digit",
      minute: "2-digit",
      second: "2-digit",
    });
    return {
      pct,
      startLabel: fmtHm(sessionRange.start),
      endLabel: fmtHm(sessionRange.end),
      centerLabel,
    };
  }, [sessionRange, pnlHistory, isLive, tickSec]);

  if (!loaded) {
    return (
      <div className="space-y-4">
        <div className="flex items-center gap-3">
          <PageBackButton />
          <p className="text-muted-foreground text-sm">Yükleniyor…</p>
        </div>
        {bot && <BotSettingsCards bot={bot} />}
      </div>
    );
  }

  if (!detail) {
    return (
      <div className="space-y-4">
        <PageBackButton />
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

  const marketTitle =
    detail.title?.trim() ? detail.title : detail.slug;
  const stateBadgeClass =
    "h-5 border px-1.5 text-[10px] font-semibold uppercase tracking-wide";

  return (
    <div className="space-y-4">
      <BotDetailHeader
        imageUrl={detail.image}
        title={marketTitle}
        subtitle={detail.slug}
        marketProgress={marketProgress}
        badges={
          isLive ? (
            <Badge
              className={`${stateBadgeClass} border-transparent bg-emerald-500/15 text-emerald-600 dark:text-emerald-400`}
            >
              LIVE
            </Badge>
          ) : (
            <Badge variant="outline" className={stateBadgeClass}>
              {detail.state}
            </Badge>
          )
        }
        actions={
          bot ? (
            bot.state === "RUNNING" ? (
              <Button
                size="sm"
                variant="secondary"
                className="gap-1.5"
                onClick={async () => {
                  try {
                    await api.stopBot(bot.id);
                  } catch {
                    /* yut */
                  }
                }}
              >
                <CircleStop className="size-4" />
                Durdur
              </Button>
            ) : (
              <Button
                size="sm"
                className="gap-1.5"
                onClick={async () => {
                  try {
                    await api.startBot(bot.id);
                  } catch {
                    /* yut */
                  }
                }}
              >
                <Play className="size-4" />
                Başlat
              </Button>
            )
          ) : null
        }
      />

      <div className="flex flex-col gap-3">
        {bot && <BotSettingsCards bot={bot} />}
        <PnLWidget pnl={pnlHistory[pnlHistory.length - 1] ?? null} />
      </div>

      <PriceChart data={ticks} session={sessionRange} />
      <SpreadSignalChart data={ticks} session={sessionRange} />
      <div className="grid grid-cols-1 gap-3 lg:grid-cols-2 lg:items-stretch">
        <BinanceSignalPanel
          data={ticks}
          strategyParams={bot?.strategy_params ?? null}
        />
        <AvgSumChart data={pnlHistory} session={sessionRange} />
      </div>

      <TradesTable botId={botId} slug={slug} isLive={isLive} />
    </div>
  );
}
