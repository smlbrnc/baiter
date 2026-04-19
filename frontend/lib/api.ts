import type {
  BotRow,
  CreateBotReq,
  LogRow,
  MarketTick,
  PnLSnapshot,
  SessionDetail,
  SessionInfo,
  SessionListResponse,
  TradeRow,
  UpdateBotReq,
} from "./types";

const API_BASE = "/api";

async function req<T>(path: string, init?: RequestInit): Promise<T> {
  const res = await fetch(`${API_BASE}${path}`, {
    cache: "no-store",
    headers: { "Content-Type": "application/json" },
    ...init,
  });
  if (!res.ok) {
    const text = await res.text().catch(() => "");
    throw new Error(`${res.status} ${res.statusText}: ${text}`);
  }
  if (res.status === 204 || res.status === 205) return undefined as T;
  const text = await res.text();
  if (!text) return undefined as T;
  return JSON.parse(text) as T;
}

function historyQs(sinceMs: number, limit: number): string {
  const qs = new URLSearchParams();
  if (sinceMs > 0) qs.set("since_ms", String(sinceMs));
  qs.set("limit", String(limit));
  return qs.toString();
}

export const api = {
  health: () => req<string>("/health"),

  listBots: () => req<BotRow[]>("/bots"),
  getBot: (id: number) => req<BotRow>(`/bots/${id}`),
  createBot: (body: CreateBotReq) =>
    req<{ id: number }>("/bots", {
      method: "POST",
      body: JSON.stringify(body),
    }),
  updateBot: (id: number, body: UpdateBotReq) =>
    req<BotRow>(`/bots/${id}`, {
      method: "PATCH",
      body: JSON.stringify(body),
    }),
  deleteBot: (id: number) =>
    req<void>(`/bots/${id}`, { method: "DELETE" }),
  startBot: (id: number) =>
    req<void>(`/bots/${id}/start`, { method: "POST" }),
  stopBot: (id: number) =>
    req<void>(`/bots/${id}/stop`, { method: "POST" }),

  botLogs: (id: number, limit = 200) =>
    req<LogRow[]>(`/bots/${id}/logs?limit=${limit}`),
  botPnl: (id: number) => req<PnLSnapshot | null>(`/bots/${id}/pnl`),
  botSession: (id: number) => req<SessionInfo | null>(`/bots/${id}/session`),

  botSessions: (id: number, limit = 10, offset = 0) =>
    req<SessionListResponse>(
      `/bots/${id}/sessions?limit=${limit}&offset=${offset}`,
    ),
  sessionDetail: (id: number, slug: string) =>
    req<SessionDetail | null>(`/bots/${id}/sessions/${slug}`),
  sessionTicks: (id: number, slug: string, sinceMs = 0, limit = 2000) =>
    req<MarketTick[]>(
      `/bots/${id}/sessions/${slug}/ticks?${historyQs(sinceMs, limit)}`,
    ),
  sessionPnlHistory: (id: number, slug: string, sinceMs = 0, limit = 2000) =>
    req<PnLSnapshot[]>(
      `/bots/${id}/sessions/${slug}/pnl?${historyQs(sinceMs, limit)}`,
    ),
  sessionTrades: (id: number, slug: string, sinceMs = 0, limit = 2000) =>
    req<TradeRow[]>(
      `/bots/${id}/sessions/${slug}/trades?${historyQs(sinceMs, limit)}`,
    ),
};
