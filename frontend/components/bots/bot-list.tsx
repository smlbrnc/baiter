"use client";

import Link from "next/link";
import Image from "next/image";
import { CircleStop, Play, Trash2 } from "lucide-react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { CardDescription, CardTitle } from "@/components/ui/card";
import { api } from "@/lib/api";
import { assetLogoForSlug } from "@/lib/market";
import type { BotRow } from "@/lib/types";
import { CARD_SHELL_CLASS, HEADER_RADIAL_GRADIENT } from "@/lib/ui-constants";
import { cn } from "@/lib/utils";

/** Compact badge size (matches BotForm header badge scale) */
const listBadge = cn(
  "h-4 min-h-4 rounded-sm px-1.5 py-0 text-[10px] font-normal leading-none",
);

function StateBadge({ state }: { state: string }) {
  switch (state) {
    case "RUNNING":
      return (
        <Badge
          className={cn(
            listBadge,
            "border-transparent bg-emerald-500/15 text-emerald-600 dark:text-emerald-400",
          )}
        >
          {state}
        </Badge>
      );
    case "STOPPED":
      return (
        <Badge variant="secondary" className={listBadge}>
          {state}
        </Badge>
      );
    case "CRASHED":
      return (
        <Badge variant="destructive" className={listBadge}>
          {state}
        </Badge>
      );
    default:
      return (
        <Badge variant="outline" className={listBadge}>
          {state}
        </Badge>
      );
  }
}

function ModeBadge({ mode }: { mode: "live" | "dryrun" }) {
  if (mode === "live") {
    return (
      <Badge
        className={cn(
          listBadge,
          "border-transparent bg-primary/15 text-primary uppercase",
        )}
      >
        live
      </Badge>
    );
  }
  return (
    <Badge
      className={cn(
        listBadge,
        "border-transparent bg-amber-500/15 text-amber-700 uppercase dark:text-amber-400",
      )}
    >
      dryrun
    </Badge>
  );
}

export function BotList({
  bots,
  onChanged,
}: {
  bots: BotRow[];
  onChanged: () => void;
}) {
  if (bots.length === 0) {
    return (
      <div className={CARD_SHELL_CLASS}>
        <div className="from-muted/35 via-background to-background border-b border-border/45 bg-gradient-to-br px-3 py-3 sm:px-4">
          <CardTitle className="font-heading text-sm font-semibold tracking-tight sm:text-base">
            Bots
          </CardTitle>
          <CardDescription className="text-muted-foreground mt-0.5 text-xs sm:text-sm">
            No bots yet. Create one from New bot.
          </CardDescription>
        </div>
      </div>
    );
  }

  const doStart = async (id: number) => {
    try {
      await api.startBot(id);
      onChanged();
    } catch {
      /* yut */
    }
  };

  const doStop = async (id: number) => {
    try {
      await api.stopBot(id);
      onChanged();
    } catch {
      /* yut */
    }
  };

  const doDelete = async (id: number) => {
    if (!confirm(`Bot #${id} silinsin mi?`)) return;
    try {
      await api.deleteBot(id);
      onChanged();
    } catch {
      /* yut */
    }
  };

  return (
    <div className="grid grid-cols-1 gap-3 md:grid-cols-2 xl:grid-cols-3">
      {bots.map((b) => (
        <article
          key={b.id}
          className={cn(CARD_SHELL_CLASS, "@container min-w-0")}
        >
          {/* Header strip — tüm alan detaya gider */}
          <Link
            href={`/bots/${b.id}`}
            className="from-muted/35 via-background to-background group relative block overflow-hidden border-b border-border/45 bg-gradient-to-br px-3 py-2.5 no-underline outline-none transition-colors hover:from-muted/45 hover:via-background sm:px-4 sm:py-3 focus-visible:ring-2 focus-visible:ring-ring/50 focus-visible:ring-offset-0"
          >
            <div
              aria-hidden
              className="pointer-events-none absolute inset-0 z-0 opacity-[0.35]"
              style={{ backgroundImage: HEADER_RADIAL_GRADIENT }}
            />
            <div className="relative z-10 flex gap-3">
              <div
                className="relative size-9 shrink-0 overflow-hidden rounded-md sm:size-10"
                aria-hidden
              >
                <Image
                  src={assetLogoForSlug(b.slug_pattern)}
                  alt=""
                  fill
                  className="object-contain"
                  sizes="(max-width: 640px) 36px, 40px"
                />
              </div>
              <div className="min-w-0 flex-1 space-y-1">
                <span className="font-heading text-foreground group-hover:text-primary block truncate text-sm font-semibold tracking-tight transition-colors sm:text-base">
                  {b.name}
                </span>
                <div className="flex flex-wrap items-center gap-1">
                  <Badge
                    variant="outline"
                    className={cn(listBadge, "uppercase")}
                  >
                    {b.strategy}
                  </Badge>
                  <ModeBadge mode={b.run_mode} />
                  <StateBadge state={b.state} />
                </div>
              </div>
            </div>
          </Link>

          {/* Body — slug centered above settings */}
          <div className="space-y-2 px-3 py-2.5 sm:px-4 sm:py-3">
            <p className="text-muted-foreground text-center font-mono text-[11px] leading-snug break-words sm:text-xs">
              {b.slug_pattern}
            </p>
            <div className="bg-muted/20 space-y-2 rounded-md border border-border/40 p-2.5 shadow-xs sm:p-3">
              <p className="text-muted-foreground text-[10px] font-medium tracking-wide uppercase">
                Settings
              </p>
              <dl className="grid grid-cols-1 gap-2">
                <div className="flex items-baseline justify-between gap-3 sm:block sm:space-y-0">
                  <dt className="text-muted-foreground text-[11px] sm:text-xs">
                    Order (USDC)
                  </dt>
                  <dd className="font-mono text-foreground text-right text-xs font-medium tabular-nums sm:text-left sm:text-sm">
                    ${b.order_usdc.toFixed(2)}
                  </dd>
                </div>
              </dl>
            </div>
          </div>

          {/* Footer — start/stop left, delete right */}
          <div className="border-border/45 flex items-center justify-between gap-2 border-t px-3 py-2 sm:px-4">
            {b.state === "RUNNING" ? (
              <Button
                size="sm"
                variant="secondary"
                title="Stop"
                aria-label="Stop bot"
                className="shadow-xs"
                onClick={() => doStop(b.id)}
              >
                <CircleStop />
                Stop
              </Button>
            ) : (
              <Button
                size="sm"
                title="Start"
                aria-label="Start bot"
                className="shadow-xs"
                onClick={() => doStart(b.id)}
              >
                <Play />
                Start
              </Button>
            )}
            <Button
              size="icon-sm"
              variant="destructive"
              title="Delete"
              aria-label="Delete bot"
              className="shadow-xs"
              onClick={() => doDelete(b.id)}
            >
              <Trash2 />
            </Button>
          </div>
        </article>
      ))}
    </div>
  );
}
