# 05 — Indicator panel Model 2 (Available | Current + type style)

**What to build:** Split indicator panel vertically into **Available** (left) and **Current** (right). Tab switches active list (default Current). Available: Enter/Space add; **`c`** type style popup (overlay strength). Current: on/off, remove, re-pin, instance hotkeys; no instance settings popup. Wire type style into overlay strength from ticket 02.

**Blocked by:** 02 — Overlay compose + type overlay strength

**Status:** ready-for-agent

- [ ] Side-by-side Available / Current lists
- [ ] Tab switches active list; default Current
- [ ] Available add respects Phase A max counts / engine reject
- [ ] Available `c` opens type style popup; strength applies to all instances of that type on the focused chart
- [ ] Current keeps instance hotkeys and re-pin; no instance settings popup
- [ ] Type style persisted with workspace/chart config
- [ ] Help text updated

## Notes

- Parent spec: `.scratch/phase-a1-chart-ux-polish/spec.md`
- Clear-all: keep usable (e.g. Shift+C or Current-only binding)—document choice in issue comments when implemented
