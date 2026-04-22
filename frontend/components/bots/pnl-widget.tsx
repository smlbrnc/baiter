import type { PnLSnapshot } from "@/lib/types";
import { cn } from "@/lib/utils";

interface Props {
  pnl: PnLSnapshot | null;
}

export function PnLWidget({ pnl }: Props) {
  if (!pnl) return null;

  return (
    <div>
      {/* Kartlar: If UP / If DOWN solda, sonra diğerleri */}
      <div className="grid grid-cols-2 gap-3 sm:grid-cols-4 lg:grid-cols-7">

        {/* If UP — yeşil aksan */}
        <PnlCard
          label="If UP"
          hint="pnl if yes wins"
          accent="up"
        >
          <MoneyVal v={pnl.pnl_if_up} />
        </PnlCard>

        {/* If DOWN — kırmızı aksan */}
        <PnlCard
          label="If DOWN"
          hint="pnl if no wins"
          accent="down"
        >
          <MoneyVal v={pnl.pnl_if_down} />
        </PnlCard>

        {/* MTM */}
        <PnlCard label="MTM" hint="mark-to-market">
          <MoneyVal v={pnl.mtm_pnl} />
        </PnlCard>

        {/* Cost */}
        <PnlCard label="Cost" hint="cost basis">
          <UsdVal v={pnl.cost_basis} />
        </PnlCard>

        {/* Shares — UP + DOWN tek kartta */}
        <PnlCard label="Shares" hint="up · down">
          <span className="font-mono text-sm font-semibold tabular-nums">
            <span className="text-emerald-500">U{pnl.up_filled.toFixed(0)}</span>
            <span className="text-border px-0.5">·</span>
            <span className="text-rose-500">D{pnl.down_filled.toFixed(0)}</span>
          </span>
        </PnlCard>

        {/* Pairs */}
        <PnlCard label="Pairs" hint="matched">
          <span className="font-mono text-sm font-semibold tabular-nums text-foreground">
            {pnl.pair_count}
          </span>
        </PnlCard>

        {/* Fee total */}
        <PnlCard label="Fee" hint="total fees">
          <UsdVal v={pnl.fee_total} neutral />
        </PnlCard>

      </div>
    </div>
  );
}

/* ── PnlCard ─────────────────────────────────────────────────────── */

function PnlCard({
  label,
  hint,
  accent,
  children,
}: {
  label: string;
  hint?: string;
  accent?: "up" | "down";
  children: React.ReactNode;
}) {
  const border =
    accent === "up"
      ? "border-emerald-500/30"
      : accent === "down"
        ? "border-rose-500/30"
        : "border-border/50";

  return (
    <div
      className={cn(
        "bg-card flex flex-col gap-2 rounded-lg border px-3 py-2.5",
        border,
      )}
    >
      <span className="text-muted-foreground text-[10px] font-medium tracking-widest uppercase">
        {label}
      </span>
      <div className="flex flex-col gap-0.5">
        {children}
        {hint && (
          <span className="text-muted-foreground/50 text-[9px] tracking-wider uppercase">
            {hint}
          </span>
        )}
      </div>
    </div>
  );
}

/* ── value helpers ───────────────────────────────────────────────── */

function MoneyVal({ v }: { v: number }) {
  const color =
    v > 0
      ? "text-emerald-500"
      : v < 0
        ? "text-rose-500"
        : "text-muted-foreground";
  return (
    <span className={cn("font-mono text-sm font-semibold tabular-nums", color)}>
      {v >= 0 ? "+" : ""}
      {v.toFixed(4)}
    </span>
  );
}

function UsdVal({ v, neutral }: { v: number; neutral?: boolean }) {
  // Negatif cost = realized cash-in (SELL > BUY notional). UI'de "−$X" göster.
  const negative = v < 0;
  return (
    <span
      className={cn(
        "font-mono text-sm font-semibold tabular-nums",
        negative
          ? "text-amber-600 dark:text-amber-400"
          : neutral
            ? "text-foreground"
            : "text-foreground",
      )}
      title={negative ? "Realized cash-in (SELL > BUY)" : undefined}
    >
      {negative ? "−" : ""}${Math.abs(v).toFixed(4)}
    </span>
  );
}
