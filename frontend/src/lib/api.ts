import type { BotRow, CreateBotReq, LogRow, PnLSnapshot } from "./types";

const API_BASE = "/api";

async function req<T>(path: string, init?: RequestInit): Promise<T> {
  const res = await fetch(`${API_BASE}${path}`, {
    headers: { "Content-Type": "application/json" },
    ...init,
  });
  if (!res.ok) {
    const text = await res.text().catch(() => "");
    throw new Error(`${res.status} ${res.statusText}: ${text}`);
  }
  if (res.status === 204) return undefined as T;
  return (await res.json()) as T;
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
  deleteBot: (id: number) =>
    req<void>(`/bots/${id}`, { method: "DELETE" }),
  startBot: (id: number) =>
    req<void>(`/bots/${id}/start`, { method: "POST" }),
  stopBot: (id: number) =>
    req<void>(`/bots/${id}/stop`, { method: "POST" }),

  botLogs: (id: number, limit = 200) =>
    req<LogRow[]>(`/bots/${id}/logs?limit=${limit}`),
  botPnl: (id: number) => req<PnLSnapshot | null>(`/bots/${id}/pnl`),
};
