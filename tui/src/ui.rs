//! Ratatui views: Welcome, workspace feed status, single/dual charts, watchlist.

use chrono::{Offset, TimeZone, Utc};
use chrono_tz::America::New_York;
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    symbols,
    text::{Line, Span},
    widgets::{
        canvas::{Canvas, Rectangle},
        Block, Borders, Clear, Paragraph, StatefulWidget, Wrap,
    },
    Frame,
};
use tui_candlestick_chart::{
    Candle, CandleStickChart, CandleStickChartState, ChartView,
};

use crate::app::{
    App, Chart, ChartSeriesState, ConnectionStatus, FrvpPinPhase, InputMode, LayoutMode, Screen,
    UNAVAILABLE_COPY,
};
use crate::ipc::{OhlcvBar, QuoteRow};
use crate::overlay::{
    paint_overlays, OverlayHistBar, OverlayLayers, OverlayLevel, OverlayLine, OverlayPin,
    MAX_VP_HIST_PANE_FRACTION,
};
use crate::timeframe::product_timeframe_to_interval;

pub fn draw(frame: &mut Frame, app: &App) {
    match app.screen {
        Screen::Welcome => draw_welcome(frame, app),
        Screen::Workspace => draw_workspace(frame, app),
    }
    if app.help_open {
        draw_help_popup(frame);
    }
}

fn draw_welcome(frame: &mut Frame, app: &App) {
    let area = frame.area();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(35),
            Constraint::Length(8),
            Constraint::Min(1),
        ])
        .split(area);

    let title = Paragraph::new(vec![
        Line::from(Span::styled(
            "Trading Telemetry",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from("Phase A — Chart terminal"),
        Line::from(""),
        Line::from(feed_line(app)),
        Line::from(""),
        Line::from(Span::styled(
            "Press Enter to open workspace  ·  ? help  ·  q to quit",
            Style::default().fg(Color::DarkGray),
        )),
    ])
    .alignment(Alignment::Center)
    .block(Block::default().borders(Borders::ALL).title(" Welcome "));

    frame.render_widget(title, chunks[1]);
}

/// Centered floating panel listing keyboard shortcuts.
fn draw_help_popup(frame: &mut Frame) {
    let area = frame.area();
    // Tall enough for the full menu; shrink on small terminals (content clips).
    let popup_h = area.height.saturating_sub(2).clamp(16, 38);
    let popup = centered_rect(area, 74, popup_h);
    frame.render_widget(Clear, popup);

    let section = |title: &str| {
        Line::from(Span::styled(
            title.to_string(),
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ))
    };
    let row = |keys: &str, desc: &str| {
        Line::from(vec![
            Span::styled(
                format!("  {keys:<14}"),
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(desc.to_string()),
        ])
    };

    let lines = vec![
        section("Global"),
        row("? / h", "Open or close this help"),
        row("q", "Quit the app"),
        row("Esc", "Close help, prompt, or panel (Welcome: quit)"),
        section("Welcome"),
        row("Enter", "Open workspace"),
        section("Chart & layout"),
        row("l", "Toggle single ↔ dual-vertical"),
        row("Tab", "Focus next chart (dual)"),
        row("[ / ]", "Previous / next timeframe"),
        row("i", "Change instrument (focused chart)"),
        row("o", "Open / close indicator panel"),
        section("Watchlist"),
        row("w", "Show / hide sidebar"),
        row("n / p", "Next / previous watchlist sheet"),
        row("↑ / ↓", "Move selection in active list"),
        row("a", "Add symbol to active list"),
        row("x / d", "Remove selected symbol"),
        section("Indicator panel"),
        row("↑ / ↓", "Select indicator row"),
        row("Space / Enter", "Enable / disable selected (incl. Fixed Range + Anchored)"),
        row("m", "Add MA stack (SMA 10 / 60 / 200)"),
        row("v", "Add Volume (max 1)"),
        row("p", "Add Session VP (max 1; note: p = prev list outside panel)"),
        row("f", "Add Fixed Range VP (max 4) → pin placement on chart"),
        row("a", "Add Anchored VP (max 2; cash open 09:30 NY default)"),
        row("y", "Add GEX (optional; unavailable without options data)"),
        row("g", "Add GARCH (optional; unavailable without enough history)"),
        row("r", "Re-place FRVP pins or AVP anchor (when selected)"),
        row("c", "Clear all indicators except Volume"),
        row("e", "Fixed Range: toggle extend-to-right"),
        row("9", "Anchored VP: snap anchor to 09:30 America/New_York"),
        row(", / .", "Nudge FRVP start or AVP anchor bar-by-bar"),
        row("s", "MA: SMA↔EMA · VP: left↔right place"),
        row("+ / -", "MA: length · VP: box width %"),
        row("1 2 3", "VP: toggle POC / VAH / VAL"),
        row("x", "Remove selected indicator"),
        row("o / Esc", "Close indicator panel"),
        section("Fixed Range pin placement"),
        row("← / →", "Move pin cursor bar-by-bar (h/l also)"),
        row("[ / ]", "Jump pin cursor 10 bars"),
        row("Enter", "Lock start pin, then lock end pin"),
        row("Esc", "Cancel placement (drops new FRVP)"),
        section("Anchored VP pin placement"),
        row("← / →", "Move anchor pin bar-by-bar (h/l also)"),
        row("[ / ]", "Jump pin cursor 10 bars"),
        row("9", "Snap to cash open 09:30 America/New_York"),
        row("Enter", "Lock anchor (profile builds to now)"),
        row("Esc", "Cancel placement (drops new AVP)"),
        section("Text prompts"),
        row("Enter", "Apply instrument or watchlist add"),
        row("Esc", "Cancel prompt"),
        Line::from(Span::styled(
            "  Close help with Esc, ?, h, or Enter",
            Style::default().fg(Color::DarkGray),
        )),
    ];

    let body = Paragraph::new(lines)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Cyan))
                .title(Span::styled(
                    " Keyboard shortcuts ",
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                )),
        )
        .wrap(Wrap { trim: false });
    frame.render_widget(body, popup);
}

fn centered_rect(area: Rect, width: u16, height: u16) -> Rect {
    let width = width.min(area.width.saturating_sub(2).max(20));
    let height = height.min(area.height.saturating_sub(2).max(10));
    let x = area.x + (area.width.saturating_sub(width)) / 2;
    let y = area.y + (area.height.saturating_sub(height)) / 2;
    Rect {
        x,
        y,
        width,
        height,
    }
}

