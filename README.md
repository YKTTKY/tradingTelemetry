# Trading Telemetry

Personal day-trading terminal: **Python market engine** + **Rust (Ratatui) TUI**, talking over localhost **HTTP** (snapshots/commands) + **WebSocket** (live events). See `CONTEXT.md` and `docs/adr/`.

## Smoke path (two terminals)

### 1. Market engine (uv)

```bash
cd engine
uv sync --extra dev
uv run market-engine
# listens on http://127.0.0.1:8765  (default vendor mode: fake)
```

Quick checks:

```bash
curl -s http://127.0.0.1:8765/v1/snapshot | python3 -m json.tool
# WebSocket: /v1/ws  (feed_status then heartbeat events)
uv run pytest
```

### 2. TUI (cargo)

```bash
cd tui
cargo run
# optional: ENGINE_URL=http://127.0.0.1:8765 cargo run
```

- **Welcome** shows feed status (connected / disconnected).
- **Enter** opens an empty **workspace** shell (charts land in later tickets).
- **q** / **Esc** quits.

The TUI does **not** call any market vendor; only the engine does.

## Layout

| Path | Role |
|------|------|
| `engine/` | uv-managed market engine (FastAPI HTTP+WS) |
| `tui/` | Ratatui client |
| `docs/adr/` | Architecture decisions |
| `.scratch/phase-a-chart-terminal/` | Phase A spec + issues |

## Defaults

- Engine vendor mode: **fake** when no real vendor is selected (`--vendor fake`).
- IPC: JSON over HTTP `/v1/snapshot` and WebSocket `/v1/ws`.
- No ZeroMQ, Redis, or Postgres in v1.
