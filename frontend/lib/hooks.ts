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

/** Bot listesini yükler; interval ile yeniler. */
export function useBots(pollMs = 2000): {
  bots: BotRow[];
  reload: () => void;
} {
  const [bots, setBots] = useState<BotRow[]>([]);

  const reload = () => {
    api
      .listBots()
      .then((b) => {
        setBots(b);
      })
      .catch(() => {});
  };

  useEffect(() => {
    reload();
    const id = setInterval(reload, pollMs);
    return () => clearInterval(id);
  }, [pollMs]);

  return { bots, reload };
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
        setItems(rows);
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
    if (!isLive || !pollMs) return;
    let t: ReturnType<typeof setInterval> | null = null;

    const start = () => {
      reload(lastTsRef.current);
      t = setInterval(() => reload(lastTsRef.current), pollMs);
    };
    const stop = () => {
      if (t !== null) {
        clearInterval(t);
        t = null;
      }
    };

    const onVisibility = () => (document.hidden ? stop() : start());
    document.addEventListener("visibilitychange", onVisibility);

    if (!document.hidden) start();

    return () => {
      stop();
      document.removeEventListener("visibilitychange", onVisibility);
    };
  }, [isLive, pollMs, reload]);

  return items;
}

/** Tek bir bot için detay + 1 sn poll. */
export function useBot(id: number | null, pollMs = 1000) {
  const [bot, setBot] = useState<BotRow | null>(null);
  const [pnl, setPnl] = useState<PnLSnapshot | null>(null);

  useEffect(() => {
    if (id == null) return;
    let cancelled = false;
    const tick = async () => {
      try {
        const [b, p] = await Promise.all([api.getBot(id), api.botPnl(id)]);
        if (!cancelled) {
          setBot(b);
          setPnl(p);
        }
      } catch {
        /* yut */
      }
    };
    tick();
    const t = setInterval(tick, pollMs);
    return () => {
      cancelled = true;
      clearInterval(t);
    };
  }, [id, pollMs]);

  return { bot, pnl };
}
