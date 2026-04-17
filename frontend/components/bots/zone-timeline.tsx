import {
  Card,
  CardContent,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";

const ZONES = [
  { key: "DeepTrade", color: "bg-sky-500", range: [0, 0.3] as const },
  { key: "NormalTrade", color: "bg-emerald-500", range: [0.3, 0.6] as const },
  { key: "AggTrade", color: "bg-amber-500", range: [0.6, 0.85] as const },
  { key: "FakTrade", color: "bg-orange-500", range: [0.85, 0.97] as const },
  { key: "StopTrade", color: "bg-red-500", range: [0.97, 1.0] as const },
];

export function ZoneTimeline({
  zone,
  pct,
}: {
  zone: string | null;
  pct: number | null;
}) {
  const p = Math.min(Math.max(pct ?? 0, 0), 1);
  return (
    <Card>
      <CardHeader>
        <CardTitle className="flex items-center gap-2">
          Zone Timeline
          <span className="text-muted-foreground text-xs font-normal">
            {zone ?? "-"} · {(p * 100).toFixed(1)}%
          </span>
        </CardTitle>
      </CardHeader>
      <CardContent>
        <div className="border-border relative flex h-6 w-full overflow-hidden rounded-md border">
          {ZONES.map((z) => (
            <div
              key={z.key}
              className={`${z.color} opacity-80`}
              style={{
                width: `${((z.range[1] - z.range[0]) * 100).toFixed(2)}%`,
              }}
              title={z.key}
            />
          ))}
          <div
            className="bg-foreground absolute top-0 h-full w-0.5 transition-all"
            style={{ left: `${(p * 100).toFixed(2)}%` }}
          />
        </div>
        <div className="text-muted-foreground mt-2 grid grid-cols-5 gap-1 text-[10px]">
          {ZONES.map((z) => (
            <span key={z.key} className="text-center">
              {z.key}
            </span>
          ))}
        </div>
      </CardContent>
    </Card>
  );
}
