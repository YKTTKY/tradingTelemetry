"""Engine IPC seam: paper desk snapshot, SQLite book, discrete WS events."""

from __future__ import annotations

import json
import time
from pathlib import Path

from fastapi.testclient import TestClient

from market_engine.app import create_app
from market_engine.feed import default_feed_state
from market_engine.vendor import FakeVendor

_CONFLATE_S = 0.05

# First-launch paper account settings (visible on the snapshot, not code-only).
_DEFAULT_NAME = "Paper"
_DEFAULT_BALANCE = 100_000.0
_DEFAULT_COMMISSION = 1.0
_DEFAULT_LEVERAGE_MULTIPLE = 1.0


def _client(
    workspace_path: Path | None = None,
    paper_path: Path | None = None,
    vendor: FakeVendor | None = None,
    conflate_interval_s: float = _CONFLATE_S,
) -> tuple[TestClient, FakeVendor]:
    v = vendor if vendor is not None else FakeVendor(auto_ticks=False)
    app = create_app(
        feed=default_feed_state("fake"),
        vendor=v,
        conflate_interval_s=conflate_interval_s,
        workspace_path=workspace_path,
        paper_path=paper_path,
    )
    return TestClient(app), v


def _receive_of_type(ws, type_name: str, max_msgs: int = 40) -> dict:
    for _ in range(max_msgs):
        msg = ws.receive_json()
        if msg.get("type") == type_name:
            return msg
    raise AssertionError(f"no {type_name!r} within {max_msgs} messages")


def test_snapshot_includes_paper_desk_without_dropping_existing_keys(tmp_path: Path):
    store = tmp_path / "workspace.json"
    client, _ = _client(workspace_path=store)

    body = client.get("/v1/snapshot").json()

    assert body["feed"]["status"] == "connected"
    assert body["feed"]["vendor_mode"] == "fake"
    assert "last_vendor_tick_ts" in body["feed"]
    assert body["feed"]["last_vendor_tick_ts"] is None
    assert "workspace" in body
    assert "quotes" in body
    assert "indicators" in body
    assert "paper" in body

    assert (tmp_path / "paper.db").is_file()

    paper = body["paper"]
    assert paper["active_account_id"]
    assert len(paper["accounts"]) == 1
    acc = paper["accounts"][0]
    assert acc["id"] == paper["active_account_id"]
    assert acc["name"] == _DEFAULT_NAME
    assert acc["currency"] == "USD"
    assert acc["balance"] == _DEFAULT_BALANCE
    assert acc["initial_balance"] == _DEFAULT_BALANCE
    assert acc["commission_per_fill_usd"] == _DEFAULT_COMMISSION
    assert acc["leverage_enabled"] is False
    assert acc["leverage_multiple"] == _DEFAULT_LEVERAGE_MULTIPLE
    assert acc.get("asset_class_restriction") in (None, "")

    defaults = paper["defaults"]
    assert defaults["name"] == _DEFAULT_NAME
    assert defaults["initial_balance"] == _DEFAULT_BALANCE
    assert defaults["currency"] == "USD"
    assert defaults["commission_per_fill_usd"] == _DEFAULT_COMMISSION
    assert defaults["leverage_enabled"] is False
    assert defaults["leverage_multiple"] == _DEFAULT_LEVERAGE_MULTIPLE

    assert paper["positions"] == []
    assert paper["filled_order_history"] == []
    assert paper["balance_history"] == []
    assert paper["working_orders"] == []


def test_create_and_select_active_paper_account_via_ipc(tmp_path: Path):
    store = tmp_path / "workspace.json"
    client, _ = _client(workspace_path=store)

    first = client.get("/v1/snapshot").json()["paper"]
    first_id = first["active_account_id"]

    created = client.post(
        "/v1/paper/accounts",
        json={
            "name": "Scalps",
            "initial_balance": 25_000.0,
            "commission_per_fill_usd": 2.0,
            "leverage_enabled": True,
            "leverage_multiple": 4.0,
        },
    )
    assert created.status_code == 200
    body = created.json()
    assert body["active_account_id"] == first_id
    ids = {a["id"] for a in body["accounts"]}
    assert first_id in ids
    scalps = next(a for a in body["accounts"] if a["name"] == "Scalps")
    assert scalps["balance"] == 25_000.0
    assert scalps["initial_balance"] == 25_000.0
    assert scalps["currency"] == "USD"
    assert scalps["commission_per_fill_usd"] == 2.0
    assert scalps["leverage_enabled"] is True
    assert scalps["leverage_multiple"] == 4.0
    assert len(body["accounts"]) == 2

    selected = client.post(
        "/v1/paper/active",
        json={"account_id": scalps["id"]},
    )
    assert selected.status_code == 200
    paper = selected.json()
    assert paper["active_account_id"] == scalps["id"]
    snap = client.get("/v1/snapshot").json()["paper"]
    assert snap["active_account_id"] == scalps["id"]
    active = next(a for a in snap["accounts"] if a["id"] == snap["active_account_id"])
    assert active["name"] == "Scalps"


