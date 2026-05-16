import type { Dispatch, SetStateAction } from "react"
import { Activity, Target } from "lucide-react"
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

  const patch = (next: Partial<StrategyParams>) => {
    setForm({
      ...form,
      strategy_params: { ...params, ...next },
    })
  }

  // ── Gravie (Dual-Balance Accumulator) ─────────────────────────────────
  const gravieBuyCooldownMs =
    params.gravie_buy_cooldown_ms ??
    STRATEGY_PARAMS_DEFAULTS.gravie_buy_cooldown_ms
  const gravieAvgSumMax =
    params.gravie_avg_sum_max ?? STRATEGY_PARAMS_DEFAULTS.gravie_avg_sum_max
  const gravieMaxAsk =
    params.gravie_max_ask ?? STRATEGY_PARAMS_DEFAULTS.gravie_max_ask
  const gravieTCutoffSecs =
    params.gravie_t_cutoff_secs ?? STRATEGY_PARAMS_DEFAULTS.gravie_t_cutoff_secs
  const gravieMaxFakSize =
    params.gravie_max_fak_size ?? STRATEGY_PARAMS_DEFAULTS.gravie_max_fak_size
  const gravieImbThr =
    params.gravie_imb_thr ?? STRATEGY_PARAMS_DEFAULTS.gravie_imb_thr
  const gravieFirstBidMin =
    params.gravie_first_bid_min ?? STRATEGY_PARAMS_DEFAULTS.gravie_first_bid_min
  const gravieLoserBypassAsk =
    params.gravie_loser_bypass_ask ??
    STRATEGY_PARAMS_DEFAULTS.gravie_loser_bypass_ask
  const gravieLwBidThr =
    params.gravie_lw_bid_thr ?? STRATEGY_PARAMS_DEFAULTS.gravie_lw_bid_thr
  const gravieLwUsdcFactor =
    params.gravie_lw_usdc_factor ??
    STRATEGY_PARAMS_DEFAULTS.gravie_lw_usdc_factor
  const gravieLwMaxPerSession =
    params.gravie_lw_max_per_session ??
    STRATEGY_PARAMS_DEFAULTS.gravie_lw_max_per_session
  const gravieAvgLoserMax =
    params.gravie_avg_loser_max ?? STRATEGY_PARAMS_DEFAULTS.gravie_avg_loser_max
  const gravieLoserScalpUsdcFactor =
    params.gravie_loser_scalp_usdc_factor ??
    STRATEGY_PARAMS_DEFAULTS.gravie_loser_scalp_usdc_factor

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
  const bonereaperLwMaxPerSession =
    params.bonereaper_lw_max_per_session ??
    STRATEGY_PARAMS_DEFAULTS.bonereaper_lw_max_per_session
  const bonereaperLwCooldownMs =
    params.bonereaper_lw_cooldown_ms ??
    STRATEGY_PARAMS_DEFAULTS.bonereaper_lw_cooldown_ms
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

  return (
    <div className="space-y-3">
      {/* ── Bonereaper parametreleri ────────────────────────────────────── */}
      {isBonereaper && (
        <div className="space-y-3">
          <div>
            <SectionLabel icon={Target} title="Bonereaper parametreleri" />
            <p className="mt-1 text-sm text-muted-foreground">
              Order-book reactive martingale + fiyat-bazlı winner injection.
              Winner ask <code>$0.99</code>'a geldiği anda (zaman bağımsız)
              atlar; küçük lot × quota = toplam cap.
            </p>
          </div>

          <div className="space-y-3 rounded-md border border-border/40 bg-muted/25 p-3">
            <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
              <Field
                label="LW bid eşiği"
                tooltip="Winner bid bu değerin üstünde iken injection tetiklenir. 0.98 = winner ask tam $0.99 — gerçek bot davranışı."
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
                tooltip="Ardışık BUY emirleri arası min bekleme (LW cooldown'u bypass eder)."
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
                tooltip="Market başına maksimum injection sayısı. Toplam risk = LW USDC × bu değer. 0 = sınırsız."
                hint={`0 – 50 (default ${STRATEGY_PARAMS_DEFAULTS.bonereaper_lw_max_per_session}).`}
              >
                <Input
                  type="number"
                  step="1"
                  min="0"
                  max="50"
                  value={bonereaperLwMaxPerSession}
                  onChange={(e) =>
                    patch({ bonereaper_lw_max_per_session: Number(e.target.value) })
                  }
                />
              </Field>
              <Field
                label="LW cooldown (ms)"
                tooltip="LW shot'ları arası minimum bekleme. LW window içindeki shot sayısını belirler."
                hint={`0 – 60000 ms (default ${STRATEGY_PARAMS_DEFAULTS.bonereaper_lw_cooldown_ms}).`}
              >
                <Input
                  type="number"
                  step="1000"
                  min="0"
                  max="60000"
                  value={bonereaperLwCooldownMs}
                  onChange={(e) =>
                    patch({ bonereaper_lw_cooldown_ms: Number(e.target.value) })
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
                  tooltip="Kapanışa X sn kala LW taraması aktif olur. 300 = penceresiz (tüm market — önerilen). Fiyat bazlı tetikleyici olduğu için büyük değer her zaman iyidir."
                  hint={`0 – 300 sn (default ${STRATEGY_PARAMS_DEFAULTS.bonereaper_late_winner_secs}).`}
                >
                  <Input
                    type="number"
                    step="30"
                    min="0"
                    max="300"
                    value={bonereaperLateWinnerSecs}
                    onChange={(e) =>
                      patch({ bonereaper_late_winner_secs: Number(e.target.value) })
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
                  label="Longshot anchor (@0.30)"
                  tooltip="Lineer interp anchor: bid ≤ 0.30 sabit USDC. 5m bot ~10, 15m bot ~3."
                  hint={`anchor @ 0.30, sabit. default ${STRATEGY_PARAMS_DEFAULTS.bonereaper_size_longshot_usdc}.`}
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
                  label="Mid anchor (@0.65)"
                  tooltip="Lineer interp anchor: 0.30→0.65 longshot→mid lineer. 5m bot ~25, 15m bot ~7."
                  hint={`anchor @ 0.65, lineer (0.30→0.65). default ${STRATEGY_PARAMS_DEFAULTS.bonereaper_size_mid_usdc}.`}
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
                  label="High anchor (@lw_thr)"
                  tooltip="Lineer interp anchor: 0.65→lw_thr mid→high lineer; bid ≥ lw_thr LW devralır. 5m bot ~80, 15m bot ~20."
                  hint={`anchor @ lw_thr, lineer (0.65→lw_thr). default ${STRATEGY_PARAMS_DEFAULTS.bonereaper_size_high_usdc}.`}
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

              {/* Loser scalp + martingale-down guard */}
              <div className="mt-4 border-t border-border/40 pt-3">
                <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
                  <Field
                    label="Loser scalp üst bid"
                    tooltip="Loser bid bu eşiğin altındaysa scalp boyutu uygulanır. 0.25 = gerçek bot dağılımına uygun."
                    hint={`0.05 – 0.50 (default ${STRATEGY_PARAMS_DEFAULTS.bonereaper_loser_scalp_max_price}).`}
                  >
                    <Input
                      type="number"
                      step="0.05"
                      min="0.05"
                      max="0.5"
                      value={bonereaperLoserScalpMaxPrice}
                      onChange={(e) =>
                        patch({ bonereaper_loser_scalp_max_price: Number(e.target.value) })
                      }
                    />
                  </Field>
                  <Field
                    label="Avg loser max"
                    tooltip="Loser tarafta avg fiyat bu eşiği aşarsa o yöne sadece minimal scalp. Pahalı martingale-down engeli."
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
              <strong>LW injection (fiyat bazlı):</strong> Winner bid ≥{" "}
              <code>{STRATEGY_PARAMS_DEFAULTS.bonereaper_late_winner_bid_thr}</code> olduğunda
              zaman bağımsız olarak tetiklenir.{" "}
              <strong>LW USDC = 3 × order_usdc</strong> (otomatik),{" "}
              fiyata göre 1–5× arb_mult uygulanır.
              Gerçek bot log analizine göre LW toplam maliyetin <strong>%65'ini</strong> oluşturuyor.
            </li>
            <li>
              <strong>Loser scalp:</strong> Kaybeden tarafa{" "}
              <code>≤{STRATEGY_PARAMS_DEFAULTS.bonereaper_loser_scalp_max_price}</code>{" "}
              bandında <strong>order_usdc / 10</strong> büyüklüğünde bilet topla (lottery aspect).{" "}
              <code>|imbalance| ≥ 10 × order_usdc</code> aşarsa weaker side rebalance (otomatik).
            </li>
            <li>
              <strong>Güvenlik:</strong> <code>avg_loser_max</code> pahalı
              martingale-down'u, <code>max_avg_sum=1.0</code> simetrik pozisyonu,{" "}
              <code>cooldown</code> spam'i engeller.
            </li>
            <li>
              <strong>min/max price:</strong> <code>0.01 – 0.99</code> önerilen
              (max=0.95 LW'nin %31'ini bloklar).
            </li>
          </ul>
        </div>
      )}

      {/* ── Gravie parametreleri (Dual-Balance Accumulator) ─────────────── */}
      {isGravie && (
        <div className="space-y-3">
          <div>
            <SectionLabel icon={Activity} title="Gravie parametreleri" />
            <p className="mt-1 text-sm text-muted-foreground">
              Dual-Balance Accumulator: <code>avg_up + avg_down &lt; 1</code>{" "}
              garantisi + her iki tarafta eşit pay birikimi. Sinyal kullanmaz;
              saf order-book reaktif, BUY-only FAK taker. Asimetrik lineer size
              çarpanı: winner 0.70→4x / 1.00→10x, loser 0.30→4x / 0.00→7x.
            </p>
          </div>

          <div className="space-y-4 rounded-md border border-border/40 bg-muted/25 p-3">
            {/* Cooldown & ask tavanı */}
            <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
              <Field
                label="BUY cooldown (ms)"
                tooltip="Ardışık BUY emirleri arası minimum bekleme. Düşük = daha agresif birikim, yüksek = daha az trade."
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
              <Field
                label="Max ask fiyatı"
                tooltip="Bu fiyatın üstündeki ask'lardan BUY yapılmaz. 0.99 = neredeyse sınırsız (strateji avg_sum_max ile kendini frenler)."
                hint={`0.10 – 0.99 (default ${STRATEGY_PARAMS_DEFAULTS.gravie_max_ask}).`}
              >
                <Input
                  type="number"
                  step="0.01"
                  min="0.10"
                  max="0.99"
                  value={gravieMaxAsk}
                  onChange={(e) =>
                    patch({ gravie_max_ask: Number(e.target.value) })
                  }
                />
              </Field>
            </div>

            {/* avg_sum + T-cutoff */}
            <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
              <Field
                label="avg_sum tavanı"
                tooltip="avg_up + avg_down bu değerin üstüne çıkacaksa yeni BUY yapılmaz. Pair fiyatı 1'e yaklaşırsa arbitraj garantisi bozulur; bu guard erken durdurur."
                hint={`0.50 – 1.00 (default ${STRATEGY_PARAMS_DEFAULTS.gravie_avg_sum_max}).`}
              >
                <Input
                  type="number"
                  step="0.01"
                  min="0.50"
                  max="1.00"
                  value={gravieAvgSumMax}
                  onChange={(e) =>
                    patch({ gravie_avg_sum_max: Number(e.target.value) })
                  }
                />
              </Field>
              <Field
                label="T-cutoff (sn)"
                tooltip="Kapanışa bu kadar sn kala yeni emir verilmez. 5m market için 30 sn önerilir."
                hint={`0 – 300 sn (default ${STRATEGY_PARAMS_DEFAULTS.gravie_t_cutoff_secs}).`}
              >
                <Input
                  type="number"
                  step="5"
                  min="0"
                  max="300"
                  value={gravieTCutoffSecs}
                  onChange={(e) =>
                    patch({ gravie_t_cutoff_secs: Number(e.target.value) })
                  }
                />
              </Field>
            </div>

            {/* Rebalance + ilk giriş */}
            <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
              <Field
                label="İmbalance eşiği (share)"
                tooltip="|up_filled − down_filled| bu değeri aşarsa az olan tarafa rebalance BUY yapılır. Küçük değer = daha sık denge hamlesi."
                hint={`1 – 100 share (default ${STRATEGY_PARAMS_DEFAULTS.gravie_imb_thr}).`}
              >
                <Input
                  type="number"
                  step="1"
                  min="1"
                  max="100"
                  value={gravieImbThr}
                  onChange={(e) =>
                    patch({ gravie_imb_thr: Number(e.target.value) })
                  }
                />
              </Field>
              <Field
                label="İlk giriş min bid"
                tooltip="Winner-momentum ilk giriş: ilk işlemde kazanan tarafın bid'i bu değerin üstünde olmalı. Güçlü sinyal olmadan market'e girilmez. 0 = devre dışı (her zaman gir)."
                hint={`0.00 – 0.99 (default ${STRATEGY_PARAMS_DEFAULTS.gravie_first_bid_min}).`}
              >
                <Input
                  type="number"
                  step="0.05"
                  min="0.00"
                  max="0.99"
                  value={gravieFirstBidMin}
                  onChange={(e) =>
                    patch({ gravie_first_bid_min: Number(e.target.value) })
                  }
                />
              </Field>
            </div>

            {/* Loser bypass + scalp factor + FAK cap + avg_loser_max */}
            <div className="grid grid-cols-1 gap-3 sm:grid-cols-2 border-t border-border/30 pt-4">
              <Field
                label="Loser-scalp bypass ask"
                tooltip="ask ≤ bu değer ise avg_sum_max gate atlanır, loser-guard pas geçilir VE size scalp formülüne döner (loser-scalp factor). Bonereaper'ın loser-scalp mantığının Gravie karşılığı. 0 = bypass kapalı."
                hint={`0.00 – 0.99 (default ${STRATEGY_PARAMS_DEFAULTS.gravie_loser_bypass_ask}).`}
              >
                <Input
                  type="number"
                  step="0.05"
                  min="0.00"
                  max="0.99"
                  value={gravieLoserBypassAsk}
                  onChange={(e) =>
                    patch({ gravie_loser_bypass_ask: Number(e.target.value) })
                  }
                />
              </Field>
              <Field
                label="Loser-scalp USDC factor"
                tooltip="Bypass aktifken size = ceil(order_usdc × X / ask). Bonereaper ile birebir; küçük scalp ile loser tarafa aşırı yığılmayı önler. 0.5 × $10 = $5/shot → ask 0.20'da 25 share. 0 = scalp kapalı, size_multiplier kullanılır."
                hint={`0.00 – 5.00 (default ${STRATEGY_PARAMS_DEFAULTS.gravie_loser_scalp_usdc_factor}).`}
              >
                <Input
                  type="number"
                  step="0.1"
                  min="0"
                  max="5"
                  value={gravieLoserScalpUsdcFactor}
                  onChange={(e) =>
                    patch({
                      gravie_loser_scalp_usdc_factor: Number(e.target.value),
                    })
                  }
                />
              </Field>
              <Field
                label="Avg loser max"
                tooltip="Loser tarafta avg fiyat bu eşiği aşarsa o yöne yeni BUY yapılmaz (pahalı martingale-down koruması). Bonereaper avg_loser_max ile aynı."
                hint={`0.10 – 0.95 (default ${STRATEGY_PARAMS_DEFAULTS.gravie_avg_loser_max}).`}
              >
                <Input
                  type="number"
                  step="0.05"
                  min="0.10"
                  max="0.95"
                  value={gravieAvgLoserMax}
                  onChange={(e) =>
                    patch({ gravie_avg_loser_max: Number(e.target.value) })
                  }
                />
              </Field>
              <Field
                label="Max FAK size (share)"
                tooltip="FAK emir başına maksimum share. Düşen fiyatlarda ceil(usdc/price) patlamasını önler. Örn order_usdc=10, price=0.05 → 200 share; cap=50 ile sınırlanır. 0 = sınırsız."
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

            {/* Late Winner injection */}
            <div className="grid grid-cols-1 gap-3 sm:grid-cols-3 border-t border-border/30 pt-4">
              <Field
                label="LW bid eşiği"
                tooltip="max(up_bid, dn_bid) ≥ bu değer iken kazanan tarafa büyük taker BUY (Late Winner). Boyut: order_usdc × lw_factor × lw_mult; lw_mult ask ≥ 0.95'te lineer 2x–5x aralığında."
                hint={`0.50 – 0.99 (default ${STRATEGY_PARAMS_DEFAULTS.gravie_lw_bid_thr}).`}
              >
                <Input
                  type="number"
                  step="0.01"
                  min="0.50"
                  max="0.99"
                  value={gravieLwBidThr}
                  onChange={(e) =>
                    patch({ gravie_lw_bid_thr: Number(e.target.value) })
                  }
                />
              </Field>
              <Field
                label="LW USDC çarpanı"
                tooltip="LW notional = order_usdc × bu çarpan × lw_mult(ask). order_usdc=10 + factor=2 → ask 0.99'da 10×2×5=$100. 0 = LW KAPALI."
                hint={`0.0 – 20.0 (default ${STRATEGY_PARAMS_DEFAULTS.gravie_lw_usdc_factor}).`}
              >
                <Input
                  type="number"
                  step="0.5"
                  min="0"
                  max="20"
                  value={gravieLwUsdcFactor}
                  onChange={(e) =>
                    patch({ gravie_lw_usdc_factor: Number(e.target.value) })
                  }
                />
              </Field>
              <Field
                label="LW max / session"
                tooltip="Session başına maksimum LW shot. 0 = sınırsız (gerçek bot davranışı)."
                hint={`0 – 200 (default ${STRATEGY_PARAMS_DEFAULTS.gravie_lw_max_per_session}).`}
              >
                <Input
                  type="number"
                  step="1"
                  min="0"
                  max="200"
                  value={gravieLwMaxPerSession}
                  onChange={(e) =>
                    patch({ gravie_lw_max_per_session: Number(e.target.value) })
                  }
                />
              </Field>
            </div>
          </div>

          <div className="space-y-2 rounded-md border border-border/40 bg-muted/10 px-3 py-2.5 text-xs leading-relaxed text-muted-foreground">
            <p className="font-medium text-foreground">Gravie — nasıl çalışır?</p>
            <ul className="list-disc space-y-1 pl-4">
              <li>
                <strong>Dual-Balance:</strong> Her iki tarafı (Up + Down)
                eşit share&apos;de biriktirir;{" "}
                <code>avg_up + avg_down &lt; 1</code> sağlandığında pair
                güvenli kâr garantisi verir.
              </li>
              <li>
                <strong>Late Winner:</strong>{" "}
                <code>winner_bid ≥ lw_bid_thr</code> olduğunda kazanan tarafa
                büyük taker BUY. Boyut{" "}
                <code>order_usdc × lw_factor × lw_mult</code>; lw_mult lineer
                (ask 0.95→2x, 0.97→3.5x, 0.99→5x). Bonereaper LW karşılığı.
              </li>
              <li>
                <strong>Winner-momentum ilk giriş:</strong> İlk işlemde
                kazanan tarafın bid&apos;i <code>first_bid_min</code>{" "}
                üstünde olmalı.
              </li>
              <li>
                <strong>Asimetrik size çarpanı:</strong> Winner taraf
                0.50→2x, 0.70→4x (kırılma), 1.00→10x. Loser taraf
                0.50→2x, 0.30→4x, 0.00→7x.
              </li>
              <li>
                <strong>Loser guard + bypass:</strong> Seçilen yön zayıf
                taraftaysa <code>ask &gt; loser_bypass_ask</code> ise alma;
                <code>ask ≤ loser_bypass_ask</code> ise gate atlanır ve denge
                için ucuz tarafa BUY yapılır.
              </li>
              <li>
                <strong>Avg loser max:</strong> Loser tarafta avg fiyat
                <code>avg_loser_max</code> üstündeyse o yöne yeni BUY yok
                (pahalı martingale-down koruması).
              </li>
              <li>
                <strong>Rebalance:</strong>{" "}
                <code>|up − down| &gt; imb_thr</code> aşıldığında az olan
                tarafa BUY (fiyat fark etmez).
              </li>
              <li>
                <strong>T-cutoff + FAK cap:</strong> Kapanıştan{" "}
                <code>t_cutoff_secs</code> önce durur; düşen fiyatlarda
                share patlamasını <code>max_fak_size</code> engeller.
              </li>
            </ul>
          </div>
        </div>
      )}
    </div>
  )
}
