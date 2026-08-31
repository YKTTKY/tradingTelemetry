//! New York wall clock and feed delay for Feed status.

use chrono::{DateTime, Utc};
use chrono_tz::{America::New_York, OffsetName};

/// Current New York civil time for Feed status (`11:17:44 EDT`).
///
/// Abbreviation follows tzdata (`EDT` / `EST`); not a frozen year-round EST.
pub fn format_ny_wall_clock(now: DateTime<Utc>) -> String {
    let ny = now.with_timezone(&New_York);
    let abbrev = ny.offset().abbreviation().unwrap_or("ET");
    format!("{} {abbrev}", ny.format("%H:%M:%S"))
}

/// Delay is shown only when vendor tick time is this far behind wall (or more).
const FEED_DELAY_VISIBLE_SECS: f64 = 5.0;

/// Compact feed delay when vendor tick time is behind wall by at least 5s.
///
/// Hidden when `last_vendor_tick_ts` is missing or delay is under 5 seconds.
pub fn format_feed_delay(now_ts: f64, last_vendor_tick_ts: Option<f64>) -> Option<String> {
    let last = last_vendor_tick_ts?;
    let delay = now_ts - last;
    if delay < FEED_DELAY_VISIBLE_SECS {
        return None;
    }
    Some(format_compact_delay(delay as u64))
}

/// Feed-status suffix: New York wall clock, plus delay when visible.
pub fn format_feed_clocks(now: DateTime<Utc>, last_vendor_tick_ts: Option<f64>) -> String {
    let clock = format_ny_wall_clock(now);
    match format_feed_delay(now.timestamp() as f64, last_vendor_tick_ts) {
        Some(delay) => format!("  {clock}  delay {delay}"),
        None => format!("  {clock}"),
    }
}

/// Compact duration: `5s`, `26m`, `1h 02m`.
fn format_compact_delay(secs: u64) -> String {
    if secs < 60 {
        format!("{secs}s")
    } else if secs < 3600 {
        format!("{}m", secs / 60)
    } else {
        let hours = secs / 3600;
        let minutes = (secs % 3600) / 60;
        format!("{hours}h {minutes:02}m")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn ny_wall_clock_uses_edt_in_summer() {
        // 2026-08-14 15:17:44 UTC is 11:17:44 in US Eastern daylight time.
        let utc = Utc.with_ymd_and_hms(2026, 8, 14, 15, 17, 44).unwrap();
        assert_eq!(format_ny_wall_clock(utc), "11:17:44 EDT");
    }

    #[test]
    fn ny_wall_clock_uses_est_in_winter() {
        // 2026-01-15 12:00:00 UTC is 07:00:00 in US Eastern standard time.
        let utc = Utc.with_ymd_and_hms(2026, 1, 15, 12, 0, 0).unwrap();
        assert_eq!(format_ny_wall_clock(utc), "07:00:00 EST");
    }

    #[test]
    fn feed_delay_hidden_when_no_vendor_tick() {
        assert_eq!(format_feed_delay(1_000.0, None), None);
    }

    #[test]
    fn feed_delay_hidden_under_five_seconds() {
        assert_eq!(format_feed_delay(1_000.0, Some(996.0)), None);
        assert_eq!(format_feed_delay(1_000.0, Some(995.1)), None);
    }

    #[test]
    fn feed_delay_compact_seconds_minutes_hours() {
        assert_eq!(
            format_feed_delay(1_000.0, Some(995.0)).as_deref(),
            Some("5s")
        );
        assert_eq!(
            format_feed_delay(1_000.0, Some(941.0)).as_deref(),
            Some("59s")
        );
        assert_eq!(
            format_feed_delay(1_000.0, Some(1_000.0 - 26.0 * 60.0)).as_deref(),
            Some("26m")
        );
        assert_eq!(
            format_feed_delay(1_000.0, Some(1_000.0 - (3600.0 + 2.0 * 60.0))).as_deref(),
            Some("1h 02m")
        );
    }

    #[test]
    fn feed_clocks_match_status_example() {
        let utc = Utc.with_ymd_and_hms(2026, 8, 14, 15, 17, 44).unwrap();
        let last = utc.timestamp() as f64 - 26.0 * 60.0;
        assert_eq!(
            format_feed_clocks(utc, Some(last)),
            "  11:17:44 EDT  delay 26m"
        );
        assert_eq!(format_feed_clocks(utc, None), "  11:17:44 EDT");
        assert_eq!(
            format_feed_clocks(utc, Some(utc.timestamp() as f64 - 4.0)),
            "  11:17:44 EDT"
        );
    }
}