fn draw_workspace(frame: &mut Frame, app: &App) {
    let area = frame.area();
    let panel_open = matches!(app.input_mode, InputMode::IndicatorPanel);
    let prompt_h = match &app.input_mode {
        InputMode::InstrumentPrompt { .. } | InputMode::WatchlistAddPrompt { .. } => 3,
        InputMode::IndicatorPanel => 10,
        InputMode::FrvpPlacing | InputMode::AvpPlacing => 3,
        InputMode::Normal => 0,
    };
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(5),
            Constraint::Length(prompt_h),
            Constraint::Length(1),
        ])
        .split(area);

    let status = Paragraph::new(feed_line(app)).block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Feed status "),
    );
    frame.render_widget(status, chunks[0]);

    // Charts left, optional watchlist sidebar docked right.
    if app.watchlist_visible {
        let body = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Min(20), Constraint::Length(36)])
            .split(chunks[1]);
        draw_charts(frame, body[0], app);
        draw_watchlist(frame, body[1], app);
    } else {
        draw_charts(frame, chunks[1], app);
    }

    match &app.input_mode {
        InputMode::InstrumentPrompt { buffer } => {
            let prompt = Paragraph::new(format!("Instrument: {buffer}_")).block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Change instrument (Enter apply · Esc cancel) "),
            );
            frame.render_widget(prompt, chunks[2]);
        }
        InputMode::WatchlistAddPrompt { buffer } => {
            let prompt = Paragraph::new(format!("Add symbol: {buffer}_")).block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Watchlist add (Enter apply · Esc cancel) "),
            );
            frame.render_widget(prompt, chunks[2]);
        }
        InputMode::IndicatorPanel => {
            draw_indicator_panel(frame, chunks[2], app);
        }
        InputMode::FrvpPlacing => {
            let (phase_label, pin_label) = match app.frvp_place.as_ref().map(|p| p.phase) {
                Some(FrvpPinPhase::End) => (
                    "PIN 2 / 2 — END of range",
                    "Yellow ▼ is the live end pin · cyan ▲ is locked start",
                ),
                _ => (
                    "PIN 1 / 2 — START of range",
                    "Yellow ▼ sits on the candle under the cursor",
                ),
            };
            let prompt = Paragraph::new(format!(
                "{phase_label}  ·  {pin_label}\n←/→ move bar  ·  [/] ±10  ·  Enter lock pin  ·  Esc cancel"
            ))
            .style(
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            )
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Yellow))
                    .title(" Fixed Range · place pins on chart "),
            );
            frame.render_widget(prompt, chunks[2]);
        }
        InputMode::AvpPlacing => {
            let prompt = Paragraph::new(
                "ANCHOR pin  ·  Yellow ▼ on candle  ·  profile builds from here → now\n←/→ move bar  ·  [/] ±10  ·  9 cash open 09:30 NY  ·  Enter lock  ·  Esc cancel",
            )
            .style(
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            )
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Yellow))
                    .title(" Anchored VP · place anchor on chart "),
            );
            frame.render_widget(prompt, chunks[2]);
        }
        InputMode::Normal => {}
    }

    let focused = app.focused_chart();
    let focus_hint = if app.layout == LayoutMode::DualVertical {
        "  ·  Tab focus"
    } else {
        ""
    };
    let wl_name = app
        .active_watchlist()
        .map(|w| w.name.as_str())
        .unwrap_or("—");
    let help = if matches!(app.input_mode, InputMode::FrvpPlacing) {
        Paragraph::new(
            "Fixed Range pins  ·  ←/→ bar  ·  [/] jump  ·  Enter lock  ·  Esc cancel  ·  ? help",
        )
        .style(Style::default().fg(Color::Yellow))
    } else if matches!(app.input_mode, InputMode::AvpPlacing) {
        Paragraph::new(
            "Anchored pin  ·  ←/→ bar  ·  [/] jump  ·  9 cash open  ·  Enter lock  ·  Esc cancel  ·  ? help",
        )
        .style(Style::default().fg(Color::Yellow))
    } else if panel_open {
        Paragraph::new(format!(
            "Indicators · {}  ·  ? help  ·  m MA  ·  v Vol  ·  p SVP  ·  f FRVP  ·  a AVP  ·  r re-pin  ·  c clear  ·  Space/Enter on/off  ·  9 cash  ·  x  ·  o/Esc",
            focused.title(),
        ))
        .style(Style::default().fg(Color::DarkGray))
    } else {
        Paragraph::new(format!(
            "{} · {} · {}  ·  ? help  ·  l layout  ·  [ ] tf  ·  i instr  ·  o ind{}  ·  w list  ·  n/p sheet  ·  a add  ·  x rem  ·  q  [{}]",
            app.layout.as_str(),
            focused.instrument,
            focused.timeframe,
            focus_hint,
            wl_name,
        ))
        .style(Style::default().fg(Color::DarkGray))
    };
    frame.render_widget(help, chunks[3]);
}

fn draw_indicator_panel(frame: &mut Frame, area: Rect, app: &App) {
    let chart = app.focused_chart();
    let mut lines: Vec<Line<'static>> = Vec::new();
    if chart.indicators.is_empty() {
        lines.push(Line::from(Span::styled(
            "(naked — m MA · v Vol · p SVP · f FRVP · a AVP · y GEX · g GARCH · c clear-keep-vol)",
            Style::default().fg(Color::DarkGray),
        )));
    } else {
        for (i, ind) in chart.indicators.iter().enumerate() {
            let selected = i == app.indicator_selected.min(chart.indicators.len() - 1);
            let mark = if selected { "› " } else { "  " };
            let on = if ind.enabled { "on " } else { "off" };
            let optional_status = |id: &str| -> String {
                match chart.indicator_series.get(id) {
                    Some(s) if s.status.as_deref() == Some("unavailable") => {
                        let reason = match s.reason.as_deref() {
                            Some("options_data_missing") | Some("options_data_unavailable") => {
                                "no options data"
                            }
                            Some("insufficient_history") => "insufficient history",
                            Some("unstable_estimate") => "unstable estimate",
                            Some("compute_failed") => "compute failed",
                            Some(other) => other,
                            None => "unavailable",
                        };
                        format!(" · UNAVAILABLE ({reason})")
                    }
                    Some(s) if s.status.as_deref() == Some("ok") && s.series_type == "gex" => {
                        match s.net_gex {
                            Some(net) => format!(" · ok net_gex={net:.0}"),
                            None => " · ok".into(),
                        }
                    }
                    Some(s) if s.status.as_deref() == Some("ok") => " · ok".into(),
                    Some(_) | None if ind.enabled => " · …".into(),
                    _ => String::new(),
                }
            };
            let vp_levels = |ind: &crate::ipc::IndicatorConfig| {
                format!(
                    "POC{} VAH{} VAL{}",
                    if ind.poc.as_ref().map(|s| s.enabled).unwrap_or(true) {
                        "+"
                    } else {
                        "-"
                    },
                    if ind.vah.as_ref().map(|s| s.enabled).unwrap_or(true) {
                        "+"
                    } else {
                        "-"
                    },
                    if ind.val.as_ref().map(|s| s.enabled).unwrap_or(true) {
                        "+"
                    } else {
                        "-"
                    },
                )
            };
            let label = match ind.indicator_type.as_str() {
                "ma" => format!(
                    "{mark}[{on}] MA {} {}",
                    ind.ma_type.as_deref().unwrap_or("sma").to_uppercase(),
                    ind.length.unwrap_or(1)
                ),
                "volume" => format!("{mark}[{on}] Volume"),
                "session_vp" => {
                    let rows = ind.rows.unwrap_or(500);
                    let place = ind.placement.as_deref().unwrap_or("right");
                    let bw = ind.box_width.unwrap_or(30.0) as i64;
                    format!(
                        "{mark}[{on}] Session VP rows={rows} w={bw}% {place} {}",
                        vp_levels(ind)
                    )
                }
                "fixed_range_vp" => {
                    let rows = ind.rows.unwrap_or(200);
                    let place = ind.placement.as_deref().unwrap_or("right");
                    let bw = ind.box_width.unwrap_or(30.0) as i64;
                    let ext = if ind.extend_to_right.unwrap_or(false) {
                        "ext+"
                    } else {
                        "ext-"
                    };
                    let pins = if !ind.enabled
                        && (ind.start.is_none()
                            || ind.end.is_none()
                            || ind.start == ind.end)
                    {
                        "pins?"
                    } else {
                        "pins✓"
                    };
                    let data = match chart.indicator_series.get(&ind.id) {
                        Some(s) if !s.profiles.is_empty() => " · profile",
                        Some(_) if ind.enabled => " · empty profile",
                        None if ind.enabled => " · …",
                        _ => "",
                    };
                    format!(
                        "{mark}[{on}] Fixed Range VP rows={rows} w={bw}% {place} {ext} {pins}{} {}  (r re-pin)",
                        data,
                        vp_levels(ind)
                    )
                }
                "anchored_vp" => {
                    let rows = ind.rows.unwrap_or(500);
                    let place = ind.placement.as_deref().unwrap_or("right");
                    let bw = ind.box_width.unwrap_or(30.0) as i64;
                    let pin = if ind.anchor.is_none() {
                        "pin?"
                    } else {
                        "pin✓"
                    };
                    let data = match chart.indicator_series.get(&ind.id) {
                        Some(s) if !s.profiles.is_empty() => " · profile",
                        Some(_) if ind.enabled => " · empty profile",
                        None if ind.enabled => " · …",
                        _ => "",
                    };
                    format!(
                        "{mark}[{on}] Anchored VP rows={rows} w={bw}% {place} {pin}{} {}  (r re-pin · 9 cash)",
                        data,
                        vp_levels(ind)
                    )
                }
                "gex" => format!(
                    "{mark}[{on}] GEX{}  (y add · needs options)",
                    optional_status(&ind.id)
                ),
                "garch" => format!(
                    "{mark}[{on}] GARCH{}  (g add · needs history)",
                    optional_status(&ind.id)
                ),
                other => format!("{mark}[{on}] {other}"),
            };
            let unavailable = chart
                .indicator_series
                .get(&ind.id)
                .and_then(|s| s.status.as_deref())
                == Some("unavailable");
            let style = if selected {
                Style::default()
                    .fg(if unavailable { Color::Yellow } else { Color::Cyan })
                    .add_modifier(Modifier::BOLD)
            } else if unavailable && ind.enabled {
                Style::default().fg(Color::Yellow)
            } else if ind.enabled {
                Style::default()
            } else {
                Style::default().fg(Color::DarkGray)
            };
            lines.push(Line::from(Span::styled(label, style)));
        }
    }
    let body = Paragraph::new(lines).block(
        Block::default()
            .borders(Borders::ALL)
            .title(format!(" Indicators · {} ", chart.title())),
    );
    frame.render_widget(body, area);
}

