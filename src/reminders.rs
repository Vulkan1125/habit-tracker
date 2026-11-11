//! Native reminders: a one-shot summary plus the unit files that schedule it.
//!
//! There is no daemon here. `habit remind` is a single run that asks the
//! desktop for a notification, and `habit schedule install` hands a daily job
//! to launchd (macOS) or systemd (Linux). Unit contents are built as plain
//! strings so the tests can inspect them without touching the real system.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};
use chrono::{NaiveDate, NaiveTime, Timelike};

use crate::format::{plural, truncate};
use crate::model::{Data, Day, Habit};
use crate::stats;

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

const LABEL: &str = "com.habit.reminder";
const TIMER: &str = "habit-reminder";

/// Habits that are due today and still open.
///
/// Quota habits only nudge while their week is short; an excused day or a
/// check-in already in the log never makes the list.
pub fn pending(data: &Data, today: NaiveDate, week_start: Day) -> Vec<&Habit> {
    let history = data.history();
    data.habits
        .iter()
        .filter(|habit| {
            !habit.archived
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

/// Path of the launchd agent, or the systemd unit pair.
pub fn unit_paths(platform: Platform) -> Result<Vec<PathBuf>> {
    let home = home_dir()?;
    Ok(match platform {
        Platform::Launchd => vec![
            home.join("Library")
                .join("LaunchAgents")
                .join(format!("{LABEL}.plist")),
        ],
        Platform::Systemd => {
            let config = std::env::var_os("XDG_CONFIG_HOME")
                .map(PathBuf::from)
                .filter(|path| path.is_absolute())
                .unwrap_or_else(|| home.join(".config"));
            let dir = config.join("systemd").join("user");
            vec![
                dir.join(format!("{TIMER}.service")),
                dir.join(format!("{TIMER}.timer")),
            ]
        }
        Platform::Other => Vec::new(),
    })
}

fn home_dir() -> Result<PathBuf> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .filter(|path| !path.as_os_str().is_empty())
        .context("cannot find HOME; scheduling needs a home directory")
}

/// The launchd property list for a daily run at `at`.
pub fn plist(program: &Path, data: &Path, at: NaiveTime) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>{LABEL}</string>
    <key>ProgramArguments</key>
    <array>
        <string>{program}</string>
        <string>--data</string>
        <string>{data}</string>
        <string>remind</string>
    </array>
    <key>StartCalendarInterval</key>
    <dict>
        <key>Hour</key>
        <integer>{hour}</integer>
        <key>Minute</key>
        <integer>{minute}</integer>
    </dict>
    <key>RunAtLoad</key>
    <false/>
</dict>
</plist>
"#,
        program = xml_escape(&program.display().to_string()),
        data = xml_escape(&data.display().to_string()),
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

/// The oneshot service that runs `habit remind`.
pub fn systemd_service(program: &Path, data: &Path) -> String {
    format!(
        "[Unit]\nDescription=Habit reminder\n\n[Service]\nType=oneshot\nExecStart={} --data {} remind\n",
        systemd_quote(&program.display().to_string()),
        systemd_quote(&data.display().to_string()),
    )
}

/// The timer that fires the service once a day.
pub fn systemd_timer(at: NaiveTime) -> String {
    format!(
        "[Unit]\nDescription=Daily habit reminder\n\n[Timer]\nOnCalendar=*-*-* {}:00\nPersistent=true\n\n[Install]\nWantedBy=timers.target\n",
        at.format("%H:%M")
    )
}

/// What `schedule install` would write, without writing it.
pub struct Units {
    pub files: Vec<(PathBuf, String)>,
}

pub fn units(platform: Platform, program: &Path, data: &Path, at: NaiveTime) -> Result<Units> {
    let paths = unit_paths(platform)?;
    let files = match platform {
        Platform::Launchd => vec![(paths[0].clone(), plist(program, data, at))],
        Platform::Systemd => vec![
            (paths[0].clone(), systemd_service(program, data)),
            (paths[1].clone(), systemd_timer(at)),
        ],
        Platform::Other => {
            bail!("no supported scheduler here; run `habit remind` from cron or Task Scheduler")
        }
    };
    Ok(Units { files })
}

impl Units {
    /// Write every unit file, creating parent directories as needed.
    pub fn write(&self) -> Result<()> {
        for (path, body) in &self.files {
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)
                    .with_context(|| format!("creating {}", parent.display()))?;
            }
            fs::write(path, body).with_context(|| format!("writing {}", path.display()))?;
        }
        Ok(())
    }
}

