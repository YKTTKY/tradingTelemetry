//! HTTP snapshot + WebSocket client against the market engine IPC.

use std::time::Duration;

use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::sync::mpsc;
use tokio_tungstenite::connect_async;
use url::Url;

/// Engine base URL, e.g. `http://127.0.0.1:8765`.
#[derive(Debug, Clone)]
pub struct EngineEndpoint {
    pub base: Url,
}

impl EngineEndpoint {
    pub fn parse(s: &str) -> Result<Self, url::ParseError> {
        Ok(Self {
            base: Url::parse(s)?,
        })
    }

    pub fn snapshot_url(&self) -> Url {
        self.base
            .join("/v1/snapshot")
            .expect("snapshot path joins base")
    }

    pub fn chart_interest_url(&self) -> Url {
        self.base
            .join("/v1/chart/interest")
            .expect("chart interest path joins base")
    }

    pub fn ws_url(&self) -> Url {
        let mut ws = self.base.clone();
        let scheme = match ws.scheme() {
            "https" => "wss",
            _ => "ws",
        };
        ws.set_scheme(scheme)
            .expect("scheme is valid for this URL");
        ws.join("/v1/ws").expect("ws path joins base")
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct FeedSnapshot {
    pub status: String,
    pub vendor_mode: String,
    pub engine: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
struct SnapshotBody {
    feed: FeedSnapshot,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ChartInterestRequest {
    pub instrument: String,
    pub timeframe: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct OhlcvBar {
    pub ts: i64,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: f64,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct ChartInterestResponse {
    pub instrument: String,
    pub timeframe: String,
    pub status: String,
    pub bars: Vec<OhlcvBar>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum IpcEvent {
    Snapshot(FeedSnapshot),
    FeedStatus {
        status: String,
        vendor_mode: String,
    },
    Heartbeat {
        ts: f64,
    },
    ChartSeries(ChartInterestResponse),
    ChartLoadFailed {
        message: String,
    },
    Disconnected {
        reason: String,
    },
}

#[derive(Debug, Error)]
pub enum IpcError {
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("WebSocket error: {0}")]
    Ws(#[from] tokio_tungstenite::tungstenite::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("URL error: {0}")]
    Url(#[from] url::ParseError),
}

pub async fn fetch_snapshot(endpoint: &EngineEndpoint) -> Result<FeedSnapshot, IpcError> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(2))
        .build()?;
    let body: SnapshotBody = client
        .get(endpoint.snapshot_url())
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    Ok(body.feed)
}

pub async fn post_chart_interest(
    endpoint: &EngineEndpoint,
    instrument: &str,
    timeframe: &str,
) -> Result<ChartInterestResponse, IpcError> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()?;
    let body = ChartInterestRequest {
        instrument: instrument.to_string(),
        timeframe: timeframe.to_string(),
    };
    let response: ChartInterestResponse = client
        .post(endpoint.chart_interest_url())
        .json(&body)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    Ok(response)
}

/// Background IPC: snapshot + WS with reconnect so the TUI recovers when the engine returns.
pub async fn run_ipc_loop(endpoint: EngineEndpoint, tx: mpsc::UnboundedSender<IpcEvent>) {
    loop {
        if tx.is_closed() {
            return;
        }
        if let Err(reason) = connect_session(&endpoint, &tx).await {
            let _ = tx.send(IpcEvent::Disconnected { reason });
            tokio::time::sleep(Duration::from_secs(1)).await;
        } else {
            // Clean server close — still retry so engine restarts are visible.
            let _ = tx.send(IpcEvent::Disconnected {
                reason: "server closed".into(),
            });
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    }
}

async fn connect_session(
    endpoint: &EngineEndpoint,
    tx: &mpsc::UnboundedSender<IpcEvent>,
) -> Result<(), String> {
    let feed = fetch_snapshot(endpoint)
        .await
        .map_err(|e| e.to_string())?;
    if tx.send(IpcEvent::Snapshot(feed)).is_err() {
        return Ok(());
    }

    let ws_url = endpoint.ws_url().to_string();
    let (ws, _) = connect_async(&ws_url).await.map_err(|e| e.to_string())?;
    let (_, mut read) = ws.split();

    while let Some(msg) = read.next().await {
        match msg {
            Ok(m) if m.is_text() => {
                let text = m.to_text().map_err(|e| e.to_string())?;
                if let Ok(value) = serde_json::from_str::<serde_json::Value>(text) {
                    match value.get("type").and_then(|t| t.as_str()) {
                        Some("feed_status") => {
                            let status = value
                                .get("status")
                                .and_then(|v| v.as_str())
                                .unwrap_or("unknown")
                                .to_string();
                            let vendor_mode = value
                                .get("vendor_mode")
                                .and_then(|v| v.as_str())
                                .unwrap_or("unknown")
                                .to_string();
                            if tx
                                .send(IpcEvent::FeedStatus {
                                    status,
                                    vendor_mode,
                                })
                                .is_err()
                            {
                                return Ok(());
                            }
                        }
                        Some("heartbeat") => {
                            let ts = value.get("ts").and_then(|v| v.as_f64()).unwrap_or(0.0);
                            if tx.send(IpcEvent::Heartbeat { ts }).is_err() {
                                return Ok(());
                            }
                        }
                        _ => {}
                    }
                }
            }
            Ok(m) if m.is_close() => return Ok(()),
            Err(err) => return Err(err.to_string()),
            _ => {}
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ws_url_rewrites_http_scheme() {
        let ep = EngineEndpoint::parse("http://127.0.0.1:8765").unwrap();
        assert_eq!(ep.ws_url().as_str(), "ws://127.0.0.1:8765/v1/ws");
        assert_eq!(
            ep.snapshot_url().as_str(),
            "http://127.0.0.1:8765/v1/snapshot"
        );
        assert_eq!(
            ep.chart_interest_url().as_str(),
            "http://127.0.0.1:8765/v1/chart/interest"
        );
    }
}
