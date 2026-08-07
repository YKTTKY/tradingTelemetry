# 02 — History candles for default workspace (fake vendor)

**What to build:** With the fake vendor, the default workspace chart (**SPY** @ **1D** — canonical instrument id, not a `:test` suffix) loads **historical OHLCV bars** over the engine IPC and the TUI draws **candles**. If the vendor marks an instrument/timeframe unavailable, the chart shows **"Data Currently not Available"** with no invented series. CI and default local runs use the **fake** vendor.

**Blocked by:** 01 — Two-process skeleton (uv engine + TUI IPC heartbeat)

**Status:** done

- [x] Vendor **seam** exists: fake adapter implements history for known instruments (at least SPY @ 1D)
- [x] Domain/IPC instrument ids are canonical (`SPY`, `QQQ`, …) — **not** `SPY:test`; fake vs real is vendor mode, not ticker suffix
- [x] Chart interest / subscribe via HTTP yields historical bars for default **SPY** @ **1D**
- [x] TUI renders candles for that series in the default single-layout workspace
- [x] Unavailable instrument or timeframe produces explicit empty state copy **Data Currently not Available** (no fake OHLCV)
- [x] Engine IPC contract tests cover history + unavailable with the fake vendor (primary test seam)
- [x] Feed status can report vendor mode **fake**

## Notes

- Parent spec: `.scratch/phase-a-chart-terminal/spec.md`
- Fake data uses the same Instrument vocabulary as production
- **Candle polish deferred:** v1 canvas candles are functional only (block-marker bodies). Do **not** expand this ticket for visual polish. Improve chart rendering later when live bars, overlays (MA/Volume/VP), or dual layout force a real chart-view path — or as a short dedicated polish pass after the series path is stable.

## Comments

### Implementation notes

- **Vendor seam:** `engine/src/market_engine/vendor.py` — `MarketDataVendor` protocol, `FakeVendor` with deterministic SPY/QQQ @ 1D history.
- **IPC:** `POST /v1/chart/interest` `{instrument, timeframe}` → `{status: ok|unavailable, bars: [...]}`.
- **TUI:** default workspace single · SPY · 1D; on Enter loads history; canvas candle chart; empty state uses exact copy.

### Follow-up (not blocking done)

- Candlestick **visual quality** (spacing, wick/body, markers, price axis) is out of scope for 02; polish in a future ticket once chart chrome stabilizes.
