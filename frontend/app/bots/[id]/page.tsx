"use client";

import { useEffect, useMemo, useState } from "react";
import { useParams, useRouter } from "next/navigation";
import { HugeiconsIcon } from "@hugeicons/react";
import {
  ArrowLeft01FreeIcons,
  PlayFreeIcons,
  StopCircleFreeIcons,
} from "@hugeicons/core-free-icons";
import { toast } from "sonner";
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
import { ZoneTimeline } from "@/components/bots/zone-timeline";
import { PriceChart } from "@/components/charts/price-chart";
import { SpreadChart } from "@/components/charts/spread-chart";
import { PnLChart } from "@/components/charts/pnl-chart";
import { SignalChart } from "@/components/charts/signal-chart";
import { api } from "@/lib/api";
import { useBot, useEventStream } from "@/lib/hooks";
import type { FrontendEvent } from "@/lib/types";

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
  const [sessionRange, setSessionRange] = useState<{
    start: number;
    end: number;
  } | null>(null);

  useEffect(() => {
    if (!Number.isFinite(botId)) return;
    let cancelled = false;
    api
      .botSession(botId)
      .then((s) => {
        if (cancelled || !s) return;
        setSessionRange({ start: s.start_ts, end: s.end_ts });
      })
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  }, [botId]);

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
        setSessionRange({ start: ev.start_ts, end: ev.end_ts });
        break;
    }
  }, filter);

  if (!bot) {
    return (
      <div className="space-y-4">
        <Button variant="ghost" onClick={() => router.back()}>
          <HugeiconsIcon icon={ArrowLeft01FreeIcons} />
          Geri
        </Button>
        <p className="text-muted-foreground text-sm">Yükleniyor…</p>
      </div>
    );
  }

  return (
    <div className="space-y-6">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div className="flex items-center gap-3">
          <Button
            variant="ghost"
            size="icon"
            onClick={() => router.back()}
          >
            <HugeiconsIcon icon={ArrowLeft01FreeIcons} />
          </Button>
          <h1 className="text-xl font-semibold tracking-tight">{bot.name}</h1>
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
                : "bg-secondary text-secondary-foreground border-transparent"
            }
          >
            {bot.state}
          </Badge>
        </div>
        <div className="flex gap-2">
          {bot.state === "RUNNING" ? (
            <Button
              size="lg"
              variant="secondary"
              onClick={async () => {
                try {
                  await api.stopBot(bot.id);
                  toast.success(`Bot #${bot.id} durduruldu`);
                } catch (e) {
                  toast.error((e as Error).message);
                }
              }}
            >
              <HugeiconsIcon icon={StopCircleFreeIcons} />
              Durdur
            </Button>
          ) : (
            <Button
              size="lg"
              onClick={async () => {
                try {
                  await api.startBot(bot.id);
                  toast.success(`Bot #${bot.id} başlatıldı`);
                } catch (e) {
                  toast.error((e as Error).message);
                }
              }}
            >
              <HugeiconsIcon icon={PlayFreeIcons} />
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

      <ZoneTimeline
        zone={metrics.lastZone?.zone ?? null}
        pct={metrics.lastZone?.zone_pct ?? null}
      />

      <PnLWidget pnl={pnl ?? null} />

      <PriceChart botId={botId} session={sessionRange} />
      <SpreadChart botId={botId} session={sessionRange} />
      <div className="grid gap-6 lg:grid-cols-2">
        <PnLChart botId={botId} session={sessionRange} />
        <SignalChart botId={botId} session={sessionRange} />
      </div>

      <LogStream botId={botId} />
    </div>
  );
}

function Item({ label, value }: { label: string; value: string }) {
  return (
    <div className="flex flex-col">
      <span className="text-muted-foreground text-[10px] tracking-wider uppercase">
        {label}
      </span>
      <span className="font-mono text-sm">{value}</span>
    </div>
  );
}
