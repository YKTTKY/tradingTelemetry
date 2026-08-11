//! Visible window + price scale helpers for overlays that share coordinates with candles.

use ratatui::layout::Rect;

use crate::x_axis::Interval;

/// Layout and scale of the last rendered (or computed) candlestick view.
///
/// Coordinates are absolute terminal cells so dual layout and chrome can place overlays
/// on the same price/time grid as the candle widget.
///
/// Layout is **dense**: one real bar per column (no empty weekend/session columns).
/// `column_timestamps` lists the open time (ms) of each painted candle column left→right.
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
    /// Inclusive window end timestamp (ms) — last painted bar.
    pub view_end_ts: i64,
    /// Inclusive window start timestamp (ms) — first painted bar.
    pub view_start_ts: i64,
    /// Candle interval (product/metadata; placement is index-dense, not calendar).
    pub interval: Interval,
    /// True when the cursor is at the live tip (right edge follows latest bar).
    pub is_live_tip: bool,
    /// Open timestamps (ms) for each candle column, left → right.
    /// Length equals the number of painted candles (≤ price_width). Right-aligned in the pane
    /// when fewer bars than columns (`column_offset` empty cells on the left).
    pub column_timestamps: Vec<i64>,
    /// Columns of left padding before the first painted candle (right-aligned live tip).
    pub column_offset: u16,
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
    pub fn price_to_row(&self, price: f64) -> Option<u16> {
        let h = self.price_height();
        if h == 0 {
            return None;
        }
        let span = (self.y_max - self.y_min).max(f64::EPSILON);
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

    /// Map a candle open timestamp (ms) to an absolute terminal column.
    ///
    /// Matches the **nearest** dense column by open time (equities skip non-trading sessions).
    pub fn timestamp_to_col(&self, ts_ms: i64) -> Option<u16> {
        if self.column_timestamps.is_empty() {
            return None;
        }
        let mut best_i = 0usize;
        let mut best_d = (self.column_timestamps[0] - ts_ms).abs();
        for (i, &t) in self.column_timestamps.iter().enumerate().skip(1) {
            let d = (t - ts_ms).abs();
            if d < best_d {
                best_d = d;
                best_i = i;
            }
        }
        // Reject timestamps far outside the painted window (more than one interval past ends).
        let step = (self.interval as i64 * 1000).max(1);
        if best_d > step {
            let first = *self.column_timestamps.first().unwrap();
            let last = *self.column_timestamps.last().unwrap();
            if ts_ms < first - step || ts_ms > last + step {
                return None;
            }
        }
        Some(self.area.x + self.y_axis_width + self.column_offset + best_i as u16)
    }

    /// Local canvas X (0-based within candle area) for a bar open timestamp (ms).
    pub fn timestamp_to_local_x(&self, ts_ms: i64) -> Option<f64> {
        let col = self.timestamp_to_col(ts_ms)?;
        let left = self.area.x + self.y_axis_width;
        Some((col - left) as f64 + 0.5)
    }

    /// Number of candle columns (full price pane width in cells).
    pub fn candle_columns(&self) -> u16 {
        self.price_width()
    }

    /// Number of painted bars in this view.
    pub fn painted_bars(&self) -> usize {
        self.column_timestamps.len()
    }
}

#[cfg(test)]
mod tests {
    use ratatui::layout::Rect;

    use super::ChartView;
    use crate::Interval;

    fn sample_view() -> ChartView {
        // 3 dense bars right-aligned into 18 columns → offset 15
        ChartView {
            area: Rect::new(10, 5, 30, 13),
            y_axis_width: 12,
            x_axis_height: 3,
            y_min: 100.0,
            y_max: 200.0,
            view_start_ts: 1_000_000,
            view_end_ts: 1_000_000 + 60_000 * 2,
            interval: Interval::OneMinute,
            is_live_tip: true,
            column_timestamps: vec![1_000_000, 1_060_000, 1_120_000],
            column_offset: 15,
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
        assert_eq!(v.price_to_row(100.0), Some(14));
    }

    #[test]
    fn timestamp_to_col_uses_dense_columns() {
        let v = sample_view();
        // first bar at local col 15 → absolute x = 22 + 15
        assert_eq!(v.timestamp_to_col(1_000_000), Some(22 + 15));
        assert_eq!(v.timestamp_to_col(1_060_000), Some(22 + 16));
        assert_eq!(v.timestamp_to_local_x(1_000_000), Some(15.5));
    }
}
