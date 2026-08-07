# 04 — Instrument and timeframe selection

**What to build:** The trader can change the focused chart’s **Instrument** and **Timeframe**. Supported timeframes are exactly: `1m`, `3m`, `5m`, `15m`, `30m`, `1h`, `4h`, `1D`, `1W`. On change, history reloads (and live interest follows). Unavailable combinations show **"Data Currently not Available"**.

**Blocked by:** 02 — History candles for default workspace (fake vendor)

**Status:** done

- [x] TUI can change instrument on the focused chart; engine serves bars for the new interest
- [x] TUI can change timeframe across the full v1 set only (no other intervals)
- [x] History reloads on instrument or timeframe change; live subscription tracks the new pair
- [x] Unavailable instrument/timeframe shows **Data Currently not Available** without inventing bars
- [x] Fake vendor supports enough symbols/timeframes for contract tests (success + unavailable cases)
- [x] IPC/contract tests cover selection and reload behavior

## Notes

- Parent spec: `.scratch/phase-a-chart-terminal/spec.md`
- Can proceed in parallel with 03 after 02; recommended path still runs 03 before leaning on live demos

## Comments

### Implementation notes

- **Fake history pairs:** SPY@1D, SPY@1h, QQQ@1D, ES@1D; other v1 timeframes accepted but may return unavailable.
- **Interest replace:** each `POST /v1/chart/interest` is the sole active pair (live follows selection). Multi-interest for dual layout is deferred to issue 05.
- **TUI keys:** `]` / `[` cycle v1 timeframes; `i` instrument prompt (Enter apply, Esc cancel).
- **Tests:** `engine/tests/test_ipc_chart_selection.py` + TUI unit tests for cycle/prompt/reload.
