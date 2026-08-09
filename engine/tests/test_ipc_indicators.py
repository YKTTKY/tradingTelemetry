"""Engine IPC seam: indicator apply, limits, MA/Volume compute, restore."""

from __future__ import annotations

import time
from pathlib import Path

import pytest
from fastapi.testclient import TestClient

from market_engine.app import create_app
from market_engine.feed import default_feed_state
from market_engine.vendor import FakeVendor

_CONFLATE_S = 0.05

# Independent SMA(3) tip for SPY 1D closes (last three: 547, 546.25, 548).
_SPY_SMA3_LAST = 547.0833333333334
# Independent EMA(3) tip for SPY 1D (seeded SMA of first 3, then recursive).
_SPY_EMA3_LAST = 546.9563802083334
_SPY_LAST_VOLUME = 50_900_000.0
_SPY_BAR_COUNT = 10


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


def _receive_of_type(ws, type_name: str, max_msgs: int = 20) -> dict:
    for _ in range(max_msgs):
        msg = ws.receive_json()
        if msg.get("type") == type_name:
            return msg
    raise AssertionError(f"no {type_name!r} within {max_msgs} messages")


def test_first_chart_opens_naked_no_indicators(tmp_path: Path):
    store = tmp_path / "workspace.json"
    client, _ = _client(workspace_path=store)

    snap = client.get("/v1/snapshot").json()
    charts = snap["workspace"]["charts"]
    assert charts == [
        {
            "id": "primary",
            "instrument": "SPY",
            "timeframe": "1D",
            "indicators": [],
        }
    ]
    # No hot indicator series until configured.
    assert snap.get("indicators", {}) == {} or snap.get("indicators", {}).get(
        "primary", {}
    ).get("series", {}) == {}


def test_apply_ma_and_volume_returns_series_for_focused_chart(tmp_path: Path):
    store = tmp_path / "workspace.json"
    client, _ = _client(workspace_path=store)

    interest = client.post(
        "/v1/chart/interest",
        json={"chart_id": "primary", "instrument": "SPY", "timeframe": "1D"},
    ).json()
    assert interest["status"] == "ok"
    assert len(interest["bars"]) == _SPY_BAR_COUNT

    response = client.post(
        "/v1/indicators",
        json={
            "chart_id": "primary",
            "indicators": [
                {
                    "id": "ma3",
                    "type": "ma",
                    "enabled": True,
                    "ma_type": "sma",
                    "length": 3,
                },
                {
                    "id": "vol",
                    "type": "volume",
                    "enabled": True,
                },
            ],
        },
    )
    assert response.status_code == 200
    body = response.json()
    assert body["chart_id"] == "primary"
    assert len(body["indicators"]) == 2

    series = body["series"]
    assert "ma3" in series and "vol" in series
    ma_vals = series["ma3"]["values"]
    vol_vals = series["vol"]["values"]
    assert len(ma_vals) == _SPY_BAR_COUNT
    assert len(vol_vals) == _SPY_BAR_COUNT
    # Warm-up nulls then independent SMA(3) tip.
    assert ma_vals[0] is None
    assert ma_vals[1] is None
    assert ma_vals[-1] == _SPY_SMA3_LAST
    assert vol_vals[-1] == _SPY_LAST_VOLUME

    snap = client.get("/v1/snapshot").json()
    primary = snap["workspace"]["charts"][0]
    assert primary["indicators"][0]["type"] == "ma"
    assert primary["indicators"][0]["length"] == 3
    assert primary["indicators"][1]["type"] == "volume"
    assert "ma3" in snap["indicators"]["primary"]["series"]


