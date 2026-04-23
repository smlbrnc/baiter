import type { Dispatch, SetStateAction } from "react";
import { Settings2 } from "lucide-react";
import { Input } from "@/components/ui/input";
import type { CreateBotReq } from "@/lib/types";
import { Field, SectionLabel } from "@/components/bots/bot-form-shared";

type Props = {
  form: CreateBotReq;
  setForm: Dispatch<SetStateAction<CreateBotReq>>;
};

export function BotFormSettingsSection({ form, setForm }: Props) {
  return (
    <div className="space-y-3">
      <div>
        <SectionLabel icon={Settings2} title="Risk ve emir parametreleri" />
        <p className="text-muted-foreground mt-1 text-sm">
          Emir boyutu, cooldown ve fiyat aralığı.
        </p>
      </div>

      <div className="bg-muted/25 space-y-3 rounded-md border border-border/40 p-3">
        <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
          <Field
            label="Order USDC"
            tooltip="Emir başına harcanacak USDC miktarı. GTC size = max(⌈order_usdc / fiyat⌉, api_min_order_size). Artırmak emir büyüklüğünü doğrudan artırır."
            hint="Minimum 1 USDC."
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
          <Field
            label="Cooldown (ms)"
            tooltip="İki averaging GTC emri arasındaki minimum bekleme süresi (milisaniye). Fiyat düştükten sonra bot bu süre dolmadan yeni averaging emri göndermez. Varsayılan: 30 000 ms = 30 sn."
            hint="Varsayılan 30 000 ms."
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
        </div>

        <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
          <Field
            label="Min price"
            tooltip="Emirlerin kabul edildiği minimum fiyat eşiği (0.01–0.50 USDC/share). Bu değerin altındaki fiyatlarda emir gönderilmez; aşırı düşük likiditeye karşı koruma sağlar."
            hint="0.01 – 0.50; emirler bu fiyatın altında olamaz."
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
            tooltip="Emirlerin kabul edildiği maksimum fiyat eşiği (0.50–0.99 USDC/share). Bu değerin üzerindeki fiyatlarda emir gönderilmez; çok pahalı pozisyon almayı önler."
            hint="0.50 – 0.99; emirler bu fiyatın üstünde olamaz."
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
