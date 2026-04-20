"use client";

import { FormEvent, useState } from "react";
import { CheckCircle2, Save } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Separator } from "@/components/ui/separator";
import { api } from "@/lib/api";
import type {
  BotRow,
  Credentials,
  CreateBotReq,
  UpdateBotReq,
} from "@/lib/types";
import { CARD_SHELL_CLASS } from "@/lib/ui-constants";
import { BotFormCredentialsSection } from "@/components/bots/bot-form-credentials-section";
import {
  BotFormNameField,
  BotFormRunModeField,
} from "@/components/bots/bot-form-fields";
import { BotFormSettingsSection } from "@/components/bots/bot-form-settings-section";
import { BotFormStrategyParamsSection } from "@/components/bots/bot-form-strategy-section";

type Props = {
  bot: BotRow;
  /** Yeni bir BotRow alındığında parent'ı güncellemek için. */
  onUpdated?: (bot: BotRow) => void;
};

/**
 * Bot ayarlarını düzenleme formu. Inputlar her durumda okunabilir kalır;
 * yalnızca kaydet butonu STOPPED dışındaki durumlarda pasifleşir
 * (backend de aynı kuralı uygular: PATCH 409 döner).
 */
export function BotSettingsEditForm({ bot, onUpdated }: Props) {
  const isLocked = bot.state !== "STOPPED";

  const [form, setForm] = useState<CreateBotReq>(() => botToForm(bot));
  const [includeCreds, setIncludeCreds] = useState(false);
  const [creds, setCreds] = useState<{
    poly_address: string;
    poly_api_key: string;
    poly_passphrase: string;
    poly_secret: string;
    polygon_private_key: string;
    signature_type: 0 | 1 | 2;
    funder?: string;
  }>({
    poly_address: "",
    poly_api_key: "",
    poly_passphrase: "",
    poly_secret: "",
    polygon_private_key: "",
    signature_type: 0,
  });
  const [submitting, setSubmitting] = useState(false);
  const [savedAt, setSavedAt] = useState<number | null>(null);
  const [error, setError] = useState<string | null>(null);

  const onSubmit = async (e: FormEvent) => {
    e.preventDefault();
    setError(null);
    const minP = Number(form.min_price);
    const maxP = Number(form.max_price);
    if (!(minP > 0 && minP < maxP && maxP < 1)) {
      setError(
        `Geçersiz fiyat aralığı: 0 < min_price (${minP}) < max_price (${maxP}) < 1 olmalı.`,
      );
      return;
    }
    const cooldown = Number(form.cooldown_threshold);
    if (!(Number.isFinite(cooldown) && cooldown > 0)) {
      setError(
        `Geçersiz cooldown_threshold (${cooldown}). 0'dan büyük bir milisaniye değeri gir.`,
      );
      return;
    }
    if (form.run_mode === "live" && !includeCreds && !hasExistingLiveCreds(bot)) {
      // Mevcut credentials varlığından emin değiliz; yine de kullanıcıya uyarı.
      setError(
        "Live moda geçiyorsan Polymarket kimlik bilgilerini de gir.",
      );
      return;
    }
    if (
      includeCreds &&
      (creds.signature_type === 1 || creds.signature_type === 2) &&
      !creds.funder?.trim()
    ) {
      setError(
        `signature_type=${creds.signature_type} için FUNDER (proxy/safe) adresi zorunludur.`,
      );
      return;
    }
    setSubmitting(true);
    try {
      // slug_pattern ve strategy backend tarafında immutable — gövdeye eklenmez.
      const body: UpdateBotReq = {
        name: form.name.trim() || bot.name,
        run_mode: form.run_mode,
        order_usdc: Number(form.order_usdc),
        min_price: minP,
        max_price: maxP,
        cooldown_threshold: cooldown,
        start_offset: form.start_offset,
        strategy_params: form.strategy_params,
      };
      if (includeCreds) {
        const credsBody: Credentials = {
          ...creds,
          funder: creds.funder?.trim() || null,
        };
        body.credentials = credsBody;
      }
      const updated = await api.updateBot(bot.id, body);
      setSavedAt(Date.now());
      onUpdated?.(updated);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setSubmitting(false);
    }
  };

  const footerHint = error ? (
    <span className="text-destructive">{error}</span>
  ) : savedAt ? (
    <span className="text-emerald-600 dark:text-emerald-400 inline-flex items-center gap-1.5">
      <CheckCircle2 className="size-3.5" />
      Ayarlar kaydedildi
    </span>
  ) : isLocked ? (
    <span>
      Kaydetmek için önce botu durdur (durum:{" "}
      <span className="font-mono">{bot.state}</span>).
    </span>
  ) : (
    "Kaydettiğinde bot bir sonraki başlatmada yeni ayarlarla çalışır."
  );

  return (
    <form onSubmit={onSubmit}>
      <fieldset disabled={submitting} className="contents">
        <div className={CARD_SHELL_CLASS}>
          <div className="space-y-5 px-4 py-5 sm:px-6">
            <BotFormNameField form={form} setForm={setForm} />
            <BotFormSettingsSection form={form} setForm={setForm} />
            <BotFormRunModeField form={form} setForm={setForm} />
            <BotFormStrategyParamsSection form={form} setForm={setForm} />
          </div>

          <Separator />

          <BotFormCredentialsSection
            includeCreds={includeCreds}
            setIncludeCreds={setIncludeCreds}
            creds={creds}
            setCreds={setCreds}
          />

          <div className="bg-muted/40 border-border/50 flex flex-col gap-3 border-t px-4 py-3 sm:flex-row sm:items-center sm:justify-between sm:px-6">
            <div className="text-muted-foreground text-xs sm:text-sm">
              {footerHint}
            </div>
            <Button type="submit" size="sm" disabled={isLocked || submitting}>
              <Save />
              {submitting ? "Kaydediliyor…" : "Ayarları kaydet"}
            </Button>
          </div>
        </div>
      </fieldset>
    </form>
  );
}

function botToForm(bot: BotRow): CreateBotReq {
  return {
    name: bot.name,
    slug_pattern: bot.slug_pattern,
    strategy: bot.strategy,
    run_mode: bot.run_mode,
    order_usdc: bot.order_usdc,
    min_price: bot.min_price,
    max_price: bot.max_price,
    cooldown_threshold: bot.cooldown_threshold,
    start_offset: bot.start_offset,
    strategy_params: bot.strategy_params ?? {},
  };
}

/**
 * Şimdilik bot satırında "creds var mı" bilgisi yok — bu yüzden live mod
 * geçişinde optimistik davranıyoruz: kullanıcı zaten daha önce live ile
 * kaydetmişse backend hata vermez. Ek alan gelirse buraya bağlanır.
 */
function hasExistingLiveCreds(bot: BotRow): boolean {
  return bot.run_mode === "live";
}
