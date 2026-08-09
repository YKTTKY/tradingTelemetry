"""Engine IPC seam: Session Volume Profile apply, limits, math, restore."""

from __future__ import annotations

from pathlib import Path

import pytest
from fastapi.testclient import TestClient

from market_engine.app import create_app
from market_engine.feed import default_feed_state
from market_engine.vendor import Bar, FakeVendor

_CONFLATE_S = 0.05

# America/New_York session anchors (computed once; independent of engine helpers).
# Equity session A: 2024-06-30 16:00 → 2024-07-01 16:00 ET
_EQ_SESS_A_START = 1_719_777_600
_EQ_SESS_A_MID = 1_719_842_400  # 2024-07-01 10:00 ET
_EQ_SESS_A_END = 1_719_864_000  # 2024-07-01 16:00 ET
# Equity session B: 2024-07-01 16:00 → 2024-07-02 16:00 ET
_EQ_SESS_B_MID = 1_719_878_400  # 2024-07-01 20:00 ET
# CME session for "Jul 1": 2024-06-30 18:00 → 2024-07-01 17:00 ET
_CME_SESS_START = 1_719_784_800
_CME_SESS_MID = 1_719_842_400  # 2024-07-01 10:00 ET
_CME_BREAK = 1_719_869_400  # 2024-07-01 17:30 ET (outside session)
_CME_NEXT_OPEN = 1_719_871_200  # 2024-07-01 18:00 ET

# Worked VP example (rows=4, prices 100–104, VA 70%):
# volumes by row after even H–L distribution: [100, 500, 300, 100]
# POC mid=101.5; value area expands POC→up → VAL=101, VAH=103.
_VP_POC = 101.5
_VP_VAL = 101.0
_VP_VAH = 103.0
_VP_ROW_VOLUMES = (100.0, 500.0, 300.0, 100.0)


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


def _equity_vp_bars() -> tuple[Bar, ...]:
    """Five bars in equity session A with known volume-by-price distribution.

    Profile high–low is exactly 100–104 so rows=4 → unit-height buckets.
    """
    return (
        Bar(ts=_EQ_SESS_A_MID + 0, open=100.4, high=100.9, low=100.0, close=100.5, volume=100.0),
        Bar(ts=_EQ_SESS_A_MID + 60, open=101.2, high=101.9, low=101.0, close=101.5, volume=400.0),
        Bar(ts=_EQ_SESS_A_MID + 120, open=102.1, high=102.9, low=102.0, close=102.4, volume=200.0),
        # high=104 pins profile high for equal unit buckets [100,101)…[103,104]
        Bar(ts=_EQ_SESS_A_MID + 180, open=103.2, high=104.0, low=103.0, close=103.4, volume=100.0),
        Bar(ts=_EQ_SESS_A_MID + 240, open=101.5, high=102.9, low=101.0, close=102.0, volume=200.0),
    )


