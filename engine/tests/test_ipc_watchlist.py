"""Engine IPC seam: multi-list watchlists + live conflated quotes (fake vendor)."""

from __future__ import annotations

import time
from pathlib import Path

from fastapi.testclient import TestClient

from market_engine.app import create_app
from market_engine.feed import default_feed_state
from market_engine.vendor import FakeVendor

_CONFLATE_S = 0.05

# Known SPY @ 1D fake fixture (independent of implementation internals).
_SPY_1D_LAST = 548.0
_SPY_1D_PREV_CLOSE = 546.25  # penultimate daily close in fake series
_SPY_1D_LAST_TS = 1_719_792_000 + 9 * 86_400

# Default Core membership (VIX only when vendor resolves — fake does not).
_CORE_DEFAULT_NO_VIX = ["ES", "NQ", "SPY", "QQQ", "SOXL"]


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


def test_snapshot_includes_default_core_watchlist_without_vix(tmp_path: Path):
    """First launch: Core = ES,NQ,SPY,QQQ,SOXL; VIX omitted when vendor cannot resolve."""
    store = tmp_path / "workspace.json"
    client, _ = _client(workspace_path=store)

    body = client.get("/v1/snapshot").json()
    assert "workspace" in body
    ws = body["workspace"]
    assert "watchlists" in ws
    assert ws["active_watchlist_id"]
    lists = {wl["id"]: wl for wl in ws["watchlists"]}
    assert len(lists) >= 1
    core = next(wl for wl in ws["watchlists"] if wl["name"] == "Core")
    assert core["symbols"] == _CORE_DEFAULT_NO_VIX
    assert "VIX" not in core["symbols"]
    assert ws["active_watchlist_id"] == core["id"]

    # Quotes for every Core symbol; partial unavailability allowed.
    quotes = {q["symbol"]: q for q in body["quotes"]}
    for sym in _CORE_DEFAULT_NO_VIX:
        assert sym in quotes
    # SPY is available with known last / previous close / change fields.
    spy = quotes["SPY"]
    assert spy["status"] == "ok"
    assert spy["last"] == _SPY_1D_LAST
    assert spy["previous_close"] == _SPY_1D_PREV_CLOSE
    assert spy["change"] == _SPY_1D_LAST - _SPY_1D_PREV_CLOSE
    expected_pct = (_SPY_1D_LAST - _SPY_1D_PREV_CLOSE) / _SPY_1D_PREV_CLOSE
    assert abs(spy["change_pct"] - expected_pct) < 1e-9
    # No logo field on quote rows.
    assert "logo" not in spy


def test_unavailable_symbol_does_not_brick_watchlist(tmp_path: Path):
    store = tmp_path / "workspace.json"
    client, _ = _client(workspace_path=store)

    body = client.get("/v1/snapshot").json()
    quotes = {q["symbol"]: q for q in body["quotes"]}
    # NQ has no fake history → unavailable row, list still returned.
    nq = quotes["NQ"]
    assert nq["status"] == "unavailable"
    assert nq.get("last") is None
    # Available rows still present.
    assert quotes["SPY"]["status"] == "ok"
    assert body["workspace"]["watchlists"]


def test_add_and_remove_symbol_on_active_watchlist(tmp_path: Path):
    store = tmp_path / "workspace.json"
    client, _ = _client(workspace_path=store)

    add = client.post("/v1/watchlist/add", json={"symbol": "es"})
    assert add.status_code == 200
    body = add.json()
    core = next(wl for wl in body["workspace"]["watchlists"] if wl["name"] == "Core")
    # ES already in default Core — add is idempotent / still present once.
    assert core["symbols"].count("ES") == 1

    add_new = client.post("/v1/watchlist/add", json={"symbol": "iwm"})
    assert add_new.status_code == 200
    core = next(
        wl for wl in add_new.json()["workspace"]["watchlists"] if wl["name"] == "Core"
    )
    assert "IWM" in core["symbols"]
    quotes = {q["symbol"]: q for q in add_new.json()["quotes"]}
    assert "IWM" in quotes
    assert quotes["IWM"]["status"] == "unavailable"

    removed = client.post("/v1/watchlist/remove", json={"symbol": "IWM"})
    assert removed.status_code == 200
    core = next(
        wl for wl in removed.json()["workspace"]["watchlists"] if wl["name"] == "Core"
    )
    assert "IWM" not in core["symbols"]
    quote_syms = {q["symbol"] for q in removed.json()["quotes"]}
    assert "IWM" not in quote_syms


