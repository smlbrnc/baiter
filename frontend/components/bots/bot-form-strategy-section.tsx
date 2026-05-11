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
  const isBonereaper = form.strategy === "bonereaper"
  const isGravie = form.strategy === "gravie"
  const isArbitrage = form.strategy === "arbitrage"
  const isBinanceLatency = form.strategy === "binance_latency"

  const patch = (next: Partial<StrategyParams>) => {
    setForm({
      ...form,
      strategy_params: { ...params, ...next },
    })
  }

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
  const bonereaperLwMaxPerSession =
    params.bonereaper_lw_max_per_session ??
    STRATEGY_PARAMS_DEFAULTS.bonereaper_lw_max_per_session
  const bonereaperImbalanceThr =
    params.bonereaper_imbalance_thr ??
    STRATEGY_PARAMS_DEFAULTS.bonereaper_imbalance_thr
  const bonereaperMaxAvgSum =
    params.bonereaper_max_avg_sum ??
    STRATEGY_PARAMS_DEFAULTS.bonereaper_max_avg_sum
  const bonereaperFirstSpreadMin =
    params.bonereaper_first_spread_min ??
    STRATEGY_PARAMS_DEFAULTS.bonereaper_first_spread_min
  const bonereaperSizeLongshotUsdc =
    params.bonereaper_size_longshot_usdc ??
    STRATEGY_PARAMS_DEFAULTS.bonereaper_size_longshot_usdc
  const bonereaperSizeMidUsdc =
    params.bonereaper_size_mid_usdc ??
    STRATEGY_PARAMS_DEFAULTS.bonereaper_size_mid_usdc
  const bonereaperSizeHighUsdc =
    params.bonereaper_size_high_usdc ??
    STRATEGY_PARAMS_DEFAULTS.bonereaper_size_high_usdc
  const bonereaperLoserMinPrice =
    params.bonereaper_loser_min_price ??
    STRATEGY_PARAMS_DEFAULTS.bonereaper_loser_min_price
  const bonereaperLoserScalpUsdc =
    params.bonereaper_loser_scalp_usdc ??
    STRATEGY_PARAMS_DEFAULTS.bonereaper_loser_scalp_usdc
  const bonereaperLoserScalpMaxPrice =
    params.bonereaper_loser_scalp_max_price ??
    STRATEGY_PARAMS_DEFAULTS.bonereaper_loser_scalp_max_price
  const bonereaperLatePyramidSecs =
    params.bonereaper_late_pyramid_secs ??
    STRATEGY_PARAMS_DEFAULTS.bonereaper_late_pyramid_secs
  const bonereaperWinnerSizeFactor =
    params.bonereaper_winner_size_factor ??
    STRATEGY_PARAMS_DEFAULTS.bonereaper_winner_size_factor
  const bonereaperLwBurstSecs =
    params.bonereaper_lw_burst_secs ??
    STRATEGY_PARAMS_DEFAULTS.bonereaper_lw_burst_secs
  const bonereaperLwBurstUsdc =
    params.bonereaper_lw_burst_usdc ??
    STRATEGY_PARAMS_DEFAULTS.bonereaper_lw_burst_usdc
  const bonereaperAvgLoserMax =
    params.bonereaper_avg_loser_max ??
    STRATEGY_PARAMS_DEFAULTS.bonereaper_avg_loser_max

  // ── Arbitrage (pure cross-leg FAK BID, avg_sum<1 garantili) ──
  const arbitrageTickIntervalMs =
    params.arbitrage_tick_interval_ms ??
    STRATEGY_PARAMS_DEFAULTS.arbitrage_tick_interval_ms
  const arbitrageCostMax =
    params.arbitrage_cost_max ?? STRATEGY_PARAMS_DEFAULTS.arbitrage_cost_max
  const arbitrageOrderUsdc =
    params.arbitrage_order_usdc ?? STRATEGY_PARAMS_DEFAULTS.arbitrage_order_usdc
  const arbitrageMaxTradesPerSession =
    params.arbitrage_max_trades_per_session ??
    STRATEGY_PARAMS_DEFAULTS.arbitrage_max_trades_per_session
  const arbitrageCooldownMs =
    params.arbitrage_cooldown_ms ?? STRATEGY_PARAMS_DEFAULTS.arbitrage_cooldown_ms
  const arbitrageEntryWindowSecs =
    params.arbitrage_entry_window_secs ??
    STRATEGY_PARAMS_DEFAULTS.arbitrage_entry_window_secs

  // ── Binance Latency Arbitrage ──────────────────────────────────────────
  const binanceLatencySigThrUsd =
    params.binance_latency_sig_thr_usd ??
    STRATEGY_PARAMS_DEFAULTS.binance_latency_sig_thr_usd
  const binanceLatencyCooldownMs =
    params.binance_latency_cooldown_ms ??
    STRATEGY_PARAMS_DEFAULTS.binance_latency_cooldown_ms
  const binanceLatencyMaxTrades =
    params.binance_latency_max_trades_per_session ??
    STRATEGY_PARAMS_DEFAULTS.binance_latency_max_trades_per_session
  const binanceLatencyOrderUsdc =
    params.binance_latency_order_usdc ??
    STRATEGY_PARAMS_DEFAULTS.binance_latency_order_usdc
  const binanceLatencyEntryWindow =
    params.binance_latency_entry_window_secs ??
    STRATEGY_PARAMS_DEFAULTS.binance_latency_entry_window_secs

  return (
    <div className="space-y-3">
      {/* ── Bonereaper parametreleri ────────────────────────────────────── */}
      {isBonereaper && (
        <div className="space-y-3">
          <div>
            <SectionLabel icon={Target} title="Bonereaper parametreleri" />
            <p className="mt-1 text-sm text-muted-foreground">
              Order-book reactive martingale + late winner injection. Backtest
              optimum (3-bot 468 session): NET ROI %0.23 (LIVE_safe).
            </p>
          </div>

          <div className="space-y-3 rounded-md border border-border/40 bg-muted/25 p-3">
            <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
              <Field
                label="Late winner USDC"
                tooltip="Kapanışa yakın kazanan tarafa tek büyük taker BUY notional'ı. LIVE_safe başlangıç: $500. 0 = kapalı."
                hint={`0 – 10000 USDC (default ${STRATEGY_PARAMS_DEFAULTS.bonereaper_late_winner_usdc}).`}
              >
                <Input
                  type="number"
                  step="50"
                  min="0"
                  max="10000"
                  value={bonereaperLateWinnerUsdc}
                  onChange={(e) =>
                    patch({ bonereaper_late_winner_usdc: Number(e.target.value) })
                  }
                />
              </Field>
              <Field
                label="LW bid eşiği"
                tooltip="Late winner için kazanan tarafın min bid değeri. Yüksek = daha sıkı/güvenli."
                hint={`0.50 – 0.99 (default ${STRATEGY_PARAMS_DEFAULTS.bonereaper_late_winner_bid_thr}).`}
              >
                <Input
                  type="number"
                  step="0.01"
                  min="0.5"
                  max="0.99"
                  value={bonereaperLateWinnerBidThr}
                  onChange={(e) =>
                    patch({ bonereaper_late_winner_bid_thr: Number(e.target.value) })
                  }
                />
              </Field>
              <Field
                label="BUY cooldown (ms)"
                tooltip="Ardışık BUY emirleri arası min bekleme. Düşük = daha çok trade."
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
                label="LW max / session"
                tooltip="Session başına maksimum late winner injection. 0 = sınırsız (spam, KULLANMA)."
                hint={`0 – 20 (default ${STRATEGY_PARAMS_DEFAULTS.bonereaper_lw_max_per_session}).`}
              >
                <Input
                  type="number"
                  step="1"
                  min="0"
                  max="20"
                  value={bonereaperLwMaxPerSession}
                  onChange={(e) =>
                    patch({ bonereaper_lw_max_per_session: Number(e.target.value) })
                  }
                />
              </Field>
            </div>

            <details className="group">
              <summary className="cursor-pointer text-xs font-medium text-muted-foreground hover:text-foreground">
                Gelişmiş ayarlar
              </summary>
              <div className="mt-3 grid grid-cols-1 gap-3 sm:grid-cols-2">
                <Field
                  label="LW penceresi (sn)"
                  tooltip="T-X sn'den itibaren late winner taraması başlar."
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
                  label="Imbalance eşik (share)"
                  tooltip="|UP-DN| bu eşiği aşarsa weaker side rebalance."
                  hint={`0 – 10000 (default ${STRATEGY_PARAMS_DEFAULTS.bonereaper_imbalance_thr}).`}
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
                  label="Max avg_sum"
                  tooltip="new_avg + opp_avg bu değerin üstünde yeni alım yok (pyramid frenleyici)."
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
                  label="İlk emir spread eşiği"
                  tooltip="|up_bid - down_bid| bu eşiği aşana kadar ilk BUY atılmaz; aşılınca yüksek bid tarafına başla. 0 = devre dışı."
                  hint={`0.00 – 0.20 (default ${STRATEGY_PARAMS_DEFAULTS.bonereaper_first_spread_min}).`}
                >
                  <Input
                    type="number"
                    step="0.01"
                    min="0"
                    max="0.2"
                    value={bonereaperFirstSpreadMin}
                    onChange={(e) =>
                      patch({ bonereaper_first_spread_min: Number(e.target.value) })
                    }
                  />
                </Field>
              </div>
              <div className="mt-3 grid grid-cols-1 gap-3 sm:grid-cols-3">
                <Field
                  label="Long-shot USDC"
                  tooltip="bid ≤ 0.30 trade büyüklüğü."
                  hint={`default ${STRATEGY_PARAMS_DEFAULTS.bonereaper_size_longshot_usdc}.`}
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
                  label="Mid USDC"
                  tooltip="0.30 < bid ≤ 0.85 trade büyüklüğü."
                  hint={`default ${STRATEGY_PARAMS_DEFAULTS.bonereaper_size_mid_usdc}.`}
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
                  label="High-conf USDC"
                  tooltip="bid > 0.85 trade büyüklüğü."
                  hint={`default ${STRATEGY_PARAMS_DEFAULTS.bonereaper_size_high_usdc}.`}
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

              {/* RealBot v2: loser scalp + winner pyramid + LW burst + martingale-down guard */}
              <div className="mt-4 border-t border-border/40 pt-3">
                <p className="mb-2 text-xs font-medium text-muted-foreground">
                  RealBot v2 — gerçek bot davranışı
                </p>
                <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
                  <Field
                    label="Loser min bid (1¢ scalp)"
                    tooltip="Kaybeden taraf için min bid eşiği. Real bot 0.01-0.05 fiyatlarda bilet topluyor. Winner için genel min_price geçerli."
                    hint={`0.001 – 0.10 (default ${STRATEGY_PARAMS_DEFAULTS.bonereaper_loser_min_price}).`}
                  >
                    <Input
                      type="number"
                      step="0.001"
                      min="0.001"
                      max="0.1"
                      value={bonereaperLoserMinPrice}
                      onChange={(e) =>
                        patch({ bonereaper_loser_min_price: Number(e.target.value) })
                      }
                    />
                  </Field>
                  <Field
                    label="Loser scalp USDC"
                    tooltip="Kaybeden tarafa kuruşluk bilet boyutu. 0 = scalp KAPALI."
                    hint={`0 – 10 (default ${STRATEGY_PARAMS_DEFAULTS.bonereaper_loser_scalp_usdc}).`}
                  >
                    <Input
                      type="number"
                      step="0.5"
                      min="0"
                      max="10"
                      value={bonereaperLoserScalpUsdc}
                      onChange={(e) =>
                        patch({ bonereaper_loser_scalp_usdc: Number(e.target.value) })
                      }
                    />
                  </Field>
                  <Field
                    label="Loser scalp üst bid"
                    tooltip="Loser side bid bu eşiğin altında ise scalp boyutu uygulanır (longshot bucket yerine). Real bot 0.10-0.30 bandında bilet topluyor."
                    hint={`0.05 – 0.50 (default ${STRATEGY_PARAMS_DEFAULTS.bonereaper_loser_scalp_max_price}).`}
                  >
                    <Input
                      type="number"
                      step="0.01"
                      min="0.05"
                      max="0.5"
                      value={bonereaperLoserScalpMaxPrice}
                      onChange={(e) =>
                        patch({ bonereaper_loser_scalp_max_price: Number(e.target.value) })
                      }
                    />
                  </Field>
                  <Field
                    label="Late pyramid sn"
                    tooltip="T-X sn'den itibaren winner tarafa size çarpanı uygula. 0 = scaling KAPALI."
                    hint={`0 – 300 (default ${STRATEGY_PARAMS_DEFAULTS.bonereaper_late_pyramid_secs}).`}
                  >
                    <Input
                      type="number"
                      step="10"
                      min="0"
                      max="300"
                      value={bonereaperLatePyramidSecs}
                      onChange={(e) =>
                        patch({ bonereaper_late_pyramid_secs: Number(e.target.value) })
                      }
                    />
                  </Field>
                  <Field
                    label="Winner size factor"
                    tooltip="Late pyramid penceresinde winner size çarpanı."
                    hint={`1.0 – 10.0 (default ${STRATEGY_PARAMS_DEFAULTS.bonereaper_winner_size_factor}).`}
                  >
                    <Input
                      type="number"
                      step="0.5"
                      min="1"
                      max="10"
                      value={bonereaperWinnerSizeFactor}
                      onChange={(e) =>
                        patch({ bonereaper_winner_size_factor: Number(e.target.value) })
                      }
                    />
                  </Field>
                  <Field
                    label="LW burst sn"
                    tooltip="T-X kala 2. dalga LW. 0 = burst KAPALI. Ana late_winner_secs > burst > 0 olmalı."
                    hint={`0 – 60 (default ${STRATEGY_PARAMS_DEFAULTS.bonereaper_lw_burst_secs}).`}
                  >
                    <Input
                      type="number"
                      step="2"
                      min="0"
                      max="60"
                      value={bonereaperLwBurstSecs}
                      onChange={(e) =>
                        patch({ bonereaper_lw_burst_secs: Number(e.target.value) })
                      }
                    />
                  </Field>
                  <Field
                    label="LW burst USDC"
                    tooltip="Burst dalga trade büyüklüğü."
                    hint={`0 – 10000 (default ${STRATEGY_PARAMS_DEFAULTS.bonereaper_lw_burst_usdc}).`}
                  >
                    <Input
                      type="number"
                      step="50"
                      min="0"
                      max="10000"
                      value={bonereaperLwBurstUsdc}
                      onChange={(e) =>
                        patch({ bonereaper_lw_burst_usdc: Number(e.target.value) })
                      }
                    />
                  </Field>
                  <Field
                    label="Avg loser max"
                    tooltip="Loser tarafta avg fiyat bu eşiği aşarsa o yöne sadece minimal scalp ($1). Pahalı martingale-down engeli."
                    hint={`0.10 – 0.95 (default ${STRATEGY_PARAMS_DEFAULTS.bonereaper_avg_loser_max}).`}
                  >
                    <Input
                      type="number"
                      step="0.05"
                      min="0.1"
                      max="0.95"
                      value={bonereaperAvgLoserMax}
                      onChange={(e) =>
                        patch({ bonereaper_avg_loser_max: Number(e.target.value) })
                      }
                    />
                  </Field>
                </div>
              </div>
            </details>
          </div>

          <ul className="list-disc space-y-1 rounded-md border border-border/40 bg-muted/10 px-4 py-2.5 pl-7 text-xs text-muted-foreground">
            <li>
              <strong>İlk emir:</strong> <code>|up_bid - down_bid|</code> ≥{" "}
              <code>{STRATEGY_PARAMS_DEFAULTS.bonereaper_first_spread_min}</code>{" "}
              olana kadar bekler; sonra yüksek bid tarafına BUY (winner momentum).
            </li>
            <li>
              <strong>Loser scalp:</strong> Kaybeden tarafa{" "}
              <code>{STRATEGY_PARAMS_DEFAULTS.bonereaper_loser_min_price}</code>+
              fiyatlardan <code>{STRATEGY_PARAMS_DEFAULTS.bonereaper_loser_scalp_usdc}</code>$
              kuruşluk bilet topla. <code>|imbalance|</code> eşiği aşarsa weaker side rebalance.
            </li>
            <li>
              <strong>Winner pyramid:</strong> T-
              {STRATEGY_PARAMS_DEFAULTS.bonereaper_late_pyramid_secs}s'den itibaren
              winner tarafa size ×{STRATEGY_PARAMS_DEFAULTS.bonereaper_winner_size_factor}.
            </li>
            <li>
              <strong>Multi-LW:</strong> T-
              {STRATEGY_PARAMS_DEFAULTS.bonereaper_late_winner_secs}s ana dalga (
              <code>LW USDC</code>) + T-
              {STRATEGY_PARAMS_DEFAULTS.bonereaper_lw_burst_secs}s burst (
              <code>LW burst USDC</code>). Toplam{" "}
              <code>{STRATEGY_PARAMS_DEFAULTS.bonereaper_lw_max_per_session}</code> dalga.
            </li>
            <li>
              <strong>Güvenlik:</strong> <code>avg_loser_max</code> pahalı
              martingale-down'u durdurur, <code>max_avg_sum</code> pyramid'i,{" "}
              <code>cooldown</code> spam'i engeller.
            </li>
          </ul>
        </div>
      )}

      {/* ── Arbitrage parametreleri (pure cross-leg FAK BID) ──────────── */}
      {isArbitrage && (
        <div className="space-y-3">
          <SectionLabel icon={Target} title="Arbitrage parametreleri" />
          <p className="text-sm text-muted-foreground">
            Pure cross-leg sentetik dolar: <code>bid_winner + bid_loser &lt; cost_max</code>{" "}
            (avg_sum&lt;1) iken winner ve loser tarafa eşzamanlı FAK BID. Yön
            tahmini yok — matematiksel garanti.
            <br />
            <strong>Backtest (bot 108):</strong> WR <strong>%100</strong>, ROI{" "}
            <strong>+%4.35</strong>, NET +$994/12.4h ($100 order, mt=5).
          </p>
          <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
            <Field
              label="Cost max (avg_sum cap)"
              tooltip="bid_winner + bid_loser bu eşiğin altındaysa fırsat. 0.95 = sıkı %5 marj. 0.99 fee yiyor."
              hint={`0.50 – 0.999 (default ${STRATEGY_PARAMS_DEFAULTS.arbitrage_cost_max}).`}
            >
              <Input
                type="number"
                step="0.005"
                min="0.5"
                max="0.999"
                value={arbitrageCostMax}
                onChange={(e) =>
                  patch({ arbitrage_cost_max: Number(e.target.value) })
                }
              />
            </Field>
            <Field
              label="Order USDC (per leg)"
              tooltip="Winner ve loser leg ayrı ayrı bu büyüklükte. Sermaye = 2× bu değer."
              hint={`0 – 10000 (default ${STRATEGY_PARAMS_DEFAULTS.arbitrage_order_usdc}).`}
            >
              <Input
                type="number"
                step="5"
                min="0"
                max="10000"
                value={arbitrageOrderUsdc}
                onChange={(e) =>
                  patch({ arbitrage_order_usdc: Number(e.target.value) })
                }
              />
            </Field>
            <Field
              label="Max trades / session"
              tooltip="Pencere başına maksimum arbitrage trade. 0=sınırsız, 5=güvenli üst."
              hint={`0 – 20 (default ${STRATEGY_PARAMS_DEFAULTS.arbitrage_max_trades_per_session}).`}
            >
              <Input
                type="number"
                step="1"
                min="0"
                max="20"
                value={arbitrageMaxTradesPerSession}
                onChange={(e) =>
                  patch({ arbitrage_max_trades_per_session: Number(e.target.value) })
                }
              />
            </Field>
            <Field
              label="Cooldown (ms)"
              tooltip="Trade'ler arası minimum bekleme. Hızlı re-entry için 5000."
              hint={`1000 – 60000 (default ${STRATEGY_PARAMS_DEFAULTS.arbitrage_cooldown_ms}).`}
            >
              <Input
                type="number"
                step="500"
                min="1000"
                max="60000"
                value={arbitrageCooldownMs}
                onChange={(e) =>
                  patch({ arbitrage_cooldown_ms: Number(e.target.value) })
                }
              />
            </Field>
            <Field
              label="Tick interval (ms)"
              tooltip="Her N ms'de bir kontrol et. 1000=saniyede 1, 200=hızlı."
              hint={`100 – 10000 (default ${STRATEGY_PARAMS_DEFAULTS.arbitrage_tick_interval_ms}).`}
            >
              <Input
                type="number"
                step="100"
                min="100"
                max="10000"
                value={arbitrageTickIntervalMs}
                onChange={(e) =>
                  patch({ arbitrage_tick_interval_ms: Number(e.target.value) })
                }
              />
            </Field>
            <Field
              label="Entry window (sn)"
              tooltip="Pencere kapanmasına bu kadar sn kala kadar arbitrage ara. 300=tüm pencere, 60=son 1dk."
              hint={`30 – 600 (default ${STRATEGY_PARAMS_DEFAULTS.arbitrage_entry_window_secs}).`}
            >
              <Input
                type="number"
                step="10"
                min="30"
                max="600"
                value={arbitrageEntryWindowSecs}
                onChange={(e) =>
                  patch({ arbitrage_entry_window_secs: Number(e.target.value) })
                }
              />
            </Field>
          </div>
          <ul className="list-disc space-y-1 rounded-md border border-border/40 bg-muted/10 px-4 py-2.5 pl-7 text-xs text-muted-foreground">
            <li>
              <strong>Mantık:</strong> Winner side (bid&gt;0.5) ve loser side
              (bid&lt;0.5) için eşzamanlı GTC limit BUY @ bid (FAK davranışı).
            </li>
            <li>
              <strong>Garanti payoff:</strong> Kim kazanırsa $1.00. Net = $1.00 −
              cost − fee = pozitif (cost &lt; cost_max).
            </li>
            <li>
              <strong>Risk:</strong> Tek leg fill (winner_bid fill, loser_bid fill
              olmaz) → directional pozisyon. Sermaye 2× order büyüklüğü.
            </li>
          </ul>
        </div>
      )}

      {/* ── Binance Latency Arbitrage parametreleri ─────────────────────── */}
      {isBinanceLatency && (
        <div className="space-y-3">
          <SectionLabel icon={Zap} title="Binance Latency parametreleri" />
          <p className="text-sm text-muted-foreground">
            Binance Spot BTC/USDT mid fiyat lag arbitrajı. Session başında
            BTC mid snapshot, her tick <code>delta = current − open</code>;{" "}
            <code>|delta| ≥ sig_thr</code> (USD) ise BUY yönü:{" "}
            <code>delta&gt;0 → UP</code>, <code>&lt;0 → DOWN</code>.
            <br />
            <strong>Backtest (bot 91, 665 session, 64h):</strong>{" "}
            <code>sig=$50 mt=10 cd=3s</code> → WR <strong>%89</strong>, NET{" "}
            <strong>+$8 323</strong>, ROI <strong>+%4.80</strong>, yıllık
            ~<strong>$1.14M</strong>.
          </p>
          <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
            <Field
              label="Sinyal eşiği (USD)"
              tooltip="|delta| ≥ X ise trade. $50 = sweet spot (WR %89). $80 → WR %93 ama düşük frekans. $30 → WR %83."
              hint={`1 – 500 USD (default ${STRATEGY_PARAMS_DEFAULTS.binance_latency_sig_thr_usd}).`}
            >
              <Input
                type="number"
                step="5"
                min="1"
                max="500"
                value={binanceLatencySigThrUsd}
                onChange={(e) =>
                  patch({
                    binance_latency_sig_thr_usd: Number(e.target.value),
                  })
                }
              />
            </Field>
            <Field
              label="Order USDC"
              tooltip="Trade başına notional. $100 default; backtest tüm sonuçlar bu değerle."
              hint={`5 – 10000 (default ${STRATEGY_PARAMS_DEFAULTS.binance_latency_order_usdc}).`}
            >
              <Input
                type="number"
                step="10"
                min="5"
                max="10000"
                value={binanceLatencyOrderUsdc}
                onChange={(e) =>
                  patch({
                    binance_latency_order_usdc: Number(e.target.value),
                  })
                }
              />
            </Field>
            <Field
              label="Max trades / session"
              tooltip="Pencere başına max trade. 10=denge, 50=max NET (+$12808 ama düşük ROI), 3=max ROI (+%9)."
              hint={`1 – 100 (default ${STRATEGY_PARAMS_DEFAULTS.binance_latency_max_trades_per_session}).`}
            >
              <Input
                type="number"
                step="1"
                min="1"
                max="100"
                value={binanceLatencyMaxTrades}
                onChange={(e) =>
                  patch({
                    binance_latency_max_trades_per_session: Number(
                      e.target.value
                    ),
                  })
                }
              />
            </Field>
            <Field
              label="Cooldown (ms)"
              tooltip="Trade'ler arası min bekleme. 3000=optimum (frekans + sinyal kalitesi dengesi)."
              hint={`1000 – 60000 (default ${STRATEGY_PARAMS_DEFAULTS.binance_latency_cooldown_ms}).`}
            >
              <Input
                type="number"
                step="500"
                min="1000"
                max="60000"
                value={binanceLatencyCooldownMs}
                onChange={(e) =>
                  patch({
                    binance_latency_cooldown_ms: Number(e.target.value),
                  })
                }
              />
            </Field>
            <Field
              label="Entry window (sn)"
              tooltip="Pencere kapanmasına bu kadar sn kala kadar trade ara. 300=tüm 5dk pencere, 60=son 1dk."
              hint={`15 – 600 (default ${STRATEGY_PARAMS_DEFAULTS.binance_latency_entry_window_secs}).`}
            >
              <Input
                type="number"
                step="15"
                min="15"
                max="600"
                value={binanceLatencyEntryWindow}
                onChange={(e) =>
                  patch({
                    binance_latency_entry_window_secs: Number(e.target.value),
                  })
                }
              />
            </Field>
          </div>
          <ul className="list-disc space-y-1 rounded-md border border-border/40 bg-muted/10 px-4 py-2.5 pl-7 text-xs text-muted-foreground">
            <li>
              <strong>Mantık:</strong> Polymarket bid/ask Binance&apos;den birkaç
              saniye geride. Binance fiyatı sıçrarsa Polymarket henüz tepki
              vermeden BUY yapılır.
            </li>
            <li>
              <strong>3 profil:</strong>{" "}
              <code>sig=$80 mt=3</code> (max ROI %9.11) /{" "}
              <code>sig=$50 mt=10</code> (denge) /{" "}
              <code>sig=$50 mt=50</code> (max NET +$12 808).
            </li>
            <li>
              <strong>Risk:</strong> Tek-yön directional (yön yanlışsa tam
              kayıp). Sinyal kalitesi sigortadır — düşük sig_thr (&lt;$10)
              kayıp riskini büyütür.
            </li>
          </ul>
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
