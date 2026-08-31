# Trading Telemetry

Personal day-trading terminal (TUI) for watching instruments, charts, and derived indicators. Not a full TradingView clone; not an execution-first brokerage platform in v1.

## Delivery phases (resolved)

- **Phase A — Chart terminal (first ship):** LSE primary, dual/single layout, watchlists, naked→restored indicators, Session/Fixed Range/Anchored VP, MA, Volume, feed status, HTTP+WS engine+TUI. **No paper trading yet.** Status: done (see `.scratch/phase-a-chart-terminal/`).
- **Phase A.1 — Chart UX polish:** vendored cell-based candles + axes, overlay compose + type overlay strength, chart pan + deeper short-TF history (A2), bar countdown, watchlist→chart + rename, indicator panel Available|Current. Spec: `.scratch/phase-a1-chart-ux-polish/spec.md`. Status: done. **No paper trading.**
- **Phase A.1.1 — Aligned live bars:** wall clock (New York time) on Feed status; forming bars follow wall clock from live last price when vendor tick time is stale; visible **feed delay**; no new candle renderer. Spec: `.scratch/phase-a1.1-aligned-live-bars/spec.md`. Status: done. Does **not** fix LSE vault/`tick.ts`. **No paper trading.**
- **Phase B — Paper M1+nudge (next):** multi account settings (as specified), bar-touch fills, order panel, TP/SL lines, keyboard nudge, trade marks (persistent, hideable pairs), positions / filled history / balance history, SQLite. Tick fills and TV-style drag later.

## Architecture (v1 intent)

- **Two processes:** **Market engine** (Python) + **TUI** (Rust / Ratatui).
- Engine owns vendor I/O (primary: LSE), bars, indicator compute (MA, VP, optional GARCH/GEX).
- TUI owns layout, rendering, mouse/keyboard, watchlists, indicator panel; does not call LSE directly in v1.
- **IPC:** **HTTP** (commands + snapshots) + **WebSocket** (live conflated events). Message bodies JSON or MessagePack. **No ZeroMQ in v1.** **No Redis / Postgres in v1.**
- **Hot state:** in engine memory (quotes, bar rings, indicator snapshots). Workspace + optional parquet on disk.
- **Publish policy:** do not stream every tick to the TUI; conflate quotes/bars/indicators for UI snappiness.
- **Later options:** Redis and/or ZMQ only if multi-subscriber or durable shared cache is proven necessary.

## Language

**Chart**:
A single panel that shows one instrument at one timeframe as candles (and its indicators/overlays).
_Avoid_: Pane (reserved for strips inside a chart), window, graph

**Price pane**:
The main region of a chart that draws OHLC candles, price (Y) and time (X) axes, and price-scale overlays (MA, Volume Profile histogram/levels, pins). Distinct from a Volume **sub-pane** under price when Volume is enabled.
_Avoid_: Treating the whole terminal as one chart surface; drawing volume histogram in the price pane by default

**Chart pan**:
Horizontal movement of the visible candle window along history (earlier ↔ later bars) without changing instrument or timeframe. Live mode typically pins the window to the latest bar until the user pans away. Pan itself only moves over **already loaded** bars (no blocking fetch per key). At the oldest loaded bar, pan clamps (optional soft hint). Loading still older history at the edge is a **later** capability (async), not required for first pan ship behavior.
_Avoid_: Changing timeframe as a substitute for pan; confusing pan with watchlist selection; blocking the UI on each pan key to fetch history

**Input focus**:
Which UI region currently receives navigation keys. Workspace regions include the **focused chart**, **watchlist sidebar**, and **indicator panel** (when open). Opening a modal panel (indicator panel, text prompt, pin placement) routes keys to that panel until it closes; the user cannot drive the watchlist while the indicator panel is active.
_Avoid_: Global arrow keys that ignore the open panel; requiring the mouse for region focus

