"use client";

import { useCallback, useEffect, useRef, useState } from "react";
import { api } from "./api";
import type { BotRow, FrontendEvent, PnLSnapshot } from "./types";

/**
 * SSE tek bir EventSource üstünden multiplexlenir; tüm component'ler
 * aynı bağlantıya abone olur. Next dev rewrite SSE body chunk'larını
 * stream'lemediği için backend'e doğrudan bağlanıyoruz (CORS açık).
 */
const SSE_URL =
  process.env.NEXT_PUBLIC_BAITER_SSE_URL ?? "http://127.0.0.1:3000/api/events";

type Listener = (ev: FrontendEvent) => void;

class EventBus {
  private src: EventSource | null = null;
  private listeners = new Set<Listener>();
  private connected = false;
  private connectedListeners = new Set<(c: boolean) => void>();

  subscribe(l: Listener): () => void {
    this.listeners.add(l);
    this.ensure();
    return () => {
      this.listeners.delete(l);
      if (this.listeners.size === 0) this.close();
    };
  }

  onConnected(cb: (c: boolean) => void): () => void {
    this.connectedListeners.add(cb);
    cb(this.connected);
    return () => {
      this.connectedListeners.delete(cb);
    };
  }

  private ensure() {
    if (this.src) return;
    const src = new EventSource(SSE_URL);
    this.src = src;
    src.onopen = () => {
      this.connected = true;
      this.connectedListeners.forEach((cb) => cb(true));
    };
    src.onerror = () => {
      this.connected = false;
      this.connectedListeners.forEach((cb) => cb(false));
    };
    src.onmessage = (msg) => {
      let ev: FrontendEvent;
      try {
        ev = JSON.parse(msg.data) as FrontendEvent;
      } catch {
        return;
      }
      this.listeners.forEach((l) => l(ev));
    };
  }

  private close() {
    if (this.src) {
      this.src.close();
      this.src = null;
      this.connected = false;
      this.connectedListeners.forEach((cb) => cb(false));
    }
  }
}

const bus: EventBus =
  (globalThis as { __baiter_bus?: EventBus }).__baiter_bus ?? new EventBus();
if (typeof globalThis !== "undefined") {
  (globalThis as { __baiter_bus?: EventBus }).__baiter_bus = bus;
}

/**
 * Backend SSE kanalına abone olan hook.
 * `filter` verilirse yalnız eşleşen event'ler callback'e iletilir.
 */
export function useEventStream(
  onEvent: (ev: FrontendEvent) => void,
  filter?: (ev: FrontendEvent) => boolean,
): { connected: boolean } {
  const [connected, setConnected] = useState(false);
  const cbRef = useRef(onEvent);
  const filterRef = useRef(filter);

  useEffect(() => {
    cbRef.current = onEvent;
    filterRef.current = filter;
  }, [onEvent, filter]);

  useEffect(() => {
    const offConn = bus.onConnected(setConnected);
    const offMsg = bus.subscribe((ev) => {
      if (filterRef.current && !filterRef.current(ev)) return;
      cbRef.current(ev);
    });
    return () => {
      offMsg();
      offConn();
    };
  }, []);

  return { connected };
}

/**
 * Bot listesini yükler; interval + visibility-aware polling ile yeniler.
 * - AbortController: eski istek uçuşta iken yeni istek başlarsa öncekini iptal eder.
 * - Visibility: sekme arka planda iken polling durur; ön plana gelince gap-fill tetiklenir.
 * - patch / remove: optimistic güncellemeler için anlık lokal state mutasyonu.
 */