def test_paper_book_restores_across_engine_restart(tmp_path: Path):
    store = tmp_path / "workspace.json"
    client1, _ = _client(workspace_path=store)
    first_id = client1.get("/v1/snapshot").json()["paper"]["active_account_id"]
    created = client1.post(
        "/v1/paper/accounts",
        json={"name": "Swing", "initial_balance": 50_000.0},
    ).json()
    swing = next(a for a in created["accounts"] if a["name"] == "Swing")
    client1.post("/v1/paper/active", json={"account_id": swing["id"]})

    paper_db = tmp_path / "paper.db"
    assert paper_db.is_file()

    client2, _ = _client(workspace_path=store)
    paper = client2.get("/v1/snapshot").json()["paper"]
    names = {a["name"]: a for a in paper["accounts"]}
    assert set(names) == {_DEFAULT_NAME, "Swing"}
    assert paper["active_account_id"] == swing["id"]
    assert names[_DEFAULT_NAME]["id"] == first_id
    assert names["Swing"]["balance"] == 50_000.0
    assert names["Swing"]["commission_per_fill_usd"] == _DEFAULT_COMMISSION
    assert names["Swing"]["leverage_enabled"] is False
    assert names["Swing"]["leverage_multiple"] == _DEFAULT_LEVERAGE_MULTIPLE


def test_workspace_json_does_not_serialize_paper_book(tmp_path: Path):
    store = tmp_path / "workspace.json"
    client, _ = _client(workspace_path=store)
    client.post("/v1/workspace", json={"layout_mode": "dual-vertical"})
    client.post("/v1/paper/accounts", json={"name": "Scalps"})

    assert store.is_file()
    raw = json.loads(store.read_text(encoding="utf-8"))
    assert "paper" not in raw
    assert "active_account_id" not in raw
    dumped = json.dumps(raw)
    assert "Scalps" not in dumped
    assert "commission_per_fill_usd" not in dumped
    assert raw["layout_mode"] == "dual-vertical"


def test_paper_ws_events_are_discrete_and_not_latest_wins(tmp_path: Path):
    """Paper updates are a new WS type and a burst must not drop earlier events."""
    store = tmp_path / "workspace.json"
    client, _ = _client(workspace_path=store)

    with client:
        with client.websocket_connect("/v1/ws") as ws:
            _receive_of_type(ws, "feed_status")

            names = [f"Book-{i}" for i in range(8)]
            created_ids: list[str] = []
            for name in names:
                body = client.post("/v1/paper/accounts", json={"name": name}).json()
                created = next(a for a in body["accounts"] if a["name"] == name)
                created_ids.append(created["id"])

            time.sleep(_CONFLATE_S * 2.5)

            events: list[dict] = []
            deadline = time.time() + 2.0
            while time.time() < deadline and len(events) < len(names):
                msg = ws.receive_json()
                assert msg.get("type") != "bar_update"
                if msg.get("type") == "paper_update":
                    events.append(msg)

            assert len(events) == len(names)
            seen_names: list[str] = []
            for ev in events:
                assert ev["type"] == "paper_update"
                assert "paper" in ev
                account_names = [a["name"] for a in ev["paper"]["accounts"]]
                added = [n for n in account_names if n.startswith("Book-") and n not in seen_names]
                assert added, f"expected a new Book-* account in {account_names}"
                seen_names.extend(added)
            assert seen_names == names
            assert all(cid in {a["id"] for a in events[-1]["paper"]["accounts"]} for cid in created_ids)


def _place(client: TestClient, **kwargs) -> object:
    return client.post("/v1/paper/orders", json=kwargs)


