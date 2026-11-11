//! Native reminders and the scheduler units that run them.
//!
//! There is no daemon here. `habit remind` is a single run that asks the
//! desktop for a notification, and the schedule commands hand small jobs to
//! launchd (macOS) or systemd (Linux). Unit contents are plain strings, so the
//! tests can inspect them without touching the real system.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};
use chrono::{NaiveDate, NaiveTime, Timelike};

use crate::format::{plural, truncate};
use crate::model::{Data, Day, Habit};
use crate::stats;

/// Filename prefixes that belong to reminders, including the 0.6.0 names.
pub const REMINDER_PREFIXES: [&str; 2] = ["com.habit.reminder", "habit-reminder"];
/// Filename prefixes that belong to the monthly report job.
pub const REPORT_PREFIXES: [&str; 2] = ["com.habit.report", "habit-report"];

const REPORT_LABEL: &str = "com.habit.report";
const REPORT_TIMER: &str = "habit-report";

/// Which scheduler the current build talks to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    Launchd,
    Systemd,
    /// Anything else: no scheduler, but `habit remind` still works.
    Other,
}

pub fn platform() -> Platform {
    if cfg!(target_os = "macos") {
        Platform::Launchd
    } else if cfg!(target_os = "linux") {
        Platform::Systemd
    } else {
        Platform::Other
    }
}

/// Habits that are due today, opted into reminders, and still open.
///
/// With `at` set, only habits whose reminder is that exact time are included,
/// which lets one scheduled unit per time share the same command.
pub fn pending(
    data: &Data,
    today: NaiveDate,
    week_start: Day,
    at: Option<NaiveTime>,
) -> Vec<&Habit> {
    let history = data.history();
    data.habits
        .iter()
        .filter(|habit| {
            habit.reminder.is_some()
                && at.is_none_or(|time| habit.reminder == Some(time))
                && !habit.archived
                && habit.is_active_on(today)
                && !habit.is_finished(today)
                && habit.schedule.is_due(today)
                && !data.log.is_done(today, &habit.id)
                && !data.cheats.is_excused(today, &habit.id)
                && match stats::weekly_progress(habit, &history, today, week_start) {
                    Some((done, target)) => done < u32::from(target),
                    None => true,
                }
        })
        .collect()
}

/// Distinct reminder times that need a scheduler unit today.
pub fn times(data: &Data, today: NaiveDate) -> Vec<NaiveTime> {
    let times: BTreeSet<NaiveTime> = data
        .habits
        .iter()
        .filter(|habit| !habit.archived && !habit.is_finished(today))
        .filter_map(|habit| habit.reminder)
        .collect();
    times.into_iter().collect()
}

/// Title and body for the notification, given the habits still open.
pub fn message(pending: &[&Habit]) -> (String, String) {
    let count = pending.len() as u32;
    let title = format!("{} left today", plural(count, "habit"));
    let names = pending
        .iter()
        .map(|habit| habit.name.as_str())
        .collect::<Vec<_>>()
        .join(" · ");
    (title, truncate(&names, 140))
}

/// Escape a string for an AppleScript double-quoted literal.
fn apple_quote(text: &str) -> String {
    let escaped = text.replace('\\', "\\\\").replace('"', "\\\"");
    format!("\"{escaped}\"")
}

/// Send a desktop notification. Returns whether one was actually asked for.
pub fn notify(title: &str, body: &str, dry_run: bool) -> Result<bool> {
    if dry_run {
        return Ok(false);
    }
    match platform() {
        Platform::Launchd => {
            let script = format!(
                "display notification {} with title {}",
                apple_quote(body),
                apple_quote(title)
            );
            run(Command::new("osascript").arg("-e").arg(script))?;
        }
        Platform::Systemd => {
            run(Command::new("notify-send")
                .arg("--app-name=habit")
                .arg(title)
                .arg(body))?;
        }
        Platform::Other => return Ok(false),
    }
    Ok(true)
}

fn run(command: &mut Command) -> Result<()> {
    let status = command
        .status()
        .with_context(|| format!("running {command:?}"))?;
    if !status.success() {
        bail!("{command:?} exited with {status}");
    }
    Ok(())
}

