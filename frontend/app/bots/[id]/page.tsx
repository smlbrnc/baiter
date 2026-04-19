"use client";

import Link from "next/link";
import { useEffect, useState } from "react";
import { useParams, useRouter } from "next/navigation";
import { ArrowLeft, ArrowRight, CircleStop, Play } from "lucide-react";
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
import { SessionsTable } from "@/components/bots/sessions-table";
import { api } from "@/lib/api";
import { useBot } from "@/lib/hooks";
import type { SessionListItem } from "@/lib/types";

export default function BotSummaryPage() {
  const { id } = useParams<{ id: string }>();
  const router = useRouter();
  const botId = Number(id);
  const { bot, pnl } = useBot(Number.isFinite(botId) ? botId : null);

  const [sessions, setSessions] = useState<SessionListItem[]>([]);

  useEffect(() => {
    if (!Number.isFinite(botId)) return;
    let cancelled = false;
    const reload = () =>
      api
        .botSessions(botId)
        .then((s) => {
          if (!cancelled) setSessions(s);
        })
        .catch(() => {});
    reload();
    const t = setInterval(reload, 5000);
    return () => {
      cancelled = true;
      clearInterval(t);
    };
  }, [botId]);

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

  const liveSession = sessions.find((s) => s.is_live);

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

      {liveSession && (
        <Link
          href={`/bots/${botId}/${liveSession.slug}`}
          className="block transition-transform hover:-translate-y-0.5"
        >
          <Card className="border-emerald-500/40 bg-emerald-500/5">
            <CardHeader className="flex flex-row items-start justify-between gap-3">
              <div className="min-w-0 space-y-2">
                <div className="flex items-center gap-2">
                  <Badge className="border-transparent bg-emerald-500/15 text-emerald-600 dark:text-emerald-400">
                    LIVE
                  </Badge>
                  <CardTitle>Aktif Market</CardTitle>
                </div>
                <CardDescription className="font-mono break-all">
                  {liveSession.slug}
                </CardDescription>
              </div>
              <ArrowRight className="text-emerald-500 h-5 w-5 shrink-0" />
            </CardHeader>
          </Card>
        </Link>
      )}

      <PnLWidget pnl={pnl ?? null} />

      <SessionsTable botId={botId} sessions={sessions} />

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
