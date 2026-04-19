import type { Dispatch, SetStateAction } from "react";
import { KeyRound } from "lucide-react";
import { Input } from "@/components/ui/input";
import { Separator } from "@/components/ui/separator";
import { cn } from "@/lib/utils";
import { Field, SectionLabel, ToggleRow } from "@/components/bots/bot-form-shared";

type Creds = {
  poly_address: string;
  poly_api_key: string;
  poly_passphrase: string;
  poly_secret: string;
  polygon_private_key: string;
  /**
   * Polymarket EIP-712 imza tipi:
   * - 0: EOA (private key direkt sahibi)
   * - 1: POLY_PROXY (funder zorunlu)
   * - 2: POLY_GNOSIS_SAFE (funder zorunlu)
   */
  signature_type: 0 | 1 | 2;
  /** signature_type ∈ {1,2} ise zorunlu — proxy/safe sahibi adres. */
  funder?: string;
};

type Props = {
  includeCreds: boolean;
  setIncludeCreds: Dispatch<SetStateAction<boolean>>;
  creds: Creds;
  setCreds: Dispatch<SetStateAction<Creds>>;
};

export function BotFormCredentialsSection({
  includeCreds,
  setIncludeCreds,
  creds,
  setCreds,
}: Props) {
  return (
    <div className="px-4 py-4 sm:px-6">
      <SectionLabel icon={KeyRound} title="Polymarket kimliği" />
      <p className="text-muted-foreground mt-1 text-sm">
        DryRun için zorunlu değil. Live öncesi adres, API anahtarı ve imza
        bilgilerini gir.
      </p>

      <div
        className={cn(
          "mt-3 rounded-md border border-border/40 p-3 transition-colors",
          includeCreds
            ? "border-primary/25 bg-primary/[0.04]"
            : "bg-muted/30 border-border/40",
        )}
      >
        <ToggleRow
          checked={includeCreds}
          onChange={setIncludeCreds}
          title="Bu bot için kimlik bilgisi gir"
          description="CLOB API ve cüzdan anahtarlarını yalnızca güvendiğin ortamda kullan."
          tooltip="Etkinleştirilirse bu bota özel Polymarket kimlik bilgileri kullanılır. Kapalıysa .env dosyasındaki varsayılan kimlik bilgileri devreye girer."
        />

        {includeCreds && (
          <>
            <Separator className="my-3" />
            <div className="grid gap-3 sm:grid-cols-2">
              <Field
                label="POLY_ADDRESS"
                tooltip="Polymarket hesabının EVM adresi (0x…). Emir imzalama ve kimlik doğrulamada kullanılır."
              >
                <Input
                  value={creds.poly_address}
                  onChange={(e) =>
                    setCreds({ ...creds, poly_address: e.target.value })
                  }
                />
              </Field>
              <Field
                label="POLY_API_KEY"
                tooltip="Polymarket CLOB L2 API anahtarı. HMAC imzalama için kullanılır; derive edilmiş veya manuel olarak alınabilir."
              >
                <Input
                  value={creds.poly_api_key}
                  onChange={(e) =>
                    setCreds({ ...creds, poly_api_key: e.target.value })
                  }
                />
              </Field>
              <Field
                label="POLY_PASSPHRASE"
                tooltip="POLY_API_KEY ile birlikte gelen passphrase. L2 kimlik doğrulama başlığında kullanılır."
              >
                <Input
                  value={creds.poly_passphrase}
                  onChange={(e) =>
                    setCreds({ ...creds, poly_passphrase: e.target.value })
                  }
                />
              </Field>
              <Field
                label="POLY_SECRET"
                tooltip="HMAC-SHA256 imzalama için kullanılan URL-safe Base64 kodlu secret. API anahtarıyla eşleşmelidir."
              >
                <Input
                  type="password"
                  value={creds.poly_secret}
                  onChange={(e) =>
                    setCreds({ ...creds, poly_secret: e.target.value })
                  }
                />
              </Field>
              <Field
                label="POLYGON_PRIVATE_KEY"
                tooltip="EIP-712 emir imzalama için kullanılan Polygon cüzdanının özel anahtarı. Asla paylaşma; yalnızca güvenli ortamda gir."
              >
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
              <Field
                label="Signature type"
                tooltip="EIP-712 imza tipi: 0 = EOA (doğrudan private key sahibi), 1 = POLY_PROXY (Magic Link proxy cüzdanı), 2 = POLY_GNOSIS_SAFE. Tip 1 ve 2 için FUNDER adresi zorunludur."
              >
                <select
                  value={creds.signature_type}
                  onChange={(e) =>
                    setCreds({
                      ...creds,
                      signature_type: Number(e.target.value) as 0 | 1 | 2,
                    })
                  }
                  className="bg-background border-input flex h-9 w-full rounded-md border px-3 py-1 text-sm shadow-sm focus-visible:ring-ring/50 focus-visible:ring-2 focus-visible:outline-none"
                >
                  <option value={0}>0 — EOA</option>
                  <option value={1}>1 — POLY_PROXY</option>
                  <option value={2}>2 — POLY_GNOSIS_SAFE</option>
                </select>
              </Field>
              {(creds.signature_type === 1 || creds.signature_type === 2) && (
                <Field
                  label="FUNDER (proxy/safe adresi)"
                  tooltip="Proxy veya Gnosis Safe tipi imzada maker adresi olarak kullanılan EVM adresi. EOA'nın güvendiği ve imza yetkisi verdiği hesap olmalıdır."
                >
                  <Input
                    value={creds.funder ?? ""}
                    placeholder="0x..."
                    onChange={(e) =>
                      setCreds({ ...creds, funder: e.target.value })
                    }
                  />
                </Field>
              )}
            </div>
          </>
        )}
      </div>
    </div>
  );
}
