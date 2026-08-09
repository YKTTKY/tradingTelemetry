//! Application state: Welcome → workspace with single or dual-vertical charts.

use std::collections::HashMap;

use crate::ipc::{
    BarUpdateEvent, ChartIndicatorsPayload, ChartInterestResponse, FeedSnapshot, IpcEvent,
    IndicatorConfig, IndicatorSeriesData, IndicatorUpdateEvent, IndicatorsApplyResponse, OhlcvBar,
    QuoteRow, QuoteUpdateEvent, WatchlistSnapshot, WorkspaceSnapshot,
};

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

/// Modal input for instrument selection and watchlist add.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputMode {
    Normal,
    InstrumentPrompt { buffer: String },
    WatchlistAddPrompt { buffer: String },
    /// Indicator panel for the focused chart (add / toggle / configure).
    IndicatorPanel,
}

/// Pending HTTP mutation against the engine watchlist API.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PendingWatchlistOp {
    SetActive { watchlist_id: String },
    Add { symbol: String },
    Remove { symbol: String },
}

/// Pending full-replace indicator apply for one chart.
#[derive(Debug, Clone, PartialEq)]
pub struct PendingIndicatorsApply {
    pub chart_id: String,
    pub indicators: Vec<IndicatorConfig>,
}

pub const MAX_MA_LINES: usize = 3;
pub const MAX_SESSION_VP: usize = 1;
pub const MAX_FIXED_RANGE_VP: usize = 4;
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
    Available { bars: Vec<OhlcvBar> },
    Unavailable,
    Error { message: String },
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
}

