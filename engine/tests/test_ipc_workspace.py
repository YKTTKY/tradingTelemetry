"""Engine IPC seam: dual layout + file-backed workspace persistence."""

from __future__ import annotations

import time
from pathlib import Path

from fastapi.testclient import TestClient

from market_engine.app import create_app
from market_engine.feed import default_feed_state
from market_engine.vendor import FakeVendor

_CONFLATE_S = 0.05
_SPY_1D_FIRST_CLOSE = 540.0
_QQQ_1D_FIRST_CLOSE = 490.0


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


def _receive_of_type(ws, type_name: str, max_msgs: int = 15) -> dict:
    for _ in range(max_msgs):
        msg = ws.receive_json()
        if msg.get("type") == type_name:
            return msg
    raise AssertionError(f"no {type_name!r} within {max_msgs} messages")


def test_snapshot_includes_default_single_workspace_spy_1d(tmp_path: Path):
    store = tmp_path / "workspace.json"
    client, _ = _client(workspace_path=store)

    body = client.get("/v1/snapshot").json()

    assert "workspace" in body
    ws = body["workspace"]
    assert ws["layout_mode"] == "single"
    assert ws["charts"] == [
        {"id": "primary", "instrument": "SPY", "timeframe": "1D"},
    ]
    # First launch: no file written until a mutation (or write-on-load is ok either way).
    # After snapshot alone, file may or may not exist; mutations must persist.


def test_set_layout_dual_vertical_defaults_top_qqq_bottom_spy(tmp_path: Path):
    store = tmp_path / "workspace.json"
    client, _ = _client(workspace_path=store)

    response = client.post(
        "/v1/workspace",
        json={"layout_mode": "dual-vertical"},
    )

    assert response.status_code == 200
    body = response.json()
    assert body["layout_mode"] == "dual-vertical"
    assert body["charts"] == [
        {"id": "top", "instrument": "QQQ", "timeframe": "1D"},
        {"id": "bottom", "instrument": "SPY", "timeframe": "1D"},
    ]

    snap = client.get("/v1/snapshot").json()["workspace"]
    assert snap == body


def test_single_and_dual_selections_are_independent(tmp_path: Path):
    """Toggling layout restores each mode's last charts (dual does not clobber single)."""
    store = tmp_path / "workspace.json"
    client, _ = _client(workspace_path=store)

    # Customize single primary first.
    client.post(
        "/v1/chart/interest",
        json={"chart_id": "primary", "instrument": "ES", "timeframe": "1D"},
    )
    dual = client.post("/v1/workspace", json={"layout_mode": "dual-vertical"}).json()
    assert dual["charts"] == [
        {"id": "top", "instrument": "QQQ", "timeframe": "1D"},
        {"id": "bottom", "instrument": "SPY", "timeframe": "1D"},
    ]
    # Customize dual top, then return to single — primary still ES.
    client.post(
        "/v1/chart/interest",
        json={"chart_id": "top", "instrument": "QQQ", "timeframe": "1h"},
    )
    body = client.post("/v1/workspace", json={"layout_mode": "single"}).json()
    assert body["layout_mode"] == "single"
    assert body["charts"] == [
        {"id": "primary", "instrument": "ES", "timeframe": "1D"},
    ]
    # Dual still remembers customized top on re-entry.
    again = client.post("/v1/workspace", json={"layout_mode": "dual-vertical"}).json()
    assert again["charts"] == [
        {"id": "top", "instrument": "QQQ", "timeframe": "1h"},
        {"id": "bottom", "instrument": "SPY", "timeframe": "1D"},
    ]


def test_dual_charts_have_independent_interest_and_history(tmp_path: Path):
    store = tmp_path / "workspace.json"
    client, _ = _client(workspace_path=store)

    client.post("/v1/workspace", json={"layout_mode": "dual-vertical"})

    top = client.post(
        "/v1/chart/interest",
        json={"chart_id": "top", "instrument": "QQQ", "timeframe": "1D"},
    ).json()
    bottom = client.post(
        "/v1/chart/interest",
        json={"chart_id": "bottom", "instrument": "SPY", "timeframe": "1D"},
    ).json()

    assert top["status"] == "ok"
    assert top["chart_id"] == "top"
    assert top["instrument"] == "QQQ"
    assert top["bars"][0]["close"] == _QQQ_1D_FIRST_CLOSE

    assert bottom["status"] == "ok"
    assert bottom["chart_id"] == "bottom"
    assert bottom["instrument"] == "SPY"
    assert bottom["bars"][0]["close"] == _SPY_1D_FIRST_CLOSE

    # Both remain active after second interest (multi-interest, not sole-active).
    snap = client.get("/v1/snapshot").json()["workspace"]
    assert snap["charts"] == [
        {"id": "top", "instrument": "QQQ", "timeframe": "1D"},
        {"id": "bottom", "instrument": "SPY", "timeframe": "1D"},
    ]


