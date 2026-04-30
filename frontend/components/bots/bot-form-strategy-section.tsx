import type { Dispatch, SetStateAction } from "react";
import { Sliders, TrendingUp, Zap } from "lucide-react";
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

  // ── Alis / ortak sinyal ──────────────────────────────────────────────
  const rtdsEnabled = params.rtds_enabled ?? STRATEGY_PARAMS_DEFAULTS.rtds_enabled;
  const windowWeight =
    params.window_delta_weight ?? STRATEGY_PARAMS_DEFAULTS.window_delta_weight;
  const profitLockPct =
    params.profit_lock_pct ?? STRATEGY_PARAMS_DEFAULTS.profit_lock_pct;
  const lookaheadSecs =
    params.signal_lookahead_secs ?? STRATEGY_PARAMS_DEFAULTS.signal_lookahead_secs;
  const openDelta = params.open_delta ?? STRATEGY_PARAMS_DEFAULTS.open_delta;
  const pyramidAggDelta =
    params.pyramid_agg_delta ?? STRATEGY_PARAMS_DEFAULTS.pyramid_agg_delta;
  const pyramidFakDelta =
    params.pyramid_fak_delta ?? STRATEGY_PARAMS_DEFAULTS.pyramid_fak_delta;
  const pyramidUsdc = params.pyramid_usdc ?? null;

  // ── Elis Dutch Book ───────────────────────────────────────────────────
  const elisSpreadThreshold =
    params.elis_spread_threshold ?? STRATEGY_PARAMS_DEFAULTS.elis_spread_threshold;
  const elisMaxBuyOrderSize =
    params.elis_max_buy_order_size ?? STRATEGY_PARAMS_DEFAULTS.elis_max_buy_order_size;
  const elisTradeCooldownMs =
    params.elis_trade_cooldown_ms ?? STRATEGY_PARAMS_DEFAULTS.elis_trade_cooldown_ms;
  const elisBalanceFactor =
    params.elis_balance_factor ?? STRATEGY_PARAMS_DEFAULTS.elis_balance_factor;
  const elisStopBeforeEndSecs =
    params.elis_stop_before_end_secs ?? STRATEGY_PARAMS_DEFAULTS.elis_stop_before_end_secs;

  // ── Aras defaults ─────────────────────────────────────────────────────
  const arasPollSecs =
    params.aras_poll_secs ?? STRATEGY_PARAMS_DEFAULTS.aras_poll_secs;
  const arasSharesPerOrder =
    params.aras_shares_per_order ?? STRATEGY_PARAMS_DEFAULTS.aras_shares_per_order;
  const arasMaxUsdPerSide =
    params.aras_max_usd_per_side ?? STRATEGY_PARAMS_DEFAULTS.aras_max_usd_per_side;
  const arasBandLow =
    params.aras_band_low ?? STRATEGY_PARAMS_DEFAULTS.aras_band_low;
  const arasBandHigh =
    params.aras_band_high ?? STRATEGY_PARAMS_DEFAULTS.aras_band_high;
  const arasRisingSharesMult =
    params.aras_rising_shares_mult ?? STRATEGY_PARAMS_DEFAULTS.aras_rising_shares_mult;

  return (
    <div className="space-y-3">

      {/* ── Elis Dutch Book parametreleri ─────────────────────────────── */}
      {isElis && (
        <div className="space-y-3">
          <div>
            <SectionLabel icon={Zap} title="Elis — Dutch Book parametreleri" />
            <p className="text-muted-foreground mt-1 text-sm">
              Her iki tarafın bid-ask spread'i geniş olduğunda UP+DOWN çift
              taraflı maker bid emri verilir. Balance factor envanter dengesini
              korur; cooldown ardışık batch'ler arasındaki beklemeyi belirler.
            </p>
          </div>

          <div className="bg-muted/25 space-y-4 rounded-md border border-border/40 p-3">
            <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
              <Field
                label="Spread eşiği"
                tooltip="Her tick'te UP_spread = UP_ask − UP_bid ve DOWN_spread = DOWN_ask − DOWN_bid hesaplanır. Her iki değer bu eşiğe ulaştığında batch emri tetiklenir. Geniş spread = yeterli likidite sinyali."
                hint={`0.01 – 0.20 (default ${STRATEGY_PARAMS_DEFAULTS.elis_spread_threshold}).`}
              >
                <Input
                  type="number"
                  step="0.005"
                  min="0.01"
                  max="0.20"
                  value={elisSpreadThreshold}
                  onChange={(e) =>
                    patch({ elis_spread_threshold: Number(e.target.value) })
                  }
                />
              </Field>
              <Field
                label="Maks emir boyutu (share)"
                tooltip="Dengeli pozisyonda UP ve DOWN taraflarına verilecek maksimum share miktarı. Balance factor bu tavan üzerinden artı/eksi uygular. Artırmak sermayeyi büyütür, azaltmak riski sınırlar."
                hint={`1 – 1000 share (default ${STRATEGY_PARAMS_DEFAULTS.elis_max_buy_order_size}).`}
              >
                <Input
                  type="number"
                  step="5"
                  min="1"
                  max="1000"
                  value={elisMaxBuyOrderSize}
                  onChange={(e) =>
                    patch({ elis_max_buy_order_size: Number(e.target.value) })
                  }
                />
              </Field>
            </div>

            <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
              <Field
                label="Trade cooldown (ms)"
                tooltip="Bir batch yerleştirildikten sonra bu süre dolmadan yeni UP+DOWN çifti verilmez. Cooldown dolduğunda açık emirler iptal edilir ve Idle'a dönülür. Artırmak batch sıklığını düşürür."
                hint={`1 000 – 30 000 ms (default ${STRATEGY_PARAMS_DEFAULTS.elis_trade_cooldown_ms}).`}
              >
                <Input
                  type="number"
                  step="500"
                  min="1000"
                  max="30000"
                  value={elisTradeCooldownMs}
                  onChange={(e) =>
                    patch({ elis_trade_cooldown_ms: Number(e.target.value) })
                  }
                />
              </Field>
              <Field
                label="Balance factor"
                tooltip="Envanter dengesizliğine karşı uygulanacak düzeltme çarpanı. adjustment = round(imbalance × factor × 0.5). 0 = denge kapalı (her batch sabit boyut); 1.0 = tam agresif denge. Default 0.7 doküman önerisidir."
                hint="0.00 – 1.00 (default 0.70)."
              >
                <Input
                  type="number"
                  step="0.05"
                  min="0"
                  max="1"
                  value={elisBalanceFactor}
                  onChange={(e) =>
                    patch({ elis_balance_factor: Number(e.target.value) })
                  }
                />
              </Field>
            </div>

            <Field
              label="Pencere stop (sn)"
              tooltip="Market kapanışından bu kadar saniye önce yeni emir verilmez; açık emirler iptal edilir ve strateji Done durumuna geçer. Kapanış volatilitesinden korunmak için kullanılır."
              hint={`10 – 120 sn (default ${STRATEGY_PARAMS_DEFAULTS.elis_stop_before_end_secs}).`}
            >
              <Input
                type="number"
                step="5"
                min="10"
                max="120"
                value={elisStopBeforeEndSecs}
                onChange={(e) =>
                  patch({ elis_stop_before_end_secs: Number(e.target.value) })
                }
              />
            </Field>
          </div>

          {/* Elis özet kartı */}
          <div className="space-y-2 rounded-md border border-border/40 bg-muted/10 px-3 py-2.5 text-xs leading-relaxed text-muted-foreground">
            <p className="font-medium text-foreground">Elis — nasıl çalışır?</p>
            <ul className="list-disc space-y-1 pl-4">
              <li>
                <strong>Spread tespiti:</strong> Her tick'te UP_ask − UP_bid ve
                DOWN_ask − DOWN_bid hesaplanır. Her ikisi de{" "}
                <code>spread_threshold</code>'u aşarsa batch tetiklenir.
              </li>
              <li>
                <strong>Maker bid emri:</strong> UP ve DOWN taraflarına{" "}
                <em>bid fiyatından</em> GTC limit emir verilir. UP_bid +
                DOWN_bid {"<"} $1.00 olduğundan fill olursa garantili kâr.
              </li>
              <li>
                <strong>Balance factor:</strong> UP ve DOWN dolum miktarı
                arasındaki fark varsa <code>balance_factor</code> ile düzeltme
                yapılır: geride kalan tarafa daha büyük emir verilir.
              </li>
              <li>
                <strong>Cooldown döngüsü:</strong> Batch yerleştikten sonra{" "}
                <code>trade_cooldown_ms</code> beklenir, açık emirler iptal
                edilir, Idle'a dönülür ve spread kontrolü yeniden başlar.
              </li>
              <li>
                <strong>Pencere stop:</strong> Kapanıştan{" "}
                <code>stop_before_end_secs</code> önce tüm yeni emirler durur;
                mevcut emirler iptal edilerek Done'a geçilir.
              </li>
            </ul>
          </div>
        </div>
      )}

      {/* ── Alis / ortak RTDS + profit-lock (Elis'te gizli) ──────────── */}
      {!isElis && (
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
      )}

      {/* ── Alis özel parametreleri ───────────────────────────────────── */}
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

      {/* ── Aras parametreleri ────────────────────────────────────────── */}
      {isAras && (
        <div className="space-y-3">
          <div>
            <SectionLabel icon={TrendingUp} title="Aras parametreleri" />
            <p className="text-muted-foreground mt-1 text-sm">
              Çift Taraflı Eş Zamanlı Alım Arbitrajı. Her{" "}
              <code>poll_secs</code> saniyede UP <strong>ve</strong> DOWN
              taraflarına aynı anda bid−1tick GTC emir verilir. Fiyat yükselince
              de, düşünce de alım devam eder; tek filtre{" "}
              <code>entry_a + ask_b &lt; 1.00</code> (pair kârlı olmalı).
            </p>
          </div>

          {/* Zamanlama */}
          <div className="bg-muted/25 space-y-4 rounded-md border border-border/40 p-3">
            <p className="text-muted-foreground text-xs font-semibold uppercase tracking-wider">
              Zamanlama
            </p>
            <Field
              label="Poll aralığı (sn)"
              tooltip="Her iki taraf için emir kontrolü bu sıklıkla yapılır. Koşullar uygunsa her tarafa bid−1tick GTC emir gönderilir. Default: 2.0 sn."
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
          </div>

          {/* Emir boyutu ve limit */}
          <div className="bg-muted/25 space-y-4 rounded-md border border-border/40 p-3">
            <p className="text-muted-foreground text-xs font-semibold uppercase tracking-wider">
              Emir boyutu &amp; limit
            </p>
            <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
              <Field
                label="Emir başı share"
                tooltip="Her emir için share miktarı. İmbalans koruması: bir taraf diğerinden > 1 emir (bu miktarda share) fazla olamaz. Default: 40."
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
                tooltip="Tek bir tarafın (UP veya DOWN) toplam maliyet tavanı. Bu eşik aşıldığında o taraf için emir gönderilmez. Default: 500 USDC."
                hint="50 – 10 000 USDC (default 500)."
              >
                <Input
                  type="number"
                  step="50"
                  min="50"
                  max="10000"
                  value={arasMaxUsdPerSide}
                  onChange={(e) =>
                    patch({ aras_max_usd_per_side: Number(e.target.value) })
                  }
                />
              </Field>
            </div>
          </div>

          {/* Bant ve yükselen ağırlık */}
          <div className="bg-muted/25 space-y-4 rounded-md border border-border/40 p-3">
            <p className="text-muted-foreground text-xs font-semibold uppercase tracking-wider">
              İşlem bandı &amp; yükselen ağırlık
            </p>
            <div className="grid grid-cols-1 gap-3 sm:grid-cols-3">
              <Field
                label="Alt bant (band_low)"
                tooltip="Mid fiyatı bu eşiğin altındaki taraf için emir verilmez. Settle yakını aşırı riskten korur. Default: 0.10."
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
                label="Üst bant (band_high)"
                tooltip="Mid fiyatı bu eşiğin üstündeki taraf için emir verilmez. Settle yakını aşırı riskten korur. Default: 0.90."
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
              <Field
                label="Yükselen taraf çarpanı"
                tooltip="Pahalı taraf (mid ≥ 0.50) için emir büyüklüğü çarpanı. 1.25 = %25 fazla share. Simülasyon: 1.25x en iyi ROI, >1.5 arbitraj garantisini bozabilir. Default: 1.25."
                hint="1.00 – 1.50 (default 1.25)."
              >
                <Input
                  type="number"
                  step="0.05"
                  min="1.00"
                  max="1.50"
                  value={arasRisingSharesMult}
                  onChange={(e) =>
                    patch({ aras_rising_shares_mult: Number(e.target.value) })
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
                <strong>Eş zamanlı alım:</strong> Her poll_secs&apos;te UP{" "}
                <em>ve</em> DOWN taraflarına bid−1tick GTC emir verilir.
                Fiyat yükselse de, düşse de alım devam eder.
              </li>
              <li>
                <strong>Pasif emir:</strong> Her iki tarafa{" "}
                <em>bid−spread</em> GTC emir verilir. Spread geniş anlarda
                daha ucuz fiyat yakalanır.
              </li>
              <li>
                <strong>Yükselen ağırlık:</strong> Pahalı taraf (mid ≥ 0.50)
                emirleri{" "}
                <em>rising_shares_mult</em> çarpanıyla büyütülür. 1.25x
                simülasyonda en iyi W/L ve ROI dengesi: baseline +300.8 →{" "}
                <strong>+352.1 USDC</strong>.
              </li>
              <li>
                <strong>Çift pair cost filtresi:</strong>{" "}
                <code>entry + opp_ask &lt; 1.00</code> koşulu sağlanmayan
                taraflar atlanır (kârsız pair alımı engellenir).
              </li>
              <li>
                <strong>İmbalans koruması:</strong> Bir taraf diğerinden{" "}
                <code>shares</code> kadar fazla fill almışsa yeni emir bekler.
              </li>
              <li>
                <strong>ARB kilidi:</strong> avg_up + avg_down &lt; 1.00 ise
                garantili kâr loglanır.
              </li>
            </ul>
          </div>
        </div>
      )}
    </div>
  );
}
