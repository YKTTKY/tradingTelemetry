"""HTTP + WebSocket IPC server for the market engine."""

from __future__ import annotations

import asyncio
import time
from collections.abc import AsyncIterator
from contextlib import asynccontextmanager
from typing import Any

from fastapi import FastAPI, WebSocket
from fastapi.websockets import WebSocketDisconnect
from pydantic import BaseModel, Field

from market_engine.chart import ChartService
from market_engine.feed import FeedState, default_feed_state
from market_engine.publish import ConflatingHub
from market_engine.vendor import Bar, MarketDataVendor, default_vendor


class ChartInterestBody(BaseModel):
    instrument: str = Field(min_length=1)
    timeframe: str = Field(min_length=1)


def create_app(
    feed: FeedState | None = None,
    vendor: MarketDataVendor | None = None,
    conflate_interval_s: float = 0.05,
) -> FastAPI:
    """Build the ASGI app. Defaults to fake vendor mode when no vendor selected."""
    state = feed if feed is not None else default_feed_state()
    market_vendor = vendor if vendor is not None else default_vendor(state.vendor_mode)
    hub = ConflatingHub(interval_s=conflate_interval_s)

    def on_bar_update(
        instrument: str,
        timeframe: str,
        completed: list[Bar],
        bar: Bar,
    ) -> None:
        hub.note_bar_update(instrument, timeframe, completed, bar)

    charts = ChartService(vendor=market_vendor, on_bar_update=on_bar_update)

    @asynccontextmanager
    async def lifespan(app: FastAPI) -> AsyncIterator[None]:
        hub.start()
        yield
        await hub.stop()

    app = FastAPI(title="market-engine", version="0.1.0", lifespan=lifespan)
    app.state.feed = state
    app.state.charts = charts
    app.state.hub = hub
    app.state.vendor = market_vendor

    @app.get("/v1/snapshot")
    def snapshot() -> dict[str, Any]:
        feed_state: FeedState = app.state.feed
        return {"feed": feed_state.to_snapshot()}

    @app.post("/v1/chart/interest")
    def chart_interest(body: ChartInterestBody) -> dict[str, Any]:
        chart_svc: ChartService = app.state.charts
        result = chart_svc.set_interest(body.instrument, body.timeframe)
        return result.to_interest_response()

    @app.websocket("/v1/ws")
    async def websocket_endpoint(websocket: WebSocket) -> None:
        await websocket.accept()
        await hub.add_client(websocket)
        feed_state: FeedState = app.state.feed
        try:
            await hub.send(websocket, feed_state.to_ws_event())
            # Trivial live event so clients can verify the stream is alive.
            await hub.send(websocket, {"type": "heartbeat", "ts": time.time()})
            # Keep the connection open until the client disconnects.
            # Live bar_update frames are pushed by ConflatingHub independently.
            while True:
                await asyncio.sleep(1.0)
                await hub.send(websocket, {"type": "heartbeat", "ts": time.time()})
        except WebSocketDisconnect:
            return
        finally:
            await hub.remove_client(websocket)

    return app
