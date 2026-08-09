//! Trading Telemetry TUI — Welcome → workspace charts over engine HTTP+WS.

mod app;
mod ipc;
mod ui;

use std::io::{self, stdout};
use std::time::Duration;

use anyhow::{Context, Result};
use app::{App, InputMode, PendingWatchlistOp, Screen};
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ipc::{
    post_chart_interest, post_indicators, post_watchlist_active, post_watchlist_add,
    post_watchlist_remove, post_workspace_layout, run_ipc_loop, EngineEndpoint, IpcEvent,
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
        InputMode::IndicatorPanel => match code {
            KeyCode::Esc | KeyCode::Char('o') | KeyCode::Char('O') => {
                app.close_indicator_panel();
            }
            KeyCode::Up => app.indicator_select_delta(-1),
            KeyCode::Down => app.indicator_select_delta(1),
            KeyCode::Char(' ') | KeyCode::Enter => app.indicator_toggle_selected(),
            KeyCode::Char('m') | KeyCode::Char('M') => app.indicator_add_default_ma_stack(),
            KeyCode::Char('v') | KeyCode::Char('V') => app.indicator_add_volume(),
            KeyCode::Char('p') | KeyCode::Char('P') => app.indicator_add_session_vp(),
            KeyCode::Char('x') | KeyCode::Char('X') => app.indicator_remove_selected(),
            KeyCode::Char('s') | KeyCode::Char('S') => app.indicator_cycle_style(),
            KeyCode::Char(']') => app.cycle_timeframe(1),
            KeyCode::Char('[') => app.cycle_timeframe(-1),
            KeyCode::Char('+') | KeyCode::Char('=') => app.indicator_adjust_length(1),
            KeyCode::Char('-') | KeyCode::Char('_') => app.indicator_adjust_length(-1),
            KeyCode::Char('1') => app.indicator_toggle_vp_level(0),
            KeyCode::Char('2') => app.indicator_toggle_vp_level(1),
            KeyCode::Char('3') => app.indicator_toggle_vp_level(2),
            KeyCode::Tab => app.focus_next(),
            KeyCode::Char('q') => app.quit(),
            _ => {}
        },
        InputMode::Normal => match code {
            KeyCode::Char('q') => app.quit(),
            KeyCode::Esc => {
                if app.screen == Screen::Welcome {
                    app.quit();
                }
            }
            KeyCode::Enter => app.enter_workspace(),
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