def test_place_market_limit_stop_working_orders(tmp_path: Path):
    """Market, limit, and stop are accepted and stored as working orders (no fill)."""
    store = tmp_path / "workspace.json"
    client, _ = _client(workspace_path=store)

    market = _place(
        client,
        instrument="SPY",
        side="buy",
        type="market",
        qty=10,
    )
    assert market.status_code == 200, market.text
    paper = market.json()
    assert len(paper["working_orders"]) == 1
    wo = paper["working_orders"][0]
    assert wo["instrument"] == "SPY"
    assert wo["side"] == "buy"
    assert wo["type"] == "market"
    assert wo["qty"] == 10
    assert wo.get("limit") in (None, 0, 0.0)
    assert wo.get("stop") in (None, 0, 0.0)
    assert wo["id"]
    assert wo["account_id"] == paper["active_account_id"]
    assert isinstance(wo["placed_ts"], int)
    assert wo["placed_ts"] > 0

    limit = _place(
        client,
        instrument="SPY",
        side="sell",
        type="limit",
        qty=5,
        limit=550.0,
    )
    assert limit.status_code == 200, limit.text
    stop = _place(
        client,
        instrument="QQQ",
        side="buy",
        type="stop",
        qty=2,
        stop=500.0,
    )
    assert stop.status_code == 200, stop.text

    snap = client.get("/v1/snapshot").json()["paper"]
    by_type = {o["type"]: o for o in snap["working_orders"]}
    assert set(by_type) == {"market", "limit", "stop"}
    assert by_type["limit"]["limit"] == 550.0
    assert by_type["limit"]["side"] == "sell"
    assert by_type["limit"]["qty"] == 5
    assert by_type["stop"]["stop"] == 500.0
    assert by_type["stop"]["instrument"] == "QQQ"
    assert by_type["market"]["type"] == "market"
    assert snap["positions"] == []
    assert snap["filled_order_history"] == []


def test_place_rejected_when_qty_exceeds_buying_power_no_stub(tmp_path: Path):
    """Oversized qty is rejected; no partial and no stub working order."""
    store = tmp_path / "workspace.json"
    client, _ = _client(workspace_path=store)

    # SPY last close on the fake vendor is 548. 200 shares * 548 + $1 commission
    # exceeds the default $100_000 cash book.
    rejected = _place(
        client,
        instrument="SPY",
        side="buy",
        type="market",
        qty=200,
    )
    assert rejected.status_code == 422
    snap = client.get("/v1/snapshot").json()["paper"]
    assert snap["working_orders"] == []
    assert snap["positions"] == []
    assert snap["accounts"][0]["balance"] == _DEFAULT_BALANCE

    # Limit notional 250 * 500 + commission also exceeds cash.
    rejected_limit = _place(
        client,
        instrument="SPY",
        side="buy",
        type="limit",
        qty=250,
        limit=500.0,
    )
    assert rejected_limit.status_code == 422
    assert client.get("/v1/snapshot").json()["paper"]["working_orders"] == []

    ok = _place(
        client,
        instrument="SPY",
        side="buy",
        type="limit",
        qty=10,
        limit=500.0,
    )
    assert ok.status_code == 200
    assert len(ok.json()["working_orders"]) == 1

    # A sell of the same oversized qty is not a cash buy — it may rest.
    sell = _place(
        client,
        instrument="SPY",
        side="sell",
        type="market",
        qty=200,
    )
    assert sell.status_code == 200, sell.text
    assert any(o["side"] == "sell" and o["qty"] == 200 for o in sell.json()["working_orders"])


def test_market_working_order_accepted_without_last_price(tmp_path: Path):
    """Unknown instrument still stores a market working order (no fill this ticket)."""
    store = tmp_path / "workspace.json"
    client, _ = _client(workspace_path=store)
    placed = _place(
        client,
        instrument="NOPE",
        side="buy",
        type="market",
        qty=1,
    )
    assert placed.status_code == 200, placed.text
    wo = placed.json()["working_orders"][0]
    assert wo["instrument"] == "NOPE"
    assert wo["type"] == "market"
    assert wo["qty"] == 1


