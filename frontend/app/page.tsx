"use client";

import { useMemo } from "react";
import Link from "next/link";
import { Plus } from "lucide-react";
import { BotList } from "@/components/bots/bot-list";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { useBots } from "@/lib/hooks";
import type { BotRow } from "@/lib/types";
import { CARD_SHELL_CLASS, HEADER_RADIAL_GRADIENT } from "@/lib/ui-constants";

function dashboardStatsLine(bots: BotRow[]): string {
  const n = bots.length;
  if (n === 0) return "No bots yet.";
  let running = 0;
  let stopped = 0;
  let crashed = 0;
  for (const b of bots) {
    if (b.state === "RUNNING") running++;
    else if (b.state === "STOPPED") stopped++;
    else if (b.state === "CRASHED") crashed++;
  }
  const other = n - running - stopped - crashed;
  const parts = [
    `${n} ${n === 1 ? "bot" : "bots"}`,
    `${running} running`,
    `${stopped} stopped`,
  ];
  if (crashed > 0) parts.push(`${crashed} crashed`);
  if (other > 0) parts.push(`${other} other`);
  return parts.join(" · ");
}

export default function DashboardPage() {
  const { bots, reload } = useBots();
  const statsLine = useMemo(() => dashboardStatsLine(bots), [bots]);
  return (
    <div className="space-y-5">
      <header className={CARD_SHELL_CLASS}>
        <div className="from-muted/35 via-background to-background relative overflow-hidden bg-gradient-to-br px-4 py-4 sm:px-5 sm:py-4">
          <div
            aria-hidden
            className="pointer-events-none absolute inset-0 z-0 opacity-[0.35]"
            style={{ backgroundImage: HEADER_RADIAL_GRADIENT }}
          />
          <div className="relative z-10 flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between sm:gap-4">
            <div className="min-w-0 space-y-1">
              <div className="flex flex-wrap items-center gap-x-2 gap-y-1">
                <h1 className="font-heading text-foreground text-xl font-semibold tracking-tight sm:text-2xl">
                  Dashboard
                </h1>
                <Badge
                  variant="secondary"
                  className="px-1.5 py-0 text-[10px] font-normal leading-none"
                >
                  Gamma + CLOB
                </Badge>
              </div>
              <p className="text-muted-foreground max-w-xl text-sm leading-snug sm:text-[13px]">
                {statsLine}
              </p>
            </div>
            <Button
              asChild
              size="default"
              className="shadow-xs w-full shrink-0 sm:w-auto"
            >
              <Link href="/bots/new">
                <Plus />
                New bot
              </Link>
            </Button>
          </div>
        </div>
      </header>
      <BotList bots={bots} onChanged={reload} />
    </div>
  );
}
