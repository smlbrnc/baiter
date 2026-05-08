import Image from "next/image"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { HEADER_RADIAL_GRADIENT } from "@/lib/ui-constants"
import { cn } from "@/lib/utils"

type Props = {
  marketPicked: boolean
  heroLogoSrc: string
  assetLabel: string
  intervalLabel: string
  submitting: boolean
}

export function BotFormHeader({
  marketPicked,
  heroLogoSrc,
  assetLabel,
  intervalLabel,
  submitting,
}: Props) {
  return (
    <div className="relative overflow-hidden border-b border-border/45 bg-gradient-to-br from-muted/35 via-background to-background px-3 py-2.5 sm:px-4 sm:py-3">
      <div
        aria-hidden
        className="pointer-events-none absolute inset-0 z-0 opacity-[0.35]"
        style={{ backgroundImage: HEADER_RADIAL_GRADIENT }}
      />
      <div className="relative z-10 flex items-center gap-2.5 sm:gap-3">
        <div
          className={cn(
            "relative flex size-12 shrink-0 overflow-hidden rounded-md p-0 font-normal",
            marketPicked
              ? "bg-primary text-primary-foreground shadow-sm"
              : "bg-background/80 shadow-xs"
          )}
          aria-hidden
        >
          <Image
            src={heroLogoSrc}
            alt=""
            fill
            className="object-contain"
            sizes="48px"
          />
        </div>
        <div className="min-w-0 flex-1 space-y-1 pr-2 sm:pr-3">
          <div className="flex flex-wrap items-center gap-x-2 gap-y-1">
            <h1 className="font-heading text-lg font-semibold tracking-tight text-foreground sm:text-xl">
              {marketPicked ? (
                <>
                  <span className="sr-only">Yeni bot — </span>
                  <span className="text-foreground">{assetLabel}</span>
                  <span className="font-medium text-muted-foreground"> · </span>
                  <span className="text-foreground">{intervalLabel}</span>
                </>
              ) : (
                "Yeni bot"
              )}
            </h1>
            <Badge
              variant="secondary"
              className="px-1.5 py-0 text-[10px] leading-none font-normal"
            >
              Gamma + CLOB
            </Badge>
          </div>
          <p className="max-w-2xl text-xs leading-snug text-muted-foreground sm:text-[13px]">
            <span className="line-clamp-2 sm:line-clamp-1">
              Solda market ve strateji, sağda mod ve risk; kimlik altta. DryRun
              önce, live için kimlik doldur.
            </span>
          </p>
        </div>
        <Button
          type="submit"
          disabled={submitting}
          size="default"
          className="shrink-0 shadow-xs"
        >
          {submitting ? "Kaydediliyor…" : "Bot oluştur"}
        </Button>
      </div>
    </div>
  )
}
