# Trading Telemetry

Personal day-trading terminal: **Python market engine** + **Rust (Ratatui) TUI**, talking over localhost **HTTP** (snapshots/commands) + **WebSocket** (live events). See `CONTEXT.md` and `docs/adr/`.

## Smoke path (two terminals)

### 1. Market engine (uv)

```bash
cd engine
uv sync --extra dev
uv run market-engine
# listens on http://127.0.0.1:8765  (default vendor mode: fake)
# Production LSE:  export LSE_API_KEY=... && uv run market-engine --vendor lse
```

Quick checks:

```bash
curl -s http://127.0.0.1:8765/v1/snapshot | python3 -m json.tool
curl -s -X POST http://127.0.0.1:8765/v1/chart/interest \
  -H 'Content-Type: application/json' \
  -d '{"instrument":"SPY","timeframe":"1D"}' | python3 -m json.tool
# WebSocket: /v1/ws  (feed_status then heartbeat events)
uv run pytest
```

### 2. TUI (cargo)

```bash
cd tui
cargo run
# optional: ENGINE_URL=http://127.0.0.1:8765 cargo run
```

- **Welcome** shows feed status (connected / disconnected; vendor mode **fake** by default, or **lse** when the engine was started with `--vendor lse`).
- **Enter** opens the **workspace** from the engine snapshot: first launch is `single` + **SPY** @ **1D**; later launches restore last layout and per-chart instrument/timeframe (file-backed on the engine).
- **Workspace keys:** `l` toggle layout (`single` ↔ `dual-vertical`); **Tab** focus chart (dual); `]` / `[` cycle timeframe (`1m`…`1W`); `i` change instrument (type symbol, Enter apply, Esc cancel); **q** quits.
- Dual first open (no saved dual selection): top **QQQ** @ **1D**, bottom **SPY** @ **1D**. Each chart has independent interest.
- Unavailable instrument/timeframe shows **Data Currently not Available** (no invented series).
- Fake vendor history covers **SPY**@**1D**, **SPY**@**1h**, **QQQ**@**1D**, **ES**@**1D** (other pairs may be unavailable).
- LSE mode uses the same IPC; history + live ticks come from London Strategic Edge when credentials are set.

The TUI does **not** call any market vendor; only the engine does.

## Layout

| Path | Role |
|------|------|
| `engine/` | uv-managed market engine (FastAPI HTTP+WS) |
| `tui/` | Ratatui client |
| `docs/adr/` | Architecture decisions |
| `.scratch/phase-a-chart-terminal/` | Phase A spec + issues |

## Defaults

- Engine vendor mode: **fake** by default (`--vendor fake`); production **lse** via `--vendor lse` + `LSE_API_KEY` (or `MARKET_ENGINE_VENDOR=lse`).
- Default chart: **SPY** @ **1D** (canonical instrument id — not `SPY:test`).
- Workspace persistence: engine file (default `~/.local/share/trading-telemetry/workspace.json`); no Redis/Postgres.
- IPC: JSON over HTTP `/v1/snapshot`, `/v1/workspace`, `/v1/chart/interest`, and WebSocket `/v1/ws`.
- No ZeroMQ, Redis, or Postgres in v1.
