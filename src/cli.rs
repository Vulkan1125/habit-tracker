//! Command line surface: everything the TUI can do, scriptable.

use std::collections::BTreeSet;

use anyhow::{Context, Result, bail};
use chrono::{Duration, NaiveDate, NaiveTime};
use clap::{Parser, Subcommand};

use habit::config::Config;
use habit::csv;
use habit::format::{pad_left, pad_right, pretty_path, truncate, width};
use habit::model::{Data, Day, Habit, Schedule, normalize_tag};
use habit::modules;
use habit::reminders;
use habit::stats;
use habit::store::Store;
use habit::theme::ThemeName;
use habit::uninstall;

#[derive(Debug, Parser)]
#[command(
    name = "habit",
    version,
    about = "A calm terminal habit tracker",
    long_about = "A single-binary habit tracker: run `habit` for the TUI, or use the subcommands \
                  to script check-ins.\n\nData lives in ~/.local/share/habit/data.json unless \
                  --data or HABITS_FILE says otherwise."
)]
pub struct Cli {
    /// Path to the data file (overrides HABITS_FILE)
    #[arg(long, global = true, value_name = "FILE")]
    pub data: Option<std::path::PathBuf>,

    /// Pretend today is this date (YYYY-MM-DD)
    #[arg(long, global = true, value_name = "DATE")]
    pub today: Option<NaiveDate>,

    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Add a habit
    Add {
        /// Name, e.g. "Morning walk"
        name: String,
        /// Quota-based: n completions per week (1-7)
        #[arg(long, short = 'w', value_name = "N", conflicts_with = "days")]
        weekly: Option<u8>,
        /// Fixed days, comma separated (mon,wed,fri)
        #[arg(long, value_delimiter = ',', value_name = "DAYS")]
        days: Option<Vec<String>>,
        /// First day the habit counts (defaults to today)
        #[arg(long, value_name = "DATE")]
        start: Option<NaiveDate>,
        /// Last day the habit counts, inclusive
        #[arg(long = "end", value_name = "DATE")]
        ends: Option<NaiveDate>,
        /// Tag to attach; repeatable
        #[arg(long = "tag", value_name = "TAG")]
        tags: Vec<String>,
        /// Description to attach
        #[arg(long = "description", visible_alias = "note", value_name = "TEXT")]
        description: Option<String>,
        /// Daily reminder time, 24-hour HH:MM (opt-in)
        #[arg(long, value_name = "HH:MM")]
        remind: Option<String>,
    },
    /// Check a habit off
    Done {
        /// Habit id or name (a unique prefix works)
        habit: String,
        /// Backfill another day (YYYY-MM-DD)
        #[arg(long, value_name = "DATE")]
        date: Option<NaiveDate>,
    },
    /// Un-check a habit
    Undo {
        habit: String,
        #[arg(long, value_name = "DATE")]
        date: Option<NaiveDate>,
    },
    /// Excuse a day: one per calendar month, does not break the streak
    Cheat {
        habit: String,
        #[arg(long, value_name = "DATE")]
        date: Option<NaiveDate>,
        /// Remove the cheat day instead
        #[arg(long)]
        off: bool,
    },
    /// List habits with today's status and streaks
    List {
        /// Only these tags (repeat for AND, use health|fitness for OR)
        #[arg(long = "tag", value_name = "TAG")]
        tags: Vec<String>,
    },
    /// Show streak and completion statistics
    Stats {
        /// Only this habit
        #[arg(long, value_name = "HABIT")]
        habit: Option<String>,
        /// Window for the completion rate, in days
        #[arg(long, default_value_t = 30, value_name = "N")]
        days: u32,
    },
    /// Show recent check-ins
    Log {
        /// How many days back to look
        #[arg(short = 'n', long, default_value_t = 14, value_name = "N")]
        count: u32,
        /// Include days with no check-ins
        #[arg(long)]
        all: bool,
    },
    /// Export every habit and its history as CSV
    Export {
        /// Write to this file instead of stdout
        #[arg(long, short = 'o', value_name = "FILE")]
        output: Option<std::path::PathBuf>,
    },
    /// Import a CSV export (additive; existing habits are kept)
    Import {
        /// CSV file to read
        #[arg(value_name = "FILE")]
        file: std::path::PathBuf,
    },
    /// Toggle a tag on a habit
    Tag {
        habit: String,
        /// Tags to add or remove
        #[arg(required = true, value_name = "TAG")]
        tags: Vec<String>,
        /// Remove instead of add
        #[arg(long)]
        remove: bool,
    },
    /// List every tag with its habit count
    Tags,
    /// Rename a tag everywhere it is used (merges if the name is already taken)
    RenameTag {
        /// The tag to rename
        #[arg(value_name = "OLD")]
        from: String,
        /// The new name
        #[arg(value_name = "NEW")]
        to: String,
    },
    /// Fold one tag into another that already exists
    MergeTags {
        /// The tag to fold away
        #[arg(value_name = "FROM")]
        from: String,
        /// The tag that stays
        #[arg(value_name = "INTO")]
        into: String,
    },
    /// Set or clear a habit's description
    #[command(alias = "note")]
    Describe {
        habit: String,
        /// The description text; omit to print the current description
        #[arg(value_name = "TEXT")]
        text: Vec<String>,
        /// Remove the description
        #[arg(long)]
        clear: bool,
    },
    /// Change the first day a habit counts
    Start {
        habit: String,
        /// New start date (YYYY-MM-DD)
        #[arg(value_name = "DATE")]
        date: NaiveDate,
    },
    /// Set or clear the last day a habit counts
    End {
        habit: String,
        /// New end date (YYYY-MM-DD); defaults to today
        #[arg(long, value_name = "DATE")]
        date: Option<NaiveDate>,
        /// Remove the end date
        #[arg(long)]
        clear: bool,
    },
    /// Archive or restore a habit (history is kept)
    Archive { habit: String },
    /// Delete a habit and its history
    Remove {
        habit: String,
        /// Required: deleting is permanent
        #[arg(long)]
        force: bool,
    },
    /// Print the data file path
    Path,
    /// Notify about habits that are still open today
    Remind {
        /// Only habits whose reminder is this time (HH:MM)
        #[arg(long, value_name = "HH:MM")]
        at: Option<String>,
        /// Print the reminder instead of sending a desktop notification
        #[arg(long)]
        dry_run: bool,
        /// Notify even when nothing is due
        #[arg(long)]
        always: bool,
    },
    /// Generate a self-contained monthly HTML report
    Report {
        /// Month as YYYY-MM (defaults to the current month)
        #[arg(long, value_name = "YYYY-MM", conflicts_with = "last_month")]
        month: Option<String>,
        /// Report on the previous calendar month
        #[arg(long)]
        last_month: bool,
        /// Write here instead of the data file's reports directory
        #[arg(long, value_name = "FILE")]
        output: Option<std::path::PathBuf>,
        /// Open the report when it is written
        #[arg(long)]
        open: bool,
        /// Notify with the path (used by the scheduled job)
        #[arg(long)]
        notify: bool,
    },
    /// Set, show or clear a habit's daily reminder time
    RemindTime {
        /// Habit id or name (a unique prefix works)
        habit: String,
        /// 24-hour HH:MM; omit to print the current time
        #[arg(value_name = "HH:MM")]
        time: Option<String>,
        /// Turn reminders off for this habit
        #[arg(long, conflicts_with = "time")]
        clear: bool,
    },
    /// Install a daily reminder with launchd (macOS) or systemd (Linux)
    Schedule {
        #[command(subcommand)]
        action: ScheduleAction,
    },
    /// List, install or remove optional feature modules
    Modules {
        #[command(subcommand)]
        action: Option<ModulesAction>,
    },
    /// Remove habit completely: data, backups, modules, reports, config, jobs and binaries
    Uninstall {
        /// Do not ask for confirmation
        #[arg(long)]
        force: bool,
        /// Show what would be removed without deleting anything
        #[arg(long)]
        dry_run: bool,
    },
    /// Print the effective configuration
    Config,
    /// List the available themes
    Themes,
}

