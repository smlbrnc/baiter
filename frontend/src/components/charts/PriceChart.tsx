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

interface Props {
  botId: number;
  windowStartTs?: number; // unix saniye
  windowEndTs?: number;
}

/**
 * YES/NO bid/ask dört satırı tek chart'ta gösterir.
 * - YES best_bid (yeşil kalın)
 * - YES best_ask (yeşil ince)
 * - NO best_bid (kırmızı kalın)
 * - NO best_ask (kırmızı ince)
 * x-ekseni unix timestamp, startDate/endDate aralığına fit'lenir.
 */
export function PriceChart({ botId, windowStartTs, windowEndTs }: Props) {
  const containerRef = useRef<HTMLDivElement>(null);
  const chartRef = useRef<IChartApi | null>(null);
  const seriesRef = useRef<{
    yesBid: ISeriesApi<"Line">;
    yesAsk: ISeriesApi<"Line">;
    noBid: ISeriesApi<"Line">;
    noAsk: ISeriesApi<"Line">;
  } | null>(null);

  useEffect(() => {
    if (!containerRef.current) return;
    const chart = createChart(containerRef.current, {
      height: 300,
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

    const yesBid = chart.addSeries(LineSeries, {
      color: "#10b981",
      lineWidth: 3,
      title: "YES bid",
    });
    const yesAsk = chart.addSeries(LineSeries, {
      color: "#34d399",
      lineWidth: 1,
      title: "YES ask",
    });
    const noBid = chart.addSeries(LineSeries, {
      color: "#ef4444",
      lineWidth: 3,
      title: "NO bid",
    });
    const noAsk = chart.addSeries(LineSeries, {
      color: "#f87171",
      lineWidth: 1,
      title: "NO ask",
    });

    chartRef.current = chart;
    seriesRef.current = { yesBid, yesAsk, noBid, noAsk };

    if (windowStartTs && windowEndTs) {
      chart.timeScale().setVisibleRange({
        from: windowStartTs as UTCTimestamp,
        to: windowEndTs as UTCTimestamp,
      });
    }

    return () => chart.remove();
  }, [windowStartTs, windowEndTs]);

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
    s.yesBid.update({ time: t, value: ev.yes_best_bid });
    s.yesAsk.update({ time: t, value: ev.yes_best_ask });
    s.noBid.update({ time: t, value: ev.no_best_bid });
    s.noAsk.update({ time: t, value: ev.no_best_ask });
  }, filter);

  return (
    <Card>
      <CardHeader>
        <CardTitle>Price</CardTitle>
        <CardDescription>
          YES bid/ask (yeşil) · NO bid/ask (kırmızı)
        </CardDescription>
      </CardHeader>
      <CardContent>
        <div ref={containerRef} className="h-[300px] w-full" />
      </CardContent>
    </Card>
  );
}
