use chrono::{FixedOffset, Offset, Utc};
use itertools::Itertools;
use ratatui::{
    prelude::{Buffer, Rect},
    style::{Color, Style, Styled},
    widgets::StatefulWidget,
};

use crate::{
    candle::{Candle, CandleType},
    candlestick_chart_state::CandleStikcChartInfo,
    chart_view::ChartView,
    x_axis::{Interval, XAxis},
    y_axis::{Numeric, YAxis},
    CandleStickChartState,
};

/// Rows reserved under the price pane for the time axis (corner + two label rows).
pub const X_AXIS_HEIGHT: u16 = 3;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CandleStickChart {
    /// Candle interval
    interval: Interval,
    /// Candle data
    candles: Vec<Candle>,
    /// y axis scale/precision
    numeric: Numeric,
    /// Widget style
    style: Style,
    /// Candle style,
    bearish_color: Color,
    bullish_color: Color,
    /// display timezone
    display_timezone: FixedOffset,
}

impl CandleStickChart {
    pub fn new(interval: Interval) -> Self {
        Self {
            interval,
            candles: Vec::default(),
            numeric: Numeric::default(),
            style: Style::default(),
            bearish_color: Color::Rgb(234, 74, 90),
            bullish_color: Color::Rgb(52, 208, 88),
            display_timezone: Utc.fix(),
        }
    }

    pub fn candles(mut self, candles: Vec<Candle>) -> Self {
        self.candles = candles;
        self
    }

    pub fn y_axis_numeric(mut self, numeric: Numeric) -> Self {
        self.numeric = numeric;
        self
    }

    pub fn style(mut self, style: Style) -> Self {
        self.style = style;
        self
    }

    pub fn bearish_color(mut self, color: Color) -> Self {
        self.bearish_color = color;
        self
    }

    pub fn bullish_color(mut self, color: Color) -> Self {
        self.bullish_color = color;
        self
    }

    pub fn display_timezone(mut self, offset: FixedOffset) -> Self {
        self.display_timezone = offset;
        self
    }

    /// Compute the visible window + price scale for `area` without painting.
    ///
    /// Mirrors the layout used by [`StatefulWidget::render`] so overlays can share
    /// coordinates. Returns `None` when there is no data or the area is too small.
    pub fn compute_view(&self, area: Rect, state: &CandleStickChartState) -> Option<ChartView> {
        if self.candles.is_empty() {
            return None;
        }

        let global_min = self.candles.iter().map(|c| c.low).min().unwrap();
        let global_max = self.candles.iter().map(|c| c.high).max().unwrap();

        let y_axis_width: u16 =
            YAxis::estimated_width(self.numeric.clone(), global_min, global_max);
        if area.width <= y_axis_width || area.height <= X_AXIS_HEIGHT {
            return None;
        }

        let chart_width = area.width - y_axis_width;
        let chart_width_usize = chart_width as usize;

        let first_timestamp = self.candles.first().unwrap().timestamp;
        let last_timestamp = self.candles.last().unwrap().timestamp;

        let chart_end_timestamp = state.cursor_timestamp.unwrap_or(last_timestamp);
        let chart_start_timestamp =
            chart_end_timestamp - self.interval as i64 * 1000 * (chart_width_usize as i64 - 1);

        // Price scale from real candles inside the visible window only.
        let (y_min, y_max) = visible_price_range(
            &self.candles,
            first_timestamp,
            last_timestamp,
            chart_start_timestamp,
            chart_end_timestamp,
        )?;

        Some(ChartView {
            area,
            y_axis_width,
            x_axis_height: X_AXIS_HEIGHT,
            y_min: *y_min,
            y_max: *y_max,
            view_start_ts: chart_start_timestamp,
            view_end_ts: chart_end_timestamp,
            interval: self.interval,
            is_live_tip: state.cursor_timestamp.is_none(),
        })
    }
}

impl Styled for CandleStickChart {
    type Item = CandleStickChart;

    fn style(&self) -> Style {
        self.style
    }

    fn set_style<S: Into<Style>>(self, style: S) -> Self::Item {
        self.style(style.into())
    }
}

