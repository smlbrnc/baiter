"use client";

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
import { cn } from "@/lib/utils";

/** Kullanıcı alttan bu kadar px içindeyse yenilemede alta yapışık say. */
const STICK_BOTTOM_THRESHOLD_PX = 64;

export function LogStream({ botId }: { botId: number }) {
  const [logs, setLogs] = useState<LogRow[]>([]);
  const boxRef = useRef<HTMLDivElement>(null);
  /** true: son yenilemede kullanıcı alta yakındı; false: yukarı kaydırmış. */
  const stickToBottomRef = useRef(true);

  useEffect(() => {
    stickToBottomRef.current = true;
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

  const updateStickFromScroll = () => {
    const el = boxRef.current;
    if (!el) return;
    const fromBottom = el.scrollHeight - el.scrollTop - el.clientHeight;
    stickToBottomRef.current = fromBottom <= STICK_BOTTOM_THRESHOLD_PX;
  };

  useEffect(() => {
    const el = boxRef.current;
    if (!el || !stickToBottomRef.current) return;
    el.scrollTop = el.scrollHeight;
  }, [logs]);

  return (
    <Card>
      <CardHeader>
        <CardTitle>Loglar</CardTitle>
        <CardDescription>
          Son 300 satır · 2 sn&apos;de bir yenilenir
        </CardDescription>
      </CardHeader>
      <CardContent>
        <div
          ref={boxRef}
          onScroll={updateStickFromScroll}
          className="bg-muted/40 h-72 overflow-auto rounded-md border border-border/45 p-3 font-mono text-[11px]"
        >
          {logs.length === 0 ? (
            <div className="text-muted-foreground">Henüz log yok.</div>
          ) : (
            logs.map((l) => (
              <div
                key={l.id}
                className={cn(
                  "py-0.5 break-all whitespace-pre-wrap",
                  l.level === "error"
                    ? "text-destructive"
                    : l.level === "warn"
                      ? "text-amber-500"
                      : "text-foreground",
                )}
              >
                {l.message}
              </div>
            ))
          )}
        </div>
      </CardContent>
    </Card>
  );
}
