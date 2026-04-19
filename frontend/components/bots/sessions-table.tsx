"use client";

import Link from "next/link";
import { ArrowRight } from "lucide-react";
import { Badge } from "@/components/ui/badge";
import {
  Card,
  CardContent,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { cn } from "@/lib/utils";
import type { SessionListItem } from "@/lib/types";

function fmtTs(ts: number): string {
  return new Date(ts * 1000).toLocaleString();
}

function fmtPnl(p: number | null): string {
  if (p == null) return "—";
  return p.toFixed(4);
}

export function SessionsTable({
  botId,
  sessions,
}: {
  botId: number;
  sessions: SessionListItem[];
}) {
  return (
    <Card>
      <CardHeader>
        <CardTitle>Geçmiş Marketler</CardTitle>
      </CardHeader>
      <CardContent className="p-0">
        {sessions.length === 0 ? (
          <p className="text-muted-foreground p-6 text-sm">
            Henüz session geçmişi yok.
          </p>
        ) : (
          <div className="divide-y">
            {sessions.map((s) => (
              <Link
                key={s.slug}
                href={`/bots/${botId}/${s.slug}`}
                className="hover:bg-muted/40 group flex items-center gap-4 px-4 py-3 transition-colors"
              >
                <div className="min-w-0 flex-1">
                  <div className="flex flex-wrap items-center gap-2">
                    <span className="font-mono text-sm break-all">
                      {s.slug}
                    </span>
                    <StateBadge state={s.state} live={s.is_live} />
                  </div>
                  <p className="text-muted-foreground mt-1 text-xs">
                    {fmtTs(s.start_ts)} → {fmtTs(s.end_ts)}
                  </p>
                </div>
                <div className="hidden flex-col items-end gap-0.5 text-right sm:flex">
                  <span className="text-muted-foreground text-[10px] tracking-wider uppercase">
                    Cost / Pos
                  </span>
                  <span className="font-mono text-xs">
                    ${s.cost_basis.toFixed(2)} · Y{s.shares_yes.toFixed(0)} · N
                    {s.shares_no.toFixed(0)}
                  </span>
                </div>
                <div className="flex flex-col items-end gap-0.5 text-right">
                  <span className="text-muted-foreground text-[10px] tracking-wider uppercase">
                    Realized
                  </span>
                  <span
                    className={cn(
                      "font-mono text-sm",
                      s.realized_pnl != null && s.realized_pnl > 0
                        ? "text-emerald-500"
                        : s.realized_pnl != null && s.realized_pnl < 0
                          ? "text-destructive"
                          : "text-foreground",
                    )}
                  >
                    {fmtPnl(s.realized_pnl)}
                  </span>
                </div>
                <ArrowRight className="text-muted-foreground group-hover:text-foreground h-4 w-4 shrink-0" />
              </Link>
            ))}
          </div>
        )}
      </CardContent>
    </Card>
  );
}

function StateBadge({ state, live }: { state: string; live: boolean }) {
  if (live) {
    return (
      <Badge className="border-transparent bg-emerald-500/15 text-emerald-600 dark:text-emerald-400">
        LIVE
      </Badge>
    );
  }
  return <Badge variant="outline">{state}</Badge>;
}
