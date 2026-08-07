//! Application state: Welcome → single-layout workspace with default chart.

use crate::ipc::{BarUpdateEvent, ChartInterestResponse, FeedSnapshot, IpcEvent, OhlcvBar};

/// Exact empty-state copy when the vendor cannot serve the chart series.
pub const UNAVAILABLE_COPY: &str = "Data Currently not Available";

pub const DEFAULT_INSTRUMENT: &str = "SPY";
pub const DEFAULT_TIMEFRAME: &str = "1D";

/// v1 product timeframes only (domain). Cycle order is coarse → fine wrap.
pub const V1_TIMEFRAMES: [&str; 9] = [
    "1m", "3m", "5m", "15m", "30m", "1h", "4h", "1D", "1W",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Welcome,
    Workspace,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayoutMode {
    Single,
}

/// Modal input for instrument selection (and future prompts).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputMode {
    Normal,
    InstrumentPrompt { buffer: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnectionStatus {
    Connecting,
    Connected,
    Disconnected { reason: String },
}

impl ConnectionStatus {
    pub fn label(&self) -> &str {
        match self {
            ConnectionStatus::Connecting => "connecting",
            ConnectionStatus::Connected => "connected",
            ConnectionStatus::Disconnected { .. } => "disconnected",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ChartSeriesState {
    /// Interest not requested yet (or reload pending).
    Idle,
    Loading,
    Available { bars: Vec<OhlcvBar> },
    Unavailable,
    Error { message: String },
}

/// One workspace chart: instrument + timeframe + series state.
#[derive(Debug, Clone, PartialEq)]
pub struct Chart {
    pub instrument: String,
    pub timeframe: String,
    pub series: ChartSeriesState,
}

impl Chart {
    pub fn default_single() -> Self {
        Self {
            instrument: DEFAULT_INSTRUMENT.to_string(),
            timeframe: DEFAULT_TIMEFRAME.to_string(),
            series: ChartSeriesState::Idle,
        }
    }

    pub fn title(&self) -> String {
        format!("{} · {}", self.instrument, self.timeframe)
    }
}

#[derive(Debug, Clone)]
pub struct App {
    pub screen: Screen,
    pub layout: LayoutMode,
    pub chart: Chart,
    pub connection: ConnectionStatus,
    pub feed: Option<FeedSnapshot>,
    pub last_heartbeat_ts: Option<f64>,
    /// When true, the main loop should POST chart interest for the focused chart.
    pub needs_chart_load: bool,
    pub input_mode: InputMode,
    pub should_quit: bool,
}

impl Default for App {
    fn default() -> Self {
        Self {
            screen: Screen::Welcome,
            layout: LayoutMode::Single,
            chart: Chart::default_single(),
            connection: ConnectionStatus::Connecting,
            feed: None,
            last_heartbeat_ts: None,
            needs_chart_load: false,
            input_mode: InputMode::Normal,
            should_quit: false,
        }
    }
}

impl App {
    pub fn enter_workspace(&mut self) {
        if self.screen == Screen::Welcome {
            self.screen = Screen::Workspace;
            self.request_chart_load();
        }
    }

    pub fn request_chart_load(&mut self) {
        self.chart.series = ChartSeriesState::Loading;
        self.needs_chart_load = true;
    }

    pub fn chart_load_started(&mut self) {
        self.needs_chart_load = false;
    }

    pub fn quit(&mut self) {
        self.should_quit = true;
    }

    /// Cycle focused chart timeframe by `delta` steps within [`V1_TIMEFRAMES`] only.
    /// No-op outside workspace normal mode, or when the index would not change.
    pub fn cycle_timeframe(&mut self, delta: i32) {
        if self.screen != Screen::Workspace {
            return;
        }
        if !matches!(self.input_mode, InputMode::Normal) {
            return;
        }
        let Some(idx) = V1_TIMEFRAMES
            .iter()
            .position(|&tf| tf == self.chart.timeframe)
        else {
            // Unknown stored value — snap to default.
            self.chart.timeframe = DEFAULT_TIMEFRAME.to_string();
            self.request_chart_load();
            return;
        };
        let n = V1_TIMEFRAMES.len() as i32;
        let next = (idx as i32 + delta).rem_euclid(n) as usize;
        let new_tf = V1_TIMEFRAMES[next];
        if new_tf == self.chart.timeframe {
            return;
        }
        self.chart.timeframe = new_tf.to_string();
        self.request_chart_load();
    }

    /// Set focused chart instrument. Returns true when interest changed (reload armed).
    pub fn set_instrument(&mut self, raw: &str) -> bool {
        if self.screen != Screen::Workspace {
            return false;
        }
        let symbol = normalize_instrument(raw);
        if symbol.is_empty() || symbol == self.chart.instrument {
            return false;
        }
        self.chart.instrument = symbol;
        self.request_chart_load();
        true
    }

    pub fn begin_instrument_prompt(&mut self) {
        if self.screen != Screen::Workspace {
            return;
        }
        self.input_mode = InputMode::InstrumentPrompt {
            buffer: String::new(),
        };
    }

    pub fn cancel_instrument_prompt(&mut self) {
        self.input_mode = InputMode::Normal;
    }

    pub fn prompt_push_char(&mut self, c: char) {
        if let InputMode::InstrumentPrompt { buffer } = &mut self.input_mode {
            // Instruments are alnum; allow `.` and `-` for future vendor symbols.
            if c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_' {
                if buffer.len() < 16 {
                    buffer.push(c.to_ascii_uppercase());
                }
            }
        }
    }

    pub fn prompt_pop_char(&mut self) {
        if let InputMode::InstrumentPrompt { buffer } = &mut self.input_mode {
            buffer.pop();
        }
    }

    /// Apply the instrument prompt buffer. Returns true when interest changed.
    pub fn apply_instrument_prompt(&mut self) -> bool {
        let InputMode::InstrumentPrompt { buffer } = &self.input_mode else {
            return false;
        };
        let raw = buffer.clone();
        self.input_mode = InputMode::Normal;
        self.set_instrument(&raw)
    }

    pub fn apply_ipc(&mut self, event: IpcEvent) {
        match event {
            IpcEvent::Snapshot(feed) => {
                self.set_feed(feed);
                // Reconnect while in workspace: reload history for the focused chart.
                if self.screen == Screen::Workspace {
                    self.request_chart_load();
                }
            }
            IpcEvent::FeedStatus {
                status,
                vendor_mode,
            } => {
                let engine = self
                    .feed
                    .as_ref()
                    .map(|f| f.engine.clone())
                    .unwrap_or_else(|| "up".into());
                self.set_feed(FeedSnapshot {
                    status,
                    vendor_mode,
                    engine,
                });
            }
            IpcEvent::Heartbeat { ts } => {
                self.last_heartbeat_ts = Some(ts);
                if matches!(self.connection, ConnectionStatus::Connecting) {
                    self.connection = ConnectionStatus::Connected;
                }
            }
            IpcEvent::ChartSeries(series) => {
                self.apply_chart_series(series);
            }
            IpcEvent::BarUpdate(update) => {
                self.apply_bar_update(update);
            }
            IpcEvent::ChartLoadFailed {
                instrument,
                timeframe,
                message,
            } => {
                self.apply_chart_load_error(&instrument, &timeframe, message);
            }
            IpcEvent::Disconnected { reason } => {
                self.connection = ConnectionStatus::Disconnected { reason };
            }
        }
    }

    pub fn apply_chart_series(&mut self, series: ChartInterestResponse) {
        // Only apply if it matches the focused chart interest.
        if series.instrument != self.chart.instrument || series.timeframe != self.chart.timeframe {
            return;
        }
        match series.status.as_str() {
            "ok" => {
                self.chart.series = ChartSeriesState::Available { bars: series.bars };
            }
            "unavailable" => {
                self.chart.series = ChartSeriesState::Unavailable;
            }
            other => {
                self.chart.series = ChartSeriesState::Error {
                    message: format!("unexpected chart status: {other}"),
                };
            }
        }
    }

    pub fn apply_chart_load_error(
        &mut self,
        instrument: &str,
        timeframe: &str,
        message: String,
    ) {
        // Ignore stale failures from a prior instrument/timeframe selection.
        if instrument != self.chart.instrument || timeframe != self.chart.timeframe {
            return;
        }
        self.chart.series = ChartSeriesState::Error { message };
    }

    /// Apply a conflated live bar tip (and any completed bars from a period roll).
    pub fn apply_bar_update(&mut self, update: BarUpdateEvent) {
        if update.instrument != self.chart.instrument || update.timeframe != self.chart.timeframe {
            return;
        }
        let ChartSeriesState::Available { bars } = &mut self.chart.series else {
            return;
        };
        for completed in update.completed_bars {
            merge_bar(bars, completed);
        }
        merge_bar(bars, update.bar);
    }

    fn set_feed(&mut self, feed: FeedSnapshot) {
        let connected = feed.status == "connected" && feed.engine == "up";
        self.connection = if connected {
            ConnectionStatus::Connected
        } else {
            ConnectionStatus::Disconnected {
                reason: format!("feed status={}", feed.status),
            }
        };
        self.feed = Some(feed);
    }

    pub fn vendor_mode_label(&self) -> &str {
        self.feed
            .as_ref()
            .map(|f| f.vendor_mode.as_str())
            .unwrap_or("—")
    }

    pub fn empty_state_copy(&self) -> Option<&'static str> {
        match self.chart.series {
            ChartSeriesState::Unavailable => Some(UNAVAILABLE_COPY),
            _ => None,
        }
    }
}

/// Canonical instrument id: trim + uppercase ASCII (e.g. `qqq` → `QQQ`).
fn normalize_instrument(raw: &str) -> String {
    raw.trim().to_ascii_uppercase()
}

/// Replace last bar when timestamps match; append when the tip advances.
fn merge_bar(bars: &mut Vec<OhlcvBar>, bar: OhlcvBar) {
    match bars.last_mut() {
        Some(last) if last.ts == bar.ts => {
            *last = bar;
        }
        Some(last) if bar.ts > last.ts => {
            bars.push(bar);
        }
        Some(_) => {
            // Stale / out-of-order tip — ignore.
        }
        None => {
            bars.push(bar);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn welcome_enters_workspace_with_default_spy_1d() {
        let mut app = App::default();
        assert_eq!(app.screen, Screen::Welcome);
        assert_eq!(app.layout, LayoutMode::Single);
        assert_eq!(app.chart.instrument, "SPY");
        assert_eq!(app.chart.timeframe, "1D");
        app.enter_workspace();
        assert_eq!(app.screen, Screen::Workspace);
        assert!(app.needs_chart_load);
        assert_eq!(app.chart.series, ChartSeriesState::Loading);
    }

    #[test]
    fn snapshot_marks_connected_feed_with_fake_vendor() {
        let mut app = App::default();
        app.apply_ipc(IpcEvent::Snapshot(FeedSnapshot {
            status: "connected".into(),
            vendor_mode: "fake".into(),
            engine: "up".into(),
        }));
        assert_eq!(app.connection, ConnectionStatus::Connected);
        assert_eq!(app.vendor_mode_label(), "fake");
    }

    #[test]
    fn disconnect_event_marks_disconnected() {
        let mut app = App::default();
        app.apply_ipc(IpcEvent::Disconnected {
            reason: "engine down".into(),
        });
        assert!(matches!(
            app.connection,
            ConnectionStatus::Disconnected { .. }
        ));
        assert_eq!(app.connection.label(), "disconnected");
    }

    #[test]
    fn available_series_stores_bars() {
        let mut app = App::default();
        app.enter_workspace();
        app.apply_chart_series(ChartInterestResponse {
            instrument: "SPY".into(),
            timeframe: "1D".into(),
            status: "ok".into(),
            bars: vec![OhlcvBar {
                ts: 1_719_792_000,
                open: 540.0,
                high: 541.0,
                low: 539.0,
                close: 540.5,
                volume: 1_000_000.0,
            }],
        });
        match &app.chart.series {
            ChartSeriesState::Available { bars } => {
                assert_eq!(bars.len(), 1);
                assert_eq!(bars[0].close, 540.5);
            }
            other => panic!("expected Available, got {other:?}"),
        }
        assert!(app.empty_state_copy().is_none());
    }

    #[test]
    fn unavailable_series_uses_exact_empty_state_copy() {
        let mut app = App::default();
        app.enter_workspace();
        app.apply_chart_series(ChartInterestResponse {
            instrument: "SPY".into(),
            timeframe: "1D".into(),
            status: "unavailable".into(),
            bars: vec![],
        });
        assert_eq!(app.chart.series, ChartSeriesState::Unavailable);
        assert_eq!(app.empty_state_copy(), Some("Data Currently not Available"));
    }

    #[test]
    fn ignores_series_for_other_instrument() {
        let mut app = App::default();
        app.enter_workspace();
        app.apply_chart_series(ChartInterestResponse {
            instrument: "QQQ".into(),
            timeframe: "1D".into(),
            status: "ok".into(),
            bars: vec![OhlcvBar {
                ts: 1,
                open: 1.0,
                high: 1.0,
                low: 1.0,
                close: 1.0,
                volume: 1.0,
            }],
        });
        assert_eq!(app.chart.series, ChartSeriesState::Loading);
    }

    #[test]
    fn bar_update_mutates_last_bar_close() {
        let mut app = App::default();
        app.enter_workspace();
        app.apply_chart_series(ChartInterestResponse {
            instrument: "SPY".into(),
            timeframe: "1D".into(),
            status: "ok".into(),
            bars: vec![OhlcvBar {
                ts: 1_720_569_600,
                open: 546.25,
                high: 548.5,
                low: 545.75,
                close: 548.0,
                volume: 50_900_000.0,
            }],
        });
        app.apply_bar_update(BarUpdateEvent {
            instrument: "SPY".into(),
            timeframe: "1D".into(),
            completed_bars: vec![],
            bar: OhlcvBar {
                ts: 1_720_569_600,
                open: 546.25,
                high: 549.25,
                low: 545.75,
                close: 549.25,
                volume: 50_910_000.0,
            },
        });
        match &app.chart.series {
            ChartSeriesState::Available { bars } => {
                assert_eq!(bars.len(), 1);
                assert_eq!(bars[0].close, 549.25);
                assert_eq!(bars[0].high, 549.25);
                assert_eq!(bars[0].volume, 50_910_000.0);
            }
            other => panic!("expected Available, got {other:?}"),
        }
    }

    #[test]
    fn bar_update_period_roll_appends_new_bar() {
        let mut app = App::default();
        app.enter_workspace();
        app.apply_chart_series(ChartInterestResponse {
            instrument: "SPY".into(),
            timeframe: "1D".into(),
            status: "ok".into(),
            bars: vec![OhlcvBar {
                ts: 1_720_569_600,
                open: 546.25,
                high: 548.5,
                low: 545.75,
                close: 548.0,
                volume: 50_900_000.0,
            }],
        });
        app.apply_bar_update(BarUpdateEvent {
            instrument: "SPY".into(),
            timeframe: "1D".into(),
            completed_bars: vec![OhlcvBar {
                ts: 1_720_569_600,
                open: 546.25,
                high: 548.5,
                low: 545.75,
                close: 548.0,
                volume: 50_900_000.0,
            }],
            bar: OhlcvBar {
                ts: 1_720_569_600 + 86_400,
                open: 550.0,
                high: 550.0,
                low: 550.0,
                close: 550.0,
                volume: 5_000.0,
            },
        });
        match &app.chart.series {
            ChartSeriesState::Available { bars } => {
                assert_eq!(bars.len(), 2);
                assert_eq!(bars[0].close, 548.0);
                assert_eq!(bars[1].ts, 1_720_569_600 + 86_400);
                assert_eq!(bars[1].close, 550.0);
            }
            other => panic!("expected Available, got {other:?}"),
        }
    }

    #[test]
    fn bar_update_ignores_other_instrument() {
        let mut app = App::default();
        app.enter_workspace();
        app.apply_chart_series(ChartInterestResponse {
            instrument: "SPY".into(),
            timeframe: "1D".into(),
            status: "ok".into(),
            bars: vec![OhlcvBar {
                ts: 1,
                open: 1.0,
                high: 1.0,
                low: 1.0,
                close: 1.0,
                volume: 1.0,
            }],
        });
        app.apply_bar_update(BarUpdateEvent {
            instrument: "QQQ".into(),
            timeframe: "1D".into(),
            completed_bars: vec![],
            bar: OhlcvBar {
                ts: 1,
                open: 9.0,
                high: 9.0,
                low: 9.0,
                close: 9.0,
                volume: 9.0,
            },
        });
        match &app.chart.series {
            ChartSeriesState::Available { bars } => assert_eq!(bars[0].close, 1.0),
            other => panic!("expected Available, got {other:?}"),
        }
    }

    #[test]
    fn v1_timeframes_are_exactly_the_product_set() {
        assert_eq!(
            V1_TIMEFRAMES,
            ["1m", "3m", "5m", "15m", "30m", "1h", "4h", "1D", "1W"]
        );
    }

    #[test]
    fn cycle_timeframe_next_from_1d_goes_to_1w_then_wraps_to_1m() {
        let mut app = App::default();
        app.enter_workspace();
        app.chart_load_started();
        app.apply_chart_series(ChartInterestResponse {
            instrument: "SPY".into(),
            timeframe: "1D".into(),
            status: "ok".into(),
            bars: vec![],
        });
        assert_eq!(app.chart.timeframe, "1D");

        app.cycle_timeframe(1);
        assert_eq!(app.chart.timeframe, "1W");
        assert!(app.needs_chart_load);
        assert_eq!(app.chart.series, ChartSeriesState::Loading);

        app.chart_load_started();
        app.cycle_timeframe(1);
        assert_eq!(app.chart.timeframe, "1m");
    }

    #[test]
    fn cycle_timeframe_prev_from_1d_goes_to_4h() {
        let mut app = App::default();
        app.enter_workspace();
        app.chart_load_started();

        app.cycle_timeframe(-1);
        assert_eq!(app.chart.timeframe, "4h");
        assert!(app.needs_chart_load);
    }

    #[test]
    fn set_instrument_reloads_history_and_normalizes_symbol() {
        let mut app = App::default();
        app.enter_workspace();
        app.chart_load_started();
        app.apply_chart_series(ChartInterestResponse {
            instrument: "SPY".into(),
            timeframe: "1D".into(),
            status: "ok".into(),
            bars: vec![OhlcvBar {
                ts: 1,
                open: 1.0,
                high: 1.0,
                low: 1.0,
                close: 1.0,
                volume: 1.0,
            }],
        });

        assert!(app.set_instrument("qqq"));
        assert_eq!(app.chart.instrument, "QQQ");
        assert_eq!(app.chart.timeframe, "1D");
        assert!(app.needs_chart_load);
        assert_eq!(app.chart.series, ChartSeriesState::Loading);
    }

    #[test]
    fn set_instrument_same_symbol_is_noop() {
        let mut app = App::default();
        app.enter_workspace();
        app.chart_load_started();
        assert!(!app.set_instrument("SPY"));
        assert!(!app.needs_chart_load);
    }

    #[test]
    fn set_instrument_rejects_empty() {
        let mut app = App::default();
        app.enter_workspace();
        app.chart_load_started();
        assert!(!app.set_instrument("   "));
        assert_eq!(app.chart.instrument, "SPY");
        assert!(!app.needs_chart_load);
    }

    #[test]
    fn instrument_prompt_apply_changes_focused_chart() {
        let mut app = App::default();
        app.enter_workspace();
        app.chart_load_started();

        app.begin_instrument_prompt();
        assert!(matches!(app.input_mode, InputMode::InstrumentPrompt { .. }));
        app.prompt_push_char('e');
        app.prompt_push_char('s');
        assert!(app.apply_instrument_prompt());
        assert_eq!(app.input_mode, InputMode::Normal);
        assert_eq!(app.chart.instrument, "ES");
        assert!(app.needs_chart_load);
    }

    #[test]
    fn instrument_prompt_cancel_restores_normal_mode() {
        let mut app = App::default();
        app.enter_workspace();
        app.chart_load_started();
        app.begin_instrument_prompt();
        app.prompt_push_char('x');
        app.cancel_instrument_prompt();
        assert_eq!(app.input_mode, InputMode::Normal);
        assert_eq!(app.chart.instrument, "SPY");
        assert!(!app.needs_chart_load);
    }

    #[test]
    fn cycle_timeframe_ignored_during_instrument_prompt() {
        let mut app = App::default();
        app.enter_workspace();
        app.chart_load_started();
        app.begin_instrument_prompt();
        app.cycle_timeframe(1);
        assert_eq!(app.chart.timeframe, "1D");
        assert!(!app.needs_chart_load);
    }

    #[test]
    fn chart_load_error_for_stale_interest_is_ignored() {
        let mut app = App::default();
        app.enter_workspace();
        app.set_instrument("QQQ");
        app.chart_load_started();
        app.apply_chart_series(ChartInterestResponse {
            instrument: "QQQ".into(),
            timeframe: "1D".into(),
            status: "ok".into(),
            bars: vec![OhlcvBar {
                ts: 1,
                open: 1.0,
                high: 1.0,
                low: 1.0,
                close: 1.0,
                volume: 1.0,
            }],
        });
        // Late failure from previous SPY interest must not clobber QQQ.
        app.apply_chart_load_error("SPY", "1D", "timeout".into());
        match &app.chart.series {
            ChartSeriesState::Available { bars } => assert_eq!(bars[0].close, 1.0),
            other => panic!("expected Available, got {other:?}"),
        }
    }
}