fn draw_watchlist(frame: &mut Frame, area: Rect, app: &App) {
    let list = app.active_watchlist();
    let title = match list {
        Some(wl) => format!(" {} ", wl.name),
        None => " Watchlist ".to_string(),
    };

    let mut lines: Vec<Line<'static>> = Vec::new();
    // Header row — no logos.
    lines.push(Line::from(Span::styled(
        format!("{:<6} {:>8} {:>8} {:>7}", "Sym", "Last", "Chg", "Chg%"),
        Style::default()
            .fg(Color::DarkGray)
            .add_modifier(Modifier::BOLD),
    )));

    let symbols = app.active_symbols();
    if symbols.is_empty() {
        lines.push(Line::from(Span::styled(
            "(empty — press a to add)",
            Style::default().fg(Color::DarkGray),
        )));
    } else {
        for (i, sym) in symbols.iter().enumerate() {
            let selected = i == app.watchlist_selected.min(symbols.len() - 1);
            let quote = app.quote_for(sym);
            lines.push(watchlist_row(sym, quote, selected));
        }
    }

    let switcher = app
        .watchlists
        .iter()
        .map(|wl| {
            if wl.id == app.active_watchlist_id {
                format!("[{}]", wl.name)
            } else {
                wl.name.clone()
            }
        })
        .collect::<Vec<_>>()
        .join(" ");
    let block = Block::default()
        .borders(Borders::ALL)
        .title(title)
        .title_bottom(Line::from(Span::styled(
            format!(" {switcher} "),
            Style::default().fg(Color::DarkGray),
        )));

    let body = Paragraph::new(lines).block(block);
    frame.render_widget(body, area);
}

fn watchlist_row(symbol: &str, quote: Option<&QuoteRow>, selected: bool) -> Line<'static> {
    let base = if selected {
        Style::default().add_modifier(Modifier::REVERSED)
    } else {
        Style::default()
    };
    let sym = format!("{symbol:<6}");
    match quote {
        Some(q) if q.status == "ok" => {
            let last = q.last.map(|v| format!("{v:>8.2}")).unwrap_or_else(|| format!("{:>8}", "—"));
            let chg = q.change.map(|v| format!("{v:>+8.2}")).unwrap_or_else(|| format!("{:>8}", "—"));
            let pct = q
                .change_pct
                .map(|v| format!("{:>+6.2}%", v * 100.0))
                .unwrap_or_else(|| format!("{:>7}", "—"));
            let dir_color = match q.change {
                Some(c) if c > 0.0 => Color::Green,
                Some(c) if c < 0.0 => Color::Red,
                _ => Color::Gray,
            };
            Line::from(vec![
                Span::styled(sym, base),
                Span::styled(format!(" {last}"), base),
                Span::styled(format!(" {chg}"), base.fg(dir_color)),
                Span::styled(format!(" {pct}"), base.fg(dir_color)),
            ])
        }
        Some(q) if q.status == "unavailable" => Line::from(vec![
            Span::styled(sym, base),
            Span::styled(
                format!(" {:>8} {:>8} {:>7}", "n/a", "—", "—"),
                base.fg(Color::DarkGray),
            ),
        ]),
        _ => Line::from(vec![
            Span::styled(sym, base),
            Span::styled(
                format!(" {:>8} {:>8} {:>7}", "…", "—", "—"),
                base.fg(Color::DarkGray),
            ),
        ]),
    }
}

fn draw_charts(frame: &mut Frame, area: Rect, app: &App) {
    match app.layout {
        LayoutMode::Single => {
            if let Some(chart) = app.charts.first() {
                draw_chart(frame, area, app, chart, true);
            }
        }
        LayoutMode::DualVertical => {
            let rows = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
                .split(area);
            for (i, chart) in app.charts.iter().enumerate() {
                if i >= rows.len() {
                    break;
                }
                let focused = i == app.focused;
                draw_chart(frame, rows[i], app, chart, focused);
            }
        }
    }
}

fn draw_chart(frame: &mut Frame, area: Rect, app: &App, chart: &Chart, focused: bool) {
    let focus_mark = if focused { "● " } else { "  " };
    let title = format!(" {focus_mark}{} ", chart.title());
    match &chart.series {
        ChartSeriesState::Idle | ChartSeriesState::Loading => {
            let body = Paragraph::new("Loading history…")
                .alignment(Alignment::Center)
                .block(Block::default().borders(Borders::ALL).title(title));
            frame.render_widget(body, area);
        }
        ChartSeriesState::Unavailable => {
            let copy = app
                .empty_state_copy_for(chart)
                .unwrap_or(UNAVAILABLE_COPY);
            let body = Paragraph::new(vec![
                Line::from(""),
                Line::from(Span::styled(
                    copy,
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                )),
            ])
            .alignment(Alignment::Center)
            .block(Block::default().borders(Borders::ALL).title(title));
            frame.render_widget(body, area);
        }
        ChartSeriesState::Error { message } => {
            let body = Paragraph::new(vec![
                Line::from(Span::styled(
                    "Chart load failed",
                    Style::default().fg(Color::Red),
                )),
                Line::from(message.clone()),
            ])
            .wrap(Wrap { trim: true })
            .alignment(Alignment::Center)
            .block(Block::default().borders(Borders::ALL).title(title));
            frame.render_widget(body, area);
        }
        ChartSeriesState::Available { bars } => {
            draw_price_and_volume(frame, area, app, &title, chart, bars);
        }
    }
}

