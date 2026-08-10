"""Engine IPC seam: optional GEX / GARCH — unavailable without inputs; success with fixtures."""

from __future__ import annotations

import math
from pathlib import Path

import pytest
from fastapi.testclient import TestClient

from market_engine.app import create_app
from market_engine.feed import default_feed_state
from market_engine.vendor import FakeVendor, OptionContract

_CONFLATE_S = 0.05

# GARCH(1,1) variance-targeting params (must match engine implementation).
_GARCH_ALPHA = 0.1
_GARCH_BETA = 0.85
# Engine requires this many closes for a "stable" estimate (SPY 1D has only 10).
_MIN_GARCH_BARS = 50

# Deterministic long close path for GARCH success (independent of engine helpers).
_GARCH_CLOSES: tuple[float, ...] = tuple(
    100.0 + 0.15 * i + (0.4 if i % 7 == 0 else -0.2 if i % 5 == 0 else 0.05)
    for i in range(60)
)
_GARCH_START_TS = 1_700_000_000
_DAY = 86_400


def _client(
    workspace_path: Path | None = None,
    vendor: FakeVendor | None = None,
) -> tuple[TestClient, FakeVendor]:
    v = vendor if vendor is not None else FakeVendor(auto_ticks=False)
    app = create_app(
        feed=default_feed_state("fake"),
        vendor=v,
        conflate_interval_s=_CONFLATE_S,
        workspace_path=workspace_path,
    )
    return TestClient(app), v


def _independent_garch_values(closes: list[float] | tuple[float, ...]) -> list[float | None]:
    """Independent GARCH(1,1) variance-targeting path for contract assertions."""
    n = len(closes)
    values: list[float | None] = [None] * n
    if n < _MIN_GARCH_BARS:
        return values
    returns = [math.log(closes[i] / closes[i - 1]) for i in range(1, n)]
    uvar = sum(r * r for r in returns) / len(returns)
    if uvar <= 0:
        return values
    omega = uvar * (1.0 - _GARCH_ALPHA - _GARCH_BETA)
    h = uvar
    for i, r in enumerate(returns, start=1):
        h = omega + _GARCH_ALPHA * (r * r) + _GARCH_BETA * h
        if h < 0:
            return [None] * n
        values[i] = math.sqrt(h)
    return values


def _independent_net_gex(
    spot: float,
    contracts: list[tuple[float, str, float, float]],
) -> float:
    """Independent net GEX: call +, put −; multiplier 100; 1% move scaling."""
    total = 0.0
    for strike, right, oi, gamma in contracts:
        sign = 1.0 if right.upper().startswith("C") else -1.0
        total += sign * oi * gamma * 100.0 * (spot**2) * 0.01
    return total


# ---------------------------------------------------------------------------
# Unavailable paths (required)
# ---------------------------------------------------------------------------


def test_gex_unavailable_without_options_data_does_not_invent_values(tmp_path: Path):
    """GEX attaches but series is unavailable when the vendor has no options chain."""
    store = tmp_path / "workspace.json"
    client, _ = _client(workspace_path=store)

    interest = client.post(
        "/v1/chart/interest",
        json={"chart_id": "primary", "instrument": "SPY", "timeframe": "1D"},
    ).json()
    assert interest["status"] == "ok"
    bar_count = len(interest["bars"])
    assert bar_count > 0

    body = client.post(
        "/v1/indicators",
        json={
            "chart_id": "primary",
            "indicators": [
                {"id": "gex1", "type": "gex", "enabled": True},
                {
                    "id": "ma3",
                    "type": "ma",
                    "enabled": True,
                    "ma_type": "sma",
                    "length": 3,
                },
            ],
        },
    )
    assert body.status_code == 200
    payload = body.json()

    # Config accepted (user can enable GEX); chart/other indicators keep working.
    assert payload["indicators"][0]["type"] == "gex"
    assert payload["indicators"][0]["enabled"] is True
    assert "ma3" in payload["series"]
    assert len(payload["series"]["ma3"]["values"]) == bar_count

    gex = payload["series"]["gex1"]
    assert gex["type"] == "gex"
    assert gex["status"] == "unavailable"
    assert gex.get("reason") in (
        "options_data_missing",
        "options_data_unavailable",
        "compute_failed",
    )
    # Never invent a numeric series or levels when unavailable.
    assert not gex.get("values")
    assert not gex.get("levels")
    assert gex.get("net_gex") is None
    assert gex.get("spot") is None


