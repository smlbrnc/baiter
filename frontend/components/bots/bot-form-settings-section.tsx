import type { Dispatch, SetStateAction } from "react";
import { Settings2 } from "lucide-react";
import { Input } from "@/components/ui/input";
import type { CreateBotReq } from "@/lib/types";
import { cn } from "@/lib/utils";
import { RUN_MODE_OPTIONS } from "@/components/bots/bot-form-constants";
import { Field, SectionLabel, ToggleRow } from "@/components/bots/bot-form-shared";

type Props = {
  form: CreateBotReq;
  setForm: Dispatch<SetStateAction<CreateBotReq>>;
};

export function BotFormSettingsSection({ form, setForm }: Props) {
  return (
    <div className="space-y-3">
      <div>
        <SectionLabel icon={Settings2} title="Ek ayarlar" />
        <p className="text-muted-foreground mt-1 text-sm">
          Bot adı, çalışma modu ve risk parametreleri.
        </p>
      </div>

      <div className="bg-muted/25 space-y-3 rounded-md border border-border/40 p-3">
        <Field label="Bot adı">
          <Input
            value={form.name}
            onChange={(e) => setForm({ ...form, name: e.target.value })}
            placeholder="İsteğe bağlı — örn. BTC 5m Prism"
          />
        </Field>
        <Field label="Çalışma modu">
          <div
            className="bg-muted/70 flex overflow-hidden rounded-md border border-border/40"
            role="radiogroup"
            aria-label="Çalışma modu"
          >
            <div className="flex min-h-9 min-w-0 flex-1 divide-x divide-border/35">
              {RUN_MODE_OPTIONS.map(({ id, label, description }) => {
                const selected = form.run_mode === id;
                return (
                  <button
                    key={id}
                    type="button"
                    role="radio"
                    aria-checked={selected}
                    onClick={() => setForm({ ...form, run_mode: id })}
                    className={cn(
                      "flex min-h-9 flex-1 flex-col justify-center gap-0.5 px-2 py-2.5 text-left transition-colors sm:px-3 sm:py-2",
                      "rounded-none focus-visible:ring-ring/50 focus-visible:ring-2 focus-visible:ring-offset-2 focus-visible:ring-offset-background focus-visible:outline-none",
                      selected &&
                        id === "live" &&
                        "bg-emerald-500/18 text-emerald-950 shadow-sm ring-1 ring-emerald-500/25 dark:bg-emerald-500/22 dark:text-emerald-50 dark:ring-emerald-400/35",
                      selected &&
                        id === "dryrun" &&
                        "bg-orange-500/18 text-orange-950 shadow-sm ring-1 ring-orange-500/25 dark:bg-orange-500/22 dark:text-orange-50 dark:ring-orange-400/35",
                      !selected &&
                        "text-muted-foreground hover:bg-background/70 hover:text-foreground",
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
                          "text-orange-900/80 dark:text-orange-100/85",
                      )}
                    >
                      {description}
                    </span>
                  </button>
                );
              })}
            </div>
          </div>
        </Field>
        <div className="grid grid-cols-1 gap-3 sm:grid-cols-3">
          <Field label="Order USDC" hint="Minimum 1 USDC.">
            <Input
              type="number"
              step="0.01"
              min="1"
              value={form.order_usdc}
              onChange={(e) =>
                setForm({ ...form, order_usdc: Number(e.target.value) })
              }
            />
          </Field>
          <Field label="Signal weight" hint="0–10 arası.">
            <Input
              type="number"
              step="0.1"
              min="0"
              max="10"
              value={form.signal_weight}
              onChange={(e) =>
                setForm({
                  ...form,
                  signal_weight: Number(e.target.value),
                })
              }
            />
          </Field>
          <div aria-hidden className="hidden sm:block" />
        </div>
        <ToggleRow
          checked={form.auto_start ?? false}
          onChange={(v) => setForm({ ...form, auto_start: v })}
          title="Oluşturduktan sonra otomatik başlat"
          description="Açıksa bot kaydedilir kaydedilmez supervisor tarafından çalıştırılır."
        />
      </div>
    </div>
  );
}
