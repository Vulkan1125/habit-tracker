//! `habit` — a terminal habit tracker.
//!
//! No arguments opens the TUI; with a subcommand it behaves like a normal CLI.

mod app;
mod cli;
mod config;
mod format;
mod model;
mod reminders;
mod stats;
mod store;
mod theme;
mod ui;

use std::io;
use std::process::ExitCode;
use std::time::Duration;

use anyhow::Result;
use chrono::{Local, NaiveDate};
use clap::Parser;
use crossterm::event::{DisableMouseCapture, EnableMouseCapture};
use crossterm::execute;

use crate::app::App;
use crate::cli::Cli;
use crate::config::Config;
use crate::store::Store;

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("habit: {error:#}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();
    let store = Store::resolve(cli.data)?;
    let today = cli.today.unwrap_or_else(|| Local::now().date_naive());
    let (config, warning) = Config::load();

    match cli.command {
        Some(command) => {
            if let Some(warning) = warning {
                eprintln!("habit: {warning}");
            }
            let mut data = store.load()?;
            cli::run(command, &store, &mut data, today, &config)
        }
        None => run_tui(store, today, &config, warning),
    }
}

fn run_tui(store: Store, today: NaiveDate, config: &Config, warning: Option<String>) -> Result<()> {
    let data = store.load()?;
    let mut app = App::new(store, data, today, config);
    if let Some(warning) = warning {
        app.flash_warning(warning);
    }

    let mut terminal = ratatui::try_init()?;
    if let Err(error) = execute!(io::stdout(), EnableMouseCapture) {
        ratatui::restore();
        return Err(error.into());
    }
    let result = event_loop(&mut terminal, &mut app);
    let _ = execute!(io::stdout(), DisableMouseCapture);
    // try_init installs its own panic hook that restores the terminal; this
    // covers the ordinary (non-panicking) exit path.
    ratatui::restore();
    result
}

fn event_loop(terminal: &mut ratatui::DefaultTerminal, app: &mut App) -> Result<()> {
    use crossterm::event;
    while !app.quit {
        terminal.draw(|frame| ui::draw(frame, app))?;
        if event::poll(Duration::from_millis(250))? {
            app.handle_event(event::read()?)?;
        }
        app.tick();
    }
    Ok(())
}
