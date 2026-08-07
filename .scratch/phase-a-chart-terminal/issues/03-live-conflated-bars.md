# 03 — Live conflated bar updates

**What to build:** After history is on the chart, **live** updates from the fake vendor move the **last bar** (and advance bars as needed). Updates reach the TUI over **WebSocket** and are **conflated/throttled** so a burst of market events does not flood the UI tick-for-tick.

**Blocked by:** 02 — History candles for default workspace (fake vendor)

**Status:** done

- [x] Fake vendor can emit live price/bar updates for an interested instrument+timeframe
- [x] WebSocket delivers bar updates that the TUI applies to the open chart
- [x] Last bar (and completed bars when the period rolls) reflect live progress
- [x] Conflation/throttle policy ensures fewer UI events than raw underlying ticks under burst load (asserted at IPC seam)
- [x] Contract tests cover live bar path with fake vendor
- [x] Chart remains usable (no freezes) under synthetic high-frequency fake ticks

## Notes

- Parent spec: `.scratch/phase-a-chart-terminal/spec.md`
- Publish policy from ADR-0002: do not forward every tick to the TUI

## Comments

### Implementation notes

- **Vendor live seam:** `FakeVendor.subscribe` / `inject_tick` (+ optional auto random-walk when CLI runs).
- **Sim clock:** live ticks without explicit `ts` advance a history-anchored clock so the open last bar updates (wall clock would immediately period-roll past 2024 fixtures).
- **Aggregation:** `ChartService` keeps per interest series; ticks update tip OHLCV or roll on timeframe bucket.
- **Conflation:** `ConflatingHub` coalesces pending tips (~50ms) and fans out `bar_update` WS events.
- **IPC event:** `{type: bar_update, instrument, timeframe, completed_bars, bar}`.
- **TUI:** parses `bar_update` on the WS loop and merges into the open chart series.
- **Tests:** `engine/tests/test_ipc_live_bars.py` (update, burst conflation, period roll, no interest, sim-clock tip update).
