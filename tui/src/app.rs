//! Application state: Welcome → workspace with single or dual-vertical charts.

use std::collections::HashMap;

use crate::ipc::{
    BarUpdateEvent, ChartIndicatorsPayload, ChartInterestResponse, FeedSnapshot, IndicatorConfig,
    IndicatorSeriesData, IndicatorTypeStyle, IndicatorUpdateEvent, IndicatorsApplyResponse,
    IpcEvent, OhlcvBar, PaperAccountSnapshot, PaperSnapshot, QuoteRow, QuoteUpdateEvent,
    WatchlistSnapshot, WorkingOrderSnapshot, WorkspaceSnapshot,
};
use crate::overlay::{
    OverlayLevel, WorkingOrderLineSpec, clamp_strength, default_strength_for_type,
    working_levels_for_instrument,
};
use crate::timeframe::{format_bar_countdown, forming_bar_remaining_secs};

/// Exact empty-state copy when the vendor cannot serve the chart series.
pub const UNAVAILABLE_COPY: &str = "Data Currently not Available";

pub const DEFAULT_INSTRUMENT: &str = "SPY";
pub const DEFAULT_TIMEFRAME: &str = "1D";
pub const DUAL_TOP_INSTRUMENT: &str = "QQQ";
pub const DUAL_BOTTOM_INSTRUMENT: &str = "SPY";

pub const CHART_PRIMARY: &str = "primary";
pub const CHART_TOP: &str = "top";
pub const CHART_BOTTOM: &str = "bottom";

/// v1 product timeframes only (domain). Cycle order is coarse → fine wrap.
pub const V1_TIMEFRAMES: [&str; 9] = ["1m", "3m", "5m", "15m", "30m", "1h", "4h", "1D", "1W"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Welcome,
    Workspace,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayoutMode {
    Single,
    DualVertical,
}

impl LayoutMode {
    pub fn as_str(self) -> &'static str {
        match self {
            LayoutMode::Single => "single",
            LayoutMode::DualVertical => "dual-vertical",
        }
    }

    pub fn from_engine(s: &str) -> Self {
        match s {
            "dual-vertical" => LayoutMode::DualVertical,
            _ => LayoutMode::Single,
        }
    }

    pub fn toggled(self) -> Self {
        match self {
            LayoutMode::Single => LayoutMode::DualVertical,
            LayoutMode::DualVertical => LayoutMode::Single,
        }
    }
}

/// Unix timestamp for **09:30 America/New_York** on the calendar day that contains `ts`.
///
/// Typical Anchored VP preset (US cash open). Implements US Eastern DST rules:
/// second Sunday of March → first Sunday of November.
pub fn cash_open_ny(ts: i64) -> i64 {
    let (year, month, day) = ny_ymd(ts);
    let _ = (month, day);
    // 09:30 local on that NY calendar day.
    local_ny_to_unix(year, month, day, 9, 30)
}

fn ny_ymd(ts: i64) -> (i32, u32, u32) {
    // Iterate once: guess EST, refine with true offset at that instant.
    let offset = ny_utc_offset_seconds(ts);
    let local = ts + offset as i64;
    unix_to_ymd(local)
}

fn local_ny_to_unix(year: i32, month: u32, day: u32, hour: u32, minute: u32) -> i64 {
    let utc_guess = ymd_hms_to_unix(year, month, day, hour, minute, 0);
    // Correct for local offset at the resulting instant (handles DST edges).
    let offset = ny_utc_offset_seconds(utc_guess);
    // utc_guess treated local-as-utc; real utc = local - offset.
    let ts = utc_guess - offset as i64;
    // Recompute once in case the first guess crossed a transition.
    let offset2 = ny_utc_offset_seconds(ts);
    utc_guess - offset2 as i64
}

/// America/New_York offset east of UTC at `ts` (negative: EST -18000, EDT -14400).
fn ny_utc_offset_seconds(ts: i64) -> i32 {
    let (y, _, _) = unix_to_ymd(ts - 5 * 3600); // year in approximate EST
    let dst_start = nth_weekday_of_month_unix(y, 3, 0, 2, 2, 0, -5 * 3600); // 2nd Sun Mar 02:00 EST
    let dst_end = nth_weekday_of_month_unix(y, 11, 0, 1, 2, 0, -4 * 3600); // 1st Sun Nov 02:00 EDT
    if ts >= dst_start && ts < dst_end {
        -4 * 3600
    } else {
        -5 * 3600
    }
}

/// nth weekday (0=Sun) of month at local hour:minute with fixed `local_offset` (seconds east of UTC).
fn nth_weekday_of_month_unix(
    year: i32,
    month: u32,
    weekday: u32,
    nth: u32,
    hour: u32,
    minute: u32,
    local_offset: i32,
) -> i64 {
    // Find the first day of month that is `weekday`, then add (nth-1) weeks.
    let mut day = 1u32;
    let mut count = 0u32;
    while day <= 31 {
        let ts = ymd_hms_to_unix(year, month, day, hour, minute, 0) - local_offset as i64;
        let wd = unix_weekday(ts + local_offset as i64); // weekday of local civil day
        if wd == weekday {
            count += 1;
            if count == nth {
                return ts;
            }
        }
        day += 1;
    }
    // Fallback: should not hit for valid months.
    ymd_hms_to_unix(year, month, 1, hour, minute, 0) - local_offset as i64
}

fn unix_weekday(local_midnightish: i64) -> u32 {
    // 1970-01-01 was Thursday (4). Days since epoch.
    let days = local_midnightish.div_euclid(86_400);
    ((days + 4).rem_euclid(7)) as u32
}

fn unix_to_ymd(ts: i64) -> (i32, u32, u32) {
    // Civil from days algorithm (Howard Hinnant).
    let z = ts.div_euclid(86_400) + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 }.div_euclid(146_097);
    let doe = (z - era * 146_097) as u32;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe as i32 + era as i32 * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}

fn ymd_hms_to_unix(year: i32, month: u32, day: u32, hour: u32, minute: u32, second: u32) -> i64 {
    // Inverse of unix_to_ymd (days) + time of day, treating as UTC.
    let y = if month <= 2 { year - 1 } else { year };
    let era = if y >= 0 { y } else { y - 399 }.div_euclid(400);
    let yoe = (y - era * 400) as u32;
    let mp = if month > 2 { month - 3 } else { month + 9 };
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = (era as i64) * 146_097 + doe as i64 - 719_468;
    days * 86_400 + (hour as i64) * 3600 + (minute as i64) * 60 + second as i64
}

/// Which pin the trader is setting for Fixed Range VP.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrvpPinPhase {
    /// Bright cursor over a bar — Enter locks the range start.
    Start,
    /// Start is locked; cursor moves for the range end.
    End,
}

/// Interactive two-pin range placement on the price chart.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrvpPlaceState {
    pub chart_id: String,
    pub indicator_id: String,
    pub phase: FrvpPinPhase,
    /// Index into the chart's full bar series.
    pub cursor_bar: usize,
    /// Locked start bar index once phase is End (and after completion until cleared).
    pub start_bar: Option<usize>,
    /// If true, Esc removes the indicator (new add cancelled mid-place).
    pub is_new: bool,
}

/// Interactive single-pin anchor placement for Anchored Volume Profile.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AvpPlaceState {
    pub chart_id: String,
    pub indicator_id: String,
    /// Index into the chart's full bar series.
    pub cursor_bar: usize,
    /// If true, Esc removes the indicator (new add cancelled mid-place).
    pub is_new: bool,
}

/// Modal input for instrument selection and watchlist add/rename.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputMode {
    Normal,
    InstrumentPrompt {
        buffer: String,
    },
    WatchlistAddPrompt {
        buffer: String,
    },
    /// Rename the active watchlist sheet (display name).
    WatchlistRenamePrompt {
        buffer: String,
    },
    /// Indicator panel for the focused chart (add / toggle / configure).
    IndicatorPanel,
    /// Togglable paper desk panel (owns keys while open; shortcut TBD).
    PaperPanel,
    /// Two-pin Fixed Range VP placement on the focused chart.
    FrvpPlacing,
    /// Single-pin Anchored VP placement on the focused chart.
    AvpPlacing,
}

/// Which side of the indicator panel receives ↑↓ / Enter / side-specific keys.
///
/// Model 2: **Available** (left catalog) | **Current** (right instances).
/// Default active list is **Current**.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum IndicatorListSide {
    Available,
    #[default]
    Current,
}

/// In-panel editor for per-type overlay strength (Available · `c`).
#[derive(Debug, Clone, PartialEq)]
pub struct TypeStyleEditState {
    pub indicator_type: String,
    /// Draft strength in \[0, 1\]; persisted only on confirm.
    pub strength: f64,
}

/// Catalog rows for the Available list: `(type_key, display label)`.
pub const AVAILABLE_INDICATOR_TYPES: &[(&str, &str)] = &[
    ("ma", "MA (SMA / EMA)"),
    ("volume", "Volume"),
    ("session_vp", "Session VP"),
    ("fixed_range_vp", "Fixed Range VP"),
    ("anchored_vp", "Anchored VP"),
    ("gex", "GEX"),
    ("garch", "GARCH"),
];

/// Step for type-style overlay strength nudge in the popup.
pub const TYPE_STYLE_STRENGTH_STEP: f64 = 0.05;

/// Pending HTTP mutation against the engine watchlist API.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PendingWatchlistOp {
    SetActive { watchlist_id: String },
    Add { symbol: String },
    Remove { symbol: String },
    Rename { name: String },
}

/// Pending full-replace indicator apply for one chart.
#[derive(Debug, Clone, PartialEq)]
pub struct PendingIndicatorsApply {
    pub chart_id: String,
    pub indicators: Vec<IndicatorConfig>,
}

/// Pending type-style (overlay strength) persist for one chart.
#[derive(Debug, Clone, PartialEq)]
pub struct PendingTypeStylesApply {
    pub chart_id: String,
    pub type_styles: HashMap<String, IndicatorTypeStyle>,
}

/// Pending paper working-order mutation (engine owns the book).
#[derive(Debug, Clone, PartialEq)]
pub enum PendingPaperOp {
    Place {
        instrument: String,
        side: String,
        order_type: String,
        qty: f64,
        limit: Option<f64>,
        stop: Option<f64>,
    },
    Modify {
        order_id: String,
        qty: Option<f64>,
        limit: Option<f64>,
        stop: Option<f64>,
    },
    Cancel {
        order_id: String,
    },
}

/// Buy or sell on the order side panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrderSide {
    Buy,
    Sell,
}

impl OrderSide {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Buy => "buy",
            Self::Sell => "sell",
        }
    }
}

/// Working-order kind on the order side panel (not a filled history type).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkingOrderKind {
    Market,
    Limit,
    Stop,
}

impl WorkingOrderKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Market => "market",
            Self::Limit => "limit",
            Self::Stop => "stop",
        }
    }
}

/// Place/modify form owned by the paper panel (not a working-orders table).
#[derive(Debug, Clone, PartialEq)]
pub struct OrderSidePanel {
    pub side: OrderSide,
    pub kind: WorkingOrderKind,
    pub qty: f64,
    pub limit: f64,
    pub stop: f64,
    pub selected_order_id: Option<String>,
}

impl Default for OrderSidePanel {
    fn default() -> Self {
        Self {
            side: OrderSide::Buy,
            kind: WorkingOrderKind::Limit,
            qty: 1.0,
            limit: 0.0,
            stop: 0.0,
            selected_order_id: None,
        }
    }
}

pub const ORDER_QTY_STEP: f64 = 1.0;
pub const ORDER_PRICE_STEP: f64 = 0.01;

pub const MAX_MA_LINES: usize = 3;
pub const MAX_SESSION_VP: usize = 1;
pub const MAX_FIXED_RANGE_VP: usize = 4;
pub const MAX_ANCHORED_VP: usize = 2;
pub const MAX_GEX: usize = 1;
pub const MAX_GARCH: usize = 1;
pub const DEFAULT_MA_LENGTHS: [i64; 3] = [10, 60, 200];

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
    Available {
        bars: Vec<OhlcvBar>,
    },
    Unavailable,
    Error {
        message: String,
    },
}

/// One workspace chart: engine chart_id + instrument + timeframe + series state.
#[derive(Debug, Clone, PartialEq)]
pub struct Chart {
    pub id: String,
    pub instrument: String,
    pub timeframe: String,
    pub series: ChartSeriesState,
    /// Indicator configs for this chart (naked when empty).
    pub indicators: Vec<IndicatorConfig>,
    /// Computed series keyed by indicator id (aligned to bars by index).
    pub indicator_series: HashMap<String, IndicatorSeriesData>,
    /// Per-indicator-type presentation (overlay strength). Shared by all instances.
    pub type_styles: HashMap<String, IndicatorTypeStyle>,
    /// Chart pan: window end as bar open time (unix **seconds**), or `None` = live tip.
    ///
    /// Live tip re-attach: when the user pans back to the newest loaded bar, this is
    /// cleared so the right edge follows live bar updates again (no sticky cursor).
    /// Pan is local over already-loaded bars only — never blocks on network (A2; edge
    /// fetch of older bars is later ship B).
    pub pan_cursor_ts: Option<i64>,
    /// Soft hint: last pan step hit the oldest loaded bar (left wall of the buffer).
    pub pan_at_oldest: bool,
}

impl Chart {
    pub fn new(
        id: impl Into<String>,
        instrument: impl Into<String>,
        timeframe: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            instrument: instrument.into(),
            timeframe: timeframe.into(),
            series: ChartSeriesState::Idle,
            indicators: Vec::new(),
            indicator_series: HashMap::new(),
            type_styles: HashMap::new(),
            pan_cursor_ts: None,
            pan_at_oldest: false,
        }
    }

    /// Reset pan to live tip (e.g. on instrument/timeframe reload).
    pub fn reset_pan(&mut self) {
        self.pan_cursor_ts = None;
        self.pan_at_oldest = false;
    }

    /// Index of the bar that ends the visible window (inclusive), or `None` if no bars.
    ///
    /// `None` pan cursor → live tip (last bar). Used by pan math and volume alignment.
    pub fn pan_window_end_index(bars: &[OhlcvBar], pan_cursor_ts: Option<i64>) -> Option<usize> {
        if bars.is_empty() {
            return None;
        }
        let tip = bars.len() - 1;
        Some(match pan_cursor_ts {
            None => tip,
            Some(ts) => bars.iter().rposition(|b| b.ts <= ts).unwrap_or(0),
        })
    }

    /// Overlay strength for a type: stored type style, else product default.
    pub fn overlay_strength(&self, indicator_type: &str) -> f64 {
        self.type_styles
            .get(indicator_type)
            .map(|s| clamp_strength(s.overlay_strength))
            .unwrap_or_else(|| default_strength_for_type(indicator_type))
    }

    /// Set overlay strength for an indicator type on this chart (clamped).
    pub fn set_overlay_strength(&mut self, indicator_type: impl Into<String>, strength: f64) {
        let key = indicator_type.into();
        self.type_styles.insert(
            key,
            IndicatorTypeStyle::with_strength(clamp_strength(strength)),
        );
    }

    /// Strength map suitable for the overlay paint pass (explicit + defaults for known paint types).
    pub fn overlay_strength_map(&self) -> HashMap<String, f64> {
        let mut map = HashMap::new();
        for key in ["ma", "session_vp", "fixed_range_vp", "anchored_vp"] {
            map.insert(key.to_string(), self.overlay_strength(key));
        }
        for (k, v) in &self.type_styles {
            map.insert(k.clone(), clamp_strength(v.overlay_strength));
        }
        map
    }

    /// Snapshot of stored type styles for engine persist (only explicit entries).
    pub fn type_styles_for_persist(&self) -> HashMap<String, IndicatorTypeStyle> {
        self.type_styles.clone()
    }

    pub fn default_single() -> Self {
        Self::new(CHART_PRIMARY, DEFAULT_INSTRUMENT, DEFAULT_TIMEFRAME)
    }

    pub fn default_dual_top() -> Self {
        Self::new(CHART_TOP, DUAL_TOP_INSTRUMENT, DEFAULT_TIMEFRAME)
    }

    pub fn default_dual_bottom() -> Self {
        Self::new(CHART_BOTTOM, DUAL_BOTTOM_INSTRUMENT, DEFAULT_TIMEFRAME)
    }

    pub fn title(&self) -> String {
        format!("{} · {}", self.instrument, self.timeframe)
    }

    /// Forming-bar countdown label for chart chrome (`m:ss` / …), when a live series tip exists.
    ///
    /// Uses the **last loaded bar** (forming incomplete tip) and wall-clock `now_ts` (unix seconds).
    /// Independent per chart (dual layout shows two countdowns). Returns `None` when the series
    /// is unavailable/empty/unknown TF so the UI does not invent times.
    pub fn forming_bar_countdown_label(&self, now_ts: i64) -> Option<String> {
        let ChartSeriesState::Available { bars } = &self.series else {
            return None;
        };
        let last = bars.last()?;
        forming_bar_remaining_secs(&self.timeframe, last.ts, now_ts).map(format_bar_countdown)
    }

    /// Chart block title including optional forming-bar countdown.
    pub fn chrome_title(&self, focused: bool, now_ts: i64) -> String {
        let focus_mark = if focused { "● " } else { "  " };
        match self.forming_bar_countdown_label(now_ts) {
            Some(cd) => format!(" {focus_mark}{} · {cd} ", self.title()),
            None => format!(" {focus_mark}{} ", self.title()),
        }
    }

    pub fn has_volume(&self) -> bool {
        self.indicators
            .iter()
            .any(|i| i.indicator_type == "volume" && i.enabled)
    }

    pub fn enabled_ma_lines(&self) -> Vec<(&IndicatorConfig, &IndicatorSeriesData)> {
        self.indicators
            .iter()
            .filter(|i| i.indicator_type == "ma" && i.enabled)
            .filter_map(|cfg| {
                self.indicator_series
                    .get(&cfg.id)
                    .map(|series| (cfg, series))
            })
            .collect()
    }

    pub fn volume_series(&self) -> Option<&IndicatorSeriesData> {
        self.indicators
            .iter()
            .find(|i| i.indicator_type == "volume" && i.enabled)
            .and_then(|cfg| self.indicator_series.get(&cfg.id))
    }

    /// Enabled Session VP config + series (max one per chart).
    pub fn enabled_session_vp(&self) -> Option<(&IndicatorConfig, &IndicatorSeriesData)> {
        self.indicators
            .iter()
            .find(|i| i.indicator_type == "session_vp" && i.enabled)
            .and_then(|cfg| self.indicator_series.get(&cfg.id).map(|s| (cfg, s)))
    }

    /// Enabled Fixed Range VP instances with their series (max 4 configured).
    pub fn enabled_fixed_range_vps(&self) -> Vec<(&IndicatorConfig, &IndicatorSeriesData)> {
        self.indicators
            .iter()
            .filter(|i| i.indicator_type == "fixed_range_vp" && i.enabled)
            .filter_map(|cfg| self.indicator_series.get(&cfg.id).map(|s| (cfg, s)))
            .collect()
    }

    /// Enabled Anchored VP instances with their series (max 2 configured).
    pub fn enabled_anchored_vps(&self) -> Vec<(&IndicatorConfig, &IndicatorSeriesData)> {
        self.indicators
            .iter()
            .filter(|i| i.indicator_type == "anchored_vp" && i.enabled)
            .filter_map(|cfg| self.indicator_series.get(&cfg.id).map(|s| (cfg, s)))
            .collect()
    }

    /// Enabled GARCH series when status is ok (no invented path when unavailable).
    pub fn enabled_garch(&self) -> Option<(&IndicatorConfig, &IndicatorSeriesData)> {
        self.indicators
            .iter()
            .find(|i| i.indicator_type == "garch" && i.enabled)
            .and_then(|cfg| self.indicator_series.get(&cfg.id).map(|s| (cfg, s)))
            .filter(|(_, s)| s.status.as_deref() == Some("ok") && !s.values.is_empty())
    }

    /// Enabled GEX series when status is ok (no invented levels when unavailable).
    pub fn enabled_gex(&self) -> Option<(&IndicatorConfig, &IndicatorSeriesData)> {
        self.indicators
            .iter()
            .find(|i| i.indicator_type == "gex" && i.enabled)
            .and_then(|cfg| self.indicator_series.get(&cfg.id).map(|s| (cfg, s)))
            .filter(|(_, s)| s.status.as_deref() == Some("ok"))
    }
}

