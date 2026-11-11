//! Optional features, installed on demand.
//!
//! The core app is deliberately small: check-ins, streaks, tags and the TUI.
//! Everything that is not needed to track a habit is a companion binary —
//! `habit-notify` for reminder times and desktop notifications, `habit-report`
//! for the monthly HTML pages — and stays dormant until the user installs it
//! with `habit modules install <name>`.
//!
//! A module is *installed* when its binary is copied into `<data dir>/modules`.
//! The source is the copy that shipped next to `habit` (release tarball,
//! Homebrew, `cargo install`) or a directory named by `HABIT_MODULE_SOURCE`,
//! so installing never touches the network.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use crate::model::Data;
use crate::reminders;

/// Names accepted on the command line, in the order they are listed.
pub const NAMES: [&str; 2] = ["notify", "report"];

/// One module as the UI and the help text describe it.
pub struct Module {
    pub name: &'static str,
    pub label: &'static str,
    pub description: &'static str,
}

pub const ALL: [Module; 2] = [
    Module {
        name: "notify",
        label: "Notifications",
        description: "reminder times and desktop notifications",
    },
    Module {
        name: "report",
        label: "Reports",
        description: "monthly HTML pages",
    },
];

/// Which modules are installed for this data file.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Modules {
    pub notify: bool,
    pub report: bool,
}

impl Modules {
    pub fn detect(data_path: &Path) -> Self {
        Self {
            notify: path_for(data_path, "notify").is_file(),
            report: path_for(data_path, "report").is_file(),
        }
    }

    pub fn installed(self, name: &str) -> bool {
        match name {
            "notify" => self.notify,
            "report" => self.report,
            _ => false,
        }
    }
}

/// The directory a data file keeps its modules in, beside `reports/`.
pub fn dir(data_path: &Path) -> PathBuf {
    data_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("modules")
}

/// Where an installed module lives.
pub fn path_for(data_path: &Path, name: &str) -> PathBuf {
    dir(data_path).join(binary_name(name))
}

/// A marker so an explicit `modules remove` is not undone by migration.
fn marker(data_path: &Path, name: &str) -> PathBuf {
    dir(data_path).join(format!(".no-{name}"))
}

fn binary_name(name: &str) -> String {
    if cfg!(windows) {
        format!("habit-{name}.exe")
    } else {
        format!("habit-{name}")
    }
}