def test_switch_active_watchlist(tmp_path: Path):
    store = tmp_path / "workspace.json"
    client, _ = _client(workspace_path=store)

    snap = client.get("/v1/snapshot").json()
    lists = snap["workspace"]["watchlists"]
    assert len(lists) >= 2, "multi-list requires at least two named lists"
    other = next(
        wl for wl in lists if wl["id"] != snap["workspace"]["active_watchlist_id"]
    )

    response = client.post(
        "/v1/watchlist/active",
        json={"watchlist_id": other["id"]},
    )
    assert response.status_code == 200
    body = response.json()
    assert body["workspace"]["active_watchlist_id"] == other["id"]

    # Add only to the newly active list.
    client.post("/v1/watchlist/add", json={"symbol": "IWM"})
    again = client.get("/v1/snapshot").json()
    active_id = again["workspace"]["active_watchlist_id"]
    active = next(wl for wl in again["workspace"]["watchlists"] if wl["id"] == active_id)
    assert "IWM" in active["symbols"]
    core = next(wl for wl in again["workspace"]["watchlists"] if wl["name"] == "Core")
    assert "IWM" not in core["symbols"]


def test_watchlist_membership_persists_across_restart(tmp_path: Path):
    store = tmp_path / "workspace.json"
    client1, _ = _client(workspace_path=store)

    client1.post("/v1/watchlist/add", json={"symbol": "IWM"})
    lists = client1.get("/v1/snapshot").json()["workspace"]["watchlists"]
    other = next(wl for wl in lists if wl["name"] != "Core")
    client1.post("/v1/watchlist/active", json={"watchlist_id": other["id"]})

    assert store.is_file()
    raw = store.read_text(encoding="utf-8")
    assert "IWM" in raw
    assert "watchlists" in raw

    client2, _ = _client(workspace_path=store)
    snap = client2.get("/v1/snapshot").json()["workspace"]
    assert snap["active_watchlist_id"] == other["id"]
    core = next(wl for wl in snap["watchlists"] if wl["name"] == "Core")
    assert "IWM" in core["symbols"]


def test_live_quote_update_over_websocket(tmp_path: Path):
    store = tmp_path / "workspace.json"
    client, vendor = _client(workspace_path=store)

    with client:
        # Ensure SPY is on the interest set via default Core.
        snap = client.get("/v1/snapshot").json()
        spy = next(q for q in snap["quotes"] if q["symbol"] == "SPY")
        assert spy["status"] == "ok"
        prev = spy["previous_close"]
        assert prev == _SPY_1D_PREV_CLOSE

        with client.websocket_connect("/v1/ws") as ws:
            _receive_of_type(ws, "feed_status")

            new_last = 550.0
            vendor.inject_tick(
                "SPY",
                price=new_last,
                volume=1_000.0,
                ts=float(_SPY_1D_LAST_TS + 60),
            )
            time.sleep(_CONFLATE_S * 2.5)

            update = _receive_of_type(ws, "quote_update")
            assert update["symbol"] == "SPY"
            assert update["status"] == "ok"
            assert update["last"] == new_last
            assert update["previous_close"] == prev
            assert update["change"] == new_last - prev
            assert abs(update["change_pct"] - (new_last - prev) / prev) < 1e-9


def test_quote_burst_is_conflated(tmp_path: Path):
    store = tmp_path / "workspace.json"
    client, vendor = _client(workspace_path=store)

    with client:
        client.get("/v1/snapshot")  # arm watchlist quote interest
        with client.websocket_connect("/v1/ws") as ws:
            _receive_of_type(ws, "feed_status")

            n_ticks = 20
            for i in range(n_ticks):
                vendor.inject_tick(
                    "SPY",
                    price=548.0 + i * 0.01,
                    volume=10.0,
                    ts=float(_SPY_1D_LAST_TS + 1 + i),
                )
            time.sleep(_CONFLATE_S * 3)

            quote_events = 0
            deadline = time.time() + 1.0
            while time.time() < deadline:
                try:
                    msg = ws.receive_json()
                except Exception:
                    break
                if msg.get("type") == "quote_update" and msg.get("symbol") == "SPY":
                    quote_events += 1
                if quote_events >= n_ticks:
                    break
            assert 1 <= quote_events < n_ticks