def test_modify_and_cancel_working_order(tmp_path: Path):
    store = tmp_path / "workspace.json"
    client, _ = _client(workspace_path=store)

    placed = _place(
        client,
        instrument="SPY",
        side="buy",
        type="limit",
        qty=10,
        limit=540.0,
    ).json()
    order_id = placed["working_orders"][0]["id"]

    modified = client.post(
        "/v1/paper/orders/modify",
        json={"order_id": order_id, "qty": 4, "limit": 541.5},
    )
    assert modified.status_code == 200, modified.text
    wo = modified.json()["working_orders"][0]
    assert wo["id"] == order_id
    assert wo["qty"] == 4
    assert wo["limit"] == 541.5
    assert wo["instrument"] == "SPY"
    assert wo["type"] == "limit"

    stop_placed = _place(
        client,
        instrument="SPY",
        side="sell",
        type="stop",
        qty=3,
        stop=530.0,
    ).json()
    stop_id = next(o["id"] for o in stop_placed["working_orders"] if o["type"] == "stop")
    stop_mod = client.post(
        "/v1/paper/orders/modify",
        json={"order_id": stop_id, "stop": 529.0, "qty": 2},
    )
    assert stop_mod.status_code == 200, stop_mod.text
    stop_wo = next(o for o in stop_mod.json()["working_orders"] if o["id"] == stop_id)
    assert stop_wo["stop"] == 529.0
    assert stop_wo["qty"] == 2

    stray_stop = client.post(
        "/v1/paper/orders/modify",
        json={"order_id": order_id, "stop": 500.0},
    )
    assert stray_stop.status_code == 200, stray_stop.text
    still_limit = next(
        o for o in stray_stop.json()["working_orders"] if o["id"] == order_id
    )
    assert still_limit["type"] == "limit"
    assert still_limit["limit"] == 541.5
    assert still_limit.get("stop") in (None, 0, 0.0)

    cancelled = client.post(
        "/v1/paper/orders/cancel",
        json={"order_id": order_id},
    )
    assert cancelled.status_code == 200, cancelled.text
    remaining = cancelled.json()["working_orders"]
    assert [o["id"] for o in remaining] == [stop_id]
    snap = client.get("/v1/snapshot").json()["paper"]
    assert [o["id"] for o in snap["working_orders"]] == [stop_id]


def test_modify_rejected_when_qty_exceeds_buying_power_leaves_original(tmp_path: Path):
    store = tmp_path / "workspace.json"
    client, _ = _client(workspace_path=store)
    placed = _place(
        client,
        instrument="SPY",
        side="buy",
        type="limit",
        qty=10,
        limit=540.0,
    ).json()
    order_id = placed["working_orders"][0]["id"]

    rejected = client.post(
        "/v1/paper/orders/modify",
        json={"order_id": order_id, "qty": 500, "limit": 540.0},
    )
    assert rejected.status_code == 422
    wo = client.get("/v1/snapshot").json()["paper"]["working_orders"][0]
    assert wo["id"] == order_id
    assert wo["qty"] == 10
    assert wo["limit"] == 540.0


def test_working_orders_persist_across_engine_restart(tmp_path: Path):
    store = tmp_path / "workspace.json"
    client1, _ = _client(workspace_path=store)
    placed = _place(
        client1,
        instrument="SPY",
        side="buy",
        type="limit",
        qty=8,
        limit=542.0,
    ).json()
    market = _place(
        client1,
        instrument="SPY",
        side="sell",
        type="market",
        qty=1,
    ).json()
    limit_id = next(o["id"] for o in placed["working_orders"] if o["type"] == "limit")
    market_id = next(o["id"] for o in market["working_orders"] if o["type"] == "market")

    client2, _ = _client(workspace_path=store)
    paper = client2.get("/v1/snapshot").json()["paper"]
    by_id = {o["id"]: o for o in paper["working_orders"]}
    assert set(by_id) == {limit_id, market_id}
    assert by_id[limit_id]["qty"] == 8
    assert by_id[limit_id]["limit"] == 542.0
    assert by_id[limit_id]["instrument"] == "SPY"
    assert by_id[limit_id]["type"] == "limit"
    assert by_id[market_id]["type"] == "market"
    assert by_id[market_id]["side"] == "sell"
    assert by_id[market_id]["qty"] == 1


def test_place_modify_cancel_emit_discrete_paper_ws_events(tmp_path: Path):
    store = tmp_path / "workspace.json"
    client, _ = _client(workspace_path=store)

    with client:
        with client.websocket_connect("/v1/ws") as ws:
            _receive_of_type(ws, "feed_status")

            placed = _place(
                client,
                instrument="SPY",
                side="buy",
                type="limit",
                qty=3,
                limit=540.0,
            ).json()
            order_id = placed["working_orders"][0]["id"]
            client.post(
                "/v1/paper/orders/modify",
                json={"order_id": order_id, "qty": 2, "limit": 541.0},
            )
            client.post("/v1/paper/orders/cancel", json={"order_id": order_id})

            time.sleep(_CONFLATE_S * 2.5)

            events: list[dict] = []
            deadline = time.time() + 2.0
            while time.time() < deadline and len(events) < 3:
                msg = ws.receive_json()
                assert msg.get("type") != "bar_update"
                if msg.get("type") == "paper_update":
                    events.append(msg)

            assert len(events) == 3
            reasons = [ev.get("reason") for ev in events]
            assert reasons == ["order_placed", "order_modified", "order_cancelled"]
            assert len(events[0]["paper"]["working_orders"]) == 1
            assert events[1]["paper"]["working_orders"][0]["qty"] == 2
            assert events[2]["paper"]["working_orders"] == []


