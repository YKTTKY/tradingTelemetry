# 02 — Overlay compose + type overlay strength

**What to build:** After the candlestick base paints, compose MA, Session/Fixed/Anchored VP histogram + POC/VAH/VAL, and FRVP/AVP pins on the **same** price/time scale. Volume remains a sub-pane. Continuous overlays **may win** candle cells; **overlay strength** (type style) softens recoloring via intensity/blend. Persist strength per chart per indicator type.

**Blocked by:** 01 — Vendor candlestick widget + price/time axes

**Status:** done

- [x] MA drawn continuous on price pane without a second independent Canvas world
- [x] VP hist + levels + pins aligned to widget window/scale
- [x] Volume sub-pane unchanged in role (under price when enabled)
- [x] Paint order: candles/axes → VP hist → levels → MA → pins
- [x] Overlay strength tunable and applied on overlap
- [x] Phase A indicator behavior (limits, enable, restore) still works

## Notes

- Parent spec: `.scratch/phase-a1-chart-ux-polish/spec.md`
- Type style UI popup is ticket 05; this ticket can plumb strength with a default if UI lands in parallel

## Comments

- Cell-based compose in `tui/src/overlay.rs`: after candle widget paint, hist → levels → MA → pins on the same buffer using `ChartView` helpers (no Braille Canvas for price overlays).
- Overlay strength: per chart + indicator type; defaults (MA 0.85, VP 0.35); blend softens candle recolor; `Chart::set_overlay_strength` + `overlay_strength_map`.
- Persist: engine `primary_type_styles` / dual slots, public `charts[].type_styles`, `POST /v1/chart/type-styles`; TUI restores via workspace snapshot; `post_type_styles` helper for ticket 05 UI.
- Volume remains a separate sub-pane under price when enabled.
