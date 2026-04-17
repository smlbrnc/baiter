import { useEffect, useMemo, useRef } from "react";
import {
  createChart,
  ColorType,
  HistogramSeries,
  type IChartApi,
  type ISeriesApi,
  type UTCTimestamp,
} from "lightweight-charts";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { useEventStream } from "@/lib/hooks";
import type { FrontendEvent } from "@/lib/types";

interface Props {
  botId: number;
}

/**
 * YES ve NO spread histogramları (separate series, aynı chart).
 * - YES spread: yeşil
 * - NO spread: kırmızı
 */
export function SpreadChart({ botId }: Props) {
  const containerRef = useRef<HTMLDivElement>(null);
  const chartRef = useRef<IChartApi | null>(null);
  const seriesRef = useRef<{
    yes: ISeriesApi<"Histogram">;
    no: ISeriesApi<"Histogram">;
  } | null>(null);

  useEffect(() => {
    if (!containerRef.current) return;
    const chart = createChart(containerRef.current, {
      height: 200,
      layout: {
        background: { type: ColorType.Solid, color: "transparent" },
        textColor: "#a1a1aa",
      },
      grid: {
        vertLines: { color: "rgba(255,255,255,0.04)" },
        horzLines: { color: "rgba(255,255,255,0.04)" },
      },
      rightPriceScale: { borderColor: "rgba(255,255,255,0.1)" },
      timeScale: {
        borderColor: "rgba(255,255,255,0.1)",
        timeVisible: true,
        secondsVisible: false,
      },
      autoSize: true,
    });

    const yes = chart.addSeries(HistogramSeries, {
      color: "rgba(16,185,129,0.7)",
      priceFormat: { type: "price", precision: 4, minMove: 0.0001 },
      title: "YES spread",
    });
    const no = chart.addSeries(HistogramSeries, {
      color: "rgba(239,68,68,0.7)",
      priceFormat: { type: "price", precision: 4, minMove: 0.0001 },
      title: "NO spread",
      priceScaleId: "no",
    });
    chart.priceScale("no").applyOptions({ visible: false });

    chartRef.current = chart;
    seriesRef.current = { yes, no };

    return () => chart.remove();
  }, []);

  const filter = useMemo(
    () => (ev: FrontendEvent) =>
      ev.kind === "BestBidAsk" && ev.bot_id === botId,
    [botId],
  );

  useEventStream((ev) => {
    if (ev.kind !== "BestBidAsk") return;
    const s = seriesRef.current;
    if (!s) return;
    const t = Math.floor(ev.ts_ms / 1000) as UTCTimestamp;
    const yesSpread = Math.max(0, ev.yes_best_ask - ev.yes_best_bid);
    const noSpread = Math.max(0, ev.no_best_ask - ev.no_best_bid);
    s.yes.update({ time: t, value: yesSpread });
    s.no.update({ time: t, value: noSpread });
  }, filter);

  return (
    <Card>
      <CardHeader>
        <CardTitle>Spread</CardTitle>
        <CardDescription>
          YES (yeşil) ve NO (kırmızı) spread histogramı.
        </CardDescription>
      </CardHeader>
      <CardContent>
        <div ref={containerRef} className="h-[200px] w-full" />
      </CardContent>
    </Card>
  );
}