def test_vix_included_when_vendor_resolves(tmp_path: Path):
    """When fake history includes VIX, default Core gains VIX."""
    store = tmp_path / "workspace.json"
    vendor = FakeVendor(auto_ticks=False)
    vendor.seed_history(
        "VIX",
        "1D",
        closes=(20.0, 20.5, 21.0),
        start_ts=1_719_792_000,
        period_seconds=86_400,
    )
    client, _ = _client(workspace_path=store, vendor=vendor)

    body = client.get("/v1/snapshot").json()
    core = next(wl for wl in body["workspace"]["watchlists"] if wl["name"] == "Core")
    assert core["symbols"] == _CORE_DEFAULT_NO_VIX + ["VIX"]
    quotes = {q["symbol"]: q for q in body["quotes"]}
    assert quotes["VIX"]["status"] == "ok"
    assert quotes["VIX"]["last"] == 21.0
    assert quotes["VIX"]["previous_close"] == 20.5


def test_rename_active_watchlist_persists_and_keeps_id(tmp_path: Path):
    """POST /v1/watchlist/rename updates active list display name; id stays stable."""
    store = tmp_path / "workspace.json"
    client, _ = _client(workspace_path=store)

    snap = client.get("/v1/snapshot").json()["workspace"]
    active_id = snap["active_watchlist_id"]
    core = next(wl for wl in snap["watchlists"] if wl["id"] == active_id)
    assert core["name"] == "Core"
    symbols_before = list(core["symbols"])

    renamed = client.post("/v1/watchlist/rename", json={"name": "  Day desk  "})
    assert renamed.status_code == 200
    body = renamed.json()
    active = next(
        wl for wl in body["workspace"]["watchlists"] if wl["id"] == active_id
    )
    assert active["id"] == active_id
    assert active["name"] == "Day desk"
    assert active["symbols"] == symbols_before
    assert body["workspace"]["active_watchlist_id"] == active_id
    # Quotes still returned for membership.
    assert "quotes" in body

    # Persist across engine restart.
    assert store.is_file()
    raw = store.read_text(encoding="utf-8")
    assert "Day desk" in raw
    assert '"id": "core"' in raw or '"id":"core"' in raw

    client2, _ = _client(workspace_path=store)
    again = client2.get("/v1/snapshot").json()["workspace"]
    assert again["active_watchlist_id"] == active_id
    restored = next(wl for wl in again["watchlists"] if wl["id"] == active_id)
    assert restored["name"] == "Day desk"
    assert restored["symbols"] == symbols_before


def test_rename_rejects_empty_name(tmp_path: Path):
    store = tmp_path / "workspace.json"
    client, _ = _client(workspace_path=store)

    for payload in ({"name": ""}, {"name": "   "}, {}):
        response = client.post("/v1/watchlist/rename", json=payload)
        assert response.status_code == 422, payload

    snap = client.get("/v1/snapshot").json()["workspace"]
    core = next(wl for wl in snap["watchlists"] if wl["name"] == "Core")
    assert core["id"] == "core"


def test_rename_allows_duplicate_display_names(tmp_path: Path):
    """Ids stay unique; display names may collide."""
    store = tmp_path / "workspace.json"
    client, _ = _client(workspace_path=store)

    snap = client.get("/v1/snapshot").json()["workspace"]
    core_id = next(wl["id"] for wl in snap["watchlists"] if wl["name"] == "Core")
    focus = next(wl for wl in snap["watchlists"] if wl["name"] == "Focus")

    client.post("/v1/watchlist/active", json={"watchlist_id": focus["id"]})
    renamed = client.post("/v1/watchlist/rename", json={"name": "Core"})
    assert renamed.status_code == 200
    lists = renamed.json()["workspace"]["watchlists"]
    names = [wl["name"] for wl in lists]
    assert names.count("Core") == 2
    ids = {wl["id"] for wl in lists}
    assert core_id in ids
    assert focus["id"] in ids
    assert len(ids) == len(lists)
