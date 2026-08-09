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

    pub fn workspace_url(&self) -> Url {
        self.base
            .join("/v1/workspace")
            .expect("workspace path joins base")
    }

    pub fn watchlist_active_url(&self) -> Url {
        self.base
            .join("/v1/watchlist/active")
            .expect("watchlist active path joins base")
    }

    pub fn watchlist_add_url(&self) -> Url {
        self.base
            .join("/v1/watchlist/add")
            .expect("watchlist add path joins base")
    }

    pub fn watchlist_remove_url(&self) -> Url {
        self.base
            .join("/v1/watchlist/remove")
            .expect("watchlist remove path joins base")
    }

    pub fn indicators_url(&self) -> Url {
        self.base
            .join("/v1/indicators")
            .expect("indicators path joins base")
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

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct LevelStyle {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default)]
    pub opacity: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct HistogramStyle {
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default)]
    pub opacity: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct IndicatorConfig {
    pub id: String,
    #[serde(rename = "type")]
    pub indicator_type: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub ma_type: Option<String>,
    #[serde(default)]
    pub length: Option<i64>,
    // Session Volume Profile
    #[serde(default)]
    pub mode: Option<String>,
    #[serde(default)]
    pub box_width: Option<f64>,
    #[serde(default)]
    pub placement: Option<String>,
    #[serde(default)]
    pub rows: Option<i64>,
    #[serde(default)]
    pub value_area_volume: Option<f64>,
    #[serde(default)]
    pub histogram: Option<HistogramStyle>,
    #[serde(default)]
    pub poc: Option<LevelStyle>,
    #[serde(default)]
    pub vah: Option<LevelStyle>,
    #[serde(default)]
    pub val: Option<LevelStyle>,
}

fn default_true() -> bool {
    true
}

impl IndicatorConfig {
    pub fn ma(id: impl Into<String>, ma_type: &str, length: i64) -> Self {
        Self {
            id: id.into(),
            indicator_type: "ma".into(),
            enabled: true,
            ma_type: Some(ma_type.into()),
            length: Some(length),
            mode: None,
            box_width: None,
            placement: None,
            rows: None,
            value_area_volume: None,
            histogram: None,
            poc: None,
            vah: None,
            val: None,
        }
    }

