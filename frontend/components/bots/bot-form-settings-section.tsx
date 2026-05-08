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
  const isAlis = form.strategy === "alis"
  const isElis = form.strategy === "elis"
  const isBonereaper = form.strategy === "bonereaper"
  const description = isElis
    ? "Fiyat aralığı (min/max price). Elis loop süresi ve emir boyutu aşağıdaki strateji parametrelerinden ayarlanır."
    : isBonereaper
      ? "Order USDC = sabit emir boyutu (size = ceil(order_usdc / price)). Min/Max price emir filtresi (executor reddi)."
      : "Emir boyutu, cooldown ve fiyat aralığı."

  const orderTooltip = isElis
    ? "Elis: api_min_order_size kontrolü için kullanılır. Gerçek emir boyutu strategy_params.elis_max_buy_order_size (share) ile belirlenir."
    : isBonereaper
      ? "Bonereaper sabit USDC: her emir bu kadar USDC notional'da yapılır. Default 10 → real bot medyan $12.32 ile birebir uyumlu. Trade size = ceil(order_usdc / price)."
      : "Emir başına harcanacak USDC miktarı. GTC size = max(order_usdc / fiyat, api_min_order_size)."

  const orderHint = isElis
    ? "api_min_order_size kontrolü için; min 1 USDC."
    : isBonereaper
      ? "Default 10 USDC (sabit, real bot medyan $12 uyumlu)."
      : "Minimum 1 USDC."

  return (
    <div className="space-y-3">
      <div>
        <SectionLabel icon={Settings2} title="Risk ve emir parametreleri" />
        <p className="mt-1 text-sm text-muted-foreground">{description}</p>
      </div>

      <div className="space-y-3 rounded-md border border-border/40 bg-muted/25 p-3">
        <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
          <div className={cn(!isAlis && "sm:col-span-2")}>
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
          {isAlis && (
            <Field
              label="Cooldown (ms)"
              tooltip="Alis: iki averaging GTC emri arasındaki minimum bekleme süresi (ms). Fiyat düştükten sonra bot bu süre dolmadan yeni averaging emri göndermez."
              hint="Varsayılan 30 000 ms (30 sn)."
            >
              <Input
                type="number"
                step="500"
                min="0"
                value={form.cooldown_threshold}
                onChange={(e) =>
                  setForm({
                    ...form,
                    cooldown_threshold: Number(e.target.value),
                  })
                }
              />
            </Field>
          )}
        </div>

        <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
          <Field
            label="Min price"
            tooltip="Executor: emirlerin kabul edildiği minimum fiyat eşiği. Strateji bu değerin altında bir fiyat önerirse otomatik reddedilir. Aşırı düşük likiditeye karşı koruma."
            hint="0.01 – 0.50; default 0.05."
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
            tooltip="Executor: emirlerin kabul edildiği maksimum fiyat eşiği. Bonereaper için 0.99 önerilir (real bot 0.99'a kadar trade yapıyor)."
            hint={
              isBonereaper
                ? "0.50 – 0.99; default 0.99 (real bot uyumu)."
                : "0.50 – 0.99; default 0.95."
            }
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
