//! What `habit uninstall` removes, and how it removes it.
//!
//! Uninstalling is the one destructive command in the program, so the paths are
//! collected into a [`Plan`] first: the CLI can print it, ask once, and only
//! then delete. Everything here is local — data, backups, optional modules,
//! generated reports, config, scheduled jobs and the binaries themselves.
//!
//! The binaries are left alone when this copy belongs to Cargo's build
//! directory or Homebrew's Cellar, because deleting those breaks the tool that
//! owns them. The plan says which command to run instead.

use std::fs;
use std::path::{Path, PathBuf};

use crate::modules;
use crate::reminders;
use crate::report;

/// How an entry has to be removed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    File,
    /// Removed only when empty, so unrelated files are never touched.
    EmptyDir,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub path: PathBuf,
    pub kind: Kind,
    /// Short description for the confirmation list.
    pub label: &'static str,
}

/// Everything an uninstall would remove.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Plan {
    /// Scheduler unit files, unloaded through [`reminders::remove`].
    pub jobs: Vec<PathBuf>,
    pub entries: Vec<Entry>,
    /// The running binary and its two companions, if they can be removed.
    pub binaries: Vec<PathBuf>,
    /// Set when the binaries belong to a package manager or a build directory.
    pub package_manager: Option<&'static str>,
}

impl Plan {
    pub fn is_empty(&self) -> bool {
        self.jobs.is_empty()
            && self.entries.is_empty()
            && self.binaries.is_empty()
            && self.package_manager.is_none()
    }

    /// Paths in the order they will be removed, for display.
    pub fn paths(&self) -> Vec<(&Path, &'static str)> {
        self.jobs
            .iter()
            .map(|path| (path.as_path(), "scheduled job"))
            .chain(
                self.entries
                    .iter()
                    .map(|entry| (entry.path.as_path(), entry.label)),
            )
            .chain(self.binaries.iter().map(|path| (path.as_path(), "binary")))
            .collect()
    }
}

/// Collect everything that exists for this data file and this copy of `habit`.
///
/// `jobs` is passed in so a dry run can show the scheduler units without this
/// module reaching into launchd or systemd itself.
pub fn plan(
    data_path: &Path,
    config_path: Option<&Path>,
    current_exe: &Path,
    jobs: Vec<PathBuf>,
) -> Plan {
    let mut plan = Plan {
        jobs,
        ..Plan::default()
    };

    add(
        &mut plan.entries,
        data_path.to_path_buf(),
        Kind::File,
        "habit data",
    );
    for (extension, label) in [
        ("json.bak", "data backup"),
        ("json.tmp", "unfinished data write"),
    ] {
        add(
            &mut plan.entries,
            data_path.with_extension(extension),
            Kind::File,
            label,
        );
    }
    // Reports and modules live in directories beside the data file. Remove only
    // the files habit wrote, then the folder itself if nothing else is in it.
    let reports = report::default_path(data_path);
    if reports.is_dir() {
        if let Ok(entries) = fs::read_dir(&reports) {
            for entry in entries.flatten() {
                let path = entry.path();
                let name = path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("");
                if name.starts_with("habit-") && name.ends_with(".html") {
                    add(&mut plan.entries, path, Kind::File, "generated report");
                }
            }
        }
        add(
            &mut plan.entries,
            reports,
            Kind::EmptyDir,
            "reports folder (only if empty)",
        );
    }
    let modules_dir = modules::dir(data_path);
    if modules_dir.is_dir() {
        for module in modules::NAMES {
            add(
                &mut plan.entries,
                modules::path_for(data_path, module),
                Kind::File,
                "installed module",
            );
            add(
                &mut plan.entries,
                modules_dir.join(format!(".no-{module}")),
                Kind::File,
                "module marker",
            );
        }
        add(
            &mut plan.entries,
            modules_dir,
            Kind::EmptyDir,
            "modules folder (only if empty)",
        );
    }
    if let Some(config_path) = config_path {
        add(
            &mut plan.entries,
            config_path.to_path_buf(),
            Kind::File,
            "config",
        );
        if let Some(parent) = config_path.parent() {
            add(
                &mut plan.entries,
                parent.to_path_buf(),
                Kind::EmptyDir,
                "config folder (only if empty)",
            );
        }
    }
    if let Some(parent) = data_path.parent() {
        add(
            &mut plan.entries,
            parent.to_path_buf(),
            Kind::EmptyDir,
            "data folder (only if empty)",
        );
    }

    let (binaries, package_manager) = binaries(current_exe);
    plan.binaries = binaries;
    plan.package_manager = package_manager;
    plan
}

fn add(entries: &mut Vec<Entry>, path: PathBuf, kind: Kind, label: &'static str) {
    if path.exists() && !entries.iter().any(|entry| entry.path == path) {
        entries.push(Entry { path, kind, label });
    }
}

