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

    pub fn chart_type_styles_url(&self) -> Url {
        self.base
            .join("/v1/chart/type-styles")
            .expect("chart type-styles path joins base")
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

    pub fn watchlist_rename_url(&self) -> Url {
        self.base
            .join("/v1/watchlist/rename")
            .expect("watchlist rename path joins base")
    }

    pub fn indicators_url(&self) -> Url {
        self.base
            .join("/v1/indicators")
            .expect("indicators path joins base")
    }

    pub fn paper_orders_url(&self) -> Url {
        self.base
            .join("/v1/paper/orders")
            .expect("paper orders path joins base")
    }

    pub fn paper_orders_modify_url(&self) -> Url {
        self.base
            .join("/v1/paper/orders/modify")
            .expect("paper orders modify path joins base")
    }

    pub fn paper_orders_cancel_url(&self) -> Url {
        self.base
            .join("/v1/paper/orders/cancel")
            .expect("paper orders cancel path joins base")
    }

    pub fn paper_positions_close_url(&self) -> Url {
        self.base
            .join("/v1/paper/positions/close")
            .expect("paper positions close path joins base")
    }

    pub fn paper_positions_bracket_url(&self) -> Url {
        self.base
            .join("/v1/paper/positions/bracket")
            .expect("paper positions bracket path joins base")
    }

    pub fn paper_trade_mark_visibility_url(&self) -> Url {
        self.base
            .join("/v1/paper/trade-marks/visibility")
            .expect("paper trade mark visibility path joins base")
    }

    pub fn ws_url(&self) -> Url {
        let mut ws = self.base.clone();
        let scheme = match ws.scheme() {
            "https" => "wss",
            _ => "ws",
        };
        ws.set_scheme(scheme).expect("scheme is valid for this URL");
        ws.join("/v1/ws").expect("ws path joins base")
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct FeedSnapshot {
    pub status: String,
    pub vendor_mode: String,
    pub engine: String,
    /// Latest vendor `tick.ts` (unix seconds). `None` until the first live tick.
    #[serde(default)]
    pub last_vendor_tick_ts: Option<f64>,
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
    // Session / Fixed Range Volume Profile
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
    // Fixed Range VP anchors (unix seconds)
    #[serde(default)]
    pub start: Option<i64>,
    #[serde(default)]
    pub end: Option<i64>,
    #[serde(default)]
    pub extend_to_right: Option<bool>,
    /// Anchored VP single time anchor (unix seconds, forward to now).
    #[serde(default)]
    pub anchor: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct GexLevel {
    pub strike: f64,
    pub gex: f64,
}

fn default_true() -> bool {
    true
}

impl IndicatorConfig {
    fn vp_level_defaults() -> (Option<LevelStyle>, Option<LevelStyle>, Option<LevelStyle>) {
        // POC blue · VAH green · VAL red (product palette; TUI maps names → RGB).
        (
            Some(LevelStyle {
                enabled: true,
                color: Some("blue".into()),
                opacity: Some(1.0),
            }),
            Some(LevelStyle {
                enabled: true,
                color: Some("lime".into()),
                opacity: Some(1.0),
            }),
            Some(LevelStyle {
                enabled: true,
                color: Some("red".into()),
                opacity: Some(1.0),
            }),
        )
    }

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
            start: None,
            end: None,
            extend_to_right: None,
            anchor: None,
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
            start: None,
            end: None,
            extend_to_right: None,
            anchor: None,
        }
    }

    pub fn session_vp_default(id: impl Into<String>) -> Self {
        let (poc, vah, val) = Self::vp_level_defaults();
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
            poc,
            vah,
            val,
            start: None,
            end: None,
            extend_to_right: None,
            anchor: None,
        }
    }

    /// Fixed Range VP defaults: rows 200, VA 70%, extend off; anchors supplied by caller.
    pub fn fixed_range_vp_default(id: impl Into<String>, start: i64, end: i64) -> Self {
        let (poc, vah, val) = Self::vp_level_defaults();
        let (start, end) = if start <= end {
            (start, end)
        } else {
            (end, start)
        };
        Self {
            id: id.into(),
            indicator_type: "fixed_range_vp".into(),
            enabled: true,
            ma_type: None,
            length: None,
            mode: None,
            box_width: Some(30.0),
            placement: Some("right".into()),
            rows: Some(200),
            value_area_volume: Some(70.0),
            histogram: Some(HistogramStyle {
                color: Some("steelblue".into()),
                opacity: Some(0.35),
            }),
            poc,
            vah,
            val,
            start: Some(start),
            end: Some(end),
            extend_to_right: Some(false),
            anchor: None,
        }
    }

    /// Anchored VP defaults: rows 500, VA 70%; single `anchor` supplied by caller.
    pub fn anchored_vp_default(id: impl Into<String>, anchor: i64) -> Self {
        let (poc, vah, val) = Self::vp_level_defaults();
        Self {
            id: id.into(),
            indicator_type: "anchored_vp".into(),
            enabled: true,
            ma_type: None,
            length: None,
            mode: None,
            box_width: Some(30.0),
            placement: Some("right".into()),
            rows: Some(500),
            value_area_volume: Some(70.0),
            histogram: Some(HistogramStyle {
                color: Some("steelblue".into()),
                opacity: Some(0.35),
            }),
            poc,
            vah,
            val,
            start: None,
            end: None,
            extend_to_right: None,
            anchor: Some(anchor),
        }
    }

    /// Optional GEX — only meaningful when engine has options data.
    pub fn gex(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            indicator_type: "gex".into(),
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
            start: None,
            end: None,
            extend_to_right: None,
            anchor: None,
        }
    }

    /// Optional GARCH — only meaningful when history supports a stable estimate.
    pub fn garch(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            indicator_type: "garch".into(),
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
            start: None,
            end: None,
            extend_to_right: None,
            anchor: None,
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
pub struct VpProfile {
    /// Session VP day bounds.
    #[serde(default)]
    pub session_start: Option<i64>,
    #[serde(default)]
    pub session_end: Option<i64>,
    /// Fixed Range / Anchored VP anchors / effective window.
    #[serde(default)]
    pub range_start: Option<i64>,
    #[serde(default)]
    pub range_end: Option<i64>,
    #[serde(default)]
    pub anchor_end: Option<i64>,
    /// Anchored VP: the single time anchor (also mirrored as range_start).
    #[serde(default)]
    pub anchor: Option<i64>,
    /// Where POC/VAH/VAL lines draw to (may project past anchor_end when extend is on).
    #[serde(default)]
    pub levels_end: Option<i64>,
    #[serde(default)]
    pub extend_to_right: Option<bool>,
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
    /// "ok" | "unavailable" for optional indicators (GEX / GARCH); absent for MA/VP.
    #[serde(default)]
    pub status: Option<String>,
    /// Machine reason when status is unavailable (e.g. options_data_missing).
    #[serde(default)]
    pub reason: Option<String>,
    /// Per-bar values for MA / Volume / GARCH (empty for profile overlays / GEX).
    #[serde(default)]
    pub values: Vec<Option<f64>>,
    #[serde(default)]
    pub ma_type: Option<String>,
    #[serde(default)]
    pub length: Option<i64>,
    /// Session / Fixed Range volume profiles.
    #[serde(default)]
    pub profiles: Vec<VpProfile>,
    /// GEX net exposure when status is ok.
    #[serde(default)]
    pub net_gex: Option<f64>,
    /// Underlying spot used for GEX when status is ok.
    #[serde(default)]
    pub spot: Option<f64>,
    /// Per-strike GEX levels when status is ok.
    #[serde(default)]
    pub levels: Vec<GexLevel>,
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

/// Per-chart presentation settings shared by all instances of an indicator type.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct IndicatorTypeStyle {
    /// Terminal overlay intensity / blend weight in \[0, 1\] (no true alpha).
    #[serde(default = "default_overlay_strength_field")]
    pub overlay_strength: f64,
}

fn default_overlay_strength_field() -> f64 {
    0.75
}

impl IndicatorTypeStyle {
    pub fn with_strength(overlay_strength: f64) -> Self {
        Self {
            overlay_strength: overlay_strength.clamp(0.0, 1.0),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct WorkspaceChartSnapshot {
    pub id: String,
    pub instrument: String,
    pub timeframe: String,
    #[serde(default)]
    pub indicators: Vec<IndicatorConfig>,
    /// Type style map keyed by indicator type (`ma`, `session_vp`, …).
    #[serde(default)]
    pub type_styles: std::collections::HashMap<String, IndicatorTypeStyle>,
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

/// One local paper account as exposed on the snapshot / paper WS events.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize, Default)]
pub struct PaperAccountSnapshot {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default = "default_usd")]
    pub currency: String,
    #[serde(default)]
    pub initial_balance: f64,
    #[serde(default)]
    pub balance: f64,
    #[serde(default)]
    pub commission_per_fill_usd: f64,
    #[serde(default)]
    pub leverage_enabled: bool,
    #[serde(default = "default_leverage_multiple")]
    pub leverage_multiple: f64,
    #[serde(default)]
    pub asset_class_restriction: Option<String>,
}

fn default_usd() -> String {
    "USD".into()
}

fn default_leverage_multiple() -> f64 {
    1.0
}

/// First-launch / create-account defaults (visible in paper account settings).
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize, Default)]
pub struct PaperDefaults {
    #[serde(default)]
    pub name: String,
    #[serde(default = "default_usd")]
    pub currency: String,
    #[serde(default)]
    pub initial_balance: f64,
    #[serde(default)]
    pub commission_per_fill_usd: f64,
    #[serde(default)]
    pub leverage_enabled: bool,
    #[serde(default = "default_leverage_multiple")]
    pub leverage_multiple: f64,
}

/// Open holding row in the Position table (distinct from the watchlist).
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize, Default)]
pub struct PaperPositionRow {
    #[serde(default)]
    pub symbol: String,
    #[serde(default)]
    pub side: String,
    #[serde(default)]
    pub qty: f64,
    #[serde(default)]
    pub avg_price: f64,
    #[serde(default)]
    pub unrealized_pnl: f64,
    #[serde(default)]
    pub take_profit: Option<f64>,
    #[serde(default)]
    pub stop_loss: Option<f64>,
}

