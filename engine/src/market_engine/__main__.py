"""CLI entry: run the market engine HTTP+WS server on localhost."""

from __future__ import annotations

import argparse
import os
from pathlib import Path

import uvicorn


def _env_vendor_default() -> str:
    raw = os.environ.get("MARKET_ENGINE_VENDOR", "fake").strip().lower()
    if raw in ("fake", "lse"):
        return raw
    return "fake"


def _default_workspace_path() -> str:
    raw = os.environ.get("MARKET_ENGINE_WORKSPACE", "").strip()
    if raw:
        return raw
    return str(Path.home() / ".local" / "share" / "trading-telemetry" / "workspace.json")


def build_parser() -> argparse.ArgumentParser:
    """Argparse surface for the engine CLI (also used by contract tests)."""
    parser = argparse.ArgumentParser(description="Trading Telemetry market engine")
    parser.add_argument(
        "--host",
        default="127.0.0.1",
        help="Bind host (default: 127.0.0.1)",
    )
    parser.add_argument(
        "--port",
        type=int,
        default=8765,
        help="Bind port (default: 8765)",
    )
    parser.add_argument(
        "--vendor",
        choices=("fake", "lse"),
        default=_env_vendor_default(),
        help=(
            "Market-data vendor mode: fake (default, CI/offline) or lse "
            "(London Strategic Edge; requires LSE_API_KEY). "
            "Also settable via MARKET_ENGINE_VENDOR."
        ),
    )
    parser.add_argument(
        "--workspace",
        default=_default_workspace_path(),
        help=(
            "Path to file-backed workspace JSON "
            "(default: ~/.local/share/trading-telemetry/workspace.json "
            "or MARKET_ENGINE_WORKSPACE)."
        ),
    )
    return parser


def main() -> None:
    parser = build_parser()
    args = parser.parse_args()

    # Import after parse so --help stays light.
    from market_engine.app import create_app
    from market_engine.feed import default_feed_state
    from market_engine.vendor import default_vendor

    feed = default_feed_state(vendor_mode=args.vendor)
    # Interactive fake: random-walk last price. LSE: live stream from vendor.
    auto_ticks = args.vendor == "fake"
    app = create_app(
        feed=feed,
        vendor=default_vendor(args.vendor, auto_ticks=auto_ticks),
        workspace_path=Path(args.workspace),
    )
    uvicorn.run(app, host=args.host, port=args.port, log_level="info")


if __name__ == "__main__":
    main()
