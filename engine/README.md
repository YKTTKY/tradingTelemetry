# market-engine

Local market engine for Trading Telemetry. Owns vendor I/O, hot state, and the **HTTP + WebSocket** IPC surface for the TUI.

Managed with **[uv](https://github.com/astral-sh/uv)**.

## Setup

```bash
uv sync --extra dev
```

## Run

```bash
uv run market-engine
# uv run market-engine --host 127.0.0.1 --port 8765 --vendor fake
```

Default (and currently only) **vendor mode is `fake`**. Real LSE mode lands with the LSE vendor adapter ticket.

## IPC (v1 skeleton)

| Endpoint | Purpose |
|----------|---------|
| `GET /v1/snapshot` | Bootstrap snapshot including `feed` status |
| `WS /v1/ws` | Live events: `feed_status`, then `heartbeat` |

Example snapshot:

```json
{
  "feed": {
    "status": "connected",
    "vendor_mode": "fake",
    "engine": "up"
  }
}
```

## Test

Primary suite is the **engine IPC seam** (black-box over HTTP+WS):

```bash
uv run pytest -v
```
