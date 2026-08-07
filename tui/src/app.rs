//! Application state: Welcome → single-layout workspace with default chart.

use crate::ipc::{ChartInterestResponse, FeedSnapshot, IpcEvent, OhlcvBar};

/// Exact empty-state copy when the vendor cannot serve the chart series.
pub const UNAVAILABLE_COPY: &str = "Data Currently not Available";

pub const DEFAULT_INSTRUMENT: &str = "SPY";
pub const DEFAULT_TIMEFRAME: &str = "1D";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Welcome,
    Workspace,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayoutMode {
    Single,
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
            IpcEvent::ChartLoadFailed { message } => {
                self.apply_chart_load_error(message);
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

    pub fn apply_chart_load_error(&mut self, message: String) {
        self.chart.series = ChartSeriesState::Error { message };
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
}
