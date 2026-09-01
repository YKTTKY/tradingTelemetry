# 02 — Working orders: order side panel, place/modify/cancel, on-chart lines

**What to build:** From the **order side panel**, the trader places, modifies, and cancels a **working order** (market, limit, or stop) on the **focused chart**’s **instrument**. Resting limits/stops draw as horizontal **lines** on matching charts. There is no permanent working-orders table. Fills are out — a market **working order** may sit until 03 evaluates it.

**Blocked by:** 01 — Paper desk

**Status:** done

- [x] Order side panel can place market / limit / stop with side and qty; instrument defaults to the **focused chart**
- [x] Place rejected when qty cannot be fully supported (buying power / account rules already on the desk) — **no partials**, no stub **working order**
- [x] Modify qty / limit / stop on a resting **working order**; cancel removes it
- [x] Dual layout: place/modify target the focused chart’s **instrument**; **lines** paint only on charts whose **instrument** matches
- [x] Working **lines** live on the existing price pane (same overlay family as VP levels), distinct from MA polylines; no new candle renderer
- [x] Market **working orders** are accepted and stored; do **not** implement **bar-touch fill** in this ticket
- [x] No dense permanent working-orders table in the TUI
- [x] Snapshot `paper` includes **working orders** so reconnecting TUI redraws lines
- [x] Place / modify / cancel emit discrete paper WS events (not `bar_update`)
- [x] SQLite persists **working orders** across engine restart
- [x] Paper panel / order side panel open → owns keys (indicator-panel pattern)
- [x] Fake-vendor IPC tests for place / modify / cancel / persist. TUI tests for overlay lines and additive serde.
- [x] TUI never calls a vendor.

## Notes

- Parent spec: `.scratch/phase-b-paper-m1-nudge/spec.md`
- Language: **working order**, **order side panel**
- **Take profit** / **stop loss** as bracket children are 04, not this ticket
- Keyboard nudge of a line is 05