#[derive(Debug, Subcommand)]
pub enum ModulesAction {
    /// List every module and whether it is installed
    List,
    /// Install a module (notify or report)
    Install {
        /// Module name: notify or report
        #[arg(value_name = "NAME")]
        name: String,
    },
    /// Remove an installed module and its scheduled jobs
    Remove {
        /// Module name: notify or report
        #[arg(value_name = "NAME")]
        name: String,
    },
}

#[derive(Debug, Subcommand)]
pub enum ScheduleAction {
    /// Install or refresh one daily job per habit reminder time
    Install {
        /// Print the unit files instead of installing them
        #[arg(long)]
        dry_run: bool,
    },
    /// Stop and delete every reminder job
    Remove {
        /// Print what would be removed without touching anything
        #[arg(long)]
        dry_run: bool,
    },
    /// Show whether reminders are installed
    Status,
    /// Schedule the monthly HTML report
    Reports {
        #[command(subcommand)]
        action: ReportScheduleAction,
    },
}

#[derive(Debug, Subcommand)]
pub enum ReportScheduleAction {
    /// Install or replace the monthly report job
    Install {
        /// Day of the month (1-28, so every month has it)
        #[arg(long, default_value_t = 1, value_parser = clap::value_parser!(u8).range(1..=28))]
        day: u8,
        /// Time of day, 24-hour HH:MM
        #[arg(long, default_value = "09:00", value_name = "HH:MM")]
        at: String,
        /// Print the unit files instead of installing them
        #[arg(long)]
        dry_run: bool,
    },
    /// Stop and delete the monthly report job
    Remove {
        /// Print what would be removed without touching anything
        #[arg(long)]
        dry_run: bool,
    },
    /// Show whether the monthly report job is installed
    Status,
}

pub fn run(
    command: Command,
    store: &Store,
    data: &mut Data,
    today: NaiveDate,
    config: &Config,
) -> Result<()> {
    match command {
        Command::Add {
            name,
            weekly,
            days,
            start,
            ends,
            tags,
            description,
            remind,
        } => {
            if remind.is_some() {
                ensure_module(store, "notify")?;
            }
            let schedule = parse_schedule(weekly, days)?;
            let reminder = remind.as_deref().map(parse_time).transpose()?;
            let start = start.unwrap_or(today);
            let index =
                data.add_habit_full(&name, schedule, start, ends, &tags, description.as_deref())?;
            data.habits[index].reminder = reminder;
            let habit = &data.habits[index];
            store.save(data)?;
            println!(
                "added {} [{}] · {} · {}",
                habit.name,
                habit.id,
                habit.schedule.describe(),
                habit.window_label()
            );
            if !habit.tags.is_empty() {
                println!("  tags {}", tags_line(habit));
            }
            if let Some(time) = reminder {
                println!("  reminder {}", time.format("%H:%M"));
                print_reminder_guidance();
            }
        }
        Command::Done { habit, date } => {
            let index = data.resolve_index(&habit)?;
            let target = date.unwrap_or(today);
            let habit = data.habits[index].clone();
            if target > today {
                bail!("{target} is in the future; nothing to check off yet");
            }
            ensure_active(&habit, target)?;
            if data.log.is_done(target, &habit.id) {
                println!(
                    "{} was already done on {target} · {}",
                    habit.name,
                    streak_summary(&habit, data, today, config)
                );
                return Ok(());
            }
            data.log.set(target, &habit.id, true);
            store.save(data)?;
            println!(
                "✓ {} on {target} · {}",
                habit.name,
                streak_summary(&habit, data, today, config)
            );
        }
        Command::Undo { habit, date } => {
            let index = data.resolve_index(&habit)?;
            let target = date.unwrap_or(today);
            let habit = data.habits[index].clone();
            if !data.log.is_done(target, &habit.id) {
                println!("{} was not checked off on {target}", habit.name);
                return Ok(());
            }
            data.log.set(target, &habit.id, false);
            store.save(data)?;
            println!("✗ {} undone on {target}", habit.name);
        }
        Command::Cheat { habit, date, off } => {
            let index = data.resolve_index(&habit)?;
            let target = date.unwrap_or(today);
            let habit = data.habits[index].clone();
            if off {
                data.cheats.set(target, &habit.id, false);
                store.save(data)?;
                println!("◆ {} cheat day removed from {target}", habit.name);
                return Ok(());
            }
            if target > today {
                bail!("{target} is in the future; nothing to excuse yet");
            }
            ensure_active(&habit, target)?;
            if data.cheats.is_cheat(target, &habit.id) {
                println!("◆ is already a cheat day for {} on {target}", habit.name);
                return Ok(());
            }
            data.cheats.set(target, &habit.id, true);
            store.save(data)?;
            let used = data.cheats.used_in_month(&habit.id, target);
            let month = target.format("%B");
            if used == 1 {
                println!(
                    "◆ {} on {target} · excused (1/1 cheat day in {month})",
                    habit.name
                );
            } else if data.cheats.is_excused(target, &habit.id) {
                println!(
                    "◆ {} on {target} · excused, but {used} cheats in {month}: later ones break the streak",
                    habit.name
                );
            } else {
                println!(
                    "◆ {} on {target} · breaks the streak: {used} cheats in {month}, only the earliest counts",
                    habit.name
                );
            }
        }
        Command::List { tags } => print_list(data, today, config, &tags)?,
        Command::Stats { habit, days } => print_stats(data, habit.as_deref(), today, config, days)?,
        Command::Log { count, all } => print_log(data, today, count, all),
        Command::Export { output } => export_csv(data, output.as_deref())?,
        Command::Import { file } => import_csv(store, data, &file)?,
        Command::Tag {
            habit,
            tags,
            remove,
        } => {
            let index = data.resolve_index(&habit)?;
            let habit = &mut data.habits[index];
            let name = habit.name.clone();
            let mut changed = Vec::new();
            for tag in &tags {
                let ok = if remove {
                    habit.remove_tag(tag)
                } else {
                    habit.add_tag(tag)
                };
                if ok {
                    changed.push(normalize_tag(tag).unwrap_or_else(|| tag.trim().to_string()));
                }
            }
            let display = tags_line(habit);
            store.save(data)?;
            if changed.is_empty() {
                println!("{name} already had exactly those tags");
            } else if remove {
                println!("removed {} from {name} · {display}", changed.join(", "));
            } else {
                println!("tagged {name} · {display}");
            }
        }
        Command::Tags => print_tags(data),
        Command::RenameTag { from, to } => rename_tag(store, data, &from, &to)?,
        Command::MergeTags { from, into } => {
            // Merging is the careful spelling of a rename: the target has to be
            // a real tag, so a typo cannot quietly create a new one.
            if !data.has_tag(&into) {
                let into = normalize_tag(&into).unwrap_or(into);
                bail!("there is no tag #{into}; use `habit rename-tag` to create one");
            }
            rename_tag(store, data, &from, &into)?
        }
        Command::Describe { habit, text, clear } => {
            let index = data.resolve_index(&habit)?;
            let habit = &mut data.habits[index];
            let name = habit.name.clone();
            if clear {
                habit.note = None;
                store.save(data)?;
                println!("description cleared on {name}");
            } else if text.is_empty() {
                match &habit.note {
                    Some(note) => println!("{note}"),
                    None => println!("{name} has no description"),
                }
            } else {
                let note = text.join(" ").trim().to_string();
                habit.note = (!note.is_empty()).then_some(note.clone());
                store.save(data)?;
                println!("described {name}: {note}");
            }
        }
        Command::Start { habit, date } => {
            let index = data.resolve_index(&habit)?;
            let habit = &mut data.habits[index];
            if habit.ends.is_some_and(|ends| ends < date) {
                bail!("the end date is before {date}; clear it first with `habit end --clear`");
            }
            habit.created = date;
            let name = habit.name.clone();
            let window = habit.window_label();
            store.save(data)?;
            println!("{name} starts on {date} · {window}");
        }
        Command::End { habit, date, clear } => {
            let index = data.resolve_index(&habit)?;
            let habit = &mut data.habits[index];
            let name = habit.name.clone();
            if clear {
                habit.ends = None;
                store.save(data)?;
                println!("{name} no longer has an end date");
                return Ok(());
            }
            let ends = date.unwrap_or(today);
            if ends < habit.created {
                bail!(
                    "the end date ({ends}) is before the start ({})",
                    habit.created
                );
            }
            habit.ends = Some(ends);
            let finished = habit.is_finished(today);
            store.save(data)?;
            println!(
                "{name} runs until {ends}{}",
                if finished { " · finished" } else { "" }
            );
        }
        Command::Archive { habit } => {
            let index = data.resolve_index(&habit)?;
            let habit = &mut data.habits[index];
            habit.archived = !habit.archived;
            let name = habit.name.clone();
            let archived = habit.archived;
            store.save(data)?;
            if archived {
                println!("archived {name} · history kept, shown dimmed");
            } else {
                println!("restored {name}");
            }
        }
        Command::Remove { habit, force } => {
            let index = data.resolve_index(&habit)?;
            let name = data.habits[index].name.clone();
            let check_ins = data.log.total_for(&data.habits[index].id);
            if !force {
                bail!("refusing to delete {name} and {check_ins} check-ins without --force");
            }
            data.remove_habit(index);
            store.save(data)?;
            println!("deleted {name} and {check_ins} check-ins");
        }
        Command::Path => println!("{}", store.path().display()),
        Command::Remind {
            at,
            dry_run,
            always,
        } => {
            let mut args = Vec::new();
            if let Some(at) = at {
                parse_time(&at)?;
                args.push("--at".to_string());
                args.push(at);
            }
            if dry_run {
                args.push("--dry-run".to_string());
            }
            if always {
                args.push("--always".to_string());
            }
            run_module(store, today, "notify", &args)?;
        }
        Command::Report {
            month,
            last_month,
            output,
            open,
            notify,
        } => {
            let mut args = Vec::new();
            if let Some(month) = month {
                args.push("--month".to_string());
                args.push(month);
            }
            if last_month {
                args.push("--last-month".to_string());
            }
            if let Some(output) = output {
                args.push("--output".to_string());
                args.push(output.display().to_string());
            }
            if open {
                args.push("--open".to_string());
            }
            if notify {
                args.push("--notify".to_string());
            }
            run_module(store, today, "report", &args)?;
        }
        Command::RemindTime { habit, time, clear } => {
            ensure_module(store, "notify")?;
            remind_time(store, data, today, &habit, time.as_deref(), clear)?
        }
        Command::Schedule { action } => schedule(store, data, today, action)?,
        Command::Modules { action } => modules_command(store, data, action)?,
        Command::Uninstall { force, dry_run } => uninstall(store, force, dry_run)?,
        Command::Config => {
            let path = Config::path();
            let state = match &path {
                Some(path) if path.exists() => path.display().to_string(),
                Some(path) => format!("{} (missing, using defaults)", path.display()),
                None => "(no config directory)".to_string(),
            };
            println!("{} {}", pad_right("config", 10), state);
            println!("{} {}", pad_right("data", 10), store.path().display());
            println!("{} {}", pad_right("theme", 10), config.theme.label());
            println!("{} {}", pad_right("mode", 10), config.mode.label());
            println!(
                "{} {}",
                pad_right("week start", 10),
                config.week_start.long()
            );
        }
        Command::Themes => {
            for name in ThemeName::ALL {
                let marker = if name == config.theme { "→" } else { " " };
                println!("{marker} {:<12} light and dark", name.label());
            }
            println!(
                "\nset it in {} with `theme = \"catppuccin\"` and `mode = \"light\"`",
                Config::path()
                    .map(|path| path.display().to_string())
                    .unwrap_or_else(|| "config.toml".to_string())
            );
        }
    }
    Ok(())
}

