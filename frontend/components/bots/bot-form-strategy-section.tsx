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
  const isAlis = form.strategy === "alis";
  const isElis = form.strategy === "elis";

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
  const openDelta =
    params.open_delta ?? STRATEGY_PARAMS_DEFAULTS.open_delta;
  const pyramidAggDelta =
    params.pyramid_agg_delta ?? STRATEGY_PARAMS_DEFAULTS.pyramid_agg_delta;
  const pyramidFakDelta =
    params.pyramid_fak_delta ?? STRATEGY_PARAMS_DEFAULTS.pyramid_fak_delta;
  const pyramidUsdc = params.pyramid_usdc ?? null;
  const baseShares =
    params.base_shares ?? STRATEGY_PARAMS_DEFAULTS.base_shares;
  const balanceLock =
    params.balance_lock ?? STRATEGY_PARAMS_DEFAULTS.balance_lock;
  const balanceUrgent =
    params.balance_urgent ?? STRATEGY_PARAMS_DEFAULTS.balance_urgent;

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
            tooltip="Tüm stratejiler için canonical profit eşiği: avg_threshold = 1 − pct (örn. 0.02 → 0.98). Alis hedge fiyatını avg_threshold − avg_filled_side olarak türetir. Elis lock testini pair_cost ≤ avg_threshold ile yapar ve aynı eşiği Extreme regime FAK projeksiyonunda kullanır. Düşük tutmak daha hızlı/yakın lock, yüksek tutmak daha karlı (ama daha az dolgun) lock anlamına gelir."
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

      {isAlis && (
        <div className="space-y-3">
          <div>
            <SectionLabel icon={Sliders} title="Alis parametreleri" />
            <p className="text-muted-foreground mt-1 text-sm">
              Opener ve pyramid emir delta&apos;ları; pyramid bütçesi.
            </p>
          </div>

          <div className="bg-muted/25 space-y-4 rounded-md border border-border/40 p-3">
            <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
              <Field
                label="Opener delta"
                tooltip="DeepTrade fazında kurulan açılış GTC emirlerinin fiyat ofseti. Dominant tarafın emri best_ask + open_delta'da, hedge tarafı ise (1 − profit_lock_pct) − opener_price'da kurulur. Skor sadece yön belirler, delta sabittir."
                hint="0.00 – 0.10 (default 0.01)."
              >
                <Input
                  type="number"
                  step="0.005"
                  min="0"
                  max="0.10"
                  value={openDelta}
                  onChange={(e) =>
                    patch({ open_delta: Number(e.target.value) })
                  }
                />
              </Field>
              <Field
                label="Pyramid USDC (boş = order_usdc)"
                tooltip="AggTrade/FakTrade fazlarında atılan pyramid (taker FAK) emir başına düşen notional. Boş bırakılırsa botun ana order_usdc değeri kullanılır."
                hint="Opsiyonel; min 1 USDC."
              >
                <Input
                  type="number"
                  step="1"
                  min="0"
                  placeholder="order_usdc"
                  value={pyramidUsdc ?? ""}
                  onChange={(e) => {
                    const raw = e.target.value.trim();
                    patch({
                      pyramid_usdc: raw === "" ? null : Number(raw),
                    });
                  }}
                />
              </Field>
            </div>

            <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
              <Field
                label="AggTrade pyramid delta"
                tooltip="AggTrade fazında (150–270 sn) trend yönünde atılan taker FAK emirlerinin fiyat ofseti: best_ask + delta. Trend filtresi: composite skor ortalaması > 5 ve dominant tarafın best_bid > 0.5."
                hint="0.00 – 0.10 (default 0.015)."
              >
                <Input
                  type="number"
                  step="0.005"
                  min="0"
                  max="0.10"
                  value={pyramidAggDelta}
                  onChange={(e) =>
                    patch({ pyramid_agg_delta: Number(e.target.value) })
                  }
                />
              </Field>
              <Field
                label="FakTrade pyramid delta"
                tooltip="FakTrade fazında (270–290 sn) atılan taker FAK delta'sı; AggTrade'e göre daha agresif (fill önceliği için)."
                hint="0.00 – 0.20 (default 0.025)."
              >
                <Input
                  type="number"
                  step="0.005"
                  min="0"
                  max="0.20"
                  value={pyramidFakDelta}
                  onChange={(e) =>
                    patch({ pyramid_fak_delta: Number(e.target.value) })
                  }
                />
              </Field>
            </div>
          </div>
        </div>
      )}

      {isElis && (
        <div className="space-y-3">
          <div>
            <SectionLabel icon={Sliders} title="Elis parametreleri" />
            <p className="text-muted-foreground mt-1 text-sm">
              Pair-trading taban share ve dengesizlik eşikleri.{" "}
              <span className="font-mono">order_usdc</span> Elis sizing&apos;inde
              kullanılmaz; emir boyutu doğrudan share sayısıyla hesaplanır.
            </p>
          </div>

          <div className="bg-muted/25 space-y-4 rounded-md border border-border/40 p-3">
            <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
              <Field
                label="Base shares"
                tooltip="Ladder weight'lerinin uygulandığı taban share. Her ladder seviyesi `base_shares × side_w(score) × pct` olarak boyutlandırılır. Tight regime 4 seviye (40/30/20/10), Medium 3 (50/30/20), Wide 2 (70/30); Extreme tek deep maker + opsiyonel FAK. Hedge-urgent'ta eksik tarafa 2× boost uygulanır."
                hint="Min 1 share (default 25)."
              >
                <Input
                  type="number"
                  step="1"
                  min="1"
                  value={baseShares}
                  onChange={(e) =>
                    patch({ base_shares: Number(e.target.value) })
                  }
                />
              </Field>
              <Field
                label="Balance lock eşiği"
                tooltip="Lock için max kabul edilen balance ratio = |up_filled − down_filled| / min(up, down). Bu eşiğin altında ve pair_cost ≤ avg_threshold iken Elis Locked state'ine geçer ve tüm açık `elis:*` emirlerini iptal eder. Düşük tutmak daha simetrik pozisyon ister."
                hint="0.00 – 0.50 (default 0.10 = ≤%10 dengesizlik)."
              >
                <Input
                  type="number"
                  step="0.01"
                  min="0"
                  max="0.50"
                  value={balanceLock}
                  onChange={(e) =>
                    patch({ balance_lock: Number(e.target.value) })
                  }
                />
              </Field>
            </div>

            <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
              <Field
                label="Hedge-urgent eşiği"
                tooltip="Balance ratio bu değeri aştığında Elis hedge-urgent moduna girer: dominant taraftaki tüm `elis:*` emirler cancel'lanır, eksik tarafa 2× weight'li ladder döşenir (Extreme regime'de FAK projeksiyonu lock eşiğinin altında kalıyorsa FAK da eklenir). Yüksek tutmak hedge'i geciktirir, düşük tutmak emir trafiğini artırır."
                hint="0.05 – 1.00 (default 0.30 = >%30 dengesizlik tetikler)."
              >
                <Input
                  type="number"
                  step="0.05"
                  min="0.05"
                  max="1"
                  value={balanceUrgent}
                  onChange={(e) =>
                    patch({ balance_urgent: Number(e.target.value) })
                  }
                />
              </Field>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
