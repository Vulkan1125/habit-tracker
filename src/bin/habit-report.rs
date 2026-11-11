//! The reports module: a self-contained monthly HTML page.
//!
//! `habit report` forwards here once the module is installed, and the
//! scheduled job calls `habit report --last-month --notify`, so the core app
//! never has to carry the renderer.

use std::path::PathBuf;
use std::process::Command;

use anyhow::{Context, Result, bail};
use chrono::{Local, NaiveDate};
use clap::Parser;

use habit::config::Config;
use habit::format::pretty_path;
use habit::modules;
use habit::report;
use habit::store::Store;

#[derive(Debug, Parser)]
#[command(
    name = "habit-report",
    version,
    about = "The habit reports module",
    long_about = "Writes one self-contained HTML page per month. Installed and run by `habit \
                  modules install report`."
)]
struct Cli {
    /// Path to the data file
    #[arg(long, global = true, value_name = "FILE")]
    data: Option<PathBuf>,

    /// Pretend today is this date (YYYY-MM-DD)
    #[arg(long, global = true, value_name = "DATE")]
    today: Option<NaiveDate>,

    /// Month as YYYY-MM (defaults to the current month)
    #[arg(long, value_name = "YYYY-MM", conflicts_with = "last_month")]
    month: Option<String>,

    /// Report on the previous calendar month
    #[arg(long)]
    last_month: bool,

    /// Write here instead of the data file's reports directory
    #[arg(long, value_name = "FILE")]
    output: Option<PathBuf>,

    /// Open the report when it is written
    #[arg(long)]
    open: bool,

    /// Notify with the path (used by the scheduled job)
    #[arg(long)]
    notify: bool,
}

fn main() -> std::process::ExitCode {
    match run() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("habit-report: {error:#}");
            std::process::ExitCode::FAILURE
        }
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();
    let store = Store::resolve(cli.data)?;
    let today = cli.today.unwrap_or_else(|| Local::now().date_naive());
    let data = store.load()?;
    let config = Config::load().0;

    let anchor = if cli.last_month {
        report::last_month(today)
    } else if let Some(text) = cli.month.as_deref() {
        report::month_start(text)?
    } else {
        today
    };
    let path = cli
        .output
        .unwrap_or_else(|| report::path_for(store.path(), anchor));
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("creating {}", parent.display()))?;
    }
    let html = report::render(&data, today, anchor, config.week_start);
    std::fs::write(&path, html).with_context(|| format!("writing {}", path.display()))?;
    println!("wrote {}", pretty_path(&path));

    if cli.notify {
        notify(store.path(), &title(anchor), &path);
    }
    if cli.open {
        open(&path)?;
    }
    Ok(())
}

fn title(anchor: NaiveDate) -> String {
    format!("Habit report · {}", anchor.format("%B %Y"))
}

/// Ask the notifications module to show the new path. A missing module is not
/// an error: the report itself was written.
fn notify(data_path: &std::path::Path, title: &str, path: &std::path::Path) {
    let module = modules::path_for(data_path, "notify");
    if !module.is_file() {
        eprintln!(
            "habit-report: report written, but the notifications module is not installed \
             (habit modules install notify)"
        );
        return;
    }
    let status = Command::new(&module)
        .arg("--title")
        .arg(title)
        .arg("--body")
        .arg(path)
        .status();
    if let Err(error) = status {
        eprintln!("habit-report: report written, but the notification failed: {error}");
    }
}

fn open(path: &std::path::Path) -> Result<()> {
    let command = if cfg!(target_os = "macos") {
        "open"
    } else if cfg!(target_os = "linux") {
        "xdg-open"
    } else {
        bail!(
            "no program known for opening files on this platform; open {} yourself",
            path.display()
        );
    };
    Command::new(command)
        .arg(path)
        .status()
        .with_context(|| format!("opening {}", path.display()))?;
    Ok(())
}
