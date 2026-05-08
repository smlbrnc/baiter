"use client"

import { FormEvent, useCallback, useEffect, useState } from "react"
import { KeyRound } from "lucide-react"
import { Input } from "@/components/ui/input"
import { Separator } from "@/components/ui/separator"
import { Field, SectionLabel } from "@/components/bots/bot-form-shared"
import { SignatureTypeSelector } from "@/components/credentials/signature-type-selector"
import { SettingsHeader } from "@/components/settings/settings-header"
import { api } from "@/lib/api"
import type { GlobalCredentials } from "@/lib/types"
import { CARD_SHELL_CLASS } from "@/lib/ui-constants"

const EMPTY: GlobalCredentials = {
  poly_address: null,
  signature_type: 0,
  funder: null,
  has_credentials: false,
  updated_at_ms: null,
}

export default function SettingsPage() {
  const [current, setCurrent] = useState<GlobalCredentials>(EMPTY)
  const [signatureType, setSignatureType] = useState<0 | 1 | 2>(0)
  const [privateKey, setPrivateKey] = useState("")
  const [funder, setFunder] = useState("")
  const [nonce, setNonce] = useState<number>(0)
  const [submitting, setSubmitting] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [success, setSuccess] = useState<string | null>(null)

  const reload = useCallback(async () => {
    setError(null)
    try {
      const data = await api.settings.getCredentials()
      setCurrent(data)
      setSignatureType((data.signature_type as 0 | 1 | 2) ?? 0)
      setFunder(data.funder ?? "")
    } catch (e) {
      setError(e instanceof Error ? e.message : "Yükleme başarısız")
    }
  }, [])

  useEffect(() => {
    void reload()
  }, [reload])

  const requiresFunder = signatureType === 1 || signatureType === 2

  const onSubmit = async (e: FormEvent) => {
    e.preventDefault()
    setError(null)
    setSuccess(null)
    const pk = privateKey.trim()
    if (!pk) {
      setError("Private key zorunlu (0x ile başlayan 32 byte hex).")
      return
    }
    if (!pk.startsWith("0x") || pk.length !== 66) {
      setError("Private key 0x + 64 hex karakter olmalı.")
      return
    }
    if (requiresFunder && !funder.trim()) {
      setError(
        `signature_type=${signatureType} için funder (proxy/safe) adresi zorunlu.`
      )
      return
    }
    setSubmitting(true)
    try {
      await api.settings.updateCredentials({
        private_key: pk,
        signature_type: signatureType,
        funder: requiresFunder ? funder.trim() : null,
        nonce: Number.isFinite(nonce) ? nonce : 0,
      })
      setPrivateKey("")
      setSuccess("Kimlik türetildi ve kaydedildi.")
      await reload()
    } catch (e) {
      setError(e instanceof Error ? e.message : "Kaydedilemedi")
    } finally {
      setSubmitting(false)
    }
  }

  const pkPlaceholder = current.has_credentials
    ? "0x… (kayıtlı — yenilemek için tekrar gir)"
    : "0x…"

  return (
    <form onSubmit={onSubmit} className="relative">
      <div className={CARD_SHELL_CLASS}>
        <SettingsHeader current={current} submitting={submitting} />

        <div className="px-4 py-5 sm:px-6">
          <SignatureTypeSelector
            value={signatureType}
            onChange={setSignatureType}
            description="Tüm botların varsayılan kimlik tipi. Tip 1/2 için funder (proxy/safe) adresi zorunlu — backend EIP-712 ile türetip global kaydeder."
          />
        </div>

        <Separator />

        <div className="px-4 py-5 sm:px-6">
          <SectionLabel icon={KeyRound} title="Anahtarlar" />
          <p className="mt-1 text-sm text-muted-foreground">
            EOA private key + (gerekirse) funder gir. Backend{" "}
            <code>
              POLY_ADDRESS / POLY_API_KEY / POLY_SECRET / POLY_PASSPHRASE
            </code>{" "}
            türetir.
          </p>
          <div className="mt-3 flex flex-col gap-3 sm:flex-row sm:items-start">
            <div className="min-w-0 flex-1">
              <Field
                label="POLYGON_PRIVATE_KEY"
                tooltip="Polygon EOA private key. L1 ClobAuth ve emir imzalama için kullanılır; sunucuya bir kez gönderilir, geri okunmaz."
                hint="0x ile başlayan 32 byte hex (66 karakter)."
              >
                <Input
                  type="password"
                  autoComplete="off"
                  value={privateKey}
                  placeholder={pkPlaceholder}
                  onChange={(e) => setPrivateKey(e.target.value)}
                />
              </Field>
            </div>
            <div className="min-w-0 flex-1">
              {requiresFunder ? (
                <Field
                  label="FUNDER (proxy/safe adresi)"
                  tooltip="Proxy veya Gnosis Safe maker adresi. EOA bu hesap adına imzalar; emirlerde maker olarak kullanılır."
                >
                  <Input
                    value={funder}
                    placeholder="0x…"
                    onChange={(e) => setFunder(e.target.value)}
                  />
                </Field>
              ) : (
                <Field label="FUNDER" hint="EOA modunda funder gerekmez.">
                  <Input
                    value=""
                    placeholder="—"
                    disabled
                    className="opacity-60"
                  />
                </Field>
              )}
            </div>
            <div className="shrink-0">
              <Field
                label="Nonce"
                tooltip="EIP-712 nonce — Polymarket tek nonce kullanır (default 0). Aynı (EOA, nonce) çiftiyle daha önce yaratılan API key geri döner."
              >
                <Input
                  type="number"
                  inputMode="numeric"
                  value={nonce}
                  onChange={(e) => setNonce(Number(e.target.value))}
                  className="w-20 text-center tabular-nums"
                />
              </Field>
            </div>
          </div>

          {error ? (
            <div className="mt-4 rounded-md border border-destructive/20 bg-destructive/10 px-3 py-2 text-sm text-destructive">
              {error}
            </div>
          ) : null}
          {success ? (
            <div className="mt-4 rounded-md border border-emerald-500/30 bg-emerald-500/10 px-3 py-2 text-sm text-emerald-700 dark:text-emerald-300">
              {success}
            </div>
          ) : null}
        </div>

        <div className="border-t border-border/50 bg-muted/40 px-4 py-3 sm:px-6">
          <p className="max-w-2xl text-xs leading-relaxed text-muted-foreground sm:text-sm">
            Bu kimlik tüm botlar için varsayılan olarak kullanılır. Bota özel
            kimlik istiyorsan{" "}
            <a
              href="/bots/new"
              className="text-primary underline-offset-2 hover:underline"
            >
              yeni bot
            </a>{" "}
            sayfasında özel PK gir.
          </p>
        </div>
      </div>
    </form>
  )
}
