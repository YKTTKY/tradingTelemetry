//! Ratatui views: Welcome, workspace feed status, and single-chart candles.

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

use crate::app::{App, ChartSeriesState, ConnectionStatus, InputMode, Screen, UNAVAILABLE_COPY};
use crate::ipc::OhlcvBar;

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
    let prompt_h = if matches!(app.input_mode, InputMode::InstrumentPrompt { .. }) {
        3
    } else {
        0
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

    draw_chart(frame, chunks[1], app);

    if let InputMode::InstrumentPrompt { buffer } = &app.input_mode {
        let prompt = Paragraph::new(format!("Instrument: {buffer}_")).block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Change instrument (Enter apply · Esc cancel) "),
        );
        frame.render_widget(prompt, chunks[2]);
    }

    let layout = match app.layout {
        crate::app::LayoutMode::Single => "single",
    };
    let help = Paragraph::new(format!(
        "{layout} · {} · {}  ·  [ ] timeframe  ·  i instrument  ·  q quit",
        app.chart.instrument, app.chart.timeframe
    ))
    .style(Style::default().fg(Color::DarkGray));
    frame.render_widget(help, chunks[3]);
}

fn draw_chart(frame: &mut Frame, area: Rect, app: &App) {
    let title = format!(" {} ", app.chart.title());
    match &app.chart.series {
        ChartSeriesState::Idle | ChartSeriesState::Loading => {
            let body = Paragraph::new("Loading history…")
                .alignment(Alignment::Center)
                .block(Block::default().borders(Borders::ALL).title(title));
            frame.render_widget(body, area);
        }
        ChartSeriesState::Unavailable => {
            let copy = app.empty_state_copy().unwrap_or(UNAVAILABLE_COPY);
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
            draw_candles(frame, area, &title, bars);
        }
    }
}

fn draw_candles(frame: &mut Frame, area: Rect, title: &str, bars: &[OhlcvBar]) {
    if bars.is_empty() {
        let body = Paragraph::new(UNAVAILABLE_COPY)
            .alignment(Alignment::Center)
            .block(Block::default().borders(Borders::ALL).title(title.to_string()));
        frame.render_widget(body, area);
        return;
    }

    // Fit roughly one candle per terminal column (inner width minus borders).
    // Drawing 500 sub-pixel bodies makes the series unreadable even with good data.
    let inner_w = area.width.saturating_sub(2).max(8) as usize;
    let visible = visible_bar_window(bars, inner_w);
    let n = visible.len() as f64;

    let mut min_p = visible[0].low;
    let mut max_p = visible[0].high;
    for b in visible {
        min_p = min_p.min(b.low);
        max_p = max_p.max(b.high);
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
    let subtitle = format!(
        " O={:.2} H={:.2} L={:.2} C={:.2}  bars={}{}",
        last.open,
        last.high,
        last.low,
        last.close,
        bars.len(),
        if visible.len() < bars.len() {
            format!(" show={}", visible.len())
        } else {
            String::new()
        }
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

    let canvas = Canvas::default()
        .block(block)
        .marker(symbols::Marker::Block)
        .x_bounds([0.0, n])
        .y_bounds([min_p, max_p])
        .paint(|ctx| {
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
        });

    frame.render_widget(canvas, area);
}

/// Right-aligned window of bars that fit the chart width (~1 candle per column).
fn visible_bar_window(bars: &[OhlcvBar], max_bars: usize) -> &[OhlcvBar] {
    if bars.len() <= max_bars {
        bars
    } else {
        &bars[bars.len() - max_bars..]
    }
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
