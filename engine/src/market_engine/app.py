"""HTTP + WebSocket IPC server for the market engine."""

from __future__ import annotations

import asyncio
import time
from collections.abc import AsyncIterator
from contextlib import asynccontextmanager
from pathlib import Path
from typing import Any

from fastapi import FastAPI, HTTPException, WebSocket
from fastapi.websockets import WebSocketDisconnect
from pydantic import BaseModel, Field

from market_engine.chart import ChartService
from market_engine.feed import FeedState, default_feed_state
from market_engine.publish import ConflatingHub
from market_engine.vendor import Bar, MarketDataVendor, default_vendor
from market_engine.workspace import VALID_LAYOUTS, WorkspaceStore


class ChartInterestBody(BaseModel):
    instrument: str = Field(min_length=1)
    timeframe: str = Field(min_length=1)
    chart_id: str | None = None


class WorkspaceBody(BaseModel):
    layout_mode: str = Field(min_length=1)


def create_app(
    feed: FeedState | None = None,
    vendor: MarketDataVendor | None = None,
    conflate_interval_s: float = 0.05,
    workspace_path: Path | str | None = None,
) -> FastAPI:
    """Build the ASGI app. Defaults to fake vendor mode when no vendor selected."""
    state = feed if feed is not None else default_feed_state()
    market_vendor = vendor if vendor is not None else default_vendor(state.vendor_mode)
    hub = ConflatingHub(interval_s=conflate_interval_s)
    path = Path(workspace_path) if workspace_path is not None else None
    workspace = WorkspaceStore(path=path)

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
        close = getattr(market_vendor, "close", None)
        if callable(close):
            close()

    app = FastAPI(title="market-engine", version="0.1.0", lifespan=lifespan)
    app.state.feed = state
    app.state.charts = charts
    app.state.hub = hub
    app.state.vendor = market_vendor
    app.state.workspace = workspace

    def public_workspace() -> dict[str, Any]:
        return workspace.state.to_public()

    @app.get("/v1/snapshot")
    def snapshot() -> dict[str, Any]:
        feed_state: FeedState = app.state.feed
        return {
            "feed": feed_state.to_snapshot(),
            "workspace": public_workspace(),
        }

    @app.post("/v1/workspace")
    def set_workspace(body: WorkspaceBody) -> dict[str, Any]:
        if body.layout_mode not in VALID_LAYOUTS:
            raise HTTPException(
                status_code=422,
                detail=f"layout_mode must be one of {sorted(VALID_LAYOUTS)}",
            )
        public = workspace.set_layout(body.layout_mode)
        # Drop live series for charts that left the layout; TUI reloads interest.
        charts.sync_active_charts(workspace.state.active_chart_ids())
        return public

    @app.post("/v1/chart/interest")
    def chart_interest(body: ChartInterestBody) -> dict[str, Any]:
        chart_svc: ChartService = app.state.charts
        try:
            chart_id = workspace.state.resolve_chart_id(body.chart_id)
            workspace.state.validate_chart_id_for_layout(chart_id)
        except ValueError as exc:
            raise HTTPException(status_code=422, detail=str(exc)) from exc

        result = chart_svc.set_interest(
            body.instrument,
            body.timeframe,
            chart_id=chart_id,
        )
        # Persist selection even when vendor reports unavailable so restore
        # matches the trader's last choice (empty state on that pair).
        workspace.set_chart(chart_id, result.instrument, result.timeframe)
        response = result.to_interest_response()
        response["chart_id"] = chart_id
        return response

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
