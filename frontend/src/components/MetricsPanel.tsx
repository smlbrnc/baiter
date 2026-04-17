import { Badge } from "@/components/ui/badge";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import type { FrontendEvent } from "@/lib/types";

export interface LiveMetrics {
  lastBestBidAsk: Extract<FrontendEvent, { kind: "BestBidAsk" }> | null;
  lastSignal: Extract<FrontendEvent, { kind: "SignalUpdate" }> | null;
  lastZone: Extract<FrontendEvent, { kind: "ZoneChanged" }> | null;
  lastFill: Extract<FrontendEvent, { kind: "Fill" }> | null;
}

export function MetricsPanel({ m }: { m: LiveMetrics }) {
  return (
    <Card>
      <CardHeader>
        <CardTitle>Canlı Metrikler</CardTitle>
      </CardHeader>
      <CardContent className="grid grid-cols-2 gap-4 text-sm sm:grid-cols-4">
        <BestBlock title="YES bid" v={m.lastBestBidAsk?.yes_best_bid} color="green" bold />
        <BestBlock title="YES ask" v={m.lastBestBidAsk?.yes_best_ask} color="green" />
        <BestBlock title="NO bid" v={m.lastBestBidAsk?.no_best_bid} color="red" bold />
        <BestBlock title="NO ask" v={m.lastBestBidAsk?.no_best_ask} color="red" />

        <Pair
          title="Zone"
          primary={m.lastZone?.zone ?? "-"}
          secondary={
            m.lastZone
              ? `${(m.lastZone.zone_pct * 100).toFixed(1)}%`
              : undefined
          }
        />
        <Pair
          title="Signal"
          primary={m.lastSignal ? m.lastSignal.signal_score.toFixed(2) : "-"}
          secondary={
            m.lastSignal
              ? `BSI ${m.lastSignal.bsi.toFixed(2)}`
              : undefined
          }
        />
        <Pair
          title="Last fill"
          primary={
            m.lastFill ? `${m.lastFill.outcome} ${m.lastFill.size}` : "-"
          }
          secondary={m.lastFill ? `@ ${m.lastFill.price}` : undefined}
        />
        <Pair
          title="Status"
          primary={<Badge variant="default">{m.lastFill?.status ?? "-"}</Badge>}
        />
      </CardContent>
    </Card>
  );
}

function BestBlock({
  title,
  v,
  color,
  bold,
}: {
  title: string;
  v: number | undefined;
  color: "green" | "red";
  bold?: boolean;
}) {
  const klass =
    color === "green"
      ? bold
        ? "text-emerald-500 font-bold"
        : "text-emerald-400"
      : bold
        ? "text-red-500 font-bold"
        : "text-red-400";
  return (
    <div className="flex flex-col">
      <span className="text-xs uppercase text-muted-foreground">{title}</span>
      <span className={`font-mono text-base ${klass}`}>
        {v == null ? "-" : v.toFixed(4)}
      </span>
    </div>
  );
}

function Pair({
  title,
  primary,
  secondary,
}: {
  title: string;
  primary: React.ReactNode;
  secondary?: React.ReactNode;
}) {
  return (
    <div className="flex flex-col">
      <span className="text-xs uppercase text-muted-foreground">{title}</span>
      <span className="font-mono text-sm">{primary}</span>
      {secondary && (
        <span className="text-xs text-muted-foreground">{secondary}</span>
      )}
    </div>
  );
}