pub fn install(platform: Platform, program: &Path, data: &Path, at: NaiveTime) -> Result<Units> {
    let units = units(platform, program, data, at)?;
    units.write()?;
    match platform {
        Platform::Launchd => {
            let path = &units.files[0].0;
            reload_launchd(path)?;
        }
        Platform::Systemd => {
            let _ = Command::new("systemctl")
                .args(["--user", "daemon-reload"])
                .status();
            run(Command::new("systemctl").args([
                "--user",
                "enable",
                "--now",
                &format!("{TIMER}.timer"),
            ]))?;
        }
        Platform::Other => {}
    }
    Ok(units)
}

fn reload_launchd(path: &Path) -> Result<()> {
    let target = format!("gui/{}", user_id());
    // Replacing an installed agent: bootout first, then bootstrap. A failure
    // here usually means it was not loaded yet, so it is ignored.
    let _ = Command::new("launchctl")
        .args(["bootout", &target])
        .arg(path)
        .status();
    run(Command::new("launchctl")
        .args(["bootstrap", &target])
        .arg(path))?;
    Ok(())
}

pub fn remove(platform: Platform) -> Result<Vec<PathBuf>> {
    let paths = unit_paths(platform)?;
    match platform {
        Platform::Launchd => {
            if let Some(path) = paths.first()
                && path.exists()
            {
                let target = format!("gui/{}", user_id());
                let _ = Command::new("launchctl")
                    .args(["bootout", &target])
                    .arg(path)
                    .status();
            }
        }
        Platform::Systemd => {
            let _ = Command::new("systemctl")
                .args(["--user", "disable", "--now", &format!("{TIMER}.timer")])
                .status();
        }
        Platform::Other => {}
    }
    let mut removed = Vec::new();
    for path in paths {
        if path.exists() {
            fs::remove_file(&path).with_context(|| format!("removing {}", path.display()))?;
            removed.push(path);
        }
    }
    if platform == Platform::Systemd {
        let _ = Command::new("systemctl")
            .args(["--user", "daemon-reload"])
            .status();
    }
    Ok(removed)
}

