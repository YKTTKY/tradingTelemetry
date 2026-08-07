"""Engine IPC seam: instrument + timeframe selection reloads history (fake vendor)."""

from __future__ import annotations

import time

from fastapi.testclient import TestClient

from market_engine.app import create_app
from market_engine.feed import default_feed_state
from market_engine.vendor import FakeVendor

# Independent fixture literals (not recomputed from implementation).
_SPY_1D_FIRST_CLOSE = 540.0
_QQQ_1D_FIRST_CLOSE = 490.0  # SPY closes offset by -50 in fake vendor
_SPY_1H_FIRST_TS = 1_719_792_000
_SPY_1H_FIRST_CLOSE = 540.0
_SPY_1H_LAST_CLOSE = 541.8
_SPY_1H_BAR_COUNT = 10

_CONFLATE_S = 0.05

# Full v1 timeframe set from product domain.
V1_TIMEFRAMES = ("1m", "3m", "5m", "15m", "30m", "1h", "4h", "1D", "1W")


def _client(vendor: FakeVendor | None = None) -> tuple[TestClient, FakeVendor]:
    v = vendor if vendor is not None else FakeVendor(auto_ticks=False)
    app = create_app(
        feed=default_feed_state("fake"),
        vendor=v,
        conflate_interval_s=_CONFLATE_S,
    )
    return TestClient(app), v


def _receive_of_type(ws, type_name: str, max_msgs: int = 15) -> dict:
    for _ in range(max_msgs):
        msg = ws.receive_json()
        if msg.get("type") == type_name:
            return msg
    raise AssertionError(f"no {type_name!r} within {max_msgs} messages")


def test_chart_interest_qqq_1d_returns_distinct_history():
    client, _ = _client()

    response = client.post(
        "/v1/chart/interest",
        json={"instrument": "QQQ", "timeframe": "1D"},
    )

    assert response.status_code == 200
    body = response.json()
    assert body["instrument"] == "QQQ"
    assert body["timeframe"] == "1D"
    assert body["status"] == "ok"
    bars = body["bars"]
    assert len(bars) >= 5
    assert bars[0]["close"] == _QQQ_1D_FIRST_CLOSE
    assert bars[0]["close"] != _SPY_1D_FIRST_CLOSE


def test_chart_interest_spy_1h_returns_intraday_history():
    client, _ = _client()

    response = client.post(
        "/v1/chart/interest",
        json={"instrument": "SPY", "timeframe": "1h"},
    )

    assert response.status_code == 200
    body = response.json()
    assert body["instrument"] == "SPY"
    assert body["timeframe"] == "1h"
    assert body["status"] == "ok"
    bars = body["bars"]
    assert len(bars) == _SPY_1H_BAR_COUNT
    assert bars[0]["ts"] == _SPY_1H_FIRST_TS
    assert bars[0]["close"] == _SPY_1H_FIRST_CLOSE
    assert bars[-1]["close"] == _SPY_1H_LAST_CLOSE
    # 1h bars are 3600s apart
    assert bars[1]["ts"] - bars[0]["ts"] == 3600


def test_changing_instrument_reloads_history_for_new_pair():
    client, _ = _client()

    first = client.post(
        "/v1/chart/interest",
        json={"instrument": "SPY", "timeframe": "1D"},
    ).json()
    assert first["status"] == "ok"
    assert first["bars"][0]["close"] == _SPY_1D_FIRST_CLOSE

    second = client.post(
        "/v1/chart/interest",
        json={"instrument": "QQQ", "timeframe": "1D"},
    ).json()
    assert second["status"] == "ok"
    assert second["instrument"] == "QQQ"
    assert second["bars"][0]["close"] == _QQQ_1D_FIRST_CLOSE
    # Distinct series — not the previous SPY payload reused
    assert second["bars"][0]["close"] != first["bars"][0]["close"]


