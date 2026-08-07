"""Engine IPC seam: chart interest yields historical bars (fake vendor)."""

from fastapi.testclient import TestClient

from market_engine.app import create_app


def test_chart_interest_spy_1d_returns_historical_ohlcv_bars():
    client = TestClient(create_app())

    response = client.post(
        "/v1/chart/interest",
        json={"instrument": "SPY", "timeframe": "1D"},
    )

    assert response.status_code == 200
    body = response.json()
    assert body["instrument"] == "SPY"
    assert body["timeframe"] == "1D"
    assert body["status"] == "ok"
    bars = body["bars"]
    assert len(bars) >= 5
    for bar in bars:
        assert set(bar) >= {"ts", "open", "high", "low", "close", "volume"}
        assert bar["high"] >= bar["low"]
        assert bar["high"] >= bar["open"]
        assert bar["high"] >= bar["close"]
        assert bar["low"] <= bar["open"]
        assert bar["low"] <= bar["close"]
        assert bar["volume"] >= 0
    # Bars ordered ascending by time
    timestamps = [b["ts"] for b in bars]
    assert timestamps == sorted(timestamps)
    # Canonical instrument id — never a :test suffix
    assert ":" not in body["instrument"]
    # Known fake-vendor fixture (independent literals, not recomputed)
    assert bars[0]["ts"] == 1_719_792_000
    assert bars[0]["close"] == 540.0
    assert bars[1]["close"] == 541.5
    assert bars[-1]["close"] == 548.0


def test_chart_interest_unavailable_instrument_has_no_bars():
    client = TestClient(create_app())

    response = client.post(
        "/v1/chart/interest",
        json={"instrument": "NOSUCH", "timeframe": "1D"},
    )

    assert response.status_code == 200
    body = response.json()
    assert body["instrument"] == "NOSUCH"
    assert body["timeframe"] == "1D"
    assert body["status"] == "unavailable"
    assert body["bars"] == []


def test_chart_interest_unavailable_timeframe_has_no_bars():
    client = TestClient(create_app())

    response = client.post(
        "/v1/chart/interest",
        json={"instrument": "SPY", "timeframe": "1m"},
    )

    assert response.status_code == 200
    body = response.json()
    assert body["status"] == "unavailable"
    assert body["bars"] == []


def test_snapshot_still_reports_fake_vendor_mode():
    client = TestClient(create_app())

    body = client.get("/v1/snapshot").json()
    assert body["feed"]["vendor_mode"] == "fake"
