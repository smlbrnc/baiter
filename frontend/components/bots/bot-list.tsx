"use client";

import Link from "next/link";
import { HugeiconsIcon } from "@hugeicons/react";
import {
  PlayFreeIcons,
  StopCircleFreeIcons,
  Delete02FreeIcons,
} from "@hugeicons/core-free-icons";
import { toast } from "sonner";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { api } from "@/lib/api";
import type { BotRow } from "@/lib/types";

function StateBadge({ state }: { state: string }) {
  switch (state) {
    case "RUNNING":
      return (
        <Badge className="border-transparent bg-emerald-500/15 text-emerald-600 dark:text-emerald-400">
          {state}
        </Badge>
      );
    case "STOPPED":
      return <Badge variant="secondary">{state}</Badge>;
    case "CRASHED":
      return <Badge variant="destructive">{state}</Badge>;
    default:
      return <Badge variant="outline">{state}</Badge>;
  }
}

function ModeBadge({ mode }: { mode: "live" | "dryrun" }) {
  if (mode === "live") {
    return (
      <Badge className="border-transparent bg-primary/15 text-primary">
        live
      </Badge>
    );
  }
  return (
    <Badge className="border-transparent bg-amber-500/15 text-amber-700 dark:text-amber-400">
      dryrun
    </Badge>
  );
}

export function BotList({
  bots,
  onChanged,
}: {
  bots: BotRow[];
  onChanged: () => void;
}) {
  if (bots.length === 0) {
    return (
      <Card>
        <CardHeader>
          <CardTitle>Bots</CardTitle>
          <CardDescription>
            Henüz bot yok. Yeni Bot ekranından oluştur.
          </CardDescription>
        </CardHeader>
      </Card>
    );
  }

  const doStart = async (id: number) => {
    try {
      await api.startBot(id);
      toast.success(`Bot #${id} başlatıldı`);
      onChanged();
    } catch (e) {
      toast.error((e as Error).message);
    }
  };

  const doStop = async (id: number) => {
    try {
      await api.stopBot(id);
      toast.success(`Bot #${id} durduruldu`);
      onChanged();
    } catch (e) {
      toast.error((e as Error).message);
    }
  };

  const doDelete = async (id: number) => {
    if (!confirm(`Bot #${id} silinsin mi?`)) return;
    try {
      await api.deleteBot(id);
      toast.success(`Bot #${id} silindi`);
      onChanged();
    } catch (e) {
      toast.error((e as Error).message);
    }
  };

  return (
    <div className="grid gap-3">
      {bots.map((b) => (
        <Card key={b.id}>
          <CardContent className="flex items-center justify-between gap-4 py-4">
            <div className="flex min-w-0 flex-col gap-1">
              <div className="flex flex-wrap items-center gap-2">
                <Link
                  href={`/bots/${b.id}`}
                  className="truncate text-sm font-semibold hover:underline"
                >
                  {b.name}
                </Link>
                <StateBadge state={b.state} />
                <Badge variant="outline">{b.strategy}</Badge>
                <ModeBadge mode={b.run_mode} />
              </div>
              <div className="text-muted-foreground truncate font-mono text-xs">
                {b.slug_pattern} · ${b.order_usdc.toFixed(2)} · weight{" "}
                {b.signal_weight}
              </div>
            </div>
            <div className="flex shrink-0 items-center gap-1.5">
              {b.state === "RUNNING" ? (
                <Button size="sm" variant="secondary" onClick={() => doStop(b.id)}>
                  <HugeiconsIcon icon={StopCircleFreeIcons} />
                  Durdur
                </Button>
              ) : (
                <Button size="sm" onClick={() => doStart(b.id)}>
                  <HugeiconsIcon icon={PlayFreeIcons} />
                  Başlat
                </Button>
              )}
              <Button
                size="icon-sm"
                variant="destructive"
                onClick={() => doDelete(b.id)}
              >
                <HugeiconsIcon icon={Delete02FreeIcons} />
              </Button>
            </div>
          </CardContent>
        </Card>
      ))}
    </div>
  );
}
