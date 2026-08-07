"""Vendor-adapter seam + IPC: LSE adapter behind the same MarketDataVendor interface.

Default CI stays on the fake vendor. These tests inject a stub LSE client so they
never require network or credentials. Live LSE integration is gated separately.
"""

from __future__ import annotations

import os
from dataclasses import dataclass, field
from typing import Any, Callable

import pytest
from fastapi.testclient import TestClient

from market_engine.app import create_app
from market_engine.feed import default_feed_state
from market_engine.vendor import HistoryResult, Tick, default_vendor


# ---------------------------------------------------------------------------
# Stub LSE client (injectable) — no network
# ---------------------------------------------------------------------------


@dataclass
class StubLseTick:
    symbol: str
    price: float
    volume: float | None = 0.0
    timestamp: str | None = None
    bid: float | None = None
    ask: float | None = None
    name: str | None = None
    replay: bool = False


@dataclass
class StubLseClient:
    """Minimal stand-in for ``lse.LSE`` used by unit/IPC tests."""

    candles_by_key: dict[tuple[str, str], list[dict[str, Any]]] = field(
        default_factory=dict
    )
    fail_with: Exception | None = None
    candles_calls: list[tuple[str, str]] = field(default_factory=list)
    subscribed: list[str] = field(default_factory=list)
    unsubscribed: list[str] = field(default_factory=list)
    connect_calls: list[list[str] | None] = field(default_factory=list)
    disconnect_calls: int = 0
    _tick_cbs: list[Callable[[Any], None]] = field(default_factory=list)

    def candles(
        self,
        symbol: str,
        timeframe: str = "1m",
        start: str | None = None,
        end: str | None = None,
        limit: int = 5000,
        order: str = "asc",
        dataset: str | None = None,
    ) -> list[dict[str, Any]]:
        self.candles_calls.append((symbol, timeframe))
        if self.fail_with is not None:
            raise self.fail_with
        return list(self.candles_by_key.get((symbol, timeframe), []))

    def on(self, event: str, callback: Callable) -> StubLseClient:
        if event == "tick":
            self._tick_cbs.append(callback)
        return self

    def subscribe(self, symbols: list[str]) -> None:
        for s in symbols:
            if s not in self.subscribed:
                self.subscribed.append(s)

    def unsubscribe(self, symbols: list[str]) -> None:
        for s in symbols:
            self.unsubscribed.append(s)
            if s in self.subscribed:
                self.subscribed.remove(s)

    def connect(self, symbols: list[str] | None = None) -> None:
        # No-op network; record call. Live path would block.
        self.connect_calls.append(list(symbols) if symbols is not None else None)

    def disconnect(self) -> None:
        self.disconnect_calls += 1

    def emit_tick(
        self,
        symbol: str,
        price: float,
        volume: float = 0.0,
        timestamp: str | None = "2024-07-10T15:00:00Z",
    ) -> None:
        tick = StubLseTick(
            symbol=symbol, price=price, volume=volume, timestamp=timestamp
        )
        for cb in self._tick_cbs:
            cb(tick)


# Deterministic stub SPY @ 1d rows (LSE lower-case timeframe, ISO timestamps).
_STUB_SPY_1D = [
    {
        "timestamp": "2024-07-01T00:00:00Z",
        "open": 540.0,
        "high": 541.0,
        "low": 539.0,
        "close": 540.5,
        "volume": 1_000_000.0,
    },
    {
        "timestamp": "2024-07-02T00:00:00Z",
        "open": 540.5,
        "high": 542.0,
        "low": 540.0,
        "close": 541.75,
        "volume": 1_100_000.0,
    },
    {
        "timestamp": "2024-07-03T00:00:00Z",
        "open": 541.75,
        "high": 543.0,
        "low": 541.0,
        "close": 542.0,
        "volume": 1_200_000.0,
    },
]

_SPY_1D_FIRST_TS = 1_719_792_000  # 2024-07-01 00:00:00 UTC


# ---------------------------------------------------------------------------
# Factory / CLI wiring
# ---------------------------------------------------------------------------


def test_default_vendor_fake_still_default():
    v = default_vendor("fake", auto_ticks=False)
    assert type(v).__name__ == "FakeVendor"


def test_default_vendor_lse_returns_lse_vendor():
    from market_engine.vendor import LseVendor

    client = StubLseClient()
    v = default_vendor("lse", client=client)
    assert isinstance(v, LseVendor)


def test_default_vendor_unknown_raises():
    with pytest.raises(ValueError, match="unsupported vendor mode"):
        default_vendor("alpaca")


def test_snapshot_reports_lse_vendor_mode_when_selected():
    client = StubLseClient()
    from market_engine.vendor import LseVendor

    vendor = LseVendor(client=client)
    app = create_app(feed=default_feed_state("lse"), vendor=vendor)
    body = TestClient(app).get("/v1/snapshot").json()
    assert body["feed"]["vendor_mode"] == "lse"
    assert body["feed"]["status"] == "connected"
    assert body["feed"]["engine"] == "up"


