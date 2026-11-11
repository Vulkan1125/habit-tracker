//! The notifications module: one desktop notification per run.
//!
//! `habit` shells out to this binary so nobody who has not installed the
//! module pays for the notification code or sees reminder options. The
//! schedule still calls `habit remind`, which forwards here.

use std::path::PathBuf;

use anyhow::Result;
use chrono::{Local, NaiveDate, NaiveTime};
use clap::Parser;

use habit::config::Config;
use habit::reminders;
use habit::store::Store;

#[derive(Debug, Parser)]
#[command(
    name = "habit-notify",
    version,
    about = "The habit notifications module",
    long_about = "Sends the desktop notification for habits whose reminder time is now. Installed \
                  and run by `habit modules install notify`."
)]
struct Cli {
    /// Path to the data file
    #[arg(long, global = true, value_name = "FILE")]
    data: Option<PathBuf>,

    /// Pretend today is this date (YYYY-MM-DD)
    #[arg(long, global = true, value_name = "DATE")]
    today: Option<NaiveDate>,

    /// Only habits whose reminder is this time (HH:MM)
    #[arg(long, value_name = "HH:MM")]
    at: Option<String>,

    /// Print the notification instead of sending it
    #[arg(long)]
    dry_run: bool,

    /// Notify even when nothing is due
    #[arg(long)]
    always: bool,

    /// Send this title instead of the habit summary
    #[arg(long, requires = "body")]
    title: Option<String>,

    /// Send this body instead of the habit summary
    #[arg(long, requires = "title")]
    body: Option<String>,
}

fn main() -> std::process::ExitCode {
    match run() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("habit-notify: {error:#}");
            std::process::ExitCode::FAILURE
        }
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();
    let store = Store::resolve(cli.data)?;
    let today = cli.today.unwrap_or_else(|| Local::now().date_naive());

    if let (Some(title), Some(body)) = (&cli.title, &cli.body) {
        if reminders::notify(title, body, cli.dry_run)? || cli.dry_run {
            println!("{title}\n{body}");
        }
        return Ok(());
    }

    let data = store.load()?;
    let config = Config::load().0;
    let at = match cli.at {
        Some(text) => Some(
            NaiveTime::parse_from_str(text.trim(), "%H:%M").map_err(|_| {
                anyhow::anyhow!("--at needs a 24-hour time like 09:00, not {text:?}")
            })?,
        ),
        None => None,
    };
    remind(&data, today, &config, at, cli.dry_run, cli.always)
}

fn remind(
    data: &habit::model::Data,
    today: NaiveDate,
    config: &Config,
    at: Option<NaiveTime>,
    dry_run: bool,
    always: bool,
) -> Result<()> {
    if !data.habits.iter().any(|habit| habit.reminder.is_some()) {
        println!("no reminders set; give a habit one with `habit remind-time <habit> 09:00`");
        return Ok(());
    }

    let open = reminders::pending(data, today, config.week_start, at);
    if open.is_empty() {
        // A stale unit for a time no habit uses should stay quiet.
        if at.is_some_and(|time| !data.habits.iter().any(|habit| habit.reminder == Some(time))) {
            return Ok(());
        }
        if always {
            let title = "habit · all clear";
            let body = "All habits are done for today";
            if reminders::notify(title, body, dry_run)? || dry_run {
                println!("{title}\n{body}");
            }
        } else {
            println!("nothing left today · all clear");
        }
        return Ok(());
    }

    let (title, body) = reminders::message(&open);
    if reminders::notify(&title, &body, dry_run)? {
        println!("{title} · {body}");
    } else {
        println!("{title}\n{body}");
    }
    Ok(())
}