export function useBots(pollMs = 2000): {
  bots: BotRow[];
  reload: () => void;
  patch: (id: number, partial: Partial<BotRow>) => void;
  remove: (id: number) => void;
} {
  const [bots, setBots] = useState<BotRow[]>([]);
  const loadRef = useRef<(() => void) | null>(null);

  const patch = useCallback((id: number, partial: Partial<BotRow>) => {
    setBots((prev) =>
      prev.map((b) => (b.id === id ? { ...b, ...partial } : b)),
    );
  }, []);

  const remove = useCallback((id: number) => {
    setBots((prev) => prev.filter((b) => b.id !== id));
  }, []);

  const reload = useCallback(() => {
    loadRef.current?.();
  }, []);

  useEffect(() => {
    let timer: ReturnType<typeof setInterval> | null = null;
    let ctrl: AbortController | null = null;

    const load = async () => {
      if (document.hidden) return;
      ctrl?.abort();
      ctrl = new AbortController();
      const { signal } = ctrl;
      try {
        const b = await api.listBots(signal);
        if (!signal.aborted) setBots(b);
      } catch (e) {
        if (e instanceof Error && e.name === "AbortError") return;
      }
    };

    loadRef.current = () => void load();

    const startTimer = () => {
      if (timer !== null) return;
      timer = setInterval(() => void load(), pollMs);
    };
    const stopTimer = () => {
      if (timer !== null) {
        clearInterval(timer);
        timer = null;
      }
    };

    const onVisibility = () => {
      if (document.hidden) {
        stopTimer();
        ctrl?.abort();
      } else {
        void load();
        startTimer();
      }
    };

    void load();
    document.addEventListener("visibilitychange", onVisibility);
    if (!document.hidden) startTimer();

    return () => {
      stopTimer();
      ctrl?.abort();
      document.removeEventListener("visibilitychange", onVisibility);
      loadRef.current = null;
    };
  }, [pollMs]);

  return { bots, reload, patch, remove };
}

/**
 * History fetch + (SSE merge | polling) için generic time-series hook.
 *
 * Mount'ta `fetchInitial(0)` ile DB geçmişini çeker; ardından `isLive` ise:
 * - `shouldAppend` verilmişse SSE'ye abone olur ve uygun event'leri append eder.
 * - `pollMs` verilmişse periyodik delta fetch ile (lastTs üzerinden) ekler.
 *
 * SSE bağlantısı kopup yeniden kurulduğunda son `ts_ms`'ten itibaren delta
 * fetch tetiklenir; sayfa yenilense bile boşluk kalmaz.
 *
 * Tip kısıtı: `T` mutlaka `ts_ms` alanına sahip olmalı.
 */