/// One file to write into a scheduler directory.
pub struct Unit {
    pub path: PathBuf,
    pub body: String,
}

/// A complete set of files for one job.
pub struct Units {
    pub units: Vec<Unit>,
}

impl Units {
    /// Write every unit file, creating parent directories as needed.
    pub fn write(&self) -> Result<()> {
        for unit in &self.units {
            if let Some(parent) = unit.path.parent() {
                fs::create_dir_all(parent)
                    .with_context(|| format!("creating {}", parent.display()))?;
            }
            fs::write(&unit.path, &unit.body)
                .with_context(|| format!("writing {}", unit.path.display()))?;
        }
        Ok(())
    }
}

fn launch_agents() -> Result<PathBuf> {
    Ok(home_dir()?.join("Library").join("LaunchAgents"))
}

fn systemd_user() -> Result<PathBuf> {
    let home = home_dir()?;
    let config = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .unwrap_or_else(|| home.join(".config"));
    Ok(config.join("systemd").join("user"))
}

fn unit_dir(platform: Platform) -> Result<PathBuf> {
    match platform {
        Platform::Launchd => launch_agents(),
        Platform::Systemd => systemd_user(),
        Platform::Other => bail!("no supported scheduler on this platform"),
    }
}

fn home_dir() -> Result<PathBuf> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .filter(|path| !path.as_os_str().is_empty())
        .context("cannot find HOME; scheduling needs a home directory")
}

fn reminder_label(time: NaiveTime) -> String {
    format!("com.habit.reminder.{}", time.format("%H%M"))
}

fn reminder_unit(time: NaiveTime) -> String {
    format!("habit-reminder-{}", time.format("%H%M"))
}

/// One launchd agent per time, so each run knows which habits to include.
pub fn reminder_units(
    platform: Platform,
    program: &Path,
    data: &Path,
    times: &[NaiveTime],
) -> Result<Units> {
    let dir = unit_dir(platform)?;
    let mut units = Vec::new();
    for time in times {
        let at = time.format("%H:%M").to_string();
        match platform {
            Platform::Launchd => units.push(Unit {
                path: dir.join(format!("{}.plist", reminder_label(*time))),
                body: plist(
                    &reminder_label(*time),
                    program,
                    data,
                    None,
                    *time,
                    &["remind", "--at", &at],
                ),
            }),
            Platform::Systemd => {
                let name = reminder_unit(*time);
                units.push(Unit {
                    path: dir.join(format!("{name}.service")),
                    body: systemd_service(
                        "Habit reminder",
                        program,
                        data,
                        &["remind", "--at", &at],
                    ),
                });
                units.push(Unit {
                    path: dir.join(format!("{name}.timer")),
                    body: systemd_timer("Daily habit reminder", None, *time),
                });
            }
            Platform::Other => bail!("no supported scheduler on this platform"),
        }
    }
    Ok(Units { units })
}

/// The launchd property list for one job.
pub fn plist(
    label: &str,
    program: &Path,
    data: &Path,
    day: Option<u8>,
    at: NaiveTime,
    command: &[&str],
) -> String {
    let mut args = vec![
        program.display().to_string(),
        "--data".to_string(),
        data.display().to_string(),
    ];
    args.extend(command.iter().map(|arg| (*arg).to_string()));
    let arguments = args
        .iter()
        .map(|arg| format!("        <string>{}</string>\n", xml_escape(arg)))
        .collect::<String>();
    let day = match day {
        Some(day) => format!("        <key>Day</key>\n        <integer>{day}</integer>\n"),
        None => String::new(),
    };
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>{label}</string>
    <key>ProgramArguments</key>
    <array>
{arguments}    </array>
    <key>StartCalendarInterval</key>
    <dict>
{day}        <key>Hour</key>
        <integer>{hour}</integer>
        <key>Minute</key>
        <integer>{minute}</integer>
    </dict>
    <key>RunAtLoad</key>
    <false/>
</dict>
</plist>
"#,
        label = xml_escape(label),
        arguments = arguments,
        day = day,
        hour = at.hour(),
        minute = at.minute(),
    )
}

