# 01 — Vendor candlestick widget + price/time axes

**What to build:** Vendor [tui-candlestick-chart](https://github.com/codingskynet/tui-candlestick-chart) into the TUI (MIT), fix `area` origin so dual layout and chrome work, map product timeframes to widget intervals, render OHLC with **price Y-axis** and **time X-axis** on the price pane. Expose visible window + price scale helpers for overlays.

**Blocked by:** none

**Status:** done

- [x] Vendored (or forked-in-tree) candlestick widget builds with Ratatui 0.30.x
- [x] All buffer writes respect `area.x` / `area.y` (dual + status bar safe)
- [x] Timeframes `1m`…`1W` map to widget intervals
- [x] Focused and dual charts show readable candles + axes
- [x] Scale/window helpers available for overlay pass
- [x] ADR (or draft) for vendor-vs-dep decision

## Notes

- Parent spec: `.scratch/phase-a1-chart-ux-polish/spec.md`
- Prefer America/New_York axis labels for product consistency

## Comments

- Implemented: vendored widget at `tui/vendor/tui-candlestick-chart` (path dep), `area` origin fix + tests, `ChartView` scale/window helpers, product TF map in `tui/src/timeframe.rs`, wired into price pane (`draw_candles`), ADR-0003.
- Dense packing follow-up: one bar per column (equity weekends no longer open empty columns); confirmed continuous on 1m + 1D dual layout.
- Overlay paint still uses a secondary Canvas on the candle area (Phase A features kept); issue 02 owns compose + strength.
- Axis TZ: America/New_York offset at last bar (FixedOffset for the whole axis; DST mid-window is a known soft limit of the widget API).
- **Closed:** human accepted visual density as-is for this ship; further chart UX in later A.1 issues.
