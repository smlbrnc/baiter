import type { Dispatch, SetStateAction } from "react"
import { Activity, Sliders, Target, Zap } from "lucide-react"
import { Input } from "@/components/ui/input"
import type { CreateBotReq, StrategyParams } from "@/lib/types"
import { STRATEGY_PARAMS_DEFAULTS } from "@/lib/types"
import {
  Field,
  SectionLabel,
  ToggleRow,
} from "@/components/bots/bot-form-shared"

type Props = {
  form: CreateBotReq
  setForm: Dispatch<SetStateAction<CreateBotReq>>
}

/**
 * Backend `config::StrategyParams` alanlarını expose eder. Tüm değerler
 * opsiyoneldir; kullanıcı dokunmazsa backend `_or_default()` uygular.
 */
export function BotFormStrategyParamsSection({ form, setForm }: Props) {
  const params: StrategyParams = form.strategy_params ?? {}
  const isAlis = form.strategy === "alis"
  const isElis = form.strategy === "elis"
  const isBonereaper = form.strategy === "bonereaper"
  const isGravie = form.strategy === "gravie"

  const patch = (next: Partial<StrategyParams>) => {
    setForm({
      ...form,
      strategy_params: { ...params, ...next },
    })
  }

  // ── Alis / ortak parametreler ─────────────────────────────────────────
  const profitLockPct =
    params.profit_lock_pct ?? STRATEGY_PARAMS_DEFAULTS.profit_lock_pct
  const openDelta = params.open_delta ?? STRATEGY_PARAMS_DEFAULTS.open_delta
  const pyramidAggDelta =
    params.pyramid_agg_delta ?? STRATEGY_PARAMS_DEFAULTS.pyramid_agg_delta
  const pyramidFakDelta =
    params.pyramid_fak_delta ?? STRATEGY_PARAMS_DEFAULTS.pyramid_fak_delta
  const pyramidUsdc = params.pyramid_usdc ?? null

  // ── Elis Dutch Book Bid Loop ──────────────────────────────────────────
  const elisMaxBuyOrderSize =
    params.elis_max_buy_order_size ??
    STRATEGY_PARAMS_DEFAULTS.elis_max_buy_order_size
  const elisTradeCooldownMs =
    params.elis_trade_cooldown_ms ??
    STRATEGY_PARAMS_DEFAULTS.elis_trade_cooldown_ms
  const elisStopBeforeEndSecs =
    params.elis_stop_before_end_secs ??
    STRATEGY_PARAMS_DEFAULTS.elis_stop_before_end_secs
  const elisMinImprovement =
    params.elis_min_improvement ?? STRATEGY_PARAMS_DEFAULTS.elis_min_improvement
  const elisVolThreshold =
    params.elis_vol_threshold ?? STRATEGY_PARAMS_DEFAULTS.elis_vol_threshold
  const elisBsiFilterThreshold =
    params.elis_bsi_filter_threshold ??
    STRATEGY_PARAMS_DEFAULTS.elis_bsi_filter_threshold
  const elisLockThreshold =
    params.elis_lock_threshold ?? STRATEGY_PARAMS_DEFAULTS.elis_lock_threshold
  const elisMaxOrderAgeMs =
    params.elis_max_order_age_ms ??
    STRATEGY_PARAMS_DEFAULTS.elis_max_order_age_ms
  const elisImpFailCooldownMs =
    params.elis_imp_fail_cooldown_ms ??
    STRATEGY_PARAMS_DEFAULTS.elis_imp_fail_cooldown_ms
  const elisImbalanceTakerThreshold =
    params.elis_imbalance_taker_threshold ??
    STRATEGY_PARAMS_DEFAULTS.elis_imbalance_taker_threshold

  // ── Gravie (Bot 66 davranış kopyası) ──────────────────────────────────
  const gravieTickIntervalSecs =
    params.gravie_tick_interval_secs ??
    STRATEGY_PARAMS_DEFAULTS.gravie_tick_interval_secs
  const gravieBuyCooldownMs =
    params.gravie_buy_cooldown_ms ??
    STRATEGY_PARAMS_DEFAULTS.gravie_buy_cooldown_ms
  const gravieEntryAskCeiling =
    params.gravie_entry_ask_ceiling ??
    STRATEGY_PARAMS_DEFAULTS.gravie_entry_ask_ceiling
  const gravieSecondLegGuardMs =
    params.gravie_second_leg_guard_ms ??
    STRATEGY_PARAMS_DEFAULTS.gravie_second_leg_guard_ms
  const gravieSecondLegOppTrigger =
    params.gravie_second_leg_opp_trigger ??
    STRATEGY_PARAMS_DEFAULTS.gravie_second_leg_opp_trigger
  const gravieTCutoffSecs =
    params.gravie_t_cutoff_secs ??
    STRATEGY_PARAMS_DEFAULTS.gravie_t_cutoff_secs
  const gravieBalanceRebalance =
    params.gravie_balance_rebalance ??
    STRATEGY_PARAMS_DEFAULTS.gravie_balance_rebalance
  const gravieRebalanceCeilingMultiplier =
    params.gravie_rebalance_ceiling_multiplier ??
    STRATEGY_PARAMS_DEFAULTS.gravie_rebalance_ceiling_multiplier
  const gravieSumAvgCeiling =
    params.gravie_sum_avg_ceiling ??
    STRATEGY_PARAMS_DEFAULTS.gravie_sum_avg_ceiling
  const gravieOppAskStopThreshold =
    params.gravie_opp_ask_stop_threshold ??
    STRATEGY_PARAMS_DEFAULTS.gravie_opp_ask_stop_threshold
  const gravieMaxFakSize =
    params.gravie_max_fak_size ?? STRATEGY_PARAMS_DEFAULTS.gravie_max_fak_size

  // ── Bonereaper (order-book reactive martingale + late winner) ─────────
  const bonereaperBuyCooldownMs =
    params.bonereaper_buy_cooldown_ms ??
    STRATEGY_PARAMS_DEFAULTS.bonereaper_buy_cooldown_ms
  const bonereaperLateWinnerSecs =
    params.bonereaper_late_winner_secs ??
    STRATEGY_PARAMS_DEFAULTS.bonereaper_late_winner_secs
  const bonereaperLateWinnerBidThr =
    params.bonereaper_late_winner_bid_thr ??
    STRATEGY_PARAMS_DEFAULTS.bonereaper_late_winner_bid_thr
  const bonereaperLateWinnerUsdc =
    params.bonereaper_late_winner_usdc ??
    STRATEGY_PARAMS_DEFAULTS.bonereaper_late_winner_usdc
  const bonereaperImbalanceThr =
    params.bonereaper_imbalance_thr ??
    STRATEGY_PARAMS_DEFAULTS.bonereaper_imbalance_thr
  const bonereaperMaxAvgSum =
    params.bonereaper_max_avg_sum ??
    STRATEGY_PARAMS_DEFAULTS.bonereaper_max_avg_sum
  const bonereaperSizeLongshotUsdc =
    params.bonereaper_size_longshot_usdc ??
    STRATEGY_PARAMS_DEFAULTS.bonereaper_size_longshot_usdc
  const bonereaperSizeMidUsdc =
    params.bonereaper_size_mid_usdc ??
    STRATEGY_PARAMS_DEFAULTS.bonereaper_size_mid_usdc
  const bonereaperSizeHighUsdc =
    params.bonereaper_size_high_usdc ??
    STRATEGY_PARAMS_DEFAULTS.bonereaper_size_high_usdc

  return (
    <div className="space-y-3">
      {/* ── Elis Dutch Book Bid Loop parametreleri ───────────────────────── */}
      {isElis && (
        <div className="space-y-3">
          <div>
            <SectionLabel icon={Zap} title="Elis — Dutch Book Bid Loop" />
            <p className="mt-1 text-sm text-muted-foreground">
              Her 2 saniyede bir döngü: <code>up_bid + dn_bid &lt; $1.00</code>{" "}
              koşulunda dominant tarafa ask (taker), weaker tarafa bid (maker)
              emir verilir. Gabagool pattern&apos;ları: P2 lock, P4 improvement, P5
              filter, P6 stale.
            </p>
          </div>

          <div className="space-y-4 rounded-md border border-border/40 bg-muted/25 p-3">
            {/* ── Emir parametreleri ─── */}
            <div className="grid grid-cols-1 gap-3 sm:grid-cols-3">
              <Field
                label="Temel emir boyutu (share)"
                tooltip="Her döngüde UP ve DOWN taraflarına verilecek temel share miktarı. Önceki döngüde dolmayan emirlerin kalan miktarı bu taban üstüne eklenir (cap: base×5)."
                hint={`1 – 1000 (default ${STRATEGY_PARAMS_DEFAULTS.elis_max_buy_order_size}).`}
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
              <Field
                label="Loop süresi (ms)"
                tooltip="Emir verildikten bu süre sonra açık elis emirleri iptal edilir ve yeni döngü başlar."
                hint={`500 – 10 000 (default ${STRATEGY_PARAMS_DEFAULTS.elis_trade_cooldown_ms}).`}
              >
                <Input
                  type="number"
                  step="500"
                  min="500"
                  max="10000"
                  value={elisTradeCooldownMs}
                  onChange={(e) =>
                    patch({ elis_trade_cooldown_ms: Number(e.target.value) })
                  }
                />
              </Field>
              <Field
                label="Pencere stop (sn)"
                tooltip="Market kapanışından bu kadar saniye önce yeni emir verilmez; Done'a geçilir."
                hint={`10 – 120 (default ${STRATEGY_PARAMS_DEFAULTS.elis_stop_before_end_secs}).`}
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

            {/* ── P4 + P2 ─── */}
            <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
              <Field
                label="P4 — Min improvement"
                tooltip="Yeni alımın avg pair cost'u (avg_up + avg_down) bu kadar düşürmesi gerekir, aksi halde emir verilmez. İlk fill'de bu kontrol atlanır. Değer = tick + slippage + fee/size ≈ 0.005."
                hint={`0.001 – 0.05 (default ${STRATEGY_PARAMS_DEFAULTS.elis_min_improvement}).`}
              >
                <Input
                  type="number"
                  step="0.001"
                  min="0.001"
                  max="0.05"
                  value={elisMinImprovement}
                  onChange={(e) =>
                    patch({ elis_min_improvement: Number(e.target.value) })
                  }
                />
              </Field>
              <Field
                label="P2 — Lock threshold"
                tooltip="avg_up + avg_down bu değerin altına düşünce VE min(up_filled, dn_filled) > cost_basis ise pozisyon kilitli sayılır — yeni emir verilmez (Done). Garantili kâr lock'u."
                hint={`0.85 – 0.99 (default ${STRATEGY_PARAMS_DEFAULTS.elis_lock_threshold}).`}
              >
                <Input
                  type="number"
                  step="0.01"
                  min="0.85"
                  max="0.99"
                  value={elisLockThreshold}
                  onChange={(e) =>
                    patch({ elis_lock_threshold: Number(e.target.value) })
                  }
                />
              </Field>
            </div>

            {/* ── P5 + P6 ─── */}
            <div className="grid grid-cols-1 gap-3 sm:grid-cols-3">
              <Field
                label="P5 — Vol threshold"
                tooltip="bid-ask spread (ask − bid) bu eşiği aşarsa OB ince sayılır ve döngü atlanır. Her iki tarafın spreadi birden kontrol edilir."
                hint={`0.01 – 0.20 (default ${STRATEGY_PARAMS_DEFAULTS.elis_vol_threshold}).`}
              >
                <Input
                  type="number"
                  step="0.01"
                  min="0.01"
                  max="0.20"
                  value={elisVolThreshold}
                  onChange={(e) =>
                    patch({ elis_vol_threshold: Number(e.target.value) })
                  }
                />
              </Field>
              <Field
                label="P5 — BSI filter eşiği"
                tooltip="|BSI| bu eşiği aşarsa: BSI > +threshold → UP baskısı, DOWN alımı engellenir; BSI < -threshold → DOWN baskısı, UP alımı engellenir. BSI None ise filter pas geçer."
                hint={`0.10 – 1.00 (default ${STRATEGY_PARAMS_DEFAULTS.elis_bsi_filter_threshold}).`}
              >
                <Input
                  type="number"
                  step="0.05"
                  min="0.10"
                  max="1.00"
                  value={elisBsiFilterThreshold}
                  onChange={(e) =>
                    patch({ elis_bsi_filter_threshold: Number(e.target.value) })
                  }
                />
              </Field>
              <Field
                label="P6 — Stale order (ms)"
                tooltip="Bu süreden daha eski açık elis emirleri 2sn timer beklenmeden zorla iptal edilir. Ghost order birikimini önler."
                hint={`5 000 – 60 000 ms (default ${STRATEGY_PARAMS_DEFAULTS.elis_max_order_age_ms}).`}
              >
                <Input
                  type="number"
                  step="5000"
                  min="5000"
                  max="60000"
                  value={elisMaxOrderAgeMs}
                  onChange={(e) =>
                    patch({ elis_max_order_age_ms: Number(e.target.value) })
                  }
                />
              </Field>
            </div>

            {/* P4 Improvement fail cooldown + Inventory taker threshold */}
            <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
              <Field
                label="P4 — Improvement fail cooldown (ms)"
                tooltip="P4 improvement başarısız olunca (avg pair cost yeterince düşmüyorsa) bu süre kadar yeni emir verilmez. Mevcut maker emirlere dolma fırsatı tanır. 97 market simülasyonu: 30sn → $146 PnL (2sn NoOp: $73)."
                hint={`5 000 – 60 000 ms (default ${STRATEGY_PARAMS_DEFAULTS.elis_imp_fail_cooldown_ms}).`}
              >
                <Input
                  type="number"
                  step="5000"
                  min="5000"
                  max="60000"
                  value={elisImpFailCooldownMs}
                  onChange={(e) =>
                    patch({ elis_imp_fail_cooldown_ms: Number(e.target.value) })
                  }
                />
              </Field>
              <Field
                label="Inventory taker threshold (share)"
                tooltip="|up_filled - down_filled| bu eşiği aşarsa weaker side ASK fiyatından (taker) alınır → anında dengeleme. Avellaneda-Stoikov inventory skew + cascade exit hibrit yaklaşımı. 0 = kapalı. 54 market simülasyonu: thr=100 → +%57 PnL ($47→$74), 0 zarar."
                hint={`0 (kapalı) veya 50–200 share (default ${STRATEGY_PARAMS_DEFAULTS.elis_imbalance_taker_threshold}).`}
              >
                <Input
                  type="number"
                  step="20"
                  min="0"
                  max="500"
                  value={elisImbalanceTakerThreshold}
                  onChange={(e) =>
                    patch({
                      elis_imbalance_taker_threshold: Number(e.target.value),
                    })
                  }
                />
              </Field>
            </div>
          </div>

          {/* Elis özet kartı */}
          <div className="space-y-2 rounded-md border border-border/40 bg-muted/10 px-3 py-2.5 text-xs leading-relaxed text-muted-foreground">
            <p className="font-medium text-foreground">Elis — nasıl çalışır?</p>
            <ul className="list-disc space-y-1 pl-4">
              <li>
                <strong>Koşul:</strong> <code>up_bid + dn_bid &lt; $1.00</code>{" "}
                → dominant taraf ask (taker), weaker taraf bid (maker) GTC emir.
                Fill = garantili kâr.
              </li>
              <li>
                <strong>P4 Improvement:</strong> Mevcut fill varsa yeni alım{" "}
                <code>avg pair cost</code>&apos;u <code>min_improvement</code>{" "}
                kadar düşürmedikçe emir verilmez.
              </li>
              <li>
                <strong>P5 Filters:</strong> Vol filter — spread geniş ise OB
                ince, atla. BSI filter — aşırı tek yönlü akışta karşı tarafı
                engelle.
              </li>
              <li>
                <strong>P2 Lock:</strong>{" "}
                <code>avg_up + avg_down &lt; lock_threshold</code> VE{" "}
                <code>pair_count &gt; cost_basis</code> → garantili kâr
                kilitlendi, Done&apos;a geç.
              </li>
              <li>
                <strong>P4 Imp.Fail Cooldown:</strong> Improvement geçemeyince{" "}
                <code>imp_fail_cooldown_ms</code> (30sn) bekle — mevcut maker
                emirlere dolma fırsatı. Sim: 2× daha yüksek PnL.
              </li>
              <li>
                <strong>Inventory Taker (Avellaneda-Stoikov):</strong>{" "}
                <code>|q| &gt; threshold</code> (default 100) ise weaker side
                ASK ile anında doldurulur (cascade exit). Tek-taraflı pozisyon
                varyansını engeller. Sim: +%57 PnL.
              </li>
              <li>
                <strong>P6 Stale:</strong> <code>max_order_age_ms</code>
                &apos;den eski emirler zorla iptal edilir (ghost order
                koruması).
              </li>
            </ul>
          </div>
        </div>
      )}

      {/* ── Alis profit-lock (sadece Alis için) ─────────────────── */}
      {isAlis && (
        <div className="space-y-3">
          <div>
            <SectionLabel icon={Sliders} title="Strateji parametreleri" />
            <p className="mt-1 text-sm text-muted-foreground">
              Sinyal: Binance CVD imbalance + OKX EMA momentum (sabit, ayar
              gerektirmez).
            </p>
          </div>

          <div className="space-y-4 rounded-md border border-border/40 bg-muted/25 p-3">
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
            <p className="mt-1 text-sm text-muted-foreground">
              Opener ve pyramid emir delta&apos;ları; pyramid bütçesi.
            </p>
          </div>

          <div className="space-y-4 rounded-md border border-border/40 bg-muted/25 p-3">
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
                    const raw = e.target.value.trim()
                    patch({
                      pyramid_usdc: raw === "" ? null : Number(raw),
                    })
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

      {/* ── Bonereaper parametreleri (real wallet kopyası) ──────────────── */}
      {isBonereaper && (
        <div className="space-y-3">
          <div>
            <SectionLabel icon={Target} title="Bonereaper parametreleri" />
            <p className="mt-1 text-sm text-muted-foreground">
              Polymarket{" "}
              <a
                href="https://polymarket.com/profile/"
                className="underline"
                target="_blank"
                rel="noreferrer"
              >
                Bonereaper
              </a>{" "}
              wallet davranış kopyası: order-book reactive martingale + late
              winner injection. Sinyal kullanmaz; her tick bid değişen tarafa
              taker BUY ve T-X sn kala kazanan tarafa massive inject.
            </p>
          </div>

          <div className="space-y-4 rounded-md border border-border/40 bg-muted/25 p-3">
            <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
              <Field
                label="BUY cooldown (ms)"
                tooltip="Ardışık BUY emirleri arasındaki minimum bekleme. Real bot ortalama ~3-5 sn aralıkla emir veriyor. Düşük = daha agresif (daha çok trade), yüksek = daha az."
                hint={`500 – 60000 ms (default ${STRATEGY_PARAMS_DEFAULTS.bonereaper_buy_cooldown_ms}).`}
              >
                <Input
                  type="number"
                  step="500"
                  min="500"
                  max="60000"
                  value={bonereaperBuyCooldownMs}
                  onChange={(e) =>
                    patch({ bonereaper_buy_cooldown_ms: Number(e.target.value) })
                  }
                />
              </Field>
              <Field
                label="Imbalance threshold (share)"
                tooltip="|UP_filled − DOWN_filled| bu eşiği aşarsa weaker side rebalance trade'i yapılır (orderbook-driven yön seçimi bypass edilir). Çok düşük = sürekli rebalance, yüksek = serbest pyramid."
                hint={`0 – 10000 share (default ${STRATEGY_PARAMS_DEFAULTS.bonereaper_imbalance_thr}).`}
              >
                <Input
                  type="number"
                  step="10"
                  min="0"
                  max="10000"
                  value={bonereaperImbalanceThr}
                  onChange={(e) =>
                    patch({ bonereaper_imbalance_thr: Number(e.target.value) })
                  }
                />
              </Field>
              <Field
                label="Max avg_sum (yumuşak cap)"
                tooltip="new_avg + opp_avg bu değeri aşarsa yeni alım yok. Real bot 1.20'ye kadar trade görüldü; default 1.30 güvenli üst sınır. Düşük = sıkı (aşırı pyramid'i durdurur)."
                hint={`0.50 – 2.00 (default ${STRATEGY_PARAMS_DEFAULTS.bonereaper_max_avg_sum}).`}
              >
                <Input
                  type="number"
                  step="0.05"
                  min="0.5"
                  max="2"
                  value={bonereaperMaxAvgSum}
                  onChange={(e) =>
                    patch({ bonereaper_max_avg_sum: Number(e.target.value) })
                  }
                />
              </Field>
              <Field
                label="Late winner penceresi (sn)"
                tooltip="T-X sn'den itibaren bid≥thr olan tarafa massive taker BUY (cooldown bypass). Real bot kapanışa <30 sn kala bid 0.95+ tarafa $1000+ atıyor. 0 = kural KAPALI."
                hint={`0 – 300 sn (default ${STRATEGY_PARAMS_DEFAULTS.bonereaper_late_winner_secs}).`}
              >
                <Input
                  type="number"
                  step="5"
                  min="0"
                  max="300"
                  value={bonereaperLateWinnerSecs}
                  onChange={(e) =>
                    patch({ bonereaper_late_winner_secs: Number(e.target.value) })
                  }
                />
              </Field>
              <Field
                label="Late winner bid eşiği"
                tooltip="Late winner penceresinde bu bid'in üstündeki taraf 'kazanan' sayılır ve massive BUY tetiklenir."
                hint={`0.50 – 0.99 (default ${STRATEGY_PARAMS_DEFAULTS.bonereaper_late_winner_bid_thr}).`}
              >
                <Input
                  type="number"
                  step="0.05"
                  min="0.5"
                  max="0.99"
                  value={bonereaperLateWinnerBidThr}
                  onChange={(e) =>
                    patch({ bonereaper_late_winner_bid_thr: Number(e.target.value) })
                  }
                />
              </Field>
              <Field
                label="Late winner USDC notional"
                tooltip="Late winner trade büyüklüğü. Real bot $1000+ atıyor; biz $100 default ile başlayıp izleyip artırırız. 0 = kural KAPALI."
                hint={`0 – 10000 USDC (default ${STRATEGY_PARAMS_DEFAULTS.bonereaper_late_winner_usdc}).`}
              >
                <Input
                  type="number"
                  step="10"
                  min="0"
                  max="10000"
                  value={bonereaperLateWinnerUsdc}
                  onChange={(e) =>
                    patch({ bonereaper_late_winner_usdc: Number(e.target.value) })
                  }
                />
              </Field>
            </div>

            <div className="grid grid-cols-1 gap-3 sm:grid-cols-3">
              <Field
                label="Long-shot USDC (bid ≤ 0.30)"
                tooltip="Düşük confidence bid'lerde (uzun-vade, yüksek pay-off) trade büyüklüğü. Real bot bu bantta küçük trade'ler ($1-5)."
                hint={`0 – 10000 USDC (default ${STRATEGY_PARAMS_DEFAULTS.bonereaper_size_longshot_usdc}).`}
              >
                <Input
                  type="number"
                  step="1"
                  min="0"
                  max="10000"
                  value={bonereaperSizeLongshotUsdc}
                  onChange={(e) =>
                    patch({ bonereaper_size_longshot_usdc: Number(e.target.value) })
                  }
                />
              </Field>
              <Field
                label="Mid USDC (0.30 < bid ≤ 0.85)"
                tooltip="Orta band trade büyüklüğü; çoğu trade burada (bizim default 10 USDC, real bot medyanı ~$5-15)."
                hint={`0 – 10000 USDC (default ${STRATEGY_PARAMS_DEFAULTS.bonereaper_size_mid_usdc}).`}
              >
                <Input
                  type="number"
                  step="1"
                  min="0"
                  max="10000"
                  value={bonereaperSizeMidUsdc}
                  onChange={(e) =>
                    patch({ bonereaper_size_mid_usdc: Number(e.target.value) })
                  }
                />
              </Field>
              <Field
                label="High-conf USDC (bid > 0.85)"
                tooltip="Yüksek confidence (kazanan taraf belirginleşmiş) trade büyüklüğü. Real bot burada $20-50 trade'ler."
                hint={`0 – 10000 USDC (default ${STRATEGY_PARAMS_DEFAULTS.bonereaper_size_high_usdc}).`}
              >
                <Input
                  type="number"
                  step="1"
                  min="0"
                  max="10000"
                  value={bonereaperSizeHighUsdc}
                  onChange={(e) =>
                    patch({ bonereaper_size_high_usdc: Number(e.target.value) })
                  }
                />
              </Field>
            </div>
          </div>

          <div className="space-y-2 rounded-md border border-border/40 bg-muted/10 px-3 py-2.5 text-xs leading-relaxed text-muted-foreground">
            <p className="font-medium text-foreground">
              Bonereaper — nasıl çalışır?
            </p>
            <ul className="list-disc space-y-1 pl-4">
              <li>
                <strong>Sinyal kullanmaz:</strong> Binance/OKX composite,
                triple gate, profit lock, freeze, mid-band ban gibi sinyal
                kuralları YOK (real bot da kullanmıyor).
              </li>
              <li>
                <strong>Yön seçimi (orderbook-driven):</strong> Her tick
                <code> |Δup_bid|</code> vs <code>|Δdn_bid|</code> karşılaştırılır;
                bid&apos;i daha çok değişen tarafa taker BUY @ ask.
              </li>
              <li>
                <strong>Imbalance rebalance:</strong>{" "}
                <code>|up_filled − down_filled| &gt; thr</code> ise weaker side
                seçilir (orderbook bypass).
              </li>
              <li>
                <strong>Late winner injection:</strong> T-X sn kala max(bid) ≥
                threshold ise winner tarafa <code>late_winner_usdc</code>
                notional taker BUY (cooldown bypass). Real bot kapanışta bid
                0.95+ tarafa $1000+ atıyor.
              </li>
              <li>
                <strong>Dinamik size:</strong> bid bucket&apos;ına göre USDC
                notional (long-shot / mid / high). Boyut ASK fiyatına bölünüp
                yukarı yuvarlanır.
              </li>
              <li>
                <strong>avg_sum yumuşak cap:</strong>{" "}
                <code>new_avg + opp_avg &gt; max_avg_sum</code> ise yeni alım
                yok (martingale güvenliği).
              </li>
              <li>
                <strong>Cooldown:</strong> Ardışık BUY arasında{" "}
                <code>buy_cooldown_ms</code> bekleme; late winner bunu bypass
                eder.
              </li>
            </ul>
          </div>
        </div>
      )}

      {/* ── Gravie parametreleri (Bot 66 davranış kopyası) ─────────────── */}
      {isGravie && (
        <div className="space-y-3">
          <div>
            <SectionLabel icon={Activity} title="Gravie parametreleri" />
            <p className="mt-1 text-sm text-muted-foreground">
              Bot 66 (<code>Lively-Authenticity</code>) davranış kopyası.
              Sinyal kullanmaz: saf order book reaktif, dual-side BUY-only FAK
              taker. Default değerler{" "}
              <a
                href="https://hudme.com/bots/71"
                className="underline"
                target="_blank"
                rel="noreferrer"
              >
                Bot 71 tick verisi
              </a>{" "}
              + Bot 66 mikro-davranış sondajından kalibre edilmiştir.
            </p>
          </div>

          <div className="space-y-4 rounded-md border border-border/40 bg-muted/25 p-3">
            {/* Tick & cooldown */}
            <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
              <Field
                label="Tick aralığı (sn)"
                tooltip="Karar döngüsü periyodu. Her N saniyede bir BUY denenebilir. Bot 66 ortalama inter-arrival 4-5 sn. Düşük = daha agresif (daha çok trade), yüksek = daha az trade."
                hint={`1 – 60 sn (default ${STRATEGY_PARAMS_DEFAULTS.gravie_tick_interval_secs}).`}
              >
                <Input
                  type="number"
                  step="1"
                  min="1"
                  max="60"
                  value={gravieTickIntervalSecs}
                  onChange={(e) =>
                    patch({
                      gravie_tick_interval_secs: Number(e.target.value),
                    })
                  }
                />
              </Field>
              <Field
                label="BUY cooldown (ms)"
                tooltip="Ardışık BUY emirleri arası minimum bekleme. Bot 66 medyan inter-arrival 4-5 sn. Cooldown çok kısa olursa over-trade, çok uzun olursa fırsat kaçırma."
                hint={`500 – 60 000 ms (default ${STRATEGY_PARAMS_DEFAULTS.gravie_buy_cooldown_ms}).`}
              >
                <Input
                  type="number"
                  step="500"
                  min="500"
                  max="60000"
                  value={gravieBuyCooldownMs}
                  onChange={(e) =>
                    patch({ gravie_buy_cooldown_ms: Number(e.target.value) })
                  }
                />
              </Field>
            </div>

            {/* Entry & second-leg */}
            <div className="grid grid-cols-1 gap-3 sm:grid-cols-3">
              <Field
                label="Entry ask tavanı"
                tooltip="Yeni leg açma için ask fiyat tavanı. Bu üstündeki fiyatlardan satın alınmaz (rebalance modu hariç). Bot 66 first entry medyan 0.50, p75 ≈ 0.575. Düşük = sıkı/seçici, yüksek = agresif birikim."
                hint={`0.10 – 0.99 (default ${STRATEGY_PARAMS_DEFAULTS.gravie_entry_ask_ceiling}).`}
              >
                <Input
                  type="number"
                  step="0.05"
                  min="0.10"
                  max="0.99"
                  value={gravieEntryAskCeiling}
                  onChange={(e) =>
                    patch({
                      gravie_entry_ask_ceiling: Number(e.target.value),
                    })
                  }
                />
              </Field>
              <Field
                label="Second-leg guard (ms)"
                tooltip="İlk leg açıldıktan sonra karşı tarafa otomatik geçiş için minimum bekleme süresi. Bu süre dolduğunda VEYA opp_ask trigger eşiğinin altına düştüğünde flip yapılır. Bot 66 5m median 38 sn."
                hint={`0 – 600 000 ms (default ${STRATEGY_PARAMS_DEFAULTS.gravie_second_leg_guard_ms}).`}
              >
                <Input
                  type="number"
                  step="1000"
                  min="0"
                  max="600000"
                  value={gravieSecondLegGuardMs}
                  onChange={(e) =>
                    patch({
                      gravie_second_leg_guard_ms: Number(e.target.value),
                    })
                  }
                />
              </Field>
              <Field
                label="Second-leg opp tetikleyici"
                tooltip="Karşı taraf ask bu eşiğin altına inerse guard süresi beklenmeden hemen flip. Bot 66 opp_first_px medyan ≈ 0.50."
                hint={`0.10 – 0.95 (default ${STRATEGY_PARAMS_DEFAULTS.gravie_second_leg_opp_trigger}).`}
              >
                <Input
                  type="number"
                  step="0.05"
                  min="0.10"
                  max="0.95"
                  value={gravieSecondLegOppTrigger}
                  onChange={(e) =>
                    patch({
                      gravie_second_leg_opp_trigger: Number(e.target.value),
                    })
                  }
                />
              </Field>
            </div>

            {/* Cutoff & rebalance */}
            <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
              <Field
                label="T-cutoff (sn)"
                tooltip="Kapanışa bu kadar sn kala yeni emir verilmez (Stopped). Bot 66 5m median son trade T-78, %58 ≤ T-90. 5m için 90, 15m için 180 önerilir."
                hint={`0 – 600 sn (default ${STRATEGY_PARAMS_DEFAULTS.gravie_t_cutoff_secs}).`}
              >
                <Input
                  type="number"
                  step="10"
                  min="0"
                  max="600"
                  value={gravieTCutoffSecs}
                  onChange={(e) =>
                    patch({ gravie_t_cutoff_secs: Number(e.target.value) })
                  }
                />
              </Field>
              <Field
                label="Sum-avg tavanı"
                tooltip="avg_up + avg_dn ≥ X olduğunda yeni emir verilmez (pair zaten pahalı; daha fazla harcama beklenen değeri kötüleştirir). Sim'de 1.20 çok geç oluyor; 1.05 erken durmayı sağlar."
                hint={`0.80 – 1.50 (default ${STRATEGY_PARAMS_DEFAULTS.gravie_sum_avg_ceiling}).`}
              >
                <Input
                  type="number"
                  step="0.01"
                  min="0.80"
                  max="1.50"
                  value={gravieSumAvgCeiling}
                  onChange={(e) =>
                    patch({ gravie_sum_avg_ceiling: Number(e.target.value) })
                  }
                />
              </Field>
            </div>

            <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
              <Field
                label="Balance rebalance eşiği"
                tooltip="min(up_filled, dn_filled) / max(...) bu eşiğin altındaysa az olan tarafa zorunlu yönelir (rebalance). Düşük = daha az rebalance. Sim'de 0.45 ile %42 trade rebalance idi; 0.30 ile %20-25'e iner."
                hint={`0.0 – 1.0 (default ${STRATEGY_PARAMS_DEFAULTS.gravie_balance_rebalance}).`}
              >
                <Input
                  type="number"
                  step="0.05"
                  min="0"
                  max="1"
                  value={gravieBalanceRebalance}
                  onChange={(e) =>
                    patch({
                      gravie_balance_rebalance: Number(e.target.value),
                    })
                  }
                />
              </Field>
              <Field
                label="Rebalance ceiling esneme"
                tooltip="Rebalance modunda entry ceiling bu oranla esnetilir. Örn 0.65 × 1.20 = 0.78'a kadar al. Az tarafa pozisyon bulmayı kolaylaştırır."
                hint={`1.0 – 2.0 (default ${STRATEGY_PARAMS_DEFAULTS.gravie_rebalance_ceiling_multiplier}).`}
              >
                <Input
                  type="number"
                  step="0.05"
                  min="1.0"
                  max="2.0"
                  value={gravieRebalanceCeilingMultiplier}
                  onChange={(e) =>
                    patch({
                      gravie_rebalance_ceiling_multiplier: Number(
                        e.target.value
                      ),
                    })
                  }
                />
              </Field>
            </div>

            {/* Risk guards (Patch A + Patch C) */}
            <div className="grid grid-cols-1 gap-3 sm:grid-cols-2 border-t border-border/30 pt-4">
              <Field
                label="Lose-side ASK cap (Patch A)"
                tooltip="ASIMETRIK TREND REVERSAL GUARD. max(up_ask, dn_ask) bu eşiğin üstüne çıkarsa tüm yeni emirler durur. Polymarket fiyatı = olasılık; bir taraf 0.95+ ise market o tarafı %95+ olası görüyor. Default 0.95 = YUMUŞAK guard: extreme collapse'ı yakalar, big-win'leri korur. 0.85 = TUTUCU (collapse mükemmel ama big-win'leri tıraşlar). 1.0 = DEVRE DIŞI."
                hint={`0.50 – 1.00 (default ${STRATEGY_PARAMS_DEFAULTS.gravie_opp_ask_stop_threshold}).`}
              >
                <Input
                  type="number"
                  step="0.05"
                  min="0.50"
                  max="1.00"
                  value={gravieOppAskStopThreshold}
                  onChange={(e) =>
                    patch({
                      gravie_opp_ask_stop_threshold: Number(e.target.value),
                    })
                  }
                />
              </Field>
              <Field
                label="Max FAK size (Patch C)"
                tooltip="FAK emir başına maksimum share. Düşen fiyatlarda ceil(usdc/price) patlamasını önler. Örn order_usdc=10, price=0.05 → 200 share; cap=50 ile 50 share. 0 = sınırsız (devre dışı)."
                hint={`0 (sınırsız) veya 1 – 10 000 share (default ${STRATEGY_PARAMS_DEFAULTS.gravie_max_fak_size}).`}
              >
                <Input
                  type="number"
                  step="10"
                  min="0"
                  max="10000"
                  value={gravieMaxFakSize}
                  onChange={(e) =>
                    patch({ gravie_max_fak_size: Number(e.target.value) })
                  }
                />
              </Field>
            </div>
          </div>

          <div className="space-y-2 rounded-md border border-border/40 bg-muted/10 px-3 py-2.5 text-xs leading-relaxed text-muted-foreground">
            <p className="font-medium text-foreground">
              Gravie — nasıl çalışır?
            </p>
            <ul className="list-disc space-y-1 pl-4">
              <li>
                <strong>BUY-only dual-side:</strong> Hem Up hem Down için BUY,
                SELL yok. Pozisyon market resolve&apos;a kadar tutulur.
              </li>
              <li>
                <strong>Reaktif ucuz-taraf:</strong>{" "}
                <code>argmin(up_ask, dn_ask)</code> tarafına FAK BUY (anında
                fill, kalan iptal).
              </li>
              <li>
                <strong>İkinci leg guard:</strong> İlk leg açıldıktan sonra
                karşı taraf ucuz olunca <em>veya</em> guard süresi geçince flip
                yapılır.
              </li>
              <li>
                <strong>Sum-avg guard:</strong>{" "}
                <code>avg_up + avg_dn ≥ ceiling</code> ise yeni emir verilmez —
                pair pahalandığında dur.
              </li>
              <li>
                <strong>Balance rebalance:</strong> Pozisyon bir tarafa çok
                kayarsa az olan tarafa zorunlu yönel (entry ceiling esnetilir).
              </li>
              <li>
                <strong>T-cutoff:</strong> Kapanıştan X sn önce tüm emirler
                durur, açık <code>gravie:</code> emirleri iptal edilir.
              </li>
              <li>
                <strong>Sinyal kullanmaz:</strong> Bonereaper&apos;dan farklı
                olarak Binance/OKX composite skor okumaz; saf orderbook
                reaktif. Veri kaynağı bağımsızlığı = düşük operasyonel risk.
              </li>
              <li>
                <strong>Risk Guards (Patch A + C):</strong>{" "}
                <code>max(up_ask, dn_ask) ≥ 0.95</code> → tüm yeni emirler
                durur (extreme collapse koruması, yumuşak default).{" "}
                <code>FAK size ≤ 50</code> → düşen fiyatlarda likidite emici
                patlamayı engeller. Daha sıkı koruma için 0.85; devre dışı için
                1.0 yapın.
              </li>
            </ul>
          </div>
        </div>
      )}
    </div>
  )
}
