import { ShieldCheck } from "lucide-react";
import { SectionLabel } from "@/components/bots/bot-form-shared";
import { cn } from "@/lib/utils";

const OPTIONS: ReadonlyArray<{
  id: 0 | 1 | 2;
  label: string;
  hint: string;
  needsFunder: boolean;
}> = [
  {
    id: 0,
    label: "EOA",
    hint: "Doğrudan PK sahibi cüzdan",
    needsFunder: false,
  },
  {
    id: 1,
    label: "POLY_PROXY",
    hint: "Magic Link proxy cüzdanı",
    needsFunder: true,
  },
  {
    id: 2,
    label: "POLY_GNOSIS_SAFE",
    hint: "Gnosis Safe (multisig)",
    needsFunder: true,
  },
];

type Props = {
  value: 0 | 1 | 2;
  onChange: (value: 0 | 1 | 2) => void;
  /**
   * Başlık + açıklamayı kapat. `false` ise yalnızca buton grubu döner
   * (örn. başlık başka bir context'te zaten varsa).
   */
  showLabel?: boolean;
  /** Section başlığı (varsayılan "İmza tipi"). */
  title?: string;
  /** Section açıklaması (label gösterilirken). */
  description?: string;
};

/**
 * Polymarket EIP-712 imza tipi seçici — newbot stratejisi ile aynı buton-grubu
 * desenini kullanır. Tip 1/2 seçildiğinde tüketici tarafta funder alanı zorunlu
 * olur (gösterim/validasyon orada yapılır).
 */
export function SignatureTypeSelector({
  value,
  onChange,
  showLabel = true,
  title = "İmza tipi",
  description,
}: Props) {
  return (
    <div>
      {showLabel ? (
        <>
          <SectionLabel icon={ShieldCheck} title={title} />
          <p className="text-muted-foreground mt-1 text-sm">
            {description ??
              "Polymarket EIP-712 imza tipi. Tip 1 ve 2 için funder (proxy/safe) adresi zorunludur."}
          </p>
        </>
      ) : null}
      <div
        className={cn(
          "bg-muted/70 flex flex-col overflow-hidden rounded-md border border-border/40 sm:flex-row",
          showLabel && "mt-3",
        )}
        role="radiogroup"
        aria-label="İmza tipi"
      >
        <div className="flex min-h-0 min-w-0 flex-1 flex-col divide-y divide-border/35 sm:flex-row sm:divide-x sm:divide-y-0">
          {OPTIONS.map(({ id, label, hint }) => {
            const selected = value === id;
            return (
              <button
                key={id}
                type="button"
                role="radio"
                aria-checked={selected}
                onClick={() => onChange(id)}
                className={cn(
                  "flex min-h-9 flex-1 flex-col justify-center gap-0.5 px-3 py-2.5 text-left transition-colors",
                  "rounded-none focus-visible:ring-ring/50 focus-visible:ring-2 focus-visible:ring-offset-2 focus-visible:ring-offset-background focus-visible:outline-none",
                  selected
                    ? "bg-background text-foreground shadow-sm"
                    : "text-muted-foreground hover:bg-background/70 hover:text-foreground",
                )}
              >
                <span className="flex items-center gap-1.5 text-sm font-semibold tracking-tight">
                  <span className="text-muted-foreground/70 font-mono text-[11px]">
                    {id}
                  </span>
                  {label}
                </span>
                <span
                  className={cn(
                    "text-xs leading-snug",
                    selected
                      ? "text-muted-foreground"
                      : "text-muted-foreground/90",
                  )}
                >
                  {hint}
                </span>
              </button>
            );
          })}
        </div>
      </div>
    </div>
  );
}
