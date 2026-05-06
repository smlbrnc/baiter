import type { Dispatch, SetStateAction } from "react";
import { Sliders, Target, Zap } from "lucide-react";
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
  const isBonereaper = form.strategy === "bonereaper";

  const patch = (next: Partial<StrategyParams>) => {
    setForm({
      ...form,
      strategy_params: { ...params, ...next },
    });
  };

  // ── Alis / ortak parametreler ─────────────────────────────────────────
  const profitLockPct =
    params.profit_lock_pct ?? STRATEGY_PARAMS_DEFAULTS.profit_lock_pct;
  const openDelta = params.open_delta ?? STRATEGY_PARAMS_DEFAULTS.open_delta;
  const pyramidAggDelta =
    params.pyramid_agg_delta ?? STRATEGY_PARAMS_DEFAULTS.pyramid_agg_delta;
  const pyramidFakDelta =
    params.pyramid_fak_delta ?? STRATEGY_PARAMS_DEFAULTS.pyramid_fak_delta;
  const pyramidUsdc = params.pyramid_usdc ?? null;

  // ── Elis Dutch Book Bid Loop ──────────────────────────────────────────
  const elisMaxBuyOrderSize =
    params.elis_max_buy_order_size ?? STRATEGY_PARAMS_DEFAULTS.elis_max_buy_order_size;
  const elisTradeCooldownMs =
    params.elis_trade_cooldown_ms ?? STRATEGY_PARAMS_DEFAULTS.elis_trade_cooldown_ms;
  const elisStopBeforeEndSecs =
    params.elis_stop_before_end_secs ?? STRATEGY_PARAMS_DEFAULTS.elis_stop_before_end_secs;
  const elisMinImprovement =
    params.elis_min_improvement ?? STRATEGY_PARAMS_DEFAULTS.elis_min_improvement;
  const elisVolThreshold =
    params.elis_vol_threshold ?? STRATEGY_PARAMS_DEFAULTS.elis_vol_threshold;
  const elisBsiFilterThreshold =
    params.elis_bsi_filter_threshold ?? STRATEGY_PARAMS_DEFAULTS.elis_bsi_filter_threshold;
  const elisLockThreshold =
    params.elis_lock_threshold ?? STRATEGY_PARAMS_DEFAULTS.elis_lock_threshold;
  const elisMaxOrderAgeMs =
    params.elis_max_order_age_ms ?? STRATEGY_PARAMS_DEFAULTS.elis_max_order_age_ms;

  // ── Bonereaper ────────────────────────────────────────────────────────
  const bonereaperSignalTaker =
    params.bonereaper_signal_taker ?? STRATEGY_PARAMS_DEFAULTS.bonereaper_signal_taker;
  const bonereaperProfitLockImbalance =
    params.bonereaper_profit_lock_imbalance ??
    STRATEGY_PARAMS_DEFAULTS.bonereaper_profit_lock_imbalance;
  const bonereaperSignalPersistenceK =
    params.bonereaper_signal_persistence_k ??
    STRATEGY_PARAMS_DEFAULTS.bonereaper_signal_persistence_k;
  const bonereaperSignalWMarket =
    params.bonereaper_signal_w_market ??
    STRATEGY_PARAMS_DEFAULTS.bonereaper_signal_w_market;
  const bonereaperSignalEmaAlpha =
    params.bonereaper_signal_ema_alpha ??
    STRATEGY_PARAMS_DEFAULTS.bonereaper_signal_ema_alpha;
  const bonereaperProfitLock =
    params.bonereaper_profit_lock ??
    STRATEGY_PARAMS_DEFAULTS.bonereaper_profit_lock;

  return (
    <div className="space-y-3">

      {/* ── Elis Dutch Book Bid Loop parametreleri ───────────────────────── */}
      {isElis && (
        <div className="space-y-3">
          <div>
            <SectionLabel icon={Zap} title="Elis — Dutch Book Bid Loop" />
            <p className="text-muted-foreground mt-1 text-sm">
              Her 2 saniyede bir döngü: <code>up_bid + dn_bid &lt; $1.00</code>{" "}
              koşulunda dominant tarafa ask (taker), weaker tarafa bid (maker) emir
              verilir. Gabagool pattern'ları: P2 lock, P4 improvement, P5 filter, P6 stale.
            </p>
          </div>

          <div className="bg-muted/25 space-y-4 rounded-md border border-border/40 p-3">

            {/* ── Emir parametreleri ─── */}
            <div className="grid grid-cols-1 gap-3 sm:grid-cols-3">
              <Field
                label="Temel emir boyutu (share)"
                tooltip="Her döngüde UP ve DOWN taraflarına verilecek temel share miktarı. Önceki döngüde dolmayan emirlerin kalan miktarı bu taban üstüne eklenir (cap: base×5)."
                hint={`1 – 1000 (default ${STRATEGY_PARAMS_DEFAULTS.elis_max_buy_order_size}).`}
              >
                <Input
                  type="number" step="5" min="1" max="1000"
                  value={elisMaxBuyOrderSize}
                  onChange={(e) => patch({ elis_max_buy_order_size: Number(e.target.value) })}
                />
              </Field>
              <Field
                label="Loop süresi (ms)"
                tooltip="Emir verildikten bu süre sonra açık elis emirleri iptal edilir ve yeni döngü başlar."
                hint={`500 – 10 000 (default ${STRATEGY_PARAMS_DEFAULTS.elis_trade_cooldown_ms}).`}
              >
                <Input
                  type="number" step="500" min="500" max="10000"
                  value={elisTradeCooldownMs}
                  onChange={(e) => patch({ elis_trade_cooldown_ms: Number(e.target.value) })}
                />
              </Field>
              <Field
                label="Pencere stop (sn)"
                tooltip="Market kapanışından bu kadar saniye önce yeni emir verilmez; Done'a geçilir."
                hint={`10 – 120 (default ${STRATEGY_PARAMS_DEFAULTS.elis_stop_before_end_secs}).`}
              >
                <Input
                  type="number" step="5" min="10" max="120"
                  value={elisStopBeforeEndSecs}
                  onChange={(e) => patch({ elis_stop_before_end_secs: Number(e.target.value) })}
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
                  type="number" step="0.001" min="0.001" max="0.05"
                  value={elisMinImprovement}
                  onChange={(e) => patch({ elis_min_improvement: Number(e.target.value) })}
                />
              </Field>
              <Field
                label="P2 — Lock threshold"
                tooltip="avg_up + avg_down bu değerin altına düşünce VE min(up_filled, dn_filled) > cost_basis ise pozisyon kilitli sayılır — yeni emir verilmez (Done). Garantili kâr lock'u."
                hint={`0.85 – 0.99 (default ${STRATEGY_PARAMS_DEFAULTS.elis_lock_threshold}).`}
              >
                <Input
                  type="number" step="0.01" min="0.85" max="0.99"
                  value={elisLockThreshold}
                  onChange={(e) => patch({ elis_lock_threshold: Number(e.target.value) })}
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
                  type="number" step="0.01" min="0.01" max="0.20"
                  value={elisVolThreshold}
                  onChange={(e) => patch({ elis_vol_threshold: Number(e.target.value) })}
                />
              </Field>
              <Field
                label="P5 — BSI filter eşiği"
                tooltip="|BSI| bu eşiği aşarsa: BSI > +threshold → UP baskısı, DOWN alımı engellenir; BSI < -threshold → DOWN baskısı, UP alımı engellenir. BSI None ise filter pas geçer."
                hint={`0.10 – 1.00 (default ${STRATEGY_PARAMS_DEFAULTS.elis_bsi_filter_threshold}).`}
              >
                <Input
                  type="number" step="0.05" min="0.10" max="1.00"
                  value={elisBsiFilterThreshold}
                  onChange={(e) => patch({ elis_bsi_filter_threshold: Number(e.target.value) })}
                />
              </Field>
              <Field
                label="P6 — Stale order (ms)"
                tooltip="Bu süreden daha eski açık elis emirleri 2sn timer beklenmeden zorla iptal edilir. Ghost order birikimini önler."
                hint={`5 000 – 60 000 ms (default ${STRATEGY_PARAMS_DEFAULTS.elis_max_order_age_ms}).`}
              >
                <Input
                  type="number" step="5000" min="5000" max="60000"
                  value={elisMaxOrderAgeMs}
                  onChange={(e) => patch({ elis_max_order_age_ms: Number(e.target.value) })}
                />
              </Field>
            </div>
          </div>

          {/* Elis özet kartı */}
          <div className="space-y-2 rounded-md border border-border/40 bg-muted/10 px-3 py-2.5 text-xs leading-relaxed text-muted-foreground">
            <p className="font-medium text-foreground">Elis — nasıl çalışır?</p>
            <ul className="list-disc space-y-1 pl-4">
              <li>
                <strong>Koşul:</strong> <code>up_bid + dn_bid &lt; $1.00</code> →
                dominant taraf ask (taker), weaker taraf bid (maker) GTC emir. Fill = garantili kâr.
              </li>
              <li>
                <strong>P4 Improvement:</strong> Mevcut fill varsa yeni alım{" "}
                <code>avg pair cost</code>&apos;u <code>min_improvement</code> kadar
                düşürmedikçe emir verilmez.
              </li>
              <li>
                <strong>P5 Filters:</strong> Vol filter — spread geniş ise OB ince,
                atla. BSI filter — aşırı tek yönlü akışta karşı tarafı engelle.
              </li>
              <li>
                <strong>P2 Lock:</strong>{" "}
                <code>avg_up + avg_down &lt; lock_threshold</code> VE{" "}
                <code>pair_count &gt; cost_basis</code> → garantili kâr kilitlendi,
                Done&apos;a geç.
              </li>
              <li>
                <strong>P6 Stale:</strong> <code>max_order_age_ms</code>&apos;den
                eski emirler zorla iptal edilir (ghost order koruması).
              </li>
            </ul>
          </div>
        </div>
      )}

      {/* ── Alis / ortak profit-lock (Elis'te gizli) ─────────────────── */}
      {!isElis && (
        <div className="space-y-3">
          <div>
            <SectionLabel icon={Sliders} title="Strateji parametreleri" />
            <p className="text-muted-foreground mt-1 text-sm">
              Sinyal: Binance CVD imbalance + OKX EMA momentum (sabit, ayar gerektirmez).
            </p>
          </div>

          <div className="bg-muted/25 space-y-4 rounded-md border border-border/40 p-3">
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

      {/* ── Bonereaper parametreleri ───────────────────────────────────── */}
      {isBonereaper && (
        <div className="space-y-3">
          <div>
            <SectionLabel icon={Target} title="Bonereaper parametreleri" />
            <p className="text-muted-foreground mt-1 text-sm">
              5 dakikalık BTC-updown marketi için 1 saniyelik decision loop.
              BUY-only; çıkış REDEEM ile kapanışta gerçekleşir.
            </p>
          </div>

          <div className="bg-muted/25 space-y-4 rounded-md border border-border/40 p-3">
            <ToggleRow
              checked={bonereaperSignalTaker}
              onChange={(v) => patch({ bonereaper_signal_taker: v })}
              title="Signal — taker (ask) kullan"
              description="Açık ise her sinyal emri ask fiyatından (taker, anında fill). Kapalı ise bid fiyatından maker GTC limit emir verilir. Default: açık."
              tooltip="Taker = anında fill ama %2 fee. Maker = fee 0% ama dolma garantisi yok."
            />

            <ToggleRow
              checked={bonereaperProfitLock}
              onChange={(v) => patch({ bonereaper_profit_lock: v })}
              title="Profit Lock"
              description="Her iki tarafta da fill oluşup imbalance eşiğin altına düşünce yeni sinyal emirleri durur. Market sonuna kadar mevcut pozisyon korunur."
            />

            <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
              <Field
                label="Profit-lock imbalance (share)"
                tooltip="|UP_filled - DOWN_filled| bu değerin altında VE her iki tarafta fill varsa profit_lock devreye girer (yeni emir verilmez). Profit Lock kapalı iken kullanılmaz."
                hint={`1 – 200 share (default ${STRATEGY_PARAMS_DEFAULTS.bonereaper_profit_lock_imbalance}).`}
              >
                <Input
                  type="number"
                  step="1"
                  min="1"
                  max="200"
                  value={bonereaperProfitLockImbalance}
                  onChange={(e) =>
                    patch({ bonereaper_profit_lock_imbalance: Number(e.target.value) })
                  }
                />
              </Field>
              <Field
                label="Signal persistence K (tick)"
                tooltip="Yeni yön için kaç ardışık decision tick (1sn/tick) onayı gerekli. K=1 (default) → anlık karar, real bot uyumlu. K=2+ → flip-flop azaltır ama yön değişiminde gecikme yaratır."
                hint={`1 – 20 tick (default ${STRATEGY_PARAMS_DEFAULTS.bonereaper_signal_persistence_k}).`}
              >
                <Input
                  type="number"
                  step="1"
                  min="1"
                  max="20"
                  value={bonereaperSignalPersistenceK}
                  onChange={(e) =>
                    patch({
                      bonereaper_signal_persistence_k: Number(e.target.value),
                    })
                  }
                />
              </Field>
            </div>

            <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
              <Field
                label="Polymarket sinyal ağırlığı"
                tooltip="Yön kararında Polymarket UP_bid trend'inin Binance/OKX composite'a göre ağırlığı. Hibrit: signal×(1-w) + market×w. 0 = sadece Binance/OKX, 1 = sadece Polymarket trend, 0.7 default."
                hint={`0.0 – 1.0 (default ${STRATEGY_PARAMS_DEFAULTS.bonereaper_signal_w_market}).`}
              >
                <Input
                  type="number"
                  step="0.05"
                  min="0"
                  max="1"
                  value={bonereaperSignalWMarket}
                  onChange={(e) =>
                    patch({ bonereaper_signal_w_market: Number(e.target.value) })
                  }
                />
              </Field>
              <Field
                label="Sinyal EMA smoothing α"
                tooltip="Composite skoru EMA filtreden geçirir: ema = α×hybrid + (1-α)×prev_ema. α=1.0 (default) smoothing yok — real bot uyumlu, anlık tepki. 0.5 → daha yumuşak ama yön değişiminde gecikme."
                hint={`0.05 – 1.0 (default ${STRATEGY_PARAMS_DEFAULTS.bonereaper_signal_ema_alpha}).`}
              >
                <Input
                  type="number"
                  step="0.05"
                  min="0.05"
                  max="1"
                  value={bonereaperSignalEmaAlpha}
                  onChange={(e) =>
                    patch({ bonereaper_signal_ema_alpha: Number(e.target.value) })
                  }
                />
              </Field>
            </div>
          </div>

          <div className="space-y-2 rounded-md border border-border/40 bg-muted/10 px-3 py-2.5 text-xs leading-relaxed text-muted-foreground">
            <p className="font-medium text-foreground">Bonereaper — nasıl çalışır?</p>
            <ul className="list-disc space-y-1 pl-4">
              <li>
                <strong>1 saniyelik döngü:</strong> Her saniyede karar verilir.
              </li>
              <li>
                <strong>Sinyal yön kararı:</strong> Hibrit composite (Binance/OKX × (1−w) + Polymarket UP_bid trend × w),
                EMA smoothing, K-tick persistence. Real bot davranışıyla uyumlu (K=1, α=1.0).
              </li>
              <li>
                <strong>Dutch Book önceliği:</strong> <code>up_ask + dn_ask &lt; $1.00</code> ise
                her iki tarafa eş zamanlı taker GTC emir → garantili kâr marjı.
              </li>
              <li>
                <strong>Signal emri (dinamik size):</strong> Sinyal kuvvetine göre 2x-7x ($10-$35);
                multiplier = 2 + 5×|signal_ema|. Real bot medyan $10.54, p90 $32 ile uyumlu.
              </li>
              <li>
                <strong>avg_sum filtresi:</strong> Her iki tarafta pozisyon varken yeni emir
                <code>new_avg + opp_avg &lt; 1.25</code> kontrolü (real bot p90 ~1.20).
              </li>
              <li>
                <strong>Profit lock:</strong> Aktifken her iki tarafta fill + imbalance
                eşiğin altına düşünce sinyal emirleri durur, pozisyon korunur.
              </li>
              <li>
                <strong>Stale cancel:</strong> Açık signal emirleri bid'den 0.05'ten fazla
                saparsa iptal edilir (price drift koruması).
              </li>
            </ul>
          </div>
        </div>
      )}

    </div>
  );
}