/// Fail with a useful hint when a feature's module is not installed.
fn ensure_module(store: &Store, name: &str) -> Result<()> {
    if !modules::path_for(store.path(), name).is_file() {
        bail!("the {name} module is not installed · run `habit modules install {name}`");
    }
    Ok(())
}

/// Run an installed module, forwarding its output and exit status.
fn run_module(store: &Store, today: NaiveDate, name: &str, args: &[String]) -> Result<()> {
    ensure_module(store, name)?;
    let path = modules::path_for(store.path(), name);
    let data_path =
        std::path::absolute(store.path()).unwrap_or_else(|_| store.path().to_path_buf());
    let status = std::process::Command::new(&path)
        .arg("--data")
        .arg(data_path)
        .arg("--today")
        .arg(today.to_string())
        .args(args)
        .status()
        .with_context(|| format!("running {}", path.display()))?;
    if !status.success() {
        bail!("the {name} module exited with {status}");
    }
    Ok(())
}

pub fn uninstall(store: &Store, force: bool, dry_run: bool) -> Result<()> {
    let platform = reminders::platform();
    let mut jobs = Vec::new();
    for prefixes in [&reminders::REMINDER_PREFIXES, &reminders::REPORT_PREFIXES] {
        jobs.extend(reminders::installed(platform, prefixes)?);
    }
    jobs.sort();
    jobs.dedup();

    let current = std::env::current_exe().unwrap_or_else(|_| std::path::PathBuf::from("habit"));
    let config_path = Config::path();
    let plan = uninstall::plan(store.path(), config_path.as_deref(), &current, jobs);

    if plan.is_empty() {
        println!("nothing to remove; habit is already gone");
        return Ok(());
    }

    println!("habit uninstall will remove:");
    for (path, label) in plan.paths() {
        println!("  {} {}", pad_right(label, 30), pretty_path(path));
    }
    if let Some(note) = plan.package_manager {
        println!("\n{note}.");
    }
    if dry_run {
        println!("\ndry run: nothing was removed");
        return Ok(());
    }
    if !force && !confirm_uninstall()? {
        println!("cancelled; nothing was removed");
        return Ok(());
    }

    let outcome = uninstall::remove(&plan);
    for failure in &outcome.failures {
        eprintln!("habit: could not remove {failure}");
    }
    if !outcome.failures.is_empty() {
        bail!(
            "removed {} item(s), but some files are still there",
            outcome.removed
        );
    }
    println!("removed {} item(s); habit is uninstalled", outcome.removed);
    Ok(())
}