#[derive(Debug, Clone)]
pub struct App {
    pub screen: Screen,
    pub layout: LayoutMode,
    pub charts: Vec<Chart>,
    /// Index into `charts` for instrument/timeframe keys.
    pub focused: usize,
    pub connection: ConnectionStatus,
    pub feed: Option<FeedSnapshot>,
    /// Last workspace from engine snapshot (applied on Welcome → Workspace).
    pub pending_workspace: Option<WorkspaceSnapshot>,
    /// Quotes stashed with the deferred Welcome snapshot.
    pub pending_quotes: Vec<QuoteRow>,
    pub last_heartbeat_ts: Option<f64>,
    /// Latest vendor tick timestamp (unix seconds) for feed delay. One value for the desk.
    pub last_vendor_tick_ts: Option<f64>,
    /// When true, the main loop should POST chart interest for every chart.
    pub needs_chart_load: bool,
    /// When Some, the main loop should POST layout change to the engine.
    pub pending_layout: Option<LayoutMode>,
    /// Right watchlist sidebar visible (local UI; not persisted).
    pub watchlist_visible: bool,
    pub watchlists: Vec<WatchlistSnapshot>,
    pub active_watchlist_id: String,
    /// Latest quote fields keyed by symbol (from snapshot + live WS).
    pub quotes: HashMap<String, QuoteRow>,
    /// Selected row index within the active watchlist symbols.
    pub watchlist_selected: usize,
    /// When Some, main loop issues the matching watchlist HTTP mutation.
    pub pending_watchlist: Option<PendingWatchlistOp>,
    /// When Some, main loop POSTs indicator apply for that chart.
    pub pending_indicators: Option<PendingIndicatorsApply>,
    /// When Some, main loop POSTs type styles for that chart.
    pub pending_type_styles: Option<PendingTypeStylesApply>,
    /// Selected row in the **Current** indicators list.
    pub indicator_selected: usize,
    /// Selected row in the **Available** catalog list.
    pub indicator_available_selected: usize,
    /// Active side of the indicator panel (Tab switches; default Current).
    pub indicator_list_side: IndicatorListSide,
    /// When Some, type-style (overlay strength) popup is open over the panel.
    pub type_style_edit: Option<TypeStyleEditState>,
    pub input_mode: InputMode,
    /// Active Fixed Range two-pin placement (when `input_mode == FrvpPlacing`).
    pub frvp_place: Option<FrvpPlaceState>,
    /// Active Anchored VP single-pin placement (when `input_mode == AvpPlacing`).
    pub avp_place: Option<AvpPlaceState>,
    /// Last engine error from indicator apply (shown in chrome).
    pub last_indicator_error: Option<String>,
    /// Floating keyboard-shortcut help overlay (does not replace input_mode).
    pub help_open: bool,
    /// Engine paper desk (accounts + empty Position / history tables).
    pub paper: PaperSnapshot,
    /// Place/modify/cancel form; visible with the paper panel.
    pub order_side: OrderSidePanel,
    /// When Some, main loop POSTs place/modify/cancel to the engine.
    pub pending_paper: Option<PendingPaperOp>,
    /// Last engine rejection from place/modify/cancel (shown on the order side panel).
    pub last_paper_error: Option<String>,
    pub should_quit: bool,
}

impl Default for App {
    fn default() -> Self {
        Self {
            screen: Screen::Welcome,
            layout: LayoutMode::Single,
            charts: vec![Chart::default_single()],
            focused: 0,
            connection: ConnectionStatus::Connecting,
            feed: None,
            pending_workspace: None,
            pending_quotes: Vec::new(),
            last_heartbeat_ts: None,
            last_vendor_tick_ts: None,
            needs_chart_load: false,
            pending_layout: None,
            watchlist_visible: true,
            watchlists: Vec::new(),
            active_watchlist_id: String::new(),
            quotes: HashMap::new(),
            watchlist_selected: 0,
            pending_watchlist: None,
            pending_indicators: None,
            pending_type_styles: None,
            indicator_selected: 0,
            indicator_available_selected: 0,
            indicator_list_side: IndicatorListSide::Current,
            type_style_edit: None,
            input_mode: InputMode::Normal,
            frvp_place: None,
            avp_place: None,
            last_indicator_error: None,
            help_open: false,
            paper: PaperSnapshot::default(),
            order_side: OrderSidePanel::default(),
            pending_paper: None,
            last_paper_error: None,
            should_quit: false,
        }
    }
}

impl App {
    pub fn focused_chart(&self) -> &Chart {
        &self.charts[self.focused.min(self.charts.len().saturating_sub(1))]
    }

    pub fn focused_chart_mut(&mut self) -> &mut Chart {
        let idx = self.focused.min(self.charts.len().saturating_sub(1));
        &mut self.charts[idx]
    }

    /// Compatibility: primary/focused chart (single-layout tests).
    pub fn chart(&self) -> &Chart {
        self.focused_chart()
    }

    pub fn enter_workspace(&mut self) {
        if self.screen == Screen::Welcome {
            self.screen = Screen::Workspace;
            if let Some(ws) = self.pending_workspace.take() {
                self.apply_workspace(ws);
            }
            let quotes = std::mem::take(&mut self.pending_quotes);
            if !quotes.is_empty() {
                self.apply_quotes(quotes);
            }
            self.request_chart_load();
        }
    }

    pub fn toggle_watchlist_sidebar(&mut self) {
        if self.screen != Screen::Workspace {
            return;
        }
        if !matches!(self.input_mode, InputMode::Normal) {
            return;
        }
        self.watchlist_visible = !self.watchlist_visible;
    }

    pub fn active_watchlist(&self) -> Option<&WatchlistSnapshot> {
        self.watchlists
            .iter()
            .find(|wl| wl.id == self.active_watchlist_id)
            .or_else(|| self.watchlists.first())
    }

    pub fn active_symbols(&self) -> &[String] {
        self.active_watchlist()
            .map(|wl| wl.symbols.as_slice())
            .unwrap_or(&[])
    }

    /// Quote row for a symbol, if known.
    pub fn quote_for(&self, symbol: &str) -> Option<&QuoteRow> {
        self.quotes.get(symbol)
    }

    /// Cycle active watchlist by `delta` steps (wraps). Arms engine mutation.
    pub fn cycle_watchlist(&mut self, delta: i32) {
        if self.screen != Screen::Workspace {
            return;
        }
        if !matches!(self.input_mode, InputMode::Normal) {
            return;
        }
        if self.watchlists.len() < 2 {
            return;
        }
        let idx = self
            .watchlists
            .iter()
            .position(|wl| wl.id == self.active_watchlist_id)
            .unwrap_or(0);
        let n = self.watchlists.len() as i32;
        let next = (idx as i32 + delta).rem_euclid(n) as usize;
        let id = self.watchlists[next].id.clone();
        if id == self.active_watchlist_id {
            return;
        }
        self.pending_watchlist = Some(PendingWatchlistOp::SetActive { watchlist_id: id });
    }

    pub fn begin_watchlist_add_prompt(&mut self) {
        if self.screen != Screen::Workspace {
            return;
        }
        if !self.watchlist_visible {
            return;
        }
        self.input_mode = InputMode::WatchlistAddPrompt {
            buffer: String::new(),
        };
    }

    pub fn apply_watchlist_add_prompt(&mut self) -> bool {
        let InputMode::WatchlistAddPrompt { buffer } = &self.input_mode else {
            return false;
        };
        let symbol = normalize_instrument(buffer);
        self.input_mode = InputMode::Normal;
        if symbol.is_empty() {
            return false;
        }
        self.pending_watchlist = Some(PendingWatchlistOp::Add { symbol });
        true
    }

    pub fn remove_selected_watchlist_symbol(&mut self) {
        if self.screen != Screen::Workspace {
            return;
        }
        if !matches!(self.input_mode, InputMode::Normal) {
            return;
        }
        if !self.watchlist_visible {
            return;
        }
        let symbols = self.active_symbols();
        if symbols.is_empty() {
            return;
        }
        let idx = self.watchlist_selected.min(symbols.len() - 1);
        let symbol = symbols[idx].clone();
        self.pending_watchlist = Some(PendingWatchlistOp::Remove { symbol });
    }

    /// Enter/Space on a watchlist row: set focused chart instrument (keep TF + indicators).
    /// Returns true when chart interest must reload.
    pub fn load_selected_watchlist_symbol(&mut self) -> bool {
        if self.screen != Screen::Workspace {
            return false;
        }
        if !matches!(self.input_mode, InputMode::Normal) {
            return false;
        }
        if !self.watchlist_visible {
            return false;
        }
        let symbols = self.active_symbols();
        if symbols.is_empty() {
            return false;
        }
        let idx = self.watchlist_selected.min(symbols.len() - 1);
        let symbol = symbols[idx].clone();
        self.set_instrument(&symbol)
    }

    /// Open rename prompt for the active watchlist (display name). Prefills current name.
    pub fn begin_watchlist_rename_prompt(&mut self) {
        if self.screen != Screen::Workspace {
            return;
        }
        if !matches!(self.input_mode, InputMode::Normal) {
            return;
        }
        let current = self
            .active_watchlist()
            .map(|wl| wl.name.clone())
            .unwrap_or_default();
        self.input_mode = InputMode::WatchlistRenamePrompt { buffer: current };
    }

    /// Apply rename prompt. Empty (after trim) is rejected; arms engine rename when non-empty.
    pub fn apply_watchlist_rename_prompt(&mut self) -> bool {
        let InputMode::WatchlistRenamePrompt { buffer } = &self.input_mode else {
            return false;
        };
        let name = buffer.trim().to_string();
        if name.is_empty() {
            // Stay in prompt so the user can correct; empty is never sent.
            return false;
        }
        self.input_mode = InputMode::Normal;
        self.pending_watchlist = Some(PendingWatchlistOp::Rename { name });
        true
    }

    pub fn watchlist_select_delta(&mut self, delta: i32) {
        if self.screen != Screen::Workspace || !self.watchlist_visible {
            return;
        }
        if !matches!(self.input_mode, InputMode::Normal) {
            return;
        }
        let n = self.active_symbols().len();
        if n == 0 {
            self.watchlist_selected = 0;
            return;
        }
        let cur = self.watchlist_selected.min(n - 1) as i32;
        self.watchlist_selected = (cur + delta).rem_euclid(n as i32) as usize;
    }

    pub fn watchlist_request_started(&mut self) {
        self.pending_watchlist = None;
    }

    pub fn apply_quotes(&mut self, quotes: Vec<QuoteRow>) {
        for q in quotes {
            self.quotes.insert(q.symbol.clone(), q);
        }
    }

    pub fn apply_quote_update(&mut self, update: QuoteUpdateEvent) {
        self.quotes.insert(update.symbol.clone(), update.to_row());
    }

    pub fn apply_watchlist_state(&mut self, workspace: WorkspaceSnapshot, quotes: Vec<QuoteRow>) {
        // Watchlist mutations return a full workspace snapshot, but only membership /
        // active sheet / names should change here. Rebuilding charts via apply_workspace
        // would drop live series (Idle) without re-arming chart interest.
        if !workspace.watchlists.is_empty() {
            self.watchlists = workspace.watchlists;
            self.active_watchlist_id = if !workspace.active_watchlist_id.is_empty()
                && self
                    .watchlists
                    .iter()
                    .any(|wl| wl.id == workspace.active_watchlist_id)
            {
                workspace.active_watchlist_id
            } else {
                self.watchlists[0].id.clone()
            };
            self.clamp_watchlist_selection();
        }
        self.apply_quotes(quotes);
    }

    fn clamp_watchlist_selection(&mut self) {
        let n = self.active_symbols().len();
        if n == 0 {
            self.watchlist_selected = 0;
        } else if self.watchlist_selected >= n {
            self.watchlist_selected = n - 1;
        }
    }

    pub fn request_chart_load(&mut self) {
        for chart in &mut self.charts {
            chart.series = ChartSeriesState::Loading;
            chart.reset_pan();
        }
        self.needs_chart_load = true;
    }

    /// Pan the **focused** chart by `delta` bars over **loaded** history only.
    ///
    /// - Negative `delta` moves earlier (←); positive moves later (→).
    /// - Clamps at oldest and newest loaded bars (no network).
    /// - Returning to the newest bar re-attaches to the live tip (`pan_cursor_ts = None`).
    /// - Sets `pan_at_oldest` when the left wall is hit (soft UI hint).
    /// - No-op outside Normal workspace mode, or without an Available series.
    /// - Pin placement / prompts own ← → elsewhere — callers should only invoke in Normal.
    pub fn pan_focused_chart(&mut self, delta: i32) {
        if self.screen != Screen::Workspace {
            return;
        }
        if !matches!(self.input_mode, InputMode::Normal) {
            return;
        }
        if delta == 0 {
            return;
        }
        let chart = self.focused_chart_mut();
        let (new_cursor, at_oldest) = {
            let ChartSeriesState::Available { bars } = &chart.series else {
                return;
            };
            let Some(end_idx) = Chart::pan_window_end_index(bars, chart.pan_cursor_ts) else {
                return;
            };
            let tip = bars.len() - 1;
            let new_idx = (end_idx as i32 + delta).clamp(0, tip as i32) as usize;
            if new_idx >= tip {
                // Live tip re-attach: follow newest bar (and subsequent live updates).
                (None, false)
            } else {
                (Some(bars[new_idx].ts), new_idx == 0)
            }
        };
        chart.pan_cursor_ts = new_cursor;
        chart.pan_at_oldest = at_oldest;
    }

    pub fn chart_load_started(&mut self) {
        self.needs_chart_load = false;
    }

    pub fn quit(&mut self) {
        self.should_quit = true;
    }

    /// Toggle layout mode; engine call is armed via `pending_layout`.
    pub fn toggle_layout(&mut self) {
        if self.screen != Screen::Workspace {
            return;
        }
        if !matches!(self.input_mode, InputMode::Normal) {
            return;
        }
        self.pending_layout = Some(self.layout.toggled());
    }

    pub fn layout_request_started(&mut self) {
        self.pending_layout = None;
    }

    /// Cycle focus between charts in dual layout (no-op for single).
    pub fn focus_next(&mut self) {
        if self.screen != Screen::Workspace {
            return;
        }
        if !matches!(self.input_mode, InputMode::Normal) {
            return;
        }
        if self.charts.len() < 2 {
            return;
        }
        self.focused = (self.focused + 1) % self.charts.len();
    }

    /// Apply engine workspace public shape (layout + charts + watchlists + indicator configs).
    pub fn apply_workspace(&mut self, ws: WorkspaceSnapshot) {
        self.layout = LayoutMode::from_engine(&ws.layout_mode);
        if ws.charts.is_empty() {
            self.charts = match self.layout {
                LayoutMode::Single => vec![Chart::default_single()],
                LayoutMode::DualVertical => {
                    vec![Chart::default_dual_top(), Chart::default_dual_bottom()]
                }
            };
        } else {
            self.charts = ws
                .charts
                .into_iter()
                .map(|c| {
                    let mut chart = Chart::new(c.id, c.instrument, c.timeframe);
                    chart.indicators = c.indicators;
                    chart.type_styles = c.type_styles;
                    chart
                })
                .collect();
        }
        if self.focused >= self.charts.len() {
            self.focused = 0;
        }
        self.clamp_indicator_selection();
        if !ws.watchlists.is_empty() {
            self.watchlists = ws.watchlists;
            self.active_watchlist_id = if !ws.active_watchlist_id.is_empty()
                && self
                    .watchlists
                    .iter()
                    .any(|wl| wl.id == ws.active_watchlist_id)
            {
                ws.active_watchlist_id
            } else {
                self.watchlists[0].id.clone()
            };
            self.clamp_watchlist_selection();
        }
    }

    pub fn apply_indicator_payloads(&mut self, payloads: HashMap<String, ChartIndicatorsPayload>) {
        for (chart_id, payload) in payloads {
            if let Some(chart) = self.charts.iter_mut().find(|c| c.id == chart_id) {
                // Always take engine configs (including empty = naked).
                chart.indicators = payload.indicators;
                chart.indicator_series = payload.series;
            }
        }
        self.clamp_indicator_selection();
    }

    fn clamp_indicator_selection(&mut self) {
        let n = self.focused_chart().indicators.len();
        if n == 0 {
            self.indicator_selected = 0;
        } else if self.indicator_selected >= n {
            self.indicator_selected = n - 1;
        }
    }

    /// Toggle the floating keyboard-shortcut help without replacing input_mode.
    pub fn toggle_help(&mut self) {
        self.help_open = !self.help_open;
    }

    pub fn close_help(&mut self) {
        self.help_open = false;
    }

    pub fn toggle_indicator_panel(&mut self) {
        if self.help_open {
            return;
        }
        if self.screen != Screen::Workspace {
            return;
        }
        match self.input_mode {
            InputMode::Normal => {
                self.input_mode = InputMode::IndicatorPanel;
                // Model 2: default active list is Current when the panel opens.
                self.indicator_list_side = IndicatorListSide::Current;
                self.type_style_edit = None;
                self.clamp_indicator_selection();
                self.clamp_available_selection();
            }
            InputMode::IndicatorPanel => {
                self.type_style_edit = None;
                self.input_mode = InputMode::Normal;
            }
            _ => {}
        }
    }

    pub fn close_indicator_panel(&mut self) {
        if matches!(self.input_mode, InputMode::IndicatorPanel) {
            self.type_style_edit = None;
            self.input_mode = InputMode::Normal;
        }
    }

    /// Toggle the paper panel (local UI control; keyboard shortcut TBD).
    ///
    /// Open → owns input focus like the indicator panel. Watchlist arrows and
    /// chart pan stay idle until it closes.
    pub fn toggle_paper_panel(&mut self) {
        if self.help_open {
            return;
        }
        if self.screen != Screen::Workspace {
            return;
        }
        match self.input_mode {
            InputMode::Normal => {
                self.input_mode = InputMode::PaperPanel;
                self.order_side = OrderSidePanel::default();
                self.seed_order_side_prices();
            }
            InputMode::PaperPanel => {
                self.input_mode = InputMode::Normal;
            }
            _ => {}
        }
    }

    pub fn close_paper_panel(&mut self) {
        if matches!(self.input_mode, InputMode::PaperPanel) {
            self.input_mode = InputMode::Normal;
        }
    }

    pub fn apply_paper(&mut self, paper: PaperSnapshot) {
        self.paper = paper;
        self.last_paper_error = None;
        self.sync_order_side_selection();
    }

    pub fn active_paper_account(&self) -> Option<&PaperAccountSnapshot> {
        self.paper.active_account()
    }

    /// Instrument for place/modify: the **focused chart**, not the watchlist row.
    pub fn order_side_instrument(&self) -> &str {
        self.focused_chart().instrument.as_str()
    }

    pub fn working_orders_for_instrument(&self, instrument: &str) -> Vec<&WorkingOrderSnapshot> {
        self.paper
            .working_orders
            .iter()
            .filter(|o| o.instrument.eq_ignore_ascii_case(instrument))
            .collect()
    }

