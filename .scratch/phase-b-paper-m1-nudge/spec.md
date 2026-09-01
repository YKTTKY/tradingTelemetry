# Phase B — Paper M1+nudge

Status: ready

Target path in repo (later, on branch `grok_bot_dev`): `.scratch/phase-b-paper-m1-nudge/spec.md`

## Problem Statement

Phase A through A.1.1 shipped a usable **chart terminal**: LSE primary, dual/single **layout mode**, **watchlists**, naked→restored **indicators**, Session/Fixed Range/Anchored VP, MA, Volume, **feed status** with **New York time** **wall clock** and **feed delay**, and **aligned live bars**. The desk can watch instruments. It cannot **paper trade**.

A day trader using this terminal still has no local **paper account**, no simulated orders against the tape, no **bar-touch fill**, no **bracket** (entry + TP/SL), no **positions**, no **filled order history**, no **balance history**, and no on-chart **working order** lines or **trade marks**. There is no SQLite paper book. The TUI has no togglable paper panel and no **order side panel**. Watchlist remains a quote list; there is no **Position** panel.

`CONTEXT.md` **Delivery phases** name **Phase B — Paper M1+nudge** as next. The **Language** section still says paper trading “scope phase still open”; Delivery phases win (see Further Notes). Tick-based fills and TradingView-grade mouse drag are later, not this ship.

## Solution

Ship **Phase B — Paper M1+nudge**: a local **paper trading** desk on the existing two-process architecture (Python market engine + Rust Ratatui TUI, HTTP + WebSocket IPC).

1. **Paper accounts:** the user creates one or more local **paper accounts** (name, initial balance, USD only in v1, leverage/commission rules, optional asset-class restriction). Multiple may exist; only one is **active** in the paper panel at a time. **Switching UX is TBD** — do not invent a switcher this ship. Engine still models many accounts and one active id so tests and settings can create them.
2. **Paper trading engine:** simulated orders, **bar-touch fills**, **positions**, commissions, optional leverage / margin call / liquidation against market data (primary LSE). Hot state in engine memory. Durability: **SQLite** (not workspace JSON, not Redis/Postgres).
3. **Bar-touch fill (v1):** a **working order** fills when a new bar or the **updating last bar** trades through its trigger: **market** on evaluation; **limit** if bar range crosses limit; **stop** if bar range crosses stop. **No partial fills** in v1. Times displayed in **America/New_York** (EST/EDT). Evaluation series is **1m**, engine-owned, independent of the focused chart’s timeframe.
4. **Brackets (model 3):** linked group — entry (working or filled position) plus take-profit and stop-loss **child working orders**. Position row shows TP/SL; engine keeps children; fills write **separate filled order history legs**.
5. **Paper UI (M1+nudge):** togglable paper panel (**keyboard shortcut TBD**) with an **order side panel** for place/modify (market / limit / stop, brackets with TP/SL); **positions** table; **filled order history** table; **balance history**. **No** permanent working-orders table. Chart **price pane** shows **lines** for active TP/SL and other working levels. User **selects** a level, **keyboard-nudges** price, then **confirms**. **No** TradingView-grade mouse drag this milestone.
6. **Trade marks:** a **dot** at fill price/time for **entry** and for **exit**. Marks **persist after the position is closed**. User can **hide/show a trade mark pair** from filled order history without deleting fills. Distinct from working TP/SL **lines**.
7. **Watchlist vs Position:** different panels. Watchlist stays a quote list. Position is an open holding in a **paper account**.
8. **Unchanged:** **aligned live bars**, **feed delay**, **wall clock**, vendored candlestick widget, chart pan, indicators. TUI never calls vendors. No new candle renderer. Paper overlays are lines/marks on the existing **price pane**.

Product language: `CONTEXT.md` (**paper account**, **paper trading**, **bar-touch fill**, **working order**, **filled order history**, **bracket**, **trade mark**, **Watchlist vs Position**, **New York time**). Architecture: ADR-0001, ADR-0002, ADR-0003.

## User Stories

