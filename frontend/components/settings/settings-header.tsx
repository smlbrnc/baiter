import { CheckCircle2, Settings2, XCircle } from "lucide-react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import type { GlobalCredentials } from "@/lib/types";
import { HEADER_RADIAL_GRADIENT } from "@/lib/ui-constants";
import { cn } from "@/lib/utils";

const SIG_LABEL: Record<number, string> = {
  0: "EOA",
  1: "POLY_PROXY",
  2: "POLY_GNOSIS_SAFE",
};

type Props = {
  current: GlobalCredentials;
  submitting: boolean;
};

/**
 * `BotFormHeader` ile aynı görsel desen: solda ikon + başlık + açıklama,
 * sağda durum + birincil aksiyon butonu (form submit). Status rozeti
 * mevcut kayıt varsa türetilmiş EOA/funder/sig_type'ı gösterir.
 */
export function SettingsHeader({ current, submitting }: Props) {
  const updatedAtLabel = current.updated_at_ms
    ? new Date(current.updated_at_ms).toLocaleString()
    : null;
  const sigLabel = SIG_LABEL[current.signature_type] ?? "?";

  return (
    <div className="from-muted/35 via-background to-background relative overflow-hidden border-b border-border/45 bg-gradient-to-br px-3 py-2.5 sm:px-4 sm:py-3">
      <div
        aria-hidden
        className="pointer-events-none absolute inset-0 z-0 opacity-[0.35]"
        style={{ backgroundImage: HEADER_RADIAL_GRADIENT }}
      />
      <div className="relative z-10 flex items-center gap-2.5 sm:gap-3">
        <div
          className={cn(
            "flex size-12 shrink-0 items-center justify-center rounded-md",
            current.has_credentials
              ? "bg-primary text-primary-foreground shadow-sm"
              : "bg-background/80 shadow-xs text-muted-foreground",
          )}
          aria-hidden
        >
          <Settings2 className="size-5" strokeWidth={1.75} />
        </div>
        <div className="min-w-0 flex-1 space-y-1 pr-2 sm:pr-3">
          <div className="flex flex-wrap items-center gap-x-2 gap-y-1">
            <h1 className="font-heading text-foreground text-lg font-semibold tracking-tight sm:text-xl">
              Ayarlar
            </h1>
            <Badge
              variant="secondary"
              className="px-1.5 py-0 text-[10px] font-normal leading-none"
            >
              Polymarket signature
            </Badge>
            {current.has_credentials ? (
              <Badge
                variant="outline"
                className="border-emerald-500/30 bg-emerald-500/10 px-1.5 py-0 text-[10px] font-normal leading-none text-emerald-600 dark:text-emerald-400"
              >
                <CheckCircle2 className="-ml-0.5 mr-1 size-3" />
                {current.signature_type} · {sigLabel}
              </Badge>
            ) : (
              <Badge
                variant="outline"
                className="border-amber-500/30 bg-amber-500/10 px-1.5 py-0 text-[10px] font-normal leading-none text-amber-600 dark:text-amber-400"
              >
                <XCircle className="-ml-0.5 mr-1 size-3" />
                Henüz kimlik yok
              </Badge>
            )}
          </div>
          <p className="text-muted-foreground max-w-2xl text-xs leading-snug sm:text-[13px]">
            <span className="line-clamp-2 sm:line-clamp-1">
              {current.has_credentials && current.poly_address ? (
                <>
                  EOA{" "}
                  <code className="font-mono">
                    {current.poly_address.slice(0, 6)}…
                    {current.poly_address.slice(-4)}
                  </code>
                  {current.funder ? (
                    <>
                      {" · funder "}
                      <code className="font-mono">
                        {current.funder.slice(0, 6)}…
                        {current.funder.slice(-4)}
                      </code>
                    </>
                  ) : null}
                  {updatedAtLabel ? ` · ${updatedAtLabel}` : null}
                </>
              ) : (
                "Önce imza tipini seç, ardından private key + (gerekirse) funder gir."
              )}
            </span>
          </p>
        </div>
        <Button
          type="submit"
          disabled={submitting}
          size="default"
          className="shadow-xs shrink-0"
        >
          {submitting ? "Kaydediliyor…" : "Türet ve kaydet"}
        </Button>
      </div>
    </div>
  );
}
