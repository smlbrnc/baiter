"use client";

import { FormEvent, useCallback, useEffect, useState } from "react";
import { useRouter } from "next/navigation";
import { Separator } from "@/components/ui/separator";
import { api } from "@/lib/api";
import type { MarketAsset, MarketInterval } from "@/lib/market";
import { ASSETS, INTERVALS, slugPattern } from "@/lib/market";
import type { CreateBotReq } from "@/lib/types";
import { CARD_SHELL_CLASS } from "@/lib/ui-constants";
import {
  DEFAULT_MARKET,
  defaultBotDisplayName,
} from "@/components/bots/bot-form-constants";
import {
  BotFormCredentialsSection,
  EMPTY_BOT_CREDS,
  type BotCredsState,
} from "@/components/bots/bot-form-credentials-section";
import { BotFormHeader } from "@/components/bots/bot-form-header";
import { BotFormMarketSection } from "@/components/bots/bot-form-market-section";
import { BotFormSettingsSection } from "@/components/bots/bot-form-settings-section";
import { BotFormStrategyParamsSection } from "@/components/bots/bot-form-strategy-section";

export function BotForm() {
  const router = useRouter();

  const [form, setForm] = useState<CreateBotReq>({
    name: "",
    slug_pattern: slugPattern(DEFAULT_MARKET.asset, DEFAULT_MARKET.interval),
    strategy: "alis",
    run_mode: "dryrun",
    order_usdc: 5,
    min_price: 0.05,
    max_price: 0.95,
    cooldown_threshold: 30000,
    start_offset: 1,
    strategy_params: {},
  });
  const [asset, setAsset] = useState<MarketAsset>(DEFAULT_MARKET.asset);
  const [interval, setInterval] = useState<MarketInterval>(
    DEFAULT_MARKET.interval,
  );
  const [marketPicked, setMarketPicked] = useState(false);

  const syncSlug = useCallback((a: MarketAsset, i: MarketInterval) => {
    setForm((f) => ({ ...f, slug_pattern: slugPattern(a, i) }));
  }, []);

  const pickAsset = (a: MarketAsset) => {
    setMarketPicked(true);
    setAsset(a);
    syncSlug(a, interval);
  };

  const pickInterval = (i: MarketInterval) => {
    setMarketPicked(true);
    setInterval(i);
    syncSlug(asset, i);
  };

  const [includeCreds, setIncludeCreds] = useState(false);
  const [creds, setCreds] = useState<BotCredsState>(EMPTY_BOT_CREDS);
  const [submitting, setSubmitting] = useState(false);
  const [hasGlobalCreds, setHasGlobalCreds] = useState(false);

  useEffect(() => {
    let cancelled = false;
    void api.settings
      .getCredentials()
      .then((data) => {
        if (!cancelled) setHasGlobalCreds(data.has_credentials);
      })
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  }, []);

  const heroAssetMeta = ASSETS.find((a) => a.id === asset) ?? ASSETS[0];
  const heroIntervalMeta =
    INTERVALS.find((i) => i.id === interval) ?? INTERVALS[0];

  const onSubmit = async (e: FormEvent) => {
    e.preventDefault();
    const minP = Number(form.min_price);
    const maxP = Number(form.max_price);
    if (!(minP > 0 && minP < maxP && maxP < 1)) {
      window.alert(
        `Geçersiz fiyat aralığı: 0 < min_price (${minP}) < max_price (${maxP}) < 1 olmalı.`,
      );
      return;
    }
    const cooldown = Number(form.cooldown_threshold);
    if (!(Number.isFinite(cooldown) && cooldown >= 0)) {
      window.alert(
        `Geçersiz cooldown_threshold (${cooldown}). 0 veya pozitif milisaniye değeri gir.`,
      );
      return;
    }
    if (form.run_mode === "live" && !includeCreds && !hasGlobalCreds) {
      window.alert(
        "Live mod için Ayarlar'da global kimlik kaydet veya bu bot için ayrı kimlik gir.",
      );
      return;
    }
    if (includeCreds) {
      if (!creds.private_key.trim()) {
        window.alert("Bota özel kimlik için private key gerekli.");
        return;
      }
      if (
        (creds.signature_type === 1 || creds.signature_type === 2) &&
        !creds.funder.trim()
      ) {
        window.alert(
          `signature_type=${creds.signature_type} için FUNDER (proxy/safe) adresi zorunludur.`,
        );
        return;
      }
    }
    setSubmitting(true);
    try {
      const nameTrim = form.name.trim();
      const assetLabel = ASSETS.find((a) => a.id === asset)?.label ?? asset;
      const resolvedName =
        nameTrim ||
        defaultBotDisplayName(assetLabel, interval, form.strategy);
      const body: CreateBotReq = {
        ...form,
        name: resolvedName,
        slug_pattern: slugPattern(asset, interval),
        order_usdc: Number(form.order_usdc),
        min_price: minP,
        max_price: maxP,
        cooldown_threshold: cooldown,
      };
      if (includeCreds) {
        const requiresFunder =
          creds.signature_type === 1 || creds.signature_type === 2;
        body.credentials = {
          private_key: creds.private_key.trim(),
          signature_type: creds.signature_type,
          funder: requiresFunder ? creds.funder.trim() : null,
          nonce: Number.isFinite(creds.nonce) ? creds.nonce : 0,
        };
      }
      const { id } = await api.createBot(body);
      router.push(`/bots/${id}`);
    } catch {
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <form onSubmit={onSubmit} className="relative">
      <div className={CARD_SHELL_CLASS}>
        <BotFormHeader
          marketPicked={marketPicked}
          heroLogoSrc={heroAssetMeta.logo}
          assetLabel={heroAssetMeta.label}
          intervalLabel={heroIntervalMeta.label}
          submitting={submitting}
        />

        <div className="px-4 py-5 sm:px-6">
          <div className="grid gap-5 lg:grid-cols-2 lg:gap-8">
            <BotFormMarketSection
              asset={asset}
              interval={interval}
              form={form}
              setForm={setForm}
              pickAsset={pickAsset}
              pickInterval={pickInterval}
            />
            <div className="space-y-5">
              <BotFormSettingsSection form={form} setForm={setForm} />
              <BotFormStrategyParamsSection form={form} setForm={setForm} />
            </div>
          </div>
        </div>

        <Separator />

        <BotFormCredentialsSection
          includeCreds={includeCreds}
          setIncludeCreds={setIncludeCreds}
          creds={creds}
          setCreds={setCreds}
        />

        <div className="bg-muted/40 border-border/50 border-t px-4 py-3 sm:px-6">
          <p className="text-muted-foreground max-w-2xl text-xs leading-relaxed sm:text-sm">
            Kaydettikten sonra bot detayından logları, grafikleri ve canlı
            metrikleri izleyebilirsin.
          </p>
        </div>
      </div>
    </form>
  );
}