fn xml_escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn systemd_quote(text: &str) -> String {
    let escaped = text.replace('\\', "\\\\").replace('"', "\\\"");
    format!("\"{escaped}\"")
}

/// The oneshot service that runs one `habit` command.
pub fn systemd_service(description: &str, program: &Path, data: &Path, command: &[&str]) -> String {
    let mut exec = format!(
        "{} --data {}",
        systemd_quote(&program.display().to_string()),
        systemd_quote(&data.display().to_string())
    );
    for arg in command {
        exec.push(' ');
        exec.push_str(&systemd_quote(arg));
    }
    format!("[Unit]\nDescription={description}\n\n[Service]\nType=oneshot\nExecStart={exec}\n")
}

/// The timer that fires a service on a calendar schedule.
pub fn systemd_timer(description: &str, day: Option<u8>, at: NaiveTime) -> String {
    let day = day.map_or_else(|| "*".to_string(), |day| format!("{day:02}"));
    format!(
        "[Unit]\nDescription={description}\n\n[Timer]\nOnCalendar=*-*-{day} {}:00\nPersistent=true\n\n[Install]\nWantedBy=timers.target\n",
        at.format("%H:%M")
    )
}

/// The monthly report job, running on the first day by default.
pub fn report_units(
    platform: Platform,
    program: &Path,
    data: &Path,
    day: u8,
    at: NaiveTime,
) -> Result<Units> {
    if !(1..=28).contains(&day) {
        bail!("report day must be between 1 and 28, so every month has it");
    }
    let dir = unit_dir(platform)?;
    let units = match platform {
        Platform::Launchd => vec![Unit {
            path: dir.join(format!("{REPORT_LABEL}.plist")),
            body: plist(
                REPORT_LABEL,
                program,
                data,
                Some(day),
                at,
                &["report", "--last-month", "--notify"],
            ),
        }],
        Platform::Systemd => vec![
            Unit {
                path: dir.join(format!("{REPORT_TIMER}.service")),
                body: systemd_service(
                    "Monthly habit report",
                    program,
                    data,
                    &["report", "--last-month", "--notify"],
                ),
            },
            Unit {
                path: dir.join(format!("{REPORT_TIMER}.timer")),
                body: systemd_timer("Monthly habit report", Some(day), at),
            },
        ],
        Platform::Other => bail!("no supported scheduler on this platform"),
    };
    Ok(Units { units })
}

/// Existing unit files for a set of prefixes.
pub fn installed(platform: Platform, prefixes: &[&str]) -> Result<Vec<PathBuf>> {
    existing(platform, prefixes)
}

/// Existing unit files for a set of prefixes.
fn existing(platform: Platform, prefixes: &[&str]) -> Result<Vec<PathBuf>> {
    let dir = match unit_dir(platform) {
        Ok(dir) => dir,
        Err(_) => return Ok(Vec::new()),
    };
    let Ok(entries) = fs::read_dir(&dir) else {
        return Ok(Vec::new());
    };
    let mut paths = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        let scheduler_file = matches!(
            path.extension().and_then(|extension| extension.to_str()),
            Some("plist" | "service" | "timer")
        );
        if scheduler_file && prefixes.iter().any(|prefix| name.starts_with(prefix)) {
            paths.push(path);
        }
    }
    paths.sort();
    Ok(paths)
}

/// Whether any job from this set is already installed.
pub fn any_installed(platform: Platform, prefixes: &[&str]) -> bool {
    existing(platform, prefixes)
        .map(|paths| paths.iter().any(|path| is_live_unit(path)))
        .unwrap_or(false)
}

fn is_live_unit(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|extension| extension.to_str()),
        Some("plist" | "timer")
    )
}

fn unit_name(path: &Path) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default()
        .trim_end_matches(".plist")
        .trim_end_matches(".service")
        .trim_end_matches(".timer")
        .to_string()
}

