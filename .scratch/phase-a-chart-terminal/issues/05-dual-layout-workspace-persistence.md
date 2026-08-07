# 05 — Dual layout + workspace persistence

**What to build:** The trader can toggle **layout mode** between `single` and `dual-vertical` (two equal-height charts, top/bottom). First dual open with nothing saved uses top **QQQ** @ **1D**, bottom **SPY** @ **1D**. Each chart has independent instrument, timeframe, and later indicator set. **Welcome** then workspace on launch; **file-backed** restore of layout mode and per-chart instrument/timeframe on later launches.

**Blocked by:** 04 — Instrument and timeframe selection

**Status:** done

- [x] User can toggle between `single` and `dual-vertical` at any time
- [x] Dual layout shows two stacked charts with independent interest (instrument + timeframe)
- [x] When dual has no saved selection, defaults are top **QQQ** @ **1D**, bottom **SPY** @ **1D**
- [x] First launch path: Welcome → workspace `single`, **SPY** @ **1D** (unless already established in earlier tickets — keep consistent)
- [x] Later launch: after Welcome, restore last layout mode and each chart’s instrument and timeframe from disk
- [x] Engine snapshot on connect is enough for TUI to rebuild workspace interest
- [x] Persistence does not use Redis/Postgres
- [x] Tests cover save/restore of layout + chart selections (IPC and/or engine restart with same store)

## Notes

- Parent spec: `.scratch/phase-a-chart-terminal/spec.md`
- Indicator restore arrives with ticket 07; this ticket owns layout + instruments + timeframes

## Comments

### Implementation notes

- **IPC:** `GET /v1/snapshot` includes `workspace`; `POST /v1/workspace` sets `layout_mode`; `POST /v1/chart/interest` accepts optional `chart_id` (`primary` | `top` | `bottom`) and returns it; multi-interest concurrent for dual.
- **Persistence:** file-backed JSON via `WorkspaceStore` (default `~/.local/share/trading-telemetry/workspace.json`, override `--workspace` / `MARKET_ENGINE_WORKSPACE`). Single vs dual chart memories are independent.
- **Dual defaults:** first dual with no saved dual memory → top QQQ@1D, bottom SPY@1D.
- **TUI keys:** `l` layout toggle; **Tab** focus chart in dual; instrument/timeframe still apply to focused chart.
- **Tests:** `engine/tests/test_ipc_workspace.py` (save/restore, dual interest, dual live) + TUI unit tests for restore/focus/toggle.
