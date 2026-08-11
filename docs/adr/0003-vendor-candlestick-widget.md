# Vendor Ratatui candlestick widget in-tree

## Status

Accepted (Phase A.1 chart UX polish)

## Context

Phase A drew OHLC on a Braille `Canvas` without real price/time axes. Day-trading use needed readable cell-based candles plus **price Y-axis** and **time X-axis**, dual-layout safe, with scale helpers so overlays (MA, VP, pins) share coordinates.

[tui-candlestick-chart](https://github.com/codingskynet/tui-candlestick-chart) (MIT; based on cli-candlestick-chart) already implements Unicode candles + axes on **Ratatui 0.30.x**, matching this repo. Upstream is small, unreleased as a crates.io package we control, and has a known layout bug: buffer writes use absolute `(0, y)` instead of `area.x` / `area.y`, which breaks dual-vertical charts and status/watchlist chrome.

Options considered:

1. **Git dependency** on the upstream repo as-is.
2. **crates.io / path dep** if published (it is not a maintained published crate for our needs).
3. **Vendor (copy) into `tui/vendor/`** and maintain local fixes + helpers.
4. **Keep Canvas-only** and hand-roll axes.

## Decision

**Vendor** the widget at `tui/vendor/tui-candlestick-chart` as a path dependency of the TUI crate.

Local modifications (non-exhaustive):

- Offset all buffer writes by `area.x` / `area.y`.
- Expose `ChartView` (visible window + price scale) and `compute_view` / `last_view` for overlay compose.
- Keep MIT license + NOTICE for upstream provenance.

Product timeframes `1m`…`1W` map to widget `Interval` in the host TUI. Axis labels use **America/New_York** offsets for consistency with session clocks.

## Consequences

- We own merge/conflict cost if upstream improves; the surface is small and we needed invasive layout/API changes anyway.
- Overlay paint (issue 02+) can use public scale helpers without reaching into private widget internals.
- No crates.io release process for a personal day-trading terminal.
- Dual layout and non-zero chart rects are correct; tests lock origin-safe rendering.