fn draw_price_and_volume(
    frame: &mut Frame,
    area: Rect,
    app: &App,
    title: &str,
    chart: &Chart,
    bars: &[OhlcvBar],
) {
    if bars.is_empty() {
        let body = Paragraph::new(UNAVAILABLE_COPY)
            .alignment(Alignment::Center)
            .block(Block::default().borders(Borders::ALL).title(title.to_string()));
        frame.render_widget(body, area);
        return;
    }

    let show_volume = chart.has_volume() && chart.volume_series().is_some();
    if show_volume && area.height >= 8 {
        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Percentage(75), Constraint::Percentage(25)])
            .split(area);
        draw_candles(frame, rows[0], app, title, chart, bars);
        draw_volume_pane(frame, rows[1], chart, bars);
    } else {
        draw_candles(frame, area, app, title, chart, bars);
    }
}

// MA colors as solid RGB (named colors blend poorly with candle green/red).
const MA_COLORS: [Color; 3] = [
    Color::Rgb(0, 200, 255),   // cyan — short MA
    Color::Rgb(255, 210, 40),  // gold — mid MA
    Color::Rgb(220, 120, 255), // magenta — long MA
];

/// America/New_York fixed offset at `ts_ms` (handles EST/EDT).
fn ny_offset_at_ms(ts_ms: i64) -> chrono::FixedOffset {
    let utc = Utc
        .timestamp_millis_opt(ts_ms)
        .single()
        .unwrap_or_else(Utc::now);
    New_York.offset_from_utc_datetime(&utc.naive_utc()).fix()
}

/// Engine bars use unix **seconds**; the candlestick widget uses **milliseconds**.
fn bars_to_widget_candles(bars: &[OhlcvBar]) -> Vec<Candle> {
    bars.iter()
        .filter_map(|b| Candle::new(b.ts.saturating_mul(1000), b.open, b.high, b.low, b.close))
        .collect()
}

/// Bars whose open time falls inside the widget's dense visible window.
fn bars_in_view<'a>(bars: &'a [OhlcvBar], view: &ChartView) -> Vec<&'a OhlcvBar> {
    if view.column_timestamps.is_empty() {
        return Vec::new();
    }
    let first = *view.column_timestamps.first().unwrap();
    let last = *view.column_timestamps.last().unwrap();
    bars.iter()
        .filter(|b| {
            let t = b.ts.saturating_mul(1000);
            t >= first && t <= last
        })
        .collect()
}

/// Local candle-area X for a bar open (seconds) via ChartView dense columns.
fn view_ts_to_x(view: &ChartView, ts_sec: i64) -> Option<f64> {
    view.timestamp_to_local_x(ts_sec.saturating_mul(1000))
}

fn draw_candles(
    frame: &mut Frame,
    area: Rect,
    app: &App,
    title: &str,
    chart: &Chart,
    bars: &[OhlcvBar],
) {
    let Some(interval) = product_timeframe_to_interval(&chart.timeframe) else {
        let body = Paragraph::new(format!(
            "Unsupported timeframe for chart axes: {}",
            chart.timeframe
        ))
        .alignment(Alignment::Center)
        .block(Block::default().borders(Borders::ALL).title(title.to_string()));
        frame.render_widget(body, area);
        return;
    };
    let widget_candles = bars_to_widget_candles(bars);
    if widget_candles.is_empty() {
        let body = Paragraph::new(UNAVAILABLE_COPY)
            .alignment(Alignment::Center)
            .block(Block::default().borders(Borders::ALL).title(title.to_string()));
        frame.render_widget(body, area);
        return;
    }

    let ma_lines = chart.enabled_ma_lines();
    let svp = chart.enabled_session_vp();
    let frvps = chart.enabled_fixed_range_vps();
    let avps = chart.enabled_anchored_vps();
    let gex = chart.enabled_gex();
    let garch = chart.enabled_garch();
    let last = bars.last().expect("non-empty");
    let ma_hint = ma_legend(&ma_lines);
    let vp_hint = {
        let mut tags: Vec<&str> = Vec::new();
        if svp.is_some() {
            tags.push("VP");
        }
        if !frvps.is_empty() {
            tags.push("FR");
        }
        if !avps.is_empty() {
            tags.push("AV");
        }
        if tags.is_empty() {
            String::new()
        } else {
            format!("  {}", tags.join("+"))
        }
    };
    let optional_hint = {
        let mut parts: Vec<String> = Vec::new();
        if let Some((_, series)) = gex {
            if let Some(net) = series.net_gex {
                parts.push(format!("GEX={net:.0}"));
            } else {
                parts.push("GEX".into());
            }
        }
        if let Some((_, series)) = garch {
            let tip = series
                .values
                .iter()
                .rev()
                .find_map(|v| *v)
                .map(|v| format!("GARCH={v:.4}"))
                .unwrap_or_else(|| "GARCH".into());
            parts.push(tip);
        }
        if parts.is_empty() {
            String::new()
        } else {
            format!("  {}", parts.join(" "))
        }
    };
    let subtitle = format!(
        " O={:.2} H={:.2} L={:.2} C={:.2}  bars={}{}{}{}",
        last.open,
        last.high,
        last.low,
        last.close,
        bars.len(),
        ma_hint,
        vp_hint,
        optional_hint,
    );

    let block = Block::default()
        .borders(Borders::ALL)
        .title(title.to_string())
        .title_bottom(Line::from(Span::styled(
            subtitle,
            Style::default().fg(Color::DarkGray),
        )));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.width < 8 || inner.height < 5 {
        return;
    }

    // Keep pin placement cursor in view (widget pan cursor = window end timestamp, ms).
    let mut candle_state = CandleStickChartState::default();
    let place_bar_ts = app
        .frvp_place
        .as_ref()
        .filter(|p| p.chart_id == chart.id)
        .map(|p| p.cursor_bar.min(bars.len().saturating_sub(1)))
        .or_else(|| {
            app.avp_place
                .as_ref()
                .filter(|p| p.chart_id == chart.id)
                .map(|p| p.cursor_bar.min(bars.len().saturating_sub(1)))
        })
        .and_then(|idx| bars.get(idx).map(|b| b.ts.saturating_mul(1000)));
    if let Some(ts_ms) = place_bar_ts {
        // Dense window ends at the pin bar (widget takes up to price_width bars ending there).
        candle_state.set_cursor_timestamp(Some(ts_ms));
    }

    let tz = ny_offset_at_ms(last.ts.saturating_mul(1000));
    let widget = CandleStickChart::new(interval)
        .candles(widget_candles)
        .display_timezone(tz)
        .bullish_color(Color::Rgb(52, 208, 88))
        .bearish_color(Color::Rgb(234, 74, 90));

    // Paint candles + price/time axes (origin-safe for dual layout).
    widget.render(inner, frame.buffer_mut(), &mut candle_state);

    let Some(view) = candle_state.last_view.clone() else {
        return;
    };
    let candle_area = view.candle_area();
    if candle_area.width == 0 || candle_area.height == 0 {
        return;
    }

    // Overlays share the widget's dense bar columns + price scale (cell compose).
    let visible_refs = bars_in_view(bars, &view);
    if visible_refs.is_empty() {
        return;
    }
    let visible: Vec<OhlcvBar> = visible_refs.iter().map(|b| (*b).clone()).collect();
    let n_bars = visible.len() as f64;
    let n_cols = view.price_width() as f64;
    // Index-space VP/pins use 0..n_bars; shift into right-aligned dense columns.
    let x_offset = view.column_offset as f64;
    let max_p = view.y_max;
    let y_span = (view.y_max - view.y_min).max(f64::EPSILON);

    // Map full-series index → global start for pin helpers.
    let start = bars
        .iter()
        .position(|b| b.ts == visible[0].ts)
        .unwrap_or(0);

    let mut layers = OverlayLayers::default();

    for (mi, (_cfg, series)) in ma_lines.iter().enumerate() {
        let color = MA_COLORS[mi % MA_COLORS.len()];
        let mut pts: Vec<(f64, f64)> = Vec::new();
        for (i, val) in series.values.iter().enumerate() {
            let Some(v) = val else { continue };
            let Some(bar) = bars.get(i) else { continue };
            let Some(x) = view_ts_to_x(&view, bar.ts) else {
                continue;
            };
            if x < 0.0 || x > n_cols {
                continue;
            }
            pts.push((x, *v));
        }
        if !pts.is_empty() {
            layers.lines.push(OverlayLine {
                points: pts,
                color,
                type_key: "ma".into(),
            });
        }
    }

    let push_vp = |layers: &mut OverlayLayers, draw: VpOverlayDraw, type_key: &str| {
        for rect in draw.hist_rects {
            layers.hist.push(OverlayHistBar {
                x: rect.0 + x_offset,
                y: rect.1,
                width: rect.2,
                height: rect.3,
                color: rect.4,
                type_key: type_key.into(),
            });
        }
        for level in draw.levels {
            layers.levels.push(OverlayLevel {
                x0: level.0 + x_offset,
                x1: level.1 + x_offset,
                price: level.2,
                color: level.3,
                type_key: type_key.into(),
            });
        }
    };
    push_vp(
        &mut layers,
        build_session_vp_draw(svp, &visible, n_bars, n_cols),
        "session_vp",
    );
    push_vp(
        &mut layers,
        build_fixed_range_vp_draw(&frvps, &visible, n_bars, n_cols),
        "fixed_range_vp",
    );
    push_vp(
        &mut layers,
        build_anchored_vp_draw(&avps, &visible, n_bars, n_cols),
        "anchored_vp",
    );

    let pin_labels = collect_frvp_pin_labels(app, chart, bars, start, &visible, max_p, y_span);
    for (x, y, glyph, color) in pin_labels {
        layers.pins.push(OverlayPin {
            x: x + x_offset,
            price: y,
            glyph,
            color,
        });
    }

    // Cell-based compose on the same buffer as candles (no Braille Canvas world).
    // Paint order inside paint_overlays: hist → levels → MA → pins.
    let strengths = chart.overlay_strength_map();
    paint_overlays(frame.buffer_mut(), &view, &layers, &strengths);
}