# Seeded 1m series for bar-touch fill tests. Geometry matches FakeVendor
# `_bars_from_closes`: last open=109, close=110, high=110.5, low=108.5.
_SPY_1M_START_TS = 1_719_792_000
_MINUTE = 60
_SPY_1M_LAST_TS = _SPY_1M_START_TS + 10 * _MINUTE  # 1_719_792_600


def _seed_1m(vendor: FakeVendor, instrument: str = "SPY", *, base: float = 100.0) -> None:
    vendor.seed_history(
        instrument,
        "1m",
        closes=tuple(base + i for i in range(11)),
        start_ts=_SPY_1M_START_TS,
        period_seconds=_MINUTE,
        base_volume=1_000.0,
    )


def _paper_client(tmp_path: Path, *, instruments: tuple[str, ...] = ("SPY",)):
    vendor = FakeVendor(auto_ticks=False)
    for inst in instruments:
        _seed_1m(vendor, inst, base=100.0 if inst == "SPY" else 200.0)
    return _client(workspace_path=tmp_path / "workspace.json", vendor=vendor)


def test_market_fills_full_qty_on_next_1m_last_bar_eval(tmp_path: Path):
    """Market fills in full at the evaluating 1m bar's close (last), not at place."""
    client, vendor = _paper_client(tmp_path)
    placed = _place(client, instrument="SPY", side="buy", type="market", qty=10)
    assert placed.status_code == 200, placed.text
    paper = placed.json()
    assert len(paper["working_orders"]) == 1
    assert paper["positions"] == []
    assert paper["filled_order_history"] == []

    vendor.inject_tick("SPY", price=111.25, volume=50.0, ts=float(_SPY_1M_LAST_TS + 5))
    time.sleep(_CONFLATE_S * 2.5)

    snap = client.get("/v1/snapshot").json()["paper"]
    assert snap["working_orders"] == []
    assert len(snap["positions"]) == 1
    pos = snap["positions"][0]
    assert pos["symbol"] == "SPY"
    assert pos["side"] == "long"
    assert pos["qty"] == 10
    assert pos["avg_price"] == 111.25
    assert "unrealized_pnl" in pos

    assert len(snap["filled_order_history"]) == 1
    fill = snap["filled_order_history"][0]
    assert fill["symbol"] == "SPY"
    assert fill["side"] == "buy"
    assert fill["type"] == "market"
    assert fill["qty"] == 10
    assert fill["fill_price"] == 111.25
    assert fill["commission"] == _DEFAULT_COMMISSION
    assert fill["limit"] in (None, 0, 0.0)
    assert fill["stop"] in (None, 0, 0.0)
    assert isinstance(fill["placed_ts"], int) and fill["placed_ts"] > 0
    assert isinstance(fill["filled_ts"], int) and fill["filled_ts"] >= fill["placed_ts"]
    assert fill["duration_s"] == fill["filled_ts"] - fill["placed_ts"]
    assert fill.get("margin") in (None, 0, 0.0)

    # 10 * 111.25 + $1 commission
    assert snap["accounts"][0]["balance"] == 98_886.5
    assert len(snap["balance_history"]) == 1
    assert snap["balance_history"][0]["balance"] == 98_886.5
    assert isinstance(snap["balance_history"][0]["ts"], int)
    assert snap["balance_history"][0]["ts"] > 0


