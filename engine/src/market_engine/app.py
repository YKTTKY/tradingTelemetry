"""HTTP + WebSocket IPC server for the market engine."""

from __future__ import annotations

import asyncio
import time
from typing import Any

from fastapi import FastAPI, WebSocket
from fastapi.websockets import WebSocketDisconnect

from market_engine.feed import FeedState, default_feed_state


def create_app(feed: FeedState | None = None) -> FastAPI:
    """Build the ASGI app. Defaults to fake vendor mode when no vendor selected."""
    state = feed if feed is not None else default_feed_state()
    app = FastAPI(title="market-engine", version="0.1.0")
    app.state.feed = state

    @app.get("/v1/snapshot")
    def snapshot() -> dict[str, Any]:
        feed_state: FeedState = app.state.feed
        return {"feed": feed_state.to_snapshot()}

    @app.websocket("/v1/ws")
    async def websocket_endpoint(websocket: WebSocket) -> None:
        await websocket.accept()
        feed_state: FeedState = app.state.feed
        try:
            await websocket.send_json(feed_state.to_ws_event())
            # Trivial live event so clients can verify the stream is alive.
            await websocket.send_json({"type": "heartbeat", "ts": time.time()})
            # Keep the connection open until the client disconnects.
            while True:
                await asyncio.sleep(1.0)
                await websocket.send_json({"type": "heartbeat", "ts": time.time()})
        except WebSocketDisconnect:
            return

    return app
