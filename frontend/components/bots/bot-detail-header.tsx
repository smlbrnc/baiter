"use client";

import { ArrowLeft } from "lucide-react";
import { useRouter } from "next/navigation";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";

/** Bot özet ve market detay sayfalarında aynı geri düğmesi. */
export function PageBackButton({ className }: { className?: string }) {
  const router = useRouter();
  return (
    <Button
      type="button"
      variant="outline"
      size="icon"
      className={cn("size-9 shrink-0", className)}
      onClick={() => router.back()}
    >
      <ArrowLeft className="size-4" aria-hidden />
    </Button>
  );
}

/** Market sayfası — header altındaki ince çizgi yerine session süresi progress bar. */
export type SessionMarketProgress = {
  pct: number;
  startLabel: string;
  centerLabel: string;
  endLabel: string;
};

type BotDetailHeaderProps = {
  /** Market kartı görseli; bot özet sayfasında genelde yok. */
  imageUrl?: string | null;
  title: string;
  /** Mono alt satır (slug veya slug_pattern). */
  subtitle: React.ReactNode;
  badges?: React.ReactNode;
  actions?: React.ReactNode;
  /** Verilince header altında süre progress barı; üst/alt border çizgileri gösterilmez. */
  marketProgress?: SessionMarketProgress | null;
};

/**
 * Bot `/bots/[id]` ve market `/bots/[id]/[slug]` üst başlığı — aynı tipografi ve layout.
 */
export function BotDetailHeader({
  imageUrl,
  title,
  subtitle,
  badges,
  actions,
  marketProgress,
}: BotDetailHeaderProps) {
  return (
    <header
      className={cn(!marketProgress && "border-border/50 border-b")}
    >
      <div
        className={cn(
          "flex flex-col gap-3 sm:flex-row sm:items-start sm:justify-between sm:gap-4",
          marketProgress ? "pb-2" : "pb-4",
        )}
      >
        <div className="flex min-w-0 flex-1 items-start gap-2.5">
          <PageBackButton />
          {imageUrl ? (
            // eslint-disable-next-line @next/next/no-img-element
            <img
              src={imageUrl}
              alt=""
              className="ring-border/50 size-9 shrink-0 rounded-md object-cover ring-1"
            />
          ) : null}
          <div className="min-w-0 flex-1 space-y-1">
            <h1 className="font-heading text-foreground truncate text-lg font-semibold leading-snug tracking-tight">
              {title}
            </h1>
            <div className="flex flex-col gap-1.5 sm:flex-row sm:flex-wrap sm:items-baseline sm:gap-x-2 sm:gap-y-1">
              <div className="text-muted-foreground min-w-0 font-mono text-[11px] leading-snug break-all">
                {subtitle}
              </div>
              {badges ? (
                <div className="flex shrink-0 flex-wrap items-center gap-1.5">
                  {badges}
                </div>
              ) : null}
            </div>
          </div>
        </div>
        {actions ? (
          <div className="flex shrink-0 flex-wrap items-center gap-2 pt-0.5 sm:pt-0">
            {actions}
          </div>
        ) : null}
      </div>
      {marketProgress ? (
        <div className="space-y-1 px-0.5 pt-2 pb-0.5">
          <div className="bg-muted/50 dark:bg-muted/30 relative h-1 w-full overflow-hidden rounded-full">
            <div
              className="h-full rounded-full bg-orange-500/45 transition-[width] duration-700 ease-out dark:bg-orange-400/40"
              style={{ width: `${marketProgress.pct}%` }}
            />
          </div>
          <div className="text-muted-foreground flex items-center justify-between text-[10px] tabular-nums">
            <span>{marketProgress.startLabel}</span>
            <span className="text-muted-foreground/70">
              {marketProgress.centerLabel}
            </span>
            <span>{marketProgress.endLabel}</span>
          </div>
        </div>
      ) : null}
    </header>
  );
}
