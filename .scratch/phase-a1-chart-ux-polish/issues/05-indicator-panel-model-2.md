# 05 — Indicator panel Model 2 (Available | Current + type style)

**What to build:** Split indicator panel vertically into **Available** (left) and **Current** (right). Tab switches active list (default Current). Available: Enter/Space add; **`c`** type style popup (overlay strength). Current: on/off, remove, re-pin, instance hotkeys; no instance settings popup. Wire type style into overlay strength from ticket 02.

**Blocked by:** 02 — Overlay compose + type overlay strength

**Status:** done

- [x] Side-by-side Available / Current lists
- [x] Tab switches active list; default Current
- [x] Available add respects Phase A max counts / engine reject
- [x] Available `c` opens type style popup; strength applies to all instances of that type on the focused chart
- [x] Current keeps instance hotkeys and re-pin; no instance settings popup
- [x] Type style persisted with workspace/chart config
- [x] Help text updated

## Notes

- Parent spec: `.scratch/phase-a1-chart-ux-polish/spec.md`
- Clear-all: keep usable (e.g. Shift+C or Current-only binding)—document choice in issue comments when implemented

## Comments

- **Clear-all binding:** Current list only — `c` or **Shift+C**. Available `c` is type style (overlay strength popup). Documented in help popup + status chrome.
- Model 2: Available (left catalog) | Current (right instances); Tab switches active side; panel open defaults to Current.
- Type style popup: draft strength with ←/→ or +/-; Enter confirms via `set_chart_overlay_strength` → `POST /v1/chart/type-styles` (ticket 02); Esc cancels.
- Letter quick-add (`m v p f a y g`) still works while the panel is open.