1. As a day trader, I want a local **paper account** I create myself (name, initial balance, USD only), so that I can simulate a brokerage without a live broker login.
2. As a day trader, I want leverage and commission rules on that **paper account**, so that fills cost money and optional margin behaves like a risk desk, not a toy button.
3. As a day trader, I want an optional asset-class restriction on a **paper account**, so that an equities-only book cannot accept a futures **instrument** (and the reverse) when I set that rule.
4. As a day trader, I want multiple **paper accounts** to exist in settings, so that I can keep separate books (e.g. swing vs scalps) without wiping SQLite.
5. As a day trader, I want only one **paper account** **active** in the paper panel at a time, so that orders, **positions**, and history belong to a single book on screen.
6. As a day trader, I do **not** need a polished **paper account** switching UX in this ship (**switching TBD**), so that Phase B is not blocked on a switcher design.
7. As a day trader, I want **paper trading** to persist across engine restarts in **SQLite**, so that working orders, **positions**, **filled order history**, **balance history**, and **trade mark** visibility survive a reboot.
8. As a day trader, I want workspace JSON to stay the chart shell (layout, charts, **watchlists**, indicators), so that paper books are not jammed into the workspace file.
9. As a day trader, I want a togglable paper panel, so that I can hide the trading chrome and keep a focused **chart**.
10. As a day trader, I want the paper panel keyboard shortcut left **TBD**, so that implementers pick a key later without this spec inventing one.
11. As a day trader, when the paper panel (or order side panel) is open, I want it to own **input focus** the way the indicator panel does, so that watchlist arrows and chart pan do not steal keys.
12. As a day trader, I want an **order side panel** to place a **working order** (market, limit, or stop) on the **focused chart**’s **instrument**, so that I can trade from the desk without a dense working-orders table.
13. As a day trader, I want to modify a **working order** (qty, limit, stop) from the order side panel, so that I can correct a rest without cancel-and-replace as the only path.
14. As a day trader, I want to cancel a **working order**, so that a resting limit or stop does not fill after I change my mind.
15. As a day trader, I want a **market** order to fill on **bar-touch** evaluation (full qty), so that “market” means now-on-this-bar, not a live broker route.
16. As a day trader, I want a **limit** **working order** to fill only when a new bar or the **updating last bar** range crosses the limit, so that I am not filled on hope.
17. As a day trader, I want a **stop** **working order** to fill only when a new bar or the **updating last bar** range crosses the stop, so that stop-through is bar-range, not tick tape.
18. As a day trader, I want **no partial fills** in v1, so that qty is all-or-nothing on that evaluation.
19. As a day trader, I want **bar-touch fill** evaluation to use an engine-owned **1m** series for that **instrument**, so that changing the **chart** timeframe does not change fill behavior.
20. As a day trader, I want **bar-touch fill** to run even if I am not currently charting that **instrument**, so that a resting order on SPY can fill while I watch NQ.
21. As a day trader, I want **bar-touch fill** to re-evaluate when the **updating last bar** extends (high/low/close move), so that an intra-bar touch fills without waiting for the candle to close.
22. As a day trader, I want **bar-touch fill** to re-evaluate when a new bar rolls, so that a gap through the trigger still fills on the new bar (no invented gap candles).
23. As a day trader, I want fill, place, and close times shown in **New York time** (`America/New_York`, EST/EDT as tzdata reports), so that the paper book matches session clocks and **wall clock**.
24. As a day trader, I want a **bracket** (entry + TP/SL, **model 3**): a linked group whose **position** row shows TP/SL and whose engine keeps child **working orders**, so that take-profit and stop-loss are not orphan lines.
25. As a day trader, when a **bracket** child fills, I want a **separate** **filled order history** leg (entry vs exit), so that a round-turn is two rows, not one collapsed trade.
26. As a day trader, when the take-profit child of a **bracket** fills, I want the stop-loss child cancelled (and the reverse), so that I am not left with a naked protective order.
27. As a day trader, I want to attach or modify TP/SL on an open **position** from the order side panel, so that I can rest a **bracket** after a naked entry.
28. As a day trader, I want a **Position** panel distinct from the **watchlist**, so that open holdings are not mixed into quote rows.
29. As a day trader, I want each **position** row to show symbol, side, qty, average price, unrealized P&L, and TP/SL when a **bracket** is attached, so that risk is glanceable.
30. As a day trader, I want **filled order history** as an append-only (filterable) table of orders that have **already filled**, so that I never confuse it with **working orders**.
31. As a day trader, I want a round-turn to appear as **two rows** (entry leg + exit leg: TP limit, stop, or manual close), so that I can audit each fill.
32. As a day trader, I want **filled order history** columns: symbol, side, type, qty, limit/stop/fill prices, commission, place time, fill/close time, duration, margin where relevant, so that the table matches the glossary.
33. As a day trader, I want **balance history** for the active **paper account** (cash/equity as fills hit), so that I can see the book move, not only the last number.
34. As a day trader, I want commissions deducted on fill per the **paper account** rule, so that P&L is not commission-free.
35. As a day trader, when leverage is enabled, I want margin reserved on entry and a **margin call / liquidation** if maintenance fails, so that a leveraged book can die like a real one.
36. As a day trader, I want **working orders** shown as **on-chart lines** on the **price pane** (price level + side/type), so that I do not need a permanent working-orders table.
37. As a day trader, I want active TP/SL (and other working levels) drawn as horizontal **lines**, distinct from **trade marks**, so that live orders and historical fills do not look the same.
38. As a day trader, I want to **select** a working level on the focused **chart** and **keyboard-nudge** its price (tick up/down) then **confirm**, so that I can adjust a rest without mouse drag.
39. As a day trader, I want an unconfirmed nudge to leave the live **working order** unchanged until I confirm, so that a mis-tap does not move the engine price.
40. As a day trader, I do **not** want TradingView-grade mouse drag or projection of working levels in this ship, so that M1+nudge stays keyboard-first.
41. As a day trader, I want a **trade mark** (dot) at fill price/time for the **entry** leg, so that I can see where I got in on the **chart**.
42. As a day trader, I want a **trade mark** (dot) at fill price/time for the **exit** leg (TP, SL, or manual close), so that I can see where I got out.
43. As a day trader, I want **trade marks** to **persist after the position is closed**, so that I can review trades on history, not only while flat.
44. As a day trader, I want to **hide/show a trade mark pair** from **filled order history** without deleting fills, so that the chart can be cleaned without rewriting the book.
45. As a day trader in **dual-vertical** layout, I want paper lines and **trade marks** on the **chart** whose **instrument** matches the order/fill, so that the unfocused chart is not painted with the other symbol’s book.
46. As a day trader, I want place/modify defaults to the **focused chart**’s **instrument**, so that dual layout does not silently order the wrong symbol.
47. As a day trader, I want **aligned live bars**, **feed delay**, and **wall clock** to stay as Phase A.1.1 shipped them, so that paper trading does not regress the tape clocks.
48. As a day trader, I want **unavailable data** to still show **Data Currently not Available** (no invented bars), so that a paper order cannot invent a series the vendor does not have.
49. As a day trader, I want the TUI to never call LSE (or any vendor), so that credentials and fill evaluation stay in the engine.
50. As an implementer, I want paper snapshot fields and paper WS events to be **additive** on the existing HTTP snapshot and WebSocket, so that we do not break feed/workspace/quotes/indicators.
51. As an implementer, I want **bar-touch fill** tests through **fake vendor** IPC (inject a live print → last-bar update / roll → fill), so that CI never needs live LSE.
52. As an implementer, I want TUI unit tests for the order side panel, keyboard nudge+confirm, **trade mark** hide/show, and New York time formatters, so that UI behavior is locked without pixel-testing the candlestick widget.
53. As an implementer, I want CI to stay on the **fake vendor**, so that Phase B does not add a live-vendor gate.
54. As a day trader, I do not want a dense permanent **working orders** table in v1, so that the desk stays chart + side panel.
55. As a day trader, I do not want tick-based fills, live brokerage, strategy/backtest, or a new candle renderer in this ship, so that Phase B stays M1+nudge.

