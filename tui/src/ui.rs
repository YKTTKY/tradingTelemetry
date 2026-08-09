//! Ratatui views: Welcome, workspace feed status, single/dual charts, watchlist.

use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    symbols,
    text::{Line, Span},
    widgets::{
        canvas::{Canvas, Line as CanvasLine, Rectangle},
        Block, Borders, Clear, Paragraph, Wrap,
    },
    Frame,
};

use crate::app::{
    App, Chart, ChartSeriesState, ConnectionStatus, InputMode, LayoutMode, Screen, UNAVAILABLE_COPY,
};
use crate::ipc::{OhlcvBar, QuoteRow};

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
        row("Space", "Enable / disable selected"),
        row("m", "Add MA stack (SMA 10 / 60 / 200)"),
        row("v", "Add Volume (max 1)"),
        row("p", "Add Session VP (max 1; note: p = prev list outside panel)"),
        row("s", "MA: SMA↔EMA · Session VP: left↔right place"),
        row("+ / -", "MA: length · Session VP: box width %"),
        row("1 2 3", "Session VP: toggle POC / VAH / VAL"),
        row("x", "Remove selected indicator"),
        row("o / Esc", "Close indicator panel"),
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
    let help = if panel_open {
        Paragraph::new(format!(
            "Indicators · {}  ·  ? help  ·  m MA  ·  v Vol  ·  p SVP  ·  Space  ·  s type/place  ·  +/-  ·  1/2/3 levels  ·  x rem  ·  o/Esc",
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
            "(naked — m MA 10/60/200 · v Volume · p Session VP)",
            Style::default().fg(Color::DarkGray),
        )));
    } else {
        for (i, ind) in chart.indicators.iter().enumerate() {
            let selected = i == app.indicator_selected.min(chart.indicators.len() - 1);
            let mark = if selected { "› " } else { "  " };
            let on = if ind.enabled { "on " } else { "off" };
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
                    let levels = format!(
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
                    );
                    format!("{mark}[{on}] Session VP rows={rows} w={bw}% {place} {levels}")
                }
                other => format!("{mark}[{on}] {other}"),
            };
            let style = if selected {
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
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
            draw_price_and_volume(frame, area, &title, chart, bars);
        }
    }
}

fn draw_price_and_volume(frame: &mut Frame, area: Rect, title: &str, chart: &Chart, bars: &[OhlcvBar]) {
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
        draw_candles(frame, rows[0], title, chart, bars);
        draw_volume_pane(frame, rows[1], chart, bars);
    } else {
        draw_candles(frame, area, title, chart, bars);
    }
}

// Distinct but calm MA colors (read as thin lines over green/red candles).
const MA_COLORS: [Color; 3] = [Color::Cyan, Color::Yellow, Color::LightMagenta];