fn visible_price_range(
    candles: &[Candle],
    first_timestamp: i64,
    last_timestamp: i64,
    chart_start_timestamp: i64,
    chart_end_timestamp: i64,
) -> Option<(crate::Float, crate::Float)> {
    let mut min_p: Option<crate::Float> = None;
    let mut max_p: Option<crate::Float> = None;
    for c in candles {
        if c.timestamp < chart_start_timestamp || c.timestamp > chart_end_timestamp {
            continue;
        }
        if c.timestamp < first_timestamp || c.timestamp > last_timestamp {
            continue;
        }
        min_p = Some(min_p.map_or(c.low, |m| m.min(c.low)));
        max_p = Some(max_p.map_or(c.high, |m| m.max(c.high)));
    }
    match (min_p, max_p) {
        (Some(min), Some(max)) => Some((min, max)),
        _ => None,
    }
}

impl StatefulWidget for CandleStickChart {
    type State = CandleStickChartState;

    /// render like:
    /// |---|-----------------------|
    /// | y |                       |
    /// |   |                       |
    /// | a |                       |
    /// | x |                       |
    /// | i |                       |
    /// | s |       chart data      |
    /// |   |                       |
    /// | a |                       |
    /// | r |                       |
    /// | e |                       |
    /// | a |                       |
    /// |---|-----------------------|
    ///     |      x axis area      |
    ///     |-----------------------|
    fn render(self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
        if self.candles.is_empty() {
            state.last_view = None;
            return;
        }

        let global_min = self.candles.iter().map(|c| c.low).min().unwrap();
        let global_max = self.candles.iter().map(|c| c.high).max().unwrap();

        let y_axis_width: u16 =
            YAxis::estimated_width(self.numeric.clone(), global_min, global_max);
        if area.width <= y_axis_width || area.height <= X_AXIS_HEIGHT {
            state.last_view = None;
            return;
        }

        let chart_width = area.width - y_axis_width;
        let chart_width_usize = chart_width as usize;

        // with first/last dummies
        let first_timestamp = self.candles.first().unwrap().timestamp;
        let last_timestamp = self.candles.last().unwrap().timestamp;

        let mut candles = Vec::new();
        for i in (1..=(chart_width as i64 - 1)).rev() {
            candles.push(
                Candle::new(
                    first_timestamp - i * self.interval as i64 * 1000,
                    0.,
                    0.,
                    0.,
                    0.,
                )
                .unwrap(),
            );
        }
        candles.extend(self.candles.clone());
        for i in 1..=(chart_width as i64 - 1) {
            candles.push(
                Candle::new(
                    last_timestamp + i * self.interval as i64 * 1000,
                    0.,
                    0.,
                    0.,
                    0.,
                )
                .unwrap(),
            );
        }

        let chart_end_timestamp = state.cursor_timestamp.unwrap_or(last_timestamp);
        let chart_start_timestamp =
            chart_end_timestamp - self.interval as i64 * 1000 * (chart_width_usize as i64 - 1);
        let rendered_candles = candles
            .iter()
            .filter(|c| c.timestamp >= chart_start_timestamp && c.timestamp <= chart_end_timestamp)
            .collect_vec();

        if rendered_candles.is_empty() {
            state.last_view = None;
            return;
        }

        state.set_info(CandleStikcChartInfo::new(
            candles[chart_width_usize - 1].timestamp,
            candles.last().unwrap().timestamp,
            self.interval,
            last_timestamp,
            rendered_candles.first().unwrap().timestamp < first_timestamp,
        ));

        let y_min = rendered_candles
            .iter()
            .filter(|c| c.timestamp >= first_timestamp && c.timestamp <= last_timestamp)
            .map(|c| c.low)
            .min()
            .unwrap();
        let y_max = rendered_candles
            .iter()
            .filter(|c| c.timestamp >= first_timestamp && c.timestamp <= last_timestamp)
            .map(|c| c.high)
            .max()
            .unwrap();

        let price_height = area.height - X_AXIS_HEIGHT;
        let y_axis = YAxis::new(self.numeric.clone(), price_height, y_min, y_max);
        let rendered_y_axis = y_axis.render();
        // All buffer writes must use area origin so dual layout / status chrome work.
        for (y, string) in rendered_y_axis.iter().enumerate() {
            buf.set_string(area.x, area.y + y as u16, string, self.style);
        }

        let timestamp_min = rendered_candles.first().unwrap().timestamp;
        let timestamp_max = rendered_candles.last().unwrap().timestamp;

        let x_axis = XAxis::new(
            chart_width,
            timestamp_min,
            timestamp_max,
            self.interval,
            state.cursor_timestamp.is_none(),
        );
        let rendered_x_axis = x_axis.render(self.display_timezone);
        buf.set_string(
            area.x + y_axis_width - 2,
            area.y + area.height - X_AXIS_HEIGHT,
            "└──",
            self.style,
        );
        for (y, string) in rendered_x_axis.iter().enumerate() {
            buf.set_string(
                area.x + y_axis_width,
                area.y + area.height - X_AXIS_HEIGHT + y as u16,
                string,
                self.style,
            );
        }

        let mut offset = 0;
        let mut prev_timestamp =
            rendered_candles.first().unwrap().timestamp - self.interval as i64 * 1000;
        for (x, candle) in rendered_candles.iter().enumerate() {
            if candle.timestamp < first_timestamp || candle.timestamp > last_timestamp {
                prev_timestamp = candle.timestamp;
                continue;
            }
            let gap = (candle.timestamp - prev_timestamp) / (self.interval as i64 * 1000);
            if gap > 1 {
                offset += gap as u16 - 1;
            }
            let (candle_type, rendered) = candle.render(&y_axis);

            let color = match candle_type {
                CandleType::Bearish => self.bearish_color,
                CandleType::Bullish => self.bullish_color,
            };

            for (y, char) in rendered.iter().enumerate() {
                let cell_x = area.x + x as u16 + y_axis_width + offset;
                let cell_y = area.y + y as u16;
                if cell_x < area.x + area.width && cell_y < area.y + price_height {
                    buf[(cell_x, cell_y)]
                        .set_symbol(char)
                        .set_style(Style::default().fg(color));
                }
            }
            prev_timestamp = candle.timestamp;
        }

        state.last_view = Some(ChartView {
            area,
            y_axis_width,
            x_axis_height: X_AXIS_HEIGHT,
            y_min: *y_min,
            y_max: *y_max,
            view_start_ts: chart_start_timestamp,
            view_end_ts: chart_end_timestamp,
            interval: self.interval,
            is_live_tip: state.cursor_timestamp.is_none(),
        });
    }
}

