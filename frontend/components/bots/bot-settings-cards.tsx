"use client";

import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
} from "@/components/ui/card";
import type { LucideIcon } from "lucide-react";
import {
  CircleDollarSign,
  Clock,
  Radio,
  SlidersHorizontal,
  SkipForward,
  Workflow,
} from "lucide-react";
import type { BotRow } from "@/lib/types";
import { STRATEGY_PARAMS_DEFAULTS } from "@/lib/types";

/** Bot ayarlarını yan yana özet kartları (bot detay / market detay sayfaları). */
export function BotSettingsCards({ bot }: { bot: BotRow }) {
  const sp = bot.strategy_params;
  const isBonereaper = bot.strategy === "bonereaper";

  const wMarket = sp?.bonereaper_signal_w_market
    ?? STRATEGY_PARAMS_DEFAULTS.bonereaper_signal_w_market;
  const alpha = sp?.bonereaper_signal_ema_alpha
    ?? STRATEGY_PARAMS_DEFAULTS.bonereaper_signal_ema_alpha;
  const kPersist = sp?.bonereaper_signal_persistence_k
    ?? STRATEGY_PARAMS_DEFAULTS.bonereaper_signal_persistence_k;

  const signalLabel = isBonereaper
    ? wMarket === 0
      ? `Saf Exch  α=${alpha}  K=${kPersist}`
      : `Hibrit w=${wMarket}  α=${alpha}  K=${kPersist}`
    : "CVD + OKX EMA";

  const cards: { label: string; value: string; icon: LucideIcon }[] = [
    {
      label: "Strategy",
      value: bot.strategy,
      icon: Workflow,
    },
    {
      label: "Order USDC",
      value: `$${bot.order_usdc.toFixed(2)}`,
      icon: CircleDollarSign,
    },
    {
      label: "Signal",
      value: signalLabel,
      icon: Radio,
    },
    {
      label: "Cooldown",
      value: `${(bot.cooldown_threshold / 1000).toFixed(0)}s`,
      icon: Clock,
    },
    {
      label: "Price band",
      value: `${bot.min_price.toFixed(2)} – ${bot.max_price.toFixed(2)}`,
      icon: SlidersHorizontal,
    },
    {
      label: "Start offset",
      value: bot.start_offset === 0 ? "Aktif" : "Sonraki",
      icon: SkipForward,
    },
  ];

  return (
    <div className="grid grid-cols-2 gap-3 sm:grid-cols-3 lg:grid-cols-6">
      {cards.map(({ label, value, icon: Icon }) => (
        <Card key={label} size="sm" className="!gap-2 !py-3">
          <CardHeader className="!px-2.5">
            <CardDescription className="text-muted-foreground flex items-center gap-1.5 text-[10px] tracking-wider uppercase">
              <Icon className="size-3.5 shrink-0 opacity-80" aria-hidden />
              {label}
            </CardDescription>
          </CardHeader>
          <CardContent className="!px-2.5">
            <div className="bg-background/70 ring-border/40 rounded-md px-2.5 py-2.5 ring-1 ring-inset">
              <div className="text-foreground text-[10px] leading-tight font-semibold tracking-wider uppercase tabular-nums">
                {value}
              </div>
            </div>
          </CardContent>
        </Card>
      ))}
    </div>
  );
}
