# 03 — Bar-touch fills: 1m eval, positions, filled history, commissions

**What to build:** A **working order** fills when the engine-owned **1m** series **bar-touch**es its trigger — last-bar update or a new-bar roll — even if that **instrument** is not on a chart. The trader sees a **Position**, append-only **filled order history**, commissions, and **balance history**. Times are unix in the engine; the TUI shows **New York time**.

**Blocked by:** 02 — Working orders

**Status:** ready-for-agent

- [ ] Evaluation series is **1m**, engine-owned, independent of the focused chart timeframe
- [ ] Engine keeps that **1m** series for any **instrument** with **working orders** or open **positions**, including uncharted symbols
- [ ] Hook the existing bar-update path (last bar or roll). Do not add a second vendor tick pipe.
- [ ] **Market:** full qty on the next evaluation at the evaluating bar’s close (last)
- [ ] **Limit:** buy if bar low ≤ limit; sell if bar high ≥ limit; fill price = limit
- [ ] **Stop:** buy if bar high ≥ stop; sell if bar low ≤ stop; fill price = stop
- [ ] Re-check on every last-bar OHLC update, not only on close; a roll that trades through still fills; do not invent skipped minutes
- [ ] **No partial fills.** Oversized qty was already rejected at place; fills are all-or-nothing
- [ ] Unavailable **1m** (no vendor series) → **working order** stays working; no invented fill
- [ ] Fill writes **filled order history**, updates **positions**, applies commission, appends **balance history**
- [ ] **Position** panel is distinct from the **watchlist**; row shows at least symbol, side, qty, average price, unrealized P&L
- [ ] **Filled order history** is append-only / filterable and is not a working-orders table; columns include symbol, side, type, qty, limit/stop/fill prices, commission, place time, fill/close time, duration, margin where relevant
- [ ] Engine stores unix; TUI formatters show **New York time** (`America/New_York`)
- [ ] Paper WS fill/position/balance events are discrete and must not drop across a burst of fills (no latest-wins)
- [ ] Fake-vendor contract: `inject_tick` updates last bar or rolls 1m through the trigger → fill. Include an uncharted-symbol case.
- [ ] Existing **aligned live bars** / feed delay tests still pass. Fake vendor only.
- [ ] TUI tables update from snapshot + paper WS without polling.

## Notes

- Parent spec: `.scratch/phase-b-paper-m1-nudge/spec.md`
- Language: **bar-touch fill**, **Position**, **filled order history**, **balance history**, **New York time**
- Brackets / sibling cancel are 04. Trade marks are 06. Leverage / liquidation / asset-class are 07.
- Manual flatten/close of a **position** (exit **filled order history** leg) belongs here so a round-turn can exist without 04.