fn confirm_uninstall() -> Result<bool> {
    use std::io::{self, Write};

    print!("Type \"yes\" to confirm: ");
    io::stdout().flush().ok();
    let mut answer = String::new();
    io::stdin().read_line(&mut answer)?;
    Ok(answer.trim().eq_ignore_ascii_case("yes"))
}

fn modules_command(store: &Store, data: &Data, action: Option<ModulesAction>) -> Result<()> {
    match action.unwrap_or(ModulesAction::List) {
        ModulesAction::List => {
            let installed = modules::Modules::detect(store.path());
            println!(
                "{} {} what it adds",
                pad_right("MODULE", 9),
                pad_right("STATUS", 14)
            );
            let mut missing = 0;
            for module in modules::ALL {
                let status = if installed.installed(module.name) {
                    "installed"
                } else if modules::source(module.name).is_some() {
                    missing += 1;
                    "not installed"
                } else {
                    "unavailable"
                };
                println!(
                    "{} {} {}",
                    pad_right(module.name, 9),
                    pad_right(status, 14),
                    module.description
                );
            }
            if missing > 0 {
                println!("\ninstall one with `habit modules install <name>`");
            }
            if modules::NAMES
                .iter()
                .any(|name| modules::source(name).is_none())
            {
                println!("`unavailable` means this habit build did not ship that module");
            }
        }
        ModulesAction::Install { name } => {
            if !modules::NAMES.contains(&name.as_str()) {
                bail!(
                    "unknown module \"{name}\" (choose from {})",
                    modules::NAMES.join(", ")
                );
            }
            let path = modules::install(store.path(), &name)?;
            println!("installed {name} · {}", habit::format::pretty_path(&path));
            match name.as_str() {
                "notify" => {
                    if data.habits.iter().any(|habit| habit.reminder.is_some()) {
                        print_reminder_guidance();
                    }
                }
                "report" => {
                    println!(
                        "  run `habit report` for this month, or `habit schedule reports install`"
                    );
                }
                _ => {}
            }
        }
        ModulesAction::Remove { name } => {
            if !modules::NAMES.contains(&name.as_str()) {
                bail!(
                    "unknown module \"{name}\" (choose from {})",
                    modules::NAMES.join(", ")
                );
            }
            let (removed, jobs) = modules::remove_with_jobs(store.path(), &name)?;
            if removed {
                println!("removed the {name} module");
            } else {
                println!("the {name} module was not installed");
            }
            if jobs > 0 {
                println!(
                    "stopped {}",
                    habit::format::plural(jobs as u32, "scheduled job")
                );
            }
            match name.as_str() {
                "notify" => {
                    println!("  reminder times stay saved; install it again to bring them back")
                }
                "report" => println!("  existing report files are left alone"),
                _ => {}
            }
        }
    }
    Ok(())
}

fn remind_time(
    store: &Store,
    data: &mut Data,
    today: NaiveDate,
    habit: &str,
    time: Option<&str>,
    clear: bool,
) -> Result<()> {
    let index = data.resolve_index(habit)?;
    let name = data.habits[index].name.clone();

    if clear {
        if data.habits[index].reminder.take().is_none() {
            println!("reminders are already off for {name}");
            return Ok(());
        }
        store.save(data)?;
        println!("reminders off for {name}");
        refresh_reminders(data, today, store)?;
        return Ok(());
    }

    let Some(raw) = time else {
        match data.habits[index].reminder {
            Some(at) => println!("{name} reminds at {}", at.format("%H:%M")),
            None => println!("reminders are off for {name}"),
        }
        return Ok(());
    };

    let at = parse_time(raw)?;
    data.habits[index].reminder = Some(at);
    store.save(data)?;
    println!("{name} reminds at {}", at.format("%H:%M"));
    refresh_reminders(data, today, store)?;
    Ok(())
}

fn refresh_reminders(data: &Data, today: NaiveDate, store: &Store) -> Result<()> {
    if reminders::any_installed(reminders::platform(), &reminders::REMINDER_PREFIXES) {
        let count = sync_reminders(data, today, store)?;
        println!(
            "schedule refreshed · {}",
            habit::format::plural(count as u32, "time")
        );
    } else {
        print_reminder_guidance();
    }
    Ok(())
}

fn print_reminder_guidance() {
    println!("  notifications are opt-in: run `habit schedule install` to hand the daily");
    println!("  check to launchd (macOS) or systemd (Linux); nothing runs in the meantime");
}

fn sync_reminders(data: &Data, today: NaiveDate, store: &Store) -> Result<usize> {
    let platform = reminders::platform();
    let program = std::env::current_exe().context("finding the habit binary")?;
    let data_path =
        std::path::absolute(store.path()).unwrap_or_else(|_| store.path().to_path_buf());
    let times = reminders::times(data, today);
    let units = reminders::reminder_units(platform, &program, &data_path, &times)?;
    reminders::sync(platform, &units, &reminders::REMINDER_PREFIXES)?;
    Ok(times.len())
}

fn print_units(units: &reminders::Units) {
    for unit in &units.units {
        println!("# {}", habit::format::pretty_path(&unit.path));
        print!("{}", unit.body);
        if !unit.body.ends_with('\n') {
            println!();
        }
    }
}

fn schedule(store: &Store, data: &Data, today: NaiveDate, action: ScheduleAction) -> Result<()> {
    let platform = reminders::platform();
    let program = std::env::current_exe().context("finding the habit binary")?;
    // launchd and systemd start with a minimal PATH, so the unit carries the
    // binary and the data file as absolute paths.
    let data_path =
        std::path::absolute(store.path()).unwrap_or_else(|_| store.path().to_path_buf());

    match action {
        ScheduleAction::Install { dry_run } => {
            ensure_module(store, "notify")?;
            let times = reminders::times(data, today);
            let units = reminders::reminder_units(platform, &program, &data_path, &times)?;
            if dry_run {
                if times.is_empty() {
                    println!("no reminder times set; give a habit one with `habit remind-time`");
                } else {
                    print_units(&units);
                }
                return Ok(());
            }
            let removed = reminders::sync(platform, &units, &reminders::REMINDER_PREFIXES)?;
            for path in removed {
                println!("removed stale {}", habit::format::pretty_path(&path));
            }
            if times.is_empty() {
                println!(
                    "no reminders set; give a habit one with `habit remind-time <habit> 09:00`"
                );
            } else {
                let labels = times
                    .iter()
                    .map(|time| time.format("%H:%M").to_string())
                    .collect::<Vec<_>>()
                    .join(", ");
                println!(
                    "reminders installed for {labels} · {}",
                    habit::format::plural(times.len() as u32, "time")
                );
                for unit in &units.units {
                    println!("  {}", habit::format::pretty_path(&unit.path));
                }
            }
        }
        ScheduleAction::Remove { dry_run } => {
            if dry_run {
                for path in reminders::installed(platform, &reminders::REMINDER_PREFIXES)? {
                    println!("would remove {}", habit::format::pretty_path(&path));
                }
                return Ok(());
            }
            let removed = reminders::remove(platform, &reminders::REMINDER_PREFIXES)?;
            if removed.is_empty() {
                println!("no scheduled reminder found");
            } else {
                for path in removed {
                    println!("removed {}", habit::format::pretty_path(&path));
                }
            }
        }
        ScheduleAction::Status => {
            if platform == reminders::Platform::Other {
                println!(
                    "scheduling is not supported on this platform; run `habit remind` from cron"
                );
                return Ok(());
            }
            let rows = reminders::status(platform, &reminders::REMINDER_PREFIXES)?;
            if rows.is_empty() {
                println!("no reminder jobs installed");
                println!("`habit schedule install` sets them up from each habit's reminder time");
            }
            for (path, _, live) in &rows {
                let state = if *live {
                    "loaded"
                } else {
                    "written, not loaded"
                };
                println!(
                    "{} {}",
                    pad_right(state, 20),
                    habit::format::pretty_path(path)
                );
            }
        }
        ScheduleAction::Reports { action } => {
            if matches!(action, ReportScheduleAction::Install { .. }) {
                ensure_module(store, "report")?;
            }
            schedule_reports(store, today, &program, &data_path, action)?;
        }
    }
    Ok(())
}

