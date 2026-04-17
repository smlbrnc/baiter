"use client";

import Link from "next/link";
import { HugeiconsIcon } from "@hugeicons/react";
import { Add01FreeIcons } from "@hugeicons/core-free-icons";
import { BotList } from "@/components/bots/bot-list";
import { Button } from "@/components/ui/button";
import { useBots } from "@/lib/hooks";

export default function DashboardPage() {
  const { bots, reload } = useBots();
  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between">
        <div>
          <h1 className="text-xl font-semibold tracking-tight">Dashboard</h1>
          <p className="text-muted-foreground text-xs">
            Aktif botlar ve kısa durum
          </p>
        </div>
        <Button asChild size="lg">
          <Link href="/bots/new">
            <HugeiconsIcon icon={Add01FreeIcons} />
            Yeni bot
          </Link>
        </Button>
      </div>
      <BotList bots={bots} onChanged={reload} />
    </div>
  );
}
