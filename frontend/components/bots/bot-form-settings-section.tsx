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
        <Field
          label="Bot adı"
          tooltip="Bota verilen görünen ad. Zorunlu değildir; boş bırakılırsa varlık + aralık + strateji birleştirilerek otomatik oluşturulur."
        >
          <Input
            value={form.name}
            onChange={(e) => setForm({ ...form, name: e.target.value })}
            placeholder="İsteğe bağlı — örn. BTC 5m Prism"
          />
        </Field>

        <Field
          label="Çalışma modu"
          tooltip="Live: gerçek CLOB API'ye emir gönderilir, kimlik bilgisi zorunludur. DryRun: emirler gönderilmez, fill anında simüle edilir; piyasa verisi gerçektir."
        >
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

        {/* Order USDC · Signal weight · Cooldown — 3 kolonlu */}
        <div className="grid grid-cols-1 gap-3 sm:grid-cols-3">
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
            label="Signal weight"
            tooltip="Binance aggTrade sinyalinin emir boyutuna etkisi. 0 = sinyal devre dışı (çarpan daima ×1.0); 10 = tam etki. Yalnızca BTC/ETH/SOL/XRP marketlerinde aktiftir."
            hint="0–10 arası."
          >
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
          <Field
            label="Cooldown (ms)"
            tooltip="İki averaging GTC emri arasındaki minimum bekleme süresi (milisaniye). Fiyat düştükten sonra bot bu süre dolmadan yeni averaging emri göndermez. Varsayılan: 30 000 ms = 30 sn."
            hint="Varsayılan 30 000 ms."
          >
            <Input
              type="number"
              step="500"
              min="500"
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

        {/* Min price · Max price — 2 kolonlu */}
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

        <ToggleRow
          checked={form.auto_start ?? false}
          onChange={(v) => setForm({ ...form, auto_start: v })}
          title="Oluşturduktan sonra otomatik başlat"
          description="Açıksa bot kaydedilir kaydedilmez supervisor tarafından çalıştırılır."
          tooltip="Etkinleştirilirse bot oluşturulur oluşturulmaz otomatik olarak başlatılır. Kapalı bırakılırsa bot kayıt edilir fakat manuel başlatma gerekir."
        />
      </div>
    </div>
  );
}
