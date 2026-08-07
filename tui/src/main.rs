//! Trading Telemetry TUI — Welcome → workspace shell over engine HTTP+WS.

mod app;
mod ipc;
mod ui;

use std::io::{self, stdout};
use std::time::Duration;

use anyhow::{Context, Result};
use app::App;
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ipc::{run_ipc_loop, EngineEndpoint};
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
    tokio::spawn(run_ipc_loop(endpoint, tx));

    let mut terminal = setup_terminal()?;
    let mut app = App::default();

    let result = run_loop(&mut terminal, &mut app, &mut rx).await;

    restore_terminal(&mut terminal)?;
    result
}

async fn run_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
    rx: &mut mpsc::UnboundedReceiver<ipc::IpcEvent>,
) -> Result<()> {
    loop {
        while let Ok(ev) = rx.try_recv() {
            app.apply_ipc(ev);
        }

        terminal.draw(|frame| ui::draw(frame, app))?;

        if app.should_quit {
            break;
        }

        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    match key.code {
                        KeyCode::Char('q') | KeyCode::Esc => app.quit(),
                        KeyCode::Enter => app.enter_workspace(),
                        _ => {}
                    }
                }
            }
        }
    }
    Ok(())
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