def test_garch_unavailable_with_insufficient_history_does_not_invent_values(
    tmp_path: Path,
):
    """Default SPY 1D (10 bars) is below the stable GARCH minimum → unavailable."""
    store = tmp_path / "workspace.json"
    client, _ = _client(workspace_path=store)

    interest = client.post(
        "/v1/chart/interest",
        json={"chart_id": "primary", "instrument": "SPY", "timeframe": "1D"},
    ).json()
    assert interest["status"] == "ok"
    assert len(interest["bars"]) < _MIN_GARCH_BARS

    body = client.post(
        "/v1/indicators",
        json={
            "chart_id": "primary",
            "indicators": [
                {"id": "garch1", "type": "garch", "enabled": True},
                {"id": "vol", "type": "volume", "enabled": True},
            ],
        },
    )
    assert body.status_code == 200
    payload = body.json()

    assert payload["indicators"][0]["type"] == "garch"
    assert "vol" in payload["series"]
    assert payload["series"]["vol"]["values"][-1] is not None

    garch = payload["series"]["garch1"]
    assert garch["type"] == "garch"
    assert garch["status"] == "unavailable"
    assert garch.get("reason") in (
        "insufficient_history",
        "compute_failed",
        "unstable_estimate",
    )
    # No invented volatility path.
    vals = garch.get("values")
    if vals is not None:
        assert all(v is None for v in vals) or vals == []
    assert garch.get("params") is None or "params" not in garch


def test_gex_and_garch_limits_one_each(tmp_path: Path):
    store = tmp_path / "workspace.json"
    client, _ = _client(workspace_path=store)
    client.post(
        "/v1/chart/interest",
        json={"chart_id": "primary", "instrument": "SPY", "timeframe": "1D"},
    )

    too_many_gex = client.post(
        "/v1/indicators",
        json={
            "chart_id": "primary",
            "indicators": [
                {"id": "gex1", "type": "gex", "enabled": True},
                {"id": "gex2", "type": "gex", "enabled": True},
            ],
        },
    )
    assert too_many_gex.status_code == 422
    assert "gex" in too_many_gex.json()["detail"].lower()

    too_many_garch = client.post(
        "/v1/indicators",
        json={
            "chart_id": "primary",
            "indicators": [
                {"id": "g1", "type": "garch", "enabled": True},
                {"id": "g2", "type": "garch", "enabled": True},
            ],
        },
    )
    assert too_many_garch.status_code == 422
    assert "garch" in too_many_garch.json()["detail"].lower()


def test_gex_garch_restore_configs_even_when_unavailable(tmp_path: Path):
    store = tmp_path / "workspace.json"
    client1, _ = _client(workspace_path=store)
    client1.post(
        "/v1/chart/interest",
        json={"chart_id": "primary", "instrument": "SPY", "timeframe": "1D"},
    )
    client1.post(
        "/v1/indicators",
        json={
            "chart_id": "primary",
            "indicators": [
                {"id": "gex1", "type": "gex", "enabled": True},
                {"id": "garch1", "type": "garch", "enabled": True},
            ],
        },
    )

    client2, _ = _client(workspace_path=store)
    snap = client2.get("/v1/snapshot").json()
    types = [i["type"] for i in snap["workspace"]["charts"][0]["indicators"]]
    assert types == ["gex", "garch"]

    client2.post(
        "/v1/chart/interest",
        json={"chart_id": "primary", "instrument": "SPY", "timeframe": "1D"},
    )
    snap2 = client2.get("/v1/snapshot").json()
    series = snap2["indicators"]["primary"]["series"]
    assert series["gex1"]["status"] == "unavailable"
    assert series["garch1"]["status"] == "unavailable"


