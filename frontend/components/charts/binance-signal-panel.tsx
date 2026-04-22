"use client";

import { useMemo } from "react";
import { Zap } from "lucide-react";
import {
  Card,
  CardContent,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import type { MarketTick, StrategyParams } from "@/lib/types";
import { STRATEGY_PARAMS_DEFAULTS } from "@/lib/types";
import { cn } from "@/lib/utils";
import {
  SECTION_LABEL_CLASS,
  SIGNAL_PAIR_CARD_CLASS,
  SIGNAL_PAIR_CONTENT_CLASS,
  SIGNAL_PAIR_HEADER_CLASS,
} from "@/lib/chart-utils";

/* ─── Signal formula (mirrors src/bot/tick.rs + src/strategy/harvest/dual.rs) ───
 *
 *   DB'deki signal_score = composite (backend hesaplar):
 *     composite = w_window × window_delta_score + (1−w_window) × binance_score
 *     (w_window = strategy_params.window_delta_weight, default 0.70)
 *
 *   OpenDual fiyatı composite'i doğrudan hedef olasılığa eşler (orderbook bağımsız):
 *     up_price   = clamp(snap(composite / 10),       MIN, MAX)
 *     down_price = clamp(snap(1 − composite / 10),   MIN, MAX)
 *
 *   composite=5 → 0.50/0.50; composite=10 → 0.95/0.05; composite=0 → 0.05/0.95.
 *
 *   Averaging size çarpanı (`HarvestContext::signal_multiplier`) da composite'e
 *   göre tier seçer; ek bir gate yok.
 */
const TICK = 0.01;
const MIN_PRICE = 0.05;
const MAX_PRICE = 0.95;
const NEUTRAL_EPS = 0.15;

const snap = (p: number) => Math.round(p / TICK) * TICK;
const clampPrice = (p: number) => Math.min(MAX_PRICE, Math.max(MIN_PRICE, snap(p)));

function dualPrices(composite: number) {
  const upRaw = Math.max(0, Math.min(1, composite / 10));
  const downRaw = 1 - upRaw;
  return {
    upBid: clampPrice(upRaw),
    downBid: clampPrice(downRaw),
    delta: (composite - 5) / 5,
  };
}

type SignalSide = "up" | "down" | "neutral";
const signalSide = (s: number): SignalSide =>
  s > 5 + NEUTRAL_EPS ? "up" : s < 5 - NEUTRAL_EPS ? "down" : "neutral";

interface Props {
  data: MarketTick[];
  strategyParams?: StrategyParams | null;
}

/* ─── Price row: market ask vs bot bid ──────────────────── */
function PriceRow({
  outcome,
  marketAsk,
  botBid,
}: {
  outcome: "UP" | "DOWN";
  marketAsk: number;
  botBid: number;
}) {
  const gap = marketAsk - botBid; // positive = bot bids below market (normal)
  const upside = outcome === "UP";
  const bidColor = upside ? "text-emerald-400" : "text-rose-400";
  const bg = upside ? "border-emerald-500/20 bg-emerald-500/5" : "border-rose-500/20 bg-rose-500/5";

  return (
    <div className={cn("rounded-md border px-2.5 py-2 space-y-1.5", bg)}>
      {/* header row */}
      <div className="flex items-center justify-between">
        <span className="text-muted-foreground font-mono text-[9px] font-semibold uppercase tracking-widest">
          {outcome}
        </span>
        <span className="text-muted-foreground/50 font-mono text-[9px]">
          piyasa ask
        </span>
      </div>

      {/* prices row */}
      <div className="flex items-end justify-between gap-2">
        {/* bot bid (signal price) */}
        <div className="flex flex-col">
          <span className="text-muted-foreground/50 font-mono text-[8px] leading-none mb-0.5">
            bot bid
          </span>
          <span className={cn("font-mono text-base font-bold tabular-nums leading-none", bidColor)}>
            {botBid.toFixed(2)}
          </span>
        </div>

        {/* gap indicator */}
        <div className="flex flex-col items-center pb-0.5">
          <span className="text-muted-foreground/40 font-mono text-[8px] leading-none mb-0.5">
            fark
          </span>
          <span className={cn(
            "font-mono text-[10px] font-semibold tabular-nums",
            gap > 0 ? "text-muted-foreground/70" : "text-amber-400",
          )}>
            {gap >= 0 ? "-" : "+"}{Math.abs(gap).toFixed(2)}
          </span>
        </div>

        {/* market ask */}
        <div className="flex flex-col items-end">
          <span className="text-muted-foreground/50 font-mono text-[8px] leading-none mb-0.5">
            ask
          </span>
          <span className="text-muted-foreground font-mono text-base font-semibold tabular-nums leading-none">
            {marketAsk > 0 ? marketAsk.toFixed(2) : "—"}
          </span>
        </div>
      </div>

      {/* gap bar */}
      {marketAsk > 0 && (
        <div className="relative h-1 rounded-full bg-white/5 overflow-hidden">
          <div
            className={cn(
              "absolute left-0 top-0 h-full rounded-full",
              gap > 0 ? "bg-white/20" : "bg-amber-400/60",
            )}
            style={{ width: `${Math.min(100, Math.abs(gap) * 200)}%` }}
          />
        </div>
      )}
    </div>
  );
}

/* ─── Panel ─────────────────────────────────────────────── */
export function BinanceSignalPanel({ data, strategyParams }: Props) {
  const sp = strategyParams ?? {};
  const rtdsEnabled = sp.rtds_enabled ?? STRATEGY_PARAMS_DEFAULTS.rtds_enabled;
  const windowWeight = sp.window_delta_weight ?? STRATEGY_PARAMS_DEFAULTS.window_delta_weight;

  const d = useMemo(() => {
    if (!data.length) return null;
    const last = data[data.length - 1]!;
    // signal_score in DB = composite (backend: w×window_delta + (1-w)×binance)
    const composite = last.signal_score;
    const { upBid, downBid, delta } = dualPrices(composite);
    const bar = (composite - 5) * 2; // [0,10] → [-10,+10] for display
    const pct = Math.max(0, Math.min(100, ((bar + 10) / 20) * 100));
    return {
      composite,
      bar,
      pct,
      side: signalSide(composite),
      upBid,
      downBid,
      delta,
      upAsk: last.up_best_ask,
      downAsk: last.down_best_ask,
    };
  }, [data]);

  const pos = d ? d.bar >= 0 : true;
  const thumbColor = pos ? "#4ade80" : "#f87171";

  return (
    <Card className={SIGNAL_PAIR_CARD_CLASS}>
      <CardHeader className={SIGNAL_PAIR_HEADER_CLASS}>
        <div className="flex min-w-0 items-center gap-1.5">
          <Zap className="text-muted-foreground size-3 shrink-0" />
          <CardTitle className={cn(SECTION_LABEL_CLASS, "normal-case tracking-[0.12em]")}>
            KOMPOZİT SİNYAL (SON)
          </CardTitle>
        </div>
        {d && (
          <span
            className={cn(
              "shrink-0 rounded border px-2 py-px font-mono text-[10px] font-bold tracking-[0.15em]",
              d.side === "up" && "border-emerald-500/40 bg-emerald-500/10 text-emerald-400",
              d.side === "down" && "border-rose-500/40 bg-rose-500/10 text-rose-400",
              d.side === "neutral" && "border-border text-muted-foreground bg-muted/30",
            )}
          >
            {d.side === "up" ? "▲ UP" : d.side === "down" ? "▼ DOWN" : "NÖTR"}
          </span>
        )}
      </CardHeader>

      <CardContent className={SIGNAL_PAIR_CONTENT_CLASS}>
        {!d ? (
          <p className="text-muted-foreground py-4 text-center font-mono text-xs">Henüz tick yok.</p>
        ) : (
          <div className="space-y-3">
            {/* Composite bar */}
            <div className="space-y-1.5">
              <div className="relative flex items-center justify-between font-mono text-[10px] font-medium">
                <span className="text-rose-400/70">-10</span>
                <span
                  className="absolute left-1/2 -translate-x-1/2 text-xs font-bold tabular-nums"
                  style={{ color: thumbColor }}
                >
                  {d.bar.toFixed(2)}
                </span>
                <span className="text-emerald-400/70">+10</span>
              </div>
              <div
                className="relative h-1.5 w-full overflow-hidden rounded-full"
                style={{
                  background:
                    "linear-gradient(to right,oklch(0.50 0.18 25/.5),oklch(0.28 0.02 260/.4) 50%,oklch(0.50 0.18 145/.5))",
                }}
              >
                <div
                  className="absolute top-1/2 h-3 w-0.5 rounded-full transition-[left] duration-300"
                  style={{
                    left: `${d.pct}%`,
                    transform: "translate(-50%,-50%)",
                    background: thumbColor,
                    boxShadow: `0 0 6px 1px ${thumbColor}70`,
                  }}
                />
              </div>
            </div>

            {/* Meta — composite + δ (OpenDual fiyatına direkt yansır) */}
            <div className="text-muted-foreground/60 flex items-center justify-between font-mono text-[9px] tabular-nums">
              <span>
                cmp{" "}
                <span className="text-muted-foreground">{d.composite.toFixed(2)}</span>
              </span>
              <span>
                δ{" "}
                <span className="text-muted-foreground">{d.delta.toFixed(2)}</span>
              </span>
            </div>

            {/* RTDS config row */}
            <div className="text-muted-foreground/50 flex items-center justify-between font-mono text-[9px] tabular-nums">
              <span>
                rtds{" "}
                <span className={rtdsEnabled ? "text-emerald-400/80" : "text-muted-foreground"}>
                  {rtdsEnabled ? "on" : "off"}
                </span>
              </span>
              <span>
                w_δ{" "}
                <span className="text-muted-foreground">{windowWeight.toFixed(2)}</span>
              </span>
              <span>
                w_bn{" "}
                <span className="text-muted-foreground">{(1 - windowWeight).toFixed(2)}</span>
              </span>
            </div>

            {/* Price rows */}
            <div className="grid grid-cols-2 gap-2">
              <PriceRow outcome="UP" marketAsk={d.upAsk} botBid={d.upBid} />
              <PriceRow outcome="DOWN" marketAsk={d.downAsk} botBid={d.downBid} />
            </div>
          </div>
        )}
      </CardContent>
    </Card>
  );
}
