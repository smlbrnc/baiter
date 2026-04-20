import type { Dispatch, SetStateAction } from "react";
import { KeyRound } from "lucide-react";
import { Input } from "@/components/ui/input";
import { Separator } from "@/components/ui/separator";
import { cn } from "@/lib/utils";
import {
  Field,
  SectionLabel,
  ToggleRow,
} from "@/components/bots/bot-form-shared";
import { SignatureTypeSelector } from "@/components/credentials/signature-type-selector";

/**
 * Bot-bazlı kimlik girişi — kullanıcı yalnızca PK + sig_type + (funder)
 * verir, backend Polymarket'ten L1 EIP-712 ile L2 credential'larını
 * türetip `bot_credentials` tablosuna kaydeder.
 */
export type BotCredsState = {
  private_key: string;
  signature_type: 0 | 1 | 2;
  funder: string;
  nonce: number;
};

export const EMPTY_BOT_CREDS: BotCredsState = {
  private_key: "",
  signature_type: 0,
  funder: "",
  nonce: 0,
};

type Props = {
  includeCreds: boolean;
  setIncludeCreds: Dispatch<SetStateAction<boolean>>;
  creds: BotCredsState;
  setCreds: Dispatch<SetStateAction<BotCredsState>>;
};

export function BotFormCredentialsSection({
  includeCreds,
  setIncludeCreds,
  creds,
  setCreds,
}: Props) {
  const requiresFunder =
    creds.signature_type === 1 || creds.signature_type === 2;

  return (
    <div className="px-4 py-4 sm:px-6">
      <SectionLabel icon={KeyRound} title="Bota özel Polymarket kimliği" />
      <p className="text-muted-foreground mt-1 text-sm">
        Yalnızca <strong>private key</strong> + <strong>signature type</strong>
        {" "}+ (gerekirse) <strong>funder</strong> gir; backend L1 EIP-712 ile{" "}
        <code>apiKey/secret/passphrase</code> türetip bu bota özel olarak
        kaydeder. Doldurmazsan{" "}
        <a
          href="/settings"
          className="text-primary underline-offset-2 hover:underline"
        >
          Ayarlar
        </a>
        'daki global kimlik kullanılır.
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
          title="Bu bot için ayrı kimlik kullan"
          description="Aktifleştirirsen aşağıdaki PK ile bu bota özel L2 credential türetilir."
          tooltip="Kapalıyken Ayarlar sayfasındaki global kimlik kullanılır. Açıkken aşağıda girdiğin PK ile bu bota özel API anahtarı türetilip bot_credentials tablosuna kaydedilir."
        />

        {includeCreds && (
          <>
            <Separator className="my-3" />

            <SignatureTypeSelector
              value={creds.signature_type}
              onChange={(v) => setCreds({ ...creds, signature_type: v })}
              showLabel={false}
            />

            <div className="mt-3 grid gap-3 sm:grid-cols-2">
              <Field
                label="POLYGON_PRIVATE_KEY"
                tooltip="Bu bota özel Polygon EOA private key. L1 ClobAuth ve emir imzalama için kullanılır; sunucuya bir kez gönderilir, tekrar geri okunmaz."
              >
                <Input
                  type="password"
                  value={creds.private_key}
                  placeholder="0x..."
                  autoComplete="off"
                  onChange={(e) =>
                    setCreds({ ...creds, private_key: e.target.value })
                  }
                />
              </Field>
              {requiresFunder && (
                <Field
                  label="FUNDER (proxy/safe adresi)"
                  tooltip="Proxy veya Gnosis Safe maker adresi. EOA bu hesap adına imzalar; emirlerde maker olarak kullanılır."
                >
                  <Input
                    value={creds.funder}
                    placeholder="0x..."
                    onChange={(e) =>
                      setCreds({ ...creds, funder: e.target.value })
                    }
                  />
                </Field>
              )}
              <Field
                label="Nonce"
                tooltip="EIP-712 nonce — Polymarket tek nonce kullanır (default 0). Aynı (EOA, nonce) çiftiyle daha önce yaratılan API key geri döner."
              >
                <Input
                  type="number"
                  inputMode="numeric"
                  value={creds.nonce}
                  onChange={(e) =>
                    setCreds({ ...creds, nonce: Number(e.target.value) })
                  }
                />
              </Field>
            </div>
          </>
        )}
      </div>
    </div>
  );
}