    pub fn volume(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            indicator_type: "volume".into(),
            enabled: true,
            ma_type: None,
            length: None,
            mode: None,
            box_width: None,
            placement: None,
            rows: None,
            value_area_volume: None,
            histogram: None,
            poc: None,
            vah: None,
            val: None,
        }
    }

    pub fn session_vp_default(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            indicator_type: "session_vp".into(),
            enabled: true,
            ma_type: None,
            length: None,
            mode: Some("all".into()),
            box_width: Some(30.0),
            placement: Some("right".into()),
            rows: Some(500),
            value_area_volume: Some(70.0),
            histogram: Some(HistogramStyle {
                color: Some("steelblue".into()),
                opacity: Some(0.35),
            }),
            poc: Some(LevelStyle {
                enabled: true,
                color: Some("yellow".into()),
                opacity: Some(1.0),
            }),
            vah: Some(LevelStyle {
                enabled: true,
                color: Some("lime".into()),
                opacity: Some(1.0),
            }),
            val: Some(LevelStyle {
                enabled: true,
                color: Some("red".into()),
                opacity: Some(1.0),
            }),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct VpBin {
    pub price_low: f64,
    pub price_high: f64,
    pub volume: f64,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct SessionVpProfile {
    pub session_start: i64,
    pub session_end: i64,
    #[serde(default)]
    pub high: f64,
    #[serde(default)]
    pub low: f64,
    pub poc: f64,
    pub vah: f64,
    pub val: f64,
    #[serde(default)]
    pub total_volume: f64,
    #[serde(default)]
    pub bins: Vec<VpBin>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct IndicatorSeriesData {
    #[serde(rename = "type")]
    pub series_type: String,
    /// Per-bar values for MA / Volume (empty for profile overlays).
    #[serde(default)]
    pub values: Vec<Option<f64>>,
    #[serde(default)]
    pub ma_type: Option<String>,
    #[serde(default)]
    pub length: Option<i64>,
    /// Session / range volume profiles (type = session_vp, …).
    #[serde(default)]
    pub profiles: Vec<SessionVpProfile>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct ChartIndicatorsPayload {
    #[serde(default)]
    pub indicators: Vec<IndicatorConfig>,
    #[serde(default)]
    pub series: std::collections::HashMap<String, IndicatorSeriesData>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct IndicatorsApplyRequest {
    pub chart_id: String,
    pub indicators: Vec<IndicatorConfig>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct IndicatorsApplyResponse {
    pub chart_id: String,
    #[serde(default)]
    pub indicators: Vec<IndicatorConfig>,
    #[serde(default)]
    pub series: std::collections::HashMap<String, IndicatorSeriesData>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct IndicatorUpdateEvent {
    pub chart_id: String,
    pub instrument: String,
    pub timeframe: String,
    #[serde(default)]
    pub indicators: Vec<IndicatorConfig>,
    #[serde(default)]
    pub series: std::collections::HashMap<String, IndicatorSeriesData>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct WorkspaceChartSnapshot {
    pub id: String,
    pub instrument: String,
    pub timeframe: String,
    #[serde(default)]
    pub indicators: Vec<IndicatorConfig>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct WatchlistSnapshot {
    pub id: String,
    pub name: String,
    pub symbols: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct WorkspaceSnapshot {
    pub layout_mode: String,
    pub charts: Vec<WorkspaceChartSnapshot>,
    #[serde(default)]
    pub watchlists: Vec<WatchlistSnapshot>,
    #[serde(default)]
    pub active_watchlist_id: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct QuoteRow {
    pub symbol: String,
    pub status: String,
    #[serde(default)]
    pub last: Option<f64>,
    #[serde(default)]
    pub previous_close: Option<f64>,
    #[serde(default)]
    pub change: Option<f64>,
    #[serde(default)]
    pub change_pct: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
struct SnapshotBody {
    feed: FeedSnapshot,
    #[serde(default)]
    workspace: Option<WorkspaceSnapshot>,
    #[serde(default)]
    quotes: Vec<QuoteRow>,
    #[serde(default)]
    indicators: std::collections::HashMap<String, ChartIndicatorsPayload>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ChartInterestRequest {
    pub instrument: String,
    pub timeframe: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chart_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct WorkspaceRequest {
    pub layout_mode: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct WatchlistActiveRequest {
    pub watchlist_id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct WatchlistSymbolRequest {
    pub symbol: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct WatchlistMutationResponse {
    pub workspace: WorkspaceSnapshot,
    #[serde(default)]
    pub quotes: Vec<QuoteRow>,
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
    #[serde(default)]
    pub chart_id: Option<String>,
    #[serde(default)]
    pub indicators: Vec<IndicatorConfig>,
    #[serde(default)]
    pub series: std::collections::HashMap<String, IndicatorSeriesData>,
}

/// Live bar tip update from the engine (conflated WebSocket event).
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct BarUpdateEvent {
    pub instrument: String,
    pub timeframe: String,
    #[serde(default)]
    pub completed_bars: Vec<OhlcvBar>,
    pub bar: OhlcvBar,
}

/// Live quote update from the engine (conflated WebSocket event).
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct QuoteUpdateEvent {
    pub symbol: String,
    pub status: String,
    #[serde(default)]
    pub last: Option<f64>,
    #[serde(default)]
    pub previous_close: Option<f64>,
    #[serde(default)]
    pub change: Option<f64>,
    #[serde(default)]
    pub change_pct: Option<f64>,
}

impl QuoteUpdateEvent {
    pub fn to_row(&self) -> QuoteRow {
        QuoteRow {
            symbol: self.symbol.clone(),
            status: self.status.clone(),
            last: self.last,
            previous_close: self.previous_close,
            change: self.change,
            change_pct: self.change_pct,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum IpcEvent {
    Snapshot {
        feed: FeedSnapshot,
        workspace: Option<WorkspaceSnapshot>,
        quotes: Vec<QuoteRow>,
        indicators: std::collections::HashMap<String, ChartIndicatorsPayload>,
    },
    FeedStatus {
        status: String,
        vendor_mode: String,
    },
    Heartbeat {
        ts: f64,
    },
    ChartSeries(ChartInterestResponse),
    BarUpdate(BarUpdateEvent),
    QuoteUpdate(QuoteUpdateEvent),
    IndicatorUpdate(IndicatorUpdateEvent),
    IndicatorsApplied(IndicatorsApplyResponse),
    Workspace(WorkspaceSnapshot),
    WatchlistState {
        workspace: WorkspaceSnapshot,
        quotes: Vec<QuoteRow>,
    },
    ChartLoadFailed {
        chart_id: String,
        instrument: String,
        timeframe: String,
        message: String,
    },
    WorkspaceFailed {
        message: String,
    },
    WatchlistFailed {
        message: String,
    },
    IndicatorsFailed {
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

pub async fn fetch_snapshot(
    endpoint: &EngineEndpoint,
) -> Result<
    (
        FeedSnapshot,
        Option<WorkspaceSnapshot>,
        Vec<QuoteRow>,
        std::collections::HashMap<String, ChartIndicatorsPayload>,
    ),
    IpcError,
> {
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
    Ok((body.feed, body.workspace, body.quotes, body.indicators))
}

pub async fn post_indicators(
    endpoint: &EngineEndpoint,
    chart_id: &str,
    indicators: &[IndicatorConfig],
) -> Result<IndicatorsApplyResponse, IpcError> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()?;
    let body = IndicatorsApplyRequest {
        chart_id: chart_id.to_string(),
        indicators: indicators.to_vec(),
    };
    let response: IndicatorsApplyResponse = client
        .post(endpoint.indicators_url())
        .json(&body)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    Ok(response)
}

pub async fn post_chart_interest(
    endpoint: &EngineEndpoint,
    instrument: &str,
    timeframe: &str,
    chart_id: &str,
) -> Result<ChartInterestResponse, IpcError> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()?;
    let body = ChartInterestRequest {
        instrument: instrument.to_string(),
        timeframe: timeframe.to_string(),
        chart_id: Some(chart_id.to_string()),
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

pub async fn post_workspace_layout(
    endpoint: &EngineEndpoint,
    layout_mode: &str,
) -> Result<WorkspaceSnapshot, IpcError> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()?;
    let body = WorkspaceRequest {
        layout_mode: layout_mode.to_string(),
    };
    let response: WorkspaceSnapshot = client
        .post(endpoint.workspace_url())
        .json(&body)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    Ok(response)
}

pub async fn post_watchlist_active(
    endpoint: &EngineEndpoint,
    watchlist_id: &str,
) -> Result<WatchlistMutationResponse, IpcError> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()?;
    let body = WatchlistActiveRequest {
        watchlist_id: watchlist_id.to_string(),
    };
    let response: WatchlistMutationResponse = client
        .post(endpoint.watchlist_active_url())
        .json(&body)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    Ok(response)
}

pub async fn post_watchlist_add(
    endpoint: &EngineEndpoint,
    symbol: &str,
) -> Result<WatchlistMutationResponse, IpcError> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()?;
    let body = WatchlistSymbolRequest {
        symbol: symbol.to_string(),
    };
    let response: WatchlistMutationResponse = client
        .post(endpoint.watchlist_add_url())
        .json(&body)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    Ok(response)
}

pub async fn post_watchlist_remove(
    endpoint: &EngineEndpoint,
    symbol: &str,
) -> Result<WatchlistMutationResponse, IpcError> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()?;
    let body = WatchlistSymbolRequest {
        symbol: symbol.to_string(),
    };
    let response: WatchlistMutationResponse = client
        .post(endpoint.watchlist_remove_url())
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
    let (feed, workspace, quotes, indicators) = fetch_snapshot(endpoint)
        .await
        .map_err(|e| e.to_string())?;
    if tx
        .send(IpcEvent::Snapshot {
            feed,
            workspace,
            quotes,
            indicators,
        })
        .is_err()
    {
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
                        Some("bar_update") => {
                            match serde_json::from_value::<BarUpdateEvent>(value) {
                                Ok(update) => {
                                    if tx.send(IpcEvent::BarUpdate(update)).is_err() {
                                        return Ok(());
                                    }
                                }
                                Err(_) => {
                                    // Ignore malformed bar_update frames.
                                }
                            }
                        }
                        Some("quote_update") => {
                            match serde_json::from_value::<QuoteUpdateEvent>(value) {
                                Ok(update) => {
                                    if tx.send(IpcEvent::QuoteUpdate(update)).is_err() {
                                        return Ok(());
                                    }
                                }
                                Err(_) => {
                                    // Ignore malformed quote_update frames.
                                }
                            }
                        }
                        Some("indicator_update") => {
                            match serde_json::from_value::<IndicatorUpdateEvent>(value) {
                                Ok(update) => {
                                    if tx.send(IpcEvent::IndicatorUpdate(update)).is_err() {
                                        return Ok(());
                                    }
                                }
                                Err(_) => {
                                    // Ignore malformed indicator_update frames.
                                }
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
        assert_eq!(
            ep.workspace_url().as_str(),
            "http://127.0.0.1:8765/v1/workspace"
        );
        assert_eq!(
            ep.watchlist_add_url().as_str(),
            "http://127.0.0.1:8765/v1/watchlist/add"
        );
    }
}