fn exe_name(name: &str) -> String {
    if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_string()
    }
}

/// The binaries beside the running one, or the reason they must be left alone.
fn binaries(current_exe: &Path) -> (Vec<PathBuf>, Option<&'static str>) {
    let Some(dir) = current_exe.parent() else {
        return (Vec::new(), None);
    };
    let text = current_exe.to_string_lossy();

    if text.contains("/target/debug/") || text.contains("/target/release/") {
        return (
            Vec::new(),
            Some(
                "this copy is a Cargo build; run `cargo uninstall habit` to remove installed copies",
            ),
        );
    }
    if text.contains("/Cellar/") || text.contains("/homebrew/") {
        return (
            Vec::new(),
            Some("Homebrew manages this copy; run `brew uninstall habit` to remove it"),
        );
    }

    let mut paths = vec![dir.join(exe_name("habit"))];
    for module in modules::NAMES {
        paths.push(dir.join(exe_name(&format!("habit-{module}"))));
    }
    paths.retain(|path| path.is_file());
    (paths, None)
}

/// What [`remove`] managed to delete.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Outcome {
    pub removed: usize,
    pub failures: Vec<String>,
}

/// Delete a plan. Scheduled jobs are unloaded first so launchd/systemd forget
/// them; failures are collected instead of stopping halfway.
pub fn remove(plan: &Plan) -> Outcome {
    let mut outcome = Outcome::default();

    if !plan.jobs.is_empty() {
        for prefixes in [&reminders::REMINDER_PREFIXES, &reminders::REPORT_PREFIXES] {
            match reminders::remove(reminders::platform(), prefixes) {
                Ok(paths) => outcome.removed += paths.len(),
                Err(error) => outcome.failures.push(format!("scheduled jobs: {error}")),
            }
        }
    }

    for entry in &plan.entries {
        let result = match entry.kind {
            Kind::File => remove_file(&entry.path),
            Kind::EmptyDir => remove_empty_dir(&entry.path),
        };
        match result {
            Ok(true) => outcome.removed += 1,
            Ok(false) => {}
            Err(error) => outcome
                .failures
                .push(format!("{}: {error}", entry.path.display())),
        }
    }

    for path in &plan.binaries {
        match remove_file(path) {
            Ok(true) => outcome.removed += 1,
            Ok(false) => {}
            Err(error) => outcome
                .failures
                .push(format!("{}: {error}", path.display())),
        }
    }

    outcome
}

fn remove_file(path: &Path) -> std::io::Result<bool> {
    match fs::remove_file(path) {
        Ok(()) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error),
    }
}

