# 02 — TUI: New York wall clock + feed delay

**What to build:** On Feed status, show **wall clock** in **New York time** next to the unix heartbeat, and **feed delay** from `now − last_vendor_tick_ts`. Hide delay when it is under 5 seconds or no vendor tick yet. No new candle renderer. Countdown formula stays as-is (it starts ticking once 01 lands a live wall-clock tip).

**Blocked by:** 01 — Engine: aligned live bars + last vendor tick time

**Status:** ready-for-agent

- [ ] Feed status shows current America/New_York time (e.g. `11:17:44 EDT`) beside `heartbeat=<unix>`
- [ ] Label follows tzdata (`EDT` / `EST`); do not hardcode year-round EST
- [ ] Feed delay shown compact (`26m`, `5s`, `1h 02m`) when `now − last_vendor_tick_ts >= 5s`
- [ ] Delay hidden when `< 5s` or `last_vendor_tick_ts` missing
- [ ] One delay for the desk (not per chart)
- [ ] Welcome Feed status line matches workspace (same clocks)
- [ ] Unit tests for NY clock formatting and delay formatting
- [ ] Help/chrome copy does not call this “last price time”
- [ ] No candlestick widget / Y-scale / gap-fill changes

## Notes

- Parent spec: `.scratch/phase-a1.1-aligned-live-bars/spec.md`
- Language: `CONTEXT.md` — **wall clock**, **New York time**, **feed delay**, **feed status**
- Example line: `feed: connected  vendor=lse  heartbeat=1786720665  11:17:44 EDT  delay 26m`
- Heartbeat remains engine unix (liveness). Wall clock is local `now` in `America/New_York`.

## Comments