def test_default_ma_stack_lengths_10_60_200(tmp_path: Path):
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
                {
                    "id": "ma10",
                    "type": "ma",
                    "enabled": True,
                    "ma_type": "sma",
                    "length": 10,
                },
                {
                    "id": "ma60",
                    "type": "ma",
                    "enabled": True,
                    "ma_type": "sma",
                    "length": 60,
                },
                {
                    "id": "ma200",
                    "type": "ma",
                    "enabled": True,
                    "ma_type": "sma",
                    "length": 200,
                },
            ],
        },
    ).json()

    lengths = [c["length"] for c in body["indicators"] if c["type"] == "ma"]
    assert lengths == [10, 60, 200]
    # 10 bars: SMA(10) defined only on last bar; longer windows stay warm-up.
    assert body["series"]["ma10"]["values"][-1] is not None
    assert body["series"]["ma60"]["values"][-1] is None
    assert body["series"]["ma200"]["values"][-1] is None


def test_ema_compute_matches_independent_fixture(tmp_path: Path):
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
                {
                    "id": "ema3",
                    "type": "ma",
                    "enabled": True,
                    "ma_type": "ema",
                    "length": 3,
                },
            ],
        },
    ).json()

    vals = body["series"]["ema3"]["values"]
    assert vals[0] is None and vals[1] is None
    assert vals[-1] == pytest.approx(_SPY_EMA3_LAST)


def test_ma_limit_three_volume_limit_one_rejected(tmp_path: Path):
    store = tmp_path / "workspace.json"
    client, _ = _client(workspace_path=store)
    client.post(
        "/v1/chart/interest",
        json={"chart_id": "primary", "instrument": "SPY", "timeframe": "1D"},
    )

    too_many_ma = client.post(
        "/v1/indicators",
        json={
            "chart_id": "primary",
            "indicators": [
                {"id": f"ma{i}", "type": "ma", "enabled": True, "ma_type": "sma", "length": i + 1}
                for i in range(4)
            ],
        },
    )
    assert too_many_ma.status_code == 422
    assert "ma" in too_many_ma.json()["detail"].lower()

    too_many_vol = client.post(
        "/v1/indicators",
        json={
            "chart_id": "primary",
            "indicators": [
                {"id": "v1", "type": "volume", "enabled": True},
                {"id": "v2", "type": "volume", "enabled": True},
            ],
        },
    )
    assert too_many_vol.status_code == 422
    assert "volume" in too_many_vol.json()["detail"].lower()

    # Reject is atomic: still naked after failed apply.
    snap = client.get("/v1/snapshot").json()["workspace"]["charts"][0]
    assert snap["indicators"] == []


def test_toggle_disabled_omits_series_values(tmp_path: Path):
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
                {
                    "id": "ma3",
                    "type": "ma",
                    "enabled": False,
                    "ma_type": "sma",
                    "length": 3,
                },
                {"id": "vol", "type": "volume", "enabled": True},
            ],
        },
    ).json()

    assert body["indicators"][0]["enabled"] is False
    assert "ma3" not in body["series"]
    assert "vol" in body["series"]


