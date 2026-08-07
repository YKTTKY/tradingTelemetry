"""CLI entry: run the market engine HTTP+WS server on localhost."""

from __future__ import annotations

import argparse

import uvicorn


def main() -> None:
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
        choices=("fake",),
        default="fake",
        help="Market-data vendor mode (default: fake; real vendors land later)",
    )
    args = parser.parse_args()

    # Import after parse so --help stays light.
    from market_engine.app import create_app
    from market_engine.feed import default_feed_state
    from market_engine.vendor import default_vendor

    feed = default_feed_state(vendor_mode=args.vendor)
    app = create_app(feed=feed, vendor=default_vendor(args.vendor))
    uvicorn.run(app, host=args.host, port=args.port, log_level="info")


if __name__ == "__main__":
    main()