#[cfg(test)]
mod tests {
    use ratatui::{
        buffer::{Buffer, Cell},
        layout::Rect,
        style::Style,
        widgets::StatefulWidget,
    };

    use crate::{Candle, CandleStickChart, CandleStickChartState, Interval};

    fn render(widget: CandleStickChart, width: u16, height: u16) -> Buffer {
        let area = Rect::new(0, 0, width, height);
        let mut cell = Cell::default();
        cell.set_symbol("x");
        let mut buffer = Buffer::filled(area, cell);
        widget.render(area, &mut buffer, &mut CandleStickChartState::default());
        buffer.set_style(area, Style::reset());
        buffer
    }

    fn render_at(
        widget: CandleStickChart,
        full: Rect,
        area: Rect,
        state: &mut CandleStickChartState,
    ) -> Buffer {
        let mut cell = Cell::default();
        cell.set_symbol("·");
        let mut buffer = Buffer::filled(full, cell);
        widget.render(area, &mut buffer, state);
        buffer
    }

    #[test]
    fn empty_candle() {
        let widget = CandleStickChart::new(Interval::OneMinute).candles(vec![]);
        let buffer = render(widget, 14, 8);
        assert_eq!(
            buffer,
            Buffer::with_lines(vec![
                "xxxxxxxxxxxxxx",
                "xxxxxxxxxxxxxx",
                "xxxxxxxxxxxxxx",
                "xxxxxxxxxxxxxx",
                "xxxxxxxxxxxxxx",
                "xxxxxxxxxxxxxx",
                "xxxxxxxxxxxxxx",
                "xxxxxxxxxxxxxx",
            ])
        );
    }

    #[test]
    fn simple_candle() {
        let widget = CandleStickChart::new(Interval::OneMinute)
            .candles(vec![Candle::new(0, 0.9, 3.0, 0.0, 2.1).unwrap()]);
        let buffer = render(widget, 14, 8);
        assert_eq!(
            buffer,
            Buffer::with_lines(vec![
                "     3.000 ├ │",
                "           │ │",
                "           │ ┃",
                "           │ │",
                "     0.600 ├ │",
                "xxxxxxxxxxx└──",
                "xxxxxxxxxxxxx ",
                "xxxxxxxxxxxxxx",
            ])
        );
    }

