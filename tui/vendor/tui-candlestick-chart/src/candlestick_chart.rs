use chrono::{FixedOffset, Offset, Utc};
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
    /// Dense layout: one real bar per column (weekends/session gaps do not open empty columns).
    pub fn compute_view(&self, area: Rect, state: &CandleStickChartState) -> Option<ChartView> {
        let layout = DenseLayout::compute(&self.candles, self.numeric.clone(), area, state)?;
        Some(layout.view(area, self.interval, state.cursor_timestamp.is_none()))
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

/// Dense (index-based) window into `candles` for the current area + cursor.
struct DenseLayout<'a> {
    y_axis_width: u16,
    chart_width: u16,
    /// Slice of real candles painted left→right (already windowed).
    visible: &'a [Candle],
    /// Empty columns on the left when fewer bars than width (right-align to live tip).
    column_offset: u16,
    y_min: crate::Float,
    y_max: crate::Float,
    /// Inclusive bar indices into the full series that can still be panned to.
    series_first_ts: i64,
    series_last_ts: i64,
    /// True when the left edge of the pane still has older bars available.
    need_previous: bool,
}

impl<'a> DenseLayout<'a> {
    fn compute(
        candles: &'a [Candle],
        numeric: Numeric,
        area: Rect,
        state: &CandleStickChartState,
    ) -> Option<Self> {
        if candles.is_empty() {
            return None;
        }

        let global_min = candles.iter().map(|c| c.low).min().unwrap();
        let global_max = candles.iter().map(|c| c.high).max().unwrap();
        let y_axis_width = YAxis::estimated_width(numeric, global_min, global_max);
        if area.width <= y_axis_width || area.height <= X_AXIS_HEIGHT {
            return None;
        }

        let chart_width = area.width - y_axis_width;
        let width = chart_width as usize;
        if width == 0 {
            return None;
        }

        let series_first_ts = candles.first().unwrap().timestamp;
        let series_last_ts = candles.last().unwrap().timestamp;

        // Window end = cursor bar (nearest at-or-before cursor) or live tip (last bar).
        let end_idx = match state.cursor_timestamp {
            Some(cursor_ts) => candles
                .iter()
                .rposition(|c| c.timestamp <= cursor_ts)
                .unwrap_or(0),
            None => candles.len() - 1,
        };
        let count = (end_idx + 1).min(width);
        let start_idx = end_idx + 1 - count;
        let visible = &candles[start_idx..=end_idx];

        let y_min = visible.iter().map(|c| c.low).min().unwrap();
        let y_max = visible.iter().map(|c| c.high).max().unwrap();

        // Right-align when the series is shorter than the pane (empty left gutter).
        let column_offset = (width - visible.len()) as u16;
        let need_previous = start_idx > 0;

        Some(Self {
            y_axis_width,
            chart_width,
            visible,
            column_offset,
            y_min,
            y_max,
            series_first_ts,
            series_last_ts,
            need_previous,
        })
    }

    fn view(&self, area: Rect, interval: Interval, is_live_tip: bool) -> ChartView {
        let column_timestamps: Vec<i64> = self.visible.iter().map(|c| c.timestamp).collect();
        ChartView {
            area,
            y_axis_width: self.y_axis_width,
            x_axis_height: X_AXIS_HEIGHT,
            y_min: *self.y_min,
            y_max: *self.y_max,
            view_start_ts: *column_timestamps.first().unwrap_or(&0),
            view_end_ts: *column_timestamps.last().unwrap_or(&0),
            interval,
            is_live_tip,
            column_timestamps,
            column_offset: self.column_offset,
        }
    }
}

impl StatefulWidget for CandleStickChart {
    type State = CandleStickChartState;

