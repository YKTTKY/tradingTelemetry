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
from market_engine.indicators import IndicatorService, parse_indicators_payload
from market_engine.paper import PaperBook
from market_engine.publish import ConflatingHub
from market_engine.quotes import QuoteService
from market_engine.vendor import (
    Bar,
    MarketDataVendor,
    OptionsChainResult,
    Tick,
    default_vendor,
)
from market_engine.workspace import VALID_LAYOUTS, WorkspaceStore


class ChartInterestBody(BaseModel):
    instrument: str = Field(min_length=1)
    timeframe: str = Field(min_length=1)
    chart_id: str | None = None


class WorkspaceBody(BaseModel):
    layout_mode: str = Field(min_length=1)


class WatchlistActiveBody(BaseModel):
    watchlist_id: str = Field(min_length=1)


class WatchlistSymbolBody(BaseModel):
    symbol: str = Field(min_length=1)


class WatchlistRenameBody(BaseModel):
    """Display name for the active watchlist sheet."""

    name: str = Field(min_length=1)


class IndicatorsBody(BaseModel):
    chart_id: str | None = None
    indicators: list[dict[str, Any]] = Field(default_factory=list)


class TypeStylesBody(BaseModel):
    """Per-chart indicator type styles (overlay strength, …)."""

    chart_id: str | None = None
    type_styles: dict[str, dict[str, Any]] = Field(default_factory=dict)


class PaperAccountCreateBody(BaseModel):
    """Create a local USD paper account. Omitted fields use visible desk defaults."""

    name: str | None = None
    initial_balance: float | None = None
    commission_per_fill_usd: float | None = None
    leverage_enabled: bool | None = None
    leverage_multiple: float | None = None
    asset_class_restriction: str | None = None


class PaperActiveBody(BaseModel):
    """Select which paper account is active (tests / settings; switching UX TBD)."""

    account_id: str = Field(min_length=1)


def _vendor_resolves_vix(vendor: MarketDataVendor) -> bool:
    try:
        result = vendor.fetch_history("VIX", "1D")
    except Exception:
        return False
    return bool(result.available)


def _vendor_options_chain(
    vendor: MarketDataVendor, instrument: str
) -> OptionsChainResult | None:
    """Fetch options chain when the adapter implements it; else treat as missing."""
    fetcher = getattr(vendor, "fetch_options_chain", None)
    if not callable(fetcher):
        return None
    try:
        result = fetcher(instrument)
    except Exception:
        return OptionsChainResult(instrument=instrument, available=False)
    if isinstance(result, OptionsChainResult):
        return result
    return OptionsChainResult(instrument=instrument, available=False)


def _resolve_paper_path(
    workspace_path: Path | None,
    paper_path: Path | str | None,
) -> Path | None:
    """Paper SQLite sits next to the workspace file unless an explicit path is given."""
    if paper_path is not None:
        return Path(paper_path)
    if workspace_path is not None:
        return workspace_path.parent / "paper.db"
    return None


