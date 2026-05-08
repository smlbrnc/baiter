"""Bot 66 canvas dosyasını üret: data/bot66_analysis_summary.json -> bot66-analysis.canvas.tsx.

Canvas'ı tek `.canvas.tsx` dosyası olarak üretir. Veri inline gömülür.
"""

from __future__ import annotations

import json
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SRC = ROOT / "data" / "bot66_analysis_summary.json"
MICRO_SRC = ROOT / "data" / "bot66_micro_analysis.json"
REALIZED_SRC = ROOT / "data" / "bot66_realized_pnl.json"
TRADE_PNL_SRC = ROOT / "data" / "bot66_trade_pnl.json"
DST = Path("/Users/dorukbirinci/.cursor/projects/Users-dorukbirinci-Desktop-baiter-pro/canvases/bot66-analysis.canvas.tsx")


def slim(m: dict) -> dict:
    return {
        "slug": m["slug"],
        "title": m["title"],
        "coin": m["coin"],
        "open_ts": m["open_ts"],
        "first_ts": m["first_ts"],
        "last_ts": m["last_ts"],
        "time_from_open": m["time_from_open"],
        "time_to_close": m["time_to_close"],
        "n_trades": m["n_trades"],
        "n_up": m["n_up"],
        "n_dn": m["n_dn"],
        "up_size": m["up_size"],
        "dn_size": m["dn_size"],
        "spent": m["spent"],
        "avg_up_price": m["avg_up_price"],
        "avg_dn_price": m["avg_dn_price"],
        "sum_avg_price": m["sum_avg_price"],
        "balance": m["balance"],
        "first_side": m["first_side"],
        "min_payout": m["min_payout"],
        "max_payout": m["max_payout"],
        "pnl_min": m["pnl_min"],
        "pnl_max": m["pnl_max"],
    }


def slim_chain(chain: list[dict]) -> list[dict]:
    return [
        {
            "t": tr["ts"],
            "s": tr["outcome"][0] if tr["outcome"] else "?",
            "sz": tr["size"],
            "px": tr["price"],
            "cu": tr["cum_up"],
            "cd": tr["cum_dn"],
            "sp": tr["spent"],
        }
        for tr in chain
    ]


