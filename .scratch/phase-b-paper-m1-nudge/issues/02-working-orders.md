# 02 — Working orders: order side panel, place/modify/cancel, on-chart lines

**What to build:** From the **order side panel**, the trader places, modifies, and cancels a **working order** (market, limit, or stop) on the **focused chart**’s **instrument**. Resting limits/stops draw as horizontal **lines** on matching charts. There is no permanent working-orders table. Fills are out — a market **working order** may sit until 03 evaluates it.

**Blocked by:** 01 — Paper desk

**Status:** ready-for-agent

- [ ] Order side panel can place market / limit / stop with side and qty; instrument defaults to the **focused chart**
- [ ] Place rejected when qty cannot be fully supported (buying power / account rules already on the desk) — **no partials**, no stub **working order**
- [ ] Modify qty / limit / stop on a resting **working order**; cancel removes it
- [ ] Dual layout: place/modify target the focused chart’s **instrument**; **lines** paint only on charts whose **instrument** matches
- [ ] Working **lines** live on the existing price pane (same overlay family as VP levels), distinct from MA polylines; no new candle renderer
- [ ] Market **working orders** are accepted and stored; do **not** implement **bar-touch fill** in this ticket
- [ ] No dense permanent working-orders table in the TUI
- [ ] Snapshot `paper` includes **working orders** so reconnecting TUI redraws lines
- [ ] Place / modify / cancel emit discrete paper WS events (not `bar_update`)
- [ ] SQLite persists **working orders** across engine restart
- [ ] Paper panel / order side panel open → owns keys (indicator-panel pattern)
- [ ] Fake-vendor IPC tests for place / modify / cancel / persist. TUI tests for overlay lines and additive serde.
- [ ] TUI never calls a vendor.

## Notes

- Parent spec: `.scratch/phase-b-paper-m1-nudge/spec.md`
- Language: **working order**, **order side panel**
- **Take profit** / **stop loss** as bracket children are 04, not this ticket
- Keyboard nudge of a line is 05