**Layout mode**:
How many charts are visible and how they are arranged. v1 supports only `single` (one full-area chart) and `dual-vertical` (two charts stacked top/bottom, equal height).
_Avoid_: Grid, multi-layout studio, freeform layout

**Instrument**:
A tradeable symbol the terminal can stream and chart (equities/ETFs such as SPY, QQQ, TSLA; futures such as NQ, ES). Charts are not pinned to one symbol; the user selects the instrument per chart.
_Avoid_: Asset (prefer Instrument in domain language); ticker is acceptable in UI copy

**Primary data vendor**:
The market-data source used first for historical candles and live updates for charts. v1 primary is **London Strategic Edge (LSE)**.
_Avoid_: Treating Alpaca/Futu as equal primaries in v1

**Unavailable data**:
When the primary vendor cannot resolve or serve an instrument (or a timeframe), the chart shows an explicit empty state rather than fake or stale series. v1 copy: **Data Currently not Available**.
_Avoid_: Silent blank chart, inventing bars, pretending the stream is live

**Timeframe**:
The bar size for a chart. v1 supports only: `1m`, `3m`, `5m`, `15m`, `30m`, `1h`, `4h`, `1D`, `1W`.
_Avoid_: Interval, resolution, period (unless matching a vendor field name at the API boundary)

**Bar countdown**:
Per-chart display of **time remaining in the forming (incomplete) bar** for that chart’s timeframe — i.e. countdown to the next candle open of the selected timeframe. Shown in that chart’s chrome (title/subtitle), not only as a global status. Dual layout: each chart has its own countdown.
_Avoid_: Session-remaining clock; time since last tick; a single global countdown for all charts; using countdown as the only stale-tape signal

**Feed status**:
Engine + vendor connectivity chrome (connected / disconnected, vendor mode, heartbeat). Not a price, not last bar time, not proof that every instrument tip is current.
_Avoid_: Treating connected as “all series are live”

**New York time**:
The terminal’s civil timezone: IANA **America/New_York**. That zone is still seasonal (**EDT** in summer, **EST** in winter) until U.S. law and tzdata say otherwise. Product copy may say **ET** / **New York**.
_Avoid_: Frozen year-round EST; machine local time; claiming EST/EDT have been abolished

**Wall clock**:
Current **New York time** shown on Feed status next to the heartbeat. Not last bar time and not vendor tape time.
_Avoid_: Printing the raw unix heartbeat as the clock; calling wall clock “last price time”

**Last bar time**:
The **open** of the newest bar on that chart (the rightmost candle). Per chart; dual layout has two. Not last print, not wall clock.
_Avoid_: Last print; last price time; a single last-bar time on Feed status

**Feed delay**:
How far the latest **vendor tick timestamp** sits behind **wall clock**. Shown so a stale or wrong tape clock is obvious while candles stay aligned. Distinct from bar countdown and from last bar time.
_Avoid_: Using the X-axis split (one symbol behind another) as the only delay signal; calling this last bar time

**Aligned live bars**:
Forming bars advance with **wall clock** from live last prices so instruments that share an index (e.g. QQQ and NQ) print together. A vendor tick timestamp that is behind wall clock does not place those bars; that lag belongs on **feed delay**.
_Avoid_: Dropping a live last price because vendor time is stale; painting delayed vendor time on the axis so related symbols look unsynced

**Workspace**:
The user's chart shell after the welcome screen: layout mode plus each chart's instrument and timeframe.
_Avoid_: Session (ambiguous with market session), screen, view

**Watchlist**:
A named, user-curated list of instruments. The user may keep **multiple watchlists** (like sheets in a spreadsheet) and switch which one is visible in the sidebar. v1 row fields: **symbol**, **last price** (live), **change** (last − previous day close), **change percent** (change / previous day close). Up = green, down = red. Logos are out of v1. The user may **rename** a watchlist (display name); identity stays on a stable id.
_Avoid_: Confusing Watchlist with broker Positions/holdings unless we explicitly merge them; single global list only; renaming by rewriting id

