import type { Dispatch, SetStateAction } from "react"
import Image from "next/image"
import { Clock, Layers, Workflow } from "lucide-react"
import { Button } from "@/components/ui/button"
import { Separator } from "@/components/ui/separator"
import type { MarketAsset, MarketInterval } from "@/lib/market"
import { ASSETS, INTERVALS, previewSlug, slugPattern } from "@/lib/market"
import { mergeBonereaperStrategyDefaults, type CreateBotReq } from "@/lib/types"
import { cn } from "@/lib/utils"
import { STRATEGY_OPTIONS } from "@/components/bots/bot-form-constants"
import {
  BotFormNameField,
  BotFormRunModeField,
} from "@/components/bots/bot-form-fields"
import { SectionLabel } from "@/components/bots/bot-form-shared"

type Props = {
  asset: MarketAsset
  interval: MarketInterval
  form: CreateBotReq
  setForm: Dispatch<SetStateAction<CreateBotReq>>
  pickAsset: (a: MarketAsset) => void
  pickInterval: (i: MarketInterval) => void
}

export function BotFormMarketSection({
  asset,
  interval,
  form,
  setForm,
  pickAsset,
  pickInterval,
}: Props) {
  const startOffset = form.start_offset ?? 0
  const setStartOffset = (offset: 0 | 1) =>
    setForm((f) => ({ ...f, start_offset: offset }))
  const resolvedSlug = previewSlug(asset, interval, startOffset)
  const slugStored = slugPattern(asset, interval)
  return (
    <div className="space-y-5">
      <BotFormNameField form={form} setForm={setForm} />

      <div>
        <SectionLabel icon={Layers} title="Market" />
        <p className="mt-1 text-sm text-muted-foreground">
          Varlık ve pencere süresi — slug otomatik üretilir.
        </p>
      </div>

      <div className="space-y-0 rounded-md border border-border/40 bg-muted/20 p-4 shadow-xs">
        <div
          className="flex flex-wrap justify-center gap-2 sm:justify-start"
          role="group"
          aria-label="Varlık"
        >
          {ASSETS.map(({ id, label, logo }) => (
            <Button
              key={id}
              type="button"
              variant={asset === id ? "default" : "outline"}
              size="sm"
              title={label}
              aria-label={label}
              aria-pressed={asset === id}
              className={cn(
                "relative size-12 shrink-0 overflow-hidden rounded-md p-0 font-normal",
                asset === id
                  ? "shadow-sm"
                  : "border-border/45 bg-background/80 hover:bg-background"
              )}
              onClick={() => pickAsset(id)}
            >
              <Image
                src={logo}
                alt=""
                fill
                className="object-contain"
                sizes="48px"
              />
            </Button>
          ))}
        </div>

        <Separator className="my-4 bg-border/40" />

        <div>
          <p className="sr-only text-muted-foreground">Pencere süresi</p>
          <div
            className="flex overflow-hidden rounded-md border border-border/40 bg-muted/70"
            role="group"
            aria-label="Pencere süresi"
          >
            <span
              className="flex size-9 shrink-0 items-center justify-center border-r border-border/35 bg-muted/90 p-0 text-muted-foreground"
              aria-hidden
            >
              <Clock className="size-3.5" strokeWidth={2} />
            </span>
            <div className="flex min-h-9 min-w-0 flex-1 divide-x divide-border/35">
              {INTERVALS.map(({ id, label }) => (
                <button
                  key={id}
                  type="button"
                  onClick={() => pickInterval(id)}
                  aria-pressed={interval === id}
                  className={cn(
                    "min-h-9 flex-1 rounded-none px-2 py-2 text-sm font-medium text-foreground transition-colors",
                    "focus-visible:ring-2 focus-visible:ring-ring/50 focus-visible:ring-offset-2 focus-visible:ring-offset-background focus-visible:outline-none",
                    interval === id
                      ? "bg-background shadow-sm"
                      : "text-muted-foreground hover:bg-background/70 hover:text-foreground"
                  )}
                >
                  {label}
                </button>
              ))}
            </div>
          </div>
        </div>

        <div className="mt-4">
          <p className="sr-only text-muted-foreground">Başlangıç penceresi</p>
          <div
            className="flex overflow-hidden rounded-md border border-border/40 bg-muted/70"
            role="radiogroup"
            aria-label="Başlangıç penceresi"
          >
            {[
              {
                id: 0 as const,
                label: "Aktif Market",
                hint: "Şu an açık pencere",
              },
              {
                id: 1 as const,
                label: "Sonraki Market",
                hint: "Bir sonraki pencere",
              },
            ].map(({ id, label, hint }) => {
              const selected = startOffset === id
              return (
                <button
                  key={id}
                  type="button"
                  role="radio"
                  aria-checked={selected}
                  onClick={() => setStartOffset(id)}
                  title={hint}
                  className={cn(
                    "flex min-h-9 flex-1 flex-col items-start justify-center gap-0 rounded-none border-r border-border/35 px-3 py-1.5 text-sm font-medium transition-colors last:border-r-0",
                    "focus-visible:ring-2 focus-visible:ring-ring/50 focus-visible:ring-offset-2 focus-visible:ring-offset-background focus-visible:outline-none",
                    selected
                      ? "bg-background text-foreground shadow-sm"
                      : "text-muted-foreground hover:bg-background/70 hover:text-foreground"
                  )}
                >
                  <span className="text-sm leading-tight">{label}</span>
                  <span className="text-[11px] leading-tight text-muted-foreground/80">
                    {hint}
                  </span>
                </button>
              )
            })}
          </div>
        </div>

        <div className="mt-4 rounded-md border border-border/40 bg-background/70 px-3 py-2.5">
          <div className="flex flex-col gap-1.5 sm:flex-row sm:items-baseline sm:gap-3">
            <span className="shrink-0 text-[10px] font-medium tracking-wide text-muted-foreground uppercase">
              Slug önizleme
            </span>
            <code
              className="min-w-0 flex-1 font-mono text-sm leading-snug break-all text-foreground"
              title={`Stored: ${slugStored}`}
            >
              {resolvedSlug}
            </code>
          </div>
        </div>
      </div>

      <div>
        <SectionLabel icon={Workflow} title="Strategy" />
        <p className="mt-1 text-sm text-muted-foreground">
          Botun emir ve fiyat mantığını belirleyen strateji.
        </p>
        <div
          className="mt-3 flex flex-col overflow-hidden rounded-md border border-border/40 bg-muted/70 sm:flex-row"
          role="radiogroup"
          aria-label="Strateji"
        >
          <div className="flex min-h-0 min-w-0 flex-1 flex-col divide-y divide-border/35 sm:flex-row sm:divide-x sm:divide-y-0">
            {STRATEGY_OPTIONS.map(({ id, label, description, disabled }) => {
              const selected = form.strategy === id
              return (
                <button
                  key={id}
                  type="button"
                  role="radio"
                  aria-checked={selected}
                  aria-disabled={disabled}
                  disabled={disabled}
                  title={
                    disabled
                      ? "Bu strateji henüz desteklenmiyor (backend reddeder)."
                      : undefined
                  }
                  onClick={() => {
                    if (disabled) return
                    setForm((f) => {
                      const next = { ...f, strategy: id }
                      if (id === "bonereaper") {
                        const p = f.strategy_params ?? {}
                        next.strategy_params = mergeBonereaperStrategyDefaults(p)
                      }
                      return next
                    })
                  }}
                  className={cn(
                    "flex min-h-9 flex-1 flex-col justify-center gap-0.5 px-2 py-2.5 text-left transition-colors sm:px-3 sm:py-2",
                    "rounded-none focus-visible:ring-2 focus-visible:ring-ring/50 focus-visible:ring-offset-2 focus-visible:ring-offset-background focus-visible:outline-none",
                    disabled
                      ? "cursor-not-allowed opacity-50"
                      : selected
                        ? "bg-background text-foreground shadow-sm"
                        : "text-muted-foreground hover:bg-background/70 hover:text-foreground"
                  )}
                >
                  <span className="text-sm font-semibold tracking-tight">
                    {label}
                  </span>
                  <span
                    className={cn(
                      "text-xs leading-snug",
                      selected
                        ? "text-muted-foreground"
                        : "text-muted-foreground/90"
                    )}
                  >
                    {description}
                  </span>
                </button>
              )
            })}
          </div>
        </div>
      </div>

      <BotFormRunModeField form={form} setForm={setForm} />
    </div>
  )
}
