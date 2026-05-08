"use client"

import Link from "next/link"
import { usePathname } from "next/navigation"
import type { LucideIcon } from "lucide-react"
import {
  Activity,
  BarChart2,
  Bot,
  Keyboard,
  LayoutDashboard,
  Plus,
  Settings2,
} from "lucide-react"
import { cn } from "@/lib/utils"

const NAV: Array<{
  href: string
  label: string
  icon: LucideIcon
  match: (p: string) => boolean
}> = [
  {
    href: "/",
    label: "Dashboard",
    icon: LayoutDashboard,
    match: (p) => p === "/",
  },
  {
    href: "/bots/new",
    label: "Yeni Bot",
    icon: Plus,
    match: (p) => p.startsWith("/bots/new"),
  },
  {
    href: "/stats",
    label: "İstatistikler",
    icon: BarChart2,
    match: (p) => p.startsWith("/stats"),
  },
  {
    href: "/settings",
    label: "Ayarlar",
    icon: Settings2,
    match: (p) => p.startsWith("/settings"),
  },
]

export function AppSidebar() {
  const pathname = usePathname()

  return (
    <aside className="flex w-14 shrink-0 flex-col border-r border-sidebar-border bg-sidebar text-sidebar-foreground shadow-xs">
      <div className="flex h-14 shrink-0 items-center justify-center border-b border-sidebar-border px-2">
        <span title="Baiter Pro">
          <Activity aria-hidden className="size-5 text-sidebar-primary" />
        </span>
      </div>
      <nav className="flex flex-col items-center gap-1 p-2">
        {NAV.map((item) => {
          const active = item.match(pathname)
          const Icon = item.icon
          return (
            <Link
              key={item.href}
              href={item.href}
              title={item.label}
              aria-label={item.label}
              className={cn(
                "flex size-10 items-center justify-center rounded-md transition-colors",
                active
                  ? "bg-sidebar-primary text-sidebar-primary-foreground shadow-xs"
                  : "text-sidebar-foreground/80 hover:bg-sidebar-accent hover:text-sidebar-accent-foreground"
              )}
            >
              <Icon aria-hidden className="size-5" />
            </Link>
          )
        })}
      </nav>
      <div className="mt-auto flex flex-col items-center gap-3 border-t border-sidebar-border p-2">
        <span title="Polymarket CLOB + Gamma">
          <Bot aria-hidden className="size-4 text-muted-foreground" />
        </span>
        <span title="Tema değiştir: d">
          <Keyboard aria-hidden className="size-4 text-muted-foreground" />
        </span>
      </div>
    </aside>
  )
}
