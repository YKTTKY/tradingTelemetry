# 05 — Keyboard nudge then confirm

**What to build:** On the focused **chart**, the trader **selects** a working **line**, **keyboard-nudges** its price (tick up/down) as a **draft**, then **confirms**. Confirm sends modify to the engine. Escape/cancel drops the draft and leaves the live **working order** unchanged. No TradingView-grade mouse drag.

**Blocked by:** 02 — Working orders

**Status:** ready-for-agent

- [ ] Select a working level on the focused chart (limit, stop, and later TP/SL lines from 04)
- [ ] Tick up/down changes a **draft** price only; engine price is unchanged until confirm
- [ ] Confirm emits modify (nudged price); the line then follows the engine
- [ ] Escape/cancel drops the draft; previous working price remains
- [ ] Unconfirmed nudge must not move SQLite / snapshot working price
- [ ] No mouse drag / projection of working levels in this ticket
- [ ] TUI unit tests: draft changes on tick keys; confirm emits modify; cancel leaves the previous price
- [ ] Input still belongs to the paper chrome while this flow is active (no watchlist/chart-pan steal)

## Notes

- Parent spec: `.scratch/phase-b-paper-m1-nudge/spec.md`
- Language: **working order**; M1+nudge means keyboard-first
- Can land in parallel with 03/04; once 04 exists, the same path nudges TP/SL children
- Paper-panel **shortcut** remains TBD — do not invent one here
