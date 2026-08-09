"""Engine IPC seam: Fixed Range Volume Profile apply, limits, extend on/off, restore."""

from __future__ import annotations

from pathlib import Path

import pytest
from fastapi.testclient import TestClient

from market_engine.app import create_app
from market_engine.feed import default_feed_state
from market_engine.vendor import Bar, FakeVendor

_CONFLATE_S = 0.05

# America/New_York wall times (independent of engine helpers).
# 2024-07-01 10:00 / 10:01 / … / 10:04 ET — in-range fixture (same VP math as Session VP).
_T0 = 1_719_842_400  # 2024-07-01 10:00 ET
_T1 = _T0 + 60
_T2 = _T0 + 120
_T3 = _T0 + 180
_T4 = _T0 + 240
# End anchor for closed window (just after last in-range bar).
_RANGE_END = _T0 + 300  # 2024-07-01 10:05 ET
# Bar past the end anchor (used only when extend_to_right is on).
_T_PAST = _T0 + 600  # 2024-07-01 10:10 ET
# Bar before start (must never contribute).
_T_BEFORE = _T0 - 600  # 2024-07-01 09:50 ET

# Worked VP example (rows=4, prices 100–104, VA 70%) from in-range bars only:
# volumes by row: [100, 500, 300, 100]; POC mid=101.5; VAL=101; VAH=103.
_VP_POC = 101.5
_VP_VAL = 101.0
_VP_VAH = 103.0
_VP_ROW_VOLUMES = (100.0, 500.0, 300.0, 100.0)
_VP_TOTAL = 1000.0

# Post-end bar dumps volume at ~110 so extend-on POC must move away from 101.5.
_PAST_LOW = 109.0
_PAST_HIGH = 111.0
_PAST_VOL = 5_000.0


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


def _in_range_bars() -> tuple[Bar, ...]:
    """Five bars inside the fixed range with known volume-by-price distribution.

    Profile high–low is exactly 100–104 so rows=4 → unit-height buckets.
    """
    return (
        Bar(ts=_T0, open=100.4, high=100.9, low=100.0, close=100.5, volume=100.0),
        Bar(ts=_T1, open=101.2, high=101.9, low=101.0, close=101.5, volume=400.0),
        Bar(ts=_T2, open=102.1, high=102.9, low=102.0, close=102.4, volume=200.0),
        Bar(ts=_T3, open=103.2, high=104.0, low=103.0, close=103.4, volume=100.0),
        Bar(ts=_T4, open=101.5, high=102.9, low=101.0, close=102.0, volume=200.0),
    )


def _bars_with_before_and_past() -> tuple[Bar, ...]:
    """Before-start + in-range + post-end bars for extend on/off behavioral tests."""
    return (
        Bar(
            ts=_T_BEFORE,
            open=90.0,
            high=91.0,
            low=89.0,
            close=90.5,
            volume=999_999.0,
        ),
        *_in_range_bars(),
        Bar(
            ts=_T_PAST,
            open=110.0,
            high=_PAST_HIGH,
            low=_PAST_LOW,
            close=110.5,
            volume=_PAST_VOL,
        ),
    )


def _default_frvp(overrides: dict | None = None) -> dict:
    cfg = {
        "id": "frvp",
        "type": "fixed_range_vp",
        "enabled": True,
        "start": _T0,
        "end": _RANGE_END,
        "extend_to_right": False,
        "box_width": 30,
        "placement": "right",
        "rows": 4,
        "value_area_volume": 70,
        "histogram": {"color": "steelblue", "opacity": 0.35},
        "poc": {"enabled": True, "color": "yellow", "opacity": 1.0},
        "vah": {"enabled": True, "color": "lime", "opacity": 1.0},
        "val": {"enabled": True, "color": "red", "opacity": 1.0},
    }
    if overrides:
        cfg.update(overrides)
    return cfg


def test_fixed_range_vp_apply_returns_profile_structure(tmp_path: Path):
    vendor = FakeVendor(auto_ticks=False)
    vendor.seed_raw_bars("SPY", "1m", _in_range_bars())
    store = tmp_path / "workspace.json"
    client, _ = _client(workspace_path=store, vendor=vendor)

    client.post(
        "/v1/chart/interest",
        json={"chart_id": "primary", "instrument": "SPY", "timeframe": "1m"},
    )
    response = client.post(
        "/v1/indicators",
        json={"chart_id": "primary", "indicators": [_default_frvp()]},
    )
    assert response.status_code == 200
    body = response.json()
    cfg = body["indicators"][0]
    assert cfg["type"] == "fixed_range_vp"
    assert cfg["start"] == _T0
    assert cfg["end"] == _RANGE_END
    assert cfg["extend_to_right"] is False
    assert cfg["rows"] == 4
    assert cfg["value_area_volume"] == 70
    assert cfg["box_width"] == 30
    assert cfg["placement"] == "right"
    assert cfg["poc"]["enabled"] is True

    series = body["series"]["frvp"]
    assert series["type"] == "fixed_range_vp"
    profiles = series["profiles"]
    assert len(profiles) == 1
    profile = profiles[0]
    assert profile["range_start"] == _T0
    assert profile["range_end"] == _RANGE_END
    assert profile["anchor_end"] == _RANGE_END
    assert profile["levels_end"] == _RANGE_END
    assert profile["extend_to_right"] is False
    assert profile["poc"] == pytest.approx(_VP_POC)
    assert profile["val"] == pytest.approx(_VP_VAL)
    assert profile["vah"] == pytest.approx(_VP_VAH)
    bins = profile["bins"]
    assert len(bins) == 4
    assert [b["volume"] for b in bins] == list(_VP_ROW_VOLUMES)
    assert bins[0]["price_low"] == pytest.approx(100.0)
    assert bins[-1]["price_high"] == pytest.approx(104.0)


