import { FormEvent, useState } from "react";
import { useNavigate } from "react-router-dom";
import { Button } from "@/components/ui/button";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { api } from "@/lib/api";
import type { CreateBotReq, RunMode, Strategy } from "@/lib/types";

const STRATEGIES: Strategy[] = ["harvest", "dutch_book", "prism"];
const RUN_MODES: RunMode[] = ["dryrun", "live"];
const SLUG_PREFIXES = [
  "btc-updown-5m-",
  "btc-updown-15m-",
  "btc-updown-1h-",
  "btc-updown-4h-",
  "eth-updown-5m-",
  "eth-updown-15m-",
  "sol-updown-5m-",
  "xrp-updown-5m-",
];

export function BotForm() {
  const nav = useNavigate();
  const [form, setForm] = useState<CreateBotReq>({
    name: "",
    slug_pattern: "btc-updown-5m-",
    strategy: "harvest",
    run_mode: "dryrun",
    order_usdc: 5,
    signal_weight: 0,
    auto_start: false,
  });
  const [includeCreds, setIncludeCreds] = useState(false);
  const [creds, setCreds] = useState({
    poly_address: "",
    poly_api_key: "",
    poly_passphrase: "",
    poly_secret: "",
    polygon_private_key: "",
    signature_type: 0,
  });
  const [error, setError] = useState<string | null>(null);
  const [submitting, setSubmitting] = useState(false);

  const onSubmit = async (e: FormEvent) => {
    e.preventDefault();
    setError(null);
    setSubmitting(true);
    try {
      const body: CreateBotReq = {
        ...form,
        order_usdc: Number(form.order_usdc),
        signal_weight: Number(form.signal_weight),
      };
      if (includeCreds) {
        body.credentials = { ...creds };
      }
      const { id } = await api.createBot(body);
      nav(`/bots/${id}`);
    } catch (e) {
      setError((e as Error).message);
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <form onSubmit={onSubmit} className="space-y-4">
      <Card>
        <CardHeader>
          <CardTitle>Temel Ayarlar</CardTitle>
          <CardDescription>Bot adı + seçilen market kalıbı.</CardDescription>
        </CardHeader>
        <CardContent className="grid gap-4 sm:grid-cols-2">
          <Field label="Bot adı">
            <Input
              value={form.name}
              required
              onChange={(e) => setForm({ ...form, name: e.target.value })}
              placeholder="btc-5m-1"
            />
          </Field>
          <Field label="Slug pattern">
            <Select
              value={form.slug_pattern}
              onValueChange={(v) => setForm({ ...form, slug_pattern: v })}
            >
              <SelectTrigger>
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                {SLUG_PREFIXES.map((s) => (
                  <SelectItem key={s} value={s}>
                    {s}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </Field>
          <Field label="Strateji">
            <Select
              value={form.strategy}
              onValueChange={(v) => setForm({ ...form, strategy: v as Strategy })}
            >
              <SelectTrigger>
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                {STRATEGIES.map((s) => (
                  <SelectItem key={s} value={s}>
                    {s}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </Field>
          <Field label="Run mode">
            <Select
              value={form.run_mode}
              onValueChange={(v) => setForm({ ...form, run_mode: v as RunMode })}
            >
              <SelectTrigger>
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                {RUN_MODES.map((s) => (
                  <SelectItem key={s} value={s}>
                    {s}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </Field>
          <Field label="Order USDC">
            <Input
              type="number"
              step="0.01"
              min="1"
              value={form.order_usdc}
              onChange={(e) =>
                setForm({ ...form, order_usdc: Number(e.target.value) })
              }
            />
          </Field>
          <Field label="Signal weight (0-10)">
            <Input
              type="number"
              step="0.1"
              min="0"
              max="10"
              value={form.signal_weight}
              onChange={(e) =>
                setForm({ ...form, signal_weight: Number(e.target.value) })
              }
            />
          </Field>
          <Field label="Otomatik başlat">
            <label className="flex items-center gap-2 text-sm">
              <input
                type="checkbox"
                checked={form.auto_start ?? false}
                onChange={(e) =>
                  setForm({ ...form, auto_start: e.target.checked })
                }
              />
              Oluşturduktan sonra başlat
            </label>
          </Field>
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>Polymarket Kimliği (ops.)</CardTitle>
          <CardDescription>
            Boşsa `.env` POLY_* fallback kullanılır (Live mod'da zorunlu).
          </CardDescription>
        </CardHeader>
        <CardContent className="space-y-4">
          <label className="flex items-center gap-2 text-sm">
            <input
              type="checkbox"
              checked={includeCreds}
              onChange={(e) => setIncludeCreds(e.target.checked)}
            />
            Bu bot için özel kimlik bilgisi gir
          </label>

          {includeCreds && (
            <div className="grid gap-4 sm:grid-cols-2">
              <Field label="POLY_ADDRESS">
                <Input
                  value={creds.poly_address}
                  onChange={(e) =>
                    setCreds({ ...creds, poly_address: e.target.value })
                  }
                />
              </Field>
              <Field label="POLY_API_KEY">
                <Input
                  value={creds.poly_api_key}
                  onChange={(e) =>
                    setCreds({ ...creds, poly_api_key: e.target.value })
                  }
                />
              </Field>
              <Field label="POLY_PASSPHRASE">
                <Input
                  value={creds.poly_passphrase}
                  onChange={(e) =>
                    setCreds({ ...creds, poly_passphrase: e.target.value })
                  }
                />
              </Field>
              <Field label="POLY_SECRET">
                <Input
                  type="password"
                  value={creds.poly_secret}
                  onChange={(e) =>
                    setCreds({ ...creds, poly_secret: e.target.value })
                  }
                />
              </Field>
              <Field label="POLYGON_PRIVATE_KEY">
                <Input
                  type="password"
                  value={creds.polygon_private_key}
                  onChange={(e) =>
                    setCreds({
                      ...creds,
                      polygon_private_key: e.target.value,
                    })
                  }
                />
              </Field>
              <Field label="Signature type">
                <Input
                  type="number"
                  value={creds.signature_type}
                  onChange={(e) =>
                    setCreds({
                      ...creds,
                      signature_type: Number(e.target.value),
                    })
                  }
                />
              </Field>
            </div>
          )}
        </CardContent>
      </Card>

      {error && <p className="text-sm text-destructive">{error}</p>}

      <div className="flex justify-end gap-2">
        <Button type="submit" disabled={submitting}>
          {submitting ? "Kaydediliyor…" : "Bot oluştur"}
        </Button>
      </div>
    </form>
  );
}

function Field({
  label,
  children,
}: {
  label: string;
  children: React.ReactNode;
}) {
  return (
    <div className="grid gap-1.5">
      <Label>{label}</Label>
      {children}
    </div>
  );
}
