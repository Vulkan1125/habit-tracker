//! Loading and saving the JSON file, atomically.

use std::env;
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use crate::model::{DATA_VERSION, Data};

/// A JSON file at `~/.local/share/habit/data.json` unless told otherwise.
#[derive(Debug, Clone)]
pub struct Store {
    path: PathBuf,
}

impl Store {
    /// Resolution order: explicit flag, `HABITS_FILE`, then the XDG data dir.
    pub fn resolve(explicit: Option<PathBuf>) -> Result<Self> {
        if let Some(path) = explicit {
            return Ok(Self { path });
        }
        if let Some(path) = env::var_os("HABITS_FILE").filter(|value| !value.is_empty()) {
            return Ok(Self {
                path: PathBuf::from(path),
            });
        }
        let base = env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .filter(|path| path.is_absolute())
            .or_else(|| {
                env::var_os("HOME").map(|home| PathBuf::from(home).join(".local").join("share"))
            })
            .context("cannot find a data directory; set HOME, XDG_DATA_HOME or HABITS_FILE")?;
        Ok(Self {
            path: base.join("habit").join("data.json"),
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn load(&self) -> Result<Data> {
        let raw = match fs::read_to_string(&self.path) {
            Ok(raw) => raw,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Data::default());
            }
            Err(error) => {
                return Err(error).with_context(|| format!("reading {}", self.path.display()));
            }
        };
        if raw.trim().is_empty() {
            return Ok(Data::default());
        }
        let mut data: Data = serde_json::from_str(&raw)
            .with_context(|| format!("parsing {} (is it valid JSON?)", self.path.display()))?;
        if data.version > DATA_VERSION {
            bail!(
                "{} was written by a newer version (format {}; this build reads {DATA_VERSION})",
                self.path.display(),
                data.version
            );
        }
        // Older formats only lack fields that default cleanly; stamp the new
        // version so the next save does not leave an old file holding new fields.
        data.version = DATA_VERSION;
        Ok(data)
    }

    /// Write through a temp file in the same directory, fsync, then rename.
    ///
    /// The previous file is copied to `data.json.bak` first, so a bad edit or a
    /// crash mid-write cannot destroy history.
    pub fn save(&self, data: &Data) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
        }

        let json = serde_json::to_string_pretty(data).context("serialising data")?;
        let temp = self.path.with_extension("json.tmp");

        {
            let mut file =
                File::create(&temp).with_context(|| format!("creating {}", temp.display()))?;
            file.write_all(json.as_bytes())
                .with_context(|| format!("writing {}", temp.display()))?;
            file.write_all(b"\n").ok();
            file.sync_all().context("flushing data file")?;
        }

        if self.path.exists() {
            let backup = self.path.with_extension("json.bak");
            let _ = fs::copy(&self.path, &backup);
        }

        fs::rename(&temp, &self.path)
            .with_context(|| format!("replacing {}", self.path.display()))?;

        // Best-effort directory sync so the rename survives a power cut.
        #[cfg(unix)]
        if let Some(parent) = self.path.parent()
            && let Ok(dir) = File::open(parent)
        {
            let _ = dir.sync_all();
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Schedule;
    use chrono::NaiveDate;

    fn date(text: &str) -> NaiveDate {
        NaiveDate::parse_from_str(text, "%Y-%m-%d").unwrap()
    }

    #[test]
    fn missing_file_loads_as_empty_data() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::resolve(Some(dir.path().join("data.json"))).unwrap();
        let data = store.load().unwrap();
        assert!(data.habits.is_empty());
        assert_eq!(data.version, DATA_VERSION);
    }

    #[test]
    fn round_trips_habits_and_log() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::resolve(Some(dir.path().join("nested/data.json"))).unwrap();
        let mut data = Data::default();
        data.add_habit("Gym", Schedule::Daily, date("2026-01-01"))
            .unwrap();
        data.log.set(date("2026-11-10"), "gym", true);
        store.save(&data).unwrap();

        let loaded = store.load().unwrap();
        assert_eq!(loaded.habits.len(), 1);
        assert_eq!(loaded.habits[0].name, "Gym");
        assert!(loaded.log.is_done(date("2026-11-10"), "gym"));
    }

    #[test]
    fn saving_twice_leaves_a_backup_and_no_temp_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("data.json");
        let store = Store::resolve(Some(path.clone())).unwrap();
        let mut data = Data::default();
        data.add_habit("Gym", Schedule::Daily, date("2026-01-01"))
            .unwrap();
        store.save(&data).unwrap();
        data.add_habit("Read", Schedule::Daily, date("2026-01-01"))
            .unwrap();
        store.save(&data).unwrap();

        assert!(path.exists());
        assert!(path.with_extension("json.bak").exists());
        assert!(!path.with_extension("json.tmp").exists());
        assert_eq!(store.load().unwrap().habits.len(), 2);
    }

    #[test]
    fn empty_file_is_treated_as_no_data() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("data.json");
        fs::write(&path, "  \n").unwrap();
        let store = Store::resolve(Some(path)).unwrap();
        assert!(store.load().unwrap().habits.is_empty());
    }

    #[test]
    fn upgrades_an_old_file_on_load_and_saves_it_as_current() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("data.json");
        fs::write(
            &path,
            r#"{"version": 1, "habits": [{"id": "gym", "name": "Gym", "schedule": {"kind": "daily"}, "created": "2026-01-01"}], "log": {"2026-01-02": ["gym"]}}"#,
        )
        .unwrap();
        let store = Store::resolve(Some(path.clone())).unwrap();
        let data = store.load().unwrap();
        assert_eq!(data.version, DATA_VERSION);
        assert_eq!(data.habits[0].tags.len(), 0);
        assert!(data.habits[0].is_active_on(date("2026-01-02")));
        store.save(&data).unwrap();
        let raw = fs::read_to_string(&path).unwrap();
        assert!(raw.contains("\"version\": 4"), "{raw}");
    }

    #[test]
    fn refuses_a_future_format_version() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("data.json");
        fs::write(&path, r#"{"version": 99, "habits": [], "log": {}}"#).unwrap();
        let store = Store::resolve(Some(path)).unwrap();
        let error = store.load().unwrap_err().to_string();
        assert!(error.contains("newer version"), "{error}");
    }

    #[test]
    fn reports_broken_json_with_the_path() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("data.json");
        fs::write(&path, "{not json").unwrap();
        let store = Store::resolve(Some(path.clone())).unwrap();
        let error = format!("{:#}", store.load().unwrap_err());
        assert!(error.contains("data.json"), "{error}");
    }
}
