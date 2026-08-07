# 05 — Dual layout + workspace persistence

**What to build:** The trader can toggle **layout mode** between `single` and `dual-vertical` (two equal-height charts, top/bottom). First dual open with nothing saved uses top **QQQ** @ **1D**, bottom **SPY** @ **1D**. Each chart has independent instrument, timeframe, and later indicator set. **Welcome** then workspace on launch; **file-backed** restore of layout mode and per-chart instrument/timeframe on later launches.

**Blocked by:** 04 — Instrument and timeframe selection

**Status:** ready-for-agent

- [ ] User can toggle between `single` and `dual-vertical` at any time
- [ ] Dual layout shows two stacked charts with independent interest (instrument + timeframe)
- [ ] When dual has no saved selection, defaults are top **QQQ** @ **1D**, bottom **SPY** @ **1D**
- [ ] First launch path: Welcome → workspace `single`, **SPY** @ **1D** (unless already established in earlier tickets — keep consistent)
- [ ] Later launch: after Welcome, restore last layout mode and each chart’s instrument and timeframe from disk
- [ ] Engine snapshot on connect is enough for TUI to rebuild workspace interest
- [ ] Persistence does not use Redis/Postgres
- [ ] Tests cover save/restore of layout + chart selections (IPC and/or engine restart with same store)

## Notes

- Parent spec: `.scratch/phase-a-chart-terminal/spec.md`
- Indicator restore arrives with ticket 07; this ticket owns layout + instruments + timeframes
