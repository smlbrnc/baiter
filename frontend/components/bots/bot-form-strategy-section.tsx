import type { Dispatch, SetStateAction } from "react";
import { Sliders, TrendingUp } from "lucide-react";
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
  const isAras = form.strategy === "aras";

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

  // ── Aras defaults ─────────────────────────────────────────────────────
  const arasPollSecs =
    params.aras_poll_secs ?? STRATEGY_PARAMS_DEFAULTS.aras_poll_secs;
  const arasDcaMinDrop =
    params.aras_dca_min_drop ?? STRATEGY_PARAMS_DEFAULTS.aras_dca_min_drop;
  const arasSharesPerOrder =
    params.aras_shares_per_order ??
    STRATEGY_PARAMS_DEFAULTS.aras_shares_per_order;
  const arasMaxUsdPerSide =
    params.aras_max_usd_per_side ??
    STRATEGY_PARAMS_DEFAULTS.aras_max_usd_per_side;
  const arasHedgeStepSecs =
    params.aras_hedge_step_secs ??
    STRATEGY_PARAMS_DEFAULTS.aras_hedge_step_secs;
  const arasBandLow =
    params.aras_band_low ?? STRATEGY_PARAMS_DEFAULTS.aras_band_low;
  const arasBandHigh =
    params.aras_band_high ?? STRATEGY_PARAMS_DEFAULTS.aras_band_high;
  const arasCheapThreshold =
    params.aras_cheap_threshold ??
    STRATEGY_PARAMS_DEFAULTS.aras_cheap_threshold;

  return (
    <div className="space-y-3">
      <div>
        <SectionLabel icon={Sliders} title="Strateji parametreleri" />
        <p className="text-muted-foreground mt-1 text-sm">
          {isElis
            ? "Elis maker bid ile spread arbitrajı. Aşağıdaki RTDS ve ağırlıklar composite skoru besler; Elis bunu yalnızca momentum (ani hareket) filtresi için kullanır. Profit-lock kar kilidi eşiğidir."
            : "RTDS Chainlink sinyali ve strateji ince ayarları."}
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
            tooltip={
              isElis
                ? "Elis: avg_up + avg_down ≤ avg_threshold olduğunda kilit modu; avg_threshold = 1 − pct. Doküman önerisi ~0.975 için pct ≈ 0.025. Alis hedge formülünden farklı olarak Elis VWAP + envanter mantığıyla çalışır."
                : "Hedge hedef fiyatı için kullanılan eşik. avg_threshold = 1 − pct (örn. 0.02 → 0.98); hedge emir fiyatı = avg_threshold − avg_filled_side olarak türetilir. Düşük tutmak hedge'i avg'ye yakın, yüksek tutmak ise daha karlı (ama daha az dolgun) konuma yerleştirir. Default: 0.02."
            }
            hint={
              isElis
                ? "0.00 – 0.50 (default 0.02 → avg_threshold 0.98). Elis önerisi: 0.025 → 0.975 (doküman §8)."
                : "0.00 – 0.50 (default 0.02 → avg_threshold 0.98)."
            }
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

      {isElis && (
        <div className="space-y-2 rounded-md border border-border/40 bg-muted/10 px-3 py-2.5 text-xs leading-relaxed text-muted-foreground">
          <p className="font-medium text-foreground">Elis — kısa özet</p>
          <ul className="list-disc space-y-1 pl-4">
            <li>
              Giriş: UP ve DOWN best bid toplamı 0.985 altında (sabit eşik);
              çift tarafta maker bid.
            </li>
            <li>
              Envanter: dengesizlikte ağır taraf iptal, hafif taraf hedge bid;
              pencere sonu fazında yeni çift taraf yok, yalnızca denge hedge.
            </li>
            <li>
              RTDS kapalıysa composite skor nötre yakın kalır; momentum filtresi
              gevşer — kapalı RTDS ile davranışı göz önünde bulundur.
            </li>
          </ul>
        </div>
      )}

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
                tooltip="AggTrade fazında (225–270 sn) trend yönünde atılan taker FAK emirlerinin fiyat ofseti: best_ask + delta. Trend filtresi: composite skor ortalaması > 5 ve dominant tarafın best_bid > 0.5."
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
                tooltip="FakTrade fazında (270–294 sn) atılan taker FAK delta'sı; AggTrade'e göre daha agresif (fill önceliği için)."
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

      {isAras && (
        <div className="space-y-3">
          <div>
            <SectionLabel icon={TrendingUp} title="Aras parametreleri" />
            <p className="text-muted-foreground mt-1 text-sm">
              DCA + Kademeli Hedge Arbitrajı ayarları. Pahalı taraf (mid &gt;{" "}
              <code>cheap_threshold</code>) her{" "}
              <code>poll_secs</code> saniyede bid−1tick ile alım yapılır; fill
              olunca ucuz tarafa bid−3t→2t→1t kademeli GTC hedge açılır.
            </p>
          </div>

          {/* Zamanlama */}
          <div className="bg-muted/25 space-y-4 rounded-md border border-border/40 p-3">
            <p className="text-muted-foreground text-xs font-semibold uppercase tracking-wider">
              Zamanlama
            </p>
            <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
              <Field
                label="DCA kontrol aralığı (sn)"
                tooltip="Pahalı taraf bid fiyatı her bu sürede kontrol edilir. Koşullar uygunsa bid−1tick fiyatına GTC emir gönderilir. Default: 2.0 sn."
                hint="0.5 – 60 sn (default 2.0)."
              >
                <Input
                  type="number"
                  step="0.5"
                  min="0.5"
                  max="60"
                  value={arasPollSecs}
                  onChange={(e) =>
                    patch({ aras_poll_secs: Number(e.target.value) })
                  }
                />
              </Field>
              <Field
                label="Hedge adım bekleme (sn)"
                tooltip="Bir hedge adımı dolmadan sonraki adıma geçilmez. bid-3tick (step 1) → bu süre bekle → bid-2tick (step 2) → bekle → bid-1tick (step 3). Default: 6.0 sn."
                hint="1 – 60 sn (default 6.0)."
              >
                <Input
                  type="number"
                  step="0.5"
                  min="1"
                  max="60"
                  value={arasHedgeStepSecs}
                  onChange={(e) =>
                    patch({ aras_hedge_step_secs: Number(e.target.value) })
                  }
                />
              </Field>
            </div>
          </div>

          {/* Emir boyutu ve limit */}
          <div className="bg-muted/25 space-y-4 rounded-md border border-border/40 p-3">
            <p className="text-muted-foreground text-xs font-semibold uppercase tracking-wider">
              Emir boyutu &amp; limit
            </p>
            <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
              <Field
                label="Emir başı share"
                tooltip="Her DCA ve hedge emrinin share miktarı. 16 market backtest'te 40 share/emir kullanıldı. Default: 40."
                hint="5 – 1000 share (default 40)."
              >
                <Input
                  type="number"
                  step="5"
                  min="5"
                  max="1000"
                  value={arasSharesPerOrder}
                  onChange={(e) =>
                    patch({ aras_shares_per_order: Number(e.target.value) })
                  }
                />
              </Field>
              <Field
                label="Taraf başı maks USDC"
                tooltip="Tek bir tarafın (UP veya DOWN) toplam maliyet tavanı. Bu eşik aşıldığında o taraf için DCA emri gönderilmez. Default: 500 USDC."
                hint="10 – 10 000 USDC (default 500)."
              >
                <Input
                  type="number"
                  step="50"
                  min="10"
                  max="10000"
                  value={arasMaxUsdPerSide}
                  onChange={(e) =>
                    patch({ aras_max_usd_per_side: Number(e.target.value) })
                  }
                />
              </Field>
            </div>

            <Field
              label="DCA min düşüş (fiyat birimi)"
              tooltip="Yeni bir DCA emri ancak güncel entry fiyatı mevcut ortalamadan en az bu kadar düşükse gönderilir: entry < avg − min_drop. İlk giriş (filled=0) bu kontrolü atlar. Default: 0.01 (1 tick)."
              hint="0.001 – 0.20 (default 0.01)."
            >
              <Input
                type="number"
                step="0.001"
                min="0.001"
                max="0.20"
                value={arasDcaMinDrop}
                onChange={(e) =>
                  patch({ aras_dca_min_drop: Number(e.target.value) })
                }
              />
            </Field>
          </div>

          {/* Bant ve eşik */}
          <div className="bg-muted/25 space-y-4 rounded-md border border-border/40 p-3">
            <p className="text-muted-foreground text-xs font-semibold uppercase tracking-wider">
              İşlem bandı &amp; eşik
            </p>
            <div className="grid grid-cols-1 gap-3 sm:grid-cols-3">
              <Field
                label="Alt bant (band_low)"
                tooltip="Bu fiyatın altındaki ucuz taraf için hedge emri gönderilmez. Settle yakını aşırı kaldıraçtan korur. Default: 0.10."
                hint="0.01 – 0.49 (default 0.10)."
              >
                <Input
                  type="number"
                  step="0.01"
                  min="0.01"
                  max="0.49"
                  value={arasBandLow}
                  onChange={(e) =>
                    patch({ aras_band_low: Number(e.target.value) })
                  }
                />
              </Field>
              <Field
                label="Pahalı/ucuz eşiği"
                tooltip="Bu eşiğin üstündeki taraf 'pahalı' (DCA hedefi), altındaki taraf 'ucuz' (hedge hedefi) olarak sınıflandırılır. Default: 0.50."
                hint="0.10 – 0.90 (default 0.50)."
              >
                <Input
                  type="number"
                  step="0.01"
                  min="0.10"
                  max="0.90"
                  value={arasCheapThreshold}
                  onChange={(e) =>
                    patch({ aras_cheap_threshold: Number(e.target.value) })
                  }
                />
              </Field>
              <Field
                label="Üst bant (band_high)"
                tooltip="DCA emri pahalı taraf bu fiyatın altındayken gönderilir. Settle yakını aşırı alımdan korur. Default: 0.90."
                hint="0.51 – 0.99 (default 0.90)."
              >
                <Input
                  type="number"
                  step="0.01"
                  min="0.51"
                  max="0.99"
                  value={arasBandHigh}
                  onChange={(e) =>
                    patch({ aras_band_high: Number(e.target.value) })
                  }
                />
              </Field>
            </div>
          </div>

          {/* Bilgi özeti */}
          <div className="space-y-2 rounded-md border border-border/40 bg-muted/10 px-3 py-2.5 text-xs leading-relaxed text-muted-foreground">
            <p className="font-medium text-foreground">Aras — nasıl çalışır?</p>
            <ul className="list-disc space-y-1 pl-4">
              <li>
                <strong>DCA:</strong> mid &gt; cheap_threshold ve mid &lt;
                band_high ise pahalı taraf her poll_secs saniyede
                bid−1tick&apos;e GTC emir alır. Ortalama düşmedikçe (min_drop)
                veya maks limit aşılınca yeni emir verilmez.
              </li>
              <li>
                <strong>Hedge:</strong> DCA fill olunca ucuz taraf (mid &lt;
                cheap_threshold) için bid−3tick emri verilir. Her hedge_step_secs
                saniyede fill olmazsa iptal edilip bir sonraki adım (bid−2t →
                bid−1t) denenir.
              </li>
              <li>
                <strong>ARB kilidi:</strong> avg_up + avg_down &lt; 1.00 ise
                pair başına garantili kâr kilitlenir ve log&apos;a yazılır.
              </li>
            </ul>
          </div>
        </div>
      )}
    </div>
  );
}