/// Whether the unit files exist and the scheduler reports them as live.
pub fn status(platform: Platform) -> Result<Vec<(PathBuf, bool, bool)>> {
    let mut rows = Vec::new();
    for path in unit_paths(platform)? {
        let file = path.exists();
        let loaded = match platform {
            Platform::Launchd => Command::new("launchctl")
                .args(["print", &format!("gui/{}/{LABEL}", user_id())])
                .output()
                .map(|output| output.status.success())
                .unwrap_or(false),
            Platform::Systemd => {
                let active = Command::new("systemctl")
                    .args(["--user", "is-active", &format!("{TIMER}.timer")])
                    .output()
                    .map(|output| output.status.success())
                    .unwrap_or(false);
                let enabled = Command::new("systemctl")
                    .args(["--user", "is-enabled", &format!("{TIMER}.timer")])
                    .output()
                    .map(|output| output.status.success())
                    .unwrap_or(false);
                active && enabled
            }
            Platform::Other => false,
        };
        rows.push((path, file, loaded));
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
        (data, today)
    }

    #[test]
    fn pending_skips_done_archived_finished_and_not_due_habits() {
        let (mut data, today) = data_with_habits();
        data.log.set(today, "gym", true);
        data.habits[1].archived = true;
        assert!(pending(&data, today, Day::Mon).is_empty());
    }

    #[test]
    fn pending_includes_a_habit_due_today_that_is_still_open() {
        let (data, today) = data_with_habits();
        let names: Vec<&str> = pending(&data, today, Day::Mon)
            .iter()
            .map(|habit| habit.name.as_str())
            .collect();
        assert_eq!(names, vec!["Gym"]);
    }

    #[test]
    fn pending_skips_a_quota_habit_whose_week_is_full() {
        let (mut data, today) = data_with_habits();
        data.habits[0].schedule = Schedule::TimesPerWeek { n: 1 };
        data.log.set(today, "gym", true);
        let names: Vec<&str> = pending(&data, today, Day::Mon)
            .iter()
            .map(|habit| habit.name.as_str())
            .collect();
        assert!(names.is_empty());
    }

    #[test]
    fn pending_skips_a_future_start_and_an_excused_day() {
        let (mut data, today) = data_with_habits();
        data.habits[0].created = today + chrono::Duration::days(1);
        data.habits[1].archived = false;
        data.habits[1].schedule = Schedule::Daily;
        data.cheats.set(today, "read", true);
        assert!(pending(&data, today, Day::Mon).is_empty());
    }

    #[test]
    fn message_pluralises_and_lists_names() {
        let (data, today) = data_with_habits();
        let open = pending(&data, today, Day::Mon);
        let (title, body) = message(&open);
        assert_eq!(title, "1 habit left today");
        assert_eq!(body, "Gym");

        let mut two = data.clone();
        two.habits[1].archived = false;
        two.habits[1].schedule = Schedule::Daily;
        let open = pending(&two, today, Day::Mon);
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
        }
        let open = pending(&data, today, Day::Mon);
        let (_, body) = message(&open);
        assert!(body.chars().count() <= 140);
        assert!(body.ends_with('…'));
    }

    #[test]
    fn plist_schedules_the_exact_time_and_data_file() {
        let body = plist(
            Path::new("/usr/local/bin/habit"),
            Path::new("/home/me/data.json"),
            at("09:05"),
        );
        assert!(body.contains("<string>com.habit.reminder</string>"));
        assert!(body.contains("<string>/usr/local/bin/habit</string>"));
        assert!(body.contains("<string>--data</string>"));
        assert!(body.contains("<string>/home/me/data.json</string>"));
        assert!(body.contains("<string>remind</string>"));
        assert!(body.contains("<integer>9</integer>"));
        assert!(body.contains("<integer>5</integer>"));
    }

    #[test]
    fn systemd_units_run_the_binary_and_fire_daily() {
        let service = systemd_service(
            Path::new("/usr/local/bin/habit"),
            Path::new("/home/me/data.json"),
        );
        assert!(service.contains("Type=oneshot"));
        assert!(
            service.contains(
                "ExecStart=\"/usr/local/bin/habit\" --data \"/home/me/data.json\" remind"
            )
        );

        let timer = systemd_timer(at("18:30"));
        assert!(timer.contains("OnCalendar=*-*-* 18:30:00"));
        assert!(timer.contains("WantedBy=timers.target"));
    }

    #[test]
    fn apple_script_strings_are_escaped() {
        assert_eq!(apple_quote(r#"say "hi""#), r#""say \"hi\"""#);
        assert_eq!(apple_quote(r"one\two"), r#""one\\two""#);
    }

    #[test]
    fn xml_and_systemd_quotes_survive_odd_paths() {
        let plist = plist(
            Path::new("/tmp/a&b/habit"),
            Path::new("/tmp/a b/data.json"),
            at("07:00"),
        );
        assert!(plist.contains("/tmp/a&amp;b/habit"));
        let service = systemd_service(Path::new("/tmp/a b/habit"), Path::new("/tmp/x.json"));
        assert!(service.contains("ExecStart=\"/tmp/a b/habit\""));
    }

    #[test]
    fn dry_run_notify_sends_nothing() {
        assert!(!notify("title", "body", true).unwrap());
    }
}
