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
  signature_type: number;
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
        />

        {includeCreds && (
          <>
            <Separator className="my-3" />
            <div className="grid gap-3 sm:grid-cols-2">
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
          </>
        )}
      </div>
    </div>
  );
}