/// Labels painted above candles for FRVP anchors / live placement cursor.
/// (x, y, glyph, color)
fn collect_frvp_pin_labels(
    app: &App,
    chart: &Chart,
    bars: &[OhlcvBar],
    view_start: usize,
    visible: &[OhlcvBar],
    max_p: f64,
    y_span: f64,
) -> Vec<(f64, f64, String, Color)> {
    let mut out: Vec<(f64, f64, String, Color)> = Vec::new();
    let pin_y = |bar: &OhlcvBar| -> f64 {
        (bar.high + y_span * 0.04).min(max_p - y_span * 0.01)
    };
    let bar_x = |global_i: usize| -> Option<(f64, &OhlcvBar)> {
        if global_i < view_start || global_i >= view_start + visible.len() {
            return None;
        }
        let local = global_i - view_start;
        Some((local as f64 + 0.5, &visible[local]))
    };
    let nearest_idx = |ts: i64| -> Option<usize> {
        if bars.is_empty() {
            return None;
        }
        Some(
            bars.iter()
                .enumerate()
                .min_by_key(|(_, b)| (b.ts - ts).abs())
                .map(|(i, _)| i)
                .unwrap_or(0),
        )
    };

    // Locked anchors for enabled Fixed Range instances.
    for cfg in chart
        .indicators
        .iter()
        .filter(|i| i.indicator_type == "fixed_range_vp" && i.enabled)
    {
        if let Some(ts) = cfg.start {
            if let Some(i) = nearest_idx(ts) {
                if let Some((x, bar)) = bar_x(i) {
                    out.push((x, pin_y(bar), "▲".into(), Color::Cyan));
                }
            }
        }
        if let Some(ts) = cfg.end {
            if let Some(i) = nearest_idx(ts) {
                if let Some((x, bar)) = bar_x(i) {
                    out.push((x, pin_y(bar), "▲".into(), Color::Magenta));
                }
            }
        }
    }

    // Locked anchors for enabled Anchored VP instances.
    for cfg in chart
        .indicators
        .iter()
        .filter(|i| i.indicator_type == "anchored_vp" && i.enabled)
    {
        if let Some(ts) = cfg.anchor {
            if let Some(i) = nearest_idx(ts) {
                if let Some((x, bar)) = bar_x(i) {
                    out.push((x, pin_y(bar), "◆".into(), Color::LightCyan));
                }
            }
        }
    }

    // Live Anchored VP placement cursor.
    if let Some(place) = app.avp_place.as_ref() {
        if place.chart_id == chart.id {
            if let Some((x, bar)) = bar_x(place.cursor_bar) {
                out.push((x, pin_y(bar), "▼".into(), Color::Yellow));
            }
        }
    }

    // Live Fixed Range placement session for this chart.
    if let Some(place) = app.frvp_place.as_ref() {
        if place.chart_id == chart.id {
            if let Some(start_i) = place.start_bar {
                if let Some((x, bar)) = bar_x(start_i) {
                    out.push((x, pin_y(bar), "▲".into(), Color::Cyan));
                }
            }
            if let Some((x, bar)) = bar_x(place.cursor_bar.min(bars.len().saturating_sub(1))) {
                let glyph = match place.phase {
                    FrvpPinPhase::Start => "▼",
                    FrvpPinPhase::End => "▼",
                };
                out.push((x, pin_y(bar), glyph.into(), Color::Yellow));
                // Extra bright stem down the wick so the pin is hard to miss.
                out.push((
                    x,
                    bar.high,
                    "●".into(),
                    Color::Yellow,
                ));
            }
        }
    }
    out
}

/// Precomputed VP geometry in candle-index / price coordinates.
struct VpOverlayDraw {
    /// (x, y, width, height, color) histogram rectangles.
    hist_rects: Vec<(f64, f64, f64, f64, Color)>,
    /// (x0, x1, y, color) level segments per profile.
    levels: Vec<(f64, f64, f64, Color)>,
}

