//! Trading Telemetry TUI — Welcome → workspace charts over engine HTTP+WS.

mod app;
mod ipc;
mod overlay;
mod timeframe;
mod ui;

use std::io::{self, stdout};
use std::time::Duration;

use anyhow::{Context, Result};
use app::{App, IndicatorListSide, InputMode, PendingWatchlistOp, Screen};
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ipc::{
    post_chart_interest, post_indicators, post_type_styles, post_watchlist_active,
    post_watchlist_add, post_watchlist_remove, post_watchlist_rename, post_workspace_layout,
    run_ipc_loop,
    EngineEndpoint, IpcEvent,
};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use tokio::sync::mpsc;

const DEFAULT_ENGINE: &str = "http://127.0.0.1:8765";

#[tokio::main]
async fn main() -> Result<()> {
    let engine_url = std::env::var("ENGINE_URL").unwrap_or_else(|_| DEFAULT_ENGINE.to_string());
    let endpoint = EngineEndpoint::parse(&engine_url)
        .with_context(|| format!("invalid ENGINE_URL: {engine_url}"))?;

    let (tx, mut rx) = mpsc::unbounded_channel();
    tokio::spawn(run_ipc_loop(endpoint.clone(), tx.clone()));

    let mut terminal = setup_terminal()?;
    let mut app = App::default();

    let result = run_loop(&mut terminal, &mut app, &endpoint, &tx, &mut rx).await;

    restore_terminal(&mut terminal)?;
    result
}

async fn run_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
    endpoint: &EngineEndpoint,
    tx: &mpsc::UnboundedSender<IpcEvent>,
    rx: &mut mpsc::UnboundedReceiver<IpcEvent>,
) -> Result<()> {
    loop {
        while let Ok(ev) = rx.try_recv() {
            app.apply_ipc(ev);
        }

        if let Some(layout) = app.pending_layout {
            app.layout_request_started();
            let ep = endpoint.clone();
            let tx = tx.clone();
            let mode = layout.as_str().to_string();
            tokio::spawn(async move {
                match post_workspace_layout(&ep, &mode).await {
                    Ok(ws) => {
                        let _ = tx.send(IpcEvent::Workspace(ws));
                    }
                    Err(err) => {
                        let _ = tx.send(IpcEvent::WorkspaceFailed {
                            message: err.to_string(),
                        });
                    }
                }
            });
        }

        if let Some(op) = app.pending_watchlist.clone() {
            app.watchlist_request_started();
            let ep = endpoint.clone();
            let tx = tx.clone();
            tokio::spawn(async move {
                let result = match op {
                    PendingWatchlistOp::SetActive { watchlist_id } => {
                        post_watchlist_active(&ep, &watchlist_id).await
                    }
                    PendingWatchlistOp::Add { symbol } => post_watchlist_add(&ep, &symbol).await,
                    PendingWatchlistOp::Remove { symbol } => {
                        post_watchlist_remove(&ep, &symbol).await
                    }
                    PendingWatchlistOp::Rename { name } => {
                        post_watchlist_rename(&ep, &name).await
                    }
                };
                match result {
                    Ok(body) => {
                        let _ = tx.send(IpcEvent::WatchlistState {
                            workspace: body.workspace,
                            quotes: body.quotes,
                        });
                    }
                    Err(err) => {
                        let _ = tx.send(IpcEvent::WatchlistFailed {
                            message: err.to_string(),
                        });
                    }
                }
            });
        }

        if let Some(pending) = app.pending_indicators.clone() {
            app.indicators_request_started();
            let ep = endpoint.clone();
            let tx = tx.clone();
            tokio::spawn(async move {
                match post_indicators(&ep, &pending.chart_id, &pending.indicators).await {
                    Ok(body) => {
                        let _ = tx.send(IpcEvent::IndicatorsApplied(body));
                    }
                    Err(err) => {
                        let _ = tx.send(IpcEvent::IndicatorsFailed {
                            message: err.to_string(),
                        });
                    }
                }
            });
        }

        if let Some(pending) = app.pending_type_styles.clone() {
            app.type_styles_request_started();
            let ep = endpoint.clone();
            let tx = tx.clone();
            tokio::spawn(async move {
                // Local chart already holds styles; engine persists for restore.
                // Do not apply full workspace (would wipe bar series + re-interest).
                if let Err(err) =
                    post_type_styles(&ep, &pending.chart_id, pending.type_styles).await
                {
                    let _ = tx.send(IpcEvent::IndicatorsFailed {
                        message: format!("type styles: {err}"),
                    });
                }
            });
        }

        if app.needs_chart_load {
            app.chart_load_started();
            for chart in app.charts.iter() {
                let instrument = chart.instrument.clone();
                let timeframe = chart.timeframe.clone();
                let chart_id = chart.id.clone();
                let ep = endpoint.clone();
                let tx = tx.clone();
                tokio::spawn(async move {
                    match post_chart_interest(&ep, &instrument, &timeframe, &chart_id).await {
                        Ok(series) => {
                            let _ = tx.send(IpcEvent::ChartSeries(series));
                        }
                        Err(err) => {
                            let _ = tx.send(IpcEvent::ChartLoadFailed {
                                chart_id,
                                instrument,
                                timeframe,
                                message: err.to_string(),
                            });
                        }
                    }
                });
            }
        }

        terminal.draw(|frame| ui::draw(frame, app))?;

        if app.should_quit {
            break;
        }

        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    handle_key(app, key.code);
                }
            }
        }
    }
    Ok(())
}