def test_changing_timeframe_reloads_history_for_new_pair():
    client, _ = _client()

    daily = client.post(
        "/v1/chart/interest",
        json={"instrument": "SPY", "timeframe": "1D"},
    ).json()
    assert daily["status"] == "ok"
    daily_count = len(daily["bars"])

    hourly = client.post(
        "/v1/chart/interest",
        json={"instrument": "SPY", "timeframe": "1h"},
    ).json()
    assert hourly["status"] == "ok"
    assert hourly["timeframe"] == "1h"
    assert len(hourly["bars"]) == _SPY_1H_BAR_COUNT
    assert len(hourly["bars"]) != daily_count or hourly["bars"][0]["ts"] == daily["bars"][0]["ts"]
    # Hourly series tip differs from daily tip fixture
    assert hourly["bars"][-1]["close"] == _SPY_1H_LAST_CLOSE


def test_unsupported_timeframe_outside_v1_set_is_unavailable():
    client, _ = _client()

    response = client.post(
        "/v1/chart/interest",
        json={"instrument": "SPY", "timeframe": "2D"},
    )

    assert response.status_code == 200
    body = response.json()
    assert body["status"] == "unavailable"
    assert body["bars"] == []
    assert body["timeframe"] == "2D"


def test_instrument_is_canonicalized_to_uppercase():
    client, _ = _client()

    body = client.post(
        "/v1/chart/interest",
        json={"instrument": "spy", "timeframe": "1D"},
    ).json()
    assert body["status"] == "ok"
    assert body["instrument"] == "SPY"
    assert body["bars"][0]["close"] == _SPY_1D_FIRST_CLOSE


def test_all_v1_timeframes_are_accepted_by_contract():
    """Engine accepts every v1 timeframe string (available or explicit unavailable)."""
    client, _ = _client()

    for tf in V1_TIMEFRAMES:
        response = client.post(
            "/v1/chart/interest",
            json={"instrument": "SPY", "timeframe": tf},
        )
        assert response.status_code == 200, tf
        body = response.json()
        assert body["timeframe"] == tf
        assert body["status"] in ("ok", "unavailable")
        if body["status"] == "unavailable":
            assert body["bars"] == []
        else:
            assert len(body["bars"]) >= 1


def test_live_updates_follow_new_instrument_after_switch():
    client, vendor = _client()
    with client:
        assert (
            client.post(
                "/v1/chart/interest",
                json={"instrument": "SPY", "timeframe": "1D"},
            ).json()["status"]
            == "ok"
        )
        qqq = client.post(
            "/v1/chart/interest",
            json={"instrument": "QQQ", "timeframe": "1D"},
        ).json()
        assert qqq["status"] == "ok"
        last_ts = qqq["bars"][-1]["ts"]
        last_close = qqq["bars"][-1]["close"]

        with client.websocket_connect("/v1/ws") as ws:
            _receive_of_type(ws, "feed_status")
            vendor.inject_tick(
                "QQQ",
                price=last_close + 1.5,
                volume=2_000.0,
                ts=float(last_ts + 3_600),
            )
            time.sleep(_CONFLATE_S * 2.5)
            update = _receive_of_type(ws, "bar_update")
            assert update["instrument"] == "QQQ"
            assert update["timeframe"] == "1D"
            assert update["bar"]["close"] == last_close + 1.5


def test_live_updates_follow_new_timeframe_after_switch():
    client, vendor = _client()
    with client:
        client.post("/v1/chart/interest", json={"instrument": "SPY", "timeframe": "1D"})
        hourly = client.post(
            "/v1/chart/interest",
            json={"instrument": "SPY", "timeframe": "1h"},
        ).json()
        assert hourly["status"] == "ok"
        last_ts = hourly["bars"][-1]["ts"]
        last_close = hourly["bars"][-1]["close"]

        with client.websocket_connect("/v1/ws") as ws:
            _receive_of_type(ws, "feed_status")
            vendor.inject_tick(
                "SPY",
                price=last_close + 0.25,
                volume=500.0,
                ts=float(last_ts + 60),
            )
            time.sleep(_CONFLATE_S * 2.5)
            update = _receive_of_type(ws, "bar_update")
            assert update["instrument"] == "SPY"
            assert update["timeframe"] == "1h"
            assert update["bar"]["ts"] == last_ts
            assert update["bar"]["close"] == last_close + 0.25