fn named_color(name: Option<&str>, fallback: Color) -> Color {
    match name.map(|s| s.to_ascii_lowercase()).as_deref() {
        Some("yellow") | Some("gold") => Color::Yellow,
        Some("green") | Some("lime") => Color::Green,
        Some("red") | Some("magenta") => Color::Red,
        Some("cyan") | Some("steelblue") | Some("blue") => Color::Cyan,
        Some("white") => Color::White,
        Some("gray") | Some("grey") | Some("darkgray") | Some("darkgrey") => Color::DarkGray,
        Some("lightblue") => Color::LightBlue,
        Some("lightgreen") => Color::LightGreen,
        _ => fallback,
    }
}

fn hist_color_for_opacity(name: Option<&str>, opacity: Option<f64>) -> Option<Color> {
    let op = opacity.unwrap_or(0.35);
    if op <= 0.0 {
        return None;
    }
    // Terminal has no true alpha: pick a quieter or stronger shade from opacity.
    let base = named_color(name, Color::DarkGray);
    if op < 0.25 {
        Some(Color::DarkGray)
    } else if op < 0.6 {
        Some(base)
    } else {
        Some(match base {
            Color::DarkGray => Color::Gray,
            other => other,
        })
    }
}

/// Distinct soft defaults so Session / FRVP / AVP don't look identical (TV-like).
fn default_vp_hist_color(type_key: &str) -> Color {
    match type_key {
        "session_vp" => Color::Rgb(70, 110, 160),   // steel blue
        "fixed_range_vp" => Color::Rgb(140, 90, 170), // purple
        "anchored_vp" => Color::Rgb(50, 140, 150),    // teal
        _ => Color::DarkGray,
    }
}

/// Cap hist box width: min(span * box_width_pct, pane_width * MAX_VP_HIST_PANE_FRACTION).
fn capped_vp_box_width(span: f64, box_width_pct: f64, pane_width: f64) -> f64 {
    let from_span = span * box_width_pct;
    let from_pane = pane_width.max(1.0) * MAX_VP_HIST_PANE_FRACTION;
    from_span.min(from_pane).max(1.0)
}

fn build_session_vp_draw(
    svp: Option<(&crate::ipc::IndicatorConfig, &crate::ipc::IndicatorSeriesData)>,
    visible: &[OhlcvBar],
    n: f64,
    pane_width: f64,
) -> VpOverlayDraw {
    let empty = VpOverlayDraw {
        hist_rects: Vec::new(),
        levels: Vec::new(),
    };
    let Some((cfg, series)) = svp else {
        return empty;
    };
    if series.profiles.is_empty() || visible.is_empty() {
        return empty;
    }

    let box_width_pct = cfg.box_width.unwrap_or(30.0).clamp(1.0, 100.0) / 100.0;
    let placement_right = cfg.placement.as_deref().unwrap_or("right") != "left";
    let hist_style = cfg.histogram.as_ref();
    let hist_color = hist_color_for_opacity(
        hist_style.and_then(|h| h.color.as_deref()),
        hist_style.and_then(|h| h.opacity),
    )
    .map(|c| {
        // Prefer type-distinct soft RGB when config still uses the generic steelblue default.
        if hist_style.and_then(|h| h.color.as_deref()) == Some("steelblue") {
            default_vp_hist_color("session_vp")
        } else {
            c
        }
    });
    let poc_on = cfg.poc.as_ref().map(|s| s.enabled).unwrap_or(true);
    let vah_on = cfg.vah.as_ref().map(|s| s.enabled).unwrap_or(true);
    let val_on = cfg.val.as_ref().map(|s| s.enabled).unwrap_or(true);
    let poc_color = named_color(cfg.poc.as_ref().and_then(|s| s.color.as_deref()), Color::Yellow);
    let vah_color = named_color(cfg.vah.as_ref().and_then(|s| s.color.as_deref()), Color::Green);
    let val_color = named_color(cfg.val.as_ref().and_then(|s| s.color.as_deref()), Color::Red);
    // Skip levels when opacity is explicitly 0.
    let level_visible = |style: Option<&crate::ipc::LevelStyle>| {
        style.and_then(|s| s.opacity).map(|o| o > 0.0).unwrap_or(true)
    };

    // Map bar ts → x index within the visible window (nearest open).
    let ts_to_x = |ts: i64| -> f64 {
        if visible.len() == 1 {
            return 0.5;
        }
        let mut best_i = 0usize;
        let mut best_d = (visible[0].ts - ts).abs();
        for (i, b) in visible.iter().enumerate().skip(1) {
            let d = (b.ts - ts).abs();
            if d < best_d {
                best_d = d;
                best_i = i;
            }
        }
        best_i as f64 + 0.5
    };

    let mut hist_rects: Vec<(f64, f64, f64, f64, Color)> = Vec::new();
    let mut levels: Vec<(f64, f64, f64, Color)> = Vec::new();

    // One histogram + levels per day that intersects the visible window.
    let vis_start = visible.first().map(|b| b.ts).unwrap_or(0);
    let vis_end = visible.last().map(|b| b.ts).unwrap_or(0);

    for profile in &series.profiles {
        let sess_start = profile.session_start.unwrap_or(0);
        let sess_end = profile.session_end.unwrap_or(0);
        if sess_end <= vis_start || sess_start > vis_end {
            continue;
        }
        let x_start = ts_to_x(sess_start);
        let x_end = ts_to_x(sess_end.saturating_sub(1));
        let (x_lo, x_hi) = if x_end >= x_start {
            (x_start.min(n - 0.5).max(0.0), x_end.min(n).max(0.5))
        } else {
            (x_end.min(n - 0.5).max(0.0), x_start.min(n).max(0.5))
        };
        let span = (x_hi - x_lo).max(1.0);
        let box_w = capped_vp_box_width(span, box_width_pct, pane_width);
        let hist_x0 = if placement_right { x_hi - box_w } else { x_lo };

        if let Some(hcolor) = hist_color {
            let max_vol = profile
                .bins
                .iter()
                .map(|b| b.volume)
                .fold(0.0_f64, f64::max)
                .max(f64::EPSILON);
            // Downsample bins to keep terminal draw cost bounded.
            let bin_count = profile.bins.len();
            let step = (bin_count / 80).max(1);
            for (i, bin) in profile.bins.iter().enumerate() {
                if i % step != 0 && i + 1 != bin_count {
                    continue;
                }
                if bin.volume <= 0.0 {
                    continue;
                }
                let frac = (bin.volume / max_vol).clamp(0.0, 1.0);
                let bar_w = box_w * frac;
                let x = if placement_right {
                    hist_x0 + box_w - bar_w
                } else {
                    hist_x0
                };
                let y = bin.price_low;
                let h = (bin.price_high - bin.price_low).max(f64::EPSILON);
                hist_rects.push((x, y, bar_w.max(0.5), h, hcolor));
            }
        }

        if poc_on && level_visible(cfg.poc.as_ref()) {
            levels.push((x_lo, x_hi, profile.poc, poc_color));
        }
        if vah_on && level_visible(cfg.vah.as_ref()) {
            levels.push((x_lo, x_hi, profile.vah, vah_color));
        }
        if val_on && level_visible(cfg.val.as_ref()) {
            levels.push((x_lo, x_hi, profile.val, val_color));
        }
    }

    VpOverlayDraw {
        hist_rects,
        levels,
    }
}