    #[test]
    fn simple_candle_with_x_label() {
        let widget = CandleStickChart::new(Interval::OneMinute)
            .candles(vec![Candle::new(0, 0.9, 3.0, 0.0, 2.1).unwrap()]);
        let buffer = render(widget, 30, 8);
        assert_eq!(
            buffer,
            Buffer::with_lines(vec![
                "     3.000 ├ xxxxxxxxxxxxxxxx│",
                "           │ xxxxxxxxxxxxxxxx│",
                "           │ xxxxxxxxxxxxxxxx┃",
                "           │ xxxxxxxxxxxxxxxx│",
                "     0.600 ├ xxxxxxxxxxxxxxxx│",
                "xxxxxxxxxxx└─────────────────┴",
                "xxxxxxxxxxxxx*1970/01/01 00:00",
                "xxxxxxxxxxxxxxxxxxxxxxxxxxxxxx",
            ])
        );
    }

    #[test]
    fn simple_candles_with_x_label() {
        let widget = CandleStickChart::new(Interval::OneMinute).candles(vec![
            Candle::new(0, 0.9, 3.0, 0.0, 2.1).unwrap(),
            Candle::new(60000, 2.1, 4.2, 2.1, 3.9).unwrap(),
            Candle::new(120000, 3.9, 4.1, 2.0, 2.3).unwrap(),
        ]);
        let buffer = render(widget, 19, 8);
        assert_eq!(
            buffer,
            Buffer::with_lines(vec![
                "     4.200 ├ xxx ╽┃",
                "           │ xxx│┃┃",
                "           │ xxx│╹╿",
                "           │ xxx│  ",
                "     0.840 ├ xxx│  ",
                "xxxxxxxxxxx└──────┴",
                "xxxxxxxxxxxxx*00:02",
                "xxxxxxxxxxxxxxxxxxx",
            ])
        );
    }

    #[test]
    fn simple_full_candles_with_x_label() {
        let widget = CandleStickChart::new(Interval::OneMinute).candles(vec![
            Candle::new(0, 0.9, 3.0, 0.0, 2.1).unwrap(),
            Candle::new(60000, 2.1, 4.2, 2.1, 3.9).unwrap(),
            Candle::new(120000, 3.9, 4.1, 2.0, 2.3).unwrap(),
            Candle::new(180000, 2.3, 3.9, 1.3, 2.0).unwrap(),
            Candle::new(240000, 2.0, 5.2, 0.9, 3.9).unwrap(),
        ]);
        let buffer = render(widget, 19, 8);
        assert_eq!(
            buffer,
            Buffer::with_lines(vec![
                "     5.200 ├ x ╷  │",
                "           │ x ╽┃││",
                "           │ x│┃╿│┃",
                "           │ x┃ ╵││",
                "     1.040 ├ x│   ╵",
                "xxxxxxxxxxx└──────┴",
                "xxxxxxxxxxxxx*00:04",
                "xxxxxxxxxxxxxxxxxxx",
            ])
        );
    }

    #[test]
    fn simple_omitted_candles_with_x_label() {
        let widget = CandleStickChart::new(Interval::OneMinute).candles(vec![
            Candle::new(0, 0.9, 3.0, 0.0, 2.1).unwrap(),
            Candle::new(240000, 2.0, 5.2, 0.9, 3.9).unwrap(),
        ]);
        let buffer = render(widget, 19, 8);
        assert_eq!(
            buffer,
            Buffer::with_lines(vec![
                "     5.200 ├ x xxx│",
                "           │ x xxx│",
                "           │ x│xxx┃",
                "           │ x┃xxx│",
                "     1.040 ├ x│xxx╵",
                "xxxxxxxxxxx└──────┴",
                "xxxxxxxxxxxxx*00:04",
                "xxxxxxxxxxxxxxxxxxx",
            ])
        );
    }

