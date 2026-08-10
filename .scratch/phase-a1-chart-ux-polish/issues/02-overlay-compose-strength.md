# 02 — Overlay compose + type overlay strength

**What to build:** After the candlestick base paints, compose MA, Session/Fixed/Anchored VP histogram + POC/VAH/VAL, and FRVP/AVP pins on the **same** price/time scale. Volume remains a sub-pane. Continuous overlays **may win** candle cells; **overlay strength** (type style) softens recoloring via intensity/blend. Persist strength per chart per indicator type.

**Blocked by:** 01 — Vendor candlestick widget + price/time axes

**Status:** ready-for-agent

- [ ] MA drawn continuous on price pane without a second independent Canvas world
- [ ] VP hist + levels + pins aligned to widget window/scale
- [ ] Volume sub-pane unchanged in role (under price when enabled)
- [ ] Paint order: candles/axes → VP hist → levels → MA → pins
- [ ] Overlay strength tunable and applied on overlap
- [ ] Phase A indicator behavior (limits, enable, restore) still works

## Notes

- Parent spec: `.scratch/phase-a1-chart-ux-polish/spec.md`
- Type style UI popup is ticket 05; this ticket can plumb strength with a default if UI lands in parallel
