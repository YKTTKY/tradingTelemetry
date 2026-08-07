# 03 — Live conflated bar updates

**What to build:** After history is on the chart, **live** updates from the fake vendor move the **last bar** (and advance bars as needed). Updates reach the TUI over **WebSocket** and are **conflated/throttled** so a burst of market events does not flood the UI tick-for-tick.

**Blocked by:** 02 — History candles for default workspace (fake vendor)

**Status:** ready-for-agent

- [ ] Fake vendor can emit live price/bar updates for an interested instrument+timeframe
- [ ] WebSocket delivers bar updates that the TUI applies to the open chart
- [ ] Last bar (and completed bars when the period rolls) reflect live progress
- [ ] Conflation/throttle policy ensures fewer UI events than raw underlying ticks under burst load (asserted at IPC seam)
- [ ] Contract tests cover live bar path with fake vendor
- [ ] Chart remains usable (no freezes) under synthetic high-frequency fake ticks

## Notes

- Parent spec: `.scratch/phase-a-chart-terminal/spec.md`
- Publish policy from ADR-0002: do not forward every tick to the TUI