def test_limit_and_stop_fill_on_last_bar_touch_not_on_miss(tmp_path: Path):
    client, vendor = _paper_client(tmp_path)

    # Last 1m low=108.5 high=110.5. Buy limit 108 is not touching yet.
    buy_limit = _place(
        client, instrument="SPY", side="buy", type="limit", qty=4, limit=108.0
    ).json()["working_orders"][0]["id"]
    # Sell stop 108 is not touching yet.
    sell_stop = _place(
        client, instrument="SPY", side="sell", type="stop", qty=3, stop=108.0
    ).json()
    sell_stop_id = next(o["id"] for o in sell_stop["working_orders"] if o["type"] == "stop")

    vendor.inject_tick("SPY", price=109.0, volume=10.0, ts=float(_SPY_1M_LAST_TS + 2))
    time.sleep(_CONFLATE_S * 2.5)
    miss = client.get("/v1/snapshot").json()["paper"]
    assert {o["id"] for o in miss["working_orders"]} == {buy_limit, sell_stop_id}
    assert miss["positions"] == []
    assert miss["filled_order_history"] == []

    vendor.inject_tick("SPY", price=107.25, volume=10.0, ts=float(_SPY_1M_LAST_TS + 3))
    time.sleep(_CONFLATE_S * 2.5)
    hit = client.get("/v1/snapshot").json()["paper"]
    assert hit["working_orders"] == []
    by_type = {f["type"]: f for f in hit["filled_order_history"]}
    assert by_type["limit"]["fill_price"] == 108.0
    assert by_type["limit"]["side"] == "buy"
    assert by_type["limit"]["qty"] == 4
    assert by_type["stop"]["fill_price"] == 108.0
    assert by_type["stop"]["side"] == "sell"
    assert by_type["stop"]["qty"] == 3


def test_sell_limit_and_buy_stop_fill_when_bar_range_crosses(tmp_path: Path):
    client, vendor = _paper_client(tmp_path)
    _place(client, instrument="SPY", side="sell", type="limit", qty=2, limit=111.0)
    _place(client, instrument="SPY", side="buy", type="stop", qty=2, stop=111.0)

    vendor.inject_tick("SPY", price=110.0, volume=5.0, ts=float(_SPY_1M_LAST_TS + 2))
    time.sleep(_CONFLATE_S * 2.5)
    assert len(client.get("/v1/snapshot").json()["paper"]["working_orders"]) == 2

    vendor.inject_tick("SPY", price=111.75, volume=5.0, ts=float(_SPY_1M_LAST_TS + 3))
    time.sleep(_CONFLATE_S * 2.5)
    paper = client.get("/v1/snapshot").json()["paper"]
    assert paper["working_orders"] == []
    by_type = {f["type"]: f for f in paper["filled_order_history"]}
    assert by_type["limit"]["fill_price"] == 111.0
    assert by_type["limit"]["side"] == "sell"
    assert by_type["stop"]["fill_price"] == 111.0
    assert by_type["stop"]["side"] == "buy"


def test_roll_that_trades_through_fills_without_inventing_minutes(tmp_path: Path):
    client, vendor = _paper_client(tmp_path)
    _place(client, instrument="SPY", side="buy", type="limit", qty=5, limit=108.0)
    # Current last low 108.5 does not touch 108. Roll a new minute at 107.
    vendor.inject_tick(
        "SPY",
        price=107.0,
        volume=20.0,
        ts=float(_SPY_1M_LAST_TS + _MINUTE),
    )
    time.sleep(_CONFLATE_S * 2.5)
    paper = client.get("/v1/snapshot").json()["paper"]
    assert paper["working_orders"] == []
    fill = paper["filled_order_history"][0]
    assert fill["fill_price"] == 108.0
    assert fill["qty"] == 5
    assert paper["positions"][0]["qty"] == 5
    assert paper["positions"][0]["avg_price"] == 108.0


def test_fill_uses_engine_1m_even_when_chart_is_daily(tmp_path: Path):
    client, vendor = _paper_client(tmp_path)
    interest = client.post(
        "/v1/chart/interest",
        json={"instrument": "SPY", "timeframe": "1D"},
    )
    assert interest.status_code == 200
    assert interest.json()["status"] == "ok"
    _place(client, instrument="SPY", side="buy", type="limit", qty=1, limit=108.0)
    vendor.inject_tick("SPY", price=107.0, volume=10.0, ts=float(_SPY_1M_LAST_TS + 4))
    time.sleep(_CONFLATE_S * 2.5)
    paper = client.get("/v1/snapshot").json()["paper"]
    assert paper["working_orders"] == []
    assert paper["filled_order_history"][0]["fill_price"] == 108.0


