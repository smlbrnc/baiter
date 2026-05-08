"use client"

import { FormEvent, useEffect, useState } from "react"
import { CheckCircle2, Save } from "lucide-react"
import { Button } from "@/components/ui/button"
import { Separator } from "@/components/ui/separator"
import { api } from "@/lib/api"
import type { BotRow, CreateBotReq, UpdateBotReq } from "@/lib/types"
import { CARD_SHELL_CLASS } from "@/lib/ui-constants"
import {
  BotFormCredentialsSection,
  EMPTY_BOT_CREDS,
  type BotCredsState,
} from "@/components/bots/bot-form-credentials-section"
import {
  BotFormNameField,
  BotFormRunModeField,
} from "@/components/bots/bot-form-fields"
import { BotFormSettingsSection } from "@/components/bots/bot-form-settings-section"
import { BotFormStrategyParamsSection } from "@/components/bots/bot-form-strategy-section"

type Props = {
  bot: BotRow
  /** Yeni bir BotRow alındığında parent'ı güncellemek için. */
  onUpdated?: (bot: BotRow) => void
}

/**
 * Bot ayarlarını düzenleme formu. Inputlar her durumda okunabilir kalır;
 * yalnızca kaydet butonu STOPPED dışındaki durumlarda pasifleşir
 * (backend de aynı kuralı uygular: PATCH 409 döner).
 */
export function BotSettingsEditForm({ bot, onUpdated }: Props) {
  const isLocked = bot.state !== "STOPPED"

  const [form, setForm] = useState<CreateBotReq>(() => botToForm(bot))
  const [includeCreds, setIncludeCreds] = useState(false)
  const [creds, setCreds] = useState<BotCredsState>(EMPTY_BOT_CREDS)
  const [submitting, setSubmitting] = useState(false)
  const [savedAt, setSavedAt] = useState<number | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [hasGlobalCreds, setHasGlobalCreds] = useState(false)

  useEffect(() => {
    let cancelled = false
    void api.settings
      .getCredentials()
      .then((data) => {
        if (!cancelled) setHasGlobalCreds(data.has_credentials)
      })
      .catch(() => {})
    return () => {
      cancelled = true
    }
  }, [])

  const onSubmit = async (e: FormEvent) => {
    e.preventDefault()
    setError(null)
    const minP = Number(form.min_price)
    const maxP = Number(form.max_price)
    if (!(minP > 0 && minP < maxP && maxP < 1)) {
      setError(
        `Geçersiz fiyat aralığı: 0 < min_price (${minP}) < max_price (${maxP}) < 1 olmalı.`
      )
      return
    }
    const cooldown = Number(form.cooldown_threshold)
    if (!(Number.isFinite(cooldown) && cooldown >= 0)) {
      setError(
        `Geçersiz cooldown_threshold (${cooldown}). 0 veya pozitif milisaniye değeri gir.`
      )
      return
    }
    if (
      form.run_mode === "live" &&
      !includeCreds &&
      !hasGlobalCreds &&
      !hasExistingLiveCreds(bot)
    ) {
      setError(
        "Live moda geçmek için Ayarlar'da global kimlik kaydet veya bu bot için ayrı kimlik gir."
      )
      return
    }
    if (includeCreds) {
      if (!creds.private_key.trim()) {
        setError("Bota özel kimlik için private key gerekli.")
        return
      }
      if (
        (creds.signature_type === 1 || creds.signature_type === 2) &&
        !creds.funder.trim()
      ) {
        setError(
          `signature_type=${creds.signature_type} için FUNDER (proxy/safe) adresi zorunludur.`
        )
        return
      }
    }
    setSubmitting(true)
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
      }
      if (includeCreds) {
        const requiresFunder =
          creds.signature_type === 1 || creds.signature_type === 2
        body.credentials = {
          private_key: creds.private_key.trim(),
          signature_type: creds.signature_type,
          funder: requiresFunder ? creds.funder.trim() : null,
          nonce: Number.isFinite(creds.nonce) ? creds.nonce : 0,
        }
      }
      const updated = await api.updateBot(bot.id, body)
      setSavedAt(Date.now())
      onUpdated?.(updated)
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
    } finally {
      setSubmitting(false)
    }
  }

  const footerHint = error ? (
    <span className="text-destructive">{error}</span>
  ) : savedAt ? (
    <span className="inline-flex items-center gap-1.5 text-emerald-600 dark:text-emerald-400">
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
  )

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

          <div className="flex flex-col gap-3 border-t border-border/50 bg-muted/40 px-4 py-3 sm:flex-row sm:items-center sm:justify-between sm:px-6">
            <div className="text-xs text-muted-foreground sm:text-sm">
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
  )
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
  }
}

/**
 * Şimdilik bot satırında "creds var mı" bilgisi yok — bu yüzden live mod
 * geçişinde optimistik davranıyoruz: kullanıcı zaten daha önce live ile
 * kaydetmişse backend hata vermez. Ek alan gelirse buraya bağlanır.
 */
function hasExistingLiveCreds(bot: BotRow): boolean {
  return bot.run_mode === "live"
}
