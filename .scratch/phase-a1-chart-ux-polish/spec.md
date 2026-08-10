# Phase A.1 — Chart UX polish

Status: ready

## Problem Statement

Phase A delivered a working chart terminal (engine + TUI, LSE, dual layout, watchlists, MA/Volume/VP/GEX/GARCH). Day-trading use is still painful:

- Candles are hard to read (Braille canvas, no real price/time axes).
- Indicators (especially MA) recolor candle cells so up/down is ambiguous; overlays feel stacked and noisy.
- No chart pan through history; short timeframes only load ~500 bars (~8h of 1m), so previous-day scrubbing fails.
- No per-chart countdown to the next bar of the selected timeframe.
- Watchlist cannot quickly drive the focused chart; watchlists cannot be renamed.
- Indicator panel is a single hotkey-heavy list; type-level style (e.g. overlay strength) has no home.

Paper trading remains **out of scope** (Phase B).

## Solution

Ship **Phase A.1 — Chart UX polish**: improve the TUI chart surface and navigation without changing the two-process architecture.

1. **Price pane** uses a **vendored Ratatui candlestick widget** (based on [codingskynet/tui-candlestick-chart](https://github.com/codingskynet/tui-candlestick-chart), itself based on cli-candlestick-chart): Unicode candles, **price Y-axis**, **time X-axis**, fix layout origin for dual/status chrome.
2. **Compose overlays** (MA, VP hist/levels, pins) on the **same** price/time scale; Volume stays a **sub-pane**. Continuous MA may paint over candle cells; **indicator type style / overlay strength** softens recoloring.
3. **Chart pan** with ← → on the focused chart; pan is **local** over loaded bars (**A2** deeper initial history for short TFs). Async edge-fetch of older bars (**B**) is **near-future**, not this ship.
4. **Bar countdown** in each chart’s chrome (time left in the forming bar).
5. **Watchlist → chart** via Enter/Space; **rename** active watchlist with `r` (Normal mode).
6. **Indicator panel** split **Available | Current** (Model 2): Available = add + type style; Current = instances + hotkeys.

Product language: `CONTEXT.md`. Architecture ADRs 0001–0002 unchanged; expect an ADR for vendoring the candlestick widget when implementation lands.

## User Stories

1. As a day trader, I want candlesticks with clear bodies/wicks and **price + time axes**, so that I can read levels and session structure at a glance.
2. As a day trader, I want MA and VP composed on that chart without destroying the whole desk layout (dual + sidebar), so that polish does not drop Phase A features.
3. As a day trader, I want continuous MA lines (overlay may win cells) with **tunable overlay strength per indicator type**, so that I control how hard overlays recolor candles.
4. As a day trader, I want **← →** to pan the focused chart through history, so that I can review earlier bars without changing timeframe.
5. As a day trader on **1m** at morning open, I want enough **loaded** history that pan can reach **prior session/day(s)** without waiting on each keypress, so that scrubbing stays snappy.
6. As a day trader, when I hit the oldest loaded bar, I want pan to clamp (with optional soft hint), so that I know I reached the buffer limit—not a frozen UI.
7. As a day trader, I want a **bar countdown** on each chart for time remaining in the forming bar of that chart’s timeframe, so that I know when the next candle opens.
8. As a day trader, I want **Enter/Space** on a watchlist row to set the **focused chart’s instrument** (keep timeframe + indicators), so that symbol switching is one key.
9. As a day trader, I want **`r`** in Normal mode to rename the **active** watchlist (persisted; empty rejected; duplicate display names allowed), so that sheets have meaningful names.
10. As a day trader, I want the indicator panel split into **Available** (left) and **Current** (right) with Tab to switch active list (default Current), so that add vs manage is clear.
11. As a day trader, on Available I want Enter/Space to **add** a type and **`c`** to edit **type style** (e.g. overlay strength) for that type on this chart, so that styling is centralized.
12. As a day trader, on Current I want on/off, remove, re-pin, and **instance hotkeys** (length, extend, levels, …) without a second settings popup this ship, so that Phase A instance controls remain fast.
13. As a day trader, when the indicator panel (or prompt/pin mode) is open, I do not want watchlist/chart keys to steal focus, so that modal ownership stays predictable.
14. As an implementer, I want deeper history caps configurable at the engine/vendor boundary and tested via IPC, so that A2 is verified without live LSE in CI.
15. As an implementer, I do **not** need async older-bar edge fetch (**B**) in this ship—only a clear seam/note for later.

## Implementation Decisions

### Scope and phasing

- **In:** chart presentation B1, overlay compose + type overlay strength, pan A2, bar countdown, watchlist Enter/Space + rename, indicator panel Model 2, focus-routed keys as in `CONTEXT.md`.
- **Out:** paper trading; async history edge-fetch (**B**); instance settings popup; freeform layouts; new indicator types; mouse drag; changing ADR-0001/0002 IPC shape beyond additive endpoints (e.g. watchlist rename, optional higher history limit).

### Chart presentation (B1)

- Vendor (copy into `tui/`) the candlestick widget from **tui-candlestick-chart** (MIT; based on cli-candlestick-chart). Prefer vendoring over an unreleased git dep so we can fix bugs and expose scale helpers.
- **Must fix:** render uses absolute `(0, y)` in upstream — offset all draws by `area.x` / `area.y` for dual layout + status/watchlist chrome.
- Map product timeframes ↔ widget `Interval` (`1m`…`1W`).
- Display timezone for axis labels: prefer **America/New_York** for product consistency with session clocks (confirm if local TZ needed later).
- **Paint order:** candle widget (axes + candles) → VP soft hist → levels → MA → pins.
- **Volume:** keep separate sub-pane under price pane when Volume indicator enabled.
- Expose **window + price scale helpers** from vendored code so overlays share coordinates with candles.
- Overlay strength: terminal intensity / blend weight (no true alpha). Per **chart** + **indicator type** (type style).

### Overlay paint policy

- When MA (or similar) shares a cell with a candle body/wick, **indicator may win** so the line stays continuous.
- Type **overlay strength** reduces how aggressive that recoloring reads (blend and/or dimmer RGB).
- VP histogram remains soft; POC/VAH/VAL as levels; pins last.

### Chart pan + history (A2 now, B later)

- **← →** pan focused chart in Normal mode; independent pan state per chart in dual.
- Pan only over **already loaded** bars; clamp at ends; optional soft hint at oldest edge.
- **A2:** increase initial history depth for short timeframes (especially **1m**) so multi-day/session scrub is possible—replace default ~500 with a higher cap (exact number at implement; target multiple RTH days of 1m). Longer TFs may keep smaller caps.
- **B (later ship):** async fetch older bars near left edge; never block pan keys on HTTP.
- Live tip: right edge follows latest bar until user pans away; returning to tip re-attaches to live (widget cursor reset pattern is fine).

### Bar countdown

- Per chart chrome (title/subtitle): time remaining in the **forming incomplete bar** for that chart’s timeframe.
- Dual: two independent countdowns.
- Hide or show `—` when no live forming bar / unavailable series (implementation pick; must not invent times).

### Input focus (arrows and modes)

- Normal: **↑ ↓** watchlist row (when sidebar usable); **← →** chart pan on focused chart; **Tab** dual chart focus.
- Indicator panel open: owns keys; **Tab** Available ↔ Current; watchlist inactive.
- Pin placement / text prompts: modal; pin mode **← →** move pin not pan.
- Welcome **Enter** unchanged (Welcome screen only).

### Watchlist

- **Enter / Space** on selected row → focused chart instrument = symbol; **keep timeframe + indicators**; engine chart interest as today.
- Unavailable → existing empty state copy.
- **`r`** Normal → rename **active** watchlist display name; persist via new engine endpoint; empty rejected; duplicate names allowed (stable id).
- Indicator panel open: **`r`** remains FRVP/AVP re-pin on Current.

### Indicator panel (Model 2)

| Pane | Role |
|------|------|
| **Available** (left) | Catalog types; Enter/Space **add**; **`c`** type style popup (overlay strength, and other type-level presentation as needed) |
| **Current** (right) | Instances; Space on/off; `x` remove; `r` re-pin; existing instance **hotkeys**; no instance settings popup this ship |
| **Tab** | Switch active list; default active = **Current** |

- Type style is **per focused chart** and **per indicator type** (all MAs on that chart share MA strength, etc.).
- Persist type style with workspace / indicator config as appropriate (implementation: extend configs or chart-level style map).

### Engine / IPC (additive)

- Watchlist **rename** command + persistence in workspace file.
- History: higher `limit` (or per-timeframe limits) on LSE/fake `fetch_history` for A2; fake vendor must support long series for tests.
- No protocol break; snapshot remains source of truth on reconnect.

### Testing

- Primary seam remains engine HTTP+WS contract tests where behavior is server-side (rename, history depth).
- TUI: unit/widget tests where practical for pan clamp, countdown formatting, key routing; manual/visual check for candle+overlay compose.
- CI continues on fake vendor.

## Out of Scope

- Phase B paper trading, orders, SQLite trade book
- Async history edge-fetch (**B**)
- Instance settings popup; configuring type style from Current
- True alpha compositing; replacing Ratatui
- New layouts beyond single / dual-vertical
- New indicators beyond Phase A set
- Welcome flow redesign

## Further Notes

- Grill decisions captured in `CONTEXT.md` (glossary + polish-ship product defaults).
- Upstream reference: https://github.com/codingskynet/tui-candlestick-chart (Ratatui 0.30.2 aligned with this repo).
- Write **ADR** when vendoring: why vendor vs git dep vs Canvas-only.
- Soft TBD at implement: exact bar caps per timeframe; soft hint copy at left wall; whether countdown uses exchange session calendar for daily/weekly boundaries.
