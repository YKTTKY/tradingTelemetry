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

## IPC (v1)

| Endpoint | Purpose |
|----------|---------|
| `GET /v1/snapshot` | Bootstrap snapshot including `feed` status |
| `POST /v1/chart/interest` | Chart interest: historical OHLCV for `instrument` + `timeframe`; arms live updates |
| `WS /v1/ws` | Live events: `feed_status`, `heartbeat`, conflated `bar_update` |

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

Example chart interest (fake vendor knows **SPY** @ **1D**; unknown pairs return `status: unavailable` with empty `bars`):

```bash
curl -s -X POST http://127.0.0.1:8765/v1/chart/interest \
  -H 'Content-Type: application/json' \
  -d '{"instrument":"SPY","timeframe":"1D"}'
```

```json
{
  "instrument": "SPY",
  "timeframe": "1D",
  "status": "ok",
  "bars": [
    {"ts": 1719792000, "open": 540.0, "high": 540.5, "low": 539.5, "close": 540.0, "volume": 50000000.0}
  ]
}
```

Instrument ids are **canonical** (`SPY`, `QQQ`, …) — never a `:test` suffix. Fake vs real is `vendor_mode`, not ticker encoding.

### Live bar updates (WebSocket)

After chart interest succeeds, the fake vendor can emit ticks (auto random-walk in the CLI process, or `inject_tick` in tests). The engine aggregates ticks into the **last bar** (and rolls a new bar on period boundaries). Updates are **conflated** (~50ms default) so a tick burst yields fewer `bar_update` frames than raw ticks:

```json
{
  "type": "bar_update",
  "instrument": "SPY",
  "timeframe": "1D",
  "completed_bars": [],
  "bar": {"ts": 1720569600, "open": 546.25, "high": 549.25, "low": 545.75, "close": 549.25, "volume": 50910000.0}
}
```

`completed_bars` is non-empty when a period rolls (previous tip closed); `bar` is always the current series tip.

## Test

Primary suite is the **engine IPC seam** (black-box over HTTP+WS):

```bash
uv run pytest -v
```
