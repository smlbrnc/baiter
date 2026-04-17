import { useEffect, useMemo, useRef } from "react";
import {
  createChart,
  ColorType,
  LineSeries,
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

/**
 * Binance sinyal skor chart'ı (`SignalUpdate` event'lerinden beslenir).
 * 0-10 arası line; 5 nötr referans.
 */
export function SignalChart({ botId }: { botId: number }) {
  const containerRef = useRef<HTMLDivElement>(null);
  const chartRef = useRef<IChartApi | null>(null);
  const seriesRef = useRef<ISeriesApi<"Line"> | null>(null);

  useEffect(() => {
    if (!containerRef.current) return;
    const chart = createChart(containerRef.current, {
      height: 180,
      layout: {
        background: { type: ColorType.Solid, color: "transparent" },
        textColor: "#a1a1aa",
      },
      grid: {
        vertLines: { color: "rgba(255,255,255,0.04)" },
        horzLines: { color: "rgba(255,255,255,0.04)" },
      },
      rightPriceScale: {
        borderColor: "rgba(255,255,255,0.1)",
        autoScale: false,
        scaleMargins: { top: 0, bottom: 0 },
      },
      timeScale: {
        borderColor: "rgba(255,255,255,0.1)",
        timeVisible: true,
        secondsVisible: false,
      },
      autoSize: true,
    });
    const s = chart.addSeries(LineSeries, {
      color: "#60a5fa",
      lineWidth: 2,
      title: "signal_score",
    });
    s.applyOptions({
      priceFormat: { type: "price", precision: 2, minMove: 0.01 },
    });
    chartRef.current = chart;
    seriesRef.current = s;
    return () => chart.remove();
  }, []);

  const filter = useMemo(
    () => (ev: FrontendEvent) =>
      ev.kind === "SignalUpdate" && ev.bot_id === botId,
    [botId],
  );

  useEventStream((ev) => {
    if (ev.kind !== "SignalUpdate" || !seriesRef.current) return;
    seriesRef.current.update({
      time: Math.floor(ev.ts_ms / 1000) as UTCTimestamp,
      value: ev.signal_score,
    });
  }, filter);

  return (
    <Card>
      <CardHeader>
        <CardTitle>Signal (Binance)</CardTitle>
        <CardDescription>
          CVD + BSI(Hawkes) + OFI → 0-10 normalize; 5 nötr.
        </CardDescription>
      </CardHeader>
      <CardContent>
        <div ref={containerRef} className="h-[180px] w-full" />
      </CardContent>
    </Card>
  );
}
