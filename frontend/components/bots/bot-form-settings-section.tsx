import type { Dispatch, SetStateAction } from "react"
import { Settings2 } from "lucide-react"
import { Input } from "@/components/ui/input"
import type { CreateBotReq } from "@/lib/types"
import { cn } from "@/lib/utils"
import { Field, SectionLabel } from "@/components/bots/bot-form-shared"

type Props = {
  form: CreateBotReq
  setForm: Dispatch<SetStateAction<CreateBotReq>>
}

export function BotFormSettingsSection({ form, setForm }: Props) {
  const isBonereaper = form.strategy === "bonereaper"
  const description = isBonereaper
    ? "Order USDC bonereaper'da api_min_order_size kontrolü için kullanılır. Trade size'ları aşağıdaki bonereaper parametrelerinden (long-shot/mid/high USDC) gelir. Min/Max price executor filtresi."
    : "Emir boyutu, cooldown ve fiyat aralığı."

  const orderTooltip = isBonereaper
    ? "Bonereaper trade size'ları stratejide ayrı (long-shot/mid/high USDC). Order USDC sadece api_min_order_size eşiği için. LIVE_safe başlangıç: 5 USDC."
    : "Emir başına harcanacak USDC miktarı. GTC size = max(order_usdc / fiyat, api_min_order_size)."

  const orderHint = isBonereaper
    ? "LIVE_safe default 5 USDC (advanced trade size'ları aşağıdan)."
    : "Minimum 1 USDC."

  return (
    <div className="space-y-3">
      <div>
        <SectionLabel icon={Settings2} title="Risk ve emir parametreleri" />
        <p className="mt-1 text-sm text-muted-foreground">{description}</p>
      </div>

      <div className="space-y-3 rounded-md border border-border/40 bg-muted/25 p-3">
        <div>
          <Field label="Order USDC" tooltip={orderTooltip} hint={orderHint}>
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
        </div>

        <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
          <Field
            label="Min price"
            tooltip="Executor: emirlerin kabul edildiği minimum fiyat eşiği. Bonereaper için 0.01 önerilir — loser scalp 0.01-0.05 bandını kapsar."
            hint={
              isBonereaper
                ? "0.01 – 0.50; Bonereaper default 0.01."
                : "0.01 – 0.50; default 0.01."
            }
          >
            <Input
              type="number"
              step="0.01"
              min="0.01"
              max="0.50"
              value={form.min_price}
              onChange={(e) =>
                setForm({ ...form, min_price: Number(e.target.value) })
              }
            />
          </Field>
          <Field
            label="Max price"
            tooltip="Executor: emirlerin kabul edildiği maksimum fiyat eşiği. Bonereaper için 0.99 önerilir — 0.95 LW alımlarının %31'ini bloklar (gerçek bot 0.96-0.99 arasında LW yapıyor)."
            hint="0.50 – 0.99; Bonereaper default 0.99."
          >
            <Input
              type="number"
              step="0.01"
              min="0.50"
              max="0.99"
              value={form.max_price}
              onChange={(e) =>
                setForm({ ...form, max_price: Number(e.target.value) })
              }
            />
          </Field>
        </div>
      </div>
    </div>
  )
}
