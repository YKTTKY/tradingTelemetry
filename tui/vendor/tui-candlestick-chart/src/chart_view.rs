//! Visible window + price scale helpers for overlays that share coordinates with candles.

use ratatui::layout::Rect;

use crate::x_axis::Interval;

/// Layout and scale of the last rendered (or computed) candlestick view.
///
/// Coordinates are absolute terminal cells so dual layout and chrome can place overlays
/// on the same price/time grid as the candle widget.
#[derive(Debug, Clone, PartialEq)]
pub struct ChartView {
    /// Full widget area (including axes).
    pub area: Rect,
    /// Width of the price Y-axis strip in cells.
    pub y_axis_width: u16,
    /// Rows reserved for the time X-axis (corner + labels). Always 3 in this widget.
    pub x_axis_height: u16,
    /// Visible price range (min/max of real candles in the window).
    pub y_min: f64,
    pub y_max: f64,
    /// Inclusive window end timestamp (ms, candle open time).
    pub view_end_ts: i64,
    /// Inclusive window start timestamp (ms).
    pub view_start_ts: i64,
    /// Candle interval for this view.
    pub interval: Interval,
    /// True when the cursor is at the live tip (right edge follows latest bar).
    pub is_live_tip: bool,
}

impl ChartView {
    /// Height of the candle body region (excludes X-axis rows).
    pub fn price_height(&self) -> u16 {
        self.area.height.saturating_sub(self.x_axis_height)
    }

    /// Width of the candle body region (excludes Y-axis strip).
    pub fn price_width(&self) -> u16 {
        self.area.width.saturating_sub(self.y_axis_width)
    }

    /// Absolute rectangle where candle glyphs are painted (no axis chrome).
    pub fn candle_area(&self) -> Rect {
        Rect {
            x: self.area.x + self.y_axis_width,
            y: self.area.y,
            width: self.price_width(),
            height: self.price_height(),
        }
    }

    /// Map a price to an absolute terminal row (top of candle area = high).
    ///
    /// Returns `None` if the price region has zero height or the price is outside
    /// the scaled range by more than a tiny epsilon (still clamps into range when inside span).
    pub fn price_to_row(&self, price: f64) -> Option<u16> {
        let h = self.price_height();
        if h == 0 {
            return None;
        }
        let span = (self.y_max - self.y_min).max(f64::EPSILON);
        // Row 0 is max price (top); row h-1 is min price (bottom of candle area).
        let frac = (self.y_max - price) / span;
        let row = (frac * (h as f64 - 1.0).max(0.0))
            .round()
            .clamp(0.0, (h.saturating_sub(1)) as f64) as u16;
        Some(self.area.y + row)
    }

    /// Map a price to the fractional Y unit used by candle glyph rendering (0 = low, height = high).
    pub fn price_to_y_unit(&self, price: f64) -> f64 {
        let h = self.price_height() as f64;
        if h <= 0.0 {
            return 0.0;
        }
        let span = (self.y_max - self.y_min).max(f64::EPSILON);
        (price - self.y_min) / span * h
    }

    /// Map a candle open timestamp (ms) to an absolute terminal column in the candle area.
    ///
    /// Uses linear time → column mapping (`view_start` + `interval`). Matches continuous
    /// series; if the renderer inserts extra gap columns for missing bars, prefer
    /// aligning overlays via the same gap-aware path as paint (see widget render).
    pub fn timestamp_to_col(&self, ts_ms: i64) -> Option<u16> {
        let w = self.price_width();
        if w == 0 {
            return None;
        }
        let step = self.interval as i64 * 1000;
        if step <= 0 {
            return None;
        }
        if ts_ms < self.view_start_ts || ts_ms > self.view_end_ts {
            return None;
        }
        let idx = (ts_ms - self.view_start_ts) / step;
        if idx < 0 || idx >= w as i64 {
            return None;
        }
        Some(self.area.x + self.y_axis_width + idx as u16)
    }

    /// Number of candle columns (one bar per cell) in the price pane.
    pub fn candle_columns(&self) -> u16 {
        self.price_width()
    }
}

#[cfg(test)]
mod tests {
    use ratatui::layout::Rect;

    use super::ChartView;
    use crate::Interval;

    fn sample_view() -> ChartView {
        ChartView {
            area: Rect::new(10, 5, 30, 13),
            y_axis_width: 12,
            x_axis_height: 3,
            y_min: 100.0,
            y_max: 200.0,
            view_start_ts: 1_000_000,
            view_end_ts: 1_000_000 + 60_000 * 17, // 18 columns of 1m
            interval: Interval::OneMinute,
            is_live_tip: true,
        }
    }

    #[test]
    fn candle_area_offsets_by_axes() {
        let v = sample_view();
        let c = v.candle_area();
        assert_eq!(c.x, 22);
        assert_eq!(c.y, 5);
        assert_eq!(c.width, 18);
        assert_eq!(c.height, 10);
    }

    #[test]
    fn price_to_row_maps_high_to_top() {
        let v = sample_view();
        assert_eq!(v.price_to_row(200.0), Some(5));
        assert_eq!(v.price_to_row(100.0), Some(14)); // y=5 + 9
    }

    #[test]
    fn timestamp_to_col_maps_window() {
        let v = sample_view();
        assert_eq!(v.timestamp_to_col(1_000_000), Some(22));
        assert_eq!(v.timestamp_to_col(1_000_000 + 60_000), Some(23));
        assert_eq!(v.timestamp_to_col(999_999), None);
    }
}
