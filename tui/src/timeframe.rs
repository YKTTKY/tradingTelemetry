//! Product timeframe ↔ candlestick widget interval mapping.

use tui_candlestick_chart::Interval;

use crate::app::V1_TIMEFRAMES;

/// Map a product timeframe string (`1m`…`1W`) to the vendored widget [`Interval`].
///
/// Returns `None` for unknown / non-v1 values.
pub fn product_timeframe_to_interval(timeframe: &str) -> Option<Interval> {
    match timeframe {
        "1m" => Some(Interval::OneMinute),
        "3m" => Some(Interval::ThreeMinutes),
        "5m" => Some(Interval::FiveMinutes),
        "15m" => Some(Interval::FifteenMinutes),
        "30m" => Some(Interval::ThirtyMinutes),
        "1h" => Some(Interval::OneHour),
        "4h" => Some(Interval::FourHours),
        "1D" => Some(Interval::OneDay),
        "1W" => Some(Interval::OneWeek),
        _ => None,
    }
}

/// Interval seconds for a product timeframe (unix bar open spacing).
pub fn product_timeframe_seconds(timeframe: &str) -> Option<i64> {
    product_timeframe_to_interval(timeframe).map(|i| i as i64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tui_candlestick_chart::Interval;

    #[test]
    fn all_v1_timeframes_map_to_intervals() {
        let expected = [
            ("1m", Interval::OneMinute, 60),
            ("3m", Interval::ThreeMinutes, 180),
            ("5m", Interval::FiveMinutes, 300),
            ("15m", Interval::FifteenMinutes, 900),
            ("30m", Interval::ThirtyMinutes, 1800),
            ("1h", Interval::OneHour, 3600),
            ("4h", Interval::FourHours, 14400),
            ("1D", Interval::OneDay, 86400),
            ("1W", Interval::OneWeek, 604800),
        ];
        assert_eq!(V1_TIMEFRAMES.len(), expected.len());
        for (tf, interval, secs) in expected {
            assert!(
                V1_TIMEFRAMES.contains(&tf),
                "fixture timeframe {tf} missing from V1_TIMEFRAMES"
            );
            assert_eq!(product_timeframe_to_interval(tf), Some(interval));
            assert_eq!(product_timeframe_seconds(tf), Some(secs));
        }
    }

    #[test]
    fn unknown_timeframe_is_none() {
        assert_eq!(product_timeframe_to_interval("2m"), None);
        assert_eq!(product_timeframe_to_interval(""), None);
        assert_eq!(product_timeframe_to_interval("1d"), None); // case-sensitive product tokens
    }
}
