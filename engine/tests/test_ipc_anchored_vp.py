"""Engine IPC seam: Anchored Volume Profile apply, limits, anchor→now window, restore."""

from __future__ import annotations

from pathlib import Path

import pytest
from fastapi.testclient import TestClient

from market_engine.app import create_app
from market_engine.feed import default_feed_state
from market_engine.vendor import Bar, FakeVendor

_CONFLATE_S = 0.05

# America/New_York wall times (independent of engine helpers).
# 2024-07-01 10:00 / 10:01 / … / 10:04 ET — post-anchor fixture (same VP math as Session VP).
_T0 = 1_719_842_400  # 2024-07-01 10:00 ET (also cash-open-ish window start for tests)
_T1 = _T0 + 60
_T2 = _T0 + 120
_T3 = _T0 + 180
_T4 = _T0 + 240
# Typical cash open 09:30 America/New_York same day (before the profile bars).
_CASH_OPEN = 1_719_840_600  # 2024-07-01 09:30 ET
# Bar before the cash-open anchor (must never contribute).
_T_BEFORE = 1_719_838_800  # 2024-07-01 09:00 ET

# Worked VP example (rows=4, prices 100–104, VA 70%) from post-anchor bars only:
# volumes by row: [100, 500, 300, 100]; POC mid=101.5; VAL=101; VAH=103.
_VP_POC = 101.5
_VP_VAL = 101.0
_VP_VAH = 103.0
_VP_ROW_VOLUMES = (100.0, 500.0, 300.0, 100.0)
_VP_TOTAL = 1000.0


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


def _post_anchor_bars() -> tuple[Bar, ...]:
    """Five bars at/after the anchor with known volume-by-price distribution.

    Profile high–low is exactly 100–104 so rows=4 → unit-height buckets.
    """
    return (
        Bar(ts=_T0, open=100.4, high=100.9, low=100.0, close=100.5, volume=100.0),
        Bar(ts=_T1, open=101.2, high=101.9, low=101.0, close=101.5, volume=400.0),
        Bar(ts=_T2, open=102.1, high=102.9, low=102.0, close=102.4, volume=200.0),
        Bar(ts=_T3, open=103.2, high=104.0, low=103.0, close=103.4, volume=100.0),
        Bar(ts=_T4, open=101.5, high=102.9, low=101.0, close=102.0, volume=200.0),
    )


def _bars_with_before() -> tuple[Bar, ...]:
    """Pre-anchor + post-anchor bars for window filtering."""
    return (
        Bar(
            ts=_T_BEFORE,
            open=90.0,
            high=91.0,
            low=89.0,
            close=90.5,
            volume=999_999.0,
        ),
        *_post_anchor_bars(),
    )


