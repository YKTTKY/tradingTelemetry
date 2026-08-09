# 09 — Fixed Range Volume Profile

**What to build:** The trader can add up to **4 Fixed Range Volume Profiles** per chart, each defined by **two time anchors** (start, end). **Extend to right** when **on** does both: (1) accumulate volume from new bars past the end anchor, and (2) project **POC / VAH / VAL** levels to the right. When **off**, only the closed window counts and levels do not project past it. Defaults include number of rows **200**, value area **70%**, box width, histogram and level styling. Anchors settable without requiring TradingView-grade drag (form/keyboard acceptable). Restore with chart.

**Blocked by:** 07 — Indicator panel + MA + Volume (naked → restore)

**Status:** done

- [x] Fixed Range VP addable via indicator panel; max 4 per chart enforced
- [x] Range defined by two time anchors (start, end)
- [x] Extend to right **on**: live build past end + level projection together
- [x] Extend to right **off**: closed [start, end] only; no level projection past window
- [x] Defaults: rows 200, value area 70%, box width and styling/toggles for histogram and POC/VAH/VAL
- [x] Overlay render on price chart; settings restore with workspace
- [x] Contract tests cover extend on vs off behavioral difference with fake bars

## Notes

- Parent spec: `.scratch/phase-a-chart-terminal/spec.md`
- Can proceed in parallel with 08 and 10 after 07
- Engine type: `fixed_range_vp`. Series profile fields: `range_start`, `range_end`, `anchor_end`, `levels_end`, `extend_to_right`.
- TUI: `f` add · `e` toggle extend · `,`/`.` nudge start · `</>` nudge end · `s` placement · `1`/`2`/`3` POC/VAH/VAL
