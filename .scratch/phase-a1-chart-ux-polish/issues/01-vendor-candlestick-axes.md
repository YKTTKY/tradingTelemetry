# 01 — Vendor candlestick widget + price/time axes

**What to build:** Vendor [tui-candlestick-chart](https://github.com/codingskynet/tui-candlestick-chart) into the TUI (MIT), fix `area` origin so dual layout and chrome work, map product timeframes to widget intervals, render OHLC with **price Y-axis** and **time X-axis** on the price pane. Expose visible window + price scale helpers for overlays.

**Blocked by:** none

**Status:** ready-for-agent

- [ ] Vendored (or forked-in-tree) candlestick widget builds with Ratatui 0.30.x
- [ ] All buffer writes respect `area.x` / `area.y` (dual + status bar safe)
- [ ] Timeframes `1m`…`1W` map to widget intervals
- [ ] Focused and dual charts show readable candles + axes
- [ ] Scale/window helpers available for overlay pass
- [ ] ADR (or draft) for vendor-vs-dep decision

## Notes

- Parent spec: `.scratch/phase-a1-chart-ux-polish/spec.md`
- Prefer America/New_York axis labels for product consistency