**Watchlist sidebar**:
UI region that shows the active watchlist. Chrome at top: **watchlist name** (switcher) and **add symbol** (opens symbol search). Then a header row (`symbol`, `last`, `change`, `change%`) and one data row per instrument.
_Avoid_: Mixing chart controls into the watchlist header

**Watchlist → chart**:
From the workspace watchlist, choosing a selected symbol with **Enter** or **Space** sets that symbol as the **focused chart’s instrument** (keeps that chart’s timeframe and indicators). Does not affect Welcome (Welcome Enter only applies on the Welcome screen). Dual layout: only the focused chart changes.
_Avoid_: Resetting timeframe on every symbol pick; loading both dual charts from one watchlist key; treating this as navigation back to Welcome

**Watchlist vs Position**:
Watchlist is a quote list for bias/context. **Position** is an open holding in a **Paper account** (or later live broker). They are different panels.

**Paper account**:
A simulated brokerage account the user creates locally (name, initial balance, USD only in v1 intent, leverage/commission rules, optional asset-class restriction). Multiple paper accounts may exist; only one is **active** in the paper panel at a time (switching TBD).
_Avoid_: Confusing with real broker login

**Paper trading** (feature — scope phase still open):
Simulated orders/fills/positions/margin against market data (primary LSE), with commissions and optional leverage / margin call / liquidation. Persistence candidate: **SQLite**. UI: togglable paper panel (shortcut TBD).
_Avoid_: Treating paper as "just a button on the chart" without a risk engine

**Bar-touch fill (v1 paper)**:
A resting paper order fills when a new bar (or the updating last bar) **trades through** its trigger: market on evaluation; limit if bar range crosses limit; stop if bar range crosses stop. No partial fills in v1. Times displayed in **America/New_York (EST/EDT)**.
_Avoid_: Assuming tick-perfect futures microstructure in v1

**Filled order history**:
Append-only (or filterable) log of **orders that have already filled**. A round-turn trade often appears as **two rows** (entry leg + exit leg: TP limit, stop, or manual close). Columns include symbol, side, type, qty, limit/stop/fill prices, commission, place time, fill/close time, duration, margin where relevant.
_Avoid_: Using "order history" for working/unfilled orders

**Working order**:
An order that is **accepted but not yet filled** (resting limit/stop, or market not yet evaluated). Engine tracks it until fill or cancel. **v1 UI intent:** prefer **on-chart markers/lines** (price level + side/type) over a dense working-orders table; optional order side panel for edit/confirm. Filled history remains a table.
_Avoid_: Dumping unfilled orders only into history as if they were trades; requiring a full TradingView DOM for "working list"

**Bracket (entry + TP/SL)**:
Linked group: position (or entry working order) plus take-profit and stop-loss child orders. Position row shows TP/SL; engine keeps child working orders; fills write separate history legs (model 3).
_Avoid_: Orphan TP/SL with no parent position/entry

**Bar-touch vs tick fill**:
First paper milestone uses **bar-touch**. **Tick-based** fill evaluation is a later milestone, not required for first paper.

**Paper UI milestone M1+nudge** (first paper ship intent):
Order **side panel** (shortcut TBD) for place/modify; market/limit/stop; brackets with TP/SL (model 3); positions + filled history + balance history; SQLite; chart shows **lines** for active TP/SL (and other working levels). User can **select** a level and **keyboard-nudge** price (e.g. tick up/down) then confirm. **No** permanent working-orders table. **No** TV-grade mouse drag/projection in this milestone (later).
_Avoid_: Promising TradingView drag-on-chart parity in the first paper ship

**Trade mark**:
Chart annotation for a **completed (or still open) paper trade leg**: a **dot** (or small marker) at the fill price/time for entry, and another for exit (TP, SL, or manual close). Marks **persist after the position is closed** so the user can review trades on the chart. User can **hide/show a trade mark pair** from order history (or related UI) without deleting the underlying fill records.
_Avoid_: Removing marks automatically on close; confusing trade marks with working TP/SL lines (lines = live orders; marks = historical fills on chart)