fn schedule_reports(
    store: &Store,
    today: NaiveDate,
    program: &std::path::Path,
    data_path: &std::path::Path,
    action: ReportScheduleAction,
) -> Result<()> {
    let platform = reminders::platform();
    match action {
        ReportScheduleAction::Install { day, at, dry_run } => {
            let at = parse_time(&at)?;
            let units = reminders::report_units(platform, program, data_path, day, at)?;
            if dry_run {
                print_units(&units);
                return Ok(());
            }
            let removed = reminders::sync(platform, &units, &reminders::REPORT_PREFIXES)?;
            for path in removed {
                println!("removed stale {}", habit::format::pretty_path(&path));
            }
            println!(
                "monthly report scheduled for day {day} at {} · {}",
                at.format("%H:%M"),
                habit::format::pretty_path(&habit::report::path_for(
                    store.path(),
                    habit::report::last_month(today)
                ))
            );
        }
        ReportScheduleAction::Remove { dry_run } => {
            if dry_run {
                for path in reminders::installed(platform, &reminders::REPORT_PREFIXES)? {
                    println!("would remove {}", habit::format::pretty_path(&path));
                }
                return Ok(());
            }
            let removed = reminders::remove(platform, &reminders::REPORT_PREFIXES)?;
            if removed.is_empty() {
                println!("no monthly report job found");
            } else {
                for path in removed {
                    println!("removed {}", habit::format::pretty_path(&path));
                }
            }
        }
        ReportScheduleAction::Status => {
            if platform == reminders::Platform::Other {
                println!(
                    "scheduling is not supported on this platform; run `habit report --last-month` from cron"
                );
                return Ok(());
            }
            let rows = reminders::status(platform, &reminders::REPORT_PREFIXES)?;
            if rows.is_empty() {
                println!("no monthly report job installed");
                println!("`habit schedule reports install` sets it up");
            }
            for (path, _, live) in &rows {
                let state = if *live {
                    "loaded"
                } else {
                    "written, not loaded"
                };
                println!(
                    "{} {}",
                    pad_right(state, 20),
                    habit::format::pretty_path(path)
                );
            }
        }
    }
    Ok(())
}

fn parse_time(raw: &str) -> Result<NaiveTime> {
    NaiveTime::parse_from_str(raw.trim(), "%H:%M")
        .with_context(|| format!("unknown time \"{raw}\" (use 24-hour HH:MM, like 09:00)"))
}

fn ensure_active(habit: &Habit, date: NaiveDate) -> Result<()> {
    if habit.is_active_on(date) {
        return Ok(());
    }
    bail!(
        "{} does not run on {date} ({})",
        habit.name,
        habit.window_label()
    )
}

fn parse_schedule(weekly: Option<u8>, days: Option<Vec<String>>) -> Result<Schedule> {
    if let Some(n) = weekly {
        let schedule = Schedule::TimesPerWeek { n };
        schedule.validate()?;
        return Ok(schedule);
    }
    if let Some(raw) = days {
        let mut set = BTreeSet::new();
        for chunk in raw {
            for token in chunk.split(',') {
                let token = token.trim();
                if token.is_empty() {
                    continue;
                }
                let day = Day::parse(token)
                    .with_context(|| format!("unknown day \"{token}\" (use mon,tue,… or 1-7)"))?;
                set.insert(day);
            }
        }
        let schedule = Schedule::Weekdays { days: set };
        schedule.validate()?;
        return Ok(schedule);
    }
    Ok(Schedule::Daily)
}

fn streak_summary(habit: &Habit, data: &Data, today: NaiveDate, config: &Config) -> String {
    match stats::weekly_progress(habit, &data.history(), today, config.week_start) {
        Some((done, target)) => format!("{done}/{target} this week"),
        None => {
            let streak = stats::current_streak(habit, &data.history(), today, config.week_start);
            format!("streak {}", streak.label())
        }
    }
}

fn tags_line(habit: &Habit) -> String {
    habit
        .tags
        .iter()
        .map(|tag| format!("#{tag}"))
        .collect::<Vec<_>>()
        .join(" ")
}

/// One `--tag` argument may offer alternatives with `|`; separate arguments are
/// still ANDed together. Unknown or empty names are ignored, as before.
fn tag_groups(tags: &[String]) -> Vec<Vec<String>> {
    tags.iter()
        .map(|arg| arg.split('|').filter_map(normalize_tag).collect())
        .filter(|group: &Vec<String>| !group.is_empty())
        .collect()
}

fn matches_tag_groups(habit: &Habit, groups: &[Vec<String>]) -> bool {
    groups
        .iter()
        .all(|group| group.iter().any(|tag| habit.has_tag(tag)))
}

fn export_csv(data: &Data, output: Option<&std::path::Path>) -> Result<()> {
    let text = csv::export(data);
    match output {
        Some(path) => {
            std::fs::write(path, text)
                .with_context(|| format!("could not write {}", path.display()))?;
            println!(
                "exported {} to {}",
                habit::format::plural(data.habits.len() as u32, "habit"),
                pretty_path(path)
            );
        }
        None => print!("{text}"),
    }
    Ok(())
}

fn import_csv(store: &Store, data: &mut Data, file: &std::path::Path) -> Result<()> {
    let text = std::fs::read_to_string(file)
        .with_context(|| format!("could not read {}", file.display()))?;
    let report = csv::import(data, &text)?;
    let changed =
        report.habits_added > 0 || report.checkins > 0 || report.cheats > 0 || report.notes > 0;
    if !changed {
        println!("nothing new to import from {}", pretty_path(file));
        return Ok(());
    }
    store.save(data)?;

    let mut parts = Vec::new();
    if report.habits_added > 0 {
        parts.push(habit::format::plural(
            report.habits_added as u32,
            "new habit",
        ));
    }
    if report.habits_merged > 0 {
        parts.push(habit::format::plural(
            report.habits_merged as u32,
            "existing habit",
        ));
    }
    if report.checkins > 0 {
        parts.push(habit::format::plural(report.checkins, "check-in"));
    }
    if report.cheats > 0 {
        parts.push(habit::format::plural(report.cheats, "cheat day"));
    }
    if report.notes > 0 {
        parts.push(habit::format::plural(report.notes, "note"));
    }
    println!("imported {} from {}", parts.join(", "), pretty_path(file));
    Ok(())
}