export function useHistoryStream<T extends { ts_ms: number }>(opts: {
  fetchInitial: (sinceMs: number) => Promise<T[]>;
  shouldAppend?: (ev: FrontendEvent) => T | null;
  isLive: boolean;
  filter?: (ev: FrontendEvent) => boolean;
  pollMs?: number;
  maxItems?: number;
}): T[] {
  const { fetchInitial, shouldAppend, isLive, filter, pollMs, maxItems } = opts;
  const [items, setItems] = useState<T[]>([]);
  const lastTsRef = useRef(0);
  const fetchRef = useRef(fetchInitial);
  const appendRef = useRef(shouldAppend);
  const filterRef = useRef(filter);

  useEffect(() => {
    fetchRef.current = fetchInitial;
    appendRef.current = shouldAppend;
    filterRef.current = filter;
  }, [fetchInitial, shouldAppend, filter]);

  const reload = useCallback(async (sinceMs: number) => {
    try {
      const rows = await fetchRef.current(sinceMs);
      if (sinceMs === 0) {
        lastTsRef.current = rows.length
          ? rows[rows.length - 1].ts_ms
          : 0;
        setItems(maxItems ? rows.slice(-maxItems) : rows);
        return;
      }
      if (rows.length === 0) return;
      setItems((prev) => {
        const last = prev.length ? prev[prev.length - 1].ts_ms : 0;
        const fresh = rows.filter((r) => r.ts_ms > last);
        if (fresh.length === 0) return prev;
        const next = [...prev, ...fresh];
        lastTsRef.current = next[next.length - 1].ts_ms;
        return maxItems ? next.slice(-maxItems) : next;
      });
    } catch {
      /* yut — sonraki tick toparlar */
    }
  }, []);

  useEffect(() => {
    lastTsRef.current = 0;
    reload(0);
    // fetchInitial referansı değiştiğinde (slug/botId değişimi) baştan yükle.
  }, [fetchInitial, reload]);

  useEffect(() => {
    if (!isLive || !shouldAppend) return;
    const offMsg = bus.subscribe((ev) => {
      if (filterRef.current && !filterRef.current(ev)) return;
      const append = appendRef.current;
      if (!append) return;
      const row = append(ev);
      if (!row) return;
      if (row.ts_ms <= lastTsRef.current) return;
      lastTsRef.current = row.ts_ms;
      setItems((prev) => {
        const next = [...prev, row];
        return maxItems ? next.slice(-maxItems) : next;
      });
    });
    // SSE bağlantısı kopup yeniden kurulduğunda gap-fill.
    const offConn = bus.onConnected((connected) => {
      if (connected && lastTsRef.current > 0) {
        reload(lastTsRef.current);
      }
    });
    return () => {
      offMsg();
      offConn();
    };
  }, [isLive, shouldAppend, reload]);

  useEffect(() => {
    // SSE (shouldAppend) ile polling aynı anda çalışabilir; ts_ms dedup zaten
    // duplicate satırları filtreler. SSE kesintisinde polling gap-fill sağlar.
    if (!isLive || !pollMs) return;
    let t: ReturnType<typeof setInterval> | null = null;

    const stop = () => {
      if (t !== null) {
        clearInterval(t);
        t = null;
      }
    };

    const startInterval = () => {
      stop();
      t = setInterval(() => reload(lastTsRef.current), pollMs);
    };

    const onVisibility = () => {
      if (document.hidden) {
        stop();
      } else {
        reload(lastTsRef.current);
        startInterval();
      }
    };

    document.addEventListener("visibilitychange", onVisibility);

    if (!document.hidden) startInterval();

    return () => {
      stop();
      document.removeEventListener("visibilitychange", onVisibility);
    };
  }, [isLive, pollMs, reload]);

  return items;
}

/**
 * Tek bir bot için detay + visibility-aware poll.
 * - mutate: optimistic state güncellemesi için.
 * - withPnl: false ise `/pnl` endpoint'i çağrılmaz.
 */
export function useBot(id: number | null, pollMs = 3000, withPnl = false) {
  const [bot, setBot] = useState<BotRow | null>(null);
  const [pnl, setPnl] = useState<PnLSnapshot | null>(null);

  const mutate = useCallback((partial: Partial<BotRow>) => {
    setBot((prev) => (prev ? { ...prev, ...partial } : prev));
  }, []);

  useEffect(() => {
    if (id == null) return;

    let timer: ReturnType<typeof setInterval> | null = null;
    let ctrl: AbortController | null = null;

    const load = async () => {
      if (document.hidden) return;
      ctrl?.abort();
      ctrl = new AbortController();
      const { signal } = ctrl;
      try {
        const [b, p] = await Promise.all([
          api.getBot(id, signal),
          withPnl ? api.botPnl(id, signal) : Promise.resolve(null),
        ]);
        if (!signal.aborted) {
          setBot(b);
          if (withPnl) setPnl(p);
        }
      } catch (e) {
        if (e instanceof Error && e.name === "AbortError") return;
      }
    };

    const startTimer = () => {
      if (timer !== null) return;
      timer = setInterval(() => void load(), pollMs);
    };
    const stopTimer = () => {
      if (timer !== null) {
        clearInterval(timer);
        timer = null;
      }
    };

    const onVisibility = () => {
      if (document.hidden) {
        stopTimer();
        ctrl?.abort();
      } else {
        void load();
        startTimer();
      }
    };

    void load();
    document.addEventListener("visibilitychange", onVisibility);
    if (!document.hidden) startTimer();

    return () => {
      stopTimer();
      ctrl?.abort();
      document.removeEventListener("visibilitychange", onVisibility);
    };
  }, [id, pollMs, withPnl]);

  return { bot, pnl, mutate };
}
