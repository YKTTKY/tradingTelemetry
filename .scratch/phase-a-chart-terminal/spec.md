# Phase A — Chart Terminal (v1 first ship)

Status: ready-for-agent

## Problem Statement

As a personal day trader, I need a fast local terminal (TUI) to watch instruments, multi-timeframe candles, watchlists, and key overlays (especially Volume Profile levels for confluence) without paying TradingView complexity tax or wiring a full brokerage stack yet.

Today there is no product in this repo — only domain language and architecture decisions from the first grill. I cannot launch a workspace, stream LSE data, chart, or restore layout/indicators. Paper trading is deliberately later; the immediate gap is a usable **chart terminal**.

## Solution

Ship **Phase A — Chart terminal**: a two-process local app where a **Python market engine** (managed with **uv**) owns LSE market data, bars, and indicator compute, and a **Rust (Ratatui) TUI** owns layout, input, watchlists, and indicator configuration. They talk over localhost **HTTP** (commands + snapshots) + **WebSocket** (live conflated events).

On first launch the user sees a Welcome path into a workspace (default **SPY** @ **1D**, single layout). Later launches restore layout, instruments, timeframes, and per-chart indicators. The right sidebar holds multiple watchlists with live last/change fields. Charts start naked and can attach MA, Volume, and Session / Fixed Range / Anchored Volume Profile (with documented v1 limits and parameters). Unavailable vendor data shows an explicit empty state, not fake bars.

**No paper trading in this ship.**

## User Stories