    /// Working **lines** for one chart instrument (market orders have no price line).
    pub fn working_overlay_levels(&self, instrument: &str, x0: f64, x1: f64) -> Vec<OverlayLevel> {
        let specs: Vec<WorkingOrderLineSpec> = self
            .paper
            .working_orders
            .iter()
            .map(|o| WorkingOrderLineSpec {
                instrument: o.instrument.clone(),
                side: o.side.clone(),
                order_type: o.order_type.clone(),
                limit: o.limit,
                stop: o.stop,
            })
            .collect();
        working_levels_for_instrument(&specs, instrument, x0, x1)
    }

    pub fn paper_request_started(&mut self) {
        self.pending_paper = None;
    }

    pub fn paper_cycle_side(&mut self) {
        if !matches!(self.input_mode, InputMode::PaperPanel) {
            return;
        }
        if self.order_side.selected_order_id.is_some() {
            return;
        }
        self.order_side.side = match self.order_side.side {
            OrderSide::Buy => OrderSide::Sell,
            OrderSide::Sell => OrderSide::Buy,
        };
    }

    pub fn paper_set_side(&mut self, side: OrderSide) {
        if !matches!(self.input_mode, InputMode::PaperPanel) {
            return;
        }
        if self.order_side.selected_order_id.is_some() {
            return;
        }
        self.order_side.side = side;
    }

    pub fn paper_set_kind(&mut self, kind: WorkingOrderKind) {
        if !matches!(self.input_mode, InputMode::PaperPanel) {
            return;
        }
        if self.order_side.selected_order_id.is_some() {
            return;
        }
        self.order_side.kind = kind;
    }

    pub fn paper_nudge_qty(&mut self, steps: i32) {
        if !matches!(self.input_mode, InputMode::PaperPanel) {
            return;
        }
        if steps == 0 {
            return;
        }
        let next = self.order_side.qty + f64::from(steps) * ORDER_QTY_STEP;
        self.order_side.qty = if next < ORDER_QTY_STEP {
            ORDER_QTY_STEP
        } else {
            next
        };
    }

    pub fn paper_nudge_price(&mut self, steps: i32) {
        if !matches!(self.input_mode, InputMode::PaperPanel) {
            return;
        }
        if steps == 0 {
            return;
        }
        match self.order_side.kind {
            WorkingOrderKind::Limit => {
                let next = self.order_side.limit + f64::from(steps) * ORDER_PRICE_STEP;
                self.order_side.limit = if next < ORDER_PRICE_STEP {
                    ORDER_PRICE_STEP
                } else {
                    next
                };
            }
            WorkingOrderKind::Stop => {
                let next = self.order_side.stop + f64::from(steps) * ORDER_PRICE_STEP;
                self.order_side.stop = if next < ORDER_PRICE_STEP {
                    ORDER_PRICE_STEP
                } else {
                    next
                };
            }
            WorkingOrderKind::Market => {}
        }
    }

    /// Cycle working orders for the focused instrument, including a None (place) slot.
    pub fn paper_select_working_delta(&mut self, delta: i32) {
        if !matches!(self.input_mode, InputMode::PaperPanel) {
            return;
        }
        if delta == 0 {
            return;
        }
        let inst = self.order_side_instrument().to_string();
        let mut ids: Vec<Option<String>> = vec![None];
        ids.extend(
            self.working_orders_for_instrument(&inst)
                .into_iter()
                .map(|o| Some(o.id.clone())),
        );
        if ids.len() == 1 {
            self.order_side.selected_order_id = None;
            return;
        }
        let cur = ids
            .iter()
            .position(|id| *id == self.order_side.selected_order_id)
            .unwrap_or(0) as i32;
        let next = (cur + delta).rem_euclid(ids.len() as i32) as usize;
        self.order_side.selected_order_id = ids[next].clone();
        self.sync_order_side_selection();
    }

    pub fn paper_submit(&mut self) {
        if !matches!(self.input_mode, InputMode::PaperPanel) {
            return;
        }
        if self.order_side.qty <= 0.0 {
            self.last_paper_error = Some("qty must be > 0".into());
            return;
        }
        let instrument = self.order_side_instrument().to_string();
        let side = self.order_side.side.as_str().to_string();
        let order_type = self.order_side.kind.as_str().to_string();
        let qty = self.order_side.qty;
        let (limit, stop) = match self.order_side.kind {
            WorkingOrderKind::Market => (None, None),
            WorkingOrderKind::Limit => {
                if self.order_side.limit <= 0.0 {
                    self.last_paper_error = Some("limit is required".into());
                    return;
                }
                (Some(self.order_side.limit), None)
            }
            WorkingOrderKind::Stop => {
                if self.order_side.stop <= 0.0 {
                    self.last_paper_error = Some("stop is required".into());
                    return;
                }
                (None, Some(self.order_side.stop))
            }
        };
        if let Some(order_id) = self.order_side.selected_order_id.clone() {
            self.pending_paper = Some(PendingPaperOp::Modify {
                order_id,
                qty: Some(qty),
                limit,
                stop,
            });
        } else {
            self.pending_paper = Some(PendingPaperOp::Place {
                instrument,
                side,
                order_type,
                qty,
                limit,
                stop,
            });
        }
    }

    pub fn paper_cancel_selected(&mut self) {
        if !matches!(self.input_mode, InputMode::PaperPanel) {
            return;
        }
        let Some(order_id) = self.order_side.selected_order_id.clone() else {
            return;
        };
        self.pending_paper = Some(PendingPaperOp::Cancel { order_id });
    }

    fn seed_order_side_prices(&mut self) {
        let inst = self.order_side_instrument().to_string();
        let last = self.quotes.get(&inst).and_then(|q| q.last);
        if let Some(last) = last {
            if self.order_side.limit <= 0.0 {
                self.order_side.limit = last;
            }
            if self.order_side.stop <= 0.0 {
                self.order_side.stop = last;
            }
        }
    }

    fn sync_order_side_selection(&mut self) {
        let Some(id) = self.order_side.selected_order_id.clone() else {
            return;
        };
        let Some(wo) = self.paper.working_orders.iter().find(|o| o.id == id) else {
            self.order_side.selected_order_id = None;
            return;
        };
        self.order_side.side = if wo.side.eq_ignore_ascii_case("sell") {
            OrderSide::Sell
        } else {
            OrderSide::Buy
        };
        self.order_side.kind = match wo.order_type.as_str() {
            "market" => WorkingOrderKind::Market,
            "stop" => WorkingOrderKind::Stop,
            _ => WorkingOrderKind::Limit,
        };
        self.order_side.qty = wo.qty;
        if let Some(limit) = wo.limit {
            self.order_side.limit = limit;
        }
        if let Some(stop) = wo.stop {
            self.order_side.stop = stop;
        }
    }

    /// Tab: switch Available ↔ Current while the indicator panel owns focus.
    ///
    /// Does **not** cycle dual-layout chart focus (panel owns Tab).
    pub fn indicator_toggle_list_side(&mut self) {
        if !matches!(self.input_mode, InputMode::IndicatorPanel) {
            return;
        }
        if self.type_style_edit.is_some() {
            return;
        }
        self.indicator_list_side = match self.indicator_list_side {
            IndicatorListSide::Available => IndicatorListSide::Current,
            IndicatorListSide::Current => IndicatorListSide::Available,
        };
        match self.indicator_list_side {
            IndicatorListSide::Available => self.clamp_available_selection(),
            IndicatorListSide::Current => self.clamp_indicator_selection(),
        }
    }

    pub fn indicator_select_delta(&mut self, delta: i32) {
        if !matches!(self.input_mode, InputMode::IndicatorPanel) {
            return;
        }
        if self.type_style_edit.is_some() {
            return;
        }
        match self.indicator_list_side {
            IndicatorListSide::Available => {
                let n = AVAILABLE_INDICATOR_TYPES.len();
                if n == 0 {
                    self.indicator_available_selected = 0;
                    return;
                }
                let cur = self.indicator_available_selected.min(n - 1) as i32;
                self.indicator_available_selected = (cur + delta).rem_euclid(n as i32) as usize;
            }
            IndicatorListSide::Current => {
                let n = self.focused_chart().indicators.len();
                if n == 0 {
                    self.indicator_selected = 0;
                    return;
                }
                let cur = self.indicator_selected.min(n - 1) as i32;
                self.indicator_selected = (cur + delta).rem_euclid(n as i32) as usize;
            }
        }
    }

    fn clamp_available_selection(&mut self) {
        let n = AVAILABLE_INDICATOR_TYPES.len();
        if n == 0 {
            self.indicator_available_selected = 0;
        } else if self.indicator_available_selected >= n {
            self.indicator_available_selected = n - 1;
        }
    }

    /// Enter/Space on the active list: Available adds; Current toggles on/off.
    pub fn indicator_activate_selected(&mut self) {
        if !matches!(self.input_mode, InputMode::IndicatorPanel) {
            return;
        }
        if self.type_style_edit.is_some() {
            return;
        }
        match self.indicator_list_side {
            IndicatorListSide::Available => self.indicator_add_selected_available(),
            IndicatorListSide::Current => self.indicator_toggle_selected(),
        }
    }

    /// Add the catalog type selected on Available (respects Phase A max counts).
    pub fn indicator_add_selected_available(&mut self) {
        if !matches!(self.input_mode, InputMode::IndicatorPanel) {
            return;
        }
        if self.indicator_list_side != IndicatorListSide::Available {
            return;
        }
        let Some(&(type_key, _)) = AVAILABLE_INDICATOR_TYPES.get(self.indicator_available_selected)
        else {
            return;
        };
        match type_key {
            "ma" => self.indicator_add_default_ma_stack(),
            "volume" => self.indicator_add_volume(),
            "session_vp" => self.indicator_add_session_vp(),
            "fixed_range_vp" => self.indicator_add_fixed_range_vp(),
            "anchored_vp" => self.indicator_add_anchored_vp(),
            "gex" => self.indicator_add_gex(),
            "garch" => self.indicator_add_garch(),
            _ => {}
        }
    }

    /// Open type-style popup for the Available selection (overlay strength).
    pub fn indicator_open_type_style(&mut self) {
        if !matches!(self.input_mode, InputMode::IndicatorPanel) {
            return;
        }
        if self.indicator_list_side != IndicatorListSide::Available {
            return;
        }
        if self.type_style_edit.is_some() {
            return;
        }
        let Some(&(type_key, _)) = AVAILABLE_INDICATOR_TYPES.get(self.indicator_available_selected)
        else {
            return;
        };
        // Volume is a sub-pane (no price overlay); type style still stored for consistency
        // but only overlay types meaningfully affect paint.
        let strength = self.focused_chart().overlay_strength(type_key);
        self.type_style_edit = Some(TypeStyleEditState {
            indicator_type: type_key.to_string(),
            strength,
        });
    }

    pub fn type_style_nudge(&mut self, delta_steps: i32) {
        let Some(edit) = self.type_style_edit.as_mut() else {
            return;
        };
        let next = edit.strength + (delta_steps as f64) * TYPE_STYLE_STRENGTH_STEP;
        edit.strength = clamp_strength(next);
    }

    /// Confirm draft type style → focused chart + engine persist.
    pub fn type_style_confirm(&mut self) {
        let Some(edit) = self.type_style_edit.take() else {
            return;
        };
        let chart_id = self.focused_chart().id.clone();
        self.set_chart_overlay_strength(&chart_id, &edit.indicator_type, edit.strength);
    }

    pub fn type_style_cancel(&mut self) {
        self.type_style_edit = None;
    }

    /// Count of instances of `type_key` on the focused chart (for Available max hints).
    pub fn focused_indicator_type_count(&self, type_key: &str) -> usize {
        self.focused_chart()
            .indicators
            .iter()
            .filter(|i| i.indicator_type == type_key)
            .count()
    }

    /// Phase A max instances for a catalog type (`None` = unbounded / unknown).
    pub fn max_for_indicator_type(type_key: &str) -> Option<usize> {
        match type_key {
            "ma" => Some(MAX_MA_LINES),
            "volume" | "session_vp" | "gex" | "garch" => Some(1),
            "fixed_range_vp" => Some(MAX_FIXED_RANGE_VP),
            "anchored_vp" => Some(MAX_ANCHORED_VP),
            _ => None,
        }
    }

    /// Set overlay strength on a chart and arm engine persist.
    pub fn set_chart_overlay_strength(
        &mut self,
        chart_id: &str,
        indicator_type: &str,
        strength: f64,
    ) {
        let Some(chart) = self.charts.iter_mut().find(|c| c.id == chart_id) else {
            return;
        };
        chart.set_overlay_strength(indicator_type, strength);
        self.pending_type_styles = Some(PendingTypeStylesApply {
            chart_id: chart.id.clone(),
            type_styles: chart.type_styles_for_persist(),
        });
    }

    pub fn type_styles_request_started(&mut self) {
        self.pending_type_styles = None;
    }

    fn arm_indicators_apply(&mut self) {
        let id = self.focused_chart().id.clone();
        self.arm_indicators_apply_for(&id);
    }

    /// Arm indicator apply for an explicit chart (FRVP/AVP pin completion).
    fn arm_indicators_apply_for(&mut self, chart_id: &str) {
        let Some(chart) = self.charts.iter().find(|c| c.id == chart_id) else {
            return;
        };
        self.pending_indicators = Some(PendingIndicatorsApply {
            chart_id: chart.id.clone(),
            indicators: chart.indicators.clone(),
        });
    }

    pub fn indicators_request_started(&mut self) {
        self.pending_indicators = None;
    }

    /// Add default MA stack (SMA 10/60/200), or fill remaining default lengths up to max 3.
    pub fn indicator_add_default_ma_stack(&mut self) {
        if !matches!(self.input_mode, InputMode::IndicatorPanel) {
            return;
        }
        let chart = self.focused_chart_mut();
        let existing_lengths: std::collections::HashSet<i64> = chart
            .indicators
            .iter()
            .filter(|i| i.indicator_type == "ma")
            .filter_map(|i| i.length)
            .collect();
        let ma_count = chart
            .indicators
            .iter()
            .filter(|i| i.indicator_type == "ma")
            .count();
        if ma_count >= MAX_MA_LINES {
            self.last_indicator_error = Some(format!("max {MAX_MA_LINES} MA lines per chart"));
            return;
        }
        let mut added = false;
        for length in DEFAULT_MA_LENGTHS {
            let ma_now = chart
                .indicators
                .iter()
                .filter(|i| i.indicator_type == "ma")
                .count();
            if ma_now >= MAX_MA_LINES {
                break;
            }
            if existing_lengths.contains(&length) {
                continue;
            }
            chart
                .indicators
                .push(IndicatorConfig::ma(format!("ma{length}"), "sma", length));
            added = true;
        }
        // If all defaults present but under max (custom lengths), add a short SMA.
        if !added {
            let ma_now = chart
                .indicators
                .iter()
                .filter(|i| i.indicator_type == "ma")
                .count();
            if ma_now < MAX_MA_LINES {
                let mut length = 20_i64;
                while chart
                    .indicators
                    .iter()
                    .any(|i| i.indicator_type == "ma" && i.length == Some(length))
                {
                    length += 1;
                }
                chart
                    .indicators
                    .push(IndicatorConfig::ma(format!("ma{length}"), "sma", length));
                added = true;
            }
        }
        if !added {
            self.last_indicator_error = Some(format!("max {MAX_MA_LINES} MA lines per chart"));
            return;
        }
        self.last_indicator_error = None;
        self.clamp_indicator_selection();
        self.arm_indicators_apply();
    }

    /// Add Volume instance if none present (max 1).
    pub fn indicator_add_volume(&mut self) {
        if !matches!(self.input_mode, InputMode::IndicatorPanel) {
            return;
        }
        let chart = self.focused_chart_mut();
        if chart
            .indicators
            .iter()
            .any(|i| i.indicator_type == "volume")
        {
            self.last_indicator_error = Some("max 1 Volume per chart".into());
            return;
        }
        chart.indicators.push(IndicatorConfig::volume("volume"));
        self.indicator_selected = chart.indicators.len().saturating_sub(1);
        self.last_indicator_error = None;
        self.arm_indicators_apply();
    }

    /// Add Session Volume Profile if none present (max 1).
    pub fn indicator_add_session_vp(&mut self) {
        if !matches!(self.input_mode, InputMode::IndicatorPanel) {
            return;
        }
        let chart = self.focused_chart_mut();
        let count = chart
            .indicators
            .iter()
            .filter(|i| i.indicator_type == "session_vp")
            .count();
        if count >= MAX_SESSION_VP {
            self.last_indicator_error = Some(format!("max {MAX_SESSION_VP} Session VP per chart"));
            return;
        }
        chart
            .indicators
            .push(IndicatorConfig::session_vp_default("session_vp"));
        self.indicator_selected = chart.indicators.len().saturating_sub(1);
        self.last_indicator_error = None;
        self.arm_indicators_apply();
    }

    /// Add Fixed Range VP (max 4) and enter two-pin placement on the chart.
    ///
    /// Does **not** POST to the engine until both pins are confirmed with Enter.
    pub fn indicator_add_fixed_range_vp(&mut self) {
        if !matches!(
            self.input_mode,
            InputMode::IndicatorPanel | InputMode::Normal
        ) {
            return;
        }
        if matches!(self.input_mode, InputMode::FrvpPlacing) {
            return;
        }
        let bar_count = match &self.focused_chart().series {
            ChartSeriesState::Available { bars } => bars.len(),
            _ => 0,
        };
        if bar_count == 0 {
            self.last_indicator_error =
                Some("Fixed Range VP needs bars on the chart before placing pins".into());
            return;
        }
        let chart = self.focused_chart_mut();
        let count = chart
            .indicators
            .iter()
            .filter(|i| i.indicator_type == "fixed_range_vp")
            .count();
        if count >= MAX_FIXED_RANGE_VP {
            self.last_indicator_error = Some(format!(
                "Fixed Range VP limit is {MAX_FIXED_RANGE_VP} per chart"
            ));
            return;
        }
        // Provisional equal anchors; replaced when both pins lock.
        let id = format!("frvp{}", count + 1);
        let provisional_ts = match &chart.series {
            ChartSeriesState::Available { bars } => bars.last().map(|b| b.ts).unwrap_or(0),
            _ => 0,
        };
        let chart_id = chart.id.clone();
        chart
            .indicators
            .push(IndicatorConfig::fixed_range_vp_default(
                id.clone(),
                provisional_ts,
                provisional_ts,
            ));
        // Disable until placed so we don't draw a junk full-width profile.
        if let Some(last) = chart.indicators.last_mut() {
            last.enabled = false;
        }
        self.indicator_selected = chart.indicators.len().saturating_sub(1);
        let cursor = bar_count.saturating_sub(1);
        self.frvp_place = Some(FrvpPlaceState {
            chart_id,
            indicator_id: id,
            phase: FrvpPinPhase::Start,
            cursor_bar: cursor,
            start_bar: None,
            is_new: true,
        });
        self.input_mode = InputMode::FrvpPlacing;
        self.last_indicator_error = None;
    }