def test_fixed_range_vp_max_four_per_chart_rejected(tmp_path: Path):
    store = tmp_path / "workspace.json"
    client, _ = _client(workspace_path=store)
    client.post(
        "/v1/chart/interest",
        json={"chart_id": "primary", "instrument": "SPY", "timeframe": "1D"},
    )
    five = [
        _default_frvp({"id": f"frvp{i}", "rows": 200, "start": _T0 + i, "end": _RANGE_END + i})
        for i in range(5)
    ]
    too_many = client.post(
        "/v1/indicators",
        json={"chart_id": "primary", "indicators": five},
    )
    assert too_many.status_code == 422
    assert "fixed" in too_many.json()["detail"].lower() or "4" in too_many.json()["detail"]
    snap = client.get("/v1/snapshot").json()["workspace"]["charts"][0]
    assert snap["indicators"] == []


def test_fixed_range_vp_defaults_rows_200_value_area_70(tmp_path: Path):
    store = tmp_path / "workspace.json"
    client, _ = _client(workspace_path=store)
    body = client.post(
        "/v1/indicators",
        json={
            "chart_id": "primary",
            "indicators": [
                {
                    "id": "frvp",
                    "type": "fixed_range_vp",
                    "enabled": True,
                    "start": _T0,
                    "end": _RANGE_END,
                }
            ],
        },
    ).json()
    cfg = body["indicators"][0]
    assert cfg["rows"] == 200
    assert cfg["value_area_volume"] == 70
    assert cfg["extend_to_right"] is False
    assert cfg["placement"] in ("left", "right")
    assert cfg["box_width"] > 0


def test_fixed_range_vp_extend_off_closed_window_no_post_end_volume(tmp_path: Path):
    """Extend off: only [start, end]; post-end and pre-start bars ignored; levels stay in window."""
    vendor = FakeVendor(auto_ticks=False)
    vendor.seed_raw_bars("SPY", "1m", _bars_with_before_and_past())
    store = tmp_path / "workspace.json"
    client, _ = _client(workspace_path=store, vendor=vendor)

    client.post(
        "/v1/chart/interest",
        json={"chart_id": "primary", "instrument": "SPY", "timeframe": "1m"},
    )
    body = client.post(
        "/v1/indicators",
        json={
            "chart_id": "primary",
            "indicators": [_default_frvp({"extend_to_right": False})],
        },
    ).json()

    profile = body["series"]["frvp"]["profiles"][0]
    assert profile["extend_to_right"] is False
    assert profile["range_start"] == _T0
    assert profile["range_end"] == _RANGE_END
    assert profile["levels_end"] == _RANGE_END
    # Pre-start 999_999 and post-end 5000 must not enter the closed window.
    assert profile["total_volume"] == pytest.approx(_VP_TOTAL)
    assert profile["poc"] == pytest.approx(_VP_POC)
    assert profile["val"] == pytest.approx(_VP_VAL)
    assert profile["vah"] == pytest.approx(_VP_VAH)


def test_fixed_range_vp_extend_on_accumulates_past_end_and_projects_levels(
    tmp_path: Path,
):
    """Extend on: live build past end + levels project past the original end anchor."""
    vendor = FakeVendor(auto_ticks=False)
    vendor.seed_raw_bars("SPY", "1m", _bars_with_before_and_past())
    store = tmp_path / "workspace.json"
    client, _ = _client(workspace_path=store, vendor=vendor)

    client.post(
        "/v1/chart/interest",
        json={"chart_id": "primary", "instrument": "SPY", "timeframe": "1m"},
    )
    body = client.post(
        "/v1/indicators",
        json={
            "chart_id": "primary",
            "indicators": [_default_frvp({"extend_to_right": True})],
        },
    ).json()

    profile = body["series"]["frvp"]["profiles"][0]
    assert profile["extend_to_right"] is True
    assert profile["range_start"] == _T0
    assert profile["anchor_end"] == _RANGE_END
    # Accumulation includes the post-end bar; levels project at least to that bar.
    assert profile["range_end"] == _T_PAST
    assert profile["levels_end"] == _T_PAST
    assert profile["levels_end"] > profile["anchor_end"]
    # Pre-start still excluded; post-end volume included.
    assert profile["total_volume"] == pytest.approx(_VP_TOTAL + _PAST_VOL)
    # POC must move — post-end volume at ~110 dominates the old 101.5 cluster.
    assert profile["poc"] != pytest.approx(_VP_POC)
    assert profile["poc"] >= _PAST_LOW
    assert profile["poc"] <= _PAST_HIGH


