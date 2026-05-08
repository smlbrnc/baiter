import type { Dispatch, SetStateAction } from "react"
import { Input } from "@/components/ui/input"
import type { CreateBotReq } from "@/lib/types"
import { cn } from "@/lib/utils"
import { RUN_MODE_OPTIONS } from "@/components/bots/bot-form-constants"
import { Field } from "@/components/bots/bot-form-shared"

type FieldProps = {
  form: CreateBotReq
  setForm: Dispatch<SetStateAction<CreateBotReq>>
}

export function BotFormNameField({ form, setForm }: FieldProps) {
  return (
    <Field
      label="Bot adı"
      tooltip="Bota verilen görünen ad. Zorunlu değildir; boş bırakılırsa varlık + aralık + strateji birleştirilerek otomatik oluşturulur."
    >
      <Input
        value={form.name}
        onChange={(e) => setForm({ ...form, name: e.target.value })}
        placeholder="İsteğe bağlı — örn. BTC 5m Alis"
      />
    </Field>
  )
}

export function BotFormRunModeField({ form, setForm }: FieldProps) {
  return (
    <Field
      label="Çalışma modu"
      tooltip="Live: gerçek CLOB API'ye emir gönderilir, kimlik bilgisi zorunludur. DryRun: emirler gönderilmez, fill anında simüle edilir; piyasa verisi gerçektir."
    >
      <div
        className="flex overflow-hidden rounded-md border border-border/40 bg-muted/70"
        role="radiogroup"
        aria-label="Çalışma modu"
      >
        <div className="flex min-h-9 min-w-0 flex-1 divide-x divide-border/35">
          {RUN_MODE_OPTIONS.map(({ id, label, description }) => {
            const selected = form.run_mode === id
            return (
              <button
                key={id}
                type="button"
                role="radio"
                aria-checked={selected}
                onClick={() => setForm({ ...form, run_mode: id })}
                className={cn(
                  "flex min-h-9 flex-1 flex-col justify-center gap-0.5 px-2 py-2.5 text-left transition-colors sm:px-3 sm:py-2",
                  "rounded-none focus-visible:ring-2 focus-visible:ring-ring/50 focus-visible:ring-offset-2 focus-visible:ring-offset-background focus-visible:outline-none",
                  selected &&
                    id === "live" &&
                    "bg-emerald-500/18 text-emerald-950 shadow-sm ring-1 ring-emerald-500/25 dark:bg-emerald-500/22 dark:text-emerald-50 dark:ring-emerald-400/35",
                  selected &&
                    id === "dryrun" &&
                    "bg-orange-500/18 text-orange-950 shadow-sm ring-1 ring-orange-500/25 dark:bg-orange-500/22 dark:text-orange-50 dark:ring-orange-400/35",
                  !selected &&
                    "text-muted-foreground hover:bg-background/70 hover:text-foreground"
                )}
              >
                <span className="text-sm font-semibold tracking-tight">
                  {label}
                </span>
                <span
                  className={cn(
                    "text-xs leading-snug",
                    !selected && "text-muted-foreground/90",
                    selected &&
                      id === "live" &&
                      "text-emerald-900/80 dark:text-emerald-100/85",
                    selected &&
                      id === "dryrun" &&
                      "text-orange-900/80 dark:text-orange-100/85"
                  )}
                >
                  {description}
                </span>
              </button>
            )
          })}
        </div>
      </div>
    </Field>
  )
}