def test_fill_uses_engine_1m_when_chart_is_also_1m(tmp_path: Path):
    """A 1m chart must not replace or double-fire the engine-owned eval series."""
    client, vendor = _paper_client(tmp_path)
    interest = client.post(
        "/v1/chart/interest",
        json={"instrument": "SPY", "timeframe": "1m"},
    )
    assert interest.status_code == 200
    assert interest.json()["status"] == "ok"
    _place(client, instrument="SPY", side="buy", type="limit", qty=1, limit=108.0)
    vendor.inject_tick("SPY", price=107.0, volume=10.0, ts=float(_SPY_1M_LAST_TS + 4))
    time.sleep(_CONFLATE_S * 2.5)
    paper = client.get("/v1/snapshot").json()["paper"]
    assert paper["working_orders"] == []
    assert len(paper["filled_order_history"]) == 1
    assert paper["filled_order_history"][0]["fill_price"] == 108.0
    assert paper["positions"][0]["qty"] == 1


def test_uncharted_symbol_still_bar_touch_fills(tmp_path: Path):
    """1m paper interest is armed for working orders even when the symbol is not charted."""
    client, vendor = _paper_client(tmp_path, instruments=("SPY", "QQQ"))
    client.post("/v1/chart/interest", json={"instrument": "SPY", "timeframe": "1D"})
    placed = _place(
        client, instrument="QQQ", side="buy", type="limit", qty=2, limit=208.0
    )
    assert placed.status_code == 200, placed.text
    assert placed.json()["working_orders"][0]["instrument"] == "QQQ"

    vendor.inject_tick("QQQ", price=207.0, volume=8.0, ts=float(_SPY_1M_LAST_TS + 4))
    time.sleep(_CONFLATE_S * 2.5)
    paper = client.get("/v1/snapshot").json()["paper"]
    assert paper["working_orders"] == []
    fill = paper["filled_order_history"][0]
    assert fill["symbol"] == "QQQ"
    assert fill["fill_price"] == 208.0
    assert paper["positions"][0]["symbol"] == "QQQ"
    assert paper["positions"][0]["qty"] == 2


def test_unavailable_1m_leaves_working_order_unfilled(tmp_path: Path):
    store = tmp_path / "workspace.json"
    client, vendor = _client(workspace_path=store)
    placed = _place(client, instrument="SPY", side="buy", type="market", qty=1)
    assert placed.status_code == 200
    vendor.inject_tick("SPY", price=549.0, volume=10.0)
    time.sleep(_CONFLATE_S * 2.5)
    paper = client.get("/v1/snapshot").json()["paper"]
    assert len(paper["working_orders"]) == 1
    assert paper["working_orders"][0]["type"] == "market"
    assert paper["positions"] == []
    assert paper["filled_order_history"] == []
    assert paper["accounts"][0]["balance"] == _DEFAULT_BALANCE


def test_no_partial_fill_on_bar_touch(tmp_path: Path):
    client, vendor = _paper_client(tmp_path)
    _place(client, instrument="SPY", side="buy", type="market", qty=10)
    vendor.inject_tick("SPY", price=111.0, volume=1.0, ts=float(_SPY_1M_LAST_TS + 1))
    time.sleep(_CONFLATE_S * 2.5)
    paper = client.get("/v1/snapshot").json()["paper"]
    assert paper["working_orders"] == []
    assert paper["positions"][0]["qty"] == 10
    assert paper["filled_order_history"][0]["qty"] == 10


def test_manual_flatten_writes_exit_filled_history_leg(tmp_path: Path):
    client, vendor = _paper_client(tmp_path)
    _place(client, instrument="SPY", side="buy", type="market", qty=10)
    vendor.inject_tick("SPY", price=111.0, volume=10.0, ts=float(_SPY_1M_LAST_TS + 1))
    time.sleep(_CONFLATE_S * 2.5)
    opened = client.get("/v1/snapshot").json()["paper"]
    assert opened["positions"][0]["side"] == "long"
    assert opened["accounts"][0]["balance"] == 98_889.0

    closed = client.post("/v1/paper/positions/close", json={"instrument": "SPY"})
    assert closed.status_code == 200, closed.text
    paper = closed.json()
    assert paper["positions"] == []
    assert paper["working_orders"] == []
    assert len(paper["filled_order_history"]) == 2
    entry, exit_leg = paper["filled_order_history"]
    assert entry["side"] == "buy"
    assert entry["type"] == "market"
    assert exit_leg["side"] == "sell"
    assert exit_leg["type"] == "close"
    assert exit_leg["qty"] == 10
    assert exit_leg["symbol"] == "SPY"
    assert exit_leg["fill_price"] == 111.0
    assert exit_leg["commission"] == _DEFAULT_COMMISSION
    # Round-turn: two commissions, no price change → 100000 - 1 - 1
    assert paper["accounts"][0]["balance"] == 99_998.0
    assert paper["balance_history"][-1]["balance"] == 99_998.0

    missing = client.post("/v1/paper/positions/close", json={"instrument": "SPY"})
    assert missing.status_code == 422