def _default_session_vp(overrides: dict | None = None) -> dict:
    cfg = {
        "id": "svp",
        "type": "session_vp",
        "enabled": True,
        "mode": "all",
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


def test_session_vp_apply_returns_profile_structure(tmp_path: Path):
    vendor = FakeVendor(auto_ticks=False)
    vendor.seed_raw_bars("SPY", "1m", _equity_vp_bars())
    store = tmp_path / "workspace.json"
    client, _ = _client(workspace_path=store, vendor=vendor)

    client.post(
        "/v1/chart/interest",
        json={"chart_id": "primary", "instrument": "SPY", "timeframe": "1m"},
    )
    response = client.post(
        "/v1/indicators",
        json={"chart_id": "primary", "indicators": [_default_session_vp()]},
    )
    assert response.status_code == 200
    body = response.json()
    cfg = body["indicators"][0]
    assert cfg["type"] == "session_vp"
    assert cfg["mode"] == "all"
    assert cfg["rows"] == 4
    assert cfg["value_area_volume"] == 70
    assert cfg["box_width"] == 30
    assert cfg["placement"] == "right"
    assert cfg["poc"]["enabled"] is True
    assert cfg["vah"]["enabled"] is True
    assert cfg["val"]["enabled"] is True

    series = body["series"]["svp"]
    assert series["type"] == "session_vp"
    profiles = series["profiles"]
    assert len(profiles) == 1
    profile = profiles[0]
    assert profile["session_start"] == _EQ_SESS_A_START
    assert profile["session_end"] == _EQ_SESS_A_END
    assert profile["poc"] == pytest.approx(_VP_POC)
    assert profile["val"] == pytest.approx(_VP_VAL)
    assert profile["vah"] == pytest.approx(_VP_VAH)
    bins = profile["bins"]
    assert len(bins) == 4
    assert [b["volume"] for b in bins] == list(_VP_ROW_VOLUMES)
    # Equal price buckets across profile high–low (100–104 for this fixture).
    assert bins[0]["price_low"] == pytest.approx(100.0)
    assert bins[-1]["price_high"] == pytest.approx(104.0)


def test_session_vp_max_one_per_chart_rejected(tmp_path: Path):
    store = tmp_path / "workspace.json"
    client, _ = _client(workspace_path=store)
    client.post(
        "/v1/chart/interest",
        json={"chart_id": "primary", "instrument": "SPY", "timeframe": "1D"},
    )
    too_many = client.post(
        "/v1/indicators",
        json={
            "chart_id": "primary",
            "indicators": [
                _default_session_vp({"id": "svp1", "rows": 500}),
                _default_session_vp({"id": "svp2", "rows": 500}),
            ],
        },
    )
    assert too_many.status_code == 422
    assert "session" in too_many.json()["detail"].lower()
    snap = client.get("/v1/snapshot").json()["workspace"]["charts"][0]
    assert snap["indicators"] == []


def test_session_vp_rejects_non_all_mode(tmp_path: Path):
    store = tmp_path / "workspace.json"
    client, _ = _client(workspace_path=store)
    bad = client.post(
        "/v1/indicators",
        json={
            "chart_id": "primary",
            "indicators": [_default_session_vp({"mode": "rth"})],
        },
    )
    assert bad.status_code == 422
    assert "mode" in bad.json()["detail"].lower()


def test_session_vp_defaults_rows_500_value_area_70(tmp_path: Path):
    store = tmp_path / "workspace.json"
    client, _ = _client(workspace_path=store)
    body = client.post(
        "/v1/indicators",
        json={
            "chart_id": "primary",
            "indicators": [{"id": "svp", "type": "session_vp", "enabled": True}],
        },
    ).json()
    cfg = body["indicators"][0]
    assert cfg["rows"] == 500
    assert cfg["value_area_volume"] == 70
    assert cfg["mode"] == "all"
    assert cfg["placement"] in ("left", "right")
    assert cfg["box_width"] > 0


def test_session_vp_one_profile_per_day_equity_windows(tmp_path: Path):
    """Bars in two equity close-to-close windows → two profiles, correct bounds."""
    vendor = FakeVendor(auto_ticks=False)
    bars = (
        # Session A
        Bar(
            ts=_EQ_SESS_A_MID,
            open=100.0,
            high=101.0,
            low=99.0,
            close=100.5,
            volume=1_000.0,
        ),
        # Session B (after 16:00 ET)
        Bar(
            ts=_EQ_SESS_B_MID,
            open=102.0,
            high=103.0,
            low=101.0,
            close=102.5,
            volume=2_000.0,
        ),
    )
    vendor.seed_raw_bars("SPY", "1h", bars)
    store = tmp_path / "workspace.json"
    client, _ = _client(workspace_path=store, vendor=vendor)

    client.post(
        "/v1/chart/interest",
        json={"chart_id": "primary", "instrument": "SPY", "timeframe": "1h"},
    )
    body = client.post(
        "/v1/indicators",
        json={
            "chart_id": "primary",
            "indicators": [_default_session_vp({"rows": 10})],
        },
    ).json()

    profiles = body["series"]["svp"]["profiles"]
    assert len(profiles) == 2
    assert profiles[0]["session_start"] == _EQ_SESS_A_START
    assert profiles[0]["session_end"] == _EQ_SESS_A_END
    assert profiles[1]["session_start"] == _EQ_SESS_A_END
    assert profiles[1]["session_end"] == 1_719_950_400  # 2024-07-02 16:00 ET
    assert profiles[0]["total_volume"] == pytest.approx(1_000.0)
    assert profiles[1]["total_volume"] == pytest.approx(2_000.0)


def test_session_vp_es_uses_cme_session_clock(tmp_path: Path):
    """ES bars use prior-day 18:00 → 17:00; break bar excluded."""
    vendor = FakeVendor(auto_ticks=False)
    bars = (
        Bar(
            ts=_CME_SESS_MID,
            open=5500.0,
            high=5501.0,
            low=5499.0,
            close=5500.5,
            volume=500.0,
        ),
        # Inside 17:00–18:00 break — must not form its own profile or join session.
        Bar(
            ts=_CME_BREAK,
            open=5502.0,
            high=5503.0,
            low=5501.0,
            close=5502.5,
            volume=999_999.0,
        ),
        Bar(
            ts=_CME_NEXT_OPEN + 60,
            open=5504.0,
            high=5505.0,
            low=5503.0,
            close=5504.5,
            volume=300.0,
        ),
    )
    vendor.seed_raw_bars("ES", "1m", bars)
    store = tmp_path / "workspace.json"
    client, _ = _client(workspace_path=store, vendor=vendor)

    client.post(
        "/v1/chart/interest",
        json={"chart_id": "primary", "instrument": "ES", "timeframe": "1m"},
    )
    body = client.post(
        "/v1/indicators",
        json={
            "chart_id": "primary",
            "indicators": [_default_session_vp({"rows": 8})],
        },
    ).json()

    profiles = body["series"]["svp"]["profiles"]
    assert len(profiles) == 2
    assert profiles[0]["session_start"] == _CME_SESS_START
    assert profiles[0]["session_end"] == 1_719_867_600  # 2024-07-01 17:00 ET
    assert profiles[0]["total_volume"] == pytest.approx(500.0)
    # Break volume must not pollute either profile.
    assert profiles[0]["total_volume"] != pytest.approx(500.0 + 999_999.0)
    assert profiles[1]["session_start"] == _CME_NEXT_OPEN
    assert profiles[1]["total_volume"] == pytest.approx(300.0)


def test_session_vp_level_toggles_and_styles_persist(tmp_path: Path):
    store = tmp_path / "workspace.json"
    client, _ = _client(workspace_path=store)

    body = client.post(
        "/v1/indicators",
        json={
            "chart_id": "primary",
            "indicators": [
                _default_session_vp(
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

    # Restore after restart.
    client2, _ = _client(workspace_path=store)
    snap = client2.get("/v1/snapshot").json()
    restored = snap["workspace"]["charts"][0]["indicators"][0]
    assert restored["type"] == "session_vp"
    assert restored["rows"] == 500
    assert restored["placement"] == "left"
    assert restored["poc"]["enabled"] is False
    assert restored["val"]["enabled"] is False
    assert restored["histogram"]["color"] == "cyan"


def test_session_vp_snapshot_and_interest_include_series(tmp_path: Path):
    vendor = FakeVendor(auto_ticks=False)
    vendor.seed_raw_bars("SPY", "1m", _equity_vp_bars())
    store = tmp_path / "workspace.json"
    client, _ = _client(workspace_path=store, vendor=vendor)

    client.post(
        "/v1/indicators",
        json={"chart_id": "primary", "indicators": [_default_session_vp()]},
    )
    interest = client.post(
        "/v1/chart/interest",
        json={"chart_id": "primary", "instrument": "SPY", "timeframe": "1m"},
    ).json()
    assert interest["status"] == "ok"
    assert interest["series"]["svp"]["profiles"][0]["poc"] == pytest.approx(_VP_POC)

    snap = client.get("/v1/snapshot").json()
    assert "svp" in snap["indicators"]["primary"]["series"]
    assert snap["indicators"]["primary"]["series"]["svp"]["profiles"][0]["vah"] == pytest.approx(
        _VP_VAH
    )


def test_session_vp_disabled_omits_series(tmp_path: Path):
    vendor = FakeVendor(auto_ticks=False)
    vendor.seed_raw_bars("SPY", "1m", _equity_vp_bars())
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
            "indicators": [_default_session_vp({"enabled": False})],
        },
    ).json()
    assert body["indicators"][0]["enabled"] is False
    assert "svp" not in body["series"]