    /// render like:
    /// |---|-----------------------|
    /// | y |                       |
    /// | a |       chart data      |
    /// | x |   (dense: 1 bar/col)  |
    /// |---|-----------------------|
    ///     |      x axis area      |
    fn render(self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
        let Some(layout) = DenseLayout::compute(&self.candles, self.numeric.clone(), area, state)
        else {
            state.last_view = None;
            return;
        };

        // Pan bounds: first/last *bar* timestamps (not calendar dummies).
        state.set_info(CandleStikcChartInfo::new(
            layout.series_first_ts,
            layout.series_last_ts,
            self.interval,
            layout.series_last_ts,
            layout.need_previous,
        ));

        let price_height = area.height - X_AXIS_HEIGHT;
        let y_axis = YAxis::new(
            self.numeric.clone(),
            price_height,
            layout.y_min,
            layout.y_max,
        );
        let rendered_y_axis = y_axis.render();
        for (y, string) in rendered_y_axis.iter().enumerate() {
            buf.set_string(area.x, area.y + y as u16, string, self.style);
        }

        let column_timestamps: Vec<i64> = layout.visible.iter().map(|c| c.timestamp).collect();
        let rendered_x_axis = XAxis::render_dense(
            layout.chart_width,
            layout.column_offset,
            &column_timestamps,
            self.interval,
            state.cursor_timestamp.is_none(),
            self.display_timezone,
        );
        buf.set_string(
            area.x + layout.y_axis_width - 2,
            area.y + area.height - X_AXIS_HEIGHT,
            "└──",
            self.style,
        );
        for (y, string) in rendered_x_axis.iter().enumerate() {
            buf.set_string(
                area.x + layout.y_axis_width,
                area.y + area.height - X_AXIS_HEIGHT + y as u16,
                string,
                self.style,
            );
        }

        // Dense paint: one real candle per column, right-aligned.
        for (i, candle) in layout.visible.iter().enumerate() {
            let (candle_type, rendered) = candle.render(&y_axis);
            let color = match candle_type {
                CandleType::Bearish => self.bearish_color,
                CandleType::Bullish => self.bullish_color,
            };
            let col = layout.column_offset + i as u16;
            for (y, char) in rendered.iter().enumerate() {
                let cell_x = area.x + layout.y_axis_width + col;
                let cell_y = area.y + y as u16;
                if cell_x < area.x + area.width && cell_y < area.y + price_height {
                    buf[(cell_x, cell_y)]
                        .set_symbol(char)
                        .set_style(Style::default().fg(color));
                }
            }
        }

        state.last_view = Some(layout.view(area, self.interval, state.cursor_timestamp.is_none()));
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
        // Single bar is right-aligned into the price pane.
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

    /// Sparse calendar timestamps (gap) still paint **adjacent** columns — dense packing.
    #[test]
    fn gapped_timestamps_paint_dense_not_calendar() {
        let widget = CandleStickChart::new(Interval::OneMinute).candles(vec![
            Candle::new(0, 0.9, 3.0, 0.0, 2.1).unwrap(),
            // 4 minutes later — calendar packing would leave empty columns; dense does not.
            Candle::new(240000, 2.0, 5.2, 0.9, 3.9).unwrap(),
        ]);
        let buffer = render(widget, 19, 8);
        // Two candles in the last two columns (right-aligned), no gap columns between them.
        assert_eq!(
            buffer,
            Buffer::with_lines(vec![
                "     5.200 ├ xxxx │",
                "           │ xxxx │",
                "           │ xxxx│┃",
                "           │ xxxx┃│",
                "     1.040 ├ xxxx│╵",
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

    #[test]
    fn render_respects_area_origin() {
        let widget = CandleStickChart::new(Interval::OneMinute)
            .candles(vec![Candle::new(0, 0.9, 3.0, 0.0, 2.1).unwrap()]);
        let full = Rect::new(0, 0, 40, 20);
        let area = Rect::new(5, 3, 14, 8);
        let mut state = CandleStickChartState::default();
        let buffer = render_at(widget, full, area, &mut state);

        assert_eq!(buffer[(0, 0)].symbol(), "·");
        assert_eq!(buffer[(4, 3)].symbol(), "·");
        assert_eq!(buffer[(5, 2)].symbol(), "·");

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

        let view = state.last_view.expect("view after render");
        assert_eq!(view.area, area);
        assert_eq!(view.candle_area().x, area.x + view.y_axis_width);
        assert_eq!(view.candle_area().y, area.y);
        assert_eq!(view.painted_bars(), 1);
    }

    #[test]
    fn dense_window_takes_last_n_bars() {
        // 10 one-minute bars; narrow pane → only the latest columns.
        let candles: Vec<_> = (0..10)
            .map(|i| {
                Candle::new(
                    i * 60_000,
                    1.0 + i as f64,
                    2.0 + i as f64,
                    0.5 + i as f64,
                    1.5 + i as f64,
                )
                .unwrap()
            })
            .collect();
        let widget = CandleStickChart::new(Interval::OneMinute).candles(candles);
        // y-axis ~12 wide → chart_width = 19-12 = 7 for width 19? estimated_width depends on prices.
        let area = Rect::new(0, 0, 40, 12);
        let mut state = CandleStickChartState::default();
        let view = widget
            .compute_view(area, &state)
            .expect("view");
        // Live tip: last painted bar is the series tip.
        assert_eq!(view.view_end_ts, 9 * 60_000);
        assert!(view.painted_bars() > 0);
        assert!(view.painted_bars() <= view.price_width() as usize);
        // Columns are contiguous bar indices (no calendar holes).
        for w in view.column_timestamps.windows(2) {
            assert_eq!(w[1] - w[0], 60_000);
        }

        let _ = render_at(
            CandleStickChart::new(Interval::OneMinute).candles(
                (0..10)
                    .map(|i| {
                        Candle::new(
                            i * 60_000,
                            1.0 + i as f64,
                            2.0 + i as f64,
                            0.5 + i as f64,
                            1.5 + i as f64,
                        )
                        .unwrap()
                    })
                    .collect(),
            ),
            Rect::new(0, 0, 40, 12),
            area,
            &mut state,
        );
        let last = state.last_view.unwrap();
        assert_eq!(last.view_end_ts, view.view_end_ts);
        assert_eq!(last.painted_bars(), view.painted_bars());
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
        let computed = widget.compute_view(area, &state).expect("compute_view");
        let widget2 = CandleStickChart::new(Interval::OneMinute).candles(candles);
        let full = Rect::new(0, 0, 40, 20);
        let _ = render_at(widget2, full, area, &mut state);
        let last = state.last_view.expect("last_view");
        assert_eq!(computed.area, last.area);
        assert_eq!(computed.y_axis_width, last.y_axis_width);
        assert_eq!(computed.view_start_ts, last.view_start_ts);
        assert_eq!(computed.view_end_ts, last.view_end_ts);
        assert_eq!(computed.column_timestamps, last.column_timestamps);
        assert_eq!(computed.column_offset, last.column_offset);
    }
}
