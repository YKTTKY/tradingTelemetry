# 04 — Brackets (model 3): entry + TP/SL children

**What to build:** A **bracket** is a linked group: parent entry (**working order** or open **position**) plus take-profit and stop-loss **child working orders**. The position row shows TP/SL. A child fill writes a **separate** **filled order history** exit leg and cancels the sibling. Orphan TP/SL is out.

**Blocked by:** 03 — Bar-touch fills

**Status:** done

- [x] Order side panel can place an entry with TP/SL, or attach/modify TP/SL on an open **position**
- [x] Engine keeps children as **working orders** until fill or cancel; they draw as working **lines** (02 overlay)
- [x] TP fill writes an exit history leg, cancels SL, and flattens/reduces the **position** by the child qty (v1: full position qty — no partials)
- [x] SL fill does the reverse (exit leg + cancel TP)
- [x] A round-turn is **two** **filled order history** rows (entry leg + exit leg)
- [x] Cancel parent entry before fill cancels children too
- [x] Position row shows TP/SL prices when a **bracket** is attached
- [x] Persist **brackets** in SQLite; snapshot + discrete paper WS keep the TUI in sync
- [x] Fake-vendor IPC: TP through last-bar/roll cancels SL (and reverse). TUI: bracket fields on the order side panel; TP/SL on the position row.
- [x] Do not add a permanent working-orders table to list children

## Notes

- Parent spec: `.scratch/phase-b-paper-m1-nudge/spec.md`
- Language: **bracket**, **working order**, **filled order history**
- TP/SL are child **working orders**, not new standalone entry types
- Keyboard nudge of those lines is 05 (same select/nudge path as other working levels)