/// Remove an empty directory, but never complain about one that still has
/// unrelated files in it.
fn remove_empty_dir(path: &Path) -> std::io::Result<bool> {
    match fs::remove_dir(path) {
        Ok(()) => Ok(true),
        Err(error)
            if matches!(
                error.kind(),
                std::io::ErrorKind::NotFound
                    | std::io::ErrorKind::DirectoryNotEmpty
                    | std::io::ErrorKind::NotADirectory
            ) =>
        {
            Ok(false)
        }
        Err(error) => Err(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn date_dir() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }

    #[test]
    fn the_plan_lists_data_backups_modules_and_reports() {
        let temp = date_dir();
        let root = temp.path().join("habit");
        fs::create_dir_all(root.join("modules")).unwrap();
        fs::create_dir_all(root.join("reports")).unwrap();
        let data = root.join("data.json");
        fs::write(&data, "{}").unwrap();
        fs::write(root.join("data.json.bak"), "{}").unwrap();
        fs::write(root.join("data.json.tmp"), "{}").unwrap();
        let config = temp.path().join("config").join("habit").join("config.toml");
        fs::create_dir_all(config.parent().unwrap()).unwrap();
        fs::write(&config, "theme = \"mono\"\n").unwrap();

        let plan = plan(
            &data,
            Some(&config),
            &temp.path().join("bin").join("habit"),
            Vec::new(),
        );
        let paths: Vec<&Path> = plan.paths().iter().map(|(path, _)| *path).collect();
        for expected in [
            data.as_path(),
            &root.join("data.json.bak"),
            &root.join("data.json.tmp"),
            &root.join("reports"),
            &root.join("modules"),
            config.as_path(),
            config.parent().unwrap(),
            root.as_path(),
        ] {
            assert!(paths.contains(&expected), "missing {}", expected.display());
        }
    }

    #[test]
    fn removing_a_plan_deletes_owned_paths_and_empty_folders() {
        let temp = date_dir();
        let root = temp.path().join("habit");
        fs::create_dir_all(root.join("modules")).unwrap();
        fs::create_dir_all(root.join("reports")).unwrap();
        fs::write(root.join("modules").join("habit-notify"), b"bin").unwrap();
        fs::write(root.join("reports").join("habit-2026-11.html"), b"page").unwrap();
        let data = root.join("data.json");
        fs::write(&data, "{}").unwrap();
        fs::write(root.join("data.json.bak"), "{}").unwrap();
        let config = temp.path().join("config").join("habit").join("config.toml");
        fs::create_dir_all(config.parent().unwrap()).unwrap();
        fs::write(&config, "theme = \"mono\"\n").unwrap();

        let bins = temp.path().join("bin");
        fs::create_dir_all(&bins).unwrap();
        for name in ["habit", "habit-notify", "habit-report"] {
            fs::write(bins.join(name), b"bin").unwrap();
        }

        let plan = plan(&data, Some(&config), &bins.join("habit"), Vec::new());
        let outcome = remove(&plan);
        assert!(outcome.failures.is_empty(), "{:?}", outcome.failures);
        assert!(!data.exists());
        assert!(!root.join("reports").exists());
        assert!(!root.join("modules").exists());
        assert!(!root.exists(), "the empty data folder goes too");
        assert!(!config.parent().unwrap().exists());
        assert!(!bins.join("habit").exists());
        assert!(!bins.join("habit-notify").exists());
        assert!(!bins.join("habit-report").exists());
    }

    #[test]
    fn a_config_folder_with_other_files_is_left_alone() {
        let temp = date_dir();
        let root = temp.path().join("habit");
        fs::create_dir_all(&root).unwrap();
        let data = root.join("data.json");
        fs::write(&data, "{}").unwrap();
        let config = temp.path().join("config").join("habit").join("config.toml");
        fs::create_dir_all(config.parent().unwrap()).unwrap();
        fs::write(&config, "theme = \"mono\"\n").unwrap();
        fs::write(config.parent().unwrap().join("notes.txt"), "keep me").unwrap();

        let plan = plan(
            &data,
            Some(&config),
            &temp.path().join("bin").join("habit"),
            Vec::new(),
        );
        let outcome = remove(&plan);
        assert!(outcome.failures.is_empty(), "{:?}", outcome.failures);
        assert!(!config.exists());
        assert!(config.parent().unwrap().exists());
        assert!(config.parent().unwrap().join("notes.txt").exists());
    }

    #[test]
    fn unrelated_files_beside_reports_and_modules_are_left_alone() {
        let temp = date_dir();
        let root = temp.path().join("habit");
        fs::create_dir_all(root.join("modules")).unwrap();
        fs::create_dir_all(root.join("reports")).unwrap();
        fs::write(root.join("modules").join("habit-notify"), b"bin").unwrap();
        fs::write(root.join("modules").join("README.txt"), b"mine").unwrap();
        fs::write(root.join("reports").join("habit-2026-11.html"), b"page").unwrap();
        fs::write(root.join("reports").join("taxes.pdf"), b"not ours").unwrap();
        let data = root.join("data.json");
        fs::write(&data, "{}").unwrap();

        let plan = plan(
            &data,
            None,
            &temp.path().join("bin").join("habit"),
            Vec::new(),
        );
        let outcome = remove(&plan);
        assert!(outcome.failures.is_empty(), "{:?}", outcome.failures);
        assert!(!root.join("modules").join("habit-notify").exists());
        assert!(!root.join("reports").join("habit-2026-11.html").exists());
        assert!(root.join("modules").join("README.txt").exists());
        assert!(root.join("reports").join("taxes.pdf").exists());
        assert!(root.join("modules").exists());
        assert!(root.join("reports").exists());
    }

    #[test]
    fn a_cargo_build_says_how_to_uninstall_instead_of_deleting_itself() {
        let temp = date_dir();
        let root = temp.path().join("habit");
        fs::create_dir_all(&root).unwrap();
        let data = root.join("data.json");
        fs::write(&data, "{}").unwrap();
        let exe = temp.path().join("target").join("debug").join("habit");
        fs::create_dir_all(exe.parent().unwrap()).unwrap();
        fs::write(&exe, b"bin").unwrap();

        let plan = plan(&data, None, &exe, Vec::new());
        assert!(plan.binaries.is_empty());
        assert!(plan.package_manager.unwrap().contains("cargo uninstall"));
    }

    #[test]
    fn jobs_are_listed_even_when_nothing_else_exists() {
        let temp = date_dir();
        let job = temp.path().join("com.habit.reminder.0900.plist");
        fs::write(&job, b"plist").unwrap();
        let plan = plan(
            &temp.path().join("missing.json"),
            None,
            &temp.path().join("bin").join("habit"),
            vec![job.clone()],
        );
        assert!(plan.paths().contains(&(job.as_path(), "scheduled job")));
    }
}