1. As a day trader, I want a local chart terminal that starts in two processes (engine + TUI), so that Python can own market math while Rust owns a responsive UI.
2. As a day trader, I want the engine process managed with **uv** (deps, lockfile, run, test), so that Python environment setup is reproducible and agent-friendly.
3. As a day trader, I want the TUI to never call LSE directly, so that vendor credentials and feed logic stay in one place.
4. As a day trader, I want HTTP for commands and full state snapshots, so that the TUI can reconnect and recover cleanly.
5. As a day trader, I want WebSocket live updates for quotes, bars, and indicators, so that the UI stays current without polling every tick.
6. As a day trader, I want live updates conflated/throttled for the UI, so that the terminal stays snappy under busy markets.
7. As a first-time user, I want a Welcome experience before the workspace, so that I know the app started and I am entering the chart shell.
8. As a first-time user, I want the default workspace in layout mode `single`, instrument **SPY**, timeframe **1D**, so that I land on a useful chart immediately.
9. As a returning user, I want my last layout mode, per-chart instruments, and per-chart timeframes restored after Welcome, so that I resume where I left off.
10. As a day trader, I want to toggle layout between `single` and `dual-vertical` at any time, so that I can compare two charts stacked top/bottom.
11. As a day trader, when I first open dual layout with no saved dual selection, I want top **QQQ** @ **1D** and bottom **SPY** @ **1D**, so that dual mode has sensible defaults.
12. As a day trader, I want each chart in dual layout to have its own instrument, timeframe, and indicator set, so that comparisons are independent.
13. As a day trader, I want to select an instrument per chart, so that charts are not pinned to a single symbol forever.
14. As a day trader, I want timeframes limited to `1m`, `3m`, `5m`, `15m`, `30m`, `1h`, `4h`, `1D`, `1W`, so that the product stays focused and testable.
15. As a day trader, I want candles (OHLCV bars) for the selected instrument and timeframe, so that I can read price action.
16. As a day trader, I want historical bars on chart open / instrument change / timeframe change, so that I am not staring at an empty live-only strip.
17. As a day trader, I want the last bar to update live as the market moves, so that the chart tracks the session.
18. As a day trader, when LSE cannot resolve or serve an instrument or timeframe, I want the chart empty state **"Data Currently not Available"**, so that I never trust silent blanks or invented series.
19. As a day trader, I want a watchlist sidebar docked on the right that I can show/hide, so that quote context is available without permanently stealing chart width.
20. As a day trader, I want multiple named watchlists (like sheets) and a switcher for the active list, so that I can group instruments by theme.
21. As a first-time user, I want a default watchlist (e.g. Core) containing **ES, NQ, SPY, QQQ, SOXL**, and **VIX** if the vendor resolves it, so that I start with liquid names.
22. As a day trader, if VIX (or any default symbol) is unavailable, I want that row omitted or marked unavailable without failing the whole watchlist, so that partial vendor coverage does not brick the sidebar.
23. As a day trader, I want watchlist rows showing **symbol**, **last price**, **change** (last − previous day close), and **change percent**, so that I can scan bias quickly.
24. As a day trader, I want up moves green and down moves red on watchlist change fields, so that direction is glanceable.
25. As a day trader, I want to add a symbol to the active watchlist via add/search chrome, so that I can extend the list without editing config files.
26. As a day trader, I want to remove a symbol from a watchlist, so that dead names do not clutter the list.
27. As a day trader, I want watchlist last prices to update from live (conflated) quotes, so that the sidebar tracks the market.
28. As a day trader, I do not want logos on watchlist rows in v1, so that implementation stays lean.
29. As a day trader, I want charts to open **naked** (no indicators) the first time a chart has no saved indicator state, so that the canvas starts clean.
30. As a day trader, I want last-used indicators and their settings restored per chart after I have configured them, so that I do not re-add MA/VP every session.
31. As a day trader, I want an indicator panel to add, toggle, and configure indicators for the focused chart, so that overlays are intentional not global.
32. As a day trader, I want Moving Averages on the price chart: up to **3** lines per chart, each **SMA or EMA**, default lengths **10 / 60 / 200** when adding the default stack, so that I get standard swing structure.
33. As a day trader, I want per-bar **Volume** as a histogram in a sub-pane under price, at most **1** Volume instance per chart, so that participation is visible without stacking duplicates.
34. As a day trader, I want **Session Volume Profile** (max **1** per chart), mode **All** only, one profile per day, so that I see daily volume-by-price structure.
35. As a day trader, I want Session VP day bounds in America/New_York: equities/ETFs **16:00 → next day 16:00**; CME ES/NQ **prior day 18:00 → 17:00**, so that session windows match how I trade those products.
36. As a day trader, I want Session VP parameters for box width (% of session span), left/right placement, number of rows (default **500**), value area volume (default **70%**), histogram color/opacity, and toggleable VAH/POC/VAL with color/opacity, so that I can tune readability and confluence lines.
37. As a day trader, I want **Fixed Range Volume Profile** (max **4** per chart) between two time anchors, so that I can profile a chosen window (e.g. this week + last week).
38. As a day trader, I want Fixed Range **extend to right** to both accumulate volume from new bars past the end anchor and project POC/VAH/VAL levels rightward when on, so that confluence stays live after the original window.
39. As a day trader, when Fixed Range extend is off, I want only the closed [start, end] window to feed the profile and levels not to project past that window, so that the range stays frozen.
40. As a day trader, I want Fixed Range defaults including number of rows **200**, value area **70%**, box width, histogram and level styling, so that new ranges are usable immediately.
41. As a day trader, I want **Anchored Volume Profile** (max **2** per chart) from one anchor forward to now (typical cash open **09:30 America/New_York**), so that I can track intraday profile from a chosen start.
42. As a day trader, I want Anchored VP defaults of number of rows **500**, value area **70%**, and toggleable VAH/POC/VAL styling, so that anchors match Session-like resolution.
43. As a day trader, I want POC, VAH, and VAL defined from the profile’s volume distribution (value area ~ configured % of total volume), so that levels are real profile stats not naive high/low percentages.
44. As a day trader, I want VP vertical resolution as **number of rows** (equal price buckets across the profile high–low), not ticks-per-row, so that configuration matches the product language.
45. As a day trader, I want Volume Profile drawn as a horizontal histogram overlay on the price chart with tunable opacity, so that candles remain readable.
46. As a day trader, I want optional **GEX** only when options data and compute succeed; otherwise unavailable without breaking charts, so that GEX is never fake.
47. As a day trader, I want optional **GARCH** only when history allows a stable estimate; otherwise unavailable, so that volatility models do not invent certainty.
48. As a day trader, I want feed / connection status visible, so that I know whether the engine and primary vendor are healthy.
49. As a day trader, I want workspace configuration persisted on disk (layout, charts, watchlists, indicator settings), so that restarts restore my desk.
50. As a day trader, I want the engine to hold hot state in memory (quotes, bar rings, indicator snapshots) and serve a snapshot on TUI connect, so that reconnect is fast.
51. As an implementer, I want Message bodies on IPC to be JSON or MessagePack, so that debug and performance tradeoffs remain open without a protocol rewrite.
52. As an implementer, I want no ZeroMQ, Redis, or Postgres in this ship, so that the local desk stays minimal (ADR-0001, ADR-0002).
53. As an implementer, I want a market-data vendor adapter so tests use a fake vendor and production uses LSE, so that CI never depends on live LSE.
54. As an implementer, I want the primary product test surface to be the engine HTTP+WS contract, so that behavior is verified at the highest useful seam.
55. As a day trader, I do not want paper accounts, orders, fills, trade marks, or SQLite trade books in this ship, so that Phase A stays chart-only.
56. As a day trader, I do not want delta volume bubbles, freeform multi-grid layouts beyond single/dual-vertical, or non-LSE equal primary vendors in this ship, so that scope stays shippable.

