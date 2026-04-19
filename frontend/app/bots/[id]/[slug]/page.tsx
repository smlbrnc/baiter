"use client";

import { useCallback, useEffect, useMemo, useState } from "react";
import Link from "next/link";
import { useParams, useRouter } from "next/navigation";
import { ArrowLeft } from "lucide-react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  Card,
  CardContent,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import {
  MetricsPanel,
  type LiveMetrics,
} from "@/components/bots/metrics-panel";
import { PriceChart } from "@/components/charts/price-chart";
import { PnLChart } from "@/components/charts/pnl-chart";
import { SpreadSignalChart } from "@/components/charts/spread-signal-chart";
import { api } from "@/lib/api";
import { useEventStream, useHistoryStream } from "@/lib/hooks";
import type {
  FrontendEvent,
  MarketTick,
  PnLSnapshot,
  SessionDetail,
} from "@/lib/types";

export default function MarketDetailPage() {
  const { id, slug } = useParams<{ id: string; slug: string }>();
  const botId = Number(id);

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
      <div className="space-y-4">
        <BackButton botId={botId} />
        <p className="text-muted-foreground text-sm">Yükleniyor…</p>
      </div>
    );
  }

  if (!detail) {
    return (
      <div className="space-y-4">
        <BackButton botId={botId} />
        <Card>
          <CardContent className="text-muted-foreground p-6 text-sm">
            Bu slug için session bulunamadı: <span className="font-mono">{slug}</span>
          </CardContent>
        </Card>
      </div>
    );
  }

  return (
    <div className="space-y-6">
      <div className="flex flex-col gap-4 lg:flex-row lg:items-start lg:justify-between">
        <div className="flex min-w-0 items-start gap-3">
          <BackButton botId={botId} />
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
      </div>

      <Card>
        <CardHeader>
          <CardTitle>Pozisyon</CardTitle>
        </CardHeader>
        <CardContent className="grid grid-cols-2 gap-3 text-sm sm:grid-cols-4">
          <Item label="Cost basis" value={`$${detail.cost_basis.toFixed(4)}`} />
          <Item label="Fee total" value={`$${detail.fee_total.toFixed(4)}`} />
          <Item label="Shares YES" value={detail.shares_yes.toFixed(2)} />
          <Item label="Shares NO" value={detail.shares_no.toFixed(2)} />
          <Item
            label="Realized PnL"
            value={
              detail.realized_pnl != null
                ? detail.realized_pnl.toFixed(4)
                : "—"
            }
          />
          <Item
            label="Window start"
            value={new Date(detail.start_ts * 1000).toLocaleString()}
          />
          <Item
            label="Window end"
            value={new Date(detail.end_ts * 1000).toLocaleString()}
          />
        </CardContent>
      </Card>

      {isLive && <MetricsPanel m={metrics} />}

      <PriceChart data={ticks} session={sessionRange} />
      <SpreadSignalChart data={ticks} session={sessionRange} />
      <PnLChart data={pnlHistory} session={sessionRange} />
    </div>
  );
}

function BackButton({ botId }: { botId: number }) {
  const router = useRouter();
  return (
    <div className="flex items-center gap-2">
      <Button
        variant="outline"
        size="icon"
        className="shrink-0"
        onClick={() => router.back()}
      >
        <ArrowLeft />
      </Button>
      <Link
        href={`/bots/${botId}`}
        className="text-muted-foreground hover:text-foreground text-xs underline-offset-4 hover:underline"
      >
        Bot detayına dön
      </Link>
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