fn handle_key(app: &mut App, code: KeyCode) {
    // Help overlay eats keys so underlying panels stay put.
    if app.help_open {
        match code {
            KeyCode::Esc
            | KeyCode::Enter
            | KeyCode::Char('?')
            | KeyCode::Char('h')
            | KeyCode::Char('H') => {
                app.close_help();
            }
            KeyCode::Char('q') | KeyCode::Char('Q') => app.quit(),
            _ => {}
        }
        return;
    }

    match &app.input_mode {
        InputMode::InstrumentPrompt { .. } => match code {
            KeyCode::Esc => app.cancel_prompt(),
            KeyCode::Enter => {
                let _ = app.apply_instrument_prompt();
            }
            KeyCode::Backspace => app.prompt_pop_char(),
            KeyCode::Char(c) => app.prompt_push_char(c),
            _ => {}
        },
        InputMode::WatchlistAddPrompt { .. } => match code {
            KeyCode::Esc => app.cancel_prompt(),
            KeyCode::Enter => {
                let _ = app.apply_watchlist_add_prompt();
            }
            KeyCode::Backspace => app.prompt_pop_char(),
            KeyCode::Char(c) => app.prompt_push_char(c),
            _ => {}
        },
        InputMode::WatchlistRenamePrompt { .. } => match code {
            KeyCode::Esc => app.cancel_prompt(),
            KeyCode::Enter => {
                let _ = app.apply_watchlist_rename_prompt();
            }
            KeyCode::Backspace => app.prompt_pop_char(),
            KeyCode::Char(c) => app.prompt_push_char(c),
            _ => {}
        },
        InputMode::IndicatorPanel => {
            // Type-style popup (Available · `c`) owns keys until confirm/cancel.
            if app.type_style_edit.is_some() {
                match code {
                    KeyCode::Char('?') | KeyCode::Char('h') | KeyCode::Char('H') => {
                        app.toggle_help()
                    }
                    KeyCode::Esc => app.type_style_cancel(),
                    KeyCode::Enter => app.type_style_confirm(),
                    KeyCode::Left | KeyCode::Char('-') | KeyCode::Char('_') => {
                        app.type_style_nudge(-1)
                    }
                    KeyCode::Right | KeyCode::Char('+') | KeyCode::Char('=') => {
                        app.type_style_nudge(1)
                    }
                    KeyCode::Char('q') => app.quit(),
                    _ => {}
                }
                return;
            }
            match code {
                KeyCode::Char('?') | KeyCode::Char('h') | KeyCode::Char('H') => app.toggle_help(),
                KeyCode::Esc | KeyCode::Char('o') | KeyCode::Char('O') => {
                    app.close_indicator_panel();
                }
                // Tab switches Available ↔ Current (not dual chart focus).
                KeyCode::Tab => app.indicator_toggle_list_side(),
                KeyCode::Up => app.indicator_select_delta(-1),
                KeyCode::Down => app.indicator_select_delta(1),
                // Space / Enter: Available = add; Current = on/off.
                // Re-pin stays on `r` so Enter never jumps into pin placement.
                KeyCode::Char(' ') | KeyCode::Enter => app.indicator_activate_selected(),
                KeyCode::Char('c') => match app.indicator_list_side {
                    IndicatorListSide::Available => app.indicator_open_type_style(),
                    // Current: clear-all (also Shift+C below).
                    IndicatorListSide::Current => app.indicator_clear_except_volume(),
                },
                KeyCode::Char('C') => {
                    // Documented clear-all binding: Shift+C on Current.
                    if app.indicator_list_side == IndicatorListSide::Current {
                        app.indicator_clear_except_volume();
                    }
                }
                // Power-user letter add shortcuts still work while the panel is open.
                KeyCode::Char('m') | KeyCode::Char('M') => app.indicator_add_default_ma_stack(),
                KeyCode::Char('v') | KeyCode::Char('V') => app.indicator_add_volume(),
                KeyCode::Char('p') | KeyCode::Char('P') => app.indicator_add_session_vp(),
                KeyCode::Char('f') | KeyCode::Char('F') => app.indicator_add_fixed_range_vp(),
                KeyCode::Char('a') | KeyCode::Char('A') => app.indicator_add_anchored_vp(),
                KeyCode::Char('y') | KeyCode::Char('Y') => app.indicator_add_gex(),
                KeyCode::Char('g') | KeyCode::Char('G') => app.indicator_add_garch(),
                KeyCode::Char('r') | KeyCode::Char('R')
                    if app.indicator_list_side == IndicatorListSide::Current =>
                {
                    let itype = app
                        .focused_chart()
                        .indicators
                        .get(app.indicator_selected)
                        .map(|i| i.indicator_type.as_str())
                        .unwrap_or("");
                    match itype {
                        "anchored_vp" => app.indicator_replace_avp_pin(),
                        "fixed_range_vp" => app.indicator_replace_frvp_pins(),
                        _ => {}
                    }
                }
                KeyCode::Char('e') | KeyCode::Char('E')
                    if app.indicator_list_side == IndicatorListSide::Current =>
                {
                    app.indicator_toggle_frvp_extend()
                }
                KeyCode::Char('9') if app.indicator_list_side == IndicatorListSide::Current => {
                    app.indicator_snap_avp_cash_open()
                }
                KeyCode::Char(',') if app.indicator_list_side == IndicatorListSide::Current => {
                    let itype = app
                        .focused_chart()
                        .indicators
                        .get(app.indicator_selected)
                        .map(|i| i.indicator_type.as_str())
                        .unwrap_or("");
                    if itype == "anchored_vp" {
                        app.indicator_nudge_avp_anchor(-1);
                    } else {
                        app.indicator_nudge_frvp_anchor(0, -1);
                    }
                }
                KeyCode::Char('.') if app.indicator_list_side == IndicatorListSide::Current => {
                    let itype = app
                        .focused_chart()
                        .indicators
                        .get(app.indicator_selected)
                        .map(|i| i.indicator_type.as_str())
                        .unwrap_or("");
                    if itype == "anchored_vp" {
                        app.indicator_nudge_avp_anchor(1);
                    } else {
                        app.indicator_nudge_frvp_anchor(0, 1);
                    }
                }
                KeyCode::Char('<') if app.indicator_list_side == IndicatorListSide::Current => {
                    app.indicator_nudge_frvp_anchor(1, -1)
                }
                KeyCode::Char('>') if app.indicator_list_side == IndicatorListSide::Current => {
                    app.indicator_nudge_frvp_anchor(1, 1)
                }
                KeyCode::Char('x') | KeyCode::Char('X')
                    if app.indicator_list_side == IndicatorListSide::Current =>
                {
                    app.indicator_remove_selected()
                }
                KeyCode::Char('s') | KeyCode::Char('S')
                    if app.indicator_list_side == IndicatorListSide::Current =>
                {
                    app.indicator_cycle_style()
                }
                KeyCode::Char(']') => app.cycle_timeframe(1),
                KeyCode::Char('[') => app.cycle_timeframe(-1),
                KeyCode::Char('+') | KeyCode::Char('=')
                    if app.indicator_list_side == IndicatorListSide::Current =>
                {
                    app.indicator_adjust_length(1)
                }
                KeyCode::Char('-') | KeyCode::Char('_')
                    if app.indicator_list_side == IndicatorListSide::Current =>
                {
                    app.indicator_adjust_length(-1)
                }
                KeyCode::Char('1') if app.indicator_list_side == IndicatorListSide::Current => {
                    app.indicator_toggle_vp_level(0)
                }
                KeyCode::Char('2') if app.indicator_list_side == IndicatorListSide::Current => {
                    app.indicator_toggle_vp_level(1)
                }
                KeyCode::Char('3') if app.indicator_list_side == IndicatorListSide::Current => {
                    app.indicator_toggle_vp_level(2)
                }
                KeyCode::Char('q') => app.quit(),
                _ => {}
            }
        }
        InputMode::FrvpPlacing => match code {
            // `?` only for help — h/l are vim-style pin nudges here.
            KeyCode::Char('?') => app.toggle_help(),
            KeyCode::Esc => app.frvp_place_cancel(),
            KeyCode::Left | KeyCode::Char('h') | KeyCode::Char('H') => app.frvp_place_move(-1),
            KeyCode::Right | KeyCode::Char('l') | KeyCode::Char('L') => app.frvp_place_move(1),
            // Faster scrub across long histories.
            KeyCode::Char('[') => app.frvp_place_move(-10),
            KeyCode::Char(']') => app.frvp_place_move(10),
            KeyCode::Enter | KeyCode::Char(' ') => app.frvp_place_confirm(),
            KeyCode::Char('q') => app.quit(),
            _ => {}
        },
        InputMode::AvpPlacing => match code {
            KeyCode::Char('?') => app.toggle_help(),
            KeyCode::Esc => app.avp_place_cancel(),
            KeyCode::Left | KeyCode::Char('h') | KeyCode::Char('H') => app.avp_place_move(-1),
            KeyCode::Right | KeyCode::Char('l') | KeyCode::Char('L') => app.avp_place_move(1),
            KeyCode::Char('[') => app.avp_place_move(-10),
            KeyCode::Char(']') => app.avp_place_move(10),
            KeyCode::Char('9') => app.avp_place_snap_cash_open(),
            KeyCode::Enter | KeyCode::Char(' ') => app.avp_place_confirm(),
            KeyCode::Char('q') => app.quit(),
            _ => {}
        },
        InputMode::Normal => match code {
            KeyCode::Char('?') | KeyCode::Char('h') | KeyCode::Char('H') => app.toggle_help(),
            KeyCode::Char('q') => app.quit(),
            KeyCode::Esc => {
                if app.screen == Screen::Welcome {
                    app.quit();
                }
            }
            // Welcome: Enter opens workspace. Workspace: Enter/Space load selected symbol.
            KeyCode::Enter if app.screen == Screen::Welcome => app.enter_workspace(),
            KeyCode::Enter | KeyCode::Char(' ') if app.screen == Screen::Workspace => {
                let _ = app.load_selected_watchlist_symbol();
            }
            KeyCode::Char(']') if app.screen == Screen::Workspace => app.cycle_timeframe(1),
            KeyCode::Char('[') if app.screen == Screen::Workspace => app.cycle_timeframe(-1),
            KeyCode::Char('i') | KeyCode::Char('I') if app.screen == Screen::Workspace => {
                app.begin_instrument_prompt();
            }
            KeyCode::Char('o') | KeyCode::Char('O') if app.screen == Screen::Workspace => {
                app.toggle_indicator_panel();
            }
            KeyCode::Char('l') | KeyCode::Char('L') if app.screen == Screen::Workspace => {
                app.toggle_layout();
            }
            KeyCode::Char('w') | KeyCode::Char('W') if app.screen == Screen::Workspace => {
                app.toggle_watchlist_sidebar();
            }
            KeyCode::Char('n') | KeyCode::Char('N') if app.screen == Screen::Workspace => {
                app.cycle_watchlist(1);
            }
            KeyCode::Char('p') | KeyCode::Char('P') if app.screen == Screen::Workspace => {
                app.cycle_watchlist(-1);
            }
            KeyCode::Char('a') | KeyCode::Char('A') if app.screen == Screen::Workspace => {
                app.begin_watchlist_add_prompt();
            }
            KeyCode::Char('r') | KeyCode::Char('R') if app.screen == Screen::Workspace => {
                app.begin_watchlist_rename_prompt();
            }
            KeyCode::Char('x') | KeyCode::Char('X') | KeyCode::Char('d') | KeyCode::Char('D')
                if app.screen == Screen::Workspace =>
            {
                app.remove_selected_watchlist_symbol();
            }
            KeyCode::Up if app.screen == Screen::Workspace => {
                app.watchlist_select_delta(-1);
            }
            KeyCode::Down if app.screen == Screen::Workspace => {
                app.watchlist_select_delta(1);
            }
            // Chart pan over loaded history (A2). Pin modes own ← → above.
            KeyCode::Left if app.screen == Screen::Workspace => {
                app.pan_focused_chart(-1);
            }
            KeyCode::Right if app.screen == Screen::Workspace => {
                app.pan_focused_chart(1);
            }
            KeyCode::Tab if app.screen == Screen::Workspace => {
                app.focus_next();
            }
            _ => {}
        },
    }
}

fn setup_terminal() -> Result<Terminal<CrosstermBackend<io::Stdout>>> {
    enable_raw_mode()?;
    let mut out = stdout();
    execute!(out, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(out);
    Terminal::new(backend).context("create terminal")
}

fn restore_terminal(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> Result<()> {
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    Ok(())
}