fn draw_candles(
    frame: &mut Frame,
    area: Rect,
    title: &str,
    chart: &Chart,
    bars: &[OhlcvBar],
) {
    // One candle column ≈ one terminal cell. Cap density so wicks stay legible.
    let inner_w = area.width.saturating_sub(2).max(8) as usize;
    // Prefer fewer bars than full width so each candle has room for a body + gap.
    let max_bars = inner_w.saturating_mul(2).div_ceil(3).max(16).min(inner_w);
    let start = if bars.len() <= max_bars {
        0
    } else {
        bars.len() - max_bars
    };
    let visible = &bars[start..];
    let n = visible.len() as f64;

    // Scale primarily from candle range so MAs don't crush price action.
    let mut min_p = visible[0].low;
    let mut max_p = visible[0].high;
    for b in visible {
        min_p = min_p.min(b.low);
        max_p = max_p.max(b.high);
    }
    let candle_span = (max_p - min_p).max(f64::EPSILON);
    let ma_lines = chart.enabled_ma_lines();
    // Only expand the scale a little for MAs that sit near price (ignore wild outliers).
    for (_, series) in &ma_lines {
        for val in series.values.iter().skip(start).take(visible.len()) {
            if let Some(v) = *val {
                if v >= min_p - candle_span && v <= max_p + candle_span {
                    min_p = min_p.min(v);
                    max_p = max_p.max(v);
                }
            }
        }
    }
    if (max_p - min_p).abs() < f64::EPSILON {
        min_p -= 1.0;
        max_p += 1.0;
    }
    let pad = ((max_p - min_p) * 0.06).max(0.01);
    min_p -= pad;
    max_p += pad;
    let y_span = max_p - min_p;

    let last = bars.last().expect("non-empty");
    let ma_hint = ma_legend(&ma_lines);
    let svp = chart.enabled_session_vp();
    let vp_hint = if svp.is_some() { "  VP" } else { "" };
    let subtitle = format!(
        " O={:.2} H={:.2} L={:.2} C={:.2}  bars={}{}{}{}",
        last.open,
        last.high,
        last.low,
        last.close,
        bars.len(),
        if visible.len() < bars.len() {
            format!(" show={}", visible.len())
        } else {
            String::new()
        },
        ma_hint,
        vp_hint,
    );

    let block = Block::default()
        .borders(Borders::ALL)
        .title(title.to_string())
        .title_bottom(Line::from(Span::styled(
            subtitle,
            Style::default().fg(Color::DarkGray),
        )));

    // Narrow bodies leave gaps; Braille gives sub-cell resolution for wicks + MA.
    let body_w = 0.35_f64;
    let half_w = body_w / 2.0;
    // Minimum body height ≈ ~1 braille row in price units.
    let min_body = y_span * 0.012;

    let mut ma_segments: Vec<(Color, Vec<(f64, f64)>)> = Vec::new();
    for (mi, (_cfg, series)) in ma_lines.iter().enumerate() {
        let color = MA_COLORS[mi % MA_COLORS.len()];
        let mut pts: Vec<(f64, f64)> = Vec::new();
        for (i, val) in series.values.iter().skip(start).take(visible.len()).enumerate() {
            if let Some(v) = val {
                pts.push((i as f64 + 0.5, *v));
            }
        }
        ma_segments.push((color, pts));
    }

    // Precompute Session VP drawable primitives in chart x/y space.
    let vp_draw = build_session_vp_draw(svp, visible, n);

    let canvas = Canvas::default()
        .block(block)
        // Braille: 2×4 subpixels per cell — far thinner than Marker::Block slabs.
        .marker(symbols::Marker::Braille)
        .x_bounds([0.0, n])
        .y_bounds([min_p, max_p])
        .paint(move |ctx| {
            // Candles first (underlays).
            for (i, bar) in visible.iter().enumerate() {
                let x = i as f64 + 0.5;
                let up = bar.close >= bar.open;
                let color = if up { Color::Green } else { Color::Red };

                ctx.draw(&CanvasLine {
                    x1: x,
                    y1: bar.low,
                    x2: x,
                    y2: bar.high,
                    color,
                });

                let top = bar.open.max(bar.close);
                let bottom = bar.open.min(bar.close);
                let height = (top - bottom).max(min_body);
                ctx.draw(&Rectangle {
                    x: x - half_w,
                    y: bottom,
                    width: body_w,
                    height,
                    color,
                });
            }
            // Session VP histogram + levels (under MA so trend lines stay sharp).
            for rect in &vp_draw.hist_rects {
                ctx.draw(&Rectangle {
                    x: rect.0,
                    y: rect.1,
                    width: rect.2,
                    height: rect.3,
                    color: rect.4,
                });
            }
            for (x0, x1, y, color) in &vp_draw.levels {
                ctx.draw(&CanvasLine {
                    x1: *x0,
                    y1: *y,
                    x2: *x1,
                    y2: *y,
                    color: *color,
                });
            }
            // MA as connected segments (thin braille strokes over candles).
            for (color, pts) in &ma_segments {
                for w in pts.windows(2) {
                    let (x1, y1) = w[0];
                    let (x2, y2) = w[1];
                    if (x2 - x1).abs() <= 1.0 + f64::EPSILON {
                        ctx.draw(&CanvasLine {
                            x1,
                            y1,
                            x2,
                            y2,
                            color: *color,
                        });
                    }
                }
            }
        });

    frame.render_widget(canvas, area);
}

/// Precomputed Session VP geometry in candle-index / price coordinates.
struct SessionVpDraw {
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

fn build_session_vp_draw(
    svp: Option<(&crate::ipc::IndicatorConfig, &crate::ipc::IndicatorSeriesData)>,
    visible: &[OhlcvBar],
    n: f64,
) -> SessionVpDraw {
    let empty = SessionVpDraw {
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
    );
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
        if profile.session_end <= vis_start || profile.session_start > vis_end {
            continue;
        }
        let x_start = ts_to_x(profile.session_start);
        let x_end = ts_to_x(profile.session_end.saturating_sub(1));
        let (x_lo, x_hi) = if x_end >= x_start {
            (x_start.min(n - 0.5).max(0.0), x_end.min(n).max(0.5))
        } else {
            (x_end.min(n - 0.5).max(0.0), x_start.min(n).max(0.5))
        };
        let span = (x_hi - x_lo).max(1.0);
        let box_w = span * box_width_pct;
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
                hist_rects.push((x, y, bar_w.max(0.02), h, hcolor));
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

    SessionVpDraw {
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

    Line::from(vec![
        Span::raw("feed: "),
        Span::styled(label, Style::default().fg(color).add_modifier(Modifier::BOLD)),
        Span::raw(format!("  vendor={vendor}{hb}")),
    ])
}