**Indicator**:
A computed series or overlay attached to a **specific chart** (not global). Charts start with **no indicators**; the user adds/toggles them via an indicator panel. Each chart in dual layout has its own indicator set.
_Avoid_: Study (TradingView jargon unless in UI parity notes)

**Indicator panel**:
UI for managing indicators on the **focused chart**. Split into two side-by-side lists: **Available Indicators** (left) and **Current Indicators** (right). The user switches which list is active (e.g. Tab).
_Avoid_: Mixing add-catalog and instance list into one undifferentiated list; putting deep type styling only on instance rows

**Available Indicators**:
Catalog of indicator **types** the client can add (MA, Volume, Session VP, Fixed Range VP, Anchored VP, optional GEX/GARCH). From here the user **adds** a type to the focused chart and edits **type style** for that type on this chart (e.g. overlay strength/opacity). Does not list per-instance rows.
_Avoid_: Treating Available as the live instance list; editing one catalog row as if it were a single MA length instance

**Current Indicators**:
List of indicator **instances** already on the focused chart. From here the user **enables/disables**, **removes**, and runs instance actions (e.g. re-pin Fixed Range / Anchored). Not the home for type-level style such as shared opacity.
_Avoid_: Duplicating full type-style editors here; calling this the catalog

**Indicator type style**:
Per **chart** and per **indicator type** presentation settings shared by all instances of that type on that chart (e.g. MA overlay strength; Session VP histogram strength). Distinct from instance parameters (MA length, VP anchors, extend-to-right).
_Avoid_: Global app-wide style only; per-line opacity for every MA length unless we explicitly add that later

**Overlay strength** (terminal “opacity”):
How strongly an overlay is drawn on the price chart. Terminals have no true alpha; strength means intensity and/or blend weight when the overlay paints over candle cells. User-tunable per indicator type style.
_Avoid_: Promising Photoshop-style transparency; silent full overwrite with no user control

**Moving Average (MA)**:
Average of close prices over N bars on the price chart. Each MA line is either **SMA** or **EMA** (user choice per line). Up to **3** lines per chart. Default lengths when added: **10, 60, 200**. Used mainly for stocks/swing. Continuous MA strokes may paint over candle cells (line continuity preferred over preserving candle color on those cells); overlay strength softens that recoloring.
_Avoid_: Calling every smoothed line "MA" without type/length

**Volume**:
Per-bar traded volume, typically as a histogram in a sub-pane under price.
_Avoid_: Confusing with Volume Profile