    #[test]
    fn simple_candle_with_not_changing() {
        let widget = CandleStickChart::new(Interval::OneSecond).candles(vec![
            Candle::new(0, 0.0, 1000.0, 0.0, 50.0).unwrap(),
            Candle::new(1000, 50.0, 50.0, 50.0, 50.0).unwrap(),
            Candle::new(2000, 500.0, 500.0, 500.0, 500.0).unwrap(),
        ]);
        let buffer = render(widget, 16, 8);
        assert_eq!(
            buffer,
            Buffer::with_lines(vec![
                "  1000.000 ├ │  ",
                "           │ │  ",
                "           │ │ ╻",
                "           │ │  ",
                "   200.000 ├ │╻ ",
                "xxxxxxxxxxx└────",
                "xxxxxxxxxxxxx   ",
                "xxxxxxxxxxxxxxxx",
            ])
        );
    }

    #[test]
    fn simple_candle_with_small_candle() {
        let widget = CandleStickChart::new(Interval::OneSecond).candles(vec![
            Candle::new(0, 0.0, 1000.0, 0.0, 50.0).unwrap(),
            Candle::new(1000, 450.0, 580.0, 320.0, 450.0).unwrap(),
            Candle::new(2000, 580.0, 580.0, 320.0, 320.0).unwrap(),
        ]);
        let buffer = render(widget, 16, 8);
        assert_eq!(
            buffer,
            Buffer::with_lines(vec![
                "  1000.000 ├ │  ",
                "           │ │  ",
                "           │ │╽┃",
                "           │ │╵╹",
                "   200.000 ├ │  ",
                "xxxxxxxxxxx└────",
                "xxxxxxxxxxxxx   ",
                "xxxxxxxxxxxxxxxx",
            ])
        );
    }

    /// Dual layout / status chrome: widget must not paint at absolute (0,0) when
    /// the layout `area` is offset.
    #[test]
    fn render_respects_area_origin() {
        let widget = CandleStickChart::new(Interval::OneMinute)
            .candles(vec![Candle::new(0, 0.9, 3.0, 0.0, 2.1).unwrap()]);
        let full = Rect::new(0, 0, 40, 20);
        let area = Rect::new(5, 3, 14, 8);
        let mut state = CandleStickChartState::default();
        let buffer = render_at(widget, full, area, &mut state);

        // Outside the chart rect stays filler.
        assert_eq!(buffer[(0, 0)].symbol(), "·");
        assert_eq!(buffer[(4, 3)].symbol(), "·");
        assert_eq!(buffer[(5, 2)].symbol(), "·");

        // Inside area has content (y-axis / candle glyphs).
        let mut painted = 0usize;
        for y in area.top()..area.bottom() {
            for x in area.left()..area.right() {
                if buffer[(x, y)].symbol() != "·" {
                    painted += 1;
                }
            }
        }
        assert!(
            painted > 10,
            "expected candle+axis paint inside offset area, painted={painted}"
        );

        // last_view records the same absolute area for overlay helpers.
        let view = state.last_view.expect("view after render");
        assert_eq!(view.area, area);
        assert_eq!(view.candle_area().x, area.x + view.y_axis_width);
        assert_eq!(view.candle_area().y, area.y);
    }

    #[test]
    fn compute_view_matches_last_view_after_render() {
        let candles = vec![
            Candle::new(0, 0.9, 3.0, 0.0, 2.1).unwrap(),
            Candle::new(60000, 2.1, 4.2, 2.1, 3.9).unwrap(),
        ];
        let widget = CandleStickChart::new(Interval::OneMinute).candles(candles.clone());
        let area = Rect::new(2, 1, 24, 10);
        let mut state = CandleStickChartState::default();
        let computed = widget
            .compute_view(area, &state)
            .expect("compute_view");
        let widget2 = CandleStickChart::new(Interval::OneMinute).candles(candles);
        let full = Rect::new(0, 0, 40, 20);
        let _ = render_at(widget2, full, area, &mut state);
        let last = state.last_view.expect("last_view");
        assert_eq!(computed.area, last.area);
        assert_eq!(computed.y_axis_width, last.y_axis_width);
        assert_eq!(computed.view_start_ts, last.view_start_ts);
        assert_eq!(computed.view_end_ts, last.view_end_ts);
        assert!((computed.y_min - last.y_min).abs() < 1e-9);
        assert!((computed.y_max - last.y_max).abs() < 1e-9);
    }
}
