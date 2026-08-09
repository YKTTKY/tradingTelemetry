//! Ratatui views: Welcome, workspace feed status, single/dual charts, watchlist.

use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    symbols,
    text::{Line, Span},
    widgets::{
        canvas::{Canvas, Line as CanvasLine, Rectangle},
        Block, Borders, Paragraph, Wrap,
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
            "Press Enter to open workspace  ·  q to quit",
            Style::default().fg(Color::DarkGray),
        )),
    ])
    .alignment(Alignment::Center)
    .block(Block::default().borders(Borders::ALL).title(" Welcome "));

    frame.render_widget(title, chunks[1]);
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
            "Indicators · {}  ·  m MA stack  ·  v Volume  ·  Space toggle  ·  s SMA/EMA  ·  [ ] length  ·  x rem  ·  o/Esc close",
            focused.title(),
        ))
        .style(Style::default().fg(Color::DarkGray))
    } else {
        Paragraph::new(format!(
            "{} · {} · {}  ·  l layout  ·  [ ] tf  ·  i instr  ·  o ind{}  ·  w watchlist  ·  n/p list  ·  a add  ·  x rem  ·  ↑↓  ·  q  [{}]",
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
            "(naked — m add MA 10/60/200 · v add Volume)",
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

const MA_COLORS: [Color; 3] = [Color::Cyan, Color::Yellow, Color::Magenta];

fn draw_candles(
    frame: &mut Frame,
    area: Rect,
    title: &str,
    chart: &Chart,
    bars: &[OhlcvBar],
) {
    // Fit roughly one candle per terminal column (inner width minus borders).
    // Drawing 500 sub-pixel bodies makes the series unreadable even with good data.
    let inner_w = area.width.saturating_sub(2).max(8) as usize;
    let start = if bars.len() <= inner_w {
        0
    } else {
        bars.len() - inner_w
    };
    let visible = &bars[start..];
    let n = visible.len() as f64;

    let mut min_p = visible[0].low;
    let mut max_p = visible[0].high;
    for b in visible {
        min_p = min_p.min(b.low);
        max_p = max_p.max(b.high);
    }
    // Include MA values in price scale when present.
    let ma_lines = chart.enabled_ma_lines();
    for (_, series) in &ma_lines {
        for val in series.values.iter().skip(start).take(visible.len()) {
            if let Some(v) = val {
                min_p = min_p.min(*v);
                max_p = max_p.max(*v);
            }
        }
    }
    // Flat series (or single print) still needs a non-zero y span for Canvas.
    if (max_p - min_p).abs() < f64::EPSILON {
        min_p -= 1.0;
        max_p += 1.0;
    }
    let pad = ((max_p - min_p) * 0.05).max(0.01);
    min_p -= pad;
    max_p += pad;

    let last = bars.last().expect("non-empty");
    let ma_hint = if ma_lines.is_empty() {
        String::new()
    } else {
        format!("  MA×{}", ma_lines.len())
    };
    let subtitle = format!(
        " O={:.2} H={:.2} L={:.2} C={:.2}  bars={}{}{}",
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
    );

    let block = Block::default()
        .borders(Borders::ALL)
        .title(title.to_string())
        .title_bottom(Line::from(Span::styled(
            subtitle,
            Style::default().fg(Color::DarkGray),
        )));

    // Body width in data units: leave a small gap between neighbors.
    let body_w = 0.7_f64;
    let half_w = body_w / 2.0;

    // Pre-extract MA segments for the paint closure (owned values).
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

    let canvas = Canvas::default()
        .block(block)
        .marker(symbols::Marker::Block)
        .x_bounds([0.0, n])
        .y_bounds([min_p, max_p])
        .paint(move |ctx| {
            for (i, bar) in visible.iter().enumerate() {
                let x = i as f64 + 0.5;
                let up = bar.close >= bar.open;
                let color = if up { Color::Green } else { Color::Red };

                // High–low wick
                ctx.draw(&CanvasLine {
                    x1: x,
                    y1: bar.low,
                    x2: x,
                    y2: bar.high,
                    color,
                });

                // Open–close body
                let top = bar.open.max(bar.close);
                let bottom = bar.open.min(bar.close);
                let height = (top - bottom).max((max_p - min_p) * 0.004);
                ctx.draw(&Rectangle {
                    x: x - half_w,
                    y: bottom,
                    width: body_w,
                    height,
                    color,
                });
            }
            for (color, pts) in &ma_segments {
                for w in pts.windows(2) {
                    let (x1, y1) = w[0];
                    let (x2, y2) = w[1];
                    // Only connect consecutive defined samples (skip warm-up gaps).
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

fn draw_volume_pane(frame: &mut Frame, area: Rect, chart: &Chart, bars: &[OhlcvBar]) {
    let Some(vol_series) = chart.volume_series() else {
        return;
    };
    let inner_w = area.width.saturating_sub(2).max(8) as usize;
    let start = if bars.len() <= inner_w {
        0
    } else {
        bars.len() - inner_w
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

    let body_w = 0.7_f64;
    let half_w = body_w / 2.0;
    let canvas = Canvas::default()
        .block(block)
        .marker(symbols::Marker::Block)
        .x_bounds([0.0, n])
        .y_bounds([0.0, max_v * 1.05])
        .paint(move |ctx| {
            for (i, (bar, &vol)) in visible_bars.iter().zip(values.iter()).enumerate() {
                let x = i as f64 + 0.5;
                let up = bar.close >= bar.open;
                let color = if up { Color::Green } else { Color::Red };
                let height = vol.max(max_v * 0.01);
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
