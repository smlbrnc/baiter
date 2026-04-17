import { Activity, LayoutDashboard, PlusCircle } from "lucide-react";
import { NavLink } from "react-router-dom";
import { cn } from "@/lib/utils";
import { Separator } from "@/components/ui/separator";

export function AppShell({ children }: { children: React.ReactNode }) {
  return (
    <div className="flex min-h-screen bg-background text-foreground">
      <aside className="flex w-60 shrink-0 flex-col border-r border-border bg-card">
        <div className="flex h-14 items-center gap-2 px-4">
          <Activity className="h-5 w-5 text-primary" />
          <span className="text-sm font-semibold tracking-wide">Baiter Pro</span>
        </div>
        <Separator />
        <nav className="flex flex-col gap-1 px-2 py-3">
          <NavItem to="/" icon={<LayoutDashboard className="h-4 w-4" />}>
            Dashboard
          </NavItem>
          <NavItem to="/bots/new" icon={<PlusCircle className="h-4 w-4" />}>
            Yeni Bot
          </NavItem>
        </nav>
        <div className="mt-auto p-4 text-xs text-muted-foreground">
          Polymarket CLOB + Gamma
        </div>
      </aside>
      <main className="flex-1 overflow-auto">
        <div className="mx-auto max-w-7xl p-6">{children}</div>
      </main>
    </div>
  );
}

function NavItem({
  to,
  icon,
  children,
}: {
  to: string;
  icon: React.ReactNode;
  children: React.ReactNode;
}) {
  return (
    <NavLink
      to={to}
      end
      className={({ isActive }) =>
        cn(
          "flex items-center gap-2 rounded-md px-3 py-2 text-sm font-medium transition-colors",
          isActive
            ? "bg-accent text-accent-foreground"
            : "text-muted-foreground hover:bg-accent hover:text-accent-foreground",
        )
      }
    >
      {icon}
      {children}
    </NavLink>
  );
}
