"""Conflating event hub: coalesce bar updates before WebSocket fan-out."""

from __future__ import annotations

import asyncio
import threading
from dataclasses import dataclass, field
from typing import Any

from fastapi import WebSocket
from starlette.websockets import WebSocketState

from market_engine.vendor import Bar


@dataclass
class PendingBarUpdate:
    """Latest tip + any bars completed since the last flush for one series."""

    instrument: str
    timeframe: str
    bar: Bar
    completed_bars: list[Bar] = field(default_factory=list)

    def merge(self, completed: list[Bar], bar: Bar) -> None:
        self.completed_bars.extend(completed)
        self.bar = bar

    def to_event(self) -> dict[str, Any]:
        return {
            "type": "bar_update",
            "instrument": self.instrument,
            "timeframe": self.timeframe,
            "completed_bars": [b.to_dict() for b in self.completed_bars],
            "bar": self.bar.to_dict(),
        }


@dataclass
class _Client:
    ws: WebSocket
    lock: asyncio.Lock = field(default_factory=asyncio.Lock)


class ConflatingHub:
    """Thread-safe pending map + async flush loop to connected WebSocket clients."""

    def __init__(self, interval_s: float = 0.05) -> None:
        self.interval_s = interval_s
        self._pending: dict[tuple[str, str], PendingBarUpdate] = {}
        self._lock = threading.Lock()
        self._clients: list[_Client] = []
        self._clients_lock = asyncio.Lock()
        self._task: asyncio.Task[None] | None = None

    def note_bar_update(
        self,
        instrument: str,
        timeframe: str,
        completed_bars: list[Bar],
        bar: Bar,
    ) -> None:
        key = (instrument, timeframe)
        with self._lock:
            existing = self._pending.get(key)
            if existing is None:
                self._pending[key] = PendingBarUpdate(
                    instrument=instrument,
                    timeframe=timeframe,
                    bar=bar,
                    completed_bars=list(completed_bars),
                )
            else:
                existing.merge(completed_bars, bar)

    async def add_client(self, websocket: WebSocket) -> None:
        async with self._clients_lock:
            self._clients.append(_Client(ws=websocket))

    async def remove_client(self, websocket: WebSocket) -> None:
        async with self._clients_lock:
            self._clients = [c for c in self._clients if c.ws is not websocket]

    async def send(self, websocket: WebSocket, payload: dict[str, Any]) -> None:
        """Serialize sends on a socket (heartbeat and hub share the connection)."""
        client = await self._find_client(websocket)
        if client is None:
            if websocket.client_state == WebSocketState.CONNECTED:
                await websocket.send_json(payload)
            return
        async with client.lock:
            if client.ws.client_state == WebSocketState.CONNECTED:
                await client.ws.send_json(payload)

    async def _find_client(self, websocket: WebSocket) -> _Client | None:
        async with self._clients_lock:
            for c in self._clients:
                if c.ws is websocket:
                    return c
        return None

    def start(self) -> None:
        if self._task is None or self._task.done():
            self._task = asyncio.create_task(self._run(), name="conflating-hub")

    async def stop(self) -> None:
        task = self._task
        self._task = None
        if task is not None:
            task.cancel()
            try:
                await task
            except asyncio.CancelledError:
                pass

    async def _run(self) -> None:
        try:
            while True:
                await asyncio.sleep(self.interval_s)
                await self._flush()
        except asyncio.CancelledError:
            await self._flush()
            raise

    async def _flush(self) -> None:
        with self._lock:
            batch = list(self._pending.values())
            self._pending.clear()
        if not batch:
            return
        events = [item.to_event() for item in batch]
        async with self._clients_lock:
            clients = list(self._clients)
        dead: list[WebSocket] = []
        for client in clients:
            if client.ws.client_state != WebSocketState.CONNECTED:
                dead.append(client.ws)
                continue
            try:
                async with client.lock:
                    for event in events:
                        await client.ws.send_json(event)
            except Exception:
                dead.append(client.ws)
        if dead:
            async with self._clients_lock:
                self._clients = [c for c in self._clients if c.ws not in dead]