def _default_avp(overrides: dict | None = None) -> dict:
    cfg = {
        "id": "avp",
        "type": "anchored_vp",
        "enabled": True,
        "anchor": _CASH_OPEN,
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


def test_anchored_vp_apply_returns_profile_structure(tmp_path: Path):
    vendor = FakeVendor(auto_ticks=False)
    vendor.seed_raw_bars("SPY", "1m", _post_anchor_bars())
    store = tmp_path / "workspace.json"
    client, _ = _client(workspace_path=store, vendor=vendor)

    client.post(
        "/v1/chart/interest",
        json={"chart_id": "primary", "instrument": "SPY", "timeframe": "1m"},
    )
    response = client.post(
        "/v1/indicators",
        json={"chart_id": "primary", "indicators": [_default_avp()]},
    )
    assert response.status_code == 200
    body = response.json()
    cfg = body["indicators"][0]
    assert cfg["type"] == "anchored_vp"
    assert cfg["anchor"] == _CASH_OPEN
    assert cfg["rows"] == 4
    assert cfg["value_area_volume"] == 70
    assert cfg["box_width"] == 30
    assert cfg["placement"] == "right"
    assert cfg["poc"]["enabled"] is True

    series = body["series"]["avp"]
    assert series["type"] == "anchored_vp"
    profiles = series["profiles"]
    assert len(profiles) == 1
    profile = profiles[0]
    assert profile["anchor"] == _CASH_OPEN
    assert profile["range_start"] == _CASH_OPEN
    # Always builds forward to latest contributing bar.
    assert profile["range_end"] == _T4
    assert profile["levels_end"] == _T4
    assert profile["poc"] == pytest.approx(_VP_POC)
    assert profile["val"] == pytest.approx(_VP_VAL)
    assert profile["vah"] == pytest.approx(_VP_VAH)
    bins = profile["bins"]
    assert len(bins) == 4
    assert [b["volume"] for b in bins] == list(_VP_ROW_VOLUMES)
    assert bins[0]["price_low"] == pytest.approx(100.0)
    assert bins[-1]["price_high"] == pytest.approx(104.0)


def test_anchored_vp_excludes_bars_before_anchor(tmp_path: Path):
    """Profile is from one anchor forward; pre-anchor volume must not contribute."""
    vendor = FakeVendor(auto_ticks=False)
    vendor.seed_raw_bars("SPY", "1m", _bars_with_before())
    store = tmp_path / "workspace.json"
    client, _ = _client(workspace_path=store, vendor=vendor)

    client.post(
        "/v1/chart/interest",
        json={"chart_id": "primary", "instrument": "SPY", "timeframe": "1m"},
    )
    body = client.post(
        "/v1/indicators",
        json={"chart_id": "primary", "indicators": [_default_avp({"anchor": _CASH_OPEN})]},
    ).json()

    profile = body["series"]["avp"]["profiles"][0]
    assert profile["total_volume"] == pytest.approx(_VP_TOTAL)
    assert profile["poc"] == pytest.approx(_VP_POC)
    assert profile["val"] == pytest.approx(_VP_VAL)
    assert profile["vah"] == pytest.approx(_VP_VAH)
    # Range still projects to the latest post-anchor bar.
    assert profile["range_end"] == _T4
    assert profile["levels_end"] == _T4


def test_anchored_vp_max_two_per_chart_rejected(tmp_path: Path):
    store = tmp_path / "workspace.json"
    client, _ = _client(workspace_path=store)
    client.post(
        "/v1/chart/interest",
        json={"chart_id": "primary", "instrument": "SPY", "timeframe": "1D"},
    )
    three = [
        _default_avp({"id": f"avp{i}", "rows": 500, "anchor": _CASH_OPEN + i})
        for i in range(3)
    ]
    too_many = client.post(
        "/v1/indicators",
        json={"chart_id": "primary", "indicators": three},
    )
    assert too_many.status_code == 422
    detail = too_many.json()["detail"].lower()
    assert "anchor" in detail or "2" in detail
    snap = client.get("/v1/snapshot").json()["workspace"]["charts"][0]
    assert snap["indicators"] == []


def test_anchored_vp_two_instances_allowed(tmp_path: Path):
    store = tmp_path / "workspace.json"
    client, _ = _client(workspace_path=store)
    two = [
        _default_avp({"id": "avp1", "rows": 500, "anchor": _CASH_OPEN}),
        _default_avp({"id": "avp2", "rows": 500, "anchor": _CASH_OPEN + 60}),
    ]
    ok = client.post(
        "/v1/indicators",
        json={"chart_id": "primary", "indicators": two},
    )
    assert ok.status_code == 200
    assert len(ok.json()["indicators"]) == 2


def test_anchored_vp_defaults_rows_500_value_area_70(tmp_path: Path):
    store = tmp_path / "workspace.json"
    client, _ = _client(workspace_path=store)
    body = client.post(
        "/v1/indicators",
        json={
            "chart_id": "primary",
            "indicators": [
                {
                    "id": "avp",
                    "type": "anchored_vp",
                    "enabled": True,
                    "anchor": _CASH_OPEN,
                }
            ],
        },
    ).json()
    cfg = body["indicators"][0]
    assert cfg["rows"] == 500
    assert cfg["value_area_volume"] == 70
    assert cfg["placement"] in ("left", "right")
    assert cfg["box_width"] > 0


def test_anchored_vp_requires_anchor(tmp_path: Path):
    store = tmp_path / "workspace.json"
    client, _ = _client(workspace_path=store)
    missing = client.post(
        "/v1/indicators",
        json={
            "chart_id": "primary",
            "indicators": [
                {"id": "avp", "type": "anchored_vp", "enabled": True}
            ],
        },
    )
    assert missing.status_code == 422
    assert "anchor" in missing.json()["detail"].lower()


def test_anchored_vp_level_toggles_and_styles_persist(tmp_path: Path):
    store = tmp_path / "workspace.json"
    client, _ = _client(workspace_path=store)

    body = client.post(
        "/v1/indicators",
        json={
            "chart_id": "primary",
            "indicators": [
                _default_avp(
                    {
                        "rows": 500,
                        "box_width": 40,
                        "placement": "left",
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
    assert cfg["histogram"]["opacity"] == pytest.approx(0.2)
    assert cfg["poc"]["enabled"] is False
    assert cfg["val"]["enabled"] is False
    assert cfg["vah"]["color"] == "green"

    client2, _ = _client(workspace_path=store)
    snap = client2.get("/v1/snapshot").json()
    restored = snap["workspace"]["charts"][0]["indicators"][0]
    assert restored["type"] == "anchored_vp"
    assert restored["rows"] == 500
    assert restored["anchor"] == _CASH_OPEN
    assert restored["placement"] == "left"
    assert restored["poc"]["enabled"] is False
    assert restored["val"]["enabled"] is False
    assert restored["histogram"]["color"] == "cyan"


def test_anchored_vp_snapshot_and_interest_include_series(tmp_path: Path):
    vendor = FakeVendor(auto_ticks=False)
    vendor.seed_raw_bars("SPY", "1m", _post_anchor_bars())
    store = tmp_path / "workspace.json"
    client, _ = _client(workspace_path=store, vendor=vendor)

    client.post(
        "/v1/indicators",
        json={"chart_id": "primary", "indicators": [_default_avp()]},
    )
    interest = client.post(
        "/v1/chart/interest",
        json={"chart_id": "primary", "instrument": "SPY", "timeframe": "1m"},
    ).json()
    assert interest["status"] == "ok"
    assert interest["series"]["avp"]["profiles"][0]["poc"] == pytest.approx(_VP_POC)

    snap = client.get("/v1/snapshot").json()
    assert "avp" in snap["indicators"]["primary"]["series"]
    assert snap["indicators"]["primary"]["series"]["avp"]["profiles"][0]["vah"] == pytest.approx(
        _VP_VAH
    )


def test_anchored_vp_disabled_omits_series(tmp_path: Path):
    vendor = FakeVendor(auto_ticks=False)
    vendor.seed_raw_bars("SPY", "1m", _post_anchor_bars())
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
            "indicators": [_default_avp({"enabled": False})],
        },
    ).json()
    assert body["indicators"][0]["enabled"] is False
    assert "avp" not in body["series"]


def test_anchored_vp_anchor_at_bar_includes_that_bar(tmp_path: Path):
    """Bars with open == anchor are included (forward window is [anchor, now])."""
    vendor = FakeVendor(auto_ticks=False)
    vendor.seed_raw_bars("SPY", "1m", _post_anchor_bars())
    store = tmp_path / "workspace.json"
    client, _ = _client(workspace_path=store, vendor=vendor)
    client.post(
        "/v1/chart/interest",
        json={"chart_id": "primary", "instrument": "SPY", "timeframe": "1m"},
    )
    body = client.post(
        "/v1/indicators",
        json={"chart_id": "primary", "indicators": [_default_avp({"anchor": _T0})]},
    ).json()
    profile = body["series"]["avp"]["profiles"][0]
    assert profile["anchor"] == _T0
    assert profile["total_volume"] == pytest.approx(_VP_TOTAL)
    assert profile["range_end"] == _T4