def test_fills_and_positions_persist_across_engine_restart(tmp_path: Path):
    store = tmp_path / "workspace.json"
    vendor1 = FakeVendor(auto_ticks=False)
    _seed_1m(vendor1, "SPY")
    client1, _ = _client(workspace_path=store, vendor=vendor1)
    _place(client1, instrument="SPY", side="buy", type="market", qty=6)
    vendor1.inject_tick("SPY", price=111.0, volume=5.0, ts=float(_SPY_1M_LAST_TS + 1))
    time.sleep(_CONFLATE_S * 2.5)
    before = client1.get("/v1/snapshot").json()["paper"]
    fill_id = before["filled_order_history"][0]["id"]
    assert before["positions"][0]["qty"] == 6
    assert before["accounts"][0]["balance"] == 100_000.0 - 6 * 111.0 - 1.0

    vendor2 = FakeVendor(auto_ticks=False)
    _seed_1m(vendor2, "SPY")
    client2, _ = _client(workspace_path=store, vendor=vendor2)
    paper = client2.get("/v1/snapshot").json()["paper"]
    assert paper["working_orders"] == []
    assert paper["positions"][0]["symbol"] == "SPY"
    assert paper["positions"][0]["qty"] == 6
    assert paper["positions"][0]["avg_price"] == 111.0
    assert paper["filled_order_history"][0]["id"] == fill_id
    assert paper["filled_order_history"][0]["fill_price"] == 111.0
    assert paper["filled_order_history"][0]["qty"] == 6
    assert paper["accounts"][0]["balance"] == 99_333.0
    assert paper["balance_history"][0]["balance"] == 99_333.0


def test_burst_of_fills_emits_discrete_paper_ws_events(tmp_path: Path):
    """Fills must not ride latest-wins bar_update; a burst must not drop events."""
    client, vendor = _paper_client(tmp_path)
    with client:
        with client.websocket_connect("/v1/ws") as ws:
            _receive_of_type(ws, "feed_status")
            _place(client, instrument="SPY", side="buy", type="market", qty=2)
            _place(client, instrument="SPY", side="buy", type="limit", qty=3, limit=108.0)

            vendor.inject_tick(
                "SPY", price=107.0, volume=10.0, ts=float(_SPY_1M_LAST_TS + 5)
            )
            time.sleep(_CONFLATE_S * 2.5)

            events: list[dict] = []
            deadline = time.time() + 2.0
            while time.time() < deadline and len(events) < 4:
                msg = ws.receive_json()
                if msg.get("type") == "paper_update":
                    events.append(msg)

            reasons = [ev.get("reason") for ev in events]
            assert reasons[:2] == ["order_placed", "order_placed"]
            fill_events = [ev for ev in events if ev.get("reason") == "order_filled"]
            assert len(fill_events) == 2
            assert fill_events[0]["type"] == "paper_update"
            assert "paper" in fill_events[0]
            # Progressive: first fill leaves one working or a position; second is fully filled.
            assert fill_events[-1]["paper"]["working_orders"] == []
            assert len(fill_events[-1]["paper"]["filled_order_history"]) == 2
            assert len(fill_events[-1]["paper"]["positions"]) == 1
            # Discrete queue — both fill snapshots arrived, not coalesced into one.
            assert len(fill_events[0]["paper"]["filled_order_history"]) == 1


def test_times_in_snapshot_are_unix_not_civil_labels(tmp_path: Path):
    client, vendor = _paper_client(tmp_path)
    _place(client, instrument="SPY", side="buy", type="market", qty=1)
    vendor.inject_tick("SPY", price=111.0, volume=1.0, ts=float(_SPY_1M_LAST_TS + 1))
    time.sleep(_CONFLATE_S * 2.5)
    fill = client.get("/v1/snapshot").json()["paper"]["filled_order_history"][0]
    assert isinstance(fill["placed_ts"], int)
    assert isinstance(fill["filled_ts"], int)
    assert "EDT" not in str(fill)
    assert "EST" not in str(fill)
    assert "America/New_York" not in str(fill)
