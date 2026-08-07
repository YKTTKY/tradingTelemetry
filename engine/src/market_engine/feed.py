"""Feed connectivity state exposed via snapshot and WebSocket events."""

from __future__ import annotations

from dataclasses import dataclass
from typing import Literal

VendorMode = Literal["fake", "lse"]
FeedConnectivity = Literal["connected", "disconnected"]


@dataclass(frozen=True)
class FeedState:
    """Engine + vendor connectivity as seen by the TUI."""

    status: FeedConnectivity
    vendor_mode: VendorMode
    engine: Literal["up", "down"] = "up"

    def to_snapshot(self) -> dict:
        return {
            "status": self.status,
            "vendor_mode": self.vendor_mode,
            "engine": self.engine,
        }

    def to_ws_event(self) -> dict:
        return {
            "type": "feed_status",
            "status": self.status,
            "vendor_mode": self.vendor_mode,
        }


def default_feed_state(vendor_mode: VendorMode | None = None) -> FeedState:
    """Default local desk: engine up, no real vendor selected → fake mode."""
    mode: VendorMode = vendor_mode if vendor_mode is not None else "fake"
    return FeedState(status="connected", vendor_mode=mode, engine="up")