fn unload(platform: Platform, path: &Path) {
    match platform {
        Platform::Launchd => {
            let target = format!("gui/{}", user_id());
            let _ = Command::new("launchctl")
                .args(["bootout", &target])
                .arg(unit_name(path))
                .status();
        }
        Platform::Systemd => {
            if path.extension().and_then(|extension| extension.to_str()) == Some("timer") {
                let _ = Command::new("systemctl")
                    .args(["--user", "disable", "--now"])
                    .arg(format!("{}.timer", unit_name(path)))
                    .status();
            }
        }
        Platform::Other => {}
    }
}

fn load(platform: Platform, units: &Units) -> Result<()> {
    match platform {
        Platform::Launchd => {
            let target = format!("gui/{}", user_id());
            for unit in &units.units {
                // Replacing an existing agent: bootout first. A failure here
                // usually just means it was not loaded yet.
                let _ = Command::new("launchctl")
                    .args(["bootout", &target])
                    .arg(unit_name(&unit.path))
                    .status();
                run(Command::new("launchctl")
                    .args(["bootstrap", &target])
                    .arg(&unit.path))?;
            }
        }
        Platform::Systemd => {
            let _ = Command::new("systemctl")
                .args(["--user", "daemon-reload"])
                .status();
            for unit in &units.units {
                if unit
                    .path
                    .extension()
                    .and_then(|extension| extension.to_str())
                    != Some("timer")
                {
                    continue;
                }
                run(Command::new("systemctl").args([
                    "--user",
                    "enable",
                    "--now",
                    &format!("{}.timer", unit_name(&unit.path)),
                ]))?;
            }
        }
        Platform::Other => bail!("no supported scheduler on this platform"),
    }
    Ok(())
}

/// Write the wanted units and remove jobs that are no longer in the set.
pub fn sync(platform: Platform, units: &Units, prefixes: &[&str]) -> Result<Vec<PathBuf>> {
    let wanted: BTreeSet<PathBuf> = units.units.iter().map(|unit| unit.path.clone()).collect();
    let mut removed = Vec::new();
    for path in existing(platform, prefixes)? {
        if !wanted.contains(&path) {
            unload(platform, &path);
            fs::remove_file(&path).with_context(|| format!("removing {}", path.display()))?;
            removed.push(path);
        }
    }
    units.write()?;
    load(platform, units)?;
    Ok(removed)
}

/// Stop and delete every job in a set.
pub fn remove(platform: Platform, prefixes: &[&str]) -> Result<Vec<PathBuf>> {
    let mut removed = Vec::new();
    for path in existing(platform, prefixes)? {
        unload(platform, &path);
        fs::remove_file(&path).with_context(|| format!("removing {}", path.display()))?;
        removed.push(path);
    }
    if platform == Platform::Systemd {
        let _ = Command::new("systemctl")
            .args(["--user", "daemon-reload"])
            .status();
    }
    Ok(removed)
}

/// Whether each installed unit is written and reported as live.
pub fn status(platform: Platform, prefixes: &[&str]) -> Result<Vec<(PathBuf, bool, bool)>> {
    let mut rows = Vec::new();
    for path in existing(platform, prefixes)? {
        if !is_live_unit(&path) {
            continue;
        }
        let live = match platform {
            Platform::Launchd => Command::new("launchctl")
                .args(["print", &format!("gui/{}/{}", user_id(), unit_name(&path))])
                .output()
                .map(|output| output.status.success())
                .unwrap_or(false),
            Platform::Systemd => {
                let timer = format!("{}.timer", unit_name(&path));
                let active = Command::new("systemctl")
                    .args(["--user", "is-active", &timer])
                    .output()
                    .map(|output| output.status.success())
                    .unwrap_or(false);
                let enabled = Command::new("systemctl")
                    .args(["--user", "is-enabled", &timer])
                    .output()
                    .map(|output| output.status.success())
                    .unwrap_or(false);
                active && enabled
            }
            Platform::Other => false,
        };
        rows.push((path, true, live));
    }
    Ok(rows)
}