## Implementation Decisions

### Scope and phasing

- This spec is **Phase A — Chart terminal (first ship)** only. **Phase B — Paper M1+nudge** is out of scope here (see Out of Scope).
- Product vocabulary follows `CONTEXT.md`. Prefer glossary terms (Chart, Layout mode, Instrument, Timeframe, Workspace, Watchlist, Indicator, Session/Fixed Range/Anchored VP, POC/VAH/VAL, Unavailable data).

### Process architecture (ADR-0001)

- Two local processes:
  - **Market engine (Python):** primary vendor I/O (LSE), historical + live bars, quote fan-in, indicator compute (MA, Volume, VP variants; optional GEX/GARCH), hot in-memory state, workspace persistence coordination as needed, HTTP+WS server.
  - **TUI (Rust / Ratatui):** Welcome + workspace shell, layout modes, chart rendering, mouse/keyboard, watchlist sidebar UI, indicator panel UI; **does not** call LSE in v1.
- Single-user desktop desk: no multi-tenant backend, no Postgres-first design.

### Python toolchain

- Manage the market engine with **uv**:
  - Project init / package metadata via uv
  - Locked dependencies (`uv.lock`) checked in
  - Dev commands: sync, run engine, run tests, add deps through uv
  - Prefer uv-native workflows over ad-hoc venv + pip scripts
- Document the minimal run path (e.g. start engine via uv, start TUI via cargo) in Further Notes / eventual README when implementation lands — not a separate product feature.

### IPC (ADR-0002)

- **HTTP:** commands (subscribe/unsubscribe instrument+timeframe, change workspace-driven interest, indicator config apply, watchlist mutations that need server-side quote interest, snapshot fetch) + full/partial state snapshots on demand or reconnect.
- **WebSocket:** live **conflated** events for quotes, bar updates, indicator payloads, feed status.
- Payload encoding: **JSON or MessagePack** (pick one default in implementation if needed; both allowed by ADR).
- **Publish policy:** do not stream every market tick to the TUI; conflate for UI snappiness.
- **No ZeroMQ, Redis, or Postgres** in Phase A.

### Test seams (confirmed)

Two real seams for Phase A:

1. **Engine IPC (primary / highest)** — HTTP + WebSocket contract. Engine behavior is tested as a black box through this interface. Optional later: TUI against a stub engine at the same contract.
2. **Market-data vendor adapter (secondary)** — engine depends on a vendor interface; production adapter = LSE; test adapter = in-process fake (history, live-ish updates, unavailable instruments/timeframes). CI default = fake.

Internal concerns (bar rings, VP math, conflation timers, workspace file format) are implementation behind those seams unless a second production adapter appears.

### Market data

- **Primary data vendor:** London Strategic Edge (LSE).
- Engine maps domain Instruments and Timeframes to vendor identifiers/resolutions at the vendor adapter boundary only.
- **Unavailable data:** explicit chart empty state copy **Data Currently not Available**; no invented bars; no pretending the stream is live.

### Workspace and defaults

- **Workspace** = layout mode + each chart’s instrument and timeframe (plus restored indicators per chart after first configuration).
- **Layout modes:** only `single` and `dual-vertical` (two charts, equal height, top/bottom).
- **First launch:** Welcome → workspace `single`, **SPY** @ **1D**.
- **Later launches:** Welcome → restore last layout mode, instruments, timeframes; restore per-chart indicators when previously set.
- **Dual defaults when nothing saved:** top **QQQ** @ **1D**, bottom **SPY** @ **1D**.
- Persistence: **file-backed workspace** (and optional parquet warm-start for history is allowed but not required for first vertical slice). No Redis/Postgres.

