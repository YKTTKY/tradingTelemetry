"""Engine IPC seam: HTTP snapshot includes feed status and fake vendor mode."""

from fastapi.testclient import TestClient

from market_engine.app import create_app


def test_snapshot_includes_feed_status_and_fake_vendor_mode():
    client = TestClient(create_app())

    response = client.get("/v1/snapshot")

    assert response.status_code == 200
    body = response.json()
    assert "feed" in body
    assert body["feed"]["status"] == "connected"
    assert body["feed"]["vendor_mode"] == "fake"
    assert body["feed"]["engine"] == "up"
