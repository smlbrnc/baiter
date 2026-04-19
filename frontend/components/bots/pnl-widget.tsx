import {
  Card,
  CardContent,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import type { PnLSnapshot } from "@/lib/types";
import { cn } from "@/lib/utils";

export function PnLWidget({ pnl }: { pnl: PnLSnapshot | null }) {
  if (!pnl) {
    return (
      <Card>
        <CardHeader>
          <CardTitle>PnL</CardTitle>
        </CardHeader>
        <CardContent className="text-muted-foreground text-sm">
          Henüz snapshot yok.
        </CardContent>
      </Card>
    );
  }

  return (
    <Card>
      <CardHeader>
        <CardTitle>PnL</CardTitle>
      </CardHeader>
      <CardContent className="grid grid-cols-2 gap-3 text-sm sm:grid-cols-4">
        <Metric label="mtm" value={pnl.mtm_pnl} fmt="money" />
        <Metric label="cost" value={pnl.cost_basis} fmt="money" />
        <Metric label="if UP" value={pnl.pnl_if_up} fmt="money" />
        <Metric label="if DOWN" value={pnl.pnl_if_down} fmt="money" />
        <Metric label="shares YES" value={pnl.shares_yes} />
        <Metric label="shares NO" value={pnl.shares_no} />
        <Metric label="pairs" value={pnl.pair_count} />
        <Metric label="fee" value={pnl.fee_total} fmt="money" />
        <Metric
          label="ts"
          value={new Date(pnl.ts_ms).toLocaleTimeString()}
          fmt="raw"
        />
      </CardContent>
    </Card>
  );
}

function Metric({
  label,
  value,
  fmt = "number",
}: {
  label: string;
  value: number | string;
  fmt?: "number" | "money" | "raw";
}) {
  const numeric = typeof value === "number";
  const color = cn(
    "font-mono text-sm",
    fmt === "money" && numeric
      ? value > 0
        ? "text-emerald-500"
        : value < 0
          ? "text-destructive"
          : "text-foreground"
      : "text-foreground",
  );
  const display =
    fmt === "raw"
      ? value
      : fmt === "money" && numeric
        ? (value as number).toFixed(4)
        : numeric
          ? (value as number).toFixed(4)
          : value;
  return (
    <div className="flex flex-col">
      <span className="text-muted-foreground text-[10px] tracking-wider uppercase">
        {label}
      </span>
      <span className={color}>{display}</span>
    </div>
  );
}
