import { useEffect, useRef, useState } from "react";
import { api } from "./api";
import type { BotRow, FrontendEvent } from "./types";

/**
 * Backend SSE `/api/events` kanalına abone olan hook.
 * `filter` verilirse yalnız eşleşen event'ler callback'e iletilir.
 */
export function useEventStream(
  onEvent: (ev: FrontendEvent) => void,
  filter?: (ev: FrontendEvent) => boolean,
): { connected: boolean } {
  const [connected, setConnected] = useState(false);
  const cbRef = useRef(onEvent);
  const filterRef = useRef(filter);
  cbRef.current = onEvent;
  filterRef.current = filter;

  useEffect(() => {
    const src = new EventSource("/api/events");
    src.onopen = () => setConnected(true);
    src.onerror = () => setConnected(false);
    src.onmessage = (msg) => {
      try {
        const ev = JSON.parse(msg.data) as FrontendEvent;
        if (filterRef.current && !filterRef.current(ev)) return;
        cbRef.current(ev);
      } catch {
        /* yut */
      }
    };
    return () => src.close();
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
  const [pnl, setPnl] = useState<
    Awaited<ReturnType<typeof api.botPnl>> | null
  >(null);

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