def test_chart_interest_without_chart_id_defaults_to_primary_in_single(tmp_path: Path):
    """Backward-compatible interest body still works for the single chart."""
    store = tmp_path / "workspace.json"
    client, _ = _client(workspace_path=store)

    body = client.post(
        "/v1/chart/interest",
        json={"instrument": "QQQ", "timeframe": "1D"},
    ).json()
    assert body["status"] == "ok"
    assert body["chart_id"] == "primary"
    assert body["instrument"] == "QQQ"

    snap = client.get("/v1/snapshot").json()["workspace"]
    assert snap["layout_mode"] == "single"
    assert snap["charts"] == [
        {"id": "primary", "instrument": "QQQ", "timeframe": "1D"},
    ]


def test_workspace_persists_across_engine_restart(tmp_path: Path):
    store = tmp_path / "workspace.json"
    client1, _ = _client(workspace_path=store)

    client1.post("/v1/workspace", json={"layout_mode": "dual-vertical"})
    client1.post(
        "/v1/chart/interest",
        json={"chart_id": "top", "instrument": "ES", "timeframe": "1D"},
    )
    client1.post(
        "/v1/chart/interest",
        json={"chart_id": "bottom", "instrument": "QQQ", "timeframe": "1D"},
    )
    # Confirm file-backed (no Redis/Postgres).
    assert store.is_file()
    raw = store.read_text(encoding="utf-8")
    assert "dual-vertical" in raw
    assert "ES" in raw
    assert "QQQ" in raw

    # Cold restart: new app instance, same store path.
    client2, _ = _client(workspace_path=store)
    snap = client2.get("/v1/snapshot").json()["workspace"]
    assert snap["layout_mode"] == "dual-vertical"
    assert snap["charts"] == [
        {"id": "top", "instrument": "ES", "timeframe": "1D"},
        {"id": "bottom", "instrument": "QQQ", "timeframe": "1D"},
    ]


def test_dual_live_updates_for_both_charts(tmp_path: Path):
    store = tmp_path / "workspace.json"
    client, vendor = _client(workspace_path=store)

    with client:
        client.post("/v1/workspace", json={"layout_mode": "dual-vertical"})
        top = client.post(
            "/v1/chart/interest",
            json={"chart_id": "top", "instrument": "QQQ", "timeframe": "1D"},
        ).json()
        bottom = client.post(
            "/v1/chart/interest",
            json={"chart_id": "bottom", "instrument": "SPY", "timeframe": "1D"},
        ).json()
        assert top["status"] == "ok" and bottom["status"] == "ok"

        q_ts, q_close = top["bars"][-1]["ts"], top["bars"][-1]["close"]
        s_ts, s_close = bottom["bars"][-1]["ts"], bottom["bars"][-1]["close"]

        with client.websocket_connect("/v1/ws") as ws:
            _receive_of_type(ws, "feed_status")
            vendor.inject_tick(
                "QQQ",
                price=q_close + 1.0,
                volume=1_000.0,
                ts=float(q_ts + 60),
            )
            vendor.inject_tick(
                "SPY",
                price=s_close + 2.0,
                volume=2_000.0,
                ts=float(s_ts + 60),
            )
            time.sleep(_CONFLATE_S * 2.5)

            seen: dict[str, dict] = {}
            for _ in range(20):
                msg = ws.receive_json()
                if msg.get("type") == "bar_update":
                    seen[msg["instrument"]] = msg
                if "QQQ" in seen and "SPY" in seen:
                    break

            assert "QQQ" in seen and "SPY" in seen
            assert seen["QQQ"]["timeframe"] == "1D"
            assert seen["QQQ"]["bar"]["close"] == q_close + 1.0
            assert seen["SPY"]["bar"]["close"] == s_close + 2.0


def test_invalid_layout_mode_is_rejected(tmp_path: Path):
    store = tmp_path / "workspace.json"
    client, _ = _client(workspace_path=store)

    response = client.post(
        "/v1/workspace",
        json={"layout_mode": "grid-3x3"},
    )
    assert response.status_code == 422