def test_dual_layout_indicators_are_independent(tmp_path: Path):
    store = tmp_path / "workspace.json"
    client, _ = _client(workspace_path=store)

    client.post("/v1/workspace", json={"layout_mode": "dual-vertical"})
    client.post(
        "/v1/chart/interest",
        json={"chart_id": "top", "instrument": "QQQ", "timeframe": "1D"},
    )
    client.post(
        "/v1/chart/interest",
        json={"chart_id": "bottom", "instrument": "SPY", "timeframe": "1D"},
    )

    client.post(
        "/v1/indicators",
        json={
            "chart_id": "top",
            "indicators": [
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
    client.post(
        "/v1/indicators",
        json={
            "chart_id": "bottom",
            "indicators": [{"id": "vol", "type": "volume", "enabled": True}],
        },
    )

    snap = client.get("/v1/snapshot").json()
    by_id = {c["id"]: c for c in snap["workspace"]["charts"]}
    assert [i["type"] for i in by_id["top"]["indicators"]] == ["ma"]
    assert [i["type"] for i in by_id["bottom"]["indicators"]] == ["volume"]
    assert "ma3" in snap["indicators"]["top"]["series"]
    assert "vol" in snap["indicators"]["bottom"]["series"]
    assert "vol" not in snap["indicators"]["top"]["series"]
    assert "ma3" not in snap["indicators"]["bottom"]["series"]


def test_indicators_restore_per_chart_after_restart(tmp_path: Path):
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
                {
                    "id": "ma10",
                    "type": "ma",
                    "enabled": True,
                    "ma_type": "ema",
                    "length": 10,
                },
                {"id": "vol", "type": "volume", "enabled": True},
            ],
        },
    )
    assert store.is_file()
    raw = store.read_text(encoding="utf-8")
    assert "ema" in raw
    assert "volume" in raw

    client2, _ = _client(workspace_path=store)
    # Restored configs appear on snapshot before re-interest.
    snap = client2.get("/v1/snapshot").json()
    primary = snap["workspace"]["charts"][0]
    assert primary["indicators"] == [
        {
            "id": "ma10",
            "type": "ma",
            "enabled": True,
            "ma_type": "ema",
            "length": 10,
        },
        {"id": "vol", "type": "volume", "enabled": True},
    ]

    # After re-interest, series recomputed from restored configs.
    client2.post(
        "/v1/chart/interest",
        json={"chart_id": "primary", "instrument": "SPY", "timeframe": "1D"},
    )
    snap2 = client2.get("/v1/snapshot").json()
    series = snap2["indicators"]["primary"]["series"]
    assert "ma10" in series and "vol" in series
    assert len(series["vol"]["values"]) == _SPY_BAR_COUNT


def test_chart_interest_includes_indicator_series_when_configured(tmp_path: Path):
    store = tmp_path / "workspace.json"
    client, _ = _client(workspace_path=store)

    client.post(
        "/v1/indicators",
        json={
            "chart_id": "primary",
            "indicators": [
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
    interest = client.post(
        "/v1/chart/interest",
        json={"chart_id": "primary", "instrument": "SPY", "timeframe": "1D"},
    ).json()

    assert interest["status"] == "ok"
    assert interest["indicators"][0]["id"] == "ma3"
    assert interest["series"]["ma3"]["values"][-1] == _SPY_SMA3_LAST


def test_live_bar_update_refreshes_indicator_series(tmp_path: Path):
    store = tmp_path / "workspace.json"
    client, vendor = _client(workspace_path=store)

    with client:
        interest = client.post(
            "/v1/chart/interest",
            json={"chart_id": "primary", "instrument": "SPY", "timeframe": "1D"},
        ).json()
        client.post(
            "/v1/indicators",
            json={
                "chart_id": "primary",
                "indicators": [
                    {
                        "id": "ma3",
                        "type": "ma",
                        "enabled": True,
                        "ma_type": "sma",
                        "length": 3,
                    },
                    {"id": "vol", "type": "volume", "enabled": True},
                ],
            },
        )

        last = interest["bars"][-1]
        with client.websocket_connect("/v1/ws") as ws:
            _receive_of_type(ws, "feed_status")
            vendor.inject_tick(
                "SPY",
                price=float(last["close"]) + 1.0,
                volume=1_000.0,
                ts=float(last["ts"] + 60),
            )
            time.sleep(_CONFLATE_S * 2.5)

            ind_msg = None
            for _ in range(25):
                msg = ws.receive_json()
                if msg.get("type") == "indicator_update" and msg.get("chart_id") == "primary":
                    ind_msg = msg
                    break
            assert ind_msg is not None
            assert ind_msg["instrument"] == "SPY"
            assert ind_msg["timeframe"] == "1D"
            # Volume tip includes the injected volume add.
            assert ind_msg["series"]["vol"]["values"][-1] == last["volume"] + 1_000.0
            # MA tip moves (new close in window).
            assert ind_msg["series"]["ma3"]["values"][-1] != _SPY_SMA3_LAST