def test_fixed_range_vp_rejects_start_after_end(tmp_path: Path):
    store = tmp_path / "workspace.json"
    client, _ = _client(workspace_path=store)
    bad = client.post(
        "/v1/indicators",
        json={
            "chart_id": "primary",
            "indicators": [
                _default_frvp({"start": _RANGE_END, "end": _T0}),
            ],
        },
    )
    assert bad.status_code == 422
    detail = bad.json()["detail"].lower()
    assert "start" in detail or "end" in detail


def test_fixed_range_vp_requires_start_and_end(tmp_path: Path):
    store = tmp_path / "workspace.json"
    client, _ = _client(workspace_path=store)
    missing_end = client.post(
        "/v1/indicators",
        json={
            "chart_id": "primary",
            "indicators": [
                {"id": "frvp", "type": "fixed_range_vp", "enabled": True, "start": _T0}
            ],
        },
    )
    assert missing_end.status_code == 422


def test_fixed_range_vp_level_toggles_and_styles_persist(tmp_path: Path):
    store = tmp_path / "workspace.json"
    client, _ = _client(workspace_path=store)

    body = client.post(
        "/v1/indicators",
        json={
            "chart_id": "primary",
            "indicators": [
                _default_frvp(
                    {
                        "rows": 200,
                        "box_width": 40,
                        "placement": "left",
                        "extend_to_right": True,
                        "histogram": {"color": "cyan", "opacity": 0.2},
                        "poc": {"enabled": False, "color": "white", "opacity": 0.5},
                        "vah": {"enabled": True, "color": "green", "opacity": 0.8},
                        "val": {"enabled": False, "color": "magenta", "opacity": 0.9},
                    }
                )
            ],
        },
    ).json()
    cfg = body["indicators"][0]
    assert cfg["placement"] == "left"
    assert cfg["box_width"] == 40
    assert cfg["extend_to_right"] is True
    assert cfg["histogram"]["opacity"] == pytest.approx(0.2)
    assert cfg["poc"]["enabled"] is False
    assert cfg["val"]["enabled"] is False
    assert cfg["vah"]["color"] == "green"

    client2, _ = _client(workspace_path=store)
    snap = client2.get("/v1/snapshot").json()
    restored = snap["workspace"]["charts"][0]["indicators"][0]
    assert restored["type"] == "fixed_range_vp"
    assert restored["rows"] == 200
    assert restored["start"] == _T0
    assert restored["end"] == _RANGE_END
    assert restored["extend_to_right"] is True
    assert restored["placement"] == "left"
    assert restored["poc"]["enabled"] is False
    assert restored["val"]["enabled"] is False
    assert restored["histogram"]["color"] == "cyan"


def test_fixed_range_vp_snapshot_and_interest_include_series(tmp_path: Path):
    vendor = FakeVendor(auto_ticks=False)
    vendor.seed_raw_bars("SPY", "1m", _in_range_bars())
    store = tmp_path / "workspace.json"
    client, _ = _client(workspace_path=store, vendor=vendor)

    client.post(
        "/v1/indicators",
        json={"chart_id": "primary", "indicators": [_default_frvp()]},
    )
    interest = client.post(
        "/v1/chart/interest",
        json={"chart_id": "primary", "instrument": "SPY", "timeframe": "1m"},
    ).json()
    assert interest["status"] == "ok"
    assert interest["series"]["frvp"]["profiles"][0]["poc"] == pytest.approx(_VP_POC)

    snap = client.get("/v1/snapshot").json()
    assert "frvp" in snap["indicators"]["primary"]["series"]
    assert snap["indicators"]["primary"]["series"]["frvp"]["profiles"][0]["vah"] == pytest.approx(
        _VP_VAH
    )


def test_fixed_range_vp_disabled_omits_series(tmp_path: Path):
    vendor = FakeVendor(auto_ticks=False)
    vendor.seed_raw_bars("SPY", "1m", _in_range_bars())
    store = tmp_path / "workspace.json"
    client, _ = _client(workspace_path=store, vendor=vendor)
    client.post(
        "/v1/chart/interest",
        json={"chart_id": "primary", "instrument": "SPY", "timeframe": "1m"},
    )
    body = client.post(
        "/v1/indicators",
        json={
            "chart_id": "primary",
            "indicators": [_default_frvp({"enabled": False})],
        },
    ).json()
    assert body["indicators"][0]["enabled"] is False
    assert "frvp" not in body["series"]


def test_fixed_range_vp_four_instances_allowed(tmp_path: Path):
    store = tmp_path / "workspace.json"
    client, _ = _client(workspace_path=store)
    four = [
        _default_frvp({"id": f"frvp{i}", "rows": 200, "start": _T0 + i * 10, "end": _RANGE_END + i * 10})
        for i in range(4)
    ]
    ok = client.post(
        "/v1/indicators",
        json={"chart_id": "primary", "indicators": four},
    )
    assert ok.status_code == 200
    assert len(ok.json()["indicators"]) == 4
