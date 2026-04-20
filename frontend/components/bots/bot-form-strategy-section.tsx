import type { Dispatch, SetStateAction } from "react";
import { Sliders } from "lucide-react";
import { Input } from "@/components/ui/input";
import type { CreateBotReq, StrategyParams } from "@/lib/types";
import { STRATEGY_PARAMS_DEFAULTS } from "@/lib/types";
import { Field, SectionLabel, ToggleRow } from "@/components/bots/bot-form-shared";

type Props = {
  form: CreateBotReq;
  setForm: Dispatch<SetStateAction<CreateBotReq>>;
};

/**
 * Backend `config::StrategyParams` alanlarını expose eder. Tüm değerler
 * opsiyoneldir; kullanıcı dokunmazsa backend `_or_default()` uygular.
 */
export function BotFormStrategyParamsSection({ form, setForm }: Props) {
  const params: StrategyParams = form.strategy_params ?? {};

  const patch = (next: Partial<StrategyParams>) => {
    setForm({
      ...form,
      strategy_params: { ...params, ...next },
    });
  };

  const rtdsEnabled = params.rtds_enabled ?? STRATEGY_PARAMS_DEFAULTS.rtds_enabled;
  const windowWeight =
    params.window_delta_weight ?? STRATEGY_PARAMS_DEFAULTS.window_delta_weight;
  const dualTimeout =
    params.harvest_dual_timeout ?? STRATEGY_PARAMS_DEFAULTS.harvest_dual_timeout;
  const profitLockPct =
    params.harvest_profit_lock_pct ??
    STRATEGY_PARAMS_DEFAULTS.harvest_profit_lock_pct;

  return (
    <div className="space-y-3">
      <div>
        <SectionLabel icon={Sliders} title="Strateji parametreleri" />
        <p className="text-muted-foreground mt-1 text-sm">
          RTDS Chainlink sinyali ve Harvest FSM ince ayarları.
        </p>
      </div>

      <div className="bg-muted/25 space-y-4 rounded-md border border-border/40 p-3">
        <ToggleRow
          checked={rtdsEnabled}
          onChange={(v) => patch({ rtds_enabled: v })}
          title="RTDS Chainlink window delta sinyali"
          description="Polymarket Real-Time Data Socket üzerinden anlık Chainlink fiyatı; pencere açılışından bu yana bps cinsinden fiyat sapmasını skora çevirir."
          tooltip="Açıkken bot, tek bir bağlantı üzerinden Chainlink BTC/ETH/SOL/XRP fiyat akışını dinler ve pencere boyunca biriken yön bilgisini composite skora yansıtır. Kapalıyken window skoru sabit 5.0 (nötr) kalır; composite doğal olarak Binance sinyaline düşer. Default: açık."
        />

        <Field
          label="Window delta ağırlığı"
          tooltip="Composite skoru = w·window_delta_score + (1−w)·binance_score. 0.70 → window_delta dominant; 0.00 → yalnız Binance; 1.00 → yalnız RTDS. RTDS kapalı ya da feed kopuk ise window skoru 5.0 (nötr) döner ve composite Binance ağırlığına kayar."
          hint="0.00 – 1.00 (default 0.70). 0 = yalnız Binance, 1 = yalnız RTDS."
        >
          <Input
            type="number"
            step="0.05"
            min="0"
            max="1"
            value={windowWeight}
            onChange={(e) =>
              patch({ window_delta_weight: Number(e.target.value) })
            }
            disabled={!rtdsEnabled}
          />
        </Field>

        <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
          <Field
            label="OpenDual fill timeout (ms)"
            tooltip="Harvest FSM'in OpenDual aşamasında her iki bacağın dolmasını beklediği maksimum süre. Süre dolunca dolan tarafı tutar (SingleLeg) ya da hiç dolmadıysa iptal edip yeniden dener (Pending). Default: 5 000 ms."
            hint="Default 5 000 ms = 5 sn."
          >
            <Input
              type="number"
              step="500"
              min="500"
              value={dualTimeout}
              onChange={(e) =>
                patch({ harvest_dual_timeout: Number(e.target.value) })
              }
            />
          </Field>
          <Field
            label="ProfitLock eşiği (oran)"
            tooltip="DoubleLeg / SingleLeg ProfitLock tetik oranı. avg_threshold = 1 − pct (örn. 0.02 → 0.98); pozisyonun toplam ortalama maliyeti bu eşiğin altına düşünce FAK ile çıkış emri yollanır. Düşük tutmak çıkışı erken, yüksek tutmak geç tetikler. Default: 0.02."
            hint="0.00 – 0.50 (default 0.02 → avg_threshold 0.98)."
          >
            <Input
              type="number"
              step="0.01"
              min="0"
              max="0.5"
              value={profitLockPct}
              onChange={(e) =>
                patch({ harvest_profit_lock_pct: Number(e.target.value) })
              }
            />
          </Field>
        </div>
      </div>
    </div>
  );
}