def test_ws_feed_status_reports_lse_vendor_mode():
    client = StubLseClient()
    from market_engine.vendor import LseVendor

    vendor = LseVendor(client=client)
    app = create_app(feed=default_feed_state("lse"), vendor=vendor)
    with TestClient(app) as http:
        with http.websocket_connect("/v1/ws") as ws:
            first = ws.receive_json()
            assert first["type"] == "feed_status"
            assert first["vendor_mode"] == "lse"
            assert first["status"] == "connected"


def test_cli_accepts_lse_and_fake_vendor_choices():
    """Argparse surface: --vendor fake|lse; default fake."""
    from market_engine.__main__ import build_parser

    parser = build_parser()
    assert parser.parse_args([]).vendor == "fake"
    assert parser.parse_args(["--vendor", "fake"]).vendor == "fake"
    assert parser.parse_args(["--vendor", "lse"]).vendor == "lse"


def test_cli_vendor_env_override(monkeypatch):
    from market_engine.__main__ import build_parser

    monkeypatch.setenv("MARKET_ENGINE_VENDOR", "lse")
    parser = build_parser()
    assert parser.parse_args([]).vendor == "lse"
    # Explicit flag wins over env
    assert parser.parse_args(["--vendor", "fake"]).vendor == "fake"


# ---------------------------------------------------------------------------
# LseVendor history + unavailable
# ---------------------------------------------------------------------------


def test_lse_fetch_history_maps_domain_tf_and_returns_bars():
    from market_engine.vendor import LseVendor

    stub = StubLseClient(
        candles_by_key={("SPY", "1d"): _STUB_SPY_1D},
    )
    vendor = LseVendor(client=stub)

    result = vendor.fetch_history("SPY", "1D")

    assert result.available is True
    assert result.instrument == "SPY"
    assert result.timeframe == "1D"
    assert len(result.bars) == 3
    assert result.bars[0].ts == _SPY_1D_FIRST_TS
    assert result.bars[0].open == 540.0
    assert result.bars[0].close == 540.5
    assert result.bars[1].close == 541.75
    assert result.bars[-1].volume == 1_200_000.0
    # Canonical instrument — never a :test suffix
    assert ":" not in result.instrument


def test_lse_fetch_history_unavailable_when_empty_rows():
    from market_engine.vendor import LseVendor

    vendor = LseVendor(client=StubLseClient(candles_by_key={("SPY", "1d"): []}))
    result = vendor.fetch_history("SPY", "1D")
    assert result.available is False
    assert result.bars == ()


def test_lse_fetch_history_unavailable_on_client_error():
    from market_engine.vendor import LseVendor

    class Boom(Exception):
        def __init__(self):
            self.status = 404
            self.message = "not found"
            super().__init__("[404] not found")

    vendor = LseVendor(client=StubLseClient(fail_with=Boom()))
    result = vendor.fetch_history("NOSUCH", "1D")
    assert result.available is False
    assert result.bars == ()
    assert result.instrument == "NOSUCH"


def test_lse_fetch_history_rejects_non_v1_timeframe():
    from market_engine.vendor import LseVendor

    stub = StubLseClient()
    vendor = LseVendor(client=stub)
    result = vendor.fetch_history("SPY", "2D")
    assert result.available is False
    assert result.bars == ()
    # Client must not be called for invalid domain timeframes
    assert stub.candles_calls == []


def test_lse_maps_all_v1_timeframes_to_lse_resolutions():
    """Domain 1D/1W map to LSE 1d/1w; intraday strings pass through."""
    from market_engine.vendor import DOMAIN_TO_LSE_TIMEFRAME, V1_TIMEFRAMES

    assert set(DOMAIN_TO_LSE_TIMEFRAME) == set(V1_TIMEFRAMES)
    assert DOMAIN_TO_LSE_TIMEFRAME["1D"] == "1d"
    assert DOMAIN_TO_LSE_TIMEFRAME["1W"] == "1w"
    assert DOMAIN_TO_LSE_TIMEFRAME["1h"] == "1h"
    assert DOMAIN_TO_LSE_TIMEFRAME["1m"] == "1m"


# ---------------------------------------------------------------------------
# Live interest (subscribe / ticks)
# ---------------------------------------------------------------------------


def test_lse_subscribe_dispatches_domain_ticks_to_handlers():
    from market_engine.vendor import LseVendor

    stub = StubLseClient()
    vendor = LseVendor(client=stub)
    received: list[Tick] = []

    vendor.subscribe("SPY", received.append)
    assert "SPY" in stub.subscribed

    stub.emit_tick("SPY", price=550.0, volume=100.0, timestamp="2024-07-10T15:00:00Z")

    assert len(received) == 1
    assert received[0].instrument == "SPY"
    assert received[0].price == 550.0
    assert received[0].volume == 100.0
    assert received[0].ts > 0