def create_app(
    feed: FeedState | None = None,
    vendor: MarketDataVendor | None = None,
    conflate_interval_s: float = 0.05,
    workspace_path: Path | str | None = None,
    paper_path: Path | str | None = None,
) -> FastAPI:
    """Build the ASGI app. Defaults to fake vendor mode when no vendor selected."""
    state = feed if feed is not None else default_feed_state()
    market_vendor = vendor if vendor is not None else default_vendor(state.vendor_mode)
    hub = ConflatingHub(interval_s=conflate_interval_s)
    path = Path(workspace_path) if workspace_path is not None else None
    include_vix = _vendor_resolves_vix(market_vendor)
    workspace = WorkspaceStore(path=path, include_vix=include_vix)
    paper = PaperBook(path=_resolve_paper_path(path, paper_path))
    indicators = IndicatorService()

    # Restore persisted per-chart indicator configs into the hot service.
    for chart_id, configs in workspace.state.all_chart_indicator_slots():
        if configs:
            indicators.load_from_workspace(chart_id, configs)

    # ChartService is created before the bar callback closes over it.
    charts = ChartService(vendor=market_vendor)

    def publish_indicators_for_chart(chart_id: str) -> None:
        bars = charts.bars_for(chart_id)
        meta = charts.slot_meta(chart_id)
        if bars is None or meta is None:
            indicators.clear_series(chart_id)
            return
        instrument, timeframe = meta
        options = _vendor_options_chain(market_vendor, instrument)
        series = indicators.recompute(
            chart_id, bars, instrument=instrument, options=options
        )
        hub.note_indicator_update(
            {
                "chart_id": chart_id,
                "instrument": instrument,
                "timeframe": timeframe,
                "indicators": [c.to_public() for c in indicators.get_configs(chart_id)],
                "series": series,
            }
        )

    def on_bar_update(
        instrument: str,
        timeframe: str,
        completed: list[Bar],
        bar: Bar,
    ) -> None:
        hub.note_bar_update(instrument, timeframe, completed, bar)
        for chart_id in charts.chart_ids_for_series(instrument, timeframe):
            if indicators.get_configs(chart_id):
                publish_indicators_for_chart(chart_id)

    def on_quote_update(symbol: str, payload: dict[str, Any]) -> None:
        hub.note_quote_update(symbol, payload)

    def on_live_tick(tick: Tick) -> None:
        state.note_vendor_tick(tick.ts)

    charts.on_bar_update = on_bar_update
    charts.on_live_tick = on_live_tick
    quotes = QuoteService(
        vendor=market_vendor,
        on_quote_update=on_quote_update,
        on_live_tick=on_live_tick,
    )

    def sync_watchlist_quotes() -> list[dict[str, Any]]:
        """Arm quote interest for every symbol across all lists."""
        symbols = workspace.state.all_watchlist_symbols()
        return quotes.sync_symbols(symbols)

    # Warm default (or restored) membership so snapshot/WS have quote rows.
    sync_watchlist_quotes()

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
    app.state.quotes = quotes
    app.state.hub = hub
    app.state.vendor = market_vendor
    app.state.workspace = workspace
    app.state.indicators = indicators
    app.state.paper = paper

    def public_workspace() -> dict[str, Any]:
        return workspace.state.to_public()

    def watchlist_payload() -> dict[str, Any]:
        return {
            "workspace": public_workspace(),
            "quotes": sync_watchlist_quotes(),
        }

    def indicators_payload() -> dict[str, Any]:
        return indicators.public_all(workspace.state.active_chart_ids())

    def publish_paper(reason: str) -> dict[str, Any]:
        payload = paper.to_public()
        hub.note_paper_event(
            {
                "type": "paper_update",
                "reason": reason,
                "paper": payload,
            }
        )
        return payload

    def indicator_apply_response(chart_id: str) -> dict[str, Any]:
        bars = charts.bars_for(chart_id)
        meta = charts.slot_meta(chart_id)
        if bars is not None:
            instrument = meta[0] if meta is not None else ""
            options = (
                _vendor_options_chain(market_vendor, instrument) if instrument else None
            )
            series = indicators.recompute(
                chart_id, bars, instrument=instrument, options=options
            )
        else:
            series = {}
            indicators.clear_series(chart_id)
        return {
            "chart_id": chart_id,
            "indicators": [c.to_public() for c in indicators.get_configs(chart_id)],
            "series": series,
            "workspace": public_workspace(),
        }

    @app.get("/v1/snapshot")
    def snapshot() -> dict[str, Any]:
        feed_state: FeedState = app.state.feed
        return {
            "feed": feed_state.to_snapshot(),
            "workspace": public_workspace(),
            "quotes": sync_watchlist_quotes(),
            "indicators": indicators_payload(),
            "paper": paper.to_public(),
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
        active = workspace.state.active_chart_ids()
        charts.sync_active_charts(active)
        indicators.prune_series_to(active)
        return public

    @app.post("/v1/chart/interest")
    def chart_interest(body: ChartInterestBody) -> dict[str, Any]:
        chart_svc: ChartService = app.state.charts
        try:
            chart_id = workspace.state.resolve_chart_id(body.chart_id)
            workspace.state.validate_chart_id_for_layout(chart_id)
        except ValueError as exc:
            raise HTTPException(status_code=422, detail=str(exc)) from exc

        # Ensure workspace configs are loaded into the service for this chart.
        indicators.load_from_workspace(
            chart_id, workspace.state.indicators_for(chart_id)
        )

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
        configs = indicators.get_configs(chart_id)
        response["indicators"] = [c.to_public() for c in configs]
        if result.available:
            bars = charts.bars_for(chart_id) or []
            options = _vendor_options_chain(market_vendor, result.instrument)
            response["series"] = indicators.recompute(
                chart_id,
                bars,
                instrument=result.instrument,
                options=options,
            )
        else:
            indicators.clear_series(chart_id)
            response["series"] = {}
        return response

    @app.post("/v1/indicators")
    def apply_indicators(body: IndicatorsBody) -> dict[str, Any]:
        try:
            chart_id = workspace.state.resolve_chart_id(body.chart_id)
            workspace.state.validate_chart_id_for_layout(chart_id)
            configs = parse_indicators_payload(body.indicators)
        except ValueError as exc:
            raise HTTPException(status_code=422, detail=str(exc)) from exc

        indicators.set_configs(chart_id, configs)
        workspace.set_indicators(chart_id, configs)
        return indicator_apply_response(chart_id)

    @app.post("/v1/chart/type-styles")
    def set_type_styles(body: TypeStylesBody) -> dict[str, Any]:
        """Replace type styles for one chart slot (persisted with workspace)."""
        try:
            chart_id = workspace.state.resolve_chart_id(body.chart_id)
            # Allow writing primary memory while dual is active (and vice versa)
            # only for the active layout's chart_ids — match indicators apply.
            workspace.state.validate_chart_id_for_layout(chart_id)
        except ValueError as exc:
            raise HTTPException(status_code=422, detail=str(exc)) from exc
        return workspace.set_type_styles(chart_id, body.type_styles)

    @app.post("/v1/watchlist/active")
    def set_active_watchlist(body: WatchlistActiveBody) -> dict[str, Any]:
        try:
            workspace.set_active_watchlist(body.watchlist_id)
        except ValueError as exc:
            raise HTTPException(status_code=422, detail=str(exc)) from exc
        return watchlist_payload()

    @app.post("/v1/watchlist/add")
    def add_watchlist_symbol(body: WatchlistSymbolBody) -> dict[str, Any]:
        try:
            workspace.add_symbol(body.symbol)
        except ValueError as exc:
            raise HTTPException(status_code=422, detail=str(exc)) from exc
        return watchlist_payload()

    @app.post("/v1/watchlist/remove")
    def remove_watchlist_symbol(body: WatchlistSymbolBody) -> dict[str, Any]:
        try:
            workspace.remove_symbol(body.symbol)
        except ValueError as exc:
            raise HTTPException(status_code=422, detail=str(exc)) from exc
        return watchlist_payload()

    @app.post("/v1/watchlist/rename")
    def rename_active_watchlist(body: WatchlistRenameBody) -> dict[str, Any]:
        """Rename the active watchlist display name; id and membership unchanged."""
        try:
            workspace.rename_active(body.name)
        except ValueError as exc:
            raise HTTPException(status_code=422, detail=str(exc)) from exc
        return watchlist_payload()

    @app.post("/v1/paper/accounts")
    def create_paper_account(body: PaperAccountCreateBody) -> dict[str, Any]:
        try:
            paper.create_account(
                name=body.name,
                initial_balance=body.initial_balance,
                commission_per_fill_usd=body.commission_per_fill_usd,
                leverage_enabled=body.leverage_enabled,
                leverage_multiple=body.leverage_multiple,
                asset_class_restriction=body.asset_class_restriction,
            )
        except ValueError as exc:
            raise HTTPException(status_code=422, detail=str(exc)) from exc
        return publish_paper("account_created")

    @app.post("/v1/paper/active")
    def set_active_paper_account(body: PaperActiveBody) -> dict[str, Any]:
        try:
            paper.set_active(body.account_id)
        except ValueError as exc:
            raise HTTPException(status_code=422, detail=str(exc)) from exc
        return publish_paper("active_changed")

    @app.websocket("/v1/ws")
    async def websocket_endpoint(websocket: WebSocket) -> None:
        await websocket.accept()
        await hub.add_client(websocket)
        feed_state: FeedState = app.state.feed

        def heartbeat_event() -> dict[str, Any]:
            return {
                "type": "heartbeat",
                "ts": time.time(),
                "last_vendor_tick_ts": feed_state.last_vendor_tick_ts,
            }

        try:
            await hub.send(websocket, feed_state.to_ws_event())
            # Trivial live event so clients can verify the stream is alive.
            await hub.send(websocket, heartbeat_event())
            # Keep the connection open until the client disconnects.
            # Live bar_update / quote_update / indicator_update frames from hub.
            while True:
                await asyncio.sleep(1.0)
                await hub.send(websocket, heartbeat_event())
        except WebSocketDisconnect:
            return
        finally:
            await hub.remove_client(websocket)

    return app