/// Append-only filled order history row (one entry or exit leg).
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize, Default)]
pub struct FilledOrderRow {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub account_id: String,
    #[serde(default)]
    pub symbol: String,
    #[serde(default)]
    pub side: String,
    #[serde(rename = "type", default)]
    pub order_type: String,
    #[serde(default)]
    pub qty: f64,
    #[serde(default)]
    pub limit: Option<f64>,
    #[serde(default)]
    pub stop: Option<f64>,
    #[serde(default)]
    pub fill_price: f64,
    #[serde(default)]
    pub commission: f64,
    #[serde(default)]
    pub placed_ts: i64,
    #[serde(default)]
    pub filled_ts: i64,
    #[serde(default)]
    pub duration_s: i64,
    #[serde(default)]
    pub margin: Option<f64>,
    #[serde(default)]
    pub trade_mark_pair_id: String,
    #[serde(default)]
    pub trade_mark_kind: String,
}

/// One fill-leg pin on the price pane (engine visibility flags).
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize, Default)]
pub struct TradeMarkPin {
    #[serde(default)]
    pub pair_id: String,
    #[serde(default)]
    pub fill_id: String,
    #[serde(default)]
    pub instrument: String,
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub price: f64,
    #[serde(default)]
    pub filled_ts: i64,
    #[serde(default)]
    pub side: String,
    #[serde(default = "default_true")]
    pub visible: bool,
}

