"use client";

import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
} from "@/components/ui/card";
import type { BotRow } from "@/lib/types";

/** Bot ayarlarını yan yana özet kartları (bot detay / market detay sayfaları). */
export function BotSettingsCards({ bot }: { bot: BotRow }) {
  const cards: { label: string; value: string; hint?: string }[] = [
    {
      label: "Strategy",
      value: bot.strategy,
      hint: bot.run_mode === "live" ? "Live mode" : "DryRun mode",
    },
    {
      label: "Order USDC",
      value: `$${bot.order_usdc.toFixed(2)}`,
      hint: "per emir",
    },
    {
      label: "Signal weight",
      value: bot.signal_weight.toFixed(1),
      hint: bot.signal_weight === 0 ? "devre dışı" : "0–10",
    },
    {
      label: "Cooldown",
      value: `${(bot.cooldown_threshold / 1000).toFixed(0)}s`,
      hint: `${bot.cooldown_threshold} ms`,
    },
    {
      label: "Price band",
      value: `${bot.min_price.toFixed(2)} – ${bot.max_price.toFixed(2)}`,
      hint: "min – max",
    },
    {
      label: "Start offset",
      value: bot.start_offset === 0 ? "Aktif" : "Sonraki",
      hint: `offset ${bot.start_offset}`,
    },
  ];

  return (
    <div className="grid grid-cols-2 gap-3 sm:grid-cols-3 lg:grid-cols-6">
      {cards.map((c) => (
        <Card key={c.label} size="sm" className="!gap-1.5 !py-2.5">
          <CardHeader className="!px-2.5">
            <CardDescription className="text-[10px] tracking-wider uppercase">
              {c.label}
            </CardDescription>
          </CardHeader>
          <CardContent className="!px-2.5">
            <div className="bg-background/70 ring-border/40 rounded-md px-2.5 py-1.5 ring-1 ring-inset">
              <div className="text-foreground text-[10px] leading-tight font-semibold tracking-wider uppercase tabular-nums">
                {c.value}
              </div>
              {c.hint && (
                <div className="text-muted-foreground/70 mt-0.5 text-[10px] leading-tight tracking-wider uppercase">
                  {c.hint}
                </div>
              )}
            </div>
          </CardContent>
        </Card>
      ))}
    </div>
  );
}
