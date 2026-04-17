"use client";

import Link from "next/link";
import { usePathname } from "next/navigation";
import { HugeiconsIcon } from "@hugeicons/react";
import {
  Activity01FreeIcons,
  DashboardCircleFreeIcons,
  Add01FreeIcons,
  BotFreeIcons,
} from "@hugeicons/core-free-icons";
import { Separator } from "@/components/ui/separator";
import { cn } from "@/lib/utils";

const NAV: Array<{
  href: string;
  label: string;
  icon: typeof DashboardCircleFreeIcons;
  match: (p: string) => boolean;
}> = [
  {
    href: "/",
    label: "Dashboard",
    icon: DashboardCircleFreeIcons,
    match: (p) => p === "/",
  },
  {
    href: "/bots/new",
    label: "Yeni Bot",
    icon: Add01FreeIcons,
    match: (p) => p.startsWith("/bots/new"),
  },
];

export function AppSidebar() {
  const pathname = usePathname();

  return (
    <aside className="bg-sidebar text-sidebar-foreground border-sidebar-border flex w-56 shrink-0 flex-col border-r">
      <div className="flex h-14 items-center gap-2 px-4">
        <HugeiconsIcon
          icon={Activity01FreeIcons}
          className="text-sidebar-primary size-5"
        />
        <span className="text-sm font-semibold tracking-wide">Baiter Pro</span>
      </div>
      <Separator className="bg-sidebar-border" />
      <nav className="flex flex-col gap-0.5 px-2 py-3">
        {NAV.map((item) => {
          const active = item.match(pathname);
          return (
            <Link
              key={item.href}
              href={item.href}
              className={cn(
                "flex items-center gap-2 rounded-md px-3 py-2 text-xs font-medium transition-colors",
                active
                  ? "bg-sidebar-primary text-sidebar-primary-foreground"
                  : "text-sidebar-foreground/70 hover:bg-sidebar-accent hover:text-sidebar-accent-foreground",
              )}
            >
              <HugeiconsIcon icon={item.icon} className="size-4" />
              {item.label}
            </Link>
          );
        })}
      </nav>
      <div className="mt-auto flex flex-col gap-2 p-4">
        <div className="text-muted-foreground flex items-center gap-2 text-[10px]">
          <HugeiconsIcon icon={BotFreeIcons} className="size-3" />
          Polymarket CLOB + Gamma
        </div>
        <div className="text-muted-foreground/70 text-[10px]">
          Temayı değiştir: <kbd className="bg-muted rounded px-1">d</kbd>
        </div>
      </div>
    </aside>
  );
}
