//! `habit`, a terminal habit tracker.
//!
//! No arguments opens the TUI; with a subcommand it behaves like a normal CLI.

mod app;
mod cli;
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
use habit::config::Config;
use habit::store::Store;

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
    // Uninstall must work even when the data file is missing or broken, and it
    // must not let module migration reinstall what it is about to delete.
    if let Some(cli::Command::Uninstall { force, dry_run }) = cli.command.as_ref() {
        return cli::uninstall(&store, *force, *dry_run);
    }
    let today = cli.today.unwrap_or_else(|| Local::now().date_naive());
    let (config, warning) = Config::load();

    let mut data = store.load()?;
    // Upgrading must not switch reminders or reports off for existing data.
    let installed = habit::modules::migrate(store.path(), &data);

    match cli.command {
        Some(command) => {
            if let Some(warning) = warning {
                eprintln!("habit: {warning}");
            }
            for note in &installed {
                eprintln!("habit: {note}");
            }
            cli::run(command, &store, &mut data, today, &config)
        }
        None => run_tui(store, data, today, &config, warning, installed),
    }
}

fn run_tui(
    store: Store,
    data: habit::model::Data,
    today: NaiveDate,
    config: &Config,
    warning: Option<String>,
    installed: Vec<String>,
) -> Result<()> {
    let mut app = App::new(store, data, today, config);
    if let Some(warning) = warning {
        app.flash_warning(warning);
    }
    for note in installed {
        app.flash_warning(note);
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