### Watchlists

- Multiple named watchlists; one visible (active) in the sidebar at a time.
- Sidebar: docked **right**, **togglable**; chrome = watchlist name/switcher + add symbol; header row `symbol | last | change | change%`; data rows with green/red change styling.
- Default list (name flexible, e.g. Core): **ES, NQ, SPY, QQQ, SOXL**, + **VIX** only if vendor resolves it.
- No logos in v1.
- Watchlist ≠ Position (paper/live holdings are Phase B+).

### Indicators (v1 set)

| Indicator | Max per chart | Notes |
|-----------|---------------|--------|
| MA lines | 3 | SMA or EMA per line; default lengths 10 / 60 / 200 for default stack |
| Volume | 1 | Sub-pane histogram under price |
| Session VP | 1 | Mode **All** only; session clocks as in glossary |
| Fixed Range VP | 4 | Two time anchors; extend-to-right = live build + level projection |
| Anchored VP | 2 | One anchor → now; typical 09:30 America/New_York |
| GEX | optional | Only if options data + compute succeed |
| GARCH | optional | Only if history allows stable estimate |
| Delta bubbles | **out** | Not Phase A |

- Indicators attach to a **specific chart**, not globally; dual layout has independent sets.
- First-ever chart open: **naked**. Thereafter restore last-used indicators/settings per chart.
- Indicator panel owns add / toggle / configure.

### Session VP parameters (must implement)

- Sessions: **All** only; one profile per day.
- Day bounds America/New_York:
  - US equities/ETFs: 16:00 → next calendar day 16:00
  - CME ES/NQ: prior calendar day 18:00 → 17:00
- Box width (% of session horizontal span), placement left/right
- Number of rows default **500** (equal price buckets)
- Value area volume default **70%**
- Histogram color/opacity; VAH/POC/VAL each color/opacity + on/off

### Fixed Range VP parameters (must implement)

- Two time anchors (start, end)
- Extend to right on/off with **both** behaviors when on: accumulate post-end bars; project POC/VAH/VAL right
- Number of rows default **200**; value area **70%**; box width; styling/toggles as Session

### Anchored VP parameters (must implement)

- One anchor forward; number of rows default **500**; value area **70%**; volume + level styling/toggles

### Feed status

- Surface engine/vendor connectivity (or degraded) state to the TUI via snapshot and/or WS events so the user can trust or distrust the glass.

### Modules (conceptual — no frozen paths)

Build/modify along these module responsibilities (names indicative, not file paths):

- **Vendor adapter** — LSE + fake; instrument/timeframe resolution; history fetch; live subscription; unavailable signals.
- **Bar store / series** — per instrument+timeframe rings; historical load; live bar update.
- **Indicator engine** — MA, Volume, Session/Fixed/Anchored VP; optional GEX/GARCH; per-chart instance limits and settings.
- **Quote service** — last, previous close, change fields for watchlist interest set.
- **Workspace store** — load/save layout, charts, watchlists, indicator configs.
- **IPC server** — HTTP commands/snapshots + WS conflated publish.
- **TUI shell** — Welcome, layout, focus, keybindings (exact bindings can land as implementation choices if not yet frozen).
- **Chart view** — candles, panes, overlays, empty state.
- **Watchlist view** — multi-list switcher, rows, add/remove.
- **Indicator panel** — catalog, toggles, settings editors for MA/VP params.

### API contract expectations (behavioral, not frozen OpenAPI)

The IPC should support at least:

- Connect → snapshot of workspace-relevant hot state + feed status
- Express interest in instruments/timeframes for charts and watchlist quotes
- Deliver historical bars then live bar updates for chart interest
- Apply indicator configs and return/stream indicator snapshots consistent with limits
- Mutate watchlists and reflect quote rows
- Persist workspace so a cold restart + new TUI session restores Phase A state

Exact route/event names are left to implementation, but tests must lock behavior at this seam once defined.

### Keybindings

- Exact keybindings for layout toggle, watchlist show/hide, indicator panel, focus between dual charts are **implementation-defined** if still TBD in domain docs; choose sensible defaults and keep them discoverable (help strip or Welcome). Do not block Phase A on perfect keybinding aesthetics.

## Testing Decisions

### What makes a good test

