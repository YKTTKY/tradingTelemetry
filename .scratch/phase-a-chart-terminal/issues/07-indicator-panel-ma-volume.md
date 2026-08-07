# 07 — Indicator panel + MA + Volume (naked → restore)

**What to build:** Charts open **naked** until the user adds indicators. An **indicator panel** adds/toggles/configures indicators for the **focused chart** only. Ship **Moving Average** (up to **3** lines: SMA or EMA each; default lengths **10 / 60 / 200** when adding the default stack) and **Volume** (max **1**, histogram sub-pane under price). Last-used indicators and settings **restore per chart** with the workspace.

**Blocked by:** 05 — Dual layout + workspace persistence

**Status:** ready-for-agent

- [ ] First-ever chart with no saved indicator state opens naked (no overlays/sub-panes)
- [ ] Indicator panel can add, toggle, and configure indicators for the focused chart only
- [ ] Dual layout: each chart has its own indicator set and limits
- [ ] MA: max 3 lines; per-line SMA or EMA; default stack lengths 10 / 60 / 200
- [ ] Volume: max 1 instance; sub-pane histogram under price
- [ ] Instance limits enforced with clear behavior (reject or clamp — pick one and test it)
- [ ] Engine computes MA/Volume and exposes them via snapshot + live updates as needed
- [ ] TUI draws MA on price and Volume in a sub-pane
- [ ] Indicator configs restore per chart after restart (file-backed workspace)
- [ ] Contract tests cover apply, limits, and restore for MA and Volume with fake vendor bars

## Notes

- Parent spec: `.scratch/phase-a-chart-terminal/spec.md`
- VP variants are separate tickets 08–10; GEX/GARCH is 12