    /// Re-place pins for the selected Fixed Range VP (`r` in the indicator panel).
    pub fn indicator_replace_frvp_pins(&mut self) {
        if !matches!(self.input_mode, InputMode::IndicatorPanel) {
            return;
        }
        let n = self.focused_chart().indicators.len();
        if n == 0 {
            return;
        }
        let idx = self.indicator_selected.min(n - 1);
        let (id, chart_id) = {
            let chart = self.focused_chart();
            let cfg = &chart.indicators[idx];
            if cfg.indicator_type != "fixed_range_vp" {
                return;
            }
            (cfg.id.clone(), chart.id.clone())
        };
        let bar_count = match &self.focused_chart().series {
            ChartSeriesState::Available { bars } => bars.len(),
            _ => 0,
        };
        if bar_count == 0 {
            self.last_indicator_error =
                Some("Fixed Range VP needs bars on the chart before placing pins".into());
            return;
        }
        // Seed cursor near existing start anchor when possible.
        let cursor = {
            let chart = self.focused_chart();
            let start_ts = chart.indicators[idx].start.unwrap_or(0);
            match &chart.series {
                ChartSeriesState::Available { bars } => bars
                    .iter()
                    .enumerate()
                    .min_by_key(|(_, b)| (b.ts - start_ts).abs())
                    .map(|(i, _)| i)
                    .unwrap_or(bar_count.saturating_sub(1)),
                _ => bar_count.saturating_sub(1),
            }
        };
        self.frvp_place = Some(FrvpPlaceState {
            chart_id,
            indicator_id: id,
            phase: FrvpPinPhase::Start,
            cursor_bar: cursor,
            start_bar: None,
            is_new: false,
        });
        self.input_mode = InputMode::FrvpPlacing;
        self.last_indicator_error = None;
    }

    pub fn frvp_place_move(&mut self, delta: i32) {
        let Some(place) = self.frvp_place.as_mut() else {
            return;
        };
        let bar_count = self
            .charts
            .iter()
            .find(|c| c.id == place.chart_id)
            .and_then(|c| match &c.series {
                ChartSeriesState::Available { bars } => Some(bars.len()),
                _ => None,
            })
            .unwrap_or(0);
        if bar_count == 0 {
            return;
        }
        let next = (place.cursor_bar as i32 + delta).clamp(0, bar_count as i32 - 1) as usize;
        place.cursor_bar = next;
    }

    /// Enter locks the current pin; after end pin, enables the FRVP and applies.
    pub fn frvp_place_confirm(&mut self) {
        let Some(place) = self.frvp_place.clone() else {
            return;
        };
        let chart_idx = match self.charts.iter().position(|c| c.id == place.chart_id) {
            Some(i) => i,
            None => {
                self.frvp_place = None;
                self.input_mode = InputMode::Normal;
                return;
            }
        };
        let bars: Vec<OhlcvBar> = match &self.charts[chart_idx].series {
            ChartSeriesState::Available { bars } => bars.clone(),
            _ => {
                self.last_indicator_error = Some("no bars for pin placement".into());
                return;
            }
        };
        if bars.is_empty() || place.cursor_bar >= bars.len() {
            return;
        }

        match place.phase {
            FrvpPinPhase::Start => {
                self.frvp_place = Some(FrvpPlaceState {
                    phase: FrvpPinPhase::End,
                    start_bar: Some(place.cursor_bar),
                    // Nudge cursor one bar right for the end pin when possible.
                    cursor_bar: (place.cursor_bar + 1).min(bars.len() - 1),
                    ..place
                });
            }
            FrvpPinPhase::End => {
                let start_i = place.start_bar.unwrap_or(place.cursor_bar);
                let end_i = place.cursor_bar;
                let (lo, hi) = if start_i <= end_i {
                    (start_i, end_i)
                } else {
                    (end_i, start_i)
                };
                let start_ts = bars[lo].ts;
                let end_ts = bars[hi].ts;
                let ind_id = place.indicator_id.clone();
                let chart_id = place.chart_id.clone();
                let found = self.charts[chart_idx]
                    .indicators
                    .iter_mut()
                    .find(|c| c.id == ind_id);
                if let Some(cfg) = found {
                    cfg.start = Some(start_ts);
                    cfg.end = Some(end_ts);
                    cfg.enabled = true;
                } else {
                    // Draft was wiped mid-placement (interest race) — recreate so pin work is not lost.
                    let mut cfg = IndicatorConfig::fixed_range_vp_default(ind_id, start_ts, end_ts);
                    cfg.enabled = true;
                    self.charts[chart_idx].indicators.push(cfg);
                }
                self.frvp_place = None;
                // Return to the indicator panel after both pins lock.
                self.input_mode = InputMode::IndicatorPanel;
                self.focused = chart_idx;
                // Apply for the chart that owns the pins (not just "focused" if focus drifted).
                self.arm_indicators_apply_for(&chart_id);
                self.last_indicator_error = None;
            }
        }
    }

    pub fn frvp_place_cancel(&mut self) {
        let Some(place) = self.frvp_place.take() else {
            self.input_mode = InputMode::IndicatorPanel;
            return;
        };
        if place.is_new {
            if let Some(chart) = self.charts.iter_mut().find(|c| c.id == place.chart_id) {
                chart.indicators.retain(|c| c.id != place.indicator_id);
                chart.indicator_series.remove(&place.indicator_id);
            }
            self.clamp_indicator_selection();
        }
        self.input_mode = InputMode::IndicatorPanel;
    }

    /// Add Anchored Volume Profile (max 2) with cash-open 09:30 NY default.
    ///
    /// Stays in the indicator panel and applies immediately so the row is
    /// visible. Use `r` to re-place the anchor pin on the chart if needed.
    pub fn indicator_add_anchored_vp(&mut self) {
        if !matches!(self.input_mode, InputMode::IndicatorPanel) {
            return;
        }
        if matches!(
            self.input_mode,
            InputMode::FrvpPlacing | InputMode::AvpPlacing
        ) {
            return;
        }
        let bar_count = match &self.focused_chart().series {
            ChartSeriesState::Available { bars } => bars.len(),
            _ => 0,
        };
        if bar_count == 0 {
            self.last_indicator_error =
                Some("Anchored VP needs bars on the chart (load history first)".into());
            return;
        }
        let chart = self.focused_chart_mut();
        let count = chart
            .indicators
            .iter()
            .filter(|i| i.indicator_type == "anchored_vp")
            .count();
        if count >= MAX_ANCHORED_VP {
            self.last_indicator_error =
                Some(format!("Anchored VP limit is {MAX_ANCHORED_VP} per chart"));
            return;
        }
        let id = format!("avp{}", count + 1);
        // Prefer cash open 09:30 NY on the last bar's day; fall back to first bar.
        let anchor_ts = match &chart.series {
            ChartSeriesState::Available { bars } if !bars.is_empty() => {
                let last_ts = bars.last().map(|b| b.ts).unwrap_or(0);
                let cash = cash_open_ny(last_ts);
                // If cash open is after the whole series, use the first bar.
                if cash > last_ts {
                    bars.first().map(|b| b.ts).unwrap_or(cash)
                } else {
                    cash
                }
            }
            _ => 0,
        };
        chart
            .indicators
            .push(IndicatorConfig::anchored_vp_default(id, anchor_ts));
        self.indicator_selected = chart.indicators.len().saturating_sub(1);
        self.last_indicator_error = None;
        self.arm_indicators_apply();
    }

    /// Add optional GEX (max 1). Engine marks series unavailable without options data.
    pub fn indicator_add_gex(&mut self) {
        if !matches!(self.input_mode, InputMode::IndicatorPanel) {
            return;
        }
        let chart = self.focused_chart_mut();
        let count = chart
            .indicators
            .iter()
            .filter(|i| i.indicator_type == "gex")
            .count();
        if count >= MAX_GEX {
            self.last_indicator_error = Some(format!("GEX limit is {MAX_GEX} per chart"));
            return;
        }
        chart.indicators.push(IndicatorConfig::gex("gex"));
        self.indicator_selected = chart.indicators.len().saturating_sub(1);
        self.last_indicator_error = None;
        self.arm_indicators_apply();
    }

    /// Add optional GARCH (max 1). Engine marks series unavailable without enough history.
    pub fn indicator_add_garch(&mut self) {
        if !matches!(self.input_mode, InputMode::IndicatorPanel) {
            return;
        }
        let chart = self.focused_chart_mut();
        let count = chart
            .indicators
            .iter()
            .filter(|i| i.indicator_type == "garch")
            .count();
        if count >= MAX_GARCH {
            self.last_indicator_error = Some(format!("GARCH limit is {MAX_GARCH} per chart"));
            return;
        }
        chart.indicators.push(IndicatorConfig::garch("garch"));
        self.indicator_selected = chart.indicators.len().saturating_sub(1);
        self.last_indicator_error = None;
        self.arm_indicators_apply();
    }

    /// Remove every indicator on the focused chart except Volume instances.
    /// Clear all indicators except Volume.
    ///
    /// Model 2: bound to **Shift+C / `c` on Current only** (Available `c` is type style).
    pub fn indicator_clear_except_volume(&mut self) {
        if !matches!(self.input_mode, InputMode::IndicatorPanel) {
            return;
        }
        if self.indicator_list_side != IndicatorListSide::Current {
            return;
        }
        if self.type_style_edit.is_some() {
            return;
        }
        let chart = self.focused_chart_mut();
        let before = chart.indicators.len();
        chart.indicators.retain(|i| i.indicator_type == "volume");
        // Drop hot series for removed ids.
        let keep: std::collections::HashSet<String> =
            chart.indicators.iter().map(|i| i.id.clone()).collect();
        chart.indicator_series.retain(|id, _| keep.contains(id));
        if chart.indicators.len() == before {
            return;
        }
        self.clamp_indicator_selection();
        self.arm_indicators_apply();
    }

    /// Re-place the anchor pin for the selected Anchored VP (`r` / Enter in panel).
    pub fn indicator_replace_avp_pin(&mut self) {
        if !matches!(self.input_mode, InputMode::IndicatorPanel) {
            return;
        }
        let n = self.focused_chart().indicators.len();
        if n == 0 {
            return;
        }
        let idx = self.indicator_selected.min(n - 1);
        let (id, chart_id, anchor_ts) = {
            let chart = self.focused_chart();
            let cfg = &chart.indicators[idx];
            if cfg.indicator_type != "anchored_vp" {
                return;
            }
            (cfg.id.clone(), chart.id.clone(), cfg.anchor.unwrap_or(0))
        };
        let bar_count = match &self.focused_chart().series {
            ChartSeriesState::Available { bars } => bars.len(),
            _ => 0,
        };
        if bar_count == 0 {
            self.last_indicator_error =
                Some("Anchored VP needs bars on the chart before placing the anchor".into());
            return;
        }
        let cursor = {
            let chart = self.focused_chart();
            match &chart.series {
                ChartSeriesState::Available { bars } => bars
                    .iter()
                    .enumerate()
                    .min_by_key(|(_, b)| (b.ts - anchor_ts).abs())
                    .map(|(i, _)| i)
                    .unwrap_or(bar_count.saturating_sub(1)),
                _ => bar_count.saturating_sub(1),
            }
        };
        self.avp_place = Some(AvpPlaceState {
            chart_id,
            indicator_id: id,
            cursor_bar: cursor,
            is_new: false,
        });
        self.input_mode = InputMode::AvpPlacing;
        self.last_indicator_error = None;
    }

    pub fn avp_place_move(&mut self, delta: i32) {
        let Some(place) = self.avp_place.as_mut() else {
            return;
        };
        let bar_count = self
            .charts
            .iter()
            .find(|c| c.id == place.chart_id)
            .and_then(|c| match &c.series {
                ChartSeriesState::Available { bars } => Some(bars.len()),
                _ => None,
            })
            .unwrap_or(0);
        if bar_count == 0 {
            return;
        }
        let next = (place.cursor_bar as i32 + delta).clamp(0, bar_count as i32 - 1) as usize;
        place.cursor_bar = next;
    }

    /// Snap the placement cursor to cash open 09:30 America/New_York for the last bar's day.
    pub fn avp_place_snap_cash_open(&mut self) {
        let Some(place) = self.avp_place.as_ref().cloned() else {
            return;
        };
        let chart = match self.charts.iter().find(|c| c.id == place.chart_id) {
            Some(c) => c,
            None => return,
        };
        let bars = match &chart.series {
            ChartSeriesState::Available { bars } if !bars.is_empty() => bars,
            _ => return,
        };
        let last_ts = bars.last().map(|b| b.ts).unwrap_or(0);
        let cash = cash_open_ny(last_ts);
        let cursor = bars
            .iter()
            .enumerate()
            .min_by_key(|(_, b)| (b.ts - cash).abs())
            .map(|(i, _)| i)
            .unwrap_or(bars.len().saturating_sub(1));
        if let Some(p) = self.avp_place.as_mut() {
            p.cursor_bar = cursor;
        }
    }

    /// Enter locks the single anchor pin, enables the AVP, and applies.
    pub fn avp_place_confirm(&mut self) {
        let Some(place) = self.avp_place.clone() else {
            return;
        };
        let chart_idx = match self.charts.iter().position(|c| c.id == place.chart_id) {
            Some(i) => i,
            None => {
                self.avp_place = None;
                self.input_mode = InputMode::Normal;
                return;
            }
        };
        let bars: Vec<OhlcvBar> = match &self.charts[chart_idx].series {
            ChartSeriesState::Available { bars } => bars.clone(),
            _ => {
                self.last_indicator_error = Some("no bars for pin placement".into());
                return;
            }
        };
        if bars.is_empty() || place.cursor_bar >= bars.len() {
            return;
        }
        let anchor_ts = bars[place.cursor_bar].ts;
        let ind_id = place.indicator_id.clone();
        if let Some(cfg) = self.charts[chart_idx]
            .indicators
            .iter_mut()
            .find(|c| c.id == ind_id)
        {
            cfg.anchor = Some(anchor_ts);
            cfg.enabled = true;
        }
        self.avp_place = None;
        // Return to the indicator panel so the AVP row stays visible after pin lock.
        self.input_mode = InputMode::IndicatorPanel;
        self.focused = chart_idx;
        self.arm_indicators_apply();
    }

    pub fn avp_place_cancel(&mut self) {
        let Some(place) = self.avp_place.take() else {
            self.input_mode = InputMode::IndicatorPanel;
            return;
        };
        if place.is_new {
            if let Some(chart) = self.charts.iter_mut().find(|c| c.id == place.chart_id) {
                chart.indicators.retain(|c| c.id != place.indicator_id);
                chart.indicator_series.remove(&place.indicator_id);
            }
            self.clamp_indicator_selection();
        }
        self.input_mode = InputMode::IndicatorPanel;
    }

    /// Nudge Anchored VP anchor by one bar step when selected in the panel.
    pub fn indicator_nudge_avp_anchor(&mut self, delta: i32) {
        if !matches!(self.input_mode, InputMode::IndicatorPanel) || delta == 0 {
            return;
        }
        let n = self.focused_chart().indicators.len();
        if n == 0 {
            return;
        }
        let idx = self.indicator_selected.min(n - 1);
        if self.focused_chart().indicators[idx].indicator_type != "anchored_vp" {
            return;
        }
        let bar_ts: Vec<i64> = match &self.focused_chart().series {
            ChartSeriesState::Available { bars } if !bars.is_empty() => {
                bars.iter().map(|b| b.ts).collect()
            }
            _ => return,
        };
        let cfg = &mut self.focused_chart_mut().indicators[idx];
        let anchor = cfg.anchor.unwrap_or(bar_ts[0]);
        let nearest = |ts: i64| -> usize {
            let mut best_i = 0usize;
            let mut best_d = (bar_ts[0] - ts).abs();
            for (i, &t) in bar_ts.iter().enumerate().skip(1) {
                let d = (t - ts).abs();
                if d < best_d {
                    best_d = d;
                    best_i = i;
                }
            }
            best_i
        };
        let i = (nearest(anchor) as i32 + delta).clamp(0, bar_ts.len() as i32 - 1) as usize;
        let new_anchor = bar_ts[i];
        if Some(new_anchor) == cfg.anchor {
            return;
        }
        cfg.anchor = Some(new_anchor);
        self.arm_indicators_apply();
    }

    /// Snap selected Anchored VP to cash open 09:30 America/New_York (preset).
    pub fn indicator_snap_avp_cash_open(&mut self) {
        if !matches!(self.input_mode, InputMode::IndicatorPanel) {
            return;
        }
        let n = self.focused_chart().indicators.len();
        if n == 0 {
            return;
        }
        let idx = self.indicator_selected.min(n - 1);
        if self.focused_chart().indicators[idx].indicator_type != "anchored_vp" {
            return;
        }
        let last_ts = match &self.focused_chart().series {
            ChartSeriesState::Available { bars } if !bars.is_empty() => {
                bars.last().map(|b| b.ts).unwrap_or(0)
            }
            _ => return,
        };
        let cash = cash_open_ny(last_ts);
        let cfg = &mut self.focused_chart_mut().indicators[idx];
        if cfg.anchor == Some(cash) {
            return;
        }
        cfg.anchor = Some(cash);
        self.arm_indicators_apply();
    }

    pub fn indicator_toggle_selected(&mut self) {
        if !matches!(self.input_mode, InputMode::IndicatorPanel) {
            return;
        }
        // Instance on/off is a Current-list action.
        if self.indicator_list_side != IndicatorListSide::Current {
            return;
        }
        if self.type_style_edit.is_some() {
            return;
        }
        let n = self.focused_chart().indicators.len();
        if n == 0 {
            return;
        }
        let idx = self.indicator_selected.min(n - 1);
        let enabled = self.focused_chart().indicators[idx].enabled;
        self.focused_chart_mut().indicators[idx].enabled = !enabled;
        self.arm_indicators_apply();
    }

    pub fn indicator_remove_selected(&mut self) {
        if !matches!(self.input_mode, InputMode::IndicatorPanel) {
            return;
        }
        if self.indicator_list_side != IndicatorListSide::Current {
            return;
        }
        if self.type_style_edit.is_some() {
            return;
        }
        let n = self.focused_chart().indicators.len();
        if n == 0 {
            return;
        }
        let idx = self.indicator_selected.min(n - 1);
        let chart = self.focused_chart_mut();
        let removed = chart.indicators.remove(idx);
        chart.indicator_series.remove(&removed.id);
        self.clamp_indicator_selection();
        self.arm_indicators_apply();
    }

    pub fn indicator_cycle_ma_type(&mut self) {
        if !matches!(self.input_mode, InputMode::IndicatorPanel) {
            return;
        }
        let n = self.focused_chart().indicators.len();
        if n == 0 {
            return;
        }
        let idx = self.indicator_selected.min(n - 1);
        let cfg = &mut self.focused_chart_mut().indicators[idx];
        if cfg.indicator_type != "ma" {
            return;
        }
        let next = match cfg.ma_type.as_deref() {
            Some("ema") => "sma",
            _ => "ema",
        };
        cfg.ma_type = Some(next.into());
        self.arm_indicators_apply();
    }

    /// Cycle placement left/right when a VP is selected; else MA type.
    pub fn indicator_cycle_style(&mut self) {
        if !matches!(self.input_mode, InputMode::IndicatorPanel) {
            return;
        }
        let n = self.focused_chart().indicators.len();
        if n == 0 {
            return;
        }
        let idx = self.indicator_selected.min(n - 1);
        let itype = self.focused_chart().indicators[idx].indicator_type.clone();
        if itype == "session_vp" || itype == "fixed_range_vp" || itype == "anchored_vp" {
            let cfg = &mut self.focused_chart_mut().indicators[idx];
            let next = match cfg.placement.as_deref() {
                Some("left") => "right",
                _ => "left",
            };
            cfg.placement = Some(next.into());
            self.arm_indicators_apply();
            return;
        }
        self.indicator_cycle_ma_type();
    }

    /// Toggle extend-to-right on the selected Fixed Range VP.
    pub fn indicator_toggle_frvp_extend(&mut self) {
        if !matches!(self.input_mode, InputMode::IndicatorPanel) {
            return;
        }
        let n = self.focused_chart().indicators.len();
        if n == 0 {
            return;
        }
        let idx = self.indicator_selected.min(n - 1);
        let cfg = &mut self.focused_chart_mut().indicators[idx];
        if cfg.indicator_type != "fixed_range_vp" {
            return;
        }
        let cur = cfg.extend_to_right.unwrap_or(false);
        cfg.extend_to_right = Some(!cur);
        self.arm_indicators_apply();
    }