/// Cash/equity history row for the active paper account.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize, Default)]
pub struct BalanceHistoryRow {
    #[serde(default)]
    pub ts: i64,
    #[serde(default)]
    pub balance: f64,
}

/// Resting working order on the active paper account (lines + order side panel).
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize, Default)]
pub struct WorkingOrderSnapshot {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub account_id: String,
    #[serde(default)]
    pub instrument: String,
    #[serde(default)]
    pub side: String,
    #[serde(rename = "type", default)]
    pub order_type: String,
    #[serde(default)]
    pub qty: f64,
    #[serde(default)]
    pub limit: Option<f64>,
    #[serde(default)]
    pub stop: Option<f64>,
    #[serde(default)]
    pub placed_ts: i64,
    #[serde(default)]
    pub bracket_id: Option<String>,
    #[serde(default)]
    pub role: String,
}

/// Additive snapshot slice. Missing `paper` key deserializes to an empty desk.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize, Default)]
pub struct PaperSnapshot {
    #[serde(default)]
    pub active_account_id: String,
    #[serde(default)]
    pub accounts: Vec<PaperAccountSnapshot>,
    #[serde(default)]
    pub defaults: PaperDefaults,
    #[serde(default)]
    pub working_orders: Vec<WorkingOrderSnapshot>,
    #[serde(default)]
    pub positions: Vec<PaperPositionRow>,
    #[serde(default)]
    pub filled_order_history: Vec<FilledOrderRow>,
    #[serde(default)]
    pub balance_history: Vec<BalanceHistoryRow>,
    #[serde(default)]
    pub trade_marks: Vec<TradeMarkPin>,
}

