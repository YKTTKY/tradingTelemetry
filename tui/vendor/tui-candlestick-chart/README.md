# tui-candlestick-chart (vendored)

Vendored into Trading Telemetry from
[codingskynet/tui-candlestick-chart](https://github.com/codingskynet/tui-candlestick-chart)
(MIT; based on [cli-candlestick-chart](https://github.com/Julien-R44/cli-candlestick-chart)).

## Local changes

- All buffer writes offset by `area.x` / `area.y` so dual layout and chrome work.
- Public `ChartView` / scale helpers for overlay compose on the same price/time scale.
- Kept on Ratatui 0.30.x to match the host TUI.

See `docs/adr/0003-vendor-candlestick-widget.md` in the monorepo root for why we vendor.
