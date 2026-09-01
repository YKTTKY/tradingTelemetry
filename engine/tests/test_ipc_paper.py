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