## Implementation Decisions

### Scope and phasing

- **In:** multi **paper account** settings (as specified); one active account in the paper panel; **bar-touch fill**; order side panel; **bracket** model 3; **positions** + **filled order history** + **balance history**; SQLite; chart **lines** for working TP/SL and other working levels; select + keyboard nudge then confirm; **trade marks** (persist, hideable pairs); additive IPC; fake-vendor tests.
- **Out:** tick-based fills; TradingView mouse drag/projection; repairing LSE vault freshness or vendor tick timestamps; live broker; strategy/backtest; async older-bar edge fetch; new candle renderer; Delta bubbles.
- **Explicit TBD (do not pick):** paper panel keyboard shortcut; **paper account** switching UX.
- Product vocabulary follows `CONTEXT.md`. Do not invent synonyms for **paper account**, **bar-touch fill**, **working order**, **filled order history**, **bracket**, **trade mark**.

### Process architecture (ADR-0001, ADR-0002)

- **Engine owns:** **paper accounts**, SQLite paper book, **bar-touch fill** evaluation against bars, **working orders** + **brackets**, **positions**, **filled order history**, **balance history**, commissions, optional leverage / margin call / liquidation. Additive fields on the existing HTTP snapshot and WS events — no protocol break. TUI never becomes a second risk engine.
- **TUI owns:** togglable paper panel, **order side panel** (place/modify/cancel/confirm), on-chart TP/SL and working **lines**, select + keyboard nudge then confirm, **trade marks**, **positions** / **filled order history** / **balance history** tables. **No vendor I/O.**
- Hot paper state lives in engine memory (same as quotes/bars). SQLite is durability + restart, not a query bus for the TUI.
- **No Redis, Postgres, or ZeroMQ** in v1.

