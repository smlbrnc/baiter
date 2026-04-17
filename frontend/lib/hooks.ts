"use client";

import { useEffect, useRef, useState } from "react";
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
  loading: boolean;
  reload: () => void;
} {
  const [bots, setBots] = useState<BotRow[]>([]);
  const [loading, setLoading] = useState(true);

  const reload = () => {
    api
      .listBots()
      .then((b) => {
        setBots(b);
        setLoading(false);
      })
      .catch(() => setLoading(false));
  };

  useEffect(() => {
    reload();
    const id = setInterval(reload, pollMs);
    return () => clearInterval(id);
  }, [pollMs]);

  return { bots, loading, reload };
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
