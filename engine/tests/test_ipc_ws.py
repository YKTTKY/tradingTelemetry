"""Engine IPC seam: WebSocket delivers live feed status / heartbeat events."""

from fastapi.testclient import TestClient

from market_engine.app import create_app


def test_websocket_delivers_feed_status_then_heartbeat():
    client = TestClient(create_app())

    with client.websocket_connect("/v1/ws") as ws:
        first = ws.receive_json()
        assert first["type"] == "feed_status"
        assert first["status"] == "connected"
        assert first["vendor_mode"] == "fake"

        second = ws.receive_json()
        assert second["type"] == "heartbeat"
        assert "ts" in second
