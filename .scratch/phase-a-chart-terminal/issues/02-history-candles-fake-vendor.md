# 02 — History candles for default workspace (fake vendor)

**What to build:** With the fake vendor, the default workspace chart (**SPY** @ **1D** — canonical instrument id, not a `:test` suffix) loads **historical OHLCV bars** over the engine IPC and the TUI draws **candles**. If the vendor marks an instrument/timeframe unavailable, the chart shows **"Data Currently not Available"** with no invented series. CI and default local runs use the **fake** vendor.

**Blocked by:** 01 — Two-process skeleton (uv engine + TUI IPC heartbeat)

**Status:** ready-for-agent

- [ ] Vendor **seam** exists: fake adapter implements history for known instruments (at least SPY @ 1D)
- [ ] Domain/IPC instrument ids are canonical (`SPY`, `QQQ`, …) — **not** `SPY:test`; fake vs real is vendor mode, not ticker suffix
- [ ] Chart interest / subscribe via HTTP yields historical bars for default **SPY** @ **1D**
- [ ] TUI renders candles for that series in the default single-layout workspace
- [ ] Unavailable instrument or timeframe produces explicit empty state copy **Data Currently not Available** (no fake OHLCV)
- [ ] Engine IPC contract tests cover history + unavailable with the fake vendor (primary test seam)
- [ ] Feed status can report vendor mode **fake**

## Notes

- Parent spec: `.scratch/phase-a-chart-terminal/spec.md`
- Fake data uses the same Instrument vocabulary as production