def main() -> None:
    raw = json.loads(SRC.read_text())
    micro = json.loads(MICRO_SRC.read_text()) if MICRO_SRC.exists() else None
    realized = None
    if REALIZED_SRC.exists():
        realized_full = json.loads(REALIZED_SRC.read_text())
        realized = {
            "method": realized_full["method"],
            "all": realized_full["all"],
            "by_dur": realized_full["by_dur"],
            "by_coin_dur": realized_full["by_coin_dur"],
            "top_wins": sorted(
                [m for m in realized_full["markets"] if m["has_redeem"] and m["pnl"] > 0],
                key=lambda x: -x["pnl"],
            )[:5],
            "top_losses": sorted(
                [m for m in realized_full["markets"] if m["has_redeem"] and m["pnl"] < 0],
                key=lambda x: x["pnl"],
            )[:5],
        }

    trade_pnl = None
    if TRADE_PNL_SRC.exists():
        tp_full = json.loads(TRADE_PNL_SRC.read_text())
        trade_pnl = {
            "method_descriptions": tp_full["method_descriptions"],
            "all": tp_full["all"],
            "by_dur": tp_full["by_dur"],
            "by_coin_dur": tp_full["by_coin_dur"],
            "top_m4_wins": sorted(
                [m for m in tp_full["markets"]],
                key=lambda x: -(x["pnl_m4_mtl"] or -1e18),
            )[:5],
            "top_m4_losses": sorted(
                [m for m in tp_full["markets"]],
                key=lambda x: (x["pnl_m4_mtl"] or 1e18),
            )[:5],
        }
    payload = {
        "overall": raw["overall"],
        "duration_summary": raw["duration_summary"],
        "coin_dur_summary": raw["coin_dur_summary"],
        "markets_5m": [slim(m) for m in raw["markets_5m"]],
        "markets_15m": [slim(m) for m in raw["markets_15m"]],
        "chains_5m": {k: slim_chain(v) for k, v in raw["chains_5m"].items()},
        "chains_15m": {k: slim_chain(v) for k, v in raw["chains_15m"].items()},
        "case_study": {
            "slug": raw["case_study"]["slug"],
            "agg": slim(raw["case_study"]["agg"]),
            "chain": slim_chain(raw["case_study"]["chain"]),
        },
        "histograms": raw["histograms"],
        "micro": micro,
        "realized": realized,
        "trade_pnl": trade_pnl,
    }

    data_literal = json.dumps(payload, separators=(",", ":"))

    tsx = '''import {
  BarChart,
  Callout,
  Card,
  CardBody,
  CardHeader,
  Code,
  Divider,
  Grid,
  H1,
  H2,
  H3,
  LineChart,
  Pill,
  Row,
  Stack,
  Stat,
  Table,
  Text,
  useCanvasState,
} from "cursor/canvas";

// ────────────────────────────────────────────────────────────────────────────
// AUTO-GENERATED data — produced by scripts/_one_off_bot66_canvas.py from
// data/bot66_analysis_summary.json. Do not edit by hand.
// ────────────────────────────────────────────────────────────────────────────

type ChainTick = { t: number; s: string; sz: number; px: number; cu: number; cd: number; sp: number };

type Market = {
  slug: string;
  title: string;
  coin: string | null;
  open_ts: number | null;
  first_ts: number;
  last_ts: number;
  time_from_open: number | null;
  time_to_close: number | null;
  n_trades: number;
  n_up: number;
  n_dn: number;
  up_size: number;
  dn_size: number;
  spent: number;
  avg_up_price: number | null;
  avg_dn_price: number | null;
  sum_avg_price: number | null;
  balance: number;
  first_side: string | null;
  min_payout: number;
  max_payout: number;
  pnl_min: number;
  pnl_max: number;
};

type DurationSummary = {
  n_markets: number;
  n_two_sided: number;
  spent_total: number;
  spent_avg: number | null;
  sum_avg_mean: number | null;
  sum_avg_median: number | null;
  balance_mean: number | null;
  first_open_mean: number | null;
  last_close_mean: number | null;
  n_trades_mean: number | null;
  pnl_min_mean: number | null;
  pnl_max_mean: number | null;
  arb_count: number;
  arb_ratio: number;
  guaranteed_profit_count: number;
  first_side_counts: { Up: number; Down: number };
};

type CoinDurRow = {
  coin: string;
  dur: string;
  n_markets: number;
  n_two_sided: number;
  n_trades_total: number;
  spent_total: number;
  spent_avg: number;
  sum_avg_mean: number;
  balance_mean: number;
  first_side_up: number;
  first_side_dn: number;
  arb_count: number;
  guaranteed_profit: number;
};

type Histograms = {
  sum_avg_bins: number[];
  sum_avg_5m: number[];
  sum_avg_15m: number[];
  balance_bins: number[];
  balance_5m: number[];
  balance_15m: number[];
  open_bins: number[];
  open_5m: number[];
  open_15m: number[];
  close_bins: number[];
  close_5m: number[];
  close_15m: number[];
};

type EntryStats = {
  n: number;
  mean: number | null;
  median: number | null;
  max?: number | null;
  p25?: number;
  p50?: number;
  p75?: number;
  p90?: number;
  p95?: number;
  p99?: number;
  histogram?: Record<string, number>;
  trades_over_0_50?: number;
  trades_over_0_70?: number;
  trades_over_0_95?: number;
};

type SizingStats = {
  n: number;
  mean: number;
  median: number;
  stdev: number;
  min: number;
  max: number;
  p10?: number;
  p25?: number;
  p50?: number;
  p75?: number;
  p90?: number;
  p95?: number;
  p99?: number;
  histogram: Record<string, number>;
};

type SecondLeg = {
  n: number;
  delay_sec: {
    mean: number;
    median: number;
    min: number;
    max: number;
    p25: number;
    p50: number;
    p75: number;
    p90: number;
    p95: number;
    histogram: Record<string, number>;
  };
  own_side_movement: {
    mean: number;
    median: number;
    p25: number;
    p50: number;
    p75: number;
    n_positive: number;
    n_negative: number;
  };
  opp_first_px: {
    mean: number;
    median: number;
    p25: number;
    p50: number;
    p75: number;
    p90: number;
  };
};

type MultiFill = {
  total_seconds_with_trades: number;
  total_trades: number;
  single_fill_seconds: number;
  multi_fill_seconds: number;
  max_fills_per_second: number;
  fills_in_multi_seconds: number;
  ratio_trades_in_multi: number;
  histogram: Record<string, number>;
};

type Rhythm = {
  n: number;
  mean: number;
  median: number;
  n_zero_seconds: number;
  n_within_1s: number;
  n_within_3s: number;
  n_over_30s: number;
  p10: number;
  p25: number;
  p50: number;
  p75: number;
  p90: number;
  p95: number;
  histogram: Record<string, number>;
};

type Cutoff = {
  n: number;
  mean: number;
  median: number;
  min: number;
  max: number;
  p10: number;
  p25: number;
  p50: number;
  p75: number;
  p90: number;
  p95: number;
  n_after_close: number;
  n_within_15s: number;
  n_within_30s: number;
  n_within_60s: number;
  n_within_90s: number;
  n_within_120s: number;
  histogram: Record<string, number>;
};

type Micro = {
  threshold: {
    first_entry_per_outcome_5m: EntryStats;
    first_entry_per_outcome_15m: EntryStats;
    all_trades_5m: EntryStats;
    all_trades_15m: EntryStats;
  };
  sizing: {
    all_5m: SizingStats;
    all_15m: SizingStats;
    per_coin: Record<string, { n: number; mean_size: number; median_size: number; max_size: number; mean_usdc: number; median_usdc: number; max_usdc: number }>;
  };
  second_leg: Record<string, SecondLeg>;
  cancel_replace_rhythm: Record<string, Rhythm>;
  multi_fill: Record<string, MultiFill>;
  t_cutoff: Record<string, Cutoff>;
};

type RealizedAgg = {
  n_markets: number;
  n_resolved: number;
  n_unresolved: number;
  spent_total: number;
  spent_resolved: number;
  redeem_resolved: number;
  pnl: number;
  roi_pct: number;
  winrate_pct: number;
  wins: number;
  losses: number;
  avg_win: number;
  avg_loss: number;
  avg_pnl_per_market: number;
  profit_factor: number | null;
};

type RealizedMarket = {
  slug: string;
  title: string;
  coin: string | null;
  dur: string | null;
  spent: number;
  up_size: number;
  dn_size: number;
  redeem: number;
  has_redeem: boolean;
  pnl: number;
  winner: string | null;
  n_trades: number;
};

type Realized = {
  method: string;
  all: RealizedAgg;
  by_dur: Record<string, RealizedAgg>;
  by_coin_dur: Record<string, RealizedAgg>;
  top_wins: RealizedMarket[];
  top_losses: RealizedMarket[];
};

type MethodAgg = {
  n: number;
  spent?: number;
  pnl?: number;
  roi_pct?: number;
  winrate_pct?: number;
  wins?: number;
  losses?: number;
  avg_win?: number;
  avg_loss?: number;
};

type TradePnLMarket = {
  slug: string;
  coin: string | null;
  dur: string | null;
  spent: number;
  up_size: number;
  dn_size: number;
  last_up_px: number;
  last_dn_px: number;
  max_up_px: number;
  max_dn_px: number;
  pnl_m1_5050: number;
  pnl_m2_lastpx: number | null;
  pnl_m2_winner: string | null;
  pnl_m3_maxpx: number | null;
  pnl_m3_winner: string | null;
  pnl_m4_mtl: number;
  pnl_m5_best: number;
  pnl_m6_worst: number;
};

type TradePnL = {
  method_descriptions: Record<string, string>;
  all: Record<string, MethodAgg>;
  by_dur: Record<string, Record<string, MethodAgg>>;
  by_coin_dur: Record<string, Record<string, MethodAgg>>;
  top_m4_wins: TradePnLMarket[];
  top_m4_losses: TradePnLMarket[];
};

type Payload = {
  overall: {
    exported_at_utc: string;
    window_start_unix: number;
    window_end_unix: number;
    wallet: string;
    pseudonym: string;
    total_trades_in_log: number;
    total_buy: number;
    total_sell: number;
    distinct_slugs: number;
    distinct_slugs_two_sided: number;
    distinct_slugs_only_up: number;
    distinct_slugs_only_dn: number;
  };
  duration_summary: Record<string, DurationSummary>;
  coin_dur_summary: CoinDurRow[];
  markets_5m: Market[];
  markets_15m: Market[];
  chains_5m: Record<string, ChainTick[]>;
  chains_15m: Record<string, ChainTick[]>;
  case_study: { slug: string; agg: Market; chain: ChainTick[] };
  histograms: Histograms;
  micro: Micro | null;
  realized: Realized | null;
  trade_pnl: TradePnL | null;
};

const DATA: Payload = __DATA_LITERAL__ as Payload;

// ────────────────────────────────────────────────────────────────────────────
// Helpers
// ────────────────────────────────────────────────────────────────────────────

function fmtUSDC(n: number | null | undefined): string {
  if (n === null || n === undefined) return "—";
  const sign = n >= 0 ? "+" : "";
  return `${sign}${n.toFixed(0)}`;
}

function fmtN(n: number | null | undefined, digits = 2): string {
  if (n === null || n === undefined) return "—";
  return n.toFixed(digits);
}

function fmtSize(n: number): string {
  return n.toFixed(0);
}

function pnlTone(pnl: number): "success" | "danger" | "warning" | undefined {
  if (pnl > 50) return "success";
  if (pnl < -100) return "danger";
  if (pnl < 0) return "warning";
  return undefined;
}

function balanceTone(b: number): "success" | "warning" | "danger" {
  if (b > 0.7) return "success";
  if (b > 0.3) return "warning";
  return "danger";
}

function sumAvgTone(s: number | null): "success" | "danger" | undefined {
  if (s === null) return undefined;
  if (s < 1.0) return "success";
  if (s > 1.1) return "danger";
  return undefined;
}

function shortSlug(slug: string): string {
  // btc-updown-5m-1778242200 → BTC 5m T8242200
  const m = slug.match(/^([a-z]+)-updown-(\\d+[hm])-(\\d+)$/);
  if (m) {
    return `${m[1].toUpperCase()} ${m[2]} ${m[3].slice(-7)}`;
  }
  return slug;
}

function localTime(unixSec: number): string {
  const d = new Date(unixSec * 1000);
  return d.toLocaleTimeString("en-US", { hour: "2-digit", minute: "2-digit", hour12: false, timeZone: "UTC" });
}

// ────────────────────────────────────────────────────────────────────────────
// Tabs
// ────────────────────────────────────────────────────────────────────────────

type TabId = "overview" | "m5" | "m15" | "case" | "hist" | "micro";
const TABS: { id: TabId; label: string }[] = [
  { id: "overview", label: "Genel Bakış" },
  { id: "m5", label: "5m Marketler" },
  { id: "m15", label: "15m Marketler" },
  { id: "case", label: "Tick-Level Vaka" },
  { id: "hist", label: "Histogramlar" },
  { id: "micro", label: "Mikro Davranış" },
];

// ────────────────────────────────────────────────────────────────────────────
// Component
// ────────────────────────────────────────────────────────────────────────────

export default function Bot66Analysis() {
  const [tab, setTab] = useCanvasState<TabId>("tab", "overview");
  const [selected5m, setSelected5m] = useCanvasState<string>("sel5m", DATA.markets_5m[0]?.slug ?? "");
  const [selected15m, setSelected15m] = useCanvasState<string>("sel15m", DATA.markets_15m[0]?.slug ?? "");

  return (
    <Stack gap={20}>
      <Header />
      <TabBar tab={tab} setTab={setTab} />
      {tab === "overview" && <OverviewTab />}
      {tab === "m5" && <MarketsTab dur="5m" markets={DATA.markets_5m} chains={DATA.chains_5m} selected={selected5m} setSelected={setSelected5m} />}
      {tab === "m15" && <MarketsTab dur="15m" markets={DATA.markets_15m} chains={DATA.chains_15m} selected={selected15m} setSelected={setSelected15m} />}
      {tab === "case" && <CaseStudyTab />}
      {tab === "hist" && <HistogramsTab />}
      {tab === "micro" && <MicroTab />}
    </Stack>
  );
}

function Header() {
  const o = DATA.overall;
  return (
    <Stack gap={8}>
      <Row gap={12} align="center">
        <H1>Bot 66 — Lively-Authenticity</H1>
        <Pill tone="neutral" size="sm">Polymarket Up-or-Down</Pill>
        <Pill tone="info" size="sm">{o.total_trades_in_log} trade · 4 gün</Pill>
      </Row>
      <Text tone="secondary" size="small">
        <Code>{o.wallet}</Code> · BTC/ETH/SOL/XRP · 5m / 15m / 1h / 4h marketler · BUY-only dual-side accumulator
      </Text>
    </Stack>
  );
}

function TabBar({ tab, setTab }: { tab: TabId; setTab: (t: TabId) => void }) {
  return (
    <Row gap={8}>
      {TABS.map((t) => (
        <Pill key={t.id} active={tab === t.id} onClick={() => setTab(t.id)}>
          {t.label}
        </Pill>
      ))}
    </Row>
  );
}

// ────────────────────────────────────────────────────────────────────────────
// Overview tab
// ────────────────────────────────────────────────────────────────────────────

function TradePnLSection({ tp }: { tp: TradePnL }) {
  const m4 = tp.all["M4_MarkToLast"];
  const m2 = tp.all["M2_LastPx_Winner"];
  const m1 = tp.all["M1_5050_EV"];
  const m3 = tp.all["M3_MaxPx_Winner"];
  const m5 = tp.all["M5_BestCase"];
  const m6 = tp.all["M6_WorstCase"];
  const fmtUSD = (n: number | undefined) => n === undefined ? "—" : `${n >= 0 ? "+" : ""}$${Math.abs(n).toLocaleString("en-US", { maximumFractionDigits: 0 })}`;
  const fmtROI = (n: number | undefined) => n === undefined ? "—" : `${n >= 0 ? "+" : ""}${n.toFixed(2)}%`;
  const fmtWR = (n: number | undefined) => n === undefined ? "—" : `${n.toFixed(1)}%`;
  const toneOf = (n: number | undefined) => n === undefined ? undefined : (n > 0 ? "success" : n < 0 ? "danger" : undefined);

  const methodRows: [string, MethodAgg][] = [
    ["M4: Mark-to-Last (MTL)", m4],
    ["M5: Best-case", m5],
    ["M2: Last-px winner", m2],
    ["M3: Max-px ≥ 0.85", m3],
    ["M1: 50/50 EV (naive)", m1],
    ["M6: Worst-case", m6],
  ];

  const durRows = (["5m", "15m", "1h", "4h"] as const).map((d) => {
    const x = tp.by_dur[d]?.["M4_MarkToLast"];
    if (!x || x.n === 0) return [d, "—", "—", "—", "—", "—"];
    return [
      d,
      String(x.n),
      `$${(x.spent ?? 0).toLocaleString("en-US", { maximumFractionDigits: 0 })}`,
      fmtUSD(x.pnl),
      fmtROI(x.roi_pct),
      `${fmtWR(x.winrate_pct)} (${x.wins ?? 0}W/${x.losses ?? 0}L)`,
    ];
  });
  const durTones = (["5m", "15m", "1h", "4h"] as const).map((d) => toneOf(tp.by_dur[d]?.["M4_MarkToLast"]?.pnl));

  const coinDurRows: [string, string, MethodAgg][] = [];
  for (const coin of ["BTC", "ETH", "SOL", "XRP"]) {
    for (const dur of ["5m", "15m"]) {
      const a = tp.by_coin_dur[`${coin}_${dur}`]?.["M4_MarkToLast"];
      if (a && a.n > 0) coinDurRows.push([coin, dur, a]);
    }
  }

  return (
    <Stack gap={16}>
      <Row gap={12} align="center">
        <H2>Trade-Based PnL — 6 Yöntem (REDEEM-bağımsız)</H2>
        <Pill tone="success" size="sm">{tp.all["M4_MarkToLast"].n} market</Pill>
      </Row>
      <Text tone="secondary" size="small">
        Sadece <Code>trades</Code> verisinden hesaplanır. Bot SELL yapmıyor; <Text weight="semibold" as="span">M4 (MTL)</Text> = pozisyonu son trade fiyatından kapatsa değeri.
        Bu, bot trade-execution-level karını gösterir.
      </Text>

      <H3>Ana KPI — M4: Mark-to-Last (önerilen)</H3>
      <Grid columns={5} gap={12}>
        <Stat value={fmtUSD(m4.pnl)} label="Net PnL (M4)" tone={toneOf(m4.pnl)} />
        <Stat value={fmtROI(m4.roi_pct)} label="ROI (M4)" tone={toneOf(m4.roi_pct)} />
        <Stat value={fmtWR(m4.winrate_pct)} label={`Winrate (${m4.wins}W/${m4.losses}L)`} tone={(m4.winrate_pct ?? 0) >= 55 ? "success" : undefined} />
        <Stat value={`$${(m4.spent ?? 0).toLocaleString("en-US", { maximumFractionDigits: 0 })}`} label="Toplam spent" />
        <Stat value={fmtUSD(m4.avg_win)} label={`Ort. kazanç`} tone="success" />
      </Grid>

      <H3>6 yöntem yan yana</H3>
      <Table
        headers={["Yöntem", "n", "PnL", "ROI", "Winrate", "Avg Win", "Avg Loss"]}
        columnAlign={["left", "right", "right", "right", "right", "right", "right"]}
        rows={methodRows.map(([name, a]) => [
          name,
          String(a.n),
          fmtUSD(a.pnl),
          fmtROI(a.roi_pct),
          fmtWR(a.winrate_pct),
          fmtUSD(a.avg_win),
          fmtUSD(a.avg_loss),
        ])}
        rowTone={methodRows.map(([_, a]) => toneOf(a.pnl))}
      />

      <H3>M4 — Süre Bazında (tüm bucket'lar pozitif)</H3>
      <Table
        headers={["Süre", "n", "Spent", "PnL (M4)", "ROI", "Winrate"]}
        columnAlign={["left", "right", "right", "right", "right", "left"]}
        rows={durRows}
        rowTone={durTones}
      />

      <H3>M4 — Coin × Süre (5m + 15m)</H3>
      <Table
        headers={["Coin", "Süre", "n", "Spent", "PnL (M4)", "ROI", "Winrate"]}
        columnAlign={["left", "left", "right", "right", "right", "right", "right"]}
        rows={coinDurRows.map(([coin, dur, a]) => [
          coin,
          dur,
          String(a.n),
          `$${(a.spent ?? 0).toLocaleString("en-US", { maximumFractionDigits: 0 })}`,
          fmtUSD(a.pnl),
          fmtROI(a.roi_pct),
          fmtWR(a.winrate_pct),
        ])}
        rowTone={coinDurRows.map(([_, __, a]) => toneOf(a.pnl))}
      />

      <Grid columns={2} gap={16}>
        <Card variant="default">
          <CardHeader trailing={<Pill tone="success" size="sm">+</Pill>}>Top 5 M4 kazanç</CardHeader>
          <CardBody>
            <Table
              headers={["Slug", "Spent", "M4 PnL", "last_up / last_dn"]}
              columnAlign={["left", "right", "right", "right"]}
              rows={tp.top_m4_wins.map((m) => [
                shortSlug(m.slug),
                `$${m.spent.toLocaleString("en-US", { maximumFractionDigits: 0 })}`,
                fmtUSD(m.pnl_m4_mtl),
                `${m.last_up_px.toFixed(2)} / ${m.last_dn_px.toFixed(2)}`,
              ])}
            />
          </CardBody>
        </Card>
        <Card variant="default">
          <CardHeader trailing={<Pill tone="warning" size="sm">−</Pill>}>Top 5 M4 kayıp</CardHeader>
          <CardBody>
            <Table
              headers={["Slug", "Spent", "M4 PnL", "last_up / last_dn"]}
              columnAlign={["left", "right", "right", "right"]}
              rows={tp.top_m4_losses.map((m) => [
                shortSlug(m.slug),
                `$${m.spent.toLocaleString("en-US", { maximumFractionDigits: 0 })}`,
                fmtUSD(m.pnl_m4_mtl),
                `${m.last_up_px.toFixed(2)} / ${m.last_dn_px.toFixed(2)}`,
              ])}
            />
          </CardBody>
        </Card>
      </Grid>

      <Callout tone="success" title="Ana sonuç">
        Trade-execution-level (M4 MTL) bot 4 günde <Text weight="semibold" as="span">{fmtUSD(m4.pnl)} ({fmtROI(m4.roi_pct)} ROI, {fmtWR(m4.winrate_pct)} winrate)</Text>.
        Tüm 4 süre bucket'ı pozitif (5m, 15m, 1h, 4h). REDEEM bazlı analiz şu anda eksik (37 market henüz redeem edilmemiş)
        — bu market'ler resolve edildikçe REDEEM toplamı M4'e yaklaşması beklenir.
      </Callout>
    </Stack>
  );
}

function RealizedSection({ r }: { r: Realized }) {
  const a = r.all;
  const pnlTone: "success" | "danger" | undefined = a.pnl > 0 ? "success" : a.pnl < 0 ? "danger" : undefined;
  const roiTone = a.roi_pct > 0 ? "success" : a.roi_pct < 0 ? "danger" : undefined;
  const winrateTone = a.winrate_pct >= 55 ? "success" : a.winrate_pct < 50 ? "warning" : undefined;
  const fmtSign = (n: number) => `${n >= 0 ? "+" : ""}${n.toFixed(2)}`;
  const fmtUSD = (n: number) => `${n >= 0 ? "+" : ""}$${Math.abs(n).toLocaleString("en-US", { maximumFractionDigits: 0 })}`;
  const durRows = (["5m", "15m", "1h", "4h"] as const).map((d) => {
    const x = r.by_dur[d];
    if (!x || x.n_resolved === 0) return [d, "—", "—", "—", "—", "—", "—"];
    return [
      d,
      `${x.n_resolved}/${x.n_markets}`,
      `$${x.spent_resolved.toLocaleString("en-US", { maximumFractionDigits: 0 })}`,
      `$${x.redeem_resolved.toLocaleString("en-US", { maximumFractionDigits: 0 })}`,
      fmtUSD(x.pnl),
      `${x.roi_pct >= 0 ? "+" : ""}${x.roi_pct.toFixed(2)}%`,
      `${x.winrate_pct.toFixed(1)}% (${x.wins}W/${x.losses}L)`,
    ];
  });
  const durTones: Array<"success" | "danger" | undefined> = (["5m", "15m", "1h", "4h"] as const).map((d) => {
    const x = r.by_dur[d];
    if (!x || x.n_resolved === 0) return undefined;
    return x.pnl > 0 ? "success" : x.pnl < 0 ? "danger" : undefined;
  });

  return (
    <Stack gap={16}>
      <Row gap={12} align="center">
        <H2>Gerçekleşmiş PnL — REDEEM tabanlı</H2>
        <Pill tone="info" size="sm">{a.n_resolved}/{a.n_markets} resolve oldu</Pill>
      </Row>
      <Text tone="secondary" size="small">
        <Code>activity[type=REDEEM].usdcSize</Code> ile market resolve sonrası nakde çevrilen miktar; PnL = redeem − spent.
      </Text>

      <Grid columns={5} gap={12}>
        <Stat value={fmtUSD(a.pnl)} label="Net PnL" tone={pnlTone} />
        <Stat value={`${fmtSign(a.roi_pct)}%`} label="ROI (resolved)" tone={roiTone} />
        <Stat value={`${a.winrate_pct.toFixed(1)}%`} label={`Winrate (${a.wins}W/${a.losses}L)`} tone={winrateTone} />
        <Stat value={`$${a.spent_resolved.toLocaleString("en-US", { maximumFractionDigits: 0 })}`} label="Toplam spent" />
        <Stat value={a.profit_factor !== null ? a.profit_factor.toFixed(3) : "—"} label="Profit factor" tone={(a.profit_factor ?? 0) >= 1 ? "success" : "danger"} />
      </Grid>

      <Grid columns={3} gap={12}>
        <Stat value={fmtUSD(a.avg_win)} label={`Ort. kazanç (n=${a.wins})`} tone="success" />
        <Stat value={fmtUSD(a.avg_loss)} label={`Ort. kayıp (n=${a.losses})`} tone="danger" />
        <Stat value={fmtUSD(a.avg_pnl_per_market)} label="Ort. PnL / market" tone={a.avg_pnl_per_market >= 0 ? "success" : "danger"} />
      </Grid>

      <H3>Süre bazında</H3>
      <Table
        headers={["Süre", "Resolve", "Spent", "Redeem", "PnL", "ROI", "Winrate"]}
        columnAlign={["left", "right", "right", "right", "right", "right", "left"]}
        rows={durRows}
        rowTone={durTones}
      />

      <Grid columns={2} gap={16}>
        <Card variant="default">
          <CardHeader trailing={<Pill tone="success" size="sm">+</Pill>}>Top 5 kazanç</CardHeader>
          <CardBody>
            <Table
              headers={["Slug", "Spent", "Redeem", "PnL"]}
              columnAlign={["left", "right", "right", "right"]}
              rows={r.top_wins.map((m) => [
                shortSlug(m.slug),
                `$${m.spent.toLocaleString("en-US", { maximumFractionDigits: 0 })}`,
                `$${m.redeem.toLocaleString("en-US", { maximumFractionDigits: 0 })}`,
                fmtUSD(m.pnl),
              ])}
            />
          </CardBody>
        </Card>
        <Card variant="default">
          <CardHeader trailing={<Pill tone="warning" size="sm">−</Pill>}>Top 5 kayıp</CardHeader>
          <CardBody>
            <Table
              headers={["Slug", "Spent", "Redeem", "PnL"]}
              columnAlign={["left", "right", "right", "right"]}
              rows={r.top_losses.map((m) => [
                shortSlug(m.slug),
                `$${m.spent.toLocaleString("en-US", { maximumFractionDigits: 0 })}`,
                `$${m.redeem.toLocaleString("en-US", { maximumFractionDigits: 0 })}`,
                fmtUSD(m.pnl),
              ])}
            />
          </CardBody>
        </Card>
      </Grid>

      <Callout tone={a.pnl >= 0 ? "success" : "warning"} title="Sentez">
        Bot 4 günde <Text weight="semibold" as="span">{fmtUSD(a.pnl)}</Text> ({fmtSign(a.roi_pct)}% ROI, {a.winrate_pct.toFixed(1)}% winrate). 5m+15m bucket'ları net kayıpta;
        1h+4h karlı — toplamı sıfıra yakın çekiyor. Avg loss ({fmtUSD(a.avg_loss)}) avg win'i ({fmtUSD(a.avg_win)}) aşıyor → asimetrik risk.
      </Callout>
    </Stack>
  );
}

function OverviewTab() {
  const o = DATA.overall;
  const ds = DATA.duration_summary;
  const r = DATA.realized;
  const tp = DATA.trade_pnl;
  const dualPct = Math.round((o.distinct_slugs_two_sided / o.distinct_slugs) * 100);

  return (
    <Stack gap={20}>
      {tp && <TradePnLSection tp={tp} />}

      <Divider />

      {r && <RealizedSection r={r} />}

      <Divider />

      <H2>Davranış Sayaçları</H2>
      <Grid columns={4} gap={12}>
        <Stat value={String(o.total_trades_in_log)} label="Toplam Trade" />
        <Stat value="100%" label="BUY oranı" tone="info" />
        <Stat value={`${dualPct}%`} label="Dual-side market" tone="success" />
        <Stat value={String(o.distinct_slugs)} label="Distinct market" />
      </Grid>

      <Divider />

      <H2>Süre Bucket Karşılaştırması</H2>
      <Text tone="secondary" size="small">
        İki taraflı market'lerde ortalama metrikler. <Code>sum_avg_price</Code> &lt; 1.0 → arbitraj fiyatlama;
        <Code>balance</Code> = min(up_size, dn_size) / max(...).
      </Text>
      <Table
        headers={["Süre", "Market", "İki taraflı", "Toplam spent", "Ort. spent", "sum_avg ort.", "Balance ort.", "Açılıştan +sn", "Kapanıştan -sn", "Trade ort.", "Arbitraj", "Garanti karlı", "İlk Up", "İlk Down"]}
        columnAlign={["left", "right", "right", "right", "right", "right", "right", "right", "right", "right", "right", "right", "right", "right"]}
        rows={(["5m", "15m", "1h", "4h"] as const).map((dur) => {
          const s = ds[dur];
          if (!s) return [dur, "—", "—", "—", "—", "—", "—", "—", "—", "—", "—", "—", "—", "—"];
          return [
            dur,
            String(s.n_markets),
            String(s.n_two_sided),
            fmtUSDC(s.spent_total),
            fmtUSDC(s.spent_avg),
            fmtN(s.sum_avg_mean, 4),
            fmtN(s.balance_mean, 3),
            s.first_open_mean !== null ? `${Math.round(s.first_open_mean)}` : "—",
            s.last_close_mean !== null ? `${Math.round(s.last_close_mean)}` : "—",
            fmtN(s.n_trades_mean, 1),
            `${s.arb_count}/${s.n_two_sided}`,
            `${s.guaranteed_profit_count}/${s.n_two_sided}`,
            String(s.first_side_counts.Up),
            String(s.first_side_counts.Down),
          ];
        })}
      />

      <Divider />

      <H2>Coin × Süre Matrisi (5m & 15m)</H2>
      <Text tone="secondary" size="small">
        Bot'un sermaye dağılımı ve davranışsal asimetrisi: 5m'de ezici çoğunlukla <Pill tone="warning" size="sm">Down</Pill> ile başlar, 15m'de <Pill tone="success" size="sm">Up</Pill> ile.
      </Text>
      <Table
        headers={["Coin", "Süre", "Market", "Trade", "Toplam spent", "Ort. spent", "sum_avg ort.", "Balance ort.", "İlk Up", "İlk Down", "Arbitraj"]}
        columnAlign={["left", "left", "right", "right", "right", "right", "right", "right", "right", "right", "right"]}
        rows={DATA.coin_dur_summary
          .filter((r) => r.dur === "5m" || r.dur === "15m")
          .sort((a, b) => (a.coin === b.coin ? a.dur.localeCompare(b.dur) : a.coin.localeCompare(b.coin)))
          .map((r) => [
            r.coin,
            r.dur,
            String(r.n_markets),
            String(r.n_trades_total),
            fmtUSDC(r.spent_total),
            fmtUSDC(r.spent_avg),
            fmtN(r.sum_avg_mean, 4),
            fmtN(r.balance_mean, 3),
            String(r.first_side_up),
            String(r.first_side_dn),
            `${r.arb_count}/${r.n_two_sided}`,
          ])}
      />

      <Divider />

      <H2>Strateji Özeti</H2>
      <Grid columns={2} gap={16}>
        <Card variant="default">
          <CardHeader>Davranış parmak izi</CardHeader>
          <CardBody>
            <Stack gap={8}>
              <Row gap={6} align="center"><Pill tone="info" size="sm">BUY-only</Pill><Text size="small">SELL yok, market resolve'a kadar tutar</Text></Row>
              <Row gap={6} align="center"><Pill tone="success" size="sm">Dual-side</Pill><Text size="small">{o.distinct_slugs_two_sided}/{o.distinct_slugs} market'te hem Up hem Down</Text></Row>
              <Row gap={6} align="center"><Pill tone="warning" size="sm">Reaktif</Pill><Text size="small">Hangi outcome ucuzsa onu BUY eder</Text></Row>
              <Row gap={6} align="center"><Pill tone="neutral" size="sm">Zamanlama</Pill><Text size="small">5m: T+32 → T-89, 15m: T+62 → T-198 sn</Text></Row>
            </Stack>
          </CardBody>
        </Card>
        <Card variant="default">
          <CardHeader>Önemli bulgular</CardHeader>
          <CardBody>
            <Stack gap={8}>
              <Text size="small"><Text weight="semibold">5m bucket sum_avg ortalama 1.05</Text> — hafif kayıp eğilimi (arbitraj %26).</Text>
              <Text size="small"><Text weight="semibold">15m bucket sum_avg ortalama 1.00</Text> — neredeyse arbitraj eşiğinde (arbitraj %44).</Text>
              <Text size="small"><Text weight="semibold">5m'de Down ile başlama oranı %78</Text> (18/23), 15m'de Up ile %69 (42/61).</Text>
              <Text size="small"><Text weight="semibold">BTC ana hedef.</Text> 15m'de ortalama 3 652 USDC harcama; SOL/XRP 5m'de minimal sermaye.</Text>
            </Stack>
          </CardBody>
        </Card>
      </Grid>

      <Callout tone="info" title="Sınırlar">
        Log sadece <Code>side</Code>, <Code>outcome</Code>, <Code>size</Code>, <Code>price</Code>, <Code>timestamp</Code> içeriyor.
        Order book (best bid/ask) yok; "passive maker mı taker mı" ve "fiyat seçim sinyali" bu veri ile cevaplanamaz.
        Ham trade davranışı net; alt-katman karar mekanizması <Code>data/baiter.db</Code> tick verisi gerektirir.
      </Callout>
    </Stack>
  );
}

// ────────────────────────────────────────────────────────────────────────────
// Markets tab (5m / 15m)
// ────────────────────────────────────────────────────────────────────────────

function MarketsTab({
  dur,
  markets,
  chains,
  selected,
  setSelected,
}: {
  dur: string;
  markets: Market[];
  chains: Record<string, ChainTick[]>;
  selected: string;
  setSelected: (s: string) => void;
}) {
  const sorted = [...markets].sort((a, b) => (b.spent ?? 0) - (a.spent ?? 0));
  const sel = markets.find((m) => m.slug === selected) ?? markets[0];
  const chain = sel ? chains[sel.slug] ?? [] : [];

  return (
    <Stack gap={20}>
      <Row gap={12} align="center">
        <H2>{dur} Marketler ({markets.length})</H2>
        <Text tone="secondary" size="small">
          Spent'e göre sıralı. Bir satıra tıklayarak alt panelde tick-level zaman serisini gör.
        </Text>
      </Row>

      <Table
        headers={["Slug", "Açılış", "Coin", "İlk", "Trade", "Spent", "Up sz", "Dn sz", "Bal", "sumAvg", "PnL min", "PnL max"]}
        columnAlign={["left", "left", "left", "center", "right", "right", "right", "right", "right", "right", "right", "right"]}
        rows={sorted.map((m) => [
          <Pill
            key={m.slug}
            active={m.slug === sel?.slug}
            size="sm"
            tone={m.slug === sel?.slug ? "info" : "neutral"}
            onClick={() => setSelected(m.slug)}
          >
            {shortSlug(m.slug)}
          </Pill>,
          m.open_ts ? localTime(m.open_ts) : "—",
          m.coin ?? "—",
          m.first_side === "Up" ? "↑" : m.first_side === "Down" ? "↓" : "—",
          String(m.n_trades),
          fmtUSDC(m.spent),
          fmtSize(m.up_size),
          fmtSize(m.dn_size),
          fmtN(m.balance, 2),
          fmtN(m.sum_avg_price, 4),
          fmtUSDC(m.pnl_min),
          fmtUSDC(m.pnl_max),
        ])}
        rowTone={sorted.map((m) => pnlTone(m.pnl_min))}
        striped
        stickyHeader
        style={{ maxHeight: 360 }}
      />

      {sel && (
        <Card variant="default">
          <CardHeader trailing={<Pill tone="neutral" size="sm">{sel.n_trades} trade</Pill>}>
            {sel.title}
          </CardHeader>
          <CardBody>
            <Stack gap={16}>
              <Grid columns={6} gap={12}>
                <Stat label="Spent" value={fmtUSDC(sel.spent)} />
                <Stat label="Up sz" value={fmtSize(sel.up_size)} />
                <Stat label="Dn sz" value={fmtSize(sel.dn_size)} />
                <Stat label="Balance" value={fmtN(sel.balance, 2)} tone={balanceTone(sel.balance)} />
                <Stat label="sum_avg" value={fmtN(sel.sum_avg_price, 4)} tone={sumAvgTone(sel.sum_avg_price)} />
                <Stat label="PnL [min, max]" value={`${fmtUSDC(sel.pnl_min)} … ${fmtUSDC(sel.pnl_max)}`} tone={pnlTone(sel.pnl_min)} />
              </Grid>
              <ChainChart chain={chain} />
              <ChainPriceChart chain={chain} />
            </Stack>
          </CardBody>
        </Card>
      )}
    </Stack>
  );
}

function ChainChart({ chain }: { chain: ChainTick[] }) {
  if (chain.length === 0) {
    return <Text tone="tertiary" size="small">Tick zinciri yok.</Text>;
  }
  // X axis: seconds from first trade
  const t0 = chain[0].t;
  // Show every Nth point for readability if too many
  const step = Math.max(1, Math.floor(chain.length / 60));
  const sampled = chain.filter((_, i) => i % step === 0 || i === chain.length - 1);
  const cats = sampled.map((c) => `+${c.t - t0}s`);
  const upSeries = sampled.map((c) => c.cu);
  const dnSeries = sampled.map((c) => c.cd);
  const spSeries = sampled.map((c) => c.sp);
  return (
    <Stack gap={8}>
      <H3>Kümülatif Pozisyon ve Harcama</H3>
      <LineChart
        categories={cats}
        series={[
          { name: "Cum Up size", data: upSeries, tone: "success" },
          { name: "Cum Dn size", data: dnSeries, tone: "danger" },
          { name: "Cum Spent (USDC)", data: spSeries, tone: "info" },
        ]}
        height={240}
        fill
      />
    </Stack>
  );
}

function ChainPriceChart({ chain }: { chain: ChainTick[] }) {
  if (chain.length === 0) return null;
  const t0 = chain[0].t;
  const step = Math.max(1, Math.floor(chain.length / 60));
  const sampled = chain.filter((_, i) => i % step === 0 || i === chain.length - 1);
  const cats = sampled.map((c) => `+${c.t - t0}s`);
  // Separate Up vs Dn px paths
  const upPx = sampled.map((c) => (c.s === "U" ? c.px : NaN));
  const dnPx = sampled.map((c) => (c.s === "D" ? c.px : NaN));
  // Replace NaN with last known value to keep line continuous
  function ffill(arr: number[]): number[] {
    let last = 0;
    return arr.map((v) => {
      if (!Number.isNaN(v)) {
        last = v;
        return v;
      }
      return last;
    });
  }
  return (
    <Stack gap={8}>
      <H3>Trade fiyatları (Up alımları yeşil, Down alımları kırmızı)</H3>
      <LineChart
        categories={cats}
        series={[
          { name: "Up BUY @", data: ffill(upPx), tone: "success" },
          { name: "Dn BUY @", data: ffill(dnPx), tone: "danger" },
        ]}
        height={200}
        valueSuffix=""
      />
    </Stack>
  );
}

// ────────────────────────────────────────────────────────────────────────────
// Case study tab
// ────────────────────────────────────────────────────────────────────────────

function CaseStudyTab() {
  const cs = DATA.case_study;
  const m = cs.agg;
  const chain = cs.chain;
  const t0 = chain[0]?.t ?? 0;

  // tick chart data - all 61 trades
  const cats = chain.map((c) => `+${c.t - t0}s`);
  const upSeries = chain.map((c) => c.cu);
  const dnSeries = chain.map((c) => c.cd);
  const spSeries = chain.map((c) => c.sp);

  // segment-by-side breakdown
  const upPx: number[] = [];
  const dnPx: number[] = [];
  let lastUp = 0;
  let lastDn = 0;
  for (const c of chain) {
    if (c.s === "U") {
      lastUp = c.px;
    } else if (c.s === "D") {
      lastDn = c.px;
    }
    upPx.push(lastUp);
    dnPx.push(lastDn);
  }

  return (
    <Stack gap={20}>
      <Stack gap={4}>
        <H2>{m.title}</H2>
        <Text tone="secondary" size="small">
          <Code>{m.slug}</Code> · Açılış: {localTime(m.open_ts!)}, Kapanış: +5dk · {m.n_trades} trade
        </Text>
      </Stack>

      <Grid columns={6} gap={12}>
        <Stat label="Spent" value={fmtUSDC(m.spent)} />
        <Stat label="Up sz" value={fmtSize(m.up_size)} tone="success" />
        <Stat label="Dn sz" value={fmtSize(m.dn_size)} tone="danger" />
        <Stat label="Balance" value={fmtN(m.balance, 2)} tone={balanceTone(m.balance)} />
        <Stat label="sum_avg" value={fmtN(m.sum_avg_price, 4)} tone={sumAvgTone(m.sum_avg_price)} />
        <Stat label="PnL [min, max]" value={`${fmtUSDC(m.pnl_min)} … ${fmtUSDC(m.pnl_max)}`} tone={pnlTone(m.pnl_min)} />
      </Grid>

      <H3>Kümülatif pozisyon (saniye bazında)</H3>
      <LineChart
        categories={cats}
        series={[
          { name: "Cum Up size", data: upSeries, tone: "success" },
          { name: "Cum Dn size", data: dnSeries, tone: "danger" },
          { name: "Cum Spent (USDC)", data: spSeries, tone: "info" },
        ]}
        height={280}
        fill
      />

      <H3>Trade fiyatları (forward-fill, son bilinen Up & Down BUY fiyatı)</H3>
      <LineChart
        categories={cats}
        series={[
          { name: "Up BUY @", data: upPx, tone: "success" },
          { name: "Dn BUY @", data: dnPx, tone: "danger" },
        ]}
        height={220}
      />

      <Divider />

      <H3>Segmentasyon — Faz Faz Davranış</H3>
      <Table
        headers={["Faz", "Saniye aralığı", "Davranış"]}
        rows={[
          ["A", "0–23 sn", "Sadece Down BUY (px 0.45 → 0.83). Bot Down ucuz iken topluyor; fiyatı kendi yukarı itiyor olabilir."],
          ["B", "23–80 sn", "Down @ 0.85 oldu, Up @ 0.12'ye düştü → ani Up BUY başlangıcı. Karşı taraf ucuzlama tetiklemesi."],
          ["C", "80–170 sn", "Up @ 0.30 ucuz pencerede büyük blok birikim (≈ 234 share); arada Down @ 0.83 devam ediyor."],
          ["D", "170–250 sn", "Down momentum'u devam etti, bot büyük Down blokları aldı (~654 share, 0.26–0.81)."],
          ["E", "250–265 sn", "Son ~15 sn'de Up patlaması (245 share, 0.26–0.71). T-16 sn'de tüm emirler durdu."],
        ]}
        columnAlign={["center", "left", "left"]}
        striped
      />

      <Callout tone="warning" title="Sonuç">
        Bot Down tarafına ağırlık verdi (1475 vs 1100 share). Resolve Up gelirse −234 USDC, Down gelirse +141 USDC.
        Balance 0.75 → mükemmel hedge değil; son 60 sn'deki büyük Down blokları bu meyilin sebebi.
      </Callout>
    </Stack>
  );
}

// ────────────────────────────────────────────────────────────────────────────
// Histograms tab
// ────────────────────────────────────────────────────────────────────────────

function HistogramsTab() {
  const h = DATA.histograms;
  return (
    <Stack gap={24}>
      <H2>Hedge Davranış Dağılımları</H2>
      <Text tone="secondary" size="small">
        5m vs 15m bucket karşılaştırması: aynı stratejinin farklı zaman pencerelerinde nasıl davrandığını gösterir.
      </Text>

      <Stack gap={8}>
        <H3>sum_avg_price dağılımı (1.0'dan küçük → arbitraj)</H3>
        <BarChart
          categories={h.sum_avg_bins.slice(0, -1).map((v, i) => `${v.toFixed(2)}–${h.sum_avg_bins[i + 1].toFixed(2)}`)}
          series={[
            { name: "5m", data: h.sum_avg_5m, tone: "info" },
            { name: "15m", data: h.sum_avg_15m, tone: "warning" },
          ]}
          height={220}
        />
      </Stack>

      <Stack gap={8}>
        <H3>Balance dağılımı (1.0 = mükemmel hedge)</H3>
        <BarChart
          categories={h.balance_bins.slice(0, -1).map((v, i) => `${v.toFixed(2)}–${h.balance_bins[i + 1].toFixed(2)}`)}
          series={[
            { name: "5m", data: h.balance_5m, tone: "info" },
            { name: "15m", data: h.balance_15m, tone: "warning" },
          ]}
          height={220}
        />
      </Stack>

      <Grid columns={2} gap={16}>
        <Stack gap={8}>
          <H3>İlk trade — açılıştan +sn</H3>
          <BarChart
            categories={h.open_bins.slice(0, -1).map((v, i) => `${v}–${h.open_bins[i + 1]}`)}
            series={[
              { name: "5m", data: h.open_5m, tone: "info" },
              { name: "15m", data: h.open_15m, tone: "warning" },
            ]}
            height={200}
          />
          <Text size="small" tone="secondary">
            5m'de market açıldıktan ortalama 32 sn sonra ilk trade. 15m'de 62 sn.
          </Text>
        </Stack>
        <Stack gap={8}>
          <H3>Son trade — kapanıştan -sn</H3>
          <BarChart
            categories={h.close_bins.slice(0, -1).map((v, i) => `${v}–${h.close_bins[i + 1]}`)}
            series={[
              { name: "5m", data: h.close_5m, tone: "info" },
              { name: "15m", data: h.close_15m, tone: "warning" },
            ]}
            height={200}
          />
          <Text size="small" tone="secondary">
            5m'de kapanıştan 89 sn önce son trade. 15m'de 198 sn.
          </Text>
        </Stack>
      </Grid>
    </Stack>
  );
}

// ────────────────────────────────────────────────────────────────────────────
// Micro behavior tab
// ────────────────────────────────────────────────────────────────────────────

function histToBarChart(h: Record<string, number>): { cats: string[]; data: number[] } {
  const cats = Object.keys(h);
  const data = cats.map((k) => h[k]);
  return { cats, data };
}

function MicroTab() {
  if (!DATA.micro) {
    return <Text tone="secondary">Mikro analiz verisi yok. <Code>scripts/_one_off_bot66_micro.py</Code> çalıştırın.</Text>;
  }
  const m = DATA.micro;
  return (
    <Stack gap={24}>
      <Stack gap={4}>
        <H2>Mikro Davranış Sondajı — 6 Kriter</H2>
        <Text tone="secondary" size="small">
          5m + 15m bucket'lardaki 2 918 trade üzerinde derin analiz. Eşik / sizing / second-leg / ritim / multi-fill / T-cutoff.
        </Text>
      </Stack>

      <ThresholdCard data={m.threshold} />
      <SizingCard data={m.sizing} />
      <SecondLegCard data={m.second_leg} />
      <RhythmCard data={m.cancel_replace_rhythm} />
      <MultiFillCard data={m.multi_fill} />
      <CutoffCard data={m.t_cutoff} />

      <Divider />
      <Callout tone="success" title="Sentez">
        Bot mid-price (~0.50) civarında dual-side, taker (FAK) emirlerle her 4-5 sn'de küçük (25-30 share)
        ya da arada bir devasa ({"≥"}500 share) blok atar; ilk leg dolduğunda ortalama 38-110 sn beklediği
        bir guard ile karşı tarafa geçer; <Text weight="semibold" as="span">5m'de T-90 sn'de keser</Text>,
        15m'de cutoff dinamiktir.
      </Callout>
    </Stack>
  );
}

function ThresholdCard({ data }: { data: Micro["threshold"] }) {
  const e5 = data.first_entry_per_outcome_5m;
  const e15 = data.first_entry_per_outcome_15m;
  const a5 = data.all_trades_5m;
  const a15 = data.all_trades_15m;
  return (
    <Card variant="default">
      <CardHeader trailing={<Pill tone="info" size="sm">Kriter 1</Pill>}>
        Eşik değeri — entry için fiyat tavanı
      </CardHeader>
      <CardBody>
        <Stack gap={12}>
          <Text size="small" tone="secondary">
            Bot ilk entry'lerinde hangi fiyat aralığını seçiyor? Sert bir bid+spread tavanı var mı?
          </Text>
          <Grid columns={4} gap={12}>
            <Stat label="5m ilk entry medyan" value={fmtN(e5.median, 3)} />
            <Stat label="5m ilk entry p95" value={fmtN(e5.p95 ?? 0, 3)} />
            <Stat label="15m ilk entry medyan" value={fmtN(e15.median, 3)} />
            <Stat label="15m ilk entry p95" value={fmtN(e15.p95 ?? 0, 3)} />
          </Grid>
          <Table
            headers={["Bucket", "n", "median", "p75", "p90", "p95", "p99", "max"]}
            columnAlign={["left", "right", "right", "right", "right", "right", "right", "right"]}
            rows={[
              ["5m ilk entry / outcome", String(e5.n), fmtN(e5.median, 3), fmtN(e5.p75 ?? 0, 3), fmtN(e5.p90 ?? 0, 3), fmtN(e5.p95 ?? 0, 3), fmtN(e5.p99 ?? 0, 3), fmtN(e5.max ?? 0, 3)],
              ["15m ilk entry / outcome", String(e15.n), fmtN(e15.median, 3), fmtN(e15.p75 ?? 0, 3), fmtN(e15.p90 ?? 0, 3), fmtN(e15.p95 ?? 0, 3), fmtN(e15.p99 ?? 0, 3), fmtN(e15.max ?? 0, 3)],
              ["5m tüm trade'ler", String(a5.n), fmtN(a5.median, 3), fmtN(a5.p75 ?? 0, 3), fmtN(a5.p90 ?? 0, 3), fmtN(a5.p95 ?? 0, 3), fmtN(a5.p99 ?? 0, 3), "—"],
              ["15m tüm trade'ler", String(a15.n), fmtN(a15.median, 3), fmtN(a15.p75 ?? 0, 3), fmtN(a15.p90 ?? 0, 3), fmtN(a15.p95 ?? 0, 3), fmtN(a15.p99 ?? 0, 3), "—"],
            ]}
          />
          <H3>İlk entry fiyat dağılımı</H3>
          {(() => {
            const h5 = histToBarChart(e5.histogram ?? {});
            const h15 = histToBarChart(e15.histogram ?? {});
            return (
              <BarChart
                categories={h5.cats}
                series={[
                  { name: "5m", data: h5.data, tone: "info" },
                  { name: "15m", data: h15.data, tone: "warning" },
                ]}
                height={200}
              />
            );
          })()}
          <Text size="small" tone="secondary">
            <Text weight="semibold" as="span">Yorum:</Text> İlk entry'ler median 0.50 civarında — favori henüz oluşmamışken bot devreye giriyor.
            Sert ceiling yok ama p95 ≈ 0.78. Üst-uçlardaki alımlar (≥0.95) yalnızca {a5.trades_over_0_95 ?? 0}+{a15.trades_over_0_95 ?? 0} trade — büyük ihtimalle hedge tamamlama amaçlı.
          </Text>
        </Stack>
      </CardBody>
    </Card>
  );
}

function SizingCard({ data }: { data: Micro["sizing"] }) {
  const s5 = data.all_5m;
  const s15 = data.all_15m;
  return (
    <Card variant="default">
      <CardHeader trailing={<Pill tone="info" size="sm">Kriter 2</Pill>}>
        Sizing fonksiyonu — depth'in yüzde kaçı?
      </CardHeader>
      <CardBody>
        <Stack gap={12}>
          <Text size="small" tone="secondary">
            Trade size (share) dağılımı. Sabit boyut yok; küçük taban + büyük blok burst pattern.
          </Text>
          <Grid columns={6} gap={12}>
            <Stat label="5m median" value={fmtN(s5.median, 1)} />
            <Stat label="5m p90" value={fmtN(s5.p90 ?? 0, 1)} />
            <Stat label="5m max" value={fmtN(s5.max, 0)} tone="warning" />
            <Stat label="15m median" value={fmtN(s15.median, 1)} />
            <Stat label="15m p90" value={fmtN(s15.p90 ?? 0, 1)} />
            <Stat label="15m max" value={fmtN(s15.max, 0)} tone="warning" />
          </Grid>
          <H3>Coin × tipik trade büyüklüğü</H3>
          <Table
            headers={["Coin", "n", "median size", "mean size", "max size", "median USDC", "mean USDC", "max USDC"]}
            columnAlign={["left", "right", "right", "right", "right", "right", "right", "right"]}
            rows={Object.entries(data.per_coin)
              .sort((a, b) => b[1].n - a[1].n)
              .map(([coin, s]) => [coin, String(s.n), fmtN(s.median_size, 1), fmtN(s.mean_size, 1), fmtN(s.max_size, 0), fmtN(s.median_usdc, 1), fmtN(s.mean_usdc, 1), fmtN(s.max_usdc, 0)])}
          />
          <H3>Size dağılımı</H3>
          {(() => {
            const h5 = histToBarChart(s5.histogram);
            const h15 = histToBarChart(s15.histogram);
            return (
              <BarChart
                categories={h5.cats}
                series={[
                  { name: "5m", data: h5.data, tone: "info" },
                  { name: "15m", data: h15.data, tone: "warning" },
                ]}
                height={200}
              />
            );
          })()}
          <Text size="small" tone="secondary">
            <Text weight="semibold" as="span">Yorum:</Text> Median &lt;&lt; mean → kalın sağ kuyruk. Tipik {s5.median.toFixed(0)} share atılıyor ama
            arada {s15.max.toFixed(0)} share'lik dev bloklar var. BTC en cömert (median 63), XRP en sıkı (median 16). FAK bursttan kaynaklanıyor olabilir (bkz. Kriter 5).
          </Text>
        </Stack>
      </CardBody>
    </Card>
  );
}

function SecondLegCard({ data }: { data: Micro["second_leg"] }) {
  const sl5 = data["5m"];
  const sl15 = data["15m"];
  return (
    <Card variant="default">
      <CardHeader trailing={<Pill tone="info" size="sm">Kriter 3</Pill>}>
        Second-leg gevşemesi — guard mekanizması
      </CardHeader>
      <CardBody>
        <Stack gap={12}>
          <Text size="small" tone="secondary">
            İlk leg açıldıktan sonra bot karşı tarafa hangi koşulda geçiyor? Süre / fiyat guard'ları.
          </Text>
          <Grid columns={4} gap={12}>
            <Stat label="5m gecikme medyan" value={`${sl5?.delay_sec.median ?? "—"} sn`} />
            <Stat label="5m karşı taraf medyan" value={fmtN(sl5?.opp_first_px.median ?? null, 3)} />
            <Stat label="15m gecikme medyan" value={`${sl15?.delay_sec.median ?? "—"} sn`} />
            <Stat label="15m karşı taraf medyan" value={fmtN(sl15?.opp_first_px.median ?? null, 3)} />
          </Grid>
          <Table
            headers={["Bucket", "n", "delay median", "p25", "p75", "p90", "own px Δ medyan", "opp first px medyan", "own ↑ / ↓"]}
            columnAlign={["left", "right", "right", "right", "right", "right", "right", "right", "center"]}
            rows={[
              ["5m", String(sl5?.n ?? 0), `${sl5?.delay_sec.median}s`, `${sl5?.delay_sec.p25}s`, `${sl5?.delay_sec.p75}s`, `${sl5?.delay_sec.p90}s`, fmtN(sl5?.own_side_movement.median ?? null, 3), fmtN(sl5?.opp_first_px.median ?? null, 3), `${sl5?.own_side_movement.n_positive}/${sl5?.own_side_movement.n_negative}`],
              ["15m", String(sl15?.n ?? 0), `${sl15?.delay_sec.median}s`, `${sl15?.delay_sec.p25}s`, `${sl15?.delay_sec.p75}s`, `${sl15?.delay_sec.p90}s`, fmtN(sl15?.own_side_movement.median ?? null, 3), fmtN(sl15?.opp_first_px.median ?? null, 3), `${sl15?.own_side_movement.n_positive}/${sl15?.own_side_movement.n_negative}`],
            ]}
          />
          <H3>Second-leg gecikme dağılımı</H3>
          {(() => {
            const h5 = sl5 ? histToBarChart(sl5.delay_sec.histogram) : { cats: [], data: [] };
            const h15 = sl15 ? histToBarChart(sl15.delay_sec.histogram) : { cats: [], data: [] };
            return (
              <BarChart
                categories={h5.cats}
                series={[
                  { name: "5m", data: h5.data, tone: "info" },
                  { name: "15m", data: h15.data, tone: "warning" },
                ]}
                height={200}
              />
            );
          })()}
          <Text size="small" tone="secondary">
            <Text weight="semibold" as="span">Yorum:</Text> Bot hemen flip yapmıyor — ilk leg dolması için 5m: {sl5?.delay_sec.median}s, 15m: {sl15?.delay_sec.median}s bekliyor.
            Karşı taraf fiyatı medyan 0.50 (fair odds) seviyesine geldiğinde devreye giriyor. 15m'de kendi taraf fiyatı +0.01 yükseldiğinde ({sl15?.own_side_movement.n_positive} pozitif / {sl15?.own_side_movement.n_negative} negatif) flip eğilimi var → "kendi taraf pahalandıysa ve karşı taraf adil odds'ta ise" guard.
          </Text>
        </Stack>
      </CardBody>
    </Card>
  );
}

function RhythmCard({ data }: { data: Micro["cancel_replace_rhythm"] }) {
  const r5 = data["5m"];
  const r15 = data["15m"];
  return (
    <Card variant="default">
      <CardHeader trailing={<Pill tone="info" size="sm">Kriter 4</Pill>}>
        Cancel-replace ritmi — GTC mı FAK mı?
      </CardHeader>
      <CardBody>
        <Stack gap={12}>
          <Text size="small" tone="secondary">
            Ardışık trade'ler arasındaki süre dağılımı. Saf GTC pasif maker'da uzun bekler beklenir; sıkı ritim taker davranışını destekler.
          </Text>
          <Grid columns={4} gap={12}>
            <Stat label="5m medyan inter-arrival" value={`${r5?.median ?? "—"}s`} />
            <Stat label="5m aynı sn fill" value={`${r5?.n_zero_seconds ?? "—"}/${r5?.n ?? "—"}`} tone="info" />
            <Stat label="15m medyan inter-arrival" value={`${r15?.median ?? "—"}s`} />
            <Stat label="15m aynı sn fill" value={`${r15?.n_zero_seconds ?? "—"}/${r15?.n ?? "—"}`} tone="info" />
          </Grid>
          <Table
            headers={["Bucket", "n", "median", "p25", "p75", "p90", "= 0 sn", "≤ 1 sn", "≤ 3 sn", "> 30 sn"]}
            columnAlign={["left", "right", "right", "right", "right", "right", "right", "right", "right", "right"]}
            rows={[
              ["5m", String(r5?.n ?? 0), `${r5?.median}s`, `${r5?.p25}s`, `${r5?.p75}s`, `${r5?.p90}s`, String(r5?.n_zero_seconds ?? 0), String(r5?.n_within_1s ?? 0), String(r5?.n_within_3s ?? 0), String(r5?.n_over_30s ?? 0)],
              ["15m", String(r15?.n ?? 0), `${r15?.median}s`, `${r15?.p25}s`, `${r15?.p75}s`, `${r15?.p90}s`, String(r15?.n_zero_seconds ?? 0), String(r15?.n_within_1s ?? 0), String(r15?.n_within_3s ?? 0), String(r15?.n_over_30s ?? 0)],
            ]}
          />
          <H3>Inter-arrival histogramı</H3>
          {(() => {
            const h5 = r5 ? histToBarChart(r5.histogram) : { cats: [], data: [] };
            const h15 = r15 ? histToBarChart(r15.histogram) : { cats: [], data: [] };
            return (
              <BarChart
                categories={h5.cats}
                series={[
                  { name: "5m", data: h5.data, tone: "info" },
                  { name: "15m", data: h15.data, tone: "warning" },
                ]}
                height={200}
              />
            );
          })()}
          <Text size="small" tone="secondary">
            <Text weight="semibold" as="span">Yorum:</Text> Sıkı bir ritim — medyan 4-5 sn. %19 aynı saniyede burst, %25 ≤1 sn. GTC saf maker bunu üretmez. Bot taker (FAK/IOC) + cooldown hibrit.
          </Text>
        </Stack>
      </CardBody>
    </Card>
  );
}

function MultiFillCard({ data }: { data: Micro["multi_fill"] }) {
  const m5 = data["5m"];
  const m15 = data["15m"];
  return (
    <Card variant="default">
      <CardHeader trailing={<Pill tone="success" size="sm">Kriter 5 — FAK kanıtı</Pill>}>
        Same-second multi-fill (FAK kanıtı)
      </CardHeader>
      <CardBody>
        <Stack gap={12}>
          <Text size="small" tone="secondary">
            Aynı saniyede birden fazla fill = bot tek taker emirle birden fazla maker bid'i süpürdü. GTC pasif maker'da nadirdir.
          </Text>
          <Grid columns={4} gap={12}>
            <Stat label="5m multi-fill saniye %" value={`${(((m5?.multi_fill_seconds ?? 0) / (m5?.total_seconds_with_trades ?? 1)) * 100).toFixed(0)}%`} tone="success" />
            <Stat label="5m max fill / sn" value={String(m5?.max_fills_per_second ?? 0)} />
            <Stat label="15m multi-fill saniye %" value={`${(((m15?.multi_fill_seconds ?? 0) / (m15?.total_seconds_with_trades ?? 1)) * 100).toFixed(0)}%`} tone="success" />
            <Stat label="15m max fill / sn" value={String(m15?.max_fills_per_second ?? 0)} tone="warning" />
          </Grid>
          <Table
            headers={["Bucket", "trade aldığı sn", "tek-fill sn", "multi-fill sn", "max fill/sn", "multi'de trade %"]}
            columnAlign={["left", "right", "right", "right", "right", "right"]}
            rows={[
              ["5m", String(m5?.total_seconds_with_trades ?? 0), String(m5?.single_fill_seconds ?? 0), String(m5?.multi_fill_seconds ?? 0), String(m5?.max_fills_per_second ?? 0), `${((m5?.ratio_trades_in_multi ?? 0) * 100).toFixed(1)}%`],
              ["15m", String(m15?.total_seconds_with_trades ?? 0), String(m15?.single_fill_seconds ?? 0), String(m15?.multi_fill_seconds ?? 0), String(m15?.max_fills_per_second ?? 0), `${((m15?.ratio_trades_in_multi ?? 0) * 100).toFixed(1)}%`],
            ]}
          />
          <H3>Saniyede X fill (concurrency)</H3>
          {(() => {
            const cats = Array.from(new Set([...Object.keys(m5?.histogram ?? {}), ...Object.keys(m15?.histogram ?? {})])).sort();
            const d5 = cats.map((k) => (m5?.histogram?.[k]) ?? 0);
            const d15 = cats.map((k) => (m15?.histogram?.[k]) ?? 0);
            return (
              <BarChart
                categories={cats}
                series={[
                  { name: "5m", data: d5, tone: "info" },
                  { name: "15m", data: d15, tone: "warning" },
                ]}
                height={200}
              />
            );
          })()}
          <Callout tone="success">
            <Text weight="semibold" as="span">GÜÇLÜ FAK KANITI.</Text> Trade'lerin ~%34-35'i bir başka trade ile aynı saniyede gerçekleşmiş.
            15m'de bir saniyede 8 fill = bot tek taker emir gönderip emir defterindeki 8 ayrı maker bid'i süpürdü.
            Pasif GTC maker bu yoğunlukta concurrency üretmez.
          </Callout>
        </Stack>
      </CardBody>
    </Card>
  );
}

function CutoffCard({ data }: { data: Micro["t_cutoff"] }) {
  const c5 = data["5m"];
  const c15 = data["15m"];
  return (
    <Card variant="default">
      <CardHeader trailing={<Pill tone="info" size="sm">Kriter 6</Pill>}>
        T-cutoff kesinliği — T-60 mı T-90 mı?
      </CardHeader>
      <CardBody>
        <Stack gap={12}>
          <Text size="small" tone="secondary">
            Kapanıştan önce bot ne zaman duruyor? Sabit kural mı, dinamik mi?
          </Text>
          <Grid columns={4} gap={12}>
            <Stat label="5m son trade medyan" value={`T-${c5?.median ?? "—"}s`} tone="info" />
            <Stat label="5m ≤ T-90 oranı" value={`${c5 ? Math.round((c5.n_within_90s / c5.n) * 100) : 0}%`} tone="success" />
            <Stat label="15m son trade medyan" value={`T-${c15?.median ?? "—"}s`} />
            <Stat label="15m ≤ T-90 oranı" value={`${c15 ? Math.round((c15.n_within_90s / c15.n) * 100) : 0}%`} tone="warning" />
          </Grid>
          <Table
            headers={["Bucket", "n", "median", "p25", "p75", "p90", "≤ 30s", "≤ 60s", "≤ 90s", "≤ 120s", "≤ 180s"]}
            columnAlign={["left", "right", "right", "right", "right", "right", "right", "right", "right", "right", "right"]}
            rows={[
              ["5m", String(c5?.n ?? 0), `${c5?.median}s`, `${c5?.p25}s`, `${c5?.p75}s`, `${c5?.p90}s`, String(c5?.n_within_30s ?? 0), String(c5?.n_within_60s ?? 0), String(c5?.n_within_90s ?? 0), String(c5?.n_within_120s ?? 0), `${c5 ? Math.round((c5.n_within_90s / c5.n) * 100) + "% ≤90" : "—"}`],
              ["15m", String(c15?.n ?? 0), `${c15?.median}s`, `${c15?.p25}s`, `${c15?.p75}s`, `${c15?.p90}s`, String(c15?.n_within_30s ?? 0), String(c15?.n_within_60s ?? 0), String(c15?.n_within_90s ?? 0), String(c15?.n_within_120s ?? 0), `${c15 ? Math.round((c15.n_within_90s / c15.n) * 100) + "% ≤90" : "—"}`],
            ]}
          />
          <H3>T-cutoff dağılımı (saniye, kapanıştan önce)</H3>
          {(() => {
            const h5 = c5 ? histToBarChart(c5.histogram) : { cats: [], data: [] };
            const h15 = c15 ? histToBarChart(c15.histogram) : { cats: [], data: [] };
            return (
              <BarChart
                categories={h5.cats}
                series={[
                  { name: "5m", data: h5.data, tone: "info" },
                  { name: "15m", data: h15.data, tone: "warning" },
                ]}
                height={220}
              />
            );
          })()}
          <Callout tone="info">
            <Text weight="semibold" as="span">5m için T-90 sınırı baskın</Text> — vakaların %58'i T-90 öncesi, %75'i T-120 öncesi durmuş; medyan 78 sn.
            T-60 sınırı zayıf (sadece %21).<br/>
            <Text weight="semibold" as="span">15m için sabit T-cutoff yok</Text> — dağılım çok geniş (medyan 167 sn, p25-p75 = 48-343 sn).
            Burada statik T-X yerine dinamik kural (orderbook spread / pozisyon dolma / rakip flip) muhtemel.
          </Callout>
        </Stack>
      </CardBody>
    </Card>
  );
}
'''

    tsx = tsx.replace("__DATA_LITERAL__", data_literal)
    DST.parent.mkdir(parents=True, exist_ok=True)
    DST.write_text(tsx)
    print(f"Wrote {DST}")
    print(f"  size: {DST.stat().st_size} bytes ({DST.stat().st_size / 1024:.1f} KB)")


if __name__ == "__main__":
    main()
