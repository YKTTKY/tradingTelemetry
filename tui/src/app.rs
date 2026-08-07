//! Application state: Welcome → empty workspace shell with feed status.

use crate::ipc::{FeedSnapshot, IpcEvent};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Welcome,
    Workspace,
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

#[derive(Debug, Clone)]
pub struct App {
    pub screen: Screen,
    pub connection: ConnectionStatus,
    pub feed: Option<FeedSnapshot>,
    pub last_heartbeat_ts: Option<f64>,
    pub should_quit: bool,
}

impl Default for App {
    fn default() -> Self {
        Self {
            screen: Screen::Welcome,
            connection: ConnectionStatus::Connecting,
            feed: None,
            last_heartbeat_ts: None,
            should_quit: false,
        }
    }
}

impl App {
    pub fn enter_workspace(&mut self) {
        if self.screen == Screen::Welcome {
            self.screen = Screen::Workspace;
        }
    }

    pub fn quit(&mut self) {
        self.should_quit = true;
    }

    pub fn apply_ipc(&mut self, event: IpcEvent) {
        match event {
            IpcEvent::Snapshot(feed) => {
                self.set_feed(feed);
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
            IpcEvent::Disconnected { reason } => {
                self.connection = ConnectionStatus::Disconnected { reason };
            }
        }
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn welcome_enters_workspace() {
        let mut app = App::default();
        assert_eq!(app.screen, Screen::Welcome);
        app.enter_workspace();
        assert_eq!(app.screen, Screen::Workspace);
    }

    #[test]
    fn snapshot_marks_connected_feed() {
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
}
