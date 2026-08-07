"""Engine IPC seam: live bar updates over WebSocket with conflation (fake vendor)."""

from __future__ import annotations

import time

from fastapi.testclient import TestClient

from market_engine.app import create_app
from market_engine.feed import default_feed_state
from market_engine.vendor import FakeVendor

# Known last bar of SPY @ 1D fake history (independent fixture literals).
_SPY_1D_LAST_TS = 1_719_792_000 + 9 * 86_400  # 1_720_569_600
_SPY_1D_LAST_CLOSE = 548.0
_DAY = 86_400

# Short interval so contract tests stay fast.
_CONFLATE_S = 0.05


def _make(vendor: FakeVendor | None = None, conflate_interval_s: float = _CONFLATE_S):
    v = vendor if vendor is not None else FakeVendor(auto_ticks=False)
    app = create_app(
        feed=default_feed_state("fake"),
        vendor=v,
        conflate_interval_s=conflate_interval_s,
    )
    return app, v


def _receive_of_type(ws, type_name: str, max_msgs: int = 15) -> dict:
    for _ in range(max_msgs):
        msg = ws.receive_json()
        if msg.get("type") == type_name:
            return msg
    raise AssertionError(f"no {type_name!r} within {max_msgs} messages")


def test_live_tick_updates_last_bar_over_websocket():
    app, vendor = _make()
    with TestClient(app) as client:
        interest = client.post(
            "/v1/chart/interest",
            json={"instrument": "SPY", "timeframe": "1D"},
        )
        assert interest.status_code == 200
        assert interest.json()["status"] == "ok"
        last_hist = interest.json()["bars"][-1]
        assert last_hist["ts"] == _SPY_1D_LAST_TS
        assert last_hist["close"] == _SPY_1D_LAST_CLOSE

        with client.websocket_connect("/v1/ws") as ws:
            _receive_of_type(ws, "feed_status")

            vendor.inject_tick(
                "SPY",
                price=549.25,
                volume=10_000.0,
                ts=float(_SPY_1D_LAST_TS + 3_600),
            )
            time.sleep(_CONFLATE_S * 2.5)

            update = _receive_of_type(ws, "bar_update")
            assert update["instrument"] == "SPY"
            assert update["timeframe"] == "1D"
            assert update["completed_bars"] == []
            bar = update["bar"]
            assert bar["ts"] == _SPY_1D_LAST_TS
            assert bar["open"] == last_hist["open"]
            assert bar["close"] == 549.25
            assert bar["high"] >= 549.25
            assert bar["low"] <= min(last_hist["low"], 549.25)
            assert bar["volume"] == last_hist["volume"] + 10_000.0


def test_burst_of_ticks_produces_fewer_ws_events_than_ticks():
    app, vendor = _make()
    with TestClient(app) as client:
        client.post("/v1/chart/interest", json={"instrument": "SPY", "timeframe": "1D"})

        n_ticks = 50
        with client.websocket_connect("/v1/ws") as ws:
            _receive_of_type(ws, "feed_status")

            for i in range(n_ticks):
                vendor.inject_tick(
                    "SPY",
                    price=548.0 + i * 0.01,
                    volume=1.0,
                    ts=float(_SPY_1D_LAST_TS + 100 + i),
                )

            # Allow a few conflation windows; still far fewer events than raw ticks.
            time.sleep(_CONFLATE_S * 4)
            updates = []
            for _ in range(n_ticks + 5):
                msg = ws.receive_json()
                if msg.get("type") == "bar_update":
                    updates.append(msg)
                elif msg.get("type") == "heartbeat":
                    if updates:
                        break
                if len(updates) >= n_ticks:
                    break

            assert len(updates) >= 1
            assert len(updates) < n_ticks
            # Latest published tip reflects the final tick price.
            assert updates[-1]["bar"]["close"] == 548.0 + (n_ticks - 1) * 0.01


def test_period_roll_appends_completed_and_new_bar():
    app, vendor = _make()
    with TestClient(app) as client:
        hist = client.post(
            "/v1/chart/interest",
            json={"instrument": "SPY", "timeframe": "1D"},
        ).json()
        last_hist = hist["bars"][-1]

        new_day_ts = _SPY_1D_LAST_TS + _DAY
        with client.websocket_connect("/v1/ws") as ws:
            _receive_of_type(ws, "feed_status")

            vendor.inject_tick(
                "SPY",
                price=550.0,
                volume=5_000.0,
                ts=float(new_day_ts + 60),
            )
            time.sleep(_CONFLATE_S * 2.5)

            update = _receive_of_type(ws, "bar_update")
            assert update["instrument"] == "SPY"
            assert update["timeframe"] == "1D"
            # Previous last bar is completed as-of roll (unchanged OHLC from history
            # when no intra-day ticks landed first).
            completed = update["completed_bars"]
            assert len(completed) == 1
            assert completed[0]["ts"] == _SPY_1D_LAST_TS
            assert completed[0]["close"] == last_hist["close"]

            bar = update["bar"]
            assert bar["ts"] == new_day_ts
            assert bar["open"] == 550.0
            assert bar["high"] == 550.0
            assert bar["low"] == 550.0
            assert bar["close"] == 550.0
            assert bar["volume"] == 5_000.0


def test_no_live_updates_without_chart_interest():
    app, vendor = _make()
    with TestClient(app) as client:
        with client.websocket_connect("/v1/ws") as ws:
            _receive_of_type(ws, "feed_status")
            vendor.inject_tick(
                "SPY",
                price=600.0,
                volume=1.0,
                ts=float(_SPY_1D_LAST_TS + 10),
            )
            time.sleep(_CONFLATE_S * 2.5)
            # Next message should be heartbeat, not bar_update.
            msg = ws.receive_json()
            assert msg.get("type") != "bar_update"


def test_inject_without_ts_updates_history_tip_not_roll():
    """Sim clock is history-anchored so default live prints move the last bar."""
    app, vendor = _make()
    with TestClient(app) as client:
        hist = client.post(
            "/v1/chart/interest",
            json={"instrument": "SPY", "timeframe": "1D"},
        ).json()
        last_hist = hist["bars"][-1]

        with client.websocket_connect("/v1/ws") as ws:
            _receive_of_type(ws, "feed_status")
            vendor.inject_tick("SPY", price=549.0, volume=100.0)  # ts=None → sim clock
            time.sleep(_CONFLATE_S * 2.5)
            update = _receive_of_type(ws, "bar_update")
            assert update["completed_bars"] == []
            assert update["bar"]["ts"] == last_hist["ts"]
            assert update["bar"]["close"] == 549.0
