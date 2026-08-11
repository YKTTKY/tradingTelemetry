//! Vendored Ratatui candlestick chart widget.
//!
//! Based on [tui-candlestick-chart](https://github.com/codingskynet/tui-candlestick-chart)
//! (MIT) / cli-candlestick-chart. Local changes: `area` origin-safe buffer writes and
//! public scale/window helpers for overlay compose.

use ordered_float::OrderedFloat;

mod candle;
mod candlestick_chart;
mod candlestick_chart_state;
mod chart_view;
mod symbols;
mod x_axis;
mod y_axis;

pub use candle::Candle;
pub use candlestick_chart::CandleStickChart;
pub use candlestick_chart_state::CandleStickChartState;
pub use chart_view::ChartView;
pub use x_axis::Interval;
pub use y_axis::Numeric;

pub(crate) type Float = OrderedFloat<f64>;
