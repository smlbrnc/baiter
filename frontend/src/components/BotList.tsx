import { Link } from "react-router-dom";
import { Play, Square, Trash2 } from "lucide-react";
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

function stateBadge(state: string) {
  switch (state) {
    case "RUNNING":
      return <Badge variant="success">{state}</Badge>;
    case "STOPPED":
      return <Badge variant="secondary">{state}</Badge>;
    case "CRASHED":
      return <Badge variant="destructive">{state}</Badge>;
    default:
      return <Badge variant="outline">{state}</Badge>;
  }
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
          <CardDescription>Henüz bot yok. Yeni Bot ekran'ından oluştur.</CardDescription>
        </CardHeader>
      </Card>
    );
  }

  return (
    <div className="grid gap-3">
      {bots.map((b) => (
        <Card key={b.id}>
          <CardContent className="flex items-center justify-between py-4">
            <div className="flex flex-col gap-1">
              <div className="flex items-center gap-2">
                <Link
                  to={`/bots/${b.id}`}
                  className="text-base font-semibold hover:underline"
                >
                  {b.name}
                </Link>
                {stateBadge(b.state)}
                <Badge variant="outline">{b.strategy}</Badge>
                <Badge variant={b.run_mode === "live" ? "default" : "warn"}>
                  {b.run_mode}
                </Badge>
              </div>
              <div className="text-xs text-muted-foreground">
                {b.slug_pattern} · ${b.order_usdc.toFixed(2)} · weight {b.signal_weight}
              </div>
            </div>
            <div className="flex items-center gap-2">
              {b.state === "RUNNING" ? (
                <Button
                  size="sm"
                  variant="secondary"
                  onClick={async () => {
                    await api.stopBot(b.id);
                    onChanged();
                  }}
                >
                  <Square className="h-4 w-4" /> Durdur
                </Button>
              ) : (
                <Button
                  size="sm"
                  onClick={async () => {
                    await api.startBot(b.id);
                    onChanged();
                  }}
                >
                  <Play className="h-4 w-4" /> Başlat
                </Button>
              )}
              <Button
                size="sm"
                variant="destructive"
                onClick={async () => {
                  if (!confirm(`Bot #${b.id} silinsin mi?`)) return;
                  await api.deleteBot(b.id);
                  onChanged();
                }}
              >
                <Trash2 className="h-4 w-4" />
              </Button>
            </div>
          </CardContent>
        </Card>
      ))}
    </div>
  );
}
