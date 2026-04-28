"use client";

import { useEffect, useMemo, useRef, useState } from "react";
import { Download, Eraser } from "lucide-react";
import { Button } from "@/components/ui/button";
import {
  Card,
  CardAction,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import { api } from "@/lib/api";
import type { LogRow } from "@/lib/types";
import { cn } from "@/lib/utils";

const STICK_BOTTOM_THRESHOLD_PX = 64;

export function LogStream({ botId }: { botId: number }) {
  const [logs, setLogs] = useState<LogRow[]>([]);
  const [clearCutoffId, setClearCutoffId] = useState<number | null>(null);
  const boxRef = useRef<HTMLDivElement>(null);
  const stickToBottomRef = useRef(true);

  useEffect(() => {
    stickToBottomRef.current = true;
    let ctrl: AbortController | null = null;
    let timer: ReturnType<typeof setInterval> | null = null;

    const load = async () => {
      if (document.hidden) return;
      ctrl?.abort();
      ctrl = new AbortController();
      const { signal } = ctrl;
      try {
        const rows = await api.botLogs(botId, 300, signal);
        if (signal.aborted) return;
        const reversed = rows.reverse();
        setLogs((prev) => {
          // Skip re-render if data is unchanged (same tail log id).
          if (
            prev.length === reversed.length &&
            prev[prev.length - 1]?.id === reversed[reversed.length - 1]?.id
          ) {
            return prev;
          }
          return reversed;
        });
      } catch (e) {
        if (e instanceof Error && e.name === "AbortError") return;
      }
    };

    const startTimer = () => {
      if (timer !== null) return;
      timer = setInterval(() => void load(), 2000);
    };
    const stopTimer = () => {
      if (timer !== null) {
        clearInterval(timer);
        timer = null;
      }
    };

    const onVisibility = () => {
      if (document.hidden) {
        stopTimer();
        ctrl?.abort();
      } else {
        void load();
        startTimer();
      }
    };

    void load();
    document.addEventListener("visibilitychange", onVisibility);
    if (!document.hidden) startTimer();

    return () => {
      stopTimer();
      ctrl?.abort();
      document.removeEventListener("visibilitychange", onVisibility);
    };
  }, [botId]);

  const visibleLogs = useMemo(
    () =>
      clearCutoffId == null
        ? logs
        : logs.filter((l) => l.id > clearCutoffId),
    [logs, clearCutoffId],
  );

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
  }, [visibleLogs]);

  const onClear = () => {
    if (logs.length === 0) return;
    const maxId = logs.reduce((m, l) => (l.id > m ? l.id : m), 0);
    setClearCutoffId(maxId);
    stickToBottomRef.current = true;
  };

  const onDownload = () => {
    if (visibleLogs.length === 0) return;
    const lines = visibleLogs
      .map((l) => {
        const ts = new Date(l.ts_ms).toISOString();
        return `[${ts}] [${l.level.toUpperCase()}] ${l.message}`;
      })
      .join("\n");
    const blob = new Blob([lines + "\n"], {
      type: "text/plain;charset=utf-8",
    });
    const url = URL.createObjectURL(blob);
    const stamp = new Date()
      .toISOString()
      .replace(/[:.]/g, "-")
      .replace("T", "_")
      .slice(0, 19);
    const a = document.createElement("a");
    a.href = url;
    a.download = `bot-${botId}-logs-${stamp}.log`;
    document.body.appendChild(a);
    a.click();
    a.remove();
    URL.revokeObjectURL(url);
  };

  return (
    <Card>
      <CardHeader>
        <CardTitle>Loglar</CardTitle>
        <CardDescription>
          Son 300 satır · 2 sn&apos;de bir yenilenir
        </CardDescription>
        <CardAction className="flex gap-1">
          <Tooltip>
            <TooltipTrigger asChild>
              <Button
                size="icon"
                variant="ghost"
                onClick={onDownload}
                disabled={visibleLogs.length === 0}
                aria-label="Logları indir"
              >
                <Download />
              </Button>
            </TooltipTrigger>
            <TooltipContent>Logları indir (.log)</TooltipContent>
          </Tooltip>
          <Tooltip>
            <TooltipTrigger asChild>
              <Button
                size="icon"
                variant="ghost"
                onClick={onClear}
                disabled={visibleLogs.length === 0}
                aria-label="Ekranı temizle"
              >
                <Eraser />
              </Button>
            </TooltipTrigger>
            <TooltipContent>Ekranı temizle (yeni loglar görünür)</TooltipContent>
          </Tooltip>
        </CardAction>
      </CardHeader>
      <CardContent>
        <div
          ref={boxRef}
          onScroll={updateStickFromScroll}
          className="bg-muted/40 border-border/45 h-[60vh] min-h-96 overflow-auto rounded-md border p-3 font-mono text-[11px]"
        >
          {visibleLogs.length === 0 ? (
            <div className="text-muted-foreground">
              {logs.length === 0
                ? "Henüz log yok."
                : "Ekran temizlendi — yeni loglar burada görünecek."}
            </div>
          ) : (
            visibleLogs.map((l) => (
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
