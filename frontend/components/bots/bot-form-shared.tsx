import type { ReactNode } from "react"
import type { LucideIcon } from "lucide-react"
import { Info } from "lucide-react"
import { Label } from "@/components/ui/label"
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip"

export function SectionLabel({
  icon: Icon,
  title,
}: {
  icon: LucideIcon
  title: string
}) {
  return (
    <div className="flex items-center gap-2">
      <span className="flex size-8 shrink-0 items-center justify-center rounded-md bg-muted text-muted-foreground">
        <Icon className="size-4" strokeWidth={1.75} />
      </span>
      <h2 className="font-heading text-base font-semibold tracking-tight">
        {title}
      </h2>
    </div>
  )
}

export function ToggleRow({
  checked,
  onChange,
  title,
  description,
  tooltip,
}: {
  checked: boolean
  onChange: (v: boolean) => void
  title: string
  description: string
  tooltip?: string
}) {
  return (
    <label className="flex cursor-pointer gap-3.5 sm:gap-4">
      <input
        type="checkbox"
        checked={checked}
        onChange={(e) => onChange(e.target.checked)}
        className="mt-0.5 size-4 shrink-0 rounded-md border border-input text-primary shadow-xs focus-visible:ring-2 focus-visible:ring-ring/40 focus-visible:outline-none disabled:cursor-not-allowed disabled:opacity-50"
      />
      <span className="min-w-0 space-y-0.5">
        <span className="flex items-center gap-1 text-sm leading-snug font-medium text-foreground">
          {title}
          {tooltip && (
            <Tooltip>
              <TooltipTrigger
                type="button"
                className="inline-flex cursor-help items-center text-muted-foreground/60 transition-colors hover:text-muted-foreground"
              >
                <Info className="size-3.5" />
              </TooltipTrigger>
              <TooltipContent side="top" className="max-w-64 text-xs">
                {tooltip}
              </TooltipContent>
            </Tooltip>
          )}
        </span>
        <span className="block text-xs leading-relaxed text-muted-foreground">
          {description}
        </span>
      </span>
    </label>
  )
}

export function Field({
  label,
  hint,
  tooltip,
  children,
}: {
  label: string
  hint?: string
  tooltip?: string
  children: ReactNode
}) {
  return (
    <div className="grid gap-2">
      <Label className="flex items-center gap-1 text-muted-foreground">
        {label}
        {tooltip && (
          <Tooltip>
            <TooltipTrigger
              type="button"
              className="inline-flex cursor-help items-center text-muted-foreground/60 transition-colors hover:text-muted-foreground"
            >
              <Info className="size-3.5" />
            </TooltipTrigger>
            <TooltipContent side="top" className="max-w-64 text-xs">
              {tooltip}
            </TooltipContent>
          </Tooltip>
        )}
      </Label>
      {children}
      {hint ? (
        <p className="text-[11px] leading-snug text-muted-foreground/90">
          {hint}
        </p>
      ) : null}
    </div>
  )
}