**Volume Profile (VP)**:
Distribution of volume by **price level** over a chosen time range, drawn as a horizontal histogram **overlay on the price chart** (opacity tunable so candles stay readable). Strategy levels from a profile: **POC**, **VAH**, **VAL**. Vertical resolution is **number of rows**: the profile’s price high–low is divided into N equal buckets (not ticks-per-row).
_Avoid_: Volume histogram (that's per-bar Volume); calling number-of-rows "tick size"

**Point of Control (POC)**:
Price level with the highest volume inside a Volume Profile.
_Avoid_: "HVN" as a synonym in product language unless we define both

**Value Area High (VAH)** / **Value Area Low (VAL)**:
Upper and lower bounds of the **value area** (price range containing a configured share of total profile volume, classically ~70%). Used with POC for confluence entries.
_Avoid_: Treating VAH/VAL as fixed % from high/low of the range

**Session Volume Profile**:
VP built automatically for **one profile per day** in mode **All** only (v1). Session clocks (America/New_York): **US equities/ETFs** = **16:00 → next calendar day 16:00**; **CME equity-index futures (ES/NQ)** = **prior calendar day 18:00 → 17:00** (break ~17:00–18:00).
_Avoid_: "Visible range profile" (different tool); assuming cash RTH 09:30–16:00 is the Session VP day

**Fixed Range Volume Profile**:
VP between **two user-chosen time anchors** on the chart (start and end). With **extend to right** enabled: (1) volume from **new bars beyond the end anchor** continues to feed the same profile, and (2) **POC / VAH / VAL** level lines **project to the right** for ongoing confluence. With it off, only the closed [start, end] window is used and levels do not project past that window.
_Avoid_: "Fix ranged" in docs — use **Fixed Range**; treating extend as only lines or only live build — it is **both**

**Anchored Volume Profile**:
VP from a **single user (or preset) anchor time** forward to "now" (or session end). Typical use: anchor at **US cash open 09:30 America/New_York** for intraday profile.
_Avoid_: Using "anchored" for two-point ranges (that's Fixed Range)

**GEX** (optional when data available):
Options-derived gamma exposure style indicator. Only shown if options data is available and computation succeeds; otherwise unavailable — not a hard dependency for charts.
_Avoid_: Shipping fake GEX without options inputs

**GARCH** (optional when data available):
Volatility model indicator on returns. Only when inputs/history allow a stable estimate; otherwise unavailable.
_Avoid_: Treating GARCH as a simple moving overlay like MA

**Delta volume bubbles**:
Order-flow visualization of buy/sell aggression. **Out of v1** (candidate for later versions).

## v1 product defaults (resolved)

- **First launch:** Welcome panel, then workspace in layout mode `single`, instrument **SPY**, timeframe **`1D`**.
- **Later launches:** After Welcome, restore last **layout mode**, each chart's **instrument**, and each chart's **timeframe**.
- **Layout toggle:** User may switch between `single` and `dual-vertical` at any time.
- **Defaults only when nothing is saved yet (or a chart has no selection):**
  - `single` → **SPY** @ **`1D`**
  - `dual-vertical` → top **QQQ** @ **`1D`**, bottom **SPY** @ **`1D`** (top/bottom assignment assumed unless overridden)
- **Watchlist sidebar:** docked **right**; **togglable** (show/hide). Exact keybinding TBD later.
- **First-launch default watchlist** (name TBD, e.g. `Core`): **ES, NQ, SPY, QQQ, SOXL, VIX** — include VIX only if the primary vendor resolves it; otherwise omit or show unavailable for that row without failing the whole list.
- **Indicators:** first-ever chart open is **naked** (no indicators). After that, **restore last-used indicators and their settings per chart**. Indicator panel is **per focused chart** in dual layout.
- **Indicator panel model (polish ship):** **Available** (left) = catalog types → add + **type style** (incl. overlay strength). **Current** (right) = instances → on/off, remove, re-pin; **instance parameters** (MA length/type, VP rows/box/extend/levels, etc.) via **hotkeys on the selected Current row** — no instance settings popup this ship. Active list switches with Tab; default active list is **Current**.
- **Overlay paint priority:** when an overlay (e.g. MA) shares a cell with a candle, **indicator may win** so lines stay continuous; type **overlay strength** reduces how hard that recoloring reads.
- **Chart presentation (polish ship):** cell-based candlesticks with **price Y-axis** and **time X-axis** (visual quality aligned with a vendored Ratatui candlestick widget approach); **chart pan** left/right through history; overlays composed on the same price/time scale; Volume remains a **sub-pane** under the price pane when enabled. Dual layout: independent pan/window per chart.
- **History depth for pan (polish ship = A2):** on chart interest, load a **deeper** initial series for short timeframes so panning can reach **prior session/day(s)** without edge-fetch. Documented caps (newest N bars): **1m → 3900** (≈10 US equity RTH sessions), 3m 2000, 5m 1500, 15m 1000, 30m 750, 1h/4h/1D 500, 1W 260. Pan clamps at the oldest loaded bar (optional soft hint). **Edge fetch of older bars (B)** is **near-future**, async, not blocking pan keys.
- **Arrow keys (polish ship, Normal workspace):** **↑ ↓** → watchlist row selection when the sidebar is usable (not when indicator panel / prompt / pin mode owns input). **← →** → **chart pan** on the focused chart. **Tab** still cycles focused chart in dual layout. Indicator panel open: ↑ ↓ move within the active Available/Current list; Tab switches those lists; watchlist is inactive until the panel closes. Pin placement remains a modal override for ← → (move pin, not pan).
- **Bar countdown (polish ship):** each chart shows time remaining in its forming bar in chart chrome; independent in dual layout.
- **Watchlist → chart (polish ship):** **Enter** / **Space** on a selected watchlist row → focused chart instrument = that symbol; timeframe and indicators unchanged. Inactive while indicator panel / text prompt / pin placement owns input.
- **Watchlist rename (polish ship):** in Normal workspace, **`r`** opens a name prompt for the **active** watchlist sheet; Enter saves (persisted). Empty name rejected; duplicate display names allowed (ids stay unique). When the indicator panel is open, **`r`** remains instance re-pin (not rename).
- **v1 indicator set (intent):** MA; Volume; Volume Profile variants (**Session**, **Fixed Range**, **Anchored**); **GEX** and **GARCH** only if data/compute available. **Delta bubbles:** not v1.
- **VP strategy use:** confluence on **VAH / POC / VAL**; Fixed Range often weekly (current + previous week); Anchored often from **09:30 America/New_York** for US intraday.
- **Max indicator instances per chart (v1):** Session VP **1** · Fixed Range VP **4** · Anchored VP **2** · MA lines **3** · Volume **1** · GEX **1** · GARCH **1**. Dual layout: each chart has its own counts.
- **MA:** per-line type **SMA or EMA**; default lengths **10 / 60 / 200** when the user adds the default stack (or first three lines).

### Session Volume Profile — v1 parameters

- **Sessions:** only **All**. One profile per day. (Pre-only / RTH-only / post-only are out of v1.)
- **Day bounds (America/New_York):**
  - **US equities / ETFs:** **16:00 → next day 16:00** (close-to-close daily window).
  - **CME ES/NQ:** **prior day 18:00 → 17:00** (futures session including overnight; break ~17:00–18:00).
- **Box width:** % of the horizontal span of that session used to draw the volume histogram bars (e.g. 30% → profile uses 30% of session width).
- **Placement:** **left** or **right** of the session span.
- **VAH / POC / VAL:** each has color + opacity; each can be toggled on/off.
- **Volume histogram:** color + opacity.
- **Number of rows:** default **500**. Vertical resolution of the profile: the **price range of that profile** is split into this many equal buckets (e.g. 500 → 500 horizontal volume bars). Higher = finer POC/VAH/VAL, more draw cost. **Not** ticks-per-row.
- **Value area volume:** default **70%**.

### Fixed Range Volume Profile — v1 parameters

- **Extend to right:** yes/no. When **on**, two effects together:
  1. **Live build:** after the user-set **end** anchor, the profile **keeps accumulating** volume from **new bars that print to the right of that end** (histogram and POC/VAH/VAL values keep updating).
  2. **Level projection:** **POC, VAH, and VAL** (whichever are toggled on) are drawn as **horizontal levels extending to the right** beyond the original range so confluence remains visible on newer bars.
  When **off**, only bars between the two anchors count; profile and levels stay within that closed window.
- **VAH / POC / VAL:** color + opacity; each toggleable.
- **Volume histogram:** color + opacity.
- **Box width:** % of the horizontal span of the (possibly growing) range.
- **Number of rows:** default **200** (same meaning: equal price buckets across the range’s high–low).
- **Value area volume:** default **70%**.
- Range defined by **two time anchors** on the chart (start + end).

### Anchored Volume Profile — v1 parameters

- **Volume** color; **VAH / POC / VAL** colors; each of VAH/POC/VAL toggleable.
- **Number of rows:** default **500**.
- **Value area volume:** default **70%**.
- Range defined by **one anchor** forward (typical: 09:30 America/New_York).
