# 10 — Anchored Volume Profile

**What to build:** The trader can add up to **2 Anchored Volume Profiles** per chart, each from a **single time anchor** forward to now (typical cash open **09:30 America/New_York**). Defaults: number of rows **500**, value area **70%**, volume and POC/VAH/VAL styling with toggles. Overlay on price; restore with chart.

**Blocked by:** 07 — Indicator panel + MA + Volume (naked → restore)

**Status:** ready-for-agent

- [ ] Anchored VP addable via indicator panel; max 2 per chart enforced
- [ ] Profile builds from one anchor forward to current/latest bars
- [ ] Defaults: rows 500, value area 70%; POC/VAH/VAL toggleable with style
- [ ] Anchor settable without full TV drawing suite (form/keyboard/preset acceptable)
- [ ] Overlay render; settings restore with workspace
- [ ] Contract tests cover anchor window and levels with fake bars

## Notes

- Parent spec: `.scratch/phase-a-chart-terminal/spec.md`
- Can proceed in parallel with 08 and 09 after 07