### Persistence split

- **Workspace file (existing JSON):** layout, charts, watchlists, indicator configs/type styles. Unchanged role. Do **not** serialize the paper book into workspace JSON.
- **SQLite (new, paper only):** **paper accounts** (including rules and the active-account id), **working orders**, **brackets**, **positions**, **filled order history**, **balance history**, **trade mark** pair visibility. Survives engine restart. Path lives under the same user data directory family as the workspace file (implementer pick; not a second product concept).
- On boot: load SQLite → hydrate hot paper state → snapshot includes paper so a reconnecting TUI recovers without a second protocol.

### Existing IPC surface (extend, do not replace)

Today the engine already serves:

- Snapshot: feed + workspace + quotes + indicators.
- Commands: workspace layout, chart interest, indicators, chart type-styles, watchlist active/add/remove/rename.
- WebSocket: `feed_status`, `heartbeat`, conflated `bar_update`, `quote_update`, `indicator_update`.

Phase B **adds** paper to snapshot (optional/default-empty so the existing blob still parses) and **adds** paper command routes in the same `/v1/…` family (same additive pattern as type-styles and watchlist rename). Exact paper route names are implementer-chosen; behavior must lock at the HTTP+WS seam.

Paper WebSocket events are a **new event type** (or small family) for account/position/working/fill/**trade mark** changes. They must **not** ride `bar_update`. The existing conflating hub is **latest-wins** per series: coalescing fills that way would drop discrete fills. Paper events are discrete; queue/flush them without latest-wins loss. Bar/quote/indicator conflation stays as today.

### Bar-touch fill evaluation (engine)

- **Trigger:** the same bar-update path that already fires when a live print updates the last bar or rolls a new bar. Do not add a second vendor tick pipe for paper.
- **Evaluation timeframe:** **1m** per **instrument**, owned by the engine, **independent of chart interest**. Changing the focused chart from `1m` to `1D` must not change fill logic. If the user is not charting that symbol, the engine still keeps the 1m series for any **instrument** with **working orders** or open **positions**.
- **Market:** fill in full on the next evaluation at the evaluating bar’s close (last).
- **Limit:** buy limit fills if bar low ≤ limit; sell limit fills if bar high ≥ limit. Fill price = limit (no microstructure improvement model in v1).
- **Stop:** buy stop fills if bar high ≥ stop; sell stop fills if bar low ≤ stop. Fill price = stop (no slippage model in v1).
- **Updating last bar:** re-check on every last-bar OHLC update, not only on close/roll.
- **New bar / gap:** a roll that trades through the trigger fills on that evaluation; do not invent skipped minutes.
- **No partials.** Reject qty that the account cannot fully support (buying power / margin / asset-class rule) rather than filling a stub.
- **Unavailable series:** if the vendor cannot serve 1m for that **instrument**, the **working order** remains working and does not invent a fill.
- Times stored as unix; UI displays **New York time**.

### Paper accounts and risk

- Fields: name, initial balance, currency **USD only** in v1, commission rule, leverage rule (including off / 1×), optional asset-class restriction.
- Multiple rows in SQLite; snapshot exposes the list plus which id is **active**. Settings can create/edit. **Switching UX TBD** — the TUI may show the active name; do not ship a designed switcher. Tests may select active via IPC.
- First-launch: if no rows exist, create one USD **paper account** with an implementer-chosen default name and initial balance, shown in settings.
- Commission: per-account rule applied on each fill; **balance history** records the cash effect.
- Leverage / margin: when enabled, reserve margin on entry; **margin call / liquidation** closes the **position** if maintenance fails (liquidation writes an exit **filled order history** leg and a **trade mark**). Exact numeric defaults are implementer-chosen and visible in settings.
- Asset-class restriction: reject place when the **instrument** is outside the account’s allow-list.

### Brackets (model 3)

- A **bracket** is a linked group id: parent entry (**working order** or open **position**) plus TP and SL **child working orders**.
- Position row shows TP/SL prices. Engine keeps children as **working orders** until fill or cancel.
- Child fill: write a separate history leg; cancel the sibling child; flatten or reduce the **position** by the child qty (v1: full position qty on the bracket — no partials).
- Cancel parent entry before fill: cancel children too. Orphan TP/SL is out of product language.

### TUI paper chrome (ADR-0003)

- Do **not** add a candlestick renderer. Paper overlays compose on the existing **price pane** (vendored widget + overlay pass).
- **Working levels** (limits, stops, TP/SL): horizontal **lines** at price, in the same overlay family as VP levels (price-scale segments), visually distinct from MA polylines and from **trade marks**.
- **Trade marks:** overlay **pins** (dot glyph) at fill price **and** fill time (column from bar time). Pins already paint last; use that layer. Persist after close; honor hide/show per pair without deleting SQLite fills.
- Paint order stays candles → existing indicator overlays → working **lines** → **trade marks** (marks must not be mistaken for live TP/SL lines).
- Dual layout: draw only on charts whose **instrument** matches. Order defaults = **focused chart**.
- Order side panel: place/modify/cancel; market/limit/stop; **bracket** TP/SL fields; confirm for nudge. **No** permanent working-orders table. **Positions**, **filled order history**, and **balance history** are tables inside the paper panel.
- **Input focus:** paper panel open → owns keys (like indicator panel). Watchlist and chart pan idle until it closes. Pin-placement / text prompt still modal if already open.
- Keyboard nudge: select a working level → tick up/down is a **draft** price → confirm sends modify to the engine. Escape/cancel drops the draft.

### Modules (conceptual — no frozen paths)

- **Paper book (engine):** accounts, orders, positions, fills, balances, mark visibility; SQLite load/save; in-memory hot copy.
- **Bar-touch evaluator (engine):** on bar-update (last bar or roll) for 1m series of paper-active instruments; market/limit/stop; no partials; **bracket** sibling cancel.
- **Paper IPC:** additive snapshot slice; command routes; discrete WS paper events.
- **1m paper interest (engine):** keep/eval 1m bars for instruments with working/open paper state, even without a visible chart slot.
- **TUI paper panel + order side panel:** place/modify/cancel/nudge confirm; tables; shortcut TBD.
- **TUI overlay:** working **lines** + **trade mark** pins on existing price-pane scale helpers.

### API contract expectations (behavioral)

Snapshot after Phase B still includes feed, workspace, quotes, indicators, **plus** a paper object: active **paper account**, account list (settings), **working orders** (for lines/nudge — not a TUI table mandate), **positions**, recent **filled order history**, **balance history** summary/rows, **trade mark** visibilities. Missing paper key = empty desk (optional/default-empty), never a parse hard-fail on old shapes during the additive land.

Commands (names flexible): create/edit **paper account**; place; modify; cancel; confirm-nudge (or modify with the nudged price); hide/show **trade mark** pair. Selecting the active account via IPC is allowed for tests even while **switching UX** stays TBD.

WS: existing events unchanged. New paper events after place/fill/cancel/liquidation/mark-visibility so the TUI does not poll.

## Testing Decisions

### What makes a good test

- Assert **external behavior** at the highest existing seams, not private helpers, SQLite schema trivia, or Ratatui pixel dumps of the vendored candles.
- Prefer: fake vendor + HTTP/WS → place a **working order** → `inject_tick` that updates last bar or rolls a 1m bar through the trigger → expect fill, **position** or flatten, **filled order history** row(s), **balance history**, snapshot + WS paper event.
- Avoid: re-testing `apply_tick` internals, asserting SQL, or requiring live LSE.

### Primary suite — engine IPC (highest seam)

Same black-box HTTP+WS + **fake vendor** pattern as live-bar, workspace, watchlist, and indicator contract tests:

- Snapshot includes paper (empty or default account) without dropping feed/workspace/quotes/indicators; `last_vendor_tick_ts` still present.
- Create **paper account**; persist; restart engine (or new app with same SQLite path) restores accounts, **working orders**, **positions**, **filled order history**, **balance history**, mark visibility. Workspace JSON unchanged in role.
- Place market / limit / stop; modify; cancel.
- **Bar-touch:** last-bar update crosses limit/stop → fill; last-bar update that does **not** touch → still working; period roll through trigger → fill; **market** fills on evaluation.
- **No partials:** oversized qty rejected; no stub fill.
- **Bracket** model 3: children exist while parent is working/open; TP fill writes exit history leg and cancels SL (and reverse); round-turn = two **filled order history** rows.
- Fill while the **instrument** is **not** the charted symbol (1m paper interest armed).
- Times in snapshot/events are unix; tests may check values, not TUI labels.
- Asset-class restriction rejects the wrong class.
- Margin/liquidation: a constructed adverse 1m series liquidates and writes an exit leg.
- Paper WS events are not lost across a burst of fills (no latest-wins drop).
- **Aligned live bars** / feed delay contract tests still pass (no regress).
- Unavailable 1m → no invented fill.

### TUI suite (in-module unit tests, existing pattern)

Prior art: New York clock / delay formatters; overlay compose (levels, pins); IPC serde defaults for additive feed fields.

- Deserialize snapshot **with and without** the paper key (additive).
- Formatter helpers for **New York time** on fill/place columns.
- Overlay: working **lines** at price; **trade mark** pins at price/time; hide pair removes pins not fills (client state from engine visibility flags).
- Keyboard nudge: draft price changes on tick keys; confirm emits modify; cancel leaves the previous working price.
- Paper panel **input focus** owns keys when open (same idea as indicator panel); do not require a full integration TUI.

### CI

- Fake vendor only. No live LSE job for Phase B. Optional LSE tests stay gated as today.

## Out of Scope

- Tick-based fill evaluation (later milestone; **bar-touch vs tick fill**)
- TradingView-grade mouse drag / projection of working levels
- Repairing LSE vault freshness or vendor `tick.ts`
- Live broker / multi-broker routing
- Strategy, alerts-as-strategy, or backtest
- Async older-bar edge fetch
- New candle renderer or replacing the vendored widget (ADR-0003 stands)
- Delta volume bubbles
- Permanent working-orders table
- Partial fills
- Non-USD paper currencies
- Redis, Postgres, ZeroMQ
- Paper panel keyboard shortcut (**TBD**)
- **Paper account** switching UX (**TBD**)
- Changing **aligned live bars**, **feed delay**, or **wall clock** behavior

## Further Notes

- Grill / glossary: `CONTEXT.md`. **Delivery phases** name Phase B as next; that wins. The **Language** entry for **paper trading** still says “scope phase still open” — leftover; do not treat it as blocking Phase B. When CONTEXT is next edited, close that sentence.
- Suggested issue tracker location for this document: `.scratch/phase-b-paper-m1-nudge/spec.md`. Follow-on tickets via `/to-tickets` under `.scratch/phase-b-paper-m1-nudge/issues/` when ready — **not this spec turn**.
- Intended git branch for all Phase B work: `grok_bot_dev` (Matt Pocock: to-spec → to-tickets → implement). Branch creation was blocked (GitHub 403) at spec time; land the file locally first.
- ADR-0001 / 0002 unchanged. ADR-0003 unchanged: paper is overlay lines/marks, not a new renderer.
- 1m evaluation is a Phase B product pick so **chart** timeframe is not a hidden fill parameter. Tick fills later can ignore bars.
- Default initial balance and commission/leverage numbers are implementer-chosen and must be visible in **paper account** settings; do not hide them in code-only constants without a settings surface.