impl Chart {
    pub fn new(id: impl Into<String>, instrument: impl Into<String>, timeframe: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            instrument: instrument.into(),
            timeframe: timeframe.into(),
            series: ChartSeriesState::Idle,
            indicators: Vec::new(),
            indicator_series: HashMap::new(),
        }
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
            .filter_map(|cfg| {
                self.indicator_series
                    .get(&cfg.id)
                    .map(|s| (cfg, s))
            })
            .collect()
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
    /// Selected row in the indicator panel list.
    pub indicator_selected: usize,
    pub input_mode: InputMode,
    /// Floating keyboard-shortcut help overlay (does not replace input_mode).
    pub help_open: bool,
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
            needs_chart_load: false,
            pending_layout: None,
            watchlist_visible: true,
            watchlists: Vec::new(),
            active_watchlist_id: String::new(),
            quotes: HashMap::new(),
            watchlist_selected: 0,
            pending_watchlist: None,
            pending_indicators: None,
            indicator_selected: 0,
            input_mode: InputMode::Normal,
            help_open: false,
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
        self.pending_watchlist = Some(PendingWatchlistOp::SetActive {
            watchlist_id: id,
        });
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
        self.quotes
            .insert(update.symbol.clone(), update.to_row());
    }

    pub fn apply_watchlist_state(
        &mut self,
        workspace: WorkspaceSnapshot,
        quotes: Vec<QuoteRow>,
    ) {
        self.apply_workspace(workspace);
        // Replace quote map for symbols we still care about; keep others for dual-list cache.
        self.apply_quotes(quotes);
        self.clamp_watchlist_selection();
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
        }
        self.needs_chart_load = true;
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

    pub fn apply_indicator_payloads(
        &mut self,
        payloads: HashMap<String, ChartIndicatorsPayload>,
    ) {
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
                self.clamp_indicator_selection();
            }
            InputMode::IndicatorPanel => {
                self.input_mode = InputMode::Normal;
            }
            _ => {}
        }
    }

    pub fn close_indicator_panel(&mut self) {
        if matches!(self.input_mode, InputMode::IndicatorPanel) {
            self.input_mode = InputMode::Normal;
        }
    }

    pub fn indicator_select_delta(&mut self, delta: i32) {
        if !matches!(self.input_mode, InputMode::IndicatorPanel) {
            return;
        }
        let n = self.focused_chart().indicators.len();
        if n == 0 {
            self.indicator_selected = 0;
            return;
        }
        let cur = self.indicator_selected.min(n - 1) as i32;
        self.indicator_selected = (cur + delta).rem_euclid(n as i32) as usize;
    }

    fn arm_indicators_apply(&mut self) {
        let chart = self.focused_chart();
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
            chart.indicators.push(IndicatorConfig::ma(format!("ma{length}"), "sma", length));
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
            return;
        }
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
            return;
        }
        chart.indicators.push(IndicatorConfig::volume("volume"));
        self.indicator_selected = chart.indicators.len().saturating_sub(1);
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
            return;
        }
        chart
            .indicators
            .push(IndicatorConfig::session_vp_default("session_vp"));
        self.indicator_selected = chart.indicators.len().saturating_sub(1);
        self.arm_indicators_apply();
    }

    /// Add Fixed Range Volume Profile (max 4). Anchors default to first/last bar ts.
    pub fn indicator_add_fixed_range_vp(&mut self) {
        if !matches!(self.input_mode, InputMode::IndicatorPanel) {
            return;
        }
        let chart = self.focused_chart_mut();
        let count = chart
            .indicators
            .iter()
            .filter(|i| i.indicator_type == "fixed_range_vp")
            .count();
        if count >= MAX_FIXED_RANGE_VP {
            return;
        }
        let (start, end, has_bars) = match &chart.series {
            ChartSeriesState::Available { bars } if !bars.is_empty() => {
                let start = bars.first().map(|b| b.ts).unwrap_or(0);
                let end = bars.last().map(|b| b.ts).unwrap_or(start);
                (start, end.max(start), true)
            }
            // Placeholder until bars arrive; apply_chart_series backfills anchors.
            _ => (0, 0, false),
        };
        let id = format!("frvp{}", count + 1);
        chart
            .indicators
            .push(IndicatorConfig::fixed_range_vp_default(id, start, end));
        self.indicator_selected = chart.indicators.len().saturating_sub(1);
        if has_bars {
            self.arm_indicators_apply();
        }
    }

    pub fn indicator_toggle_selected(&mut self) {
        if !matches!(self.input_mode, InputMode::IndicatorPanel) {
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
        if itype == "session_vp" || itype == "fixed_range_vp" {
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
        let clamp_i = |i: i32| -> usize {
            i.clamp(0, (bar_ts.len() as i32 - 1).max(0)) as usize
        };
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
        if cfg.indicator_type == "session_vp" || cfg.indicator_type == "fixed_range_vp" {
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
        if cfg.indicator_type != "session_vp" && cfg.indicator_type != "fixed_range_vp" {
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
        self.focused_chart_mut().series = ChartSeriesState::Loading;
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
            InputMode::InstrumentPrompt { buffer }
            | InputMode::WatchlistAddPrompt { buffer } => {
                // Instruments are alnum; allow `.` and `-` for future vendor symbols.
                if c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_' {
                    if buffer.len() < 16 {
                        buffer.push(c.to_ascii_uppercase());
                    }
                }
            }
            InputMode::Normal | InputMode::IndicatorPanel => {}
        }
    }

    pub fn prompt_pop_char(&mut self) {
        match &mut self.input_mode {
            InputMode::InstrumentPrompt { buffer }
            | InputMode::WatchlistAddPrompt { buffer } => {
                buffer.pop();
            }
            InputMode::Normal | InputMode::IndicatorPanel => {}
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
            } => {
                self.set_feed(feed);
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
            IpcEvent::QuoteUpdate(update) => {
                self.apply_quote_update(update);
            }
            IpcEvent::IndicatorUpdate(update) => {
                self.apply_indicator_update(update);
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
            IpcEvent::IndicatorsFailed { message: _ } => {
                // Leave local indicator draft; trader can retry panel action.
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
        // Interest always carries the engine's config list (may be empty = naked).
        // Keep local draft Fixed Range rows that were not yet applied (e.g. added before bars).
        let local_pending_frvp: Vec<IndicatorConfig> = chart
            .indicators
            .iter()
            .filter(|i| {
                i.indicator_type == "fixed_range_vp"
                    && !series.indicators.iter().any(|e| e.id == i.id)
            })
            .cloned()
            .collect();
        chart.indicators = series.indicators;
        chart.indicators.extend(local_pending_frvp);
        chart.indicator_series = series.series;
        match series.status.as_str() {
            "ok" => {
                chart.series = ChartSeriesState::Available { bars: series.bars };
                // Backfill placeholder (0,0) anchors to first/last bar and persist.
                let (first_ts, last_ts) = match &chart.series {
                    ChartSeriesState::Available { bars } if !bars.is_empty() => {
                        let s = bars.first().map(|b| b.ts).unwrap_or(0);
                        let e = bars.last().map(|b| b.ts).unwrap_or(s).max(s);
                        (s, e)
                    }
                    _ => return,
                };
                let mut need_apply = false;
                for ind in &mut chart.indicators {
                    if ind.indicator_type == "fixed_range_vp"
                        && ind.start.unwrap_or(0) == 0
                        && ind.end.unwrap_or(0) == 0
                    {
                        ind.start = Some(first_ts);
                        ind.end = Some(last_ts);
                        need_apply = true;
                    }
                }
                if need_apply {
                    let chart_id = chart.id.clone();
                    let indicators = chart.indicators.clone();
                    self.pending_indicators = Some(PendingIndicatorsApply {
                        chart_id,
                        indicators,
                    });
                }
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
    use crate::ipc::WorkspaceChartSnapshot;

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
            },
            workspace: None,
            quotes: vec![],
                    indicators: HashMap::new(),
        });
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
            },
            workspace: Some(WorkspaceSnapshot {
                layout_mode: "dual-vertical".into(),
                charts: vec![
                    WorkspaceChartSnapshot {
                        id: "top".into(),
                        instrument: "ES".into(),
                        timeframe: "1D".into(),
                                            indicators: vec![],
                    },
                    WorkspaceChartSnapshot {
                        id: "bottom".into(),
                        instrument: "QQQ".into(),
                        timeframe: "1h".into(),
                                            indicators: vec![],
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
        assert_eq!(app.active_symbols(), &["SPY".to_string(), "QQQ".to_string()]);
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

    fn dual_workspace() -> WorkspaceSnapshot {
        WorkspaceSnapshot {
            layout_mode: "dual-vertical".into(),
            charts: vec![
                WorkspaceChartSnapshot {
                    id: "top".into(),
                    instrument: "QQQ".into(),
                    timeframe: "1D".into(),
                                        indicators: vec![],
                    },
                WorkspaceChartSnapshot {
                    id: "bottom".into(),
                    instrument: "SPY".into(),
                    timeframe: "1D".into(),
                                        indicators: vec![],
                    },
            ],
            watchlists: vec![],
            active_watchlist_id: String::new(),
        }
    }
}