impl PaperSnapshot {
    pub fn active_account(&self) -> Option<&PaperAccountSnapshot> {
        self.accounts
            .iter()
            .find(|a| !self.active_account_id.is_empty() && a.id == self.active_account_id)
    }
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
    #[serde(default)]
    paper: PaperSnapshot,
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
pub struct TypeStylesRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chart_id: Option<String>,
    pub type_styles: std::collections::HashMap<String, IndicatorTypeStyle>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct WatchlistActiveRequest {
    pub watchlist_id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct WatchlistSymbolRequest {
    pub symbol: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct WatchlistRenameRequest {
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PaperOrderPlaceRequest {
    pub instrument: String,
    pub side: String,
    #[serde(rename = "type")]
    pub order_type: String,
    pub qty: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub take_profit: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_loss: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PaperOrderModifyRequest {
    pub order_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub qty: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PaperOrderCancelRequest {
    pub order_id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PaperPositionCloseRequest {
    pub instrument: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PaperPositionBracketRequest {
    pub instrument: String,
    pub take_profit: f64,
    pub stop_loss: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PaperTradeMarkVisibilityRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pair_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fill_id: Option<String>,
    pub visible: bool,
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
        paper: PaperSnapshot,
    },
    FeedStatus {
        status: String,
        vendor_mode: String,
        last_vendor_tick_ts: Option<f64>,
    },
    Heartbeat {
        ts: f64,
        last_vendor_tick_ts: Option<f64>,
    },
    ChartSeries(ChartInterestResponse),
    BarUpdate(BarUpdateEvent),
    QuoteUpdate(QuoteUpdateEvent),
    IndicatorUpdate(IndicatorUpdateEvent),
    PaperUpdate(PaperSnapshot),
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
    PaperFailed {
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
    #[error("{0}")]
    Status(String),
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
        PaperSnapshot,
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
    Ok((
        body.feed,
        body.workspace,
        body.quotes,
        body.indicators,
        body.paper,
    ))
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
    let resp = client
        .post(endpoint.indicators_url())
        .json(&body)
        .send()
        .await?;
    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        // Prefer FastAPI `{"detail": "..."}` so the trader sees the real reason.
        let detail = serde_json::from_str::<serde_json::Value>(&text)
            .ok()
            .and_then(|v| {
                v.get("detail").map(|d| {
                    d.as_str()
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| d.to_string())
                })
            })
            .unwrap_or(text);
        return Err(IpcError::Status(format!("{status}: {detail}")));
    }
    let response: IndicatorsApplyResponse = resp.json().await?;
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

/// Persist per-chart indicator type styles (overlay strength) via workspace store.
pub async fn post_type_styles(
    endpoint: &EngineEndpoint,
    chart_id: &str,
    type_styles: std::collections::HashMap<String, IndicatorTypeStyle>,
) -> Result<WorkspaceSnapshot, IpcError> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()?;
    let body = TypeStylesRequest {
        chart_id: Some(chart_id.to_string()),
        type_styles,
    };
    let response: WorkspaceSnapshot = client
        .post(endpoint.chart_type_styles_url())
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

pub async fn post_watchlist_rename(
    endpoint: &EngineEndpoint,
    name: &str,
) -> Result<WatchlistMutationResponse, IpcError> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()?;
    let body = WatchlistRenameRequest {
        name: name.to_string(),
    };
    let response: WatchlistMutationResponse = client
        .post(endpoint.watchlist_rename_url())
        .json(&body)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    Ok(response)
}

async fn post_paper_json<T: serde::Serialize>(
    url: Url,
    body: &T,
) -> Result<PaperSnapshot, IpcError> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()?;
    let resp = client.post(url).json(body).send().await?;
    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        let detail = serde_json::from_str::<serde_json::Value>(&text)
            .ok()
            .and_then(|v| {
                v.get("detail").map(|d| {
                    d.as_str()
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| d.to_string())
                })
            })
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| {
                if text.is_empty() {
                    status.to_string()
                } else {
                    text
                }
            });
        return Err(IpcError::Status(detail));
    }
    Ok(resp.json().await?)
}

pub async fn post_paper_place(
    endpoint: &EngineEndpoint,
    instrument: &str,
    side: &str,
    order_type: &str,
    qty: f64,
    limit: Option<f64>,
    stop: Option<f64>,
    take_profit: Option<f64>,
    stop_loss: Option<f64>,
) -> Result<PaperSnapshot, IpcError> {
    post_paper_json(
        endpoint.paper_orders_url(),
        &PaperOrderPlaceRequest {
            instrument: instrument.to_string(),
            side: side.to_string(),
            order_type: order_type.to_string(),
            qty,
            limit,
            stop,
            take_profit,
            stop_loss,
        },
    )
    .await
}

pub async fn post_paper_modify(
    endpoint: &EngineEndpoint,
    order_id: &str,
    qty: Option<f64>,
    limit: Option<f64>,
    stop: Option<f64>,
) -> Result<PaperSnapshot, IpcError> {
    post_paper_json(
        endpoint.paper_orders_modify_url(),
        &PaperOrderModifyRequest {
            order_id: order_id.to_string(),
            qty,
            limit,
            stop,
        },
    )
    .await
}

pub async fn post_paper_cancel(
    endpoint: &EngineEndpoint,
    order_id: &str,
) -> Result<PaperSnapshot, IpcError> {
    post_paper_json(
        endpoint.paper_orders_cancel_url(),
        &PaperOrderCancelRequest {
            order_id: order_id.to_string(),
        },
    )
    .await
}

pub async fn post_paper_close(
    endpoint: &EngineEndpoint,
    instrument: &str,
) -> Result<PaperSnapshot, IpcError> {
    post_paper_json(
        endpoint.paper_positions_close_url(),
        &PaperPositionCloseRequest {
            instrument: instrument.to_string(),
        },
    )
    .await
}

pub async fn post_paper_attach_bracket(
    endpoint: &EngineEndpoint,
    instrument: &str,
    take_profit: f64,
    stop_loss: f64,
) -> Result<PaperSnapshot, IpcError> {
    post_paper_json(
        endpoint.paper_positions_bracket_url(),
        &PaperPositionBracketRequest {
            instrument: instrument.to_string(),
            take_profit,
            stop_loss,
        },
    )
    .await
}

pub async fn post_paper_trade_mark_visibility(
    endpoint: &EngineEndpoint,
    pair_id: Option<&str>,
    fill_id: Option<&str>,
    visible: bool,
) -> Result<PaperSnapshot, IpcError> {
    post_paper_json(
        endpoint.paper_trade_mark_visibility_url(),
        &PaperTradeMarkVisibilityRequest {
            pair_id: pair_id.map(str::to_string),
            fill_id: fill_id.map(str::to_string),
            visible,
        },
    )
    .await
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
    let (feed, workspace, quotes, indicators, paper) =
        fetch_snapshot(endpoint).await.map_err(|e| e.to_string())?;
    if tx
        .send(IpcEvent::Snapshot {
            feed,
            workspace,
            quotes,
            indicators,
            paper,
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
                            let last_vendor_tick_ts =
                                value.get("last_vendor_tick_ts").and_then(|v| v.as_f64());
                            if tx
                                .send(IpcEvent::FeedStatus {
                                    status,
                                    vendor_mode,
                                    last_vendor_tick_ts,
                                })
                                .is_err()
                            {
                                return Ok(());
                            }
                        }
                        Some("heartbeat") => {
                            let ts = value.get("ts").and_then(|v| v.as_f64()).unwrap_or(0.0);
                            let last_vendor_tick_ts =
                                value.get("last_vendor_tick_ts").and_then(|v| v.as_f64());
                            if tx
                                .send(IpcEvent::Heartbeat {
                                    ts,
                                    last_vendor_tick_ts,
                                })
                                .is_err()
                            {
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
                        Some("paper_update") => {
                            let parsed = value
                                .get("paper")
                                .cloned()
                                .and_then(|v| serde_json::from_value::<PaperSnapshot>(v).ok());
                            if let Some(paper) = parsed {
                                if tx.send(IpcEvent::PaperUpdate(paper)).is_err() {
                                    return Ok(());
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
    fn feed_snapshot_reads_last_vendor_tick_ts() {
        let with_tick: FeedSnapshot = serde_json::from_str(
            r#"{"status":"connected","vendor_mode":"lse","engine":"up","last_vendor_tick_ts":1719790800.0}"#,
        )
        .unwrap();
        assert_eq!(with_tick.last_vendor_tick_ts, Some(1_719_790_800.0));

        let none_yet: FeedSnapshot = serde_json::from_str(
            r#"{"status":"connected","vendor_mode":"fake","engine":"up","last_vendor_tick_ts":null}"#,
        )
        .unwrap();
        assert_eq!(none_yet.last_vendor_tick_ts, None);

        let omitted: FeedSnapshot =
            serde_json::from_str(r#"{"status":"connected","vendor_mode":"fake","engine":"up"}"#)
                .unwrap();
        assert_eq!(omitted.last_vendor_tick_ts, None);
    }

    #[test]
    fn snapshot_deserializes_with_and_without_paper_key() {
        let with_paper: SnapshotBody = serde_json::from_str(
            r#"{
                "feed": {"status":"connected","vendor_mode":"fake","engine":"up","last_vendor_tick_ts":null},
                "workspace": {"layout_mode":"single","charts":[]},
                "quotes": [],
                "indicators": {},
                "paper": {
                    "active_account_id": "pa_1",
                    "accounts": [{
                        "id": "pa_1",
                        "name": "Paper",
                        "currency": "USD",
                        "initial_balance": 100000.0,
                        "balance": 100000.0,
                        "commission_per_fill_usd": 1.0,
                        "leverage_enabled": false,
                        "leverage_multiple": 1.0
                    }],
                    "defaults": {
                        "name": "Paper",
                        "currency": "USD",
                        "initial_balance": 100000.0,
                        "commission_per_fill_usd": 1.0,
                        "leverage_enabled": false,
                        "leverage_multiple": 1.0
                    },
                    "positions": [],
                    "filled_order_history": [],
                    "balance_history": []
                }
            }"#,
        )
        .unwrap();
        assert_eq!(with_paper.paper.active_account_id, "pa_1");
        assert_eq!(with_paper.paper.accounts.len(), 1);
        assert_eq!(with_paper.paper.accounts[0].name, "Paper");
        assert_eq!(with_paper.paper.accounts[0].balance, 100_000.0);
        assert_eq!(with_paper.paper.accounts[0].commission_per_fill_usd, 1.0);
        assert!(!with_paper.paper.accounts[0].leverage_enabled);
        assert_eq!(with_paper.paper.accounts[0].leverage_multiple, 1.0);
        assert!(with_paper.paper.positions.is_empty());
        assert!(with_paper.paper.filled_order_history.is_empty());
        assert!(with_paper.paper.balance_history.is_empty());
        assert!(with_paper.paper.working_orders.is_empty());

        let without: SnapshotBody = serde_json::from_str(
            r#"{
                "feed": {"status":"connected","vendor_mode":"fake","engine":"up"},
                "workspace": {"layout_mode":"single","charts":[]},
                "quotes": [],
                "indicators": {}
            }"#,
        )
        .unwrap();
        assert!(without.paper.accounts.is_empty());
        assert!(without.paper.active_account_id.is_empty());
        assert!(without.paper.positions.is_empty());
        assert!(without.paper.working_orders.is_empty());
        assert_eq!(without.feed.last_vendor_tick_ts, None);
    }

    #[test]
    fn snapshot_deserializes_working_orders_additively() {
        let with_orders: SnapshotBody = serde_json::from_str(
            r#"{
                "feed": {"status":"connected","vendor_mode":"fake","engine":"up"},
                "paper": {
                    "active_account_id": "pa_1",
                    "accounts": [{"id":"pa_1","name":"Paper","currency":"USD","balance":100000.0}],
                    "working_orders": [{
                        "id": "wo_1",
                        "account_id": "pa_1",
                        "instrument": "SPY",
                        "side": "buy",
                        "type": "limit",
                        "qty": 10,
                        "limit": 540.0,
                        "stop": null,
                        "placed_ts": 1719792000
                    }]
                }
            }"#,
        )
        .unwrap();
        assert_eq!(with_orders.paper.working_orders.len(), 1);
        let wo = &with_orders.paper.working_orders[0];
        assert_eq!(wo.id, "wo_1");
        assert_eq!(wo.instrument, "SPY");
        assert_eq!(wo.side, "buy");
        assert_eq!(wo.order_type, "limit");
        assert_eq!(wo.qty, 10.0);
        assert_eq!(wo.limit, Some(540.0));
        assert_eq!(wo.stop, None);
        assert_eq!(wo.placed_ts, 1_719_792_000);

        let omitted_orders: SnapshotBody = serde_json::from_str(
            r#"{
                "feed": {"status":"connected","vendor_mode":"fake","engine":"up"},
                "paper": {
                    "active_account_id": "pa_1",
                    "accounts": []
                }
            }"#,
        )
        .unwrap();
        assert!(omitted_orders.paper.working_orders.is_empty());
    }

    #[test]
    fn snapshot_deserializes_positions_fills_and_balance_additively() {
        let body: SnapshotBody = serde_json::from_str(
            r#"{
                "feed": {"status":"connected","vendor_mode":"fake","engine":"up"},
                "paper": {
                    "active_account_id": "pa_1",
                    "accounts": [{"id":"pa_1","name":"Paper","currency":"USD","balance":98889.0}],
                    "positions": [{
                        "symbol": "SPY",
                        "side": "long",
                        "qty": 10,
                        "avg_price": 111.0,
                        "unrealized_pnl": 5.0
                    }],
                    "filled_order_history": [{
                        "id": "fo_1",
                        "symbol": "SPY",
                        "side": "buy",
                        "type": "market",
                        "qty": 10,
                        "fill_price": 111.0,
                        "commission": 1.0,
                        "placed_ts": 1719792000,
                        "filled_ts": 1719792065,
                        "duration_s": 65
                    }],
                    "balance_history": [{"ts": 1719792065, "balance": 98889.0}]
                }
            }"#,
        )
        .unwrap();
        assert_eq!(body.paper.positions.len(), 1);
        let pos = &body.paper.positions[0];
        assert_eq!(pos.symbol, "SPY");
        assert_eq!(pos.side, "long");
        assert_eq!(pos.qty, 10.0);
        assert_eq!(pos.avg_price, 111.0);
        assert_eq!(pos.unrealized_pnl, 5.0);
        let fill = &body.paper.filled_order_history[0];
        assert_eq!(fill.symbol, "SPY");
        assert_eq!(fill.order_type, "market");
        assert_eq!(fill.fill_price, 111.0);
        assert_eq!(fill.commission, 1.0);
        assert_eq!(fill.placed_ts, 1_719_792_000);
        assert_eq!(fill.filled_ts, 1_719_792_065);
        assert_eq!(fill.duration_s, 65);
        assert_eq!(body.paper.balance_history[0].balance, 98_889.0);
        assert_eq!(body.paper.balance_history[0].ts, 1_719_792_065);

        let omitted: SnapshotBody = serde_json::from_str(
            r#"{
                "feed": {"status":"connected","vendor_mode":"fake","engine":"up"},
                "paper": {"active_account_id": "pa_1", "accounts": []}
            }"#,
        )
        .unwrap();
        assert!(omitted.paper.positions.is_empty());
        assert!(omitted.paper.filled_order_history.is_empty());
        assert!(omitted.paper.balance_history.is_empty());
        assert!(omitted.paper.trade_marks.is_empty());
        assert_eq!(pos.take_profit, None);
        assert_eq!(pos.stop_loss, None);
    }

    #[test]
    fn snapshot_deserializes_trade_marks_additively() {
        let body: SnapshotBody = serde_json::from_str(
            r#"{
                "feed": {"status":"connected","vendor_mode":"fake","engine":"up"},
                "paper": {
                    "active_account_id": "pa_1",
                    "accounts": [{"id":"pa_1","name":"Paper","currency":"USD","balance":98889.0}],
                    "filled_order_history": [{
                        "id": "fo_1",
                        "symbol": "SPY",
                        "side": "buy",
                        "type": "market",
                        "qty": 10,
                        "fill_price": 111.0,
                        "filled_ts": 1719792065,
                        "trade_mark_pair_id": "tm_1",
                        "trade_mark_kind": "entry"
                    }],
                    "trade_marks": [{
                        "pair_id": "tm_1",
                        "fill_id": "fo_1",
                        "instrument": "SPY",
                        "kind": "entry",
                        "price": 111.0,
                        "filled_ts": 1719792065,
                        "side": "buy",
                        "visible": true
                    }]
                }
            }"#,
        )
        .unwrap();
        assert_eq!(body.paper.trade_marks.len(), 1);
        let mark = &body.paper.trade_marks[0];
        assert_eq!(mark.pair_id, "tm_1");
        assert_eq!(mark.fill_id, "fo_1");
        assert_eq!(mark.instrument, "SPY");
        assert_eq!(mark.kind, "entry");
        assert_eq!(mark.price, 111.0);
        assert_eq!(mark.filled_ts, 1_719_792_065);
        assert!(mark.visible);
        assert_eq!(
            body.paper.filled_order_history[0].trade_mark_pair_id,
            "tm_1"
        );
        assert_eq!(body.paper.filled_order_history[0].trade_mark_kind, "entry");

        let omitted: SnapshotBody = serde_json::from_str(
            r#"{
                "feed": {"status":"connected","vendor_mode":"fake","engine":"up"},
                "paper": {"active_account_id": "pa_1", "accounts": []}
            }"#,
        )
        .unwrap();
        assert!(omitted.paper.trade_marks.is_empty());
    }

    #[test]
    fn snapshot_deserializes_bracket_fields_additively() {
        let body: SnapshotBody = serde_json::from_str(
            r#"{
                "feed": {"status":"connected","vendor_mode":"fake","engine":"up"},
                "paper": {
                    "active_account_id": "pa_1",
                    "accounts": [{"id":"pa_1","name":"Paper","currency":"USD","balance":98886.5}],
                    "working_orders": [
                        {
                            "id": "wo_tp",
                            "account_id": "pa_1",
                            "instrument": "SPY",
                            "side": "sell",
                            "type": "limit",
                            "qty": 10,
                            "limit": 112.0,
                            "role": "tp",
                            "bracket_id": "br_1",
                            "placed_ts": 1719792000
                        },
                        {
                            "id": "wo_sl",
                            "account_id": "pa_1",
                            "instrument": "SPY",
                            "side": "sell",
                            "type": "stop",
                            "qty": 10,
                            "stop": 108.0,
                            "role": "sl",
                            "bracket_id": "br_1",
                            "placed_ts": 1719792000
                        }
                    ],
                    "positions": [{
                        "symbol": "SPY",
                        "side": "long",
                        "qty": 10,
                        "avg_price": 111.0,
                        "unrealized_pnl": 5.0,
                        "take_profit": 112.0,
                        "stop_loss": 108.0
                    }]
                }
            }"#,
        )
        .unwrap();
        assert_eq!(body.paper.positions[0].take_profit, Some(112.0));
        assert_eq!(body.paper.positions[0].stop_loss, Some(108.0));
        assert_eq!(body.paper.working_orders.len(), 2);
        assert_eq!(body.paper.working_orders[0].role, "tp");
        assert_eq!(
            body.paper.working_orders[0].bracket_id.as_deref(),
            Some("br_1")
        );
        assert_eq!(body.paper.working_orders[1].role, "sl");
        assert_eq!(
            body.paper.working_orders[1].bracket_id.as_deref(),
            Some("br_1")
        );

        let omitted: SnapshotBody = serde_json::from_str(
            r#"{
                "feed": {"status":"connected","vendor_mode":"fake","engine":"up"},
                "paper": {
                    "active_account_id": "pa_1",
                    "accounts": [],
                    "working_orders": [{
                        "id": "wo_1",
                        "instrument": "SPY",
                        "side": "buy",
                        "type": "limit",
                        "qty": 1,
                        "limit": 540.0
                    }]
                }
            }"#,
        )
        .unwrap();
        assert_eq!(omitted.paper.working_orders[0].role, "");
        assert_eq!(omitted.paper.working_orders[0].bracket_id, None);
        assert!(omitted.paper.positions.is_empty());
    }

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
        assert_eq!(
            ep.watchlist_rename_url().as_str(),
            "http://127.0.0.1:8765/v1/watchlist/rename"
        );
    }
}