/// Find the shipped copy of a module: an explicit source directory, the
/// directory `habit` itself lives in, then `PATH`.
pub fn source(name: &str) -> Option<PathBuf> {
    let file = binary_name(name);
    if let Some(dir) = env::var_os("HABIT_MODULE_SOURCE").filter(|value| !value.is_empty()) {
        let candidate = PathBuf::from(dir).join(&file);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    if let Ok(current) = env::current_exe()
        && let Some(dir) = current.parent()
    {
        let candidate = dir.join(&file);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    env::var_os("PATH").and_then(|paths| {
        env::split_paths(&paths)
            .map(|dir| dir.join(&file))
            .find(|candidate| candidate.is_file())
    })
}

/// Copy a module next to the data file. Installing twice is harmless.
pub fn install(data_path: &Path, name: &str) -> Result<PathBuf> {
    let source = source(name).with_context(|| {
        format!(
            "the {name} module was not installed with habit; reinstall habit (install.sh, \
             Homebrew or `cargo install` all ship it) or point HABIT_MODULE_SOURCE at its directory"
        )
    })?;
    install_from(data_path, name, &source)
}

/// Install a module from an explicit file; used by [`install`] and the tests.
pub fn install_from(data_path: &Path, name: &str, source: &Path) -> Result<PathBuf> {
    if !NAMES.contains(&name) {
        bail!(
            "unknown module \"{name}\" (choose from {})",
            NAMES.join(", ")
        );
    }
    let directory = dir(data_path);
    fs::create_dir_all(&directory).with_context(|| format!("creating {}", directory.display()))?;
    let target = path_for(data_path, name);
    fs::copy(source, &target)
        .with_context(|| format!("copying {} to {}", source.display(), target.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&target, fs::Permissions::from_mode(0o755))
            .with_context(|| format!("making {} executable", target.display()))?;
    }
    // Installing is an explicit yes, so a previous removal no longer applies.
    let _ = fs::remove_file(marker(data_path, name));
    Ok(target)
}

/// Delete an installed module. Returns whether there was one.
pub fn remove(data_path: &Path, name: &str) -> Result<bool> {
    let target = path_for(data_path, name);
    let removed = match fs::remove_file(&target) {
        Ok(()) => true,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
        Err(error) => {
            return Err(error).with_context(|| format!("removing {}", target.display()));
        }
    };
    // The module was removed on purpose: migration must not resurrect it just
    // because a habit still carries a reminder.
    let directory = dir(data_path);
    fs::create_dir_all(&directory).with_context(|| format!("creating {}", directory.display()))?;
    fs::write(marker(data_path, name), b"removed")
        .with_context(|| format!("writing {}", marker(data_path, name).display()))?;
    Ok(removed)
}

/// Remove a module and the scheduler jobs that belong to it.
///
/// The habit data is untouched: reminder times and report settings survive, so
/// installing the module again restores the feature.
pub fn remove_with_jobs(data_path: &Path, name: &str) -> Result<(bool, usize)> {
    let removed = remove(data_path, name)?;
    let prefixes: &[&str] = match name {
        "notify" => &reminders::REMINDER_PREFIXES,
        "report" => &reminders::REPORT_PREFIXES,
        _ => &[],
    };
    let jobs = reminders::remove(reminders::platform(), prefixes)?.len();
    Ok((removed, jobs))
}

/// Whether migration should install a module: the data relies on it, it is not
/// there, and the user has not explicitly removed it.
fn wants_install(installed: bool, data_path: &Path, name: &str, needed: bool) -> bool {
    needed && !installed && !marker(data_path, name).exists()
}

/// Install the modules that existing data already relies on.
///
/// Upgrading must not silently switch reminders or scheduled reports off, so a
/// data file that already uses a feature gets its module installed the first
/// time the new build reads it.
pub fn migrate(data_path: &Path, data: &Data) -> Vec<String> {
    let installed = Modules::detect(data_path);
    let mut notes = Vec::new();
    let needs_notify = data.habits.iter().any(|habit| habit.reminder.is_some());
    if wants_install(installed.notify, data_path, "notify", needs_notify)
        && let Ok(path) = install(data_path, "notify")
    {
        notes.push(format!(
            "installed the notifications module for your existing reminders ({})",
            path.display()
        ));
    }
    let reports = data_path
        .parent()
        .map(|dir| dir.join("reports"))
        .is_some_and(|dir| dir.is_dir());
    if wants_install(installed.report, data_path, "report", reports)
        && let Ok(path) = install(data_path, "report")
    {
        notes.push(format!(
            "installed the reports module for your existing reports ({})",
            path.display()
        ));
    }
    notes
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Data, Schedule};
    use std::io::Write as _;

    fn fake_module(dir: &Path) -> PathBuf {
        let path = dir.join("habit-notify");
        let mut file = fs::File::create(&path).unwrap();
        file.write_all(b"#!/bin/sh\necho module\n").unwrap();
        path
    }

    fn data_path(dir: &Path) -> PathBuf {
        dir.join("data.json")
    }

    fn dated(text: &str) -> chrono::NaiveDate {
        chrono::NaiveDate::parse_from_str(text, "%Y-%m-%d").unwrap()
    }

    #[test]
    fn installing_copies_the_module_and_marks_it_installed() {
        let dir = tempfile::tempdir().unwrap();
        let data = data_path(dir.path());
        let source = fake_module(dir.path());

        assert!(!Modules::detect(&data).notify);
        let installed = install_from(&data, "notify", &source).unwrap();
        assert_eq!(installed, dir.path().join("modules").join("habit-notify"));
        assert!(installed.is_file());
        assert!(Modules::detect(&data).notify);
        assert!(!Modules::detect(&data).report);
    }

    #[test]
    fn removing_a_module_leaves_the_data_file_alone() {
        let dir = tempfile::tempdir().unwrap();
        let data = data_path(dir.path());
        fs::write(&data, "{}").unwrap();
        let source = fake_module(dir.path());
        install_from(&data, "notify", &source).unwrap();

        assert!(remove(&data, "notify").unwrap());
        assert!(!remove(&data, "notify").unwrap());
        assert!(data.is_file());
        assert!(!Modules::detect(&data).notify);
    }

    #[test]
    fn a_removal_is_remembered_until_the_next_install() {
        let dir = tempfile::tempdir().unwrap();
        let data = data_path(dir.path());
        let source = fake_module(dir.path());
        let mut habits = Data::default();
        habits
            .add_habit("Gym", Schedule::Daily, dated("2026-11-01"))
            .unwrap();
        habits.habits[0].reminder = Some(chrono::NaiveTime::from_hms_opt(9, 0, 0).unwrap());

        install_from(&data, "notify", &source).unwrap();
        assert_eq!(migrate(&data, &habits).len(), 0, "already installed");

        remove(&data, "notify").unwrap();
        assert_eq!(
            migrate(&data, &habits).len(),
            0,
            "an explicit removal must not be undone"
        );

        install_from(&data, "notify", &source).unwrap();
        assert!(!marker(&data, "notify").exists());
    }

    #[test]
    fn migration_skips_modules_the_user_removed() {
        let dir = tempfile::tempdir().unwrap();
        let data = data_path(dir.path());

        assert!(wants_install(false, &data, "report", true));
        assert!(!wants_install(true, &data, "report", true));
        assert!(!wants_install(false, &data, "report", false));

        remove(&data, "report").unwrap();
        assert!(
            !wants_install(false, &data, "report", true),
            "an explicit removal must survive the next run"
        );
    }

    #[test]
    fn unknown_modules_are_refused() {
        let dir = tempfile::tempdir().unwrap();
        assert!(install_from(&data_path(dir.path()), "telepathy", Path::new("x")).is_err());
    }

    #[test]
    fn module_metadata_covers_the_installable_names() {
        assert_eq!(
            ALL.iter().map(|module| module.name).collect::<Vec<_>>(),
            NAMES
        );
    }

    #[test]
    fn existing_reminders_ask_for_the_notify_module() {
        let mut data = Data::default();
        data.add_habit("Gym", Schedule::Daily, dated("2026-11-01"))
            .unwrap();
        assert!(migrate(Path::new("/nonexistent/data.json"), &data).is_empty());

        data.habits[0].reminder = Some(chrono::NaiveTime::from_hms_opt(9, 0, 0).unwrap());
        // No shipped module to copy in a unit test, so migration stays quiet
        // instead of failing the command.
        assert!(migrate(Path::new("/nonexistent/data.json"), &data).is_empty());
    }
}