# ---------------------------------------------------------------------------
# Success paths (deterministic fixtures)
# ---------------------------------------------------------------------------


def test_garch_success_with_sufficient_history(tmp_path: Path):
    store = tmp_path / "workspace.json"
    vendor = FakeVendor(auto_ticks=False)
    vendor.seed_history(
        "SPY",
        "1D",
        closes=_GARCH_CLOSES,
        start_ts=_GARCH_START_TS,
        period_seconds=_DAY,
    )
    client, _ = _client(workspace_path=store, vendor=vendor)

    interest = client.post(
        "/v1/chart/interest",
        json={"chart_id": "primary", "instrument": "SPY", "timeframe": "1D"},
    ).json()
    assert interest["status"] == "ok"
    assert len(interest["bars"]) == len(_GARCH_CLOSES)

    body = client.post(
        "/v1/indicators",
        json={
            "chart_id": "primary",
            "indicators": [{"id": "garch1", "type": "garch", "enabled": True}],
        },
    ).json()

    garch = body["series"]["garch1"]
    assert garch["type"] == "garch"
    assert garch["status"] == "ok"
    expected = _independent_garch_values(_GARCH_CLOSES)
    assert len(garch["values"]) == len(expected)
    assert garch["values"][0] is None
    assert garch["values"][-1] == pytest.approx(expected[-1])
    # Tip must be a real positive vol, not a placeholder zero series.
    assert garch["values"][-1] is not None and garch["values"][-1] > 0


def test_gex_success_with_options_fixture(tmp_path: Path):
    store = tmp_path / "workspace.json"
    vendor = FakeVendor(auto_ticks=False)
    spot = 100.0
    contracts = [
        OptionContract(strike=100.0, right="C", open_interest=1000.0, gamma=0.05),
        OptionContract(strike=100.0, right="P", open_interest=800.0, gamma=0.05),
        OptionContract(strike=105.0, right="C", open_interest=500.0, gamma=0.03),
    ]
    vendor.seed_options_chain("SPY", spot=spot, contracts=contracts)
    client, _ = _client(workspace_path=store, vendor=vendor)

    client.post(
        "/v1/chart/interest",
        json={"chart_id": "primary", "instrument": "SPY", "timeframe": "1D"},
    )

    body = client.post(
        "/v1/indicators",
        json={
            "chart_id": "primary",
            "indicators": [{"id": "gex1", "type": "gex", "enabled": True}],
        },
    ).json()

    gex = body["series"]["gex1"]
    assert gex["type"] == "gex"
    assert gex["status"] == "ok"
    expected_net = _independent_net_gex(
        spot,
        [
            (100.0, "C", 1000.0, 0.05),
            (100.0, "P", 800.0, 0.05),
            (105.0, "C", 500.0, 0.03),
        ],
    )
    assert gex["spot"] == pytest.approx(spot)
    assert gex["net_gex"] == pytest.approx(expected_net)
    assert isinstance(gex["levels"], list) and len(gex["levels"]) >= 1
    # Levels are strike aggregates; never empty invented zeros when status is ok.
    assert any(abs(float(lv["gex"])) > 0 for lv in gex["levels"])


def test_disabled_gex_garch_omit_series(tmp_path: Path):
    store = tmp_path / "workspace.json"
    client, _ = _client(workspace_path=store)
    client.post(
        "/v1/chart/interest",
        json={"chart_id": "primary", "instrument": "SPY", "timeframe": "1D"},
    )
    body = client.post(
        "/v1/indicators",
        json={
            "chart_id": "primary",
            "indicators": [
                {"id": "gex1", "type": "gex", "enabled": False},
                {"id": "garch1", "type": "garch", "enabled": False},
            ],
        },
    ).json()
    assert "gex1" not in body["series"]
    assert "garch1" not in body["series"]