- Assert **external behavior** through the agreed seams, not private helpers, private file layouts, or pure function call graphs.
- Prefer: given a fake vendor producing known bars/quotes → engine IPC → expect history, live updates, indicator payloads, empty states, watchlist fields, restore-after-restart.
- Avoid: snapshotting internal class structure, asserting call order into private methods, or re-testing Ratatui draw calls for every indicator pixel.

### Primary suite — engine IPC (seam 1)

Test the market engine as a black box over HTTP+WS with the **fake vendor** (seam 2):

- Snapshot on connect includes feed status and enough state for TUI bootstrap
- Subscribe / chart interest returns historical bars for a known instrument+timeframe
- Live bar updates appear on the WebSocket (conflated, not necessarily tick-perfect)
- Unavailable instrument/timeframe yields explicit unavailable behavior (no fake OHLCV series)
- Watchlist interest yields last / change / change% consistent with fake previous close and last
- Indicator apply: MA lines with type/length; Volume presence; Session VP structure with POC/VAH/VAL under fixed fake volume-by-price; Fixed Range with extend on/off behavioral differences; Anchored from a known anchor
- Per-chart instance limits enforced (reject or clamp with clear behavior — pick one and test it)
- Workspace save/load: restart engine (or reload store) restores layout, instruments, timeframes, indicator settings, watchlists
- Dual layout independence: two charts can differ in instrument, timeframe, indicators
- Conflation policy at least smoke-tested (burst of ticks → fewer UI events than raw ticks)

### Secondary suite — vendor adapter (seam 2)

- Fake vendor: deterministic bars, quotes, unavailable flags for contract tests
- LSE adapter: thin mapping/integration tests **gated** on credentials/env; not required for default CI green

### TUI suite (thinner)

- Layout defaults and restore wiring against stub/fake engine at IPC
- Empty state copy **Data Currently not Available** when engine reports unavailable
- Watchlist chrome structure (switcher, columns, green/red change presentation with fixture quotes)
- Indicator panel can send configs the engine accepts
- Prefer not to re-implement market math tests in the TUI

### Prior art

- Greenfield repo: **no existing test suite**. Establish engine IPC tests first as the pattern other features copy. Use uv to run the Python test runner (pytest or whatever the engine chooses) consistently.

### Tooling

- Python tests and engine execution via **uv**.
- Rust tests via standard cargo test for pure TUI logic that cannot sensibly sit on IPC.

## Out of Scope

- **Paper trading** entirely: paper accounts, orders, bar-touch/tick fills, brackets, TP/SL lines, trade marks, positions, filled history, balance history, SQLite trade persistence, order side panel, keyboard nudge of working levels
- Live brokerage execution / multi-broker routing
- **Delta volume bubbles** and other order-flow visuals not listed in Phase A indicators
- Layout modes beyond `single` and `dual-vertical` (grids, freeform studio)
- Equal primary vendors (Alpaca/Futu as co-primaries); vendor multi-homing
- ZeroMQ, Redis, Postgres, multi-subscriber bus
- Pre-only / RTH-only / post-only Session VP modes
- Logos on watchlist rows
- TradingView-grade mouse drag for drawing tools / VP anchors if not needed for MVP anchors (anchors must be settable somehow — keyboard/form is fine; full TV charting suite is not required)
- GEX/GARCH as hard dependencies (optional only)
- Mobile/web clients
- Auth multi-user cloud hosting
- Exact production keybinding finalization as a product research track (implementation may choose defaults)

## Further Notes

- Domain glossary and resolved product defaults live in root `CONTEXT.md`; ADRs in `docs/adr/`. This spec must not invent synonym language that contradicts the glossary.
- **uv** is a hard engineering decision for the Python side of Phase A (reproducible env for humans and AFK agents). It is not a user-visible trading feature.
- Phase B remains documented in `CONTEXT.md` for later specs; do not sneak paper tables into Phase A “while we are there.”
- MessagePack vs JSON default: either is fine per ADR; if both supported, tests should use one canonical codec.
- Optional parquet history warm-start is allowed behind the engine if it speeds restarts; it is not a separate user story and must not require Redis/Postgres.
- If implementation discovers LSE cannot supply a listed default watchlist symbol, follow unavailable rules rather than blocking the ship.
- Suggested issue tracker location for this document: `.scratch/phase-a-chart-terminal/spec.md` (this file). Follow-on implementation tickets should land under `.scratch/phase-a-chart-terminal/issues/` via `/to-tickets` when ready.
