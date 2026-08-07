# 09 — Fixed Range Volume Profile

**What to build:** The trader can add up to **4 Fixed Range Volume Profiles** per chart, each defined by **two time anchors** (start, end). **Extend to right** when **on** does both: (1) accumulate volume from new bars past the end anchor, and (2) project **POC / VAH / VAL** levels to the right. When **off**, only the closed window counts and levels do not project past it. Defaults include number of rows **200**, value area **70%**, box width, histogram and level styling. Anchors settable without requiring TradingView-grade drag (form/keyboard acceptable). Restore with chart.

**Blocked by:** 07 — Indicator panel + MA + Volume (naked → restore)

**Status:** ready-for-agent

- [ ] Fixed Range VP addable via indicator panel; max 4 per chart enforced
- [ ] Range defined by two time anchors (start, end)
- [ ] Extend to right **on**: live build past end + level projection together
- [ ] Extend to right **off**: closed [start, end] only; no level projection past window
- [ ] Defaults: rows 200, value area 70%, box width and styling/toggles for histogram and POC/VAH/VAL
- [ ] Overlay render on price chart; settings restore with workspace
- [ ] Contract tests cover extend on vs off behavioral difference with fake bars

## Notes

- Parent spec: `.scratch/phase-a-chart-terminal/spec.md`
- Can proceed in parallel with 08 and 10 after 07
