"use client";

import { useCallback, useMemo } from "react";
import { TrendingDown, TrendingUp } from "lucide-react";
import { Badge } from "@/components/ui/badge";
import {
  Card,
  CardContent,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { api } from "@/lib/api";
import { useHistoryStream } from "@/lib/hooks";
import type { TradeRow } from "@/lib/types";
import { SECTION_LABEL_CLASS } from "@/lib/chart-utils";
import { cn } from "@/lib/utils";

type Direction = "UP" | "DOWN";

function normalizeOutcome(o: string | null): Direction | null {
  if (!o) return null;
  const s = o.toUpperCase();
  if (s === "UP" || s === "YES") return "UP";
  if (s === "DOWN" || s === "NO") return "DOWN";
  return null;
}

function fmtTime(ms: number): string {
  return new Date(ms).toLocaleTimeString([], { hour12: false });
}

export function TradesTable({
  botId,
  slug,
  isLive,
}: {
  botId: number;
  slug: string;
  isLive: boolean;
}) {
  const fetchTrades = useCallback(
    (sinceMs: number) => api.sessionTrades(botId, slug, sinceMs),
    [botId, slug],
  );

  const trades = useHistoryStream<TradeRow>({
    fetchInitial: fetchTrades,
    isLive,
    pollMs: 1500,
  });

  const { up, down } = useMemo(() => {
    // `upsert_trade` aynı trade_id için status değişince ts_ms'i de günceller;
    // delta-poll (ts_ms > lastTs) aynı kaydı tekrar getirebilir. Map ile son
    // sürümü tut → React `key={t.trade_id}` çakışması önlenir.
    const latest = new Map<string, TradeRow>();
    for (const t of trades) latest.set(t.trade_id, t);
    const u: TradeRow[] = [];
    const d: TradeRow[] = [];
    for (const t of latest.values()) {
      const dir = normalizeOutcome(t.outcome);
      if (dir === "UP") u.push(t);
      else if (dir === "DOWN") d.push(t);
    }
    u.sort((a, b) => a.ts_ms - b.ts_ms);
    d.sort((a, b) => a.ts_ms - b.ts_ms);
    return { up: u, down: d };
  }, [trades]);

  return (
    <div className="grid grid-cols-1 gap-3 lg:grid-cols-2">
      <SideTable direction="UP" rows={up} />
      <SideTable direction="DOWN" rows={down} />
    </div>
  );
}

function isSell(side: string | null): boolean {
  return (side ?? "").trim().toUpperCase() === "SELL";
}

function SideTable({ direction, rows }: { direction: Direction; rows: TradeRow[] }) {
  // Net notional = Σ buy − Σ sell (SELL = pozisyondan çıkış / cash-in).
  const notional = rows.reduce(
    (acc, r) => acc + (isSell(r.side) ? -1 : 1) * r.size * r.price,
    0,
  );
  const isUp = direction === "UP";
  const Icon = isUp ? TrendingUp : TrendingDown;
  const color = isUp ? "text-emerald-500" : "text-destructive";

  return (
    <Card>
      <CardHeader className="flex flex-row items-center justify-between gap-3">
        <CardTitle
          className={cn(
            SECTION_LABEL_CLASS,
            "flex items-center gap-1.5",
          )}
        >
          <Icon className={cn("size-3.5 shrink-0 opacity-80", color)} aria-hidden />
          {direction} trades
        </CardTitle>
        <div className="text-muted-foreground flex items-center gap-3 text-[10px] tracking-wider uppercase">
          <span>{rows.length} işlem</span>
          <span
            className={cn(
              "font-mono",
              notional < 0
                ? "text-amber-600 dark:text-amber-400"
                : "text-foreground",
            )}
            title={notional < 0 ? "Net cash-in (SELL > BUY)" : "Net notional"}
          >
            {notional < 0 ? "−" : ""}${Math.abs(notional).toFixed(2)}
          </span>
        </div>
      </CardHeader>
      <CardContent className="px-0 pb-0">
        <div className="border-border/40 grid grid-cols-[68px_56px_44px_64px_56px_1fr_72px] gap-3 border-y px-4 py-2 text-[10px] font-medium tracking-wider text-muted-foreground uppercase">
          <span>Zaman</span>
          <span>Yön</span>
          <span>Aksiyon</span>
          <span className="text-right">Fiyat</span>
          <span className="text-right">Adet</span>
          <span>Durum</span>
          <span>Tip</span>
        </div>
        {rows.length === 0 ? (
          <p className="text-muted-foreground px-4 py-6 text-center text-xs">
            —
          </p>
        ) : (
          <div className="divide-border/30 max-h-80 divide-y overflow-y-auto">
            {rows
              .slice()
              .reverse()
              .map((t) => (
                <Row key={t.trade_id} t={t} direction={direction} />
              ))}
          </div>
        )}
      </CardContent>
    </Card>
  );
}

function Row({ t, direction }: { t: TradeRow; direction: Direction }) {
  const sell = isSell(t.side);
  return (
    <div
      className={cn(
        "grid grid-cols-[68px_56px_44px_64px_56px_1fr_72px] items-center gap-3 px-4 py-2 text-xs",
        sell && "bg-amber-500/5",
      )}
    >
      <span className="text-muted-foreground font-mono">{fmtTime(t.ts_ms)}</span>
      <span
        className={cn(
          "font-mono font-semibold",
          direction === "UP" ? "text-emerald-500" : "text-destructive",
        )}
      >
        {direction}
      </span>
      <ActionTag sell={sell} />
      <span className="text-foreground text-right font-mono">
        {t.price.toFixed(4)}
      </span>
      <span className="text-foreground text-right font-mono">
        {t.size.toFixed(2)}
      </span>
      <StatusBadge status={t.status} />
      <TraderTag side={t.trader_side} />
    </div>
  );
}

function ActionTag({ sell }: { sell: boolean }) {
  return (
    <span
      className={cn(
        "rounded-sm px-1.5 py-0.5 text-[10px] font-semibold tracking-wide uppercase",
        sell
          ? "bg-amber-500/20 text-amber-700 dark:text-amber-300"
          : "bg-sky-500/15 text-sky-700 dark:text-sky-300",
      )}
    >
      {sell ? "SAT" : "AL"}
    </span>
  );
}

function StatusBadge({ status }: { status: string }) {
  const s = (status ?? "").toUpperCase();
  const cls = statusClass(s);
  return (
    <Badge className={cn("rounded-sm border-transparent text-[10px]", cls)}>
      {s || "—"}
    </Badge>
  );
}

function statusClass(s: string): string {
  switch (s) {
    case "MATCHED":
    case "MINED":
    case "CONFIRMED":
      return "bg-emerald-500/15 text-emerald-600 dark:text-emerald-400";
    case "RETRYING":
      return "bg-amber-500/15 text-amber-700 dark:text-amber-400";
    case "FAILED":
      return "bg-destructive/15 text-destructive";
    case "CANCELED":
    case "CANCELLED":
      return "bg-muted text-muted-foreground";
    case "LIVE":
      return "bg-sky-500/15 text-sky-600 dark:text-sky-400";
    default:
      return "bg-secondary text-secondary-foreground";
  }
}

function TraderTag({ side }: { side: string | null }) {
  if (!side) return <span className="text-muted-foreground">—</span>;
  const s = side.toLowerCase();
  return (
    <span className="text-muted-foreground rounded-sm bg-muted/60 px-1.5 py-0.5 text-[10px] font-medium tracking-wide uppercase">
      {s === "maker" ? "maker" : s === "taker" ? "taker" : s}
    </span>
  );
}
