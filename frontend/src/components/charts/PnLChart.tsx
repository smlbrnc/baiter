import { useEffect, useRef } from "react";
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
import { api } from "@/lib/api";

interface Props {
  botId: number;
  pollMs?: number;
}

/**
 * `/api/bots/:id/pnl` endpoint'inden saniyede bir snapshot alır,
 * `mtm_pnl` için tek line chart (§17 1sn polling kuralı).
 */
export function PnLChart({ botId, pollMs = 1000 }: Props) {
  const containerRef = useRef<HTMLDivElement>(null);
  const chartRef = useRef<IChartApi | null>(null);
  const seriesRef = useRef<ISeriesApi<"Line"> | null>(null);

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
    const series = chart.addSeries(LineSeries, {
      color: "#f59e0b",
      lineWidth: 2,
      title: "mtm_pnl",
    });
    chartRef.current = chart;
    seriesRef.current = series;
    return () => chart.remove();
  }, []);

  useEffect(() => {
    let cancelled = false;
    let last = -1;
    const tick = async () => {
      try {
        const p = await api.botPnl(botId);
        if (cancelled || !p || !seriesRef.current) return;
        if (p.ts_ms === last) return;
        last = p.ts_ms;
        seriesRef.current.update({
          time: Math.floor(p.ts_ms / 1000) as UTCTimestamp,
          value: p.mtm_pnl,
        });
      } catch {
        /* yut */
      }
    };
    const t = setInterval(tick, pollMs);
    tick();
    return () => {
      cancelled = true;
      clearInterval(t);
    };
  }, [botId, pollMs]);

  return (
    <Card>
      <CardHeader>
        <CardTitle>PnL (mtm)</CardTitle>
        <CardDescription>1 sn polling (§17).</CardDescription>
      </CardHeader>
      <CardContent>
        <div ref={containerRef} className="h-[200px] w-full" />
      </CardContent>
    </Card>
  );
}