fn build_fixed_range_vp_draw(
    frvps: &[(&crate::ipc::IndicatorConfig, &crate::ipc::IndicatorSeriesData)],
    visible: &[OhlcvBar],
    n: f64,
    pane_width: f64,
) -> VpOverlayDraw {
    let empty = VpOverlayDraw {
        hist_rects: Vec::new(),
        levels: Vec::new(),
    };
    if frvps.is_empty() || visible.is_empty() {
        return empty;
    }

    let vis_start = visible.first().map(|b| b.ts).unwrap_or(0);
    let vis_end = visible.last().map(|b| b.ts).unwrap_or(0);

    let ts_to_x = |ts: i64| -> f64 {
        if visible.len() == 1 {
            return 0.5;
        }
        let mut best_i = 0usize;
        let mut best_d = (visible[0].ts - ts).abs();
        for (i, b) in visible.iter().enumerate().skip(1) {
            let d = (b.ts - ts).abs();
            if d < best_d {
                best_d = d;
                best_i = i;
            }
        }
        best_i as f64 + 0.5
    };

    let level_visible = |style: Option<&crate::ipc::LevelStyle>| {
        style.and_then(|s| s.opacity).map(|o| o > 0.0).unwrap_or(true)
    };

    let mut hist_rects: Vec<(f64, f64, f64, f64, Color)> = Vec::new();
    let mut levels: Vec<(f64, f64, f64, Color)> = Vec::new();

    for (cfg, series) in frvps {
        if series.profiles.is_empty() {
            continue;
        }
        let box_width_pct = cfg.box_width.unwrap_or(30.0).clamp(1.0, 100.0) / 100.0;
        let placement_right = cfg.placement.as_deref().unwrap_or("right") != "left";
        let hist_style = cfg.histogram.as_ref();
        let hist_color = hist_color_for_opacity(
            hist_style.and_then(|h| h.color.as_deref()),
            hist_style.and_then(|h| h.opacity),
        )
        .map(|c| {
            if hist_style.and_then(|h| h.color.as_deref()) == Some("steelblue") {
                default_vp_hist_color("fixed_range_vp")
            } else {
                c
            }
        });
        let poc_on = cfg.poc.as_ref().map(|s| s.enabled).unwrap_or(true);
        let vah_on = cfg.vah.as_ref().map(|s| s.enabled).unwrap_or(true);
        let val_on = cfg.val.as_ref().map(|s| s.enabled).unwrap_or(true);
        let poc_color =
            named_color(cfg.poc.as_ref().and_then(|s| s.color.as_deref()), Color::Yellow);
        let vah_color =
            named_color(cfg.vah.as_ref().and_then(|s| s.color.as_deref()), Color::Green);
        let val_color =
            named_color(cfg.val.as_ref().and_then(|s| s.color.as_deref()), Color::Red);

        for profile in &series.profiles {
            let range_start = profile
                .range_start
                .or(cfg.start)
                .unwrap_or(vis_start);
            let range_end = profile.range_end.or(cfg.end).unwrap_or(vis_end);
            let levels_end = profile
                .levels_end
                .or(profile.range_end)
                .or(cfg.end)
                .unwrap_or(range_end);
            // Histogram spans accumulation window; skip if wholly outside view.
            if range_end < vis_start || range_start > vis_end {
                continue;
            }
            let x_start = ts_to_x(range_start);
            let x_hist_end = ts_to_x(range_end);
            let x_levels_end = ts_to_x(levels_end);
            let (hist_lo, hist_hi) = if x_hist_end >= x_start {
                (
                    x_start.min(n - 0.5).max(0.0),
                    x_hist_end.min(n).max(0.5),
                )
            } else {
                (
                    x_hist_end.min(n - 0.5).max(0.0),
                    x_start.min(n).max(0.5),
                )
            };
            let levels_hi = x_levels_end.min(n).max(hist_hi);
            let span = (hist_hi - hist_lo).max(1.0);
            let box_w = capped_vp_box_width(span, box_width_pct, pane_width);
            let hist_x0 = if placement_right {
                hist_hi - box_w
            } else {
                hist_lo
            };

            if let Some(hcolor) = hist_color {
                let max_vol = profile
                    .bins
                    .iter()
                    .map(|b| b.volume)
                    .fold(0.0_f64, f64::max)
                    .max(f64::EPSILON);
                let bin_count = profile.bins.len();
                let step = (bin_count / 80).max(1);
                for (i, bin) in profile.bins.iter().enumerate() {
                    if i % step != 0 && i + 1 != bin_count {
                        continue;
                    }
                    if bin.volume <= 0.0 {
                        continue;
                    }
                    let frac = (bin.volume / max_vol).clamp(0.0, 1.0);
                    let bar_w = box_w * frac;
                    let x = if placement_right {
                        hist_x0 + box_w - bar_w
                    } else {
                        hist_x0
                    };
                    let y = bin.price_low;
                    let h = (bin.price_high - bin.price_low).max(f64::EPSILON);
                    hist_rects.push((x, y, bar_w.max(0.5), h, hcolor));
                }
            }

            // Levels: from range start to levels_end (projects past anchor when extend on).
            let x_lo = hist_lo;
            let x_hi = levels_hi.max(hist_hi);
            if poc_on && level_visible(cfg.poc.as_ref()) {
                levels.push((x_lo, x_hi, profile.poc, poc_color));
            }
            if vah_on && level_visible(cfg.vah.as_ref()) {
                levels.push((x_lo, x_hi, profile.vah, vah_color));
            }
            if val_on && level_visible(cfg.val.as_ref()) {
                levels.push((x_lo, x_hi, profile.val, val_color));
            }
        }
    }

    VpOverlayDraw {
        hist_rects,
        levels,
    }
}

