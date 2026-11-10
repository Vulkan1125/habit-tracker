//! Command line surface: everything the TUI can do, scriptable.

use std::collections::BTreeSet;
use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use chrono::{Duration, NaiveDate};
use clap::{Parser, Subcommand};

use crate::config::{Config, ThemeName};
use crate::format::{pad_left, pad_right, truncate, width};
use crate::model::{Data, Day, Schedule};
use crate::stats;
use crate::store::Store;

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
    pub data: Option<PathBuf>,

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
    /// List habits with today's status and streaks
    List,
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
    /// Print the effective configuration
    Config,
}

pub fn run(
    command: Command,
    store: &Store,
    data: &mut Data,
    today: NaiveDate,
    config: &Config,
) -> Result<()> {
    match command {
        Command::Add { name, weekly, days } => {
            let schedule = parse_schedule(weekly, days)?;
            let index = data.add_habit(&name, schedule, today)?;
            let habit = &data.habits[index];
            store.save(data)?;
            println!(
                "added {} [{}] · {}",
                habit.name,
                habit.id,
                habit.schedule.describe()
            );
        }
        Command::Done { habit, date } => {
            let index = data.resolve_index(&habit)?;
            let target = date.unwrap_or(today);
            if target > today {
                bail!("{target} is in the future; nothing to check off yet");
            }
            let habit = data.habits[index].clone();
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
        Command::List => print_list(data, today, config),
        Command::Stats { habit, days } => print_stats(data, habit.as_deref(), today, config, days)?,
        Command::Log { count, all } => print_log(data, today, count, all),
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
        Command::Config => {
            let path = Config::path();
            let state = match &path {
                Some(path) if path.exists() => path.display().to_string(),
                Some(path) => format!("{} (missing, using defaults)", path.display()),
                None => "(no config directory)".to_string(),
            };
            println!("{} {}", pad_right("config", 10), state);
            println!("{} {}", pad_right("data", 10), store.path().display());
            println!("{} {}", pad_right("theme", 10), theme_name(config.theme));
            println!(
                "{} {}",
                pad_right("week start", 10),
                config.week_start.long()
            );
        }
    }
    Ok(())
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

fn streak_summary(
    habit: &crate::model::Habit,
    data: &Data,
    today: NaiveDate,
    config: &Config,
) -> String {
    match stats::weekly_progress(habit, &data.log, today, config.week_start) {
        Some((done, target)) => format!("{done}/{target} this week"),
        None => {
            let streak = stats::current_streak(habit, &data.log, today, config.week_start);
            format!("streak {}", streak.label())
        }
    }
}

fn print_list(data: &Data, today: NaiveDate, config: &Config) {
    if data.habits.is_empty() {
        println!("No habits yet. Try: habit add \"Drink water\"");
        return;
    }

    let id_width = data
        .habits
        .iter()
        .map(|habit| width(&habit.id))
        .max()
        .unwrap_or(4)
        .clamp(4, 18);
    let name_width = data
        .habits
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
    for habit in &data.habits {
        let marker = if habit.archived {
            "·"
        } else if data.log.is_done(today, &habit.id) {
            "✓"
        } else if habit.schedule.is_due(today) {
            "○"
        } else {
            "·"
        };
        let streak = stats::current_streak(habit, &data.log, today, config.week_start);
        let streak_label = if streak.length == 0 {
            "—".to_string()
        } else {
            streak.label()
        };
        let archived = if habit.archived { " (archived)" } else { "" };
        println!(
            "{} {} {} {} {}{}",
            pad_right(&habit.id, id_width),
            pad_right(&habit.name, name_width),
            pad_right(&habit.schedule.describe(), 13),
            pad_left(marker, 5),
            pad_right(&streak_label, 10),
            archived
        );
    }
    println!();
    let active = data.habits.iter().filter(|habit| !habit.archived).count() as u32;
    println!(
        "Today: {} done · {} · space toggles in the TUI",
        data.done_today(today),
        crate::format::plural(active, "active habit")
    );
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
        let streak = stats::current_streak(habit, &data.log, today, config.week_start);
        let best = stats::longest_streak(habit, &data.log, today, config.week_start);
        let completion = stats::completion(habit, &data.log, today, config.week_start, days);
        let (week_done, week_target) =
            stats::week_progress(habit, &data.log, today, config.week_start);

        println!(
            "{} · {} · since {}{}",
            habit.name,
            habit.schedule.describe(),
            habit.created,
            if habit.archived { " · archived" } else { "" }
        );
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
            crate::format::plural(days, "day")
        );
        println!(
            "  {} {}",
            pad_right("check-ins", 12),
            data.log.total_for(&habit.id)
        );
    }
    Ok(())
}

fn print_log(data: &Data, today: NaiveDate, count: u32, all: bool) {
    let mut printed = 0;
    for back in 0..count {
        let date = today - Duration::days(i64::from(back));
        let ids = data.log.ids_on(date);
        if ids.is_empty() && !all {
            continue;
        }
        let names: Vec<&str> = ids
            .iter()
            .filter_map(|id| {
                data.habits
                    .iter()
                    .find(|habit| habit.id == *id)
                    .map(|habit| habit.name.as_str())
            })
            .collect();
        let summary = if names.is_empty() {
            "—".to_string()
        } else {
            truncate(&names.join(", "), 80)
        };
        println!("{date}  {summary}");
        printed += 1;
    }
    if printed == 0 {
        println!(
            "No check-ins in the last {}.",
            crate::format::plural(count, "day")
        );
    }
}

fn theme_name(theme: ThemeName) -> &'static str {
    match theme {
        ThemeName::Dark => "dark",
        ThemeName::Light => "light",
        ThemeName::Mono => "mono",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn date(text: &str) -> NaiveDate {
        NaiveDate::parse_from_str(text, "%Y-%m-%d").unwrap()
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
        let dir = tempfile::tempdir().unwrap();
        let store = Store::resolve(Some(dir.path().join("data.json"))).unwrap();
        let mut data = Data::default();
        let today = date("2026-11-10");
        run(
            Command::Add {
                name: "Gym".into(),
                weekly: None,
                days: None,
            },
            &store,
            &mut data,
            today,
            &Config::default(),
        )
        .unwrap();
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
        let dir = tempfile::tempdir().unwrap();
        let store = Store::resolve(Some(dir.path().join("data.json"))).unwrap();
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
        let dir = tempfile::tempdir().unwrap();
        let store = Store::resolve(Some(dir.path().join("data.json"))).unwrap();
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
}