/// The numeric uid launchd wants in `gui/<uid>`.
///
/// `std` has no `getuid`, and the only consumer is a local command line, so
/// asking `id` keeps the dependency list short.
fn user_id() -> u32 {
    Command::new("id")
        .arg("-u")
        .output()
        .ok()
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .and_then(|text| text.trim().parse().ok())
        .unwrap_or(501)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Schedule;
    use chrono::NaiveDate;

    fn date(text: &str) -> NaiveDate {
        NaiveDate::parse_from_str(text, "%Y-%m-%d").unwrap()
    }

    fn at(text: &str) -> NaiveTime {
        NaiveTime::parse_from_str(text, "%H:%M").unwrap()
    }

    fn data_with_habits() -> (Data, NaiveDate) {
        let today = date("2026-11-10"); // a Tuesday
        let mut data = Data::default();
        data.add_habit("Gym", Schedule::Daily, date("2026-11-01"))
            .unwrap();
        data.add_habit(
            "Read",
            Schedule::Weekdays {
                days: [Day::Wed].into_iter().collect(),
            },
            date("2026-11-01"),
        )
        .unwrap();
        data.habits[0].reminder = Some(at("09:00"));
        data.habits[1].reminder = Some(at("09:00"));
        (data, today)
    }

    #[test]
    fn pending_skips_done_archived_finished_and_not_due_habits() {
        let (mut data, today) = data_with_habits();
        data.log.set(today, "gym", true);
        data.habits[1].archived = true;
        assert!(pending(&data, today, Day::Mon, None).is_empty());
    }

    #[test]
    fn pending_only_includes_habits_opted_into_reminders() {
        let (mut data, today) = data_with_habits();
        data.habits[0].reminder = None;
        let names: Vec<&str> = pending(&data, today, Day::Mon, None)
            .iter()
            .map(|habit| habit.name.as_str())
            .collect();
        assert!(names.is_empty());
    }

    #[test]
    fn pending_includes_a_habit_due_today_that_is_still_open() {
        let (data, today) = data_with_habits();
        let names: Vec<&str> = pending(&data, today, Day::Mon, None)
            .iter()
            .map(|habit| habit.name.as_str())
            .collect();
        assert_eq!(names, vec!["Gym"]);
    }

    #[test]
    fn pending_filters_to_one_reminder_time() {
        let (mut data, today) = data_with_habits();
        data.habits[1].reminder = Some(at("18:30"));
        data.habits[1].schedule = Schedule::Daily;
        let names: Vec<&str> = pending(&data, today, Day::Mon, Some(at("18:30")))
            .iter()
            .map(|habit| habit.name.as_str())
            .collect();
        assert_eq!(names, vec!["Read"]);
        let names: Vec<&str> = pending(&data, today, Day::Mon, Some(at("07:00")))
            .iter()
            .map(|habit| habit.name.as_str())
            .collect();
        assert!(names.is_empty());
    }

    #[test]
    fn pending_skips_a_quota_habit_whose_week_is_full() {
        let (mut data, today) = data_with_habits();
        data.habits[0].schedule = Schedule::TimesPerWeek { n: 1 };
        data.log.set(today, "gym", true);
        assert!(pending(&data, today, Day::Mon, None).is_empty());
    }

    #[test]
    fn pending_skips_a_future_start_and_an_excused_day() {
        let (mut data, today) = data_with_habits();
        data.habits[0].created = today + chrono::Duration::days(1);
        data.habits[1].archived = false;
        data.habits[1].schedule = Schedule::Daily;
        data.cheats.set(today, "read", true);
        assert!(pending(&data, today, Day::Mon, None).is_empty());
    }

    #[test]
    fn times_deduplicates_and_skips_closed_habits() {
        let (mut data, today) = data_with_habits();
        data.habits[0].reminder = Some(at("09:00"));
        data.habits[1].reminder = Some(at("09:00"));
        data.habits[0].archived = true;
        assert_eq!(times(&data, today), vec![at("09:00")]);

        data.habits[0].archived = false;
        data.habits[1].reminder = Some(at("18:30"));
        assert_eq!(times(&data, today), vec![at("09:00"), at("18:30")]);
    }

    #[test]
    fn message_pluralises_and_lists_names() {
        let (data, today) = data_with_habits();
        let open = pending(&data, today, Day::Mon, None);
        let (title, body) = message(&open);
        assert_eq!(title, "1 habit left today");
        assert_eq!(body, "Gym");

        let mut two = data.clone();
        two.habits[1].archived = false;
        two.habits[1].schedule = Schedule::Daily;
        let open = pending(&two, today, Day::Mon, None);
        let (title, body) = message(&open);
        assert_eq!(title, "2 habits left today");
        assert_eq!(body, "Gym · Read");
    }

    #[test]
    fn message_truncates_a_long_list() {
        let mut data = Data::default();
        let today = date("2026-11-10");
        for index in 0..12 {
            data.add_habit(
                &format!("Habit number {index}"),
                Schedule::Daily,
                date("2026-11-01"),
            )
            .unwrap();
            data.habits[index].reminder = Some(at("09:00"));
        }
        let open = pending(&data, today, Day::Mon, None);
        let (_, body) = message(&open);
        assert!(body.chars().count() <= 140);
        assert!(body.ends_with('…'));
    }

    #[test]
    fn plist_schedules_the_exact_time_and_data_file() {
        let body = plist(
            "com.habit.reminder.0905",
            Path::new("/usr/local/bin/habit"),
            Path::new("/home/me/data.json"),
            None,
            at("09:05"),
            &["remind", "--at", "09:05"],
        );
        assert!(body.contains("<string>com.habit.reminder.0905</string>"));
        assert!(body.contains("<string>/usr/local/bin/habit</string>"));
        assert!(body.contains("<string>--data</string>"));
        assert!(body.contains("<string>/home/me/data.json</string>"));
        assert!(body.contains("<string>remind</string>"));
        assert!(body.contains("<string>--at</string>"));
        assert!(body.contains("<integer>9</integer>"));
        assert!(body.contains("<integer>5</integer>"));
        assert!(!body.contains("<key>Day</key>"));
    }

    #[test]
    fn the_report_plist_runs_monthly_on_the_chosen_day() {
        let body = plist(
            REPORT_LABEL,
            Path::new("/usr/local/bin/habit"),
            Path::new("/home/me/data.json"),
            Some(1),
            at("08:00"),
            &["report", "--last-month", "--notify"],
        );
        assert!(body.contains("<key>Day</key>"));
        assert!(body.contains("<integer>1</integer>"));
        assert!(body.contains("<string>--last-month</string>"));
        assert!(body.contains("<string>--notify</string>"));
    }

    #[test]
    fn systemd_units_run_the_binary_and_fire_daily() {
        let service = systemd_service(
            "Habit reminder",
            Path::new("/usr/local/bin/habit"),
            Path::new("/home/me/data.json"),
            &["remind", "--at", "09:00"],
        );
        assert!(service.contains("Type=oneshot"));
        assert!(service.contains("ExecStart=\"/usr/local/bin/habit\" --data \"/home/me/data.json\" \"remind\" \"--at\" \"09:00\""));

        let timer = systemd_timer("Daily habit reminder", None, at("18:30"));
        assert!(timer.contains("OnCalendar=*-*-* 18:30:00"));
        assert!(timer.contains("WantedBy=timers.target"));

        let monthly = systemd_timer("Monthly habit report", Some(1), at("08:00"));
        assert!(monthly.contains("OnCalendar=*-*-01 08:00:00"));
    }

    #[test]
    fn apple_script_strings_are_escaped() {
        assert_eq!(apple_quote(r#"say "hi""#), r#""say \"hi\"""#);
        assert_eq!(apple_quote(r"one\two"), r#""one\\two""#);
    }

    #[test]
    fn xml_and_systemd_quotes_survive_odd_paths() {
        let plist = plist(
            "com.habit.reminder.0700",
            Path::new("/tmp/a&b/habit"),
            Path::new("/tmp/a b/data.json"),
            None,
            at("07:00"),
            &["remind"],
        );
        assert!(plist.contains("/tmp/a&amp;b/habit"));
        let service = systemd_service(
            "Habit reminder",
            Path::new("/tmp/a b/habit"),
            Path::new("/tmp/x.json"),
            &["remind"],
        );
        assert!(service.contains("ExecStart=\"/tmp/a b/habit\""));
    }

    #[test]
    fn dry_run_notify_sends_nothing() {
        assert!(!notify("title", "body", true).unwrap());
    }
}
