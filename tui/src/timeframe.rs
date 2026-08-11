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

/// Unix seconds when the forming bar (open = `last_bar_open_ts`) closes / next bar opens.
///
/// Uses product timeframe period alignment (same unix-bucket model as the engine).
/// Returns `None` for unknown timeframes.
pub fn forming_bar_end_ts(timeframe: &str, last_bar_open_ts: i64) -> Option<i64> {
    let period = product_timeframe_seconds(timeframe)?;
    Some(last_bar_open_ts.saturating_add(period))
}

/// Seconds remaining in the forming incomplete bar for `timeframe`.
///
/// - `None` when the timeframe is unknown (do not invent a countdown).
/// - `Some(0)` when the period has already elapsed (stale tip / market closed).
/// - Clamped to ≥ 0; does not go negative.
pub fn forming_bar_remaining_secs(
    timeframe: &str,
    last_bar_open_ts: i64,
    now_ts: i64,
) -> Option<u64> {
    let end = forming_bar_end_ts(timeframe, last_bar_open_ts)?;
    // Clamp past-end to 0 — never invent negative times (i64::saturating_sub is not a floor-at-zero).
    let remaining = end - now_ts;
    Some(if remaining > 0 { remaining as u64 } else { 0 })
}

/// Human-readable forming-bar countdown for chart chrome.
///
/// - under 1 hour: `m:ss` / `mm:ss`
/// - under 1 day: `h:mm:ss`
/// - 1 day+: `Nd HH:MM:SS`
pub fn format_bar_countdown(remaining_secs: u64) -> String {
    let s = remaining_secs % 60;
    let m = (remaining_secs / 60) % 60;
    let h = (remaining_secs / 3600) % 24;
    let d = remaining_secs / 86_400;
    if d > 0 {
        format!("{d}d {h:02}:{m:02}:{s:02}")
    } else if h > 0 || remaining_secs >= 3600 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
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

    #[test]
    fn forming_bar_remaining_1m_counts_down_to_period_end() {
        // Bar open at t=1000, 1m period → end at 1060.
        assert_eq!(forming_bar_end_ts("1m", 1000), Some(1060));
        assert_eq!(forming_bar_remaining_secs("1m", 1000, 1000), Some(60));
        assert_eq!(forming_bar_remaining_secs("1m", 1000, 1030), Some(30));
        assert_eq!(forming_bar_remaining_secs("1m", 1000, 1059), Some(1));
        assert_eq!(forming_bar_remaining_secs("1m", 1000, 1060), Some(0));
        // Past end: clamp at 0 (do not invent negative times).
        assert_eq!(forming_bar_remaining_secs("1m", 1000, 2000), Some(0));
    }

    #[test]
    fn forming_bar_remaining_uses_chart_timeframe_independently() {
        // Same open + now; 5m has more time left than 1m.
        let open = 10_000;
        let now = open + 30;
        assert_eq!(forming_bar_remaining_secs("1m", open, now), Some(30));
        assert_eq!(forming_bar_remaining_secs("5m", open, now), Some(270));
        assert_eq!(forming_bar_remaining_secs("1h", open, now), Some(3570));
    }

    #[test]
    fn forming_bar_remaining_unknown_timeframe_is_none() {
        assert_eq!(forming_bar_remaining_secs("2m", 0, 0), None);
        assert_eq!(forming_bar_end_ts("", 0), None);
    }

    #[test]
    fn format_bar_countdown_compact_for_short_and_long() {
        assert_eq!(format_bar_countdown(0), "0:00");
        assert_eq!(format_bar_countdown(5), "0:05");
        assert_eq!(format_bar_countdown(65), "1:05");
        assert_eq!(format_bar_countdown(3599), "59:59");
        assert_eq!(format_bar_countdown(3600), "1:00:00");
        assert_eq!(format_bar_countdown(3661), "1:01:01");
        assert_eq!(format_bar_countdown(86_400), "1d 00:00:00");
        assert_eq!(format_bar_countdown(90_061), "1d 01:01:01");
    }
}