    /// Nudge Fixed Range start (`which=0`) or end (`which=1`) by one bar step.
    pub fn indicator_nudge_frvp_anchor(&mut self, which: u8, delta: i32) {
        if !matches!(self.input_mode, InputMode::IndicatorPanel) || delta == 0 {
            return;
        }
        let n = self.focused_chart().indicators.len();
        if n == 0 {
            return;
        }
        let idx = self.indicator_selected.min(n - 1);
        if self.focused_chart().indicators[idx].indicator_type != "fixed_range_vp" {
            return;
        }
        let bar_ts: Vec<i64> = match &self.focused_chart().series {
            ChartSeriesState::Available { bars } if !bars.is_empty() => {
                bars.iter().map(|b| b.ts).collect()
            }
            _ => return,
        };
        let cfg = &mut self.focused_chart_mut().indicators[idx];
        let start = cfg.start.unwrap_or(bar_ts[0]);
        let end = cfg.end.unwrap_or(*bar_ts.last().unwrap_or(&start));
        let nearest = |ts: i64| -> usize {
            let mut best_i = 0usize;
            let mut best_d = (bar_ts[0] - ts).abs();
            for (i, &t) in bar_ts.iter().enumerate().skip(1) {
                let d = (t - ts).abs();
                if d < best_d {
                    best_d = d;
                    best_i = i;
                }
            }
            best_i
        };
        let clamp_i = |i: i32| -> usize { i.clamp(0, (bar_ts.len() as i32 - 1).max(0)) as usize };
        match which {
            0 => {
                let i = clamp_i(nearest(start) as i32 + delta);
                let mut new_start = bar_ts[i];
                if new_start > end {
                    new_start = end;
                }
                if Some(new_start) == cfg.start {
                    return;
                }
                cfg.start = Some(new_start);
            }
            1 => {
                let i = clamp_i(nearest(end) as i32 + delta);
                let mut new_end = bar_ts[i];
                if new_end < start {
                    new_end = start;
                }
                if Some(new_end) == cfg.end {
                    return;
                }
                cfg.end = Some(new_end);
            }
            _ => return,
        }
        self.arm_indicators_apply();
    }

    pub fn indicator_adjust_length(&mut self, delta: i64) {
        if !matches!(self.input_mode, InputMode::IndicatorPanel) {
            return;
        }
        let n = self.focused_chart().indicators.len();
        if n == 0 {
            return;
        }
        let idx = self.indicator_selected.min(n - 1);
        let cfg = &mut self.focused_chart_mut().indicators[idx];
        if cfg.indicator_type == "ma" {
            let cur = cfg.length.unwrap_or(1).max(1);
            let next = (cur + delta).max(1);
            if next == cur {
                return;
            }
            cfg.length = Some(next);
            self.arm_indicators_apply();
            return;
        }
        if cfg.indicator_type == "session_vp"
            || cfg.indicator_type == "fixed_range_vp"
            || cfg.indicator_type == "anchored_vp"
        {
            // +/- adjusts box width %; [ ] still used for timeframe in panel path.
            let cur = cfg.box_width.unwrap_or(30.0);
            let next = (cur + delta as f64 * 5.0).clamp(5.0, 100.0);
            if (next - cur).abs() < f64::EPSILON {
                return;
            }
            cfg.box_width = Some(next);
            self.arm_indicators_apply();
        }
    }

    /// Toggle POC / VAH / VAL when a VP is selected (`which`: 0/1/2).
    pub fn indicator_toggle_vp_level(&mut self, which: u8) {
        if !matches!(self.input_mode, InputMode::IndicatorPanel) {
            return;
        }
        let n = self.focused_chart().indicators.len();
        if n == 0 {
            return;
        }
        let idx = self.indicator_selected.min(n - 1);
        let cfg = &mut self.focused_chart_mut().indicators[idx];
        if cfg.indicator_type != "session_vp"
            && cfg.indicator_type != "fixed_range_vp"
            && cfg.indicator_type != "anchored_vp"
        {
            return;
        }
        let slot = match which {
            0 => &mut cfg.poc,
            1 => &mut cfg.vah,
            2 => &mut cfg.val,
            _ => return,
        };
        if let Some(style) = slot.as_mut() {
            style.enabled = !style.enabled;
        } else {
            *slot = Some(crate::ipc::LevelStyle {
                enabled: false,
                color: None,
                opacity: Some(1.0),
            });
        }
        self.arm_indicators_apply();
    }

    pub fn apply_indicators_response(&mut self, body: IndicatorsApplyResponse) {
        if let Some(chart) = self.charts.iter_mut().find(|c| c.id == body.chart_id) {
            chart.indicators = body.indicators;
            chart.indicator_series = body.series;
        }
        self.last_indicator_error = None;
        self.clamp_indicator_selection();
    }

    pub fn apply_indicator_update(&mut self, update: IndicatorUpdateEvent) {
        let Some(idx) = self.find_chart_index(
            Some(&update.chart_id),
            &update.instrument,
            &update.timeframe,
        ) else {
            return;
        };
        let chart = &mut self.charts[idx];
        if update.instrument != chart.instrument || update.timeframe != chart.timeframe {
            return;
        }
        // Engine always includes the full config list for this chart (may be empty).
        chart.indicators = update.indicators;
        chart.indicator_series = update.series;
    }

    /// Cycle focused chart timeframe by `delta` steps within [`V1_TIMEFRAMES`] only.
    /// No-op outside workspace normal mode, or when the index would not change.
    pub fn cycle_timeframe(&mut self, delta: i32) {
        if self.screen != Screen::Workspace {
            return;
        }
        // When indicator panel is open, [ ] adjust MA length instead.
        if matches!(self.input_mode, InputMode::IndicatorPanel) {
            self.indicator_adjust_length(delta as i64);
            return;
        }
        if !matches!(self.input_mode, InputMode::Normal) {
            return;
        }
        let current = self.focused_chart().timeframe.clone();
        let Some(idx) = V1_TIMEFRAMES.iter().position(|&tf| tf == current) else {
            // Unknown stored value — snap to default.
            self.focused_chart_mut().timeframe = DEFAULT_TIMEFRAME.to_string();
            self.request_focused_reload();
            return;
        };
        let n = V1_TIMEFRAMES.len() as i32;
        let next = (idx as i32 + delta).rem_euclid(n) as usize;
        let new_tf = V1_TIMEFRAMES[next];
        if new_tf == current {
            return;
        }
        self.focused_chart_mut().timeframe = new_tf.to_string();
        self.request_focused_reload();
    }

    /// Set focused chart instrument. Returns true when interest changed (reload armed).
    pub fn set_instrument(&mut self, raw: &str) -> bool {
        if self.screen != Screen::Workspace {
            return false;
        }
        let symbol = normalize_instrument(raw);
        if symbol.is_empty() || symbol == self.focused_chart().instrument {
            return false;
        }
        self.focused_chart_mut().instrument = symbol;
        self.request_focused_reload();
        true
    }

    fn request_focused_reload(&mut self) {
        let chart = self.focused_chart_mut();
        chart.series = ChartSeriesState::Loading;
        chart.reset_pan();
        self.needs_chart_load = true;
    }

    pub fn begin_instrument_prompt(&mut self) {
        if self.screen != Screen::Workspace {
            return;
        }
        self.input_mode = InputMode::InstrumentPrompt {
            buffer: String::new(),
        };
    }

    pub fn cancel_prompt(&mut self) {
        self.input_mode = InputMode::Normal;
    }

    /// Back-compat alias used by older call sites/tests.
    pub fn cancel_instrument_prompt(&mut self) {
        self.cancel_prompt();
    }

