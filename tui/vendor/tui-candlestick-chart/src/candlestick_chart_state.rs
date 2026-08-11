use std::cmp::{max, min};

use crate::chart_view::ChartView;
use crate::Interval;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CandleStikcChartInfo {
    cursor_first_timestamp: i64,
    cursor_last_timestamp: i64,
    interval: Interval,
    latest_timestamp: i64,
    need_previous_candles: bool,
}

impl CandleStikcChartInfo {
    pub(crate) fn new(
        cursor_first_timestamp: i64,
        cursor_last_timestamp: i64,
        interval: Interval,
        latest_timestamp: i64,
        need_previous_candles: bool,
    ) -> Self {
        Self {
            cursor_first_timestamp,
            cursor_last_timestamp,
            latest_timestamp,
            interval,
            need_previous_candles,
        }
    }
}

#[derive(Debug, Default, Clone, PartialEq)]
pub struct CandleStickChartState {
    pub(crate) info: Option<CandleStikcChartInfo>,
    pub(crate) cursor_timestamp: Option<i64>,
    /// Last computed layout/scale for overlay compose (filled on each successful render).
    pub last_view: Option<ChartView>,
}

impl CandleStickChartState {
    pub(crate) fn set_info(&mut self, info: CandleStikcChartInfo) {
        if let Some(cursor_timestamp) = self.cursor_timestamp {
            if cursor_timestamp == info.latest_timestamp {
                self.cursor_timestamp = None;
            } else {
                self.cursor_timestamp = Some(
                    cursor_timestamp.clamp(info.cursor_first_timestamp, info.cursor_last_timestamp),
                );
            }
        }
        self.info = Some(info);
    }

    pub fn try_move_backward(&mut self) {
        if let Some(info) = &self.info {
            let cursor = if let Some(cursor_timestamp) = self.cursor_timestamp {
                cursor_timestamp - info.interval as i64 * 1000
            } else {
                info.latest_timestamp - info.interval as i64 * 1000
            };

            self.cursor_timestamp = Some(max(cursor, info.cursor_first_timestamp));
        }
    }

    pub fn try_move_forward(&mut self) {
        if let Some(info) = &self.info {
            let cursor = if let Some(cursor_timestamp) = self.cursor_timestamp {
                cursor_timestamp + info.interval as i64 * 1000
            } else {
                info.latest_timestamp + info.interval as i64 * 1000
            };

            self.cursor_timestamp = Some(min(cursor, info.cursor_last_timestamp));
        }
    }

    pub fn is_needed_previous_candles(&self) -> bool {
        if let Some(info) = &self.info {
            info.need_previous_candles
        } else {
            false
        }
    }

    pub fn reset_cursor(&mut self) {
        self.cursor_timestamp = None;
    }

    /// Set the visible window end (ms). `None` = live tip (follows latest bar).
    pub fn set_cursor_timestamp(&mut self, ts_ms: Option<i64>) {
        self.cursor_timestamp = ts_ms;
    }

    /// Visible window end (ms), or `None` when live tip (follows latest bar).
    pub fn cursor_timestamp(&self) -> Option<i64> {
        self.cursor_timestamp
    }
}
