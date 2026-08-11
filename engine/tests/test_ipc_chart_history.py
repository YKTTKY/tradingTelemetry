"""Engine IPC seam: chart interest yields historical bars (fake vendor)."""

from fastapi.testclient import TestClient

from market_engine.app import create_app
from market_engine.vendor import FakeVendor, history_limit_for


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


def test_a2_history_caps_document_multi_day_1m():
    """Documented A2 caps: 1m supports multi-RTH-day scrub; longer TFs keep smaller caps."""
    assert history_limit_for("1m") == 3900  # ≈ 10 × 390 RTH minutes
    assert history_limit_for("1m") > 500  # raised vs prior flat 500
    assert history_limit_for("1D") == 500
    assert history_limit_for("1W") == 260


def test_chart_interest_seeded_deep_1m_returns_full_loaded_series():
    """Fake vendor serves long seeded 1m series on interest (A2 buffer for pan tests)."""
    # 800 one-minute bars — more than the old flat 500, less than the 1m cap (no trim).
    n = 800
    start_ts = 1_719_792_000
    closes = tuple(100.0 + i * 0.01 for i in range(n))
    vendor = FakeVendor(auto_ticks=False)
    vendor.seed_history(
        "SPY",
        "1m",
        closes=closes,
        start_ts=start_ts,
        period_seconds=60,
        base_volume=1_000.0,
    )
    client = TestClient(create_app(vendor=vendor))

    response = client.post(
        "/v1/chart/interest",
        json={"instrument": "SPY", "timeframe": "1m", "chart_id": "primary"},
    )

    assert response.status_code == 200
    body = response.json()
    assert body["status"] == "ok"
    assert body["timeframe"] == "1m"
    bars = body["bars"]
    assert len(bars) == n
    assert bars[0]["ts"] == start_ts
    assert bars[-1]["ts"] == start_ts + (n - 1) * 60
    assert bars[0]["close"] == 100.0
    assert bars[-1]["close"] == 100.0 + (n - 1) * 0.01
