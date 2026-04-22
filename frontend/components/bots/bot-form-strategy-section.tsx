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
  const profitLockPct =
    params.profit_lock_pct ??
    STRATEGY_PARAMS_DEFAULTS.profit_lock_pct;
  const lookaheadSecs =
    params.signal_lookahead_secs ??
    STRATEGY_PARAMS_DEFAULTS.signal_lookahead_secs;

  return (
    <div className="space-y-3">
      <div>
        <SectionLabel icon={Sliders} title="Strateji parametreleri" />
        <p className="text-muted-foreground mt-1 text-sm">
          RTDS Chainlink sinyali ve strateji ince ayarları.
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

        <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
          <Field
            label="Window delta ağırlığı"
            tooltip="Composite skoru = w·window_delta_score + (1−w)·binance_score. 0.70 → window_delta dominant; 0.00 → yalnız Binance; 1.00 → yalnız RTDS. RTDS kapalı ya da feed kopuk ise window skoru 5.0 (nötr) döner ve composite Binance ağırlığına kayar."
            hint="0.00 – 1.00 (default 0.70)."
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
          <Field
            label="Sinyal ileri-bakış (sn)"
            tooltip="RTDS son 5 sn'lik fiyat hızını (bps/sn) bu süreyle çarpıp window_delta'ya ekler → sinyal projeksiyonu. 3 sn → 'şu anki trend 3 sn sonra ne olur' tahmini. 0 → projeksiyon kapalı (eski davranış); kümülatif window_delta tek başına kullanılır. Yüksek değer (>5) gürültüye duyarlılık artırır."
            hint="0 – 30 sn (default 3.0). RTDS kapalı ise etkisiz."
          >
            <Input
              type="number"
              step="0.5"
              min="0"
              max="30"
              value={lookaheadSecs}
              onChange={(e) =>
                patch({ signal_lookahead_secs: Number(e.target.value) })
              }
              disabled={!rtdsEnabled}
            />
          </Field>
        </div>

        <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
          <Field
            label="Profit-lock oranı"
            tooltip="Hedge hedef fiyatı için kullanılan eşik. avg_threshold = 1 − pct (örn. 0.02 → 0.98); hedge emir fiyatı = avg_threshold − avg_filled_side olarak türetilir. Düşük tutmak hedge'i avg'ye yakın, yüksek tutmak ise daha karlı (ama daha az dolgun) konuma yerleştirir. Default: 0.02."
            hint="0.00 – 0.50 (default 0.02 → avg_threshold 0.98)."
          >
            <Input
              type="number"
              step="0.01"
              min="0"
              max="0.5"
              value={profitLockPct}
              onChange={(e) =>
                patch({ profit_lock_pct: Number(e.target.value) })
              }
            />
          </Field>
        </div>
      </div>
    </div>
  );
}
