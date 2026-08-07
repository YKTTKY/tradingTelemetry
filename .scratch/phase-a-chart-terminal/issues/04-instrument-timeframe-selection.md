# 04 — Instrument and timeframe selection

**What to build:** The trader can change the focused chart’s **Instrument** and **Timeframe**. Supported timeframes are exactly: `1m`, `3m`, `5m`, `15m`, `30m`, `1h`, `4h`, `1D`, `1W`. On change, history reloads (and live interest follows). Unavailable combinations show **"Data Currently not Available"**.

**Blocked by:** 02 — History candles for default workspace (fake vendor)

**Status:** ready-for-agent

- [ ] TUI can change instrument on the focused chart; engine serves bars for the new interest
- [ ] TUI can change timeframe across the full v1 set only (no other intervals)
- [ ] History reloads on instrument or timeframe change; live subscription tracks the new pair
- [ ] Unavailable instrument/timeframe shows **Data Currently not Available** without inventing bars
- [ ] Fake vendor supports enough symbols/timeframes for contract tests (success + unavailable cases)
- [ ] IPC/contract tests cover selection and reload behavior

## Notes

- Parent spec: `.scratch/phase-a-chart-terminal/spec.md`
- Can proceed in parallel with 03 after 02; recommended path still runs 03 before leaning on live demos