fn print_list(data: &Data, today: NaiveDate, config: &Config, tags: &[String]) -> Result<()> {
    if data.habits.is_empty() {
        println!("No habits yet. Try: habit add \"Drink water\"");
        return Ok(());
    }

    let groups = tag_groups(tags);
    let rows = data
        .habits
        .iter()
        .filter(|habit| matches_tag_groups(habit, &groups))
        .collect::<Vec<_>>();
    if rows.is_empty() {
        println!("No habits match those tags.");
        return Ok(());
    }

    let id_width = rows
        .iter()
        .map(|habit| width(&habit.id))
        .max()
        .unwrap_or(4)
        .clamp(4, 18);
    let name_width = rows
        .iter()
        .map(|habit| width(&habit.name))
        .max()
        .unwrap_or(6)
        .clamp(6, 28);

    println!(
        "{} {} {} TODAY STREAK",
        pad_right("ID", id_width),
        pad_right("HABIT", name_width),
        pad_right("SCHEDULE", 13)
    );
    for habit in &rows {
        let marker = if habit.archived || habit.is_finished(today) {
            "·"
        } else if data.log.is_done(today, &habit.id) {
            "✓"
        } else if data.cheats.is_excused(today, &habit.id) {
            "◆"
        } else if habit.schedule.is_due(today) && habit.is_active_on(today) {
            "○"
        } else {
            "·"
        };
        let streak = stats::current_streak(habit, &data.history(), today, config.week_start);
        let streak_label = if streak.length == 0 {
            "-".to_string()
        } else {
            streak.label()
        };
        let state = if habit.archived {
            " (archived)"
        } else if habit.is_finished(today) {
            " (finished)"
        } else {
            ""
        };
        println!(
            "{} {} {} {} {}{}",
            pad_right(&habit.id, id_width),
            pad_right(&habit.name, name_width),
            pad_right(&habit.schedule.describe(), 13),
            pad_left(marker, 5),
            pad_right(&streak_label, 10),
            state
        );
    }
    println!();
    let active = rows
        .iter()
        .filter(|habit| !habit.archived && !habit.is_finished(today) && habit.is_active_on(today))
        .count() as u32;
    println!(
        "Today: {} done · {} · space toggles in the TUI",
        data.done_today(today),
        habit::format::plural(active, "active habit")
    );
    Ok(())
}

fn print_stats(
    data: &Data,
    only: Option<&str>,
    today: NaiveDate,
    config: &Config,
    days: u32,
) -> Result<()> {
    let indices: Vec<usize> = match only {
        Some(key) => vec![data.resolve_index(key)?],
        None => (0..data.habits.len()).collect(),
    };
    if indices.is_empty() {
        println!("No habits yet. Try: habit add \"Drink water\"");
        return Ok(());
    }

    for (position, index) in indices.iter().enumerate() {
        if position > 0 {
            println!();
        }
        let habit = &data.habits[*index];
        let history = data.history();
        let streak = stats::current_streak(habit, &history, today, config.week_start);
        let best = stats::longest_streak(habit, &history, today, config.week_start);
        let completion = stats::completion(habit, &history, today, config.week_start, days);
        let (week_done, week_target) =
            stats::week_progress(habit, &history, today, config.week_start);

        let state = if habit.archived {
            " · archived"
        } else if habit.is_finished(today) {
            " · finished"
        } else {
            ""
        };
        println!(
            "{} · {} · {}{}",
            habit.name,
            habit.schedule.describe(),
            habit.window_label(),
            state
        );
        if !habit.tags.is_empty() {
            println!("  {}", tags_line(habit));
        }
        println!(
            "  {} {} (best {})",
            pad_right("streak", 12),
            streak.label(),
            best.label()
        );
        println!(
            "  {} {}/{}",
            pad_right("this week", 12),
            week_done,
            week_target
        );
        println!(
            "  {} {}% ({} of {}) over {}",
            pad_right("completion", 12),
            completion.percent(),
            completion.done,
            completion.possible,
            habit::format::plural(days, "day")
        );
        println!(
            "  {} {}",
            pad_right("check-ins", 12),
            data.log.total_for(&habit.id)
        );
        let cheats = data.cheats.used_in_month(&habit.id, today);
        if cheats > 0 {
            println!(
                "  {} {} this month (limit 1)",
                pad_right("cheat days", 12),
                cheats
            );
        }
    }
    Ok(())
}

fn print_log(data: &Data, today: NaiveDate, count: u32, all: bool) {
    let mut printed = 0;
    for back in 0..count {
        let date = today - Duration::days(i64::from(back));
        let ids = data.log.ids_on(date);
        let cheats = data.cheats.ids_on(date);
        if ids.is_empty() && cheats.is_empty() && !all {
            continue;
        }
        let names: Vec<String> = ids
            .iter()
            .filter_map(|id| {
                data.habits
                    .iter()
                    .find(|habit| habit.id == *id)
                    .map(|habit| habit.name.clone())
            })
            .collect();
        let excused: Vec<String> = cheats
            .iter()
            .filter_map(|id| {
                data.habits
                    .iter()
                    .find(|habit| habit.id == *id)
                    .map(|habit| {
                        if data.cheats.is_excused(date, id) {
                            format!("◆ {}", habit.name)
                        } else {
                            format!("◆ {} (extra, broke the streak)", habit.name)
                        }
                    })
            })
            .collect();
        let mut parts = names;
        parts.extend(excused);
        let summary = if parts.is_empty() {
            "-".to_string()
        } else {
            truncate(&parts.join(", "), 80)
        };
        println!("{date}  {summary}");
        printed += 1;
    }
    if printed == 0 {
        println!(
            "No check-ins in the last {}.",
            habit::format::plural(count, "day")
        );
    }
}

fn print_tags(data: &Data) {
    let mut counts: std::collections::BTreeMap<&str, u32> = std::collections::BTreeMap::new();
    for habit in &data.habits {
        for tag in &habit.tags {
            *counts.entry(tag.as_str()).or_default() += 1;
        }
    }
    if counts.is_empty() {
        println!("No tags yet. Try: habit tag gym health");
        return;
    }
    for (tag, count) in counts {
        println!("#{:<20} {}", tag, habit::format::plural(count, "habit"));
    }
}

