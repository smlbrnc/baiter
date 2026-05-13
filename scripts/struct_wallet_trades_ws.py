#!/usr/bin/env python3
"""Struct Polymarket WebSocket: tek cüzdanın anlık trade akışı.

Dokümantasyon:
  - https://docs.struct.to/websockets/getting-started
  - https://docs.struct.to/websockets/rooms/trades

Ortam:
  STRUCT_API_KEY   Struct API anahtarı (zorunlu; koda yazmayın)
  Proje kökündeki .env içinde STRUCT_API_KEY=... varsa otomatik okunur
  (shell’de zaten tanımlıysa shell değeri önceliklidir).

Örnek:
  export STRUCT_API_KEY='...'
  python3 scripts/struct_wallet_trades_ws.py
  python3 scripts/struct_wallet_trades_ws.py --wallet 0xabc... --status all

Hudme (ör. https://hudme.com/bots/133 ) ile kıyas için zaman damgalı satırları log dosyasına
yönlendirip bot logu ile diff/grep yapabilirsiniz.

Bağımlılık: pip install websockets
"""
from __future__ import annotations

import argparse
import asyncio
import contextlib
import json
import os
import sys
from datetime import datetime, timezone
from pathlib import Path
from urllib.parse import urlencode

try:
    import websockets
except ImportError:
    print("Eksik paket: pip install websockets", file=sys.stderr)
    sys.exit(1)

WS_BASE = "wss://api.struct.to/ws"
ROOM_ID = "polymarket_trades"
PING_INTERVAL_SEC = 30


def _load_repo_dotenv() -> None:
    """Repoda .env varsa oku; mevcut ortam değişkenlerinin üzerine yazma."""
    path = Path(__file__).resolve().parent.parent / ".env"
    if not path.is_file():
        return
    try:
        raw = path.read_text(encoding="utf-8")
    except OSError:
        return
    for line in raw.splitlines():
        line = line.strip()
        if not line or line.startswith("#"):
            continue
        if line.startswith("export "):
            line = line[7:].strip()
        if "=" not in line:
            continue
        key, _, val = line.partition("=")
        key = key.strip()
        if not key or key in os.environ:
            continue
        val = val.strip()
        if len(val) >= 2 and val[0] == val[-1] and val[0] in ('"', "'"):
            val = val[1:-1]
        os.environ[key] = val


def _iso_from_unix(ts: int | float | None) -> str:
    if ts is None:
        return "-"
    if ts > 1e12:
        ts = ts / 1000.0
    return datetime.fromtimestamp(ts, tz=timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")


def _fmt_trade(msg: dict) -> str:
    status = msg.get("status", "")
    data = msg.get("data") or {}
    tt = data.get("trade_type", "?")
    slug = data.get("slug") or "-"
    event_slug = data.get("event_slug") or "-"
    side = data.get("side", "-")
    usd = data.get("usd_amount")
    price = data.get("price")
    shares = data.get("shares_amount")
    h = data.get("hash") or data.get("id") or "-"
    trader = (data.get("trader") or {}).get("address", "-")
    taker = data.get("taker", "-")
    confirmed = data.get("confirmed_at")
    received_ms = data.get("received_at")
    t_wall = _iso_from_unix(confirmed if confirmed is not None else received_ms)

    parts = [
        f"t={t_wall}",
        f"status={status}",
        f"type={tt}",
        f"side={side}",
        f"usd={usd}",
        f"price={price}",
        f"shares={shares}",
        f"slug={slug}",
        f"event={event_slug}",
        f"trader={trader}",
        f"taker={taker}",
        f"hash={h}",
    ]
    return " ".join(str(p) for p in parts)


async def _pinger(ws) -> None:
    while True:
        await asyncio.sleep(PING_INTERVAL_SEC)
        try:
            await ws.send(json.dumps({"type": "ping"}))
        except Exception:
            break


async def run(wallet: str, status: str, raw: bool) -> None:
    key = os.environ.get("STRUCT_API_KEY", "").strip()
    if not key:
        print("STRUCT_API_KEY tanımlı değil.", file=sys.stderr)
        sys.exit(1)

    wallet = wallet.strip().lower()
    if not wallet.startswith("0x") or len(wallet) != 42:
        print("Geçersiz cüzdan (0x + 40 hex beklenir).", file=sys.stderr)
        sys.exit(1)

    q = urlencode({"api-key": key})
    uri = f"{WS_BASE}?{q}"

    async with websockets.connect(uri) as ws:
        await ws.send(
            json.dumps({"type": "join_room", "payload": {"room_id": ROOM_ID}})
        )
        await ws.send(
            json.dumps(
                {
                    "type": "room_message",
                    "payload": {
                        "room_id": ROOM_ID,
                        "message": {
                            "action": "subscribe",
                            "traders": [wallet],
                            "status": status,
                        },
                    },
                }
            )
        )

        ping_task = asyncio.create_task(_pinger(ws))
        try:
            async for raw_msg in ws:
                try:
                    msg = json.loads(raw_msg)
                except json.JSONDecodeError:
                    continue

                mtype = msg.get("type")
                if mtype == "pong":
                    continue
                if raw:
                    print(json.dumps(msg, ensure_ascii=False), flush=True)
                    continue

                if mtype == "trade_stream_subscribe_response":
                    print(
                        "[abone]",
                        json.dumps(msg.get("data"), ensure_ascii=False),
                        flush=True,
                    )
                    continue

                if mtype == "trade_stream_update":
                    print(_fmt_trade(msg), flush=True)
                    continue

                if mtype not in (None, "ping"):
                    print(f"[ws] {mtype}: {msg.get('message', msg)}", flush=True)
        finally:
            ping_task.cancel()
            with contextlib.suppress(Exception):
                await ping_task


def main() -> None:
    _load_repo_dotenv()
    ap = argparse.ArgumentParser(description="Struct WS: cüzdan trade akışı")
    ap.add_argument(
        "--wallet",
        default="0xeebde7a0e019a63e6b476eb425505b7b3e6eba30",
        help="İzlenecek cüzdan (Struct: traders filtresi, küçük harf)",
    )
    ap.add_argument(
        "--status",
        choices=("confirmed", "pending", "all"),
        default="confirmed",
        help="confirmed (varsayılan), pending veya all",
    )
    ap.add_argument(
        "--raw",
        action="store_true",
        help="Her mesajı ham JSON yazdır (debug)",
    )
    args = ap.parse_args()
    asyncio.run(run(args.wallet, args.status, args.raw))


if __name__ == "__main__":
    main()
