import { Link } from "react-router-dom";
import { PlusCircle } from "lucide-react";
import { Button } from "@/components/ui/button";
import { BotList } from "@/components/BotList";
import { useBots } from "@/lib/hooks";

export function Dashboard() {
  const { bots, reload } = useBots();
  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between">
        <h1 className="text-2xl font-semibold tracking-tight">Dashboard</h1>
        <Button asChild>
          <Link to="/bots/new">
            <PlusCircle className="h-4 w-4" /> Yeni bot
          </Link>
        </Button>
      </div>
      <BotList bots={bots} onChanged={reload} />
    </div>
  );
}