fn build_anchored_vp_draw(
    avps: &[(&crate::ipc::IndicatorConfig, &crate::ipc::IndicatorSeriesData)],
    visible: &[OhlcvBar],
    n: f64,
    pane_width: f64,
) -> VpOverlayDraw {
    // Anchored VP is always "extend to now": reuse Fixed Range-style draw with
    // range_start=anchor and range_end/levels_end from the engine profile.
    // Map cfg.anchor into the same fields FRVP expects via a thin adapter.
    let empty = VpOverlayDraw {
        hist_rects: Vec::new(),
        levels: Vec::new(),
    };
    if avps.is_empty() || visible.is_empty() {
        return empty;
    }

    let vis_start = visible.first().map(|b| b.ts).unwrap_or(0);
    let vis_end = visible.last().map(|b| b.ts).unwrap_or(0);

    let ts_to_x = |ts: i64| -> f64 {
        if visible.len() == 1 {
            return 0.5;
        }
        let mut best_i = 0usize;
        let mut best_d = (visible[0].ts - ts).abs();
        for (i, b) in visible.iter().enumerate().skip(1) {
            let d = (b.ts - ts).abs();
            if d < best_d {
                best_d = d;
                best_i = i;
            }
        }
        best_i as f64 + 0.5
    };

    let level_visible = |style: Option<&crate::ipc::LevelStyle>| {
        style.and_then(|s| s.opacity).map(|o| o > 0.0).unwrap_or(true)
    };

    let mut hist_rects: Vec<(f64, f64, f64, f64, Color)> = Vec::new();
    let mut levels: Vec<(f64, f64, f64, Color)> = Vec::new();

    for (cfg, series) in avps {
        if series.profiles.is_empty() {
            continue;
        }
        let box_width_pct = cfg.box_width.unwrap_or(30.0).clamp(1.0, 100.0) / 100.0;
        let placement_right = cfg.placement.as_deref().unwrap_or("right") != "left";
        let hist_style = cfg.histogram.as_ref();
        let hist_color = hist_color_for_opacity(
            hist_style.and_then(|h| h.color.as_deref()),
            hist_style.and_then(|h| h.opacity),
        )
        .map(|c| {
            if hist_style.and_then(|h| h.color.as_deref()) == Some("steelblue") {
                default_vp_hist_color("anchored_vp")
            } else {
                c
            }
        });
        let poc_on = cfg.poc.as_ref().map(|s| s.enabled).unwrap_or(true);
        let vah_on = cfg.vah.as_ref().map(|s| s.enabled).unwrap_or(true);
        let val_on = cfg.val.as_ref().map(|s| s.enabled).unwrap_or(true);
        let poc_color =
            named_color(cfg.poc.as_ref().and_then(|s| s.color.as_deref()), Color::Yellow);
        let vah_color =
            named_color(cfg.vah.as_ref().and_then(|s| s.color.as_deref()), Color::Green);
        let val_color =
            named_color(cfg.val.as_ref().and_then(|s| s.color.as_deref()), Color::Red);

        for profile in &series.profiles {
            let range_start = profile
                .range_start
                .or(profile.anchor)
                .or(cfg.anchor)
                .unwrap_or(vis_start);
            let range_end = profile.range_end.unwrap_or(vis_end);
            let levels_end = profile
                .levels_end
                .or(profile.range_end)
                .unwrap_or(range_end);
            if range_end < vis_start || range_start > vis_end {
                continue;
            }
            let x_start = ts_to_x(range_start);
            let x_hist_end = ts_to_x(range_end);
            let x_levels_end = ts_to_x(levels_end);
            let (hist_lo, hist_hi) = if x_hist_end >= x_start {
                (
                    x_start.min(n - 0.5).max(0.0),
                    x_hist_end.min(n).max(0.5),
                )
            } else {
                (
                    x_hist_end.min(n - 0.5).max(0.0),
                    x_start.min(n).max(0.5),
                )
            };
            let levels_hi = x_levels_end.min(n).max(hist_hi);
            let span = (hist_hi - hist_lo).max(1.0);
            let box_w = capped_vp_box_width(span, box_width_pct, pane_width);
            let hist_x0 = if placement_right {
                hist_hi - box_w
            } else {
                hist_lo
            };

            if let Some(hcolor) = hist_color {
                let max_vol = profile
                    .bins
                    .iter()
                    .map(|b| b.volume)
                    .fold(0.0_f64, f64::max)
                    .max(f64::EPSILON);
                let bin_count = profile.bins.len();
                let step = (bin_count / 80).max(1);
                for (i, bin) in profile.bins.iter().enumerate() {
                    if i % step != 0 && i + 1 != bin_count {
                        continue;
                    }
                    if bin.volume <= 0.0 {
                        continue;
                    }
                    let frac = (bin.volume / max_vol).clamp(0.0, 1.0);
                    let bar_w = box_w * frac;
                    let x = if placement_right {
                        hist_x0 + box_w - bar_w
                    } else {
                        hist_x0
                    };
                    let y = bin.price_low;
                    let h = (bin.price_high - bin.price_low).max(f64::EPSILON);
                    hist_rects.push((x, y, bar_w.max(0.5), h, hcolor));
                }
            }

            let x_lo = hist_lo;
            let x_hi = levels_hi.max(hist_hi);
            if poc_on && level_visible(cfg.poc.as_ref()) {
                levels.push((x_lo, x_hi, profile.poc, poc_color));
            }
            if vah_on && level_visible(cfg.vah.as_ref()) {
                levels.push((x_lo, x_hi, profile.vah, vah_color));
            }
            if val_on && level_visible(cfg.val.as_ref()) {
                levels.push((x_lo, x_hi, profile.val, val_color));
            }
        }
    }

    VpOverlayDraw {
        hist_rects,
        levels,
    }
}

fn ma_legend(ma_lines: &[(&crate::ipc::IndicatorConfig, &crate::ipc::IndicatorSeriesData)]) -> String {
    if ma_lines.is_empty() {
        return String::new();
    }
    let parts: Vec<String> = ma_lines
        .iter()
        .enumerate()
        .map(|(i, (cfg, _))| {
            let len = cfg.length.unwrap_or(0);
            let tag = match i % 3 {
                0 => "c", // cyan
                1 => "y", // yellow
                _ => "m", // magenta
            };
            format!("{tag}{len}")
        })
        .collect();
    format!("  MA[{}]", parts.join(" "))
}

fn draw_volume_pane(frame: &mut Frame, area: Rect, chart: &Chart, bars: &[OhlcvBar]) {
    let Some(vol_series) = chart.volume_series() else {
        return;
    };
    let inner_w = area.width.saturating_sub(2).max(8) as usize;
    let max_bars = inner_w.saturating_mul(2).div_ceil(3).max(16).min(inner_w);
    let start = if bars.len() <= max_bars {
        0
    } else {
        bars.len() - max_bars
    };
    let visible_bars = &bars[start..];
    let n = visible_bars.len() as f64;
    let values: Vec<f64> = vol_series
        .values
        .iter()
        .skip(start)
        .take(visible_bars.len())
        .map(|v| v.unwrap_or(0.0))
        .collect();
    let max_v = values.iter().cloned().fold(0.0_f64, f64::max).max(1.0);

    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Volume ");

    // Thin stems read better than full-width Block slabs in a short pane.
    let body_w = 0.45_f64;
    let half_w = body_w / 2.0;
    let canvas = Canvas::default()
        .block(block)
        .marker(symbols::Marker::Braille)
        .x_bounds([0.0, n])
        .y_bounds([0.0, max_v * 1.05])
        .paint(move |ctx| {
            for (i, (bar, &vol)) in visible_bars.iter().zip(values.iter()).enumerate() {
                let x = i as f64 + 0.5;
                let up = bar.close >= bar.open;
                // Soft directional tint — thinner stems so volume stays secondary.
                let color = if up {
                    Color::Rgb(0, 120, 60)
                } else {
                    Color::Rgb(140, 40, 40)
                };
                let height = vol.max(max_v * 0.02);
                ctx.draw(&Rectangle {
                    x: x - half_w,
                    y: 0.0,
                    width: body_w,
                    height,
                    color,
                });
            }
        });
    frame.render_widget(canvas, area);
}

fn feed_line(app: &App) -> Line<'static> {
    let (label, color) = match &app.connection {
        ConnectionStatus::Connecting => (app.connection.label().to_string(), Color::Yellow),
        ConnectionStatus::Connected => (app.connection.label().to_string(), Color::Green),
        ConnectionStatus::Disconnected { reason } => {
            (format!("{} ({reason})", app.connection.label()), Color::Red)
        }
    };
    let vendor = app.vendor_mode_label().to_string();
    let hb = app
        .last_heartbeat_ts
        .map(|ts| format!("  heartbeat={ts:.0}"))
        .unwrap_or_default();
    let err = app
        .last_indicator_error
        .as_ref()
        .map(|m| format!("  ind-err={m}"))
        .unwrap_or_default();

    Line::from(vec![
        Span::raw("feed: "),
        Span::styled(label, Style::default().fg(color).add_modifier(Modifier::BOLD)),
        Span::raw(format!("  vendor={vendor}{hb}")),
        Span::styled(err, Style::default().fg(Color::Red)),
    ])
}
