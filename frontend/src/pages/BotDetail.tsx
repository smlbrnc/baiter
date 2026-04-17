import { useMemo, useState } from "react";
import { useNavigate, useParams } from "react-router-dom";
import { ArrowLeft } from "lucide-react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { LogStream } from "@/components/LogStream";
import { PnLWidget } from "@/components/PnLWidget";
import { MetricsPanel, type LiveMetrics } from "@/components/MetricsPanel";
import { ZoneTimeline } from "@/components/ZoneTimeline";
import { PriceChart } from "@/components/charts/PriceChart";
import { SpreadChart } from "@/components/charts/SpreadChart";
import { PnLChart } from "@/components/charts/PnLChart";
import { SignalChart } from "@/components/charts/SignalChart";
import { api } from "@/lib/api";
import { useBot, useEventStream } from "@/lib/hooks";
import type { FrontendEvent } from "@/lib/types";

export function BotDetail() {
  const { id } = useParams<{ id: string }>();
  const nav = useNavigate();
  const botId = Number(id);
  const { bot, pnl } = useBot(Number.isFinite(botId) ? botId : null);

  const [metrics, setMetrics] = useState<LiveMetrics>({
    lastBestBidAsk: null,
    lastSignal: null,
    lastZone: null,
    lastFill: null,
  });
  const [sessionRange, setSessionRange] = useState<{ start: number; end: number } | null>(
    null,
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
        setSessionRange({ start: ev.start_ts, end: ev.end_ts });
        break;
    }
  }, filter);

  if (!bot) {
    return (
      <div className="space-y-4">
        <Button variant="ghost" onClick={() => nav(-1)}>
          <ArrowLeft className="h-4 w-4" /> Geri
        </Button>
        <p className="text-sm text-muted-foreground">Yükleniyor…</p>
      </div>
    );
  }

  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-3">
          <Button variant="ghost" size="icon" onClick={() => nav(-1)}>
            <ArrowLeft className="h-4 w-4" />
          </Button>
          <h1 className="text-2xl font-semibold tracking-tight">{bot.name}</h1>
          <Badge variant="outline">{bot.strategy}</Badge>
          <Badge variant={bot.run_mode === "live" ? "default" : "warn"}>
            {bot.run_mode}
          </Badge>
          <Badge variant={bot.state === "RUNNING" ? "success" : "secondary"}>
            {bot.state}
          </Badge>
        </div>
        <div className="flex gap-2">
          {bot.state === "RUNNING" ? (
            <Button
              variant="secondary"
              onClick={async () => {
                await api.stopBot(bot.id);
              }}
            >
              Durdur
            </Button>
          ) : (
            <Button
              onClick={async () => {
                await api.startBot(bot.id);
              }}
            >
              Başlat
            </Button>
          )}
        </div>
      </div>

      <Card>
        <CardHeader>
          <CardTitle>Özet</CardTitle>
          <CardDescription>{bot.slug_pattern}</CardDescription>
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

      <PriceChart
        botId={botId}
        windowStartTs={sessionRange?.start}
        windowEndTs={sessionRange?.end}
      />
      <SpreadChart botId={botId} />
      <div className="grid gap-6 lg:grid-cols-2">
        <PnLChart botId={botId} />
        <SignalChart botId={botId} />
      </div>

      <LogStream botId={botId} />
    </div>
  );
}

function Item({ label, value }: { label: string; value: string }) {
  return (
    <div className="flex flex-col">
      <span className="text-xs uppercase text-muted-foreground">{label}</span>
      <span className="font-mono text-sm">{value}</span>
    </div>
  );
}
