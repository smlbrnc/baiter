import type { Dispatch, SetStateAction } from "react";
import { Settings2 } from "lucide-react";
import { Input } from "@/components/ui/input";
import type { CreateBotReq } from "@/lib/types";
import { cn } from "@/lib/utils";
import { Field, SectionLabel } from "@/components/bots/bot-form-shared";

type Props = {
  form: CreateBotReq;
  setForm: Dispatch<SetStateAction<CreateBotReq>>;
};

export function BotFormSettingsSection({ form, setForm }: Props) {
  const isElis = form.strategy === "elis";
  return (
    <div className="space-y-3">
      <div>
        <SectionLabel icon={Settings2} title="Risk ve emir parametreleri" />
        <p className="text-muted-foreground mt-1 text-sm">
          {isElis
            ? "Emir başına USDC ve fiyat aralığı. Elis cooldown'u strategy_params üzerinden (elis_trade_cooldown_ms) yönetir; bu alandaki cooldown değeri Elis tarafından kullanılmaz."
            : "Emir boyutu, cooldown ve fiyat aralığı."}
        </p>
      </div>

      <div className="bg-muted/25 space-y-3 rounded-md border border-border/40 p-3">
        <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
          <div className={cn(isElis && "sm:col-span-2")}>
            <Field
              label="Order USDC"
              tooltip={
                isElis
                  ? "Elis: her batch'te UP ve DOWN emirleri için temel notional. Balance factor bu degeri arti/eksi ayarlar; gercek emir boyutu = round(order_usdc / fiyat). Min 1 USDC."
                  : "Emir basina harcanacak USDC miktari. GTC size = max(order_usdc / fiyat, api_min_order_size). Artirmak emir buyuklugunu dogrudan arttirir."
              }
              hint={isElis ? "Her outcome basina notional; min 1 USDC." : "Minimum 1 USDC."}
            >
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
          {!isElis && (
            <Field
              label="Cooldown (ms)"
              tooltip="Iki averaging GTC emri arasindaki minimum bekleme suresi (milisaniye). Fiyat dustukten sonra bot bu sure dolmadan yeni averaging emri gondermez. Varsayilan: 30 000 ms = 30 sn."
              hint="Varsayilan 30 000 ms."
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
            tooltip="Emirlerin kabul edildigi minimum fiyat esigi (0.01-0.50 USDC/share). Bu degerin altindaki fiyatlarda emir gonderilmez; asiri dusuk likiditeye karsi koruma saglar."
            hint="0.01 – 0.50; emirler bu fiyatin altinda olamaz."
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
            tooltip="Emirlerin kabul edildigi maksimum fiyat esigi (0.50-0.99 USDC/share). Bu degerin uzerindeki fiyatlarda emir gonderilmez; cok pahali pozisyon almayi onler."
            hint="0.50 – 0.99; emirler bu fiyatin ustunde olamaz."
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
  );
}
