import type { ReactNode } from "react";
import type { LucideIcon } from "lucide-react";
import { Info } from "lucide-react";
import { Label } from "@/components/ui/label";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip";

export function SectionLabel({
  icon: Icon,
  title,
}: {
  icon: LucideIcon;
  title: string;
}) {
  return (
    <div className="flex items-center gap-2">
      <span className="bg-muted text-muted-foreground flex size-8 shrink-0 items-center justify-center rounded-md">
        <Icon className="size-4" strokeWidth={1.75} />
      </span>
      <h2 className="font-heading text-base font-semibold tracking-tight">
        {title}
      </h2>
    </div>
  );
}

export function ToggleRow({
  checked,
  onChange,
  title,
  description,
  tooltip,
}: {
  checked: boolean;
  onChange: (v: boolean) => void;
  title: string;
  description: string;
  tooltip?: string;
}) {
  return (
    <label className="flex cursor-pointer gap-3.5 sm:gap-4">
      <input
        type="checkbox"
        checked={checked}
        onChange={(e) => onChange(e.target.checked)}
        className="border-input text-primary focus-visible:ring-ring/40 mt-0.5 size-4 shrink-0 rounded-md border shadow-xs focus-visible:ring-2 focus-visible:outline-none disabled:cursor-not-allowed disabled:opacity-50"
      />
      <span className="min-w-0 space-y-0.5">
        <span className="text-foreground flex items-center gap-1 text-sm font-medium leading-snug">
          {title}
          {tooltip && (
            <Tooltip>
              <TooltipTrigger
                type="button"
                className="text-muted-foreground/60 hover:text-muted-foreground inline-flex cursor-help items-center transition-colors"
              >
                <Info className="size-3.5" />
              </TooltipTrigger>
              <TooltipContent side="top" className="max-w-64 text-xs">
                {tooltip}
              </TooltipContent>
            </Tooltip>
          )}
        </span>
        <span className="text-muted-foreground block text-xs leading-relaxed">
          {description}
        </span>
      </span>
    </label>
  );
}

export function Field({
  label,
  hint,
  tooltip,
  children,
}: {
  label: string;
  hint?: string;
  tooltip?: string;
  children: ReactNode;
}) {
  return (
    <div className="grid gap-2">
      <Label className="text-muted-foreground flex items-center gap-1">
        {label}
        {tooltip && (
          <Tooltip>
            <TooltipTrigger
              type="button"
              className="text-muted-foreground/60 hover:text-muted-foreground inline-flex cursor-help items-center transition-colors"
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
        <p className="text-muted-foreground/90 text-[11px] leading-snug">
          {hint}
        </p>
      ) : null}
    </div>
  );
}