/// Renames one tag onto another, saving only when something changed.
fn rename_tag(store: &Store, data: &mut Data, from: &str, to: &str) -> Result<()> {
    let outcome = match data.rename_tag(from, to) {
        Ok(outcome) => outcome,
        Err(error) => bail!("{}", error.message()),
    };
    store.save(data)?;
    let habits = habit::format::plural(outcome.habits as u32, "habit");
    if outcome.merged {
        println!("merged #{} into #{} on {habits}", outcome.from, outcome.to);
    } else {
        println!("renamed #{} to #{} on {habits}", outcome.from, outcome.to);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn date(text: &str) -> NaiveDate {
        NaiveDate::parse_from_str(text, "%Y-%m-%d").unwrap()
    }

    fn add(name: &str) -> Command {
        Command::Add {
            name: name.into(),
            weekly: None,
            days: None,
            start: None,
            ends: None,
            tags: Vec::new(),
            description: None,
            remind: None,
        }
    }

    fn store() -> (tempfile::TempDir, Store) {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::resolve(Some(dir.path().join("data.json"))).unwrap();
        (dir, store)
    }

    #[cfg(unix)]
    fn install_fake_module(store: &Store, name: &str) {
        let dir = store.path().parent().unwrap().join("build");
        std::fs::create_dir_all(&dir).unwrap();
        let source = dir.join(format!("habit-{name}"));
        std::fs::write(&source, "#!/bin/sh\nexit 0\n").unwrap();
        modules::install_from(store.path(), name, &source).unwrap();
    }

    fn remind() -> Command {
        Command::Remind {
            at: None,
            dry_run: true,
            always: false,
        }
    }

    #[test]
    fn reminder_and_report_commands_need_their_modules() {
        let (_dir, store) = store();
        let mut data = Data::default();
        let today = date("2026-11-10");

        let mut command = add("Gym");
        if let Command::Add { remind, .. } = &mut command {
            *remind = Some("09:00".into());
        }
        let error = run(command, &store, &mut data, today, &Config::default()).unwrap_err();
        assert!(
            error.to_string().contains("modules install notify"),
            "{error}"
        );
        assert!(data.habits.is_empty(), "nothing may be saved");

        let error = run(
            Command::Report {
                month: None,
                last_month: false,
                output: None,
                open: false,
                notify: false,
            },
            &store,
            &mut data,
            today,
            &Config::default(),
        )
        .unwrap_err();
        assert!(
            error.to_string().contains("modules install report"),
            "{error}"
        );

        let error = run(remind(), &store, &mut data, today, &Config::default()).unwrap_err();
        assert!(
            error.to_string().contains("modules install notify"),
            "{error}"
        );
    }

    #[cfg(unix)]
    #[test]
    fn an_installed_module_is_executed() {
        let (_dir, store) = store();
        install_fake_module(&store, "notify");
        let mut data = Data::default();
        let today = date("2026-11-10");
        run(add("Gym"), &store, &mut data, today, &Config::default()).unwrap();
        assert!(modules::Modules::detect(store.path()).notify);
        run(remind(), &store, &mut data, today, &Config::default()).unwrap();

        run(
            Command::Modules {
                action: Some(ModulesAction::List),
            },
            &store,
            &mut data,
            today,
            &Config::default(),
        )
        .unwrap();
    }

    #[test]
    fn schedule_time_accepts_24_hour_and_rejects_garbage() {
        assert_eq!(
            parse_time("09:05").unwrap(),
            NaiveTime::from_hms_opt(9, 5, 0).unwrap()
        );
        assert!(parse_time("9pm").is_err());
    }

    #[test]
    fn weekly_flag_wins_over_nothing() {
        let schedule = parse_schedule(Some(4), None).unwrap();
        assert_eq!(schedule, Schedule::TimesPerWeek { n: 4 });
    }

    #[test]
    fn weekday_list_is_parsed_and_deduplicated() {
        let schedule = parse_schedule(None, Some(vec!["mon,wed".into(), "Mon".into()])).unwrap();
        assert_eq!(
            schedule,
            Schedule::Weekdays {
                days: BTreeSet::from([Day::Mon, Day::Wed])
            }
        );
    }

    #[test]
    fn unknown_day_is_an_error() {
        let error = parse_schedule(None, Some(vec!["funday".into()])).unwrap_err();
        assert!(error.to_string().contains("funday"));
    }

    #[test]
    fn out_of_range_weekly_is_rejected() {
        assert!(parse_schedule(Some(0), None).is_err());
        assert!(parse_schedule(Some(9), None).is_err());
    }

    #[test]
    fn default_schedule_is_daily() {
        assert_eq!(parse_schedule(None, None).unwrap(), Schedule::Daily);
    }

    #[test]
    fn cli_definition_is_valid() {
        use clap::CommandFactory;
        Cli::command().debug_assert();
    }

    #[test]
    fn done_command_updates_the_log() {
        let (_dir, store) = store();
        let mut data = Data::default();
        let today = date("2026-11-10");
        run(add("Gym"), &store, &mut data, today, &Config::default()).unwrap();
        run(
            Command::Done {
                habit: "gym".into(),
                date: None,
            },
            &store,
            &mut data,
            today,
            &Config::default(),
        )
        .unwrap();
        assert!(store.load().unwrap().log.is_done(today, "gym"));
    }

    #[test]
    fn future_completion_is_refused() {
        let (_dir, store) = store();
        let mut data = Data::default();
        let today = date("2026-11-10");
        data.add_habit("Gym", Schedule::Daily, today).unwrap();
        let error = run(
            Command::Done {
                habit: "gym".into(),
                date: Some(date("2026-11-11")),
            },
            &store,
            &mut data,
            today,
            &Config::default(),
        )
        .unwrap_err();
        assert!(error.to_string().contains("future"));
    }

    #[test]
    fn remove_requires_force() {
        let (_dir, store) = store();
        let mut data = Data::default();
        let today = date("2026-11-10");
        data.add_habit("Gym", Schedule::Daily, today).unwrap();
        let error = run(
            Command::Remove {
                habit: "gym".into(),
                force: false,
            },
            &store,
            &mut data,
            today,
            &Config::default(),
        )
        .unwrap_err();
        assert!(error.to_string().contains("--force"));
        assert_eq!(data.habits.len(), 1);
    }

    #[test]
    fn add_carries_dates_tags_and_the_description() {
        let (_dir, store) = store();
        let mut data = Data::default();
        let today = date("2026-11-10");
        run(
            Command::Add {
                name: "Read".into(),
                weekly: None,
                days: None,
                start: Some(date("2026-11-01")),
                ends: Some(date("2026-12-31")),
                tags: vec!["Health".into(), "morning".into()],
                description: Some("20 pages".into()),
                remind: None,
            },
            &store,
            &mut data,
            today,
            &Config::default(),
        )
        .unwrap();
        let habit = &data.habits[0];
        assert_eq!(habit.created, date("2026-11-01"));
        assert_eq!(habit.ends, Some(date("2026-12-31")));
        assert_eq!(habit.tags, vec!["health", "morning"]);
        assert_eq!(habit.note.as_deref(), Some("20 pages"));
    }

    #[test]
    fn cheat_is_idempotent_and_reversible() {
        let (_dir, store) = store();
        let mut data = Data::default();
        let today = date("2026-11-10");
        data.add_habit("Gym", Schedule::Daily, date("2026-11-01"))
            .unwrap();
        run(
            Command::Cheat {
                habit: "gym".into(),
                date: Some(date("2026-11-05")),
                off: false,
            },
            &store,
            &mut data,
            today,
            &Config::default(),
        )
        .unwrap();
        assert_eq!(data.cheats.used_in_month("gym", today), 1);
        run(
            Command::Cheat {
                habit: "gym".into(),
                date: Some(date("2026-11-05")),
                off: true,
            },
            &store,
            &mut data,
            today,
            &Config::default(),
        )
        .unwrap();
        assert_eq!(data.cheats.used_in_month("gym", today), 0);
    }

    #[test]
    fn cheat_outside_the_window_is_refused() {
        let (_dir, store) = store();
        let mut data = Data::default();
        let today = date("2026-11-10");
        data.add_habit("Gym", Schedule::Daily, date("2026-11-01"))
            .unwrap();
        let error = run(
            Command::Cheat {
                habit: "gym".into(),
                date: Some(date("2026-10-01")),
                off: false,
            },
            &store,
            &mut data,
            today,
            &Config::default(),
        )
        .unwrap_err();
        assert!(error.to_string().contains("does not run"));
    }

    #[test]
    fn rename_tag_rewrites_every_habit_and_saves() {
        let (_dir, store) = store();
        let mut data = Data::default();
        let today = date("2026-11-10");
        data.add_habit("Gym", Schedule::Daily, today).unwrap();
        data.add_habit("Read", Schedule::Daily, today).unwrap();
        data.habits[0].tags = vec!["health".into()];
        data.habits[1].tags = vec!["health".into(), "mind".into()];

        run(
            Command::RenameTag {
                from: "#Health".into(),
                to: "Fitness".into(),
            },
            &store,
            &mut data,
            today,
            &Config::default(),
        )
        .unwrap();

        assert_eq!(data.habits[0].tags, vec!["fitness"]);
        assert_eq!(data.habits[1].tags, vec!["fitness", "mind"]);
        // Saved, not just changed in memory.
        let reloaded = store.load().unwrap();
        assert_eq!(reloaded.habits[0].tags, vec!["fitness"]);
    }

    #[test]
    fn rename_tag_onto_a_taken_name_merges() {
        let (_dir, store) = store();
        let mut data = Data::default();
        let today = date("2026-11-10");
        data.add_habit("Gym", Schedule::Daily, today).unwrap();
        data.habits[0].tags = vec!["health".into(), "body".into()];

        run(
            Command::RenameTag {
                from: "health".into(),
                to: "body".into(),
            },
            &store,
            &mut data,
            today,
            &Config::default(),
        )
        .unwrap();
        assert_eq!(data.habits[0].tags, vec!["body"]);
    }

    #[test]
    fn merge_tags_needs_a_real_target() {
        let (_dir, store) = store();
        let mut data = Data::default();
        let today = date("2026-11-10");
        data.add_habit("Gym", Schedule::Daily, today).unwrap();
        data.habits[0].tags = vec!["health".into()];

        // A typo in the target must not quietly rename the tag.
        let error = run(
            Command::MergeTags {
                from: "health".into(),
                into: "bdy".into(),
            },
            &store,
            &mut data,
            today,
            &Config::default(),
        )
        .unwrap_err();
        assert!(error.to_string().contains("no tag #bdy"), "{error}");
        assert_eq!(data.habits[0].tags, vec!["health"]);

        run(
            Command::MergeTags {
                from: "health".into(),
                into: "body".into(),
            },
            &store,
            &mut data,
            today,
            &Config::default(),
        )
        .unwrap_err();
        assert_eq!(data.habits[0].tags, vec!["health"]);
    }

    #[test]
    fn a_missing_tag_cannot_be_renamed() {
        let (_dir, store) = store();
        let mut data = Data::default();
        let today = date("2026-11-10");
        data.add_habit("Gym", Schedule::Daily, today).unwrap();

        let error = run(
            Command::RenameTag {
                from: "nope".into(),
                to: "body".into(),
            },
            &store,
            &mut data,
            today,
            &Config::default(),
        )
        .unwrap_err();
        assert!(error.to_string().contains("no habit carries"), "{error}");
    }

    #[test]
    fn tags_can_be_added_and_removed() {
        let (_dir, store) = store();
        let mut data = Data::default();
        let today = date("2026-11-10");
        data.add_habit("Gym", Schedule::Daily, today).unwrap();
        run(
            Command::Tag {
                habit: "gym".into(),
                tags: vec!["Health".into(), "morning".into()],
                remove: false,
            },
            &store,
            &mut data,
            today,
            &Config::default(),
        )
        .unwrap();
        assert_eq!(data.habits[0].tags, vec!["health", "morning"]);
        run(
            Command::Tag {
                habit: "gym".into(),
                tags: vec!["morning".into()],
                remove: true,
            },
            &store,
            &mut data,
            today,
            &Config::default(),
        )
        .unwrap();
        assert_eq!(data.habits[0].tags, vec!["health"]);
    }

    #[test]
    fn descriptions_can_be_set_cleared_and_read() {
        let (_dir, store) = store();
        let mut data = Data::default();
        let today = date("2026-11-10");
        data.add_habit("Gym", Schedule::Daily, today).unwrap();
        run(
            Command::Describe {
                habit: "gym".into(),
                text: vec!["ten".into(), "minutes".into()],
                clear: false,
            },
            &store,
            &mut data,
            today,
            &Config::default(),
        )
        .unwrap();
        assert_eq!(data.habits[0].note.as_deref(), Some("ten minutes"));
        run(
            Command::Describe {
                habit: "gym".into(),
                text: Vec::new(),
                clear: true,
            },
            &store,
            &mut data,
            today,
            &Config::default(),
        )
        .unwrap();
        assert!(data.habits[0].note.is_none());
    }

    #[test]
    fn end_must_not_precede_the_start() {
        let (_dir, store) = store();
        let mut data = Data::default();
        let today = date("2026-11-10");
        data.add_habit("Gym", Schedule::Daily, date("2026-11-01"))
            .unwrap();
        let error = run(
            Command::End {
                habit: "gym".into(),
                date: Some(date("2026-10-01")),
                clear: false,
            },
            &store,
            &mut data,
            today,
            &Config::default(),
        )
        .unwrap_err();
        assert!(error.to_string().contains("before the start"));
    }

    #[test]
    fn start_moves_the_window() {
        let (_dir, store) = store();
        let mut data = Data::default();
        let today = date("2026-11-10");
        data.add_habit("Gym", Schedule::Daily, date("2026-11-01"))
            .unwrap();
        run(
            Command::Start {
                habit: "gym".into(),
                date: date("2026-11-03"),
            },
            &store,
            &mut data,
            today,
            &Config::default(),
        )
        .unwrap();
        assert_eq!(data.habits[0].created, date("2026-11-03"));
    }

    #[test]
    fn list_filters_by_tags() {
        let (_dir, _store) = store();
        let mut data = Data::default();
        let today = date("2026-11-10");
        data.add_habit("Gym", Schedule::Daily, today).unwrap();
        data.add_habit("Read", Schedule::Daily, today).unwrap();
        data.habits[0].tags = vec!["health".into()];
        data.habits[1].tags = vec!["mind".into()];
        print_list(&data, today, &Config::default(), &["health".into()]).unwrap();
        let error = print_list(&data, today, &Config::default(), &["nope".into()]);
        assert!(error.is_ok());
    }

    #[test]
    fn export_then_import_round_trips_through_a_file() {
        let (dir, store) = store();
        let today = date("2026-11-10");
        let mut data = Data::default();
        let index = data
            .add_habit("Gym", Schedule::Daily, today - Duration::days(3))
            .unwrap();
        let id = data.habits[index].id.clone();
        data.log.set(today - Duration::days(1), &id, true);
        data.habits[index].add_tag("health");

        let file = dir.path().join("habits.csv");
        run(
            Command::Export {
                output: Some(file.clone()),
            },
            &store,
            &mut data,
            today,
            &Config::default(),
        )
        .unwrap();
        assert!(file.exists());

        let mut restored = Data::default();
        run(
            Command::Import { file: file.clone() },
            &store,
            &mut restored,
            today,
            &Config::default(),
        )
        .unwrap();
        assert_eq!(restored, data);

        // Importing the same file again adds nothing.
        run(
            Command::Import { file },
            &store,
            &mut restored,
            today,
            &Config::default(),
        )
        .unwrap();
        assert_eq!(restored, data);
    }

    #[test]
    fn piped_tags_match_either_while_arguments_stay_anded() {
        let mut data = Data::default();
        let today = date("2026-11-10");
        data.add_habit("Gym", Schedule::Daily, today).unwrap();
        data.add_habit("Read", Schedule::Daily, today).unwrap();
        data.habits[0].tags = vec!["health".into(), "morning".into()];
        data.habits[1].tags = vec!["mind".into()];

        let either = tag_groups(&["health|mind".into()]);
        assert!(matches_tag_groups(&data.habits[0], &either));
        assert!(matches_tag_groups(&data.habits[1], &either));

        let both = tag_groups(&["health|mind".into(), "morning".into()]);
        assert!(matches_tag_groups(&data.habits[0], &both));
        assert!(
            !matches_tag_groups(&data.habits[1], &both),
            "separate --tag arguments are ANDed"
        );

        // Empty or unparsable groups do not filter anything out.
        let empty = tag_groups(&["!!!".into()]);
        assert!(empty.is_empty());
        assert!(matches_tag_groups(&data.habits[0], &empty));
    }
}
