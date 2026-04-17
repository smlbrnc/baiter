import { useEffect, useRef, useState } from "react";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { api } from "@/lib/api";
import type { LogRow } from "@/lib/types";

export function LogStream({ botId }: { botId: number }) {
  const [logs, setLogs] = useState<LogRow[]>([]);
  const boxRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    let cancelled = false;
    const load = async () => {
      try {
        const rows = await api.botLogs(botId, 300);
        if (!cancelled) setLogs(rows.reverse());
      } catch {
        /* yut */
      }
    };
    load();
    const t = setInterval(load, 2000);
    return () => {
      cancelled = true;
      clearInterval(t);
    };
  }, [botId]);

  useEffect(() => {
    if (boxRef.current) boxRef.current.scrollTop = boxRef.current.scrollHeight;
  }, [logs]);

  return (
    <Card>
      <CardHeader>
        <CardTitle>Loglar</CardTitle>
        <CardDescription>Son 300 satır (2 sn'de bir yenilenir).</CardDescription>
      </CardHeader>
      <CardContent>
        <div
          ref={boxRef}
          className="h-72 overflow-auto rounded-md border border-border bg-muted/40 p-3 font-mono text-xs"
        >
          {logs.length === 0 ? (
            <div className="text-muted-foreground">Henüz log yok.</div>
          ) : (
            logs.map((l) => (
              <div key={l.id} className="flex gap-2 py-0.5">
                <span className="text-muted-foreground">
                  {new Date(l.ts_ms).toLocaleTimeString()}
                </span>
                <span
                  className={
                    l.level === "error"
                      ? "text-destructive"
                      : l.level === "warn"
                        ? "text-amber-500"
                        : "text-foreground"
                  }
                >
                  [{l.level}]
                </span>
                <span className="whitespace-pre-wrap break-all">{l.message}</span>
              </div>
            ))
          )}
        </div>
      </CardContent>
    </Card>
  );
}