def test_lse_unsubscribe_stops_delivery():
    from market_engine.vendor import LseVendor

    stub = StubLseClient()
    vendor = LseVendor(client=stub)
    received: list[Tick] = []
    vendor.subscribe("SPY", received.append)
    vendor.unsubscribe("SPY", received.append)
    assert "SPY" in stub.unsubscribed

    stub.emit_tick("SPY", price=551.0, volume=1.0)
    assert received == []


# ---------------------------------------------------------------------------
# IPC seam with LSE vendor (stub client)
# ---------------------------------------------------------------------------


def test_ipc_chart_interest_via_lse_vendor_returns_history():
    from market_engine.vendor import LseVendor

    stub = StubLseClient(candles_by_key={("SPY", "1d"): _STUB_SPY_1D})
    vendor = LseVendor(client=stub)
    app = create_app(feed=default_feed_state("lse"), vendor=vendor)

    with TestClient(app) as client:
        response = client.post(
            "/v1/chart/interest",
            json={"instrument": "SPY", "timeframe": "1D"},
        )
        assert response.status_code == 200
        body = response.json()
        assert body["status"] == "ok"
        assert body["instrument"] == "SPY"
        assert body["timeframe"] == "1D"
        assert len(body["bars"]) == 3
        assert body["bars"][0]["ts"] == _SPY_1D_FIRST_TS
        assert body["bars"][0]["close"] == 540.5
        assert ":" not in body["instrument"]


def test_ipc_chart_interest_via_lse_unavailable_surface():
    from market_engine.vendor import LseVendor

    stub = StubLseClient()  # no candles
    vendor = LseVendor(client=stub)
    app = create_app(feed=default_feed_state("lse"), vendor=vendor)

    with TestClient(app) as client:
        body = client.post(
            "/v1/chart/interest",
            json={"instrument": "NOSUCH", "timeframe": "1D"},
        ).json()
        assert body["status"] == "unavailable"
        assert body["bars"] == []
        assert body["instrument"] == "NOSUCH"


def test_ipc_live_bar_update_from_lse_tick():
    from market_engine.vendor import LseVendor

    stub = StubLseClient(candles_by_key={("SPY", "1d"): _STUB_SPY_1D})
    vendor = LseVendor(client=stub)
    app = create_app(
        feed=default_feed_state("lse"),
        vendor=vendor,
        conflate_interval_s=0.05,
    )

    with TestClient(app) as client:
        hist = client.post(
            "/v1/chart/interest",
            json={"instrument": "SPY", "timeframe": "1D"},
        ).json()
        assert hist["status"] == "ok"
        last = hist["bars"][-1]

        with client.websocket_connect("/v1/ws") as ws:
            # drain feed_status
            for _ in range(5):
                msg = ws.receive_json()
                if msg.get("type") == "feed_status":
                    assert msg["vendor_mode"] == "lse"
                    break

            # Tick inside the open last bar (2024-07-03)
            stub.emit_tick(
                "SPY",
                price=543.5,
                volume=500.0,
                timestamp="2024-07-03T12:00:00Z",
            )
            import time

            time.sleep(0.15)
            update = None
            for _ in range(20):
                msg = ws.receive_json()
                if msg.get("type") == "bar_update":
                    update = msg
                    break
            assert update is not None
            assert update["instrument"] == "SPY"
            assert update["timeframe"] == "1D"
            assert update["bar"]["close"] == 543.5
            assert update["bar"]["ts"] == last["ts"]


# ---------------------------------------------------------------------------
# Env-gated live LSE integration (skipped without credentials)
# ---------------------------------------------------------------------------


@pytest.mark.skipif(
    not os.environ.get("LSE_API_KEY"),
    reason="LSE_API_KEY not set; live LSE integration is credential-gated",
)
def test_live_lse_history_spy_1d():
    from market_engine.vendor import LseVendor

    vendor = LseVendor()
    result = vendor.fetch_history("SPY", "1D")
    assert isinstance(result, HistoryResult)
    assert result.available is True
    assert len(result.bars) >= 5
    assert result.bars[0].ts < result.bars[-1].ts
    for bar in result.bars:
        assert bar.high >= bar.low
        assert bar.volume >= 0


@pytest.mark.skipif(
    not os.environ.get("LSE_API_KEY"),
    reason="LSE_API_KEY not set; live LSE integration is credential-gated",
)
def test_live_lse_ipc_snapshot_and_interest():
    from market_engine.vendor import LseVendor

    vendor = LseVendor()
    app = create_app(feed=default_feed_state("lse"), vendor=vendor)
    with TestClient(app) as client:
        snap = client.get("/v1/snapshot").json()
        assert snap["feed"]["vendor_mode"] == "lse"
        body = client.post(
            "/v1/chart/interest",
            json={"instrument": "SPY", "timeframe": "1D"},
        ).json()
        assert body["status"] == "ok"
        assert len(body["bars"]) >= 5