    pub fn prompt_push_char(&mut self, c: char) {
        match &mut self.input_mode {
            InputMode::InstrumentPrompt { buffer } | InputMode::WatchlistAddPrompt { buffer } => {
                // Instruments are alnum; allow `.` and `-` for future vendor symbols.
                if c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_' {
                    if buffer.len() < 16 {
                        buffer.push(c.to_ascii_uppercase());
                    }
                }
            }
            InputMode::WatchlistRenamePrompt { buffer } => {
                // Display names: printable ASCII, including spaces; preserve case.
                if c.is_ascii_graphic() || c == ' ' {
                    if buffer.len() < 40 {
                        buffer.push(c);
                    }
                }
            }
            InputMode::Normal
            | InputMode::IndicatorPanel
            | InputMode::PaperPanel
            | InputMode::FrvpPlacing
            | InputMode::AvpPlacing => {}
        }
    }

    pub fn prompt_pop_char(&mut self) {
        match &mut self.input_mode {
            InputMode::InstrumentPrompt { buffer }
            | InputMode::WatchlistAddPrompt { buffer }
            | InputMode::WatchlistRenamePrompt { buffer } => {
                buffer.pop();
            }
            InputMode::Normal
            | InputMode::IndicatorPanel
            | InputMode::PaperPanel
            | InputMode::FrvpPlacing
            | InputMode::AvpPlacing => {}
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
            IpcEvent::Snapshot {
                feed,
                workspace,
                quotes,
                indicators,
                paper,
            } => {
                self.last_vendor_tick_ts = feed.last_vendor_tick_ts;
                self.set_feed(feed);
                self.apply_paper(paper);
                if let Some(ws) = workspace {
                    if self.screen == Screen::Welcome {
                        // Defer until Enter so Welcome stays clean; still stash for restore.
                        self.pending_workspace = Some(ws);
                        self.pending_quotes = quotes;
                    } else {
                        self.apply_workspace(ws);
                        self.apply_quotes(quotes);
                        self.apply_indicator_payloads(indicators);
                        self.request_chart_load();
                    }
                } else {
                    if !quotes.is_empty() {
                        if self.screen == Screen::Welcome {
                            self.pending_quotes = quotes;
                        } else {
                            self.apply_quotes(quotes);
                        }
                    }
                    if self.screen == Screen::Workspace {
                        self.apply_indicator_payloads(indicators);
                        self.request_chart_load();
                    }
                }
            }
            IpcEvent::FeedStatus {
                status,
                vendor_mode,
                last_vendor_tick_ts,
            } => {
                let engine = self
                    .feed
                    .as_ref()
                    .map(|f| f.engine.clone())
                    .unwrap_or_else(|| "up".into());
                self.last_vendor_tick_ts = last_vendor_tick_ts;
                self.set_feed(FeedSnapshot {
                    status,
                    vendor_mode,
                    engine,
                    last_vendor_tick_ts,
                });
            }
            IpcEvent::Heartbeat {
                ts,
                last_vendor_tick_ts,
            } => {
                self.last_heartbeat_ts = Some(ts);
                self.last_vendor_tick_ts = last_vendor_tick_ts;
                if let Some(feed) = self.feed.as_mut() {
                    feed.last_vendor_tick_ts = last_vendor_tick_ts;
                }
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
            IpcEvent::QuoteUpdate(update) => {
                self.apply_quote_update(update);
            }
            IpcEvent::IndicatorUpdate(update) => {
                self.apply_indicator_update(update);
            }
            IpcEvent::PaperUpdate(paper) => {
                self.apply_paper(paper);
            }
            IpcEvent::IndicatorsApplied(body) => {
                self.apply_indicators_response(body);
            }
            IpcEvent::Workspace(ws) => {
                self.apply_workspace(ws);
                self.request_chart_load();
            }
            IpcEvent::WatchlistState { workspace, quotes } => {
                self.apply_watchlist_state(workspace, quotes);
            }
            IpcEvent::ChartLoadFailed {
                chart_id,
                instrument,
                timeframe,
                message,
            } => {
                self.apply_chart_load_error(&chart_id, &instrument, &timeframe, message);
            }
            IpcEvent::WorkspaceFailed { message: _ } => {
                // Leave layout unchanged; trader can retry toggle.
            }
            IpcEvent::WatchlistFailed { message: _ } => {
                // Leave membership unchanged; trader can retry.
            }
            IpcEvent::PaperFailed { message } => {
                self.last_paper_error = Some(message);
            }
            IpcEvent::IndicatorsFailed { message } => {
                // Leave local indicator draft; surface engine reason (often 422 detail).
                self.last_indicator_error = Some(message);
            }
            IpcEvent::Disconnected { reason } => {
                self.connection = ConnectionStatus::Disconnected { reason };
            }
        }
    }

    pub fn apply_chart_series(&mut self, series: ChartInterestResponse) {
        let target = self.find_chart_index(
            series.chart_id.as_deref(),
            &series.instrument,
            &series.timeframe,
        );
        let Some(idx) = target else {
            return;
        };
        let chart = &mut self.charts[idx];
        // Only apply if it still matches this chart's interest.
        if series.instrument != chart.instrument || series.timeframe != chart.timeframe {
            return;
        }
        // Interest carries the engine config list. Keep local FRVP/AVP that the
        // engine does not yet know about (in-progress pin drafts OR just-confirmed
        // pins whose POST has not completed / interest raced ahead of apply).
        let local_pending_pins: Vec<IndicatorConfig> = chart
            .indicators
            .iter()
            .filter(|i| {
                let is_pin_type =
                    i.indicator_type == "fixed_range_vp" || i.indicator_type == "anchored_vp";
                if !is_pin_type {
                    return false;
                }
                !series.indicators.iter().any(|e| e.id == i.id)
            })
            .cloned()
            .collect();
        chart.indicators = series.indicators;
        chart.indicators.extend(local_pending_pins);
        chart.indicator_series = series.series;
        // Fresh interest payload: re-attach pan to live tip (independent of dual peer).
        chart.reset_pan();
        match series.status.as_str() {
            "ok" => {
                chart.series = ChartSeriesState::Available { bars: series.bars };
            }
            "unavailable" => {
                chart.series = ChartSeriesState::Unavailable;
                chart.indicator_series.clear();
            }
            other => {
                chart.series = ChartSeriesState::Error {
                    message: format!("unexpected chart status: {other}"),
                };
            }
        }
    }

    pub fn apply_chart_load_error(
        &mut self,
        chart_id: &str,
        instrument: &str,
        timeframe: &str,
        message: String,
    ) {
        let Some(idx) = self.find_chart_index(Some(chart_id), instrument, timeframe) else {
            return;
        };
        let chart = &mut self.charts[idx];
        // Ignore stale failures from a prior instrument/timeframe selection.
        if instrument != chart.instrument || timeframe != chart.timeframe {
            return;
        }
        chart.series = ChartSeriesState::Error { message };
    }

    /// Apply a conflated live bar tip (and any completed bars from a period roll).
    pub fn apply_bar_update(&mut self, update: BarUpdateEvent) {
        for chart in &mut self.charts {
            if update.instrument != chart.instrument || update.timeframe != chart.timeframe {
                continue;
            }
            let ChartSeriesState::Available { bars } = &mut chart.series else {
                continue;
            };
            for completed in &update.completed_bars {
                merge_bar(bars, completed.clone());
            }
            merge_bar(bars, update.bar.clone());
        }
    }

    fn find_chart_index(
        &self,
        chart_id: Option<&str>,
        instrument: &str,
        timeframe: &str,
    ) -> Option<usize> {
        if let Some(id) = chart_id {
            if let Some(idx) = self.charts.iter().position(|c| c.id == id) {
                return Some(idx);
            }
        }
        self.charts
            .iter()
            .position(|c| c.instrument == instrument && c.timeframe == timeframe)
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
        match self.focused_chart().series {
            ChartSeriesState::Unavailable => Some(UNAVAILABLE_COPY),
            _ => None,
        }
    }

    pub fn empty_state_copy_for(&self, chart: &Chart) -> Option<&'static str> {
        match chart.series {
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
    use crate::ipc::{PaperDefaults, WorkspaceChartSnapshot};
    use crate::overlay::{DEFAULT_MA_STRENGTH, DEFAULT_VP_STRENGTH};

    #[test]
    fn chart_overlay_strength_defaults_by_type() {
        let chart = Chart::default_single();
        assert_eq!(chart.overlay_strength("ma"), DEFAULT_MA_STRENGTH);
        assert_eq!(chart.overlay_strength("session_vp"), DEFAULT_VP_STRENGTH);
        assert_eq!(
            chart.overlay_strength("fixed_range_vp"),
            DEFAULT_VP_STRENGTH
        );
    }

    fn sample_bars(n: usize, start_ts: i64, period: i64) -> Vec<OhlcvBar> {
        (0..n)
            .map(|i| {
                let px = 100.0 + i as f64;
                OhlcvBar {
                    ts: start_ts + i as i64 * period,
                    open: px,
                    high: px + 1.0,
                    low: px - 1.0,
                    close: px,
                    volume: 1_000.0,
                }
            })
            .collect()
    }

    fn app_with_available_bars(bars: Vec<OhlcvBar>) -> App {
        let mut app = App::default();
        app.enter_workspace();
        app.apply_chart_series(ChartInterestResponse {
            instrument: "SPY".into(),
            timeframe: "1D".into(),
            status: "ok".into(),
            bars,
            chart_id: Some("primary".into()),
            indicators: vec![],
            series: HashMap::new(),
        });
        app
    }

    #[test]
    fn pan_left_moves_over_loaded_bars_and_clamps_at_oldest() {
        let bars = sample_bars(5, 1_000, 60);
        let mut app = app_with_available_bars(bars.clone());
        assert_eq!(app.chart().pan_cursor_ts, None);
        assert!(!app.chart().pan_at_oldest);

        app.pan_focused_chart(-1);
        assert_eq!(app.chart().pan_cursor_ts, Some(bars[3].ts));
        assert!(!app.chart().pan_at_oldest);

        app.pan_focused_chart(-1);
        assert_eq!(app.chart().pan_cursor_ts, Some(bars[2].ts));

        // Jump past the left wall — clamp at oldest loaded bar.
        app.pan_focused_chart(-100);
        assert_eq!(app.chart().pan_cursor_ts, Some(bars[0].ts));
        assert!(app.chart().pan_at_oldest);

        // Further left stays clamped; soft-hint flag remains.
        app.pan_focused_chart(-1);
        assert_eq!(app.chart().pan_cursor_ts, Some(bars[0].ts));
        assert!(app.chart().pan_at_oldest);
    }

    #[test]
    fn pan_right_to_tip_reattaches_live() {
        let bars = sample_bars(4, 1_000, 60);
        let mut app = app_with_available_bars(bars.clone());
        app.pan_focused_chart(-2);
        assert_eq!(app.chart().pan_cursor_ts, Some(bars[1].ts));

        app.pan_focused_chart(1);
        assert_eq!(app.chart().pan_cursor_ts, Some(bars[2].ts));
        assert!(!app.chart().pan_at_oldest);

        // Reach newest loaded bar → clear cursor so live updates stick to the tip.
        app.pan_focused_chart(1);
        assert_eq!(app.chart().pan_cursor_ts, None);
        assert!(!app.chart().pan_at_oldest);

        // Extra right stays live-attached.
        app.pan_focused_chart(5);
        assert_eq!(app.chart().pan_cursor_ts, None);
    }

    #[test]
    fn pan_is_independent_per_dual_chart() {
        let mut app = App::default();
        app.enter_workspace();
        app.apply_workspace(WorkspaceSnapshot {
            layout_mode: "dual-vertical".into(),
            charts: vec![
                WorkspaceChartSnapshot {
                    id: "top".into(),
                    instrument: "QQQ".into(),
                    timeframe: "1D".into(),
                    indicators: vec![],
                    type_styles: HashMap::new(),
                },
                WorkspaceChartSnapshot {
                    id: "bottom".into(),
                    instrument: "SPY".into(),
                    timeframe: "1D".into(),
                    indicators: vec![],
                    type_styles: HashMap::new(),
                },
            ],
            watchlists: vec![],
            active_watchlist_id: String::new(),
        });
        let top_bars = sample_bars(5, 2_000, 60);
        let bottom_bars = sample_bars(5, 3_000, 60);
        app.apply_chart_series(ChartInterestResponse {
            instrument: "QQQ".into(),
            timeframe: "1D".into(),
            status: "ok".into(),
            bars: top_bars.clone(),
            chart_id: Some("top".into()),
            indicators: vec![],
            series: HashMap::new(),
        });
        app.apply_chart_series(ChartInterestResponse {
            instrument: "SPY".into(),
            timeframe: "1D".into(),
            status: "ok".into(),
            bars: bottom_bars.clone(),
            chart_id: Some("bottom".into()),
            indicators: vec![],
            series: HashMap::new(),
        });
        assert_eq!(app.focused, 0);
        app.pan_focused_chart(-2);
        assert_eq!(app.charts[0].pan_cursor_ts, Some(top_bars[2].ts));
        assert_eq!(app.charts[1].pan_cursor_ts, None);

        app.focus_next();
        assert_eq!(app.focused, 1);
        app.pan_focused_chart(-1);
        assert_eq!(app.charts[0].pan_cursor_ts, Some(top_bars[2].ts));
        assert_eq!(app.charts[1].pan_cursor_ts, Some(bottom_bars[3].ts));
    }

    #[test]
    fn pan_noop_when_indicator_panel_open() {
        let bars = sample_bars(5, 1_000, 60);
        let mut app = app_with_available_bars(bars);
        app.input_mode = InputMode::IndicatorPanel;
        app.pan_focused_chart(-1);
        assert_eq!(app.chart().pan_cursor_ts, None);
    }

    #[test]
    fn watchlist_nav_blocked_when_indicator_panel_open() {
        let mut app = App::default();
        app.enter_workspace();
        app.apply_workspace(WorkspaceSnapshot {
            layout_mode: "single".into(),
            charts: vec![WorkspaceChartSnapshot {
                id: "primary".into(),
                instrument: "SPY".into(),
                timeframe: "1D".into(),
                indicators: vec![],
                type_styles: HashMap::new(),
            }],
            watchlists: vec![WatchlistSnapshot {
                id: "wl1".into(),
                name: "Core".into(),
                symbols: vec!["SPY".into(), "QQQ".into(), "IWM".into()],
            }],
            active_watchlist_id: "wl1".into(),
        });
        app.watchlist_visible = true;
        app.watchlist_selected = 0;
        app.input_mode = InputMode::IndicatorPanel;

        app.watchlist_select_delta(1);
        assert_eq!(
            app.watchlist_selected, 0,
            "↑↓ must not move watchlist under panel"
        );
        assert!(!app.load_selected_watchlist_symbol());
        assert_eq!(app.focused_chart().instrument, "SPY");
    }

    #[test]
    fn pin_placement_left_right_moves_pin_not_pan() {
        let bars = sample_bars(5, 1_000, 60);
        let mut app = app_with_available_bars(bars);
        // Simulate FRVP pin placement mode (owns ← →).
        app.input_mode = InputMode::FrvpPlacing;
        app.frvp_place = Some(FrvpPlaceState {
            chart_id: "primary".into(),
            indicator_id: "frvp-1".into(),
            phase: FrvpPinPhase::Start,
            cursor_bar: 2,
            start_bar: None,
            is_new: true,
        });

        app.pan_focused_chart(-1);
        assert_eq!(
            app.chart().pan_cursor_ts,
            None,
            "pan must no-op in pin mode"
        );

        app.frvp_place_move(-1);
        assert_eq!(
            app.frvp_place.as_ref().map(|p| p.cursor_bar),
            Some(1),
            "← moves pin cursor"
        );
        assert_eq!(app.chart().pan_cursor_ts, None);
    }

    #[test]
    fn forming_bar_countdown_label_from_available_series() {
        // 1m tip open at 1_000; at now=1_025 remaining is 35s → "0:35".
        let bars = sample_bars(3, 1_000, 60);
        let mut chart = Chart::default_single();
        chart.timeframe = "1m".into();
        chart.series = ChartSeriesState::Available { bars: bars.clone() };
        assert_eq!(
            chart.forming_bar_countdown_label(1_000 + 2 * 60 + 25),
            Some("0:35".into())
        );
        // Loading / empty: no invented countdown.
        chart.series = ChartSeriesState::Loading;
        assert_eq!(chart.forming_bar_countdown_label(1_200), None);
        chart.series = ChartSeriesState::Available { bars: vec![] };
        assert_eq!(chart.forming_bar_countdown_label(1_200), None);
    }

    #[test]
    fn dual_charts_have_independent_countdowns() {
        // Both tips open at t=10_000; now is 10s into the forming bar.
        let mut top = Chart::default_dual_top();
        top.timeframe = "1m".into();
        top.series = ChartSeriesState::Available {
            bars: sample_bars(2, 9_940, 60), // tip open 10_000
        };
        let mut bottom = Chart::default_dual_bottom();
        bottom.timeframe = "5m".into();
        bottom.series = ChartSeriesState::Available {
            bars: sample_bars(2, 9_700, 300), // tip open 10_000
        };
        let now = 10_010;
        // 1m → 50s left; 5m → 290s left (independent of dual peer).
        assert_eq!(top.forming_bar_countdown_label(now), Some("0:50".into()));
        assert_eq!(bottom.forming_bar_countdown_label(now), Some("4:50".into()));
        let top_title = top.chrome_title(true, now);
        let bottom_title = bottom.chrome_title(false, now);
        assert!(top_title.contains("0:50"), "{top_title}");
        assert!(bottom_title.contains("4:50"), "{bottom_title}");
        assert!(!top_title.contains("4:50"));
    }

    #[test]
    fn live_bar_update_keeps_panned_cursor() {
        let bars = sample_bars(3, 1_000, 60);
        let mut app = app_with_available_bars(bars.clone());
        app.pan_focused_chart(-1);
        let pinned = app.chart().pan_cursor_ts;
        assert_eq!(pinned, Some(bars[1].ts));

        app.apply_bar_update(BarUpdateEvent {
            instrument: "SPY".into(),
            timeframe: "1D".into(),
            completed_bars: vec![],
            bar: OhlcvBar {
                ts: bars[2].ts,
                open: 999.0,
                high: 1000.0,
                low: 998.0,
                close: 999.5,
                volume: 50.0,
            },
        });
        // Still panned away from tip — cursor unchanged while series tip updates.
        assert_eq!(app.chart().pan_cursor_ts, pinned);
        match &app.chart().series {
            ChartSeriesState::Available { bars: b } => assert_eq!(b[2].close, 999.5),
            other => panic!("expected Available, got {other:?}"),
        }
    }

    #[test]
    fn chart_set_overlay_strength_clamps_and_persists_per_type() {
        let mut chart = Chart::default_single();
        chart.set_overlay_strength("ma", 0.4);
        chart.set_overlay_strength("session_vp", 2.0);
        assert_eq!(chart.overlay_strength("ma"), 0.4);
        assert_eq!(chart.overlay_strength("session_vp"), 1.0);
        // Independent per type.
        assert_eq!(chart.overlay_strength("anchored_vp"), DEFAULT_VP_STRENGTH);
        let map = chart.overlay_strength_map();
        assert_eq!(map.get("ma"), Some(&0.4));
        assert_eq!(map.get("session_vp"), Some(&1.0));
    }

    #[test]
    fn set_chart_overlay_strength_arms_engine_persist() {
        let mut app = App::default();
        app.enter_workspace();
        app.set_chart_overlay_strength("primary", "ma", 0.33);
        assert_eq!(app.charts[0].overlay_strength("ma"), 0.33);
        let pending = app.pending_type_styles.expect("armed");
        assert_eq!(pending.chart_id, "primary");
        assert_eq!(
            pending.type_styles.get("ma").map(|s| s.overlay_strength),
            Some(0.33)
        );
    }

    #[test]
    fn apply_workspace_restores_type_styles() {
        let mut styles = HashMap::new();
        styles.insert("ma".into(), IndicatorTypeStyle::with_strength(0.55));
        let mut app = App::default();
        app.apply_workspace(WorkspaceSnapshot {
            layout_mode: "single".into(),
            charts: vec![WorkspaceChartSnapshot {
                id: "primary".into(),
                instrument: "SPY".into(),
                timeframe: "1D".into(),
                indicators: vec![],
                type_styles: styles,
            }],
            watchlists: vec![],
            active_watchlist_id: String::new(),
        });
        assert_eq!(app.charts[0].overlay_strength("ma"), 0.55);
        assert_eq!(
            app.charts[0].overlay_strength("session_vp"),
            DEFAULT_VP_STRENGTH
        );
    }

    #[test]
    fn welcome_enters_workspace_with_default_spy_1d() {
        let mut app = App::default();
        assert_eq!(app.screen, Screen::Welcome);
        assert_eq!(app.layout, LayoutMode::Single);
        assert_eq!(app.chart().instrument, "SPY");
        assert_eq!(app.chart().timeframe, "1D");
        app.enter_workspace();
        assert_eq!(app.screen, Screen::Workspace);
        assert!(app.needs_chart_load);
        assert_eq!(app.chart().series, ChartSeriesState::Loading);
    }

    #[test]
    fn help_overlay_toggles_without_changing_input_mode() {
        let mut app = App::default();
        app.enter_workspace();
        app.input_mode = InputMode::IndicatorPanel;
        assert!(!app.help_open);
        app.toggle_help();
        assert!(app.help_open);
        assert_eq!(app.input_mode, InputMode::IndicatorPanel);
        app.close_help();
        assert!(!app.help_open);
        assert_eq!(app.input_mode, InputMode::IndicatorPanel);
    }

    #[test]
    fn snapshot_marks_connected_feed_with_fake_vendor() {
        let mut app = App::default();
        app.apply_ipc(IpcEvent::Snapshot {
            feed: FeedSnapshot {
                status: "connected".into(),
                vendor_mode: "fake".into(),
                engine: "up".into(),
                last_vendor_tick_ts: None,
            },
            workspace: None,
            quotes: vec![],
            indicators: HashMap::new(),
            paper: PaperSnapshot::default(),
        });
        assert_eq!(app.connection, ConnectionStatus::Connected);
        assert_eq!(app.vendor_mode_label(), "fake");
        assert_eq!(app.last_vendor_tick_ts, None);
    }

    #[test]
    fn snapshot_and_heartbeat_record_last_vendor_tick_ts() {
        let mut app = App::default();
        app.apply_ipc(IpcEvent::Snapshot {
            feed: FeedSnapshot {
                status: "connected".into(),
                vendor_mode: "lse".into(),
                engine: "up".into(),
                last_vendor_tick_ts: Some(1_719_790_800.0),
            },
            workspace: None,
            quotes: vec![],
            indicators: HashMap::new(),
            paper: PaperSnapshot::default(),
        });
        assert_eq!(app.last_vendor_tick_ts, Some(1_719_790_800.0));

        app.apply_ipc(IpcEvent::Heartbeat {
            ts: 1_719_792_400.0,
            last_vendor_tick_ts: Some(1_719_790_830.0),
        });
        assert_eq!(app.last_heartbeat_ts, Some(1_719_792_400.0));
        assert_eq!(app.last_vendor_tick_ts, Some(1_719_790_830.0));
        assert_eq!(
            app.feed.as_ref().and_then(|f| f.last_vendor_tick_ts),
            Some(1_719_790_830.0)
        );
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
            chart_id: Some("primary".into()),
            indicators: vec![],
            series: HashMap::new(),
        });
        match &app.chart().series {
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
            chart_id: Some("primary".into()),
            indicators: vec![],
            series: HashMap::new(),
        });
        assert_eq!(app.chart().series, ChartSeriesState::Unavailable);
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
            chart_id: Some("other".into()),
            indicators: vec![],
            series: HashMap::new(),
        });
        assert_eq!(app.chart().series, ChartSeriesState::Loading);
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
            chart_id: Some("primary".into()),
            indicators: vec![],
            series: HashMap::new(),
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
        match &app.chart().series {
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
            chart_id: Some("primary".into()),
            indicators: vec![],
            series: HashMap::new(),
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
        match &app.chart().series {
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
            chart_id: Some("primary".into()),
            indicators: vec![],
            series: HashMap::new(),
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
        match &app.chart().series {
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
            chart_id: Some("primary".into()),
            indicators: vec![],
            series: HashMap::new(),
        });
        assert_eq!(app.chart().timeframe, "1D");

        app.cycle_timeframe(1);
        assert_eq!(app.chart().timeframe, "1W");
        assert!(app.needs_chart_load);
        assert_eq!(app.chart().series, ChartSeriesState::Loading);

        app.chart_load_started();
        app.cycle_timeframe(1);
        assert_eq!(app.chart().timeframe, "1m");
    }

    #[test]
    fn cycle_timeframe_prev_from_1d_goes_to_4h() {
        let mut app = App::default();
        app.enter_workspace();
        app.chart_load_started();

        app.cycle_timeframe(-1);
        assert_eq!(app.chart().timeframe, "4h");
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
            chart_id: Some("primary".into()),
            indicators: vec![],
            series: HashMap::new(),
        });

        assert!(app.set_instrument("qqq"));
        assert_eq!(app.chart().instrument, "QQQ");
        assert_eq!(app.chart().timeframe, "1D");
        assert!(app.needs_chart_load);
        assert_eq!(app.chart().series, ChartSeriesState::Loading);
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
        assert_eq!(app.chart().instrument, "SPY");
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
        assert_eq!(app.chart().instrument, "ES");
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
        assert_eq!(app.chart().instrument, "SPY");
        assert!(!app.needs_chart_load);
    }

    #[test]
    fn cycle_timeframe_ignored_during_instrument_prompt() {
        let mut app = App::default();
        app.enter_workspace();
        app.chart_load_started();
        app.begin_instrument_prompt();
        app.cycle_timeframe(1);
        assert_eq!(app.chart().timeframe, "1D");
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
            chart_id: Some("primary".into()),
            indicators: vec![],
            series: HashMap::new(),
        });
        // Late failure from previous SPY interest must not clobber QQQ.
        app.apply_chart_load_error("primary", "SPY", "1D", "timeout".into());
        match &app.chart().series {
            ChartSeriesState::Available { bars } => assert_eq!(bars[0].close, 1.0),
            other => panic!("expected Available, got {other:?}"),
        }
    }

    #[test]
    fn snapshot_workspace_restored_on_enter_workspace() {
        let mut app = App::default();
        app.apply_ipc(IpcEvent::Snapshot {
            feed: FeedSnapshot {
                status: "connected".into(),
                vendor_mode: "fake".into(),
                engine: "up".into(),
                last_vendor_tick_ts: None,
            },
            workspace: Some(WorkspaceSnapshot {
                layout_mode: "dual-vertical".into(),
                charts: vec![
                    WorkspaceChartSnapshot {
                        id: "top".into(),
                        instrument: "ES".into(),
                        timeframe: "1D".into(),
                        indicators: vec![],
                        type_styles: HashMap::new(),
                    },
                    WorkspaceChartSnapshot {
                        id: "bottom".into(),
                        instrument: "QQQ".into(),
                        timeframe: "1h".into(),
                        indicators: vec![],
                        type_styles: HashMap::new(),
                    },
                ],
                watchlists: vec![
                    WatchlistSnapshot {
                        id: "core".into(),
                        name: "Core".into(),
                        symbols: vec!["SPY".into(), "QQQ".into()],
                    },
                    WatchlistSnapshot {
                        id: "focus".into(),
                        name: "Focus".into(),
                        symbols: vec![],
                    },
                ],
                active_watchlist_id: "core".into(),
            }),
            quotes: vec![QuoteRow {
                symbol: "SPY".into(),
                status: "ok".into(),
                last: Some(548.0),
                previous_close: Some(546.25),
                change: Some(1.75),
                change_pct: Some(1.75 / 546.25),
            }],
            indicators: HashMap::new(),
            paper: PaperSnapshot::default(),
        });
        // Still on Welcome with defaults until Enter.
        assert_eq!(app.screen, Screen::Welcome);
        assert_eq!(app.layout, LayoutMode::Single);

        app.enter_workspace();
        assert_eq!(app.layout, LayoutMode::DualVertical);
        assert_eq!(app.charts.len(), 2);
        assert_eq!(app.charts[0].id, "top");
        assert_eq!(app.charts[0].instrument, "ES");
        assert_eq!(app.charts[0].timeframe, "1D");
        assert_eq!(app.charts[1].id, "bottom");
        assert_eq!(app.charts[1].instrument, "QQQ");
        assert_eq!(app.charts[1].timeframe, "1h");
        assert!(app.needs_chart_load);
        assert_eq!(app.active_watchlist_id, "core");
        assert_eq!(
            app.active_symbols(),
            &["SPY".to_string(), "QQQ".to_string()]
        );
        assert_eq!(app.quote_for("SPY").map(|q| q.last), Some(Some(548.0)));
    }

    #[test]
    fn toggle_layout_arms_pending_layout() {
        let mut app = App::default();
        app.enter_workspace();
        app.chart_load_started();
        app.toggle_layout();
        assert_eq!(app.pending_layout, Some(LayoutMode::DualVertical));
        app.layout_request_started();
        assert!(app.pending_layout.is_none());
    }

    #[test]
    fn workspace_event_applies_dual_defaults() {
        let mut app = App::default();
        app.enter_workspace();
        app.chart_load_started();
        app.apply_ipc(IpcEvent::Workspace(dual_workspace()));
        assert_eq!(app.layout, LayoutMode::DualVertical);
        assert_eq!(app.charts[0].instrument, "QQQ");
        assert_eq!(app.charts[1].instrument, "SPY");
        assert!(app.needs_chart_load);
    }

    #[test]
    fn focus_next_cycles_dual_charts() {
        let mut app = App::default();
        app.enter_workspace();
        app.apply_workspace(dual_workspace());
        assert_eq!(app.focused, 0);
        app.focus_next();
        assert_eq!(app.focused, 1);
        assert_eq!(app.focused_chart().id, "bottom");
        app.focus_next();
        assert_eq!(app.focused, 0);
    }

    #[test]
    fn set_instrument_only_changes_focused_dual_chart() {
        let mut app = App::default();
        app.enter_workspace();
        app.apply_workspace(dual_workspace());
        app.chart_load_started();
        app.focus_next(); // bottom
        assert!(app.set_instrument("ES"));
        assert_eq!(app.charts[0].instrument, "QQQ");
        assert_eq!(app.charts[1].instrument, "ES");
    }

    fn core_watchlist_workspace() -> WorkspaceSnapshot {
        WorkspaceSnapshot {
            layout_mode: "single".into(),
            charts: vec![WorkspaceChartSnapshot {
                id: "primary".into(),
                instrument: "SPY".into(),
                timeframe: "1h".into(),
                indicators: vec![],
                type_styles: HashMap::new(),
            }],
            watchlists: vec![
                WatchlistSnapshot {
                    id: "core".into(),
                    name: "Core".into(),
                    symbols: vec!["ES".into(), "NQ".into(), "SPY".into()],
                },
                WatchlistSnapshot {
                    id: "focus".into(),
                    name: "Focus".into(),
                    symbols: vec!["QQQ".into()],
                },
            ],
            active_watchlist_id: "core".into(),
        }
    }

    #[test]
    fn load_selected_watchlist_symbol_sets_focused_instrument_keeps_tf_and_indicators() {
        let mut app = App::default();
        app.enter_workspace();
        app.apply_workspace(core_watchlist_workspace());
        app.chart_load_started();
        // Seed indicators on focused chart — must survive symbol switch.
        app.input_mode = InputMode::IndicatorPanel;
        app.indicator_add_default_ma_stack();
        app.input_mode = InputMode::Normal;
        let indicator_ids: Vec<String> = app
            .focused_chart()
            .indicators
            .iter()
            .map(|i| i.id.clone())
            .collect();
        assert!(!indicator_ids.is_empty());
        assert_eq!(app.focused_chart().timeframe, "1h");
        assert_eq!(app.focused_chart().instrument, "SPY");

        app.watchlist_selected = 0; // ES
        assert!(app.load_selected_watchlist_symbol());
        assert_eq!(app.focused_chart().instrument, "ES");
        assert_eq!(app.focused_chart().timeframe, "1h");
        let after: Vec<String> = app
            .focused_chart()
            .indicators
            .iter()
            .map(|i| i.id.clone())
            .collect();
        assert_eq!(after, indicator_ids);
        assert!(app.needs_chart_load);
    }

    #[test]
    fn load_selected_watchlist_symbol_only_changes_focused_dual_chart() {
        let mut app = App::default();
        app.enter_workspace();
        app.apply_workspace(dual_workspace());
        // Attach a multi-list with symbols for selection.
        app.apply_workspace(WorkspaceSnapshot {
            layout_mode: "dual-vertical".into(),
            charts: vec![
                WorkspaceChartSnapshot {
                    id: "top".into(),
                    instrument: "QQQ".into(),
                    timeframe: "1D".into(),
                    indicators: vec![],
                    type_styles: HashMap::new(),
                },
                WorkspaceChartSnapshot {
                    id: "bottom".into(),
                    instrument: "SPY".into(),
                    timeframe: "1D".into(),
                    indicators: vec![],
                    type_styles: HashMap::new(),
                },
            ],
            watchlists: vec![WatchlistSnapshot {
                id: "core".into(),
                name: "Core".into(),
                symbols: vec!["ES".into(), "NQ".into()],
            }],
            active_watchlist_id: "core".into(),
        });
        app.chart_load_started();
        app.focus_next(); // bottom
        app.watchlist_selected = 0; // ES
        assert!(app.load_selected_watchlist_symbol());
        assert_eq!(app.charts[0].instrument, "QQQ");
        assert_eq!(app.charts[1].instrument, "ES");
    }

    #[test]
    fn load_selected_watchlist_symbol_inactive_outside_normal_workspace_sidebar() {
        let mut app = App::default();
        app.enter_workspace();
        app.apply_workspace(core_watchlist_workspace());
        app.chart_load_started();
        app.watchlist_selected = 0;

        app.input_mode = InputMode::IndicatorPanel;
        assert!(!app.load_selected_watchlist_symbol());
        assert_eq!(app.focused_chart().instrument, "SPY");

        app.input_mode = InputMode::Normal;
        app.watchlist_visible = false;
        assert!(!app.load_selected_watchlist_symbol());
        assert_eq!(app.focused_chart().instrument, "SPY");

        app.watchlist_visible = true;
        app.input_mode = InputMode::FrvpPlacing;
        assert!(!app.load_selected_watchlist_symbol());

        app.input_mode = InputMode::WatchlistAddPrompt {
            buffer: String::new(),
        };
        assert!(!app.load_selected_watchlist_symbol());
    }

    #[test]
    fn rename_prompt_arms_pending_rename_and_rejects_empty() {
        let mut app = App::default();
        app.enter_workspace();
        app.apply_workspace(core_watchlist_workspace());

        app.begin_watchlist_rename_prompt();
        assert!(matches!(
            app.input_mode,
            InputMode::WatchlistRenamePrompt { ref buffer } if buffer == "Core"
        ));

        // Clear to empty → reject, stay in prompt.
        if let InputMode::WatchlistRenamePrompt { buffer } = &mut app.input_mode {
            buffer.clear();
        }
        assert!(!app.apply_watchlist_rename_prompt());
        assert!(matches!(
            app.input_mode,
            InputMode::WatchlistRenamePrompt { .. }
        ));
        assert!(app.pending_watchlist.is_none());

        // Type a new name with spaces (preserve case).
        if let InputMode::WatchlistRenamePrompt { buffer } = &mut app.input_mode {
            buffer.clear();
        }
        for c in "Day desk".chars() {
            app.prompt_push_char(c);
        }
        assert!(app.apply_watchlist_rename_prompt());
        assert_eq!(app.input_mode, InputMode::Normal);
        assert_eq!(
            app.pending_watchlist,
            Some(PendingWatchlistOp::Rename {
                name: "Day desk".into()
            })
        );
    }

    #[test]
    fn rename_prompt_inactive_when_indicator_panel_open() {
        let mut app = App::default();
        app.enter_workspace();
        app.apply_workspace(core_watchlist_workspace());
        app.input_mode = InputMode::IndicatorPanel;
        app.begin_watchlist_rename_prompt();
        assert_eq!(app.input_mode, InputMode::IndicatorPanel);
        assert!(app.pending_watchlist.is_none());
    }

    #[test]
    fn apply_watchlist_state_updates_names_without_wiping_chart_series() {
        let mut app = App::default();
        app.enter_workspace();
        app.apply_workspace(core_watchlist_workspace());
        app.chart_load_started();
        app.apply_chart_series(ChartInterestResponse {
            instrument: "SPY".into(),
            timeframe: "1h".into(),
            status: "ok".into(),
            bars: vec![OhlcvBar {
                ts: 1,
                open: 1.0,
                high: 1.0,
                low: 1.0,
                close: 1.0,
                volume: 1.0,
            }],
            chart_id: Some("primary".into()),
            indicators: vec![],
            series: HashMap::new(),
        });
        assert!(matches!(
            app.focused_chart().series,
            ChartSeriesState::Available { .. }
        ));

        app.apply_watchlist_state(
            WorkspaceSnapshot {
                layout_mode: "single".into(),
                charts: vec![WorkspaceChartSnapshot {
                    id: "primary".into(),
                    instrument: "SPY".into(),
                    timeframe: "1h".into(),
                    indicators: vec![],
                    type_styles: HashMap::new(),
                }],
                watchlists: vec![
                    WatchlistSnapshot {
                        id: "core".into(),
                        name: "Day desk".into(),
                        symbols: vec!["ES".into(), "NQ".into(), "SPY".into()],
                    },
                    WatchlistSnapshot {
                        id: "focus".into(),
                        name: "Focus".into(),
                        symbols: vec![],
                    },
                ],
                active_watchlist_id: "core".into(),
            },
            vec![],
        );
        assert_eq!(
            app.active_watchlist().map(|w| w.name.as_str()),
            Some("Day desk")
        );
        assert!(matches!(
            app.focused_chart().series,
            ChartSeriesState::Available { .. }
        ));
        assert_eq!(app.focused_chart().instrument, "SPY");
        assert_eq!(app.focused_chart().timeframe, "1h");
        assert!(!app.needs_chart_load);
    }

    #[test]
    fn dual_bar_update_routes_to_matching_chart_only() {
        let mut app = App::default();
        app.enter_workspace();
        app.apply_workspace(dual_workspace());
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
            chart_id: Some("top".into()),
            indicators: vec![],
            series: HashMap::new(),
        });
        app.apply_chart_series(ChartInterestResponse {
            instrument: "SPY".into(),
            timeframe: "1D".into(),
            status: "ok".into(),
            bars: vec![OhlcvBar {
                ts: 1,
                open: 2.0,
                high: 2.0,
                low: 2.0,
                close: 2.0,
                volume: 2.0,
            }],
            chart_id: Some("bottom".into()),
            indicators: vec![],
            series: HashMap::new(),
        });
        app.apply_bar_update(BarUpdateEvent {
            instrument: "QQQ".into(),
            timeframe: "1D".into(),
            completed_bars: vec![],
            bar: OhlcvBar {
                ts: 1,
                open: 1.0,
                high: 3.0,
                low: 1.0,
                close: 3.0,
                volume: 10.0,
            },
        });
        match &app.charts[0].series {
            ChartSeriesState::Available { bars } => assert_eq!(bars[0].close, 3.0),
            other => panic!("top expected Available, got {other:?}"),
        }
        match &app.charts[1].series {
            ChartSeriesState::Available { bars } => assert_eq!(bars[0].close, 2.0),
            other => panic!("bottom expected Available, got {other:?}"),
        }
    }

    #[test]
    fn toggle_watchlist_sidebar() {
        let mut app = App::default();
        app.enter_workspace();
        assert!(app.watchlist_visible);
        app.toggle_watchlist_sidebar();
        assert!(!app.watchlist_visible);
        app.toggle_watchlist_sidebar();
        assert!(app.watchlist_visible);
    }

    #[test]
    fn cycle_watchlist_arms_set_active() {
        let mut app = App::default();
        app.enter_workspace();
        app.apply_workspace(WorkspaceSnapshot {
            layout_mode: "single".into(),
            charts: vec![WorkspaceChartSnapshot {
                id: "primary".into(),
                instrument: "SPY".into(),
                timeframe: "1D".into(),
                indicators: vec![],
                type_styles: HashMap::new(),
            }],
            watchlists: vec![
                WatchlistSnapshot {
                    id: "core".into(),
                    name: "Core".into(),
                    symbols: vec!["SPY".into()],
                },
                WatchlistSnapshot {
                    id: "focus".into(),
                    name: "Focus".into(),
                    symbols: vec!["QQQ".into()],
                },
            ],
            active_watchlist_id: "core".into(),
        });
        app.cycle_watchlist(1);
        assert_eq!(
            app.pending_watchlist,
            Some(PendingWatchlistOp::SetActive {
                watchlist_id: "focus".into()
            })
        );
    }

    #[test]
    fn quote_update_mutates_last_and_change() {
        let mut app = App::default();
        app.enter_workspace();
        app.apply_quotes(vec![QuoteRow {
            symbol: "SPY".into(),
            status: "ok".into(),
            last: Some(548.0),
            previous_close: Some(546.25),
            change: Some(1.75),
            change_pct: Some(1.75 / 546.25),
        }]);
        app.apply_quote_update(QuoteUpdateEvent {
            symbol: "SPY".into(),
            status: "ok".into(),
            last: Some(550.0),
            previous_close: Some(546.25),
            change: Some(3.75),
            change_pct: Some(3.75 / 546.25),
        });
        let q = app.quote_for("SPY").expect("spy quote");
        assert_eq!(q.last, Some(550.0));
        assert_eq!(q.change, Some(3.75));
        assert!(q.change.unwrap() > 0.0);
    }

    #[test]
    fn remove_selected_arms_remove_op() {
        let mut app = App::default();
        app.enter_workspace();
        app.apply_workspace(WorkspaceSnapshot {
            layout_mode: "single".into(),
            charts: vec![WorkspaceChartSnapshot {
                id: "primary".into(),
                instrument: "SPY".into(),
                timeframe: "1D".into(),
                indicators: vec![],
                type_styles: HashMap::new(),
            }],
            watchlists: vec![WatchlistSnapshot {
                id: "core".into(),
                name: "Core".into(),
                symbols: vec!["ES".into(), "SPY".into()],
            }],
            active_watchlist_id: "core".into(),
        });
        app.watchlist_selected = 1;
        app.remove_selected_watchlist_symbol();
        assert_eq!(
            app.pending_watchlist,
            Some(PendingWatchlistOp::Remove {
                symbol: "SPY".into()
            })
        );
    }

    #[test]
    fn cash_open_ny_july_2024_is_0930_edt() {
        // 2024-07-01 10:00 ET (EDT) → cash open same day 09:30 ET.
        let ten_am_et = 1_719_842_400_i64;
        let expected_cash = 1_719_840_600_i64; // 2024-07-01 09:30 ET
        assert_eq!(cash_open_ny(ten_am_et), expected_cash);
    }

    #[test]
    fn anchored_vp_add_stays_in_panel_and_applies_with_cash_open() {
        let mut app = App::default();
        app.enter_workspace();
        app.chart_load_started();
        // 2024-07-01 10:00 ET bars so cash open 09:30 is on that day.
        let bars: Vec<OhlcvBar> = (0..5)
            .map(|i| OhlcvBar {
                ts: 1_719_842_400 + i * 60,
                open: 100.0,
                high: 101.0,
                low: 99.0,
                close: 100.5,
                volume: 1_000.0,
            })
            .collect();
        // Chart default is SPY@1D — interest response must match or bars are ignored.
        app.focused_chart_mut().timeframe = "1m".into();
        app.apply_chart_series(ChartInterestResponse {
            status: "ok".into(),
            instrument: "SPY".into(),
            timeframe: "1m".into(),
            chart_id: Some("primary".into()),
            bars: bars.clone(),
            indicators: vec![],
            series: HashMap::new(),
        });
        app.input_mode = InputMode::IndicatorPanel;
        app.indicator_add_anchored_vp();
        // Immediate apply — stay in panel; no forced pin mode.
        assert_eq!(app.input_mode, InputMode::IndicatorPanel);
        assert!(app.avp_place.is_none());
        let av = app
            .focused_chart()
            .indicators
            .iter()
            .find(|i| i.indicator_type == "anchored_vp")
            .expect("avp added");
        assert!(av.enabled);
        assert_eq!(av.rows, Some(500));
        assert_eq!(av.anchor, Some(cash_open_ny(bars[0].ts)));
        assert!(app.pending_indicators.is_some());
    }

    #[test]
    fn anchored_vp_re_pin_returns_to_panel_on_confirm() {
        let mut app = App::default();
        app.enter_workspace();
        app.chart_load_started();
        let bars: Vec<OhlcvBar> = (0..5)
            .map(|i| OhlcvBar {
                ts: 1_700_000_000 + i * 60,
                open: 100.0,
                high: 101.0,
                low: 99.0,
                close: 100.5,
                volume: 1_000.0,
            })
            .collect();
        app.apply_chart_series(ChartInterestResponse {
            status: "ok".into(),
            instrument: "SPY".into(),
            timeframe: "1D".into(),
            chart_id: Some("primary".into()),
            bars: bars.clone(),
            indicators: vec![],
            series: HashMap::new(),
        });
        app.input_mode = InputMode::IndicatorPanel;
        app.indicator_add_anchored_vp();
        app.pending_indicators = None;
        app.indicator_replace_avp_pin();
        assert_eq!(app.input_mode, InputMode::AvpPlacing);
        app.avp_place_move(-100);
        app.avp_place_move(2);
        app.avp_place_confirm();
        assert!(app.avp_place.is_none());
        assert_eq!(app.input_mode, InputMode::IndicatorPanel);
        let av = app
            .focused_chart()
            .indicators
            .iter()
            .find(|i| i.indicator_type == "anchored_vp")
            .expect("avp still present");
        assert!(av.enabled);
        assert_eq!(av.anchor, Some(bars[2].ts));
        assert!(app.pending_indicators.is_some());
    }

    #[test]
    fn clear_except_volume_keeps_only_volume() {
        let mut app = App::default();
        app.enter_workspace();
        app.input_mode = InputMode::IndicatorPanel;
        let chart = app.focused_chart_mut();
        chart
            .indicators
            .push(IndicatorConfig::ma("ma10", "sma", 10));
        chart.indicators.push(IndicatorConfig::volume("volume"));
        chart
            .indicators
            .push(IndicatorConfig::session_vp_default("svp"));
        chart
            .indicators
            .push(IndicatorConfig::anchored_vp_default("avp", 1_700_000_000));
        app.indicator_clear_except_volume();
        let inds = &app.focused_chart().indicators;
        assert_eq!(inds.len(), 1);
        assert_eq!(inds[0].indicator_type, "volume");
        assert!(app.pending_indicators.is_some());
    }

    #[test]
    fn fixed_range_vp_two_pin_placement_locks_start_then_end() {
        let mut app = App::default();
        app.enter_workspace();
        app.chart_load_started();
        let bars: Vec<OhlcvBar> = (0..5)
            .map(|i| OhlcvBar {
                ts: 1_700_000_000 + i * 60,
                open: 100.0,
                high: 101.0,
                low: 99.0,
                close: 100.5,
                volume: 1_000.0,
            })
            .collect();
        app.apply_chart_series(ChartInterestResponse {
            status: "ok".into(),
            instrument: "SPY".into(),
            timeframe: "1D".into(),
            chart_id: Some("primary".into()),
            bars: bars.clone(),
            indicators: vec![],
            series: HashMap::new(),
        });
        app.input_mode = InputMode::IndicatorPanel;
        app.indicator_add_fixed_range_vp();
        assert_eq!(app.input_mode, InputMode::FrvpPlacing);
        assert!(app.frvp_place.is_some());
        // New FRVP is disabled until both pins lock.
        let fr = app
            .focused_chart()
            .indicators
            .iter()
            .find(|i| i.indicator_type == "fixed_range_vp")
            .expect("frvp added");
        assert!(!fr.enabled);
        assert!(app.pending_indicators.is_none());

        // Move to bar 1 and lock start.
        app.frvp_place_move(-100); // clamp to 0
        app.frvp_place_move(1);
        app.frvp_place_confirm();
        assert_eq!(
            app.frvp_place.as_ref().map(|p| p.phase),
            Some(FrvpPinPhase::End)
        );
        assert_eq!(app.frvp_place.as_ref().and_then(|p| p.start_bar), Some(1));

        // After start lock, cursor auto-advances one bar (to index 2); nudge once to 3.
        app.frvp_place_move(1);
        app.frvp_place_confirm();
        assert!(app.frvp_place.is_none());
        assert_eq!(app.input_mode, InputMode::IndicatorPanel);
        let fr = app
            .focused_chart()
            .indicators
            .iter()
            .find(|i| i.indicator_type == "fixed_range_vp")
            .expect("frvp still present");
        assert!(fr.enabled);
        assert_eq!(fr.start, Some(bars[1].ts));
        assert_eq!(fr.end, Some(bars[3].ts));
        assert!(app.pending_indicators.is_some());
        let pending = app.pending_indicators.as_ref().unwrap();
        assert_eq!(pending.chart_id, "primary");
        assert!(
            pending
                .indicators
                .iter()
                .any(|i| i.indicator_type == "fixed_range_vp" && i.enabled),
            "armed apply must include enabled FRVP"
        );
    }

    #[test]
    fn frvp_survives_chart_interest_race_after_confirm() {
        // After both pins lock, a stale interest response without FRVP must not drop it.
        let mut app = App::default();
        app.enter_workspace();
        app.chart_load_started();
        let bars: Vec<OhlcvBar> = (0..8)
            .map(|i| OhlcvBar {
                ts: 1_700_000_000 + i * 60,
                open: 100.0,
                high: 101.0,
                low: 99.0,
                close: 100.5,
                volume: 1_000.0,
            })
            .collect();
        app.apply_chart_series(ChartInterestResponse {
            status: "ok".into(),
            instrument: "SPY".into(),
            timeframe: "1D".into(),
            chart_id: Some("primary".into()),
            bars: bars.clone(),
            indicators: vec![IndicatorConfig::volume("volume")],
            series: HashMap::new(),
        });
        app.input_mode = InputMode::IndicatorPanel;
        app.indicator_add_fixed_range_vp();
        app.frvp_place_move(-100);
        app.frvp_place_confirm(); // start
        app.frvp_place_move(3);
        app.frvp_place_confirm(); // end
        assert!(
            app.focused_chart()
                .indicators
                .iter()
                .any(|i| i.indicator_type == "fixed_range_vp" && i.enabled)
        );

        // Stale interest: engine still only knows volume (POST not completed).
        app.apply_chart_series(ChartInterestResponse {
            status: "ok".into(),
            instrument: "SPY".into(),
            timeframe: "1D".into(),
            chart_id: Some("primary".into()),
            bars: bars.clone(),
            indicators: vec![IndicatorConfig::volume("volume")],
            series: HashMap::new(),
        });
        let fr = app
            .focused_chart()
            .indicators
            .iter()
            .find(|i| i.indicator_type == "fixed_range_vp")
            .expect("FRVP must survive interest race");
        assert!(fr.enabled);
        assert!(fr.start.is_some() && fr.end.is_some());
    }

    #[test]
    fn fixed_range_vp_cancel_drops_new_draft() {
        let mut app = App::default();
        app.enter_workspace();
        app.chart_load_started();
        app.apply_chart_series(ChartInterestResponse {
            status: "ok".into(),
            instrument: "SPY".into(),
            timeframe: "1D".into(),
            chart_id: Some("primary".into()),
            bars: vec![OhlcvBar {
                ts: 1,
                open: 1.0,
                high: 1.0,
                low: 1.0,
                close: 1.0,
                volume: 1.0,
            }],
            indicators: vec![],
            series: HashMap::new(),
        });
        app.input_mode = InputMode::IndicatorPanel;
        app.indicator_add_fixed_range_vp();
        assert_eq!(app.focused_chart().indicators.len(), 1);
        app.frvp_place_cancel();
        assert!(app.focused_chart().indicators.is_empty());
        assert_eq!(app.input_mode, InputMode::IndicatorPanel);
    }

    // --- Indicator panel Model 2 (Available | Current) -----------------------

    #[test]
    fn indicator_panel_opens_with_current_list_active() {
        let mut app = App::default();
        app.enter_workspace();
        app.indicator_list_side = IndicatorListSide::Available;
        app.toggle_indicator_panel();
        assert_eq!(app.input_mode, InputMode::IndicatorPanel);
        assert_eq!(app.indicator_list_side, IndicatorListSide::Current);
    }

    #[test]
    fn indicator_tab_switches_available_and_current() {
        let mut app = App::default();
        app.enter_workspace();
        app.input_mode = InputMode::IndicatorPanel;
        assert_eq!(app.indicator_list_side, IndicatorListSide::Current);
        app.indicator_toggle_list_side();
        assert_eq!(app.indicator_list_side, IndicatorListSide::Available);
        app.indicator_toggle_list_side();
        assert_eq!(app.indicator_list_side, IndicatorListSide::Current);
    }

    #[test]
    fn available_enter_adds_selected_type_respecting_max() {
        let mut app = App::default();
        app.enter_workspace();
        app.input_mode = InputMode::IndicatorPanel;
        app.indicator_list_side = IndicatorListSide::Available;
        // volume is catalog index 1
        app.indicator_available_selected = 1;
        app.indicator_activate_selected();
        assert_eq!(app.focused_chart().indicators.len(), 1);
        assert_eq!(app.focused_chart().indicators[0].indicator_type, "volume");
        assert!(app.pending_indicators.is_some());
        // max 1 volume — second add is a no-op
        app.pending_indicators = None;
        app.indicator_activate_selected();
        assert_eq!(app.focused_chart().indicators.len(), 1);
        assert!(app.pending_indicators.is_none());
    }

    #[test]
    fn available_ma_add_fills_default_stack_up_to_max() {
        let mut app = App::default();
        app.enter_workspace();
        app.input_mode = InputMode::IndicatorPanel;
        app.indicator_list_side = IndicatorListSide::Available;
        app.indicator_available_selected = 0; // ma
        app.indicator_activate_selected();
        let mas: Vec<_> = app
            .focused_chart()
            .indicators
            .iter()
            .filter(|i| i.indicator_type == "ma")
            .collect();
        assert_eq!(mas.len(), MAX_MA_LINES);
        app.pending_indicators = None;
        app.indicator_activate_selected();
        assert_eq!(
            app.focused_chart()
                .indicators
                .iter()
                .filter(|i| i.indicator_type == "ma")
                .count(),
            MAX_MA_LINES
        );
        assert!(app.pending_indicators.is_none());
    }

    #[test]
    fn available_c_opens_type_style_confirm_persists_strength() {
        let mut app = App::default();
        app.enter_workspace();
        app.input_mode = InputMode::IndicatorPanel;
        app.indicator_list_side = IndicatorListSide::Available;
        app.indicator_available_selected = 0; // ma
        app.indicator_open_type_style();
        let edit = app.type_style_edit.as_ref().expect("popup open");
        assert_eq!(edit.indicator_type, "ma");
        assert!((edit.strength - DEFAULT_MA_STRENGTH).abs() < 1e-9);

        app.type_style_nudge(-4); // 0.05 * 4
        let draft = app.type_style_edit.as_ref().unwrap().strength;
        assert!((draft - (DEFAULT_MA_STRENGTH - 0.20)).abs() < 1e-9);
        // Cancel does not persist.
        app.type_style_cancel();
        assert!(app.type_style_edit.is_none());
        assert!((app.focused_chart().overlay_strength("ma") - DEFAULT_MA_STRENGTH).abs() < 1e-9);
        assert!(app.pending_type_styles.is_none());

        app.indicator_open_type_style();
        app.type_style_nudge(-4);
        app.type_style_confirm();
        assert!(app.type_style_edit.is_none());
        assert!(
            (app.focused_chart().overlay_strength("ma") - (DEFAULT_MA_STRENGTH - 0.20)).abs()
                < 1e-9
        );
        let pending = app.pending_type_styles.expect("armed for engine");
        assert_eq!(pending.chart_id, "primary");
        assert!(
            (pending
                .type_styles
                .get("ma")
                .map(|s| s.overlay_strength)
                .unwrap()
                - (DEFAULT_MA_STRENGTH - 0.20))
                .abs()
                < 1e-9
        );
    }

    #[test]
    fn type_style_not_opened_from_current_list() {
        let mut app = App::default();
        app.enter_workspace();
        app.input_mode = InputMode::IndicatorPanel;
        app.indicator_list_side = IndicatorListSide::Current;
        app.indicator_open_type_style();
        assert!(app.type_style_edit.is_none());
    }

    #[test]
    fn current_activate_toggles_on_off_available_does_not() {
        let mut app = App::default();
        app.enter_workspace();
        app.input_mode = InputMode::IndicatorPanel;
        app.indicator_list_side = IndicatorListSide::Current;
        app.focused_chart_mut()
            .indicators
            .push(IndicatorConfig::volume("volume"));
        assert!(app.focused_chart().indicators[0].enabled);
        app.indicator_activate_selected();
        assert!(!app.focused_chart().indicators[0].enabled);

        // On Available, activate adds (not toggle of Current row).
        app.indicator_list_side = IndicatorListSide::Available;
        app.indicator_available_selected = 2; // session_vp
        app.indicator_activate_selected();
        assert!(
            app.focused_chart()
                .indicators
                .iter()
                .any(|i| i.indicator_type == "session_vp")
        );
    }

    #[test]
    fn clear_except_volume_only_on_current_side() {
        let mut app = App::default();
        app.enter_workspace();
        app.input_mode = InputMode::IndicatorPanel;
        let chart = app.focused_chart_mut();
        chart
            .indicators
            .push(IndicatorConfig::ma("ma10", "sma", 10));
        chart.indicators.push(IndicatorConfig::volume("volume"));
        app.indicator_list_side = IndicatorListSide::Available;
        app.indicator_clear_except_volume();
        assert_eq!(app.focused_chart().indicators.len(), 2);
        assert!(app.pending_indicators.is_none());

        app.indicator_list_side = IndicatorListSide::Current;
        app.indicator_clear_except_volume();
        assert_eq!(app.focused_chart().indicators.len(), 1);
        assert_eq!(app.focused_chart().indicators[0].indicator_type, "volume");
        assert!(app.pending_indicators.is_some());
    }

    #[test]
    fn indicator_select_delta_routes_by_active_list() {
        let mut app = App::default();
        app.enter_workspace();
        app.input_mode = InputMode::IndicatorPanel;
        app.indicator_list_side = IndicatorListSide::Available;
        app.indicator_available_selected = 0;
        app.indicator_select_delta(2);
        assert_eq!(app.indicator_available_selected, 2);

        app.indicator_list_side = IndicatorListSide::Current;
        app.focused_chart_mut()
            .indicators
            .push(IndicatorConfig::volume("volume"));
        app.focused_chart_mut()
            .indicators
            .push(IndicatorConfig::ma("ma10", "sma", 10));
        app.indicator_selected = 0;
        app.indicator_select_delta(1);
        assert_eq!(app.indicator_selected, 1);
        // Available selection unchanged while navigating Current.
        assert_eq!(app.indicator_available_selected, 2);
    }

    fn dual_workspace() -> WorkspaceSnapshot {
        WorkspaceSnapshot {
            layout_mode: "dual-vertical".into(),
            charts: vec![
                WorkspaceChartSnapshot {
                    id: "top".into(),
                    instrument: "QQQ".into(),
                    timeframe: "1D".into(),
                    indicators: vec![],
                    type_styles: HashMap::new(),
                },
                WorkspaceChartSnapshot {
                    id: "bottom".into(),
                    instrument: "SPY".into(),
                    timeframe: "1D".into(),
                    indicators: vec![],
                    type_styles: HashMap::new(),
                },
            ],
            watchlists: vec![],
            active_watchlist_id: String::new(),
        }
    }

    fn sample_paper_desk() -> PaperSnapshot {
        PaperSnapshot {
            active_account_id: "pa_1".into(),
            accounts: vec![
                PaperAccountSnapshot {
                    id: "pa_1".into(),
                    name: "Paper".into(),
                    currency: "USD".into(),
                    initial_balance: 100_000.0,
                    balance: 100_000.0,
                    commission_per_fill_usd: 1.0,
                    leverage_enabled: false,
                    leverage_multiple: 1.0,
                    asset_class_restriction: None,
                },
                PaperAccountSnapshot {
                    id: "pa_2".into(),
                    name: "Scalps".into(),
                    currency: "USD".into(),
                    initial_balance: 25_000.0,
                    balance: 25_000.0,
                    commission_per_fill_usd: 2.0,
                    leverage_enabled: true,
                    leverage_multiple: 4.0,
                    asset_class_restriction: None,
                },
            ],
            defaults: PaperDefaults {
                name: "Paper".into(),
                currency: "USD".into(),
                initial_balance: 100_000.0,
                commission_per_fill_usd: 1.0,
                leverage_enabled: false,
                leverage_multiple: 1.0,
            },
            working_orders: vec![],
            positions: vec![],
            filled_order_history: vec![],
            balance_history: vec![],
        }
    }

    fn sample_working_order(
        id: &str,
        instrument: &str,
        side: &str,
        order_type: &str,
        qty: f64,
        limit: Option<f64>,
        stop: Option<f64>,
    ) -> WorkingOrderSnapshot {
        WorkingOrderSnapshot {
            id: id.into(),
            account_id: "pa_1".into(),
            instrument: instrument.into(),
            side: side.into(),
            order_type: order_type.into(),
            qty,
            limit,
            stop,
            placed_ts: 1_719_792_000,
        }
    }

    #[test]
    fn snapshot_without_paper_is_empty_desk() {
        let mut app = App::default();
        app.apply_ipc(IpcEvent::Snapshot {
            feed: FeedSnapshot {
                status: "connected".into(),
                vendor_mode: "fake".into(),
                engine: "up".into(),
                last_vendor_tick_ts: None,
            },
            workspace: None,
            quotes: vec![],
            indicators: HashMap::new(),
            paper: PaperSnapshot::default(),
        });
        assert!(app.paper.accounts.is_empty());
        assert!(app.active_paper_account().is_none());
        assert!(app.paper.positions.is_empty());
        assert!(app.paper.filled_order_history.is_empty());
        assert!(app.paper.balance_history.is_empty());
        assert!(app.paper.working_orders.is_empty());
    }

    #[test]
    fn paper_update_event_replaces_desk() {
        let mut app = App::default();
        app.apply_ipc(IpcEvent::PaperUpdate(sample_paper_desk()));
        assert_eq!(
            app.active_paper_account().map(|a| a.name.as_str()),
            Some("Paper")
        );
        assert_eq!(
            app.active_paper_account().map(|a| a.balance),
            Some(100_000.0)
        );
    }

    #[test]
    fn paper_panel_shows_active_account_settings_and_empty_tables() {
        let mut app = App::default();
        app.enter_workspace();
        app.apply_paper(sample_paper_desk());

        let active = app.active_paper_account().expect("active paper account");
        assert_eq!(active.name, "Paper");
        assert_eq!(active.balance, 100_000.0);
        assert_eq!(active.currency, "USD");
        assert_eq!(active.initial_balance, 100_000.0);
        assert_eq!(active.commission_per_fill_usd, 1.0);
        assert!(!active.leverage_enabled);
        assert_eq!(active.leverage_multiple, 1.0);
        assert_eq!(app.paper.defaults.name, "Paper");
        assert_eq!(app.paper.defaults.initial_balance, 100_000.0);
        assert_eq!(app.paper.defaults.commission_per_fill_usd, 1.0);
        assert_eq!(app.paper.defaults.leverage_multiple, 1.0);
        assert!(app.paper.positions.is_empty());
        assert!(app.paper.filled_order_history.is_empty());
        assert!(app.paper.balance_history.is_empty());

        app.apply_paper(PaperSnapshot {
            active_account_id: "pa_2".into(),
            ..sample_paper_desk()
        });
        let active = app.active_paper_account().expect("active paper account");
        assert_eq!(active.name, "Scalps");
        assert_eq!(active.balance, 25_000.0);

        app.apply_paper(PaperSnapshot {
            active_account_id: "missing".into(),
            ..sample_paper_desk()
        });
        assert!(app.active_paper_account().is_none());
    }

    #[test]
    fn paper_panel_toggle_owns_input_focus() {
        let bars = sample_bars(5, 1_000, 60);
        let mut app = app_with_available_bars(bars);
        app.apply_workspace(WorkspaceSnapshot {
            layout_mode: "single".into(),
            charts: vec![WorkspaceChartSnapshot {
                id: "primary".into(),
                instrument: "SPY".into(),
                timeframe: "1D".into(),
                indicators: vec![],
                type_styles: HashMap::new(),
            }],
            watchlists: vec![WatchlistSnapshot {
                id: "wl1".into(),
                name: "Core".into(),
                symbols: vec!["SPY".into(), "QQQ".into(), "IWM".into()],
            }],
            active_watchlist_id: "wl1".into(),
        });
        app.watchlist_visible = true;
        app.watchlist_selected = 0;

        app.toggle_paper_panel();
        assert_eq!(app.input_mode, InputMode::PaperPanel);

        app.watchlist_select_delta(1);
        assert_eq!(
            app.watchlist_selected, 0,
            "↑↓ must not move watchlist while paper panel owns focus"
        );
        app.pan_focused_chart(-1);
        assert_eq!(app.chart().pan_cursor_ts, None);
        assert!(!app.load_selected_watchlist_symbol());
        assert_eq!(app.focused_chart().instrument, "SPY");

        app.toggle_paper_panel();
        assert_eq!(app.input_mode, InputMode::Normal);
        app.watchlist_select_delta(1);
        assert_eq!(app.watchlist_selected, 1);
    }

    #[test]
    fn order_side_panel_place_defaults_to_focused_chart_instrument() {
        let mut app = App::default();
        app.enter_workspace();
        app.apply_workspace(dual_workspace());
        app.focused = 0;
        app.toggle_paper_panel();
        assert_eq!(app.order_side_instrument(), "QQQ");
        app.order_side.kind = WorkingOrderKind::Limit;
        app.order_side.qty = 3.0;
        app.order_side.limit = 480.0;
        app.paper_submit();
        assert_eq!(
            app.pending_paper,
            Some(PendingPaperOp::Place {
                instrument: "QQQ".into(),
                side: "buy".into(),
                order_type: "limit".into(),
                qty: 3.0,
                limit: Some(480.0),
                stop: None,
            })
        );

        app.pending_paper = None;
        app.close_paper_panel();
        app.focused = 1;
        app.toggle_paper_panel();
        assert_eq!(app.order_side_instrument(), "SPY");
        app.paper_set_kind(WorkingOrderKind::Market);
        app.paper_submit();
        assert_eq!(
            app.pending_paper,
            Some(PendingPaperOp::Place {
                instrument: "SPY".into(),
                side: "buy".into(),
                order_type: "market".into(),
                qty: 1.0,
                limit: None,
                stop: None,
            })
        );
    }

    #[test]
    fn order_side_panel_modify_and_cancel_selected_working_order() {
        let mut app = App::default();
        app.enter_workspace();
        let mut desk = sample_paper_desk();
        desk.working_orders = vec![sample_working_order(
            "wo_1",
            "SPY",
            "buy",
            "limit",
            10.0,
            Some(540.0),
            None,
        )];
        app.apply_paper(desk);
        app.toggle_paper_panel();
        app.paper_select_working_delta(1);
        assert_eq!(app.order_side.selected_order_id.as_deref(), Some("wo_1"));
        assert_eq!(app.order_side.qty, 10.0);
        assert_eq!(app.order_side.limit, 540.0);

        app.paper_nudge_qty(-6);
        app.paper_nudge_price(50);
        app.paper_submit();
        assert_eq!(
            app.pending_paper,
            Some(PendingPaperOp::Modify {
                order_id: "wo_1".into(),
                qty: Some(4.0),
                limit: Some(540.5),
                stop: None,
            })
        );

        app.pending_paper = None;
        app.paper_cancel_selected();
        assert_eq!(
            app.pending_paper,
            Some(PendingPaperOp::Cancel {
                order_id: "wo_1".into(),
            })
        );
    }

    #[test]
    fn working_overlay_lines_only_on_matching_instrument() {
        let mut app = App::default();
        let mut desk = sample_paper_desk();
        desk.working_orders = vec![
            sample_working_order("wo_spy_l", "SPY", "buy", "limit", 1.0, Some(540.0), None),
            sample_working_order("wo_spy_s", "SPY", "sell", "stop", 1.0, None, Some(530.0)),
            sample_working_order("wo_spy_m", "SPY", "buy", "market", 1.0, None, None),
            sample_working_order("wo_qqq", "QQQ", "buy", "limit", 1.0, Some(480.0), None),
        ];
        app.apply_paper(desk);

        let spy = app.working_overlay_levels("SPY", 0.0, 10.0);
        assert_eq!(spy.len(), 2);
        assert!(spy.iter().all(|l| l.type_key == "working_order"));
        assert_eq!(spy[0].price, 540.0);
        assert_eq!(spy[1].price, 530.0);
        let qqq = app.working_overlay_levels("QQQ", 0.0, 10.0);
        assert_eq!(qqq.len(), 1);
        assert_eq!(qqq[0].price, 480.0);
        assert!(app.working_overlay_levels("IWM", 0.0, 10.0).is_empty());
    }

    #[test]
    fn paper_panel_keys_idle_until_open() {
        let mut app = App::default();
        app.enter_workspace();
        app.paper_cycle_side();
        app.paper_submit();
        assert_eq!(app.order_side.side, OrderSide::Buy);
        assert!(app.pending_paper.is_none());
        app.toggle_paper_panel();
        app.paper_cycle_side();
        assert_eq!(app.order_side.side, OrderSide::Sell);
    }

    #[test]
    fn order_side_panel_reports_missing_limit_instead_of_silent_noop() {
        let mut app = App::default();
        app.enter_workspace();
        app.toggle_paper_panel();
        app.paper_set_kind(WorkingOrderKind::Limit);
        app.order_side.limit = 0.0;
        app.paper_submit();
        assert!(app.pending_paper.is_none());
        assert_eq!(app.last_paper_error.as_deref(), Some("limit is required"));
    }

    #[test]
    fn order_side_panel_locks_type_while_a_working_order_is_selected() {
        let mut app = App::default();
        app.enter_workspace();
        let mut desk = sample_paper_desk();
        desk.working_orders = vec![sample_working_order(
            "wo_1",
            "SPY",
            "buy",
            "limit",
            10.0,
            Some(540.0),
            None,
        )];
        app.apply_paper(desk);
        app.toggle_paper_panel();
        app.paper_select_working_delta(1);
        app.paper_set_kind(WorkingOrderKind::Stop);
        app.paper_set_side(OrderSide::Sell);
        assert_eq!(app.order_side.kind, WorkingOrderKind::Limit);
        assert_eq!(app.order_side.side, OrderSide::Buy);
    }
}
