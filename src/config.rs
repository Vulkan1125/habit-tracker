//! Optional user configuration at `~/.config/habit/config.toml`.

use std::env;
use std::fs;
use std::path::PathBuf;

use serde::Deserialize;

use crate::model::Day;
use crate::theme::{ColorMode, Theme, ThemeName};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    pub theme: ThemeName,
    pub mode: ColorMode,
    pub week_start: Day,
    /// Where this config was loaded from, so the TUI can save theme changes
    /// back. `None` means "not loaded from a file" (tests, or no home dir).
    path: Option<PathBuf>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            theme: ThemeName::Terminal,
            mode: ColorMode::Dark,
            week_start: Day::Mon,
            path: None,
        }
    }
}

impl Config {
    /// Resolve the configured palette and terminal capabilities.
    pub fn palette(&self) -> Theme {
        Theme::new(self.theme, self.mode)
    }

    pub fn path() -> Option<PathBuf> {
        let base = env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .filter(|path| path.is_absolute())
            .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))?;
        Some(base.join("habit").join("config.toml"))
    }

    /// Missing file means defaults. A broken file is reported but not fatal.
    pub fn load() -> (Config, Option<String>) {
        let Some(path) = Self::path() else {
            return (Config::default(), None);
        };
        let raw = match fs::read_to_string(&path) {
            Ok(raw) => raw,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return (
                    Config {
                        path: Some(path),
                        ..Config::default()
                    },
                    None,
                );
            }
            Err(error) => {
                let warning = format!("could not read {}: {error}", path.display());
                return (
                    Config {
                        path: Some(path),
                        ..Config::default()
                    },
                    Some(warning),
                );
            }
        };
        match Self::parse(&raw) {
            Ok(config) => (
                Config {
                    path: Some(path),
                    ..config
                },
                None,
            ),
            Err(error) => {
                let warning = format!("ignoring config {}: {error}", path.display());
                (
                    Config {
                        path: Some(path),
                        ..Config::default()
                    },
                    Some(warning),
                )
            }
        }
    }

    /// Rewrite the config file with the current theme, mode and week start.
    ///
    /// Comments in a hand-written file are not preserved; the file is small and
    /// the values are what matter. A config that was never loaded from disk
    /// (for example in tests) is not written anywhere.
    pub fn save(&self) -> Result<(), String> {
        let Some(path) = &self.path else {
            return Err("no config path to write to".to_string());
        };
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| format!("creating {}: {error}", parent.display()))?;
        }
        fs::write(path, self.to_toml())
            .map_err(|error| format!("writing {}: {error}", path.display()))
    }

    pub fn to_toml(&self) -> String {
        format!(
            "# habit configuration — `T` and `m` in the TUI update these values.\ntheme = \"{}\"\nmode = \"{}\"\nweek_start = \"{}\"\n",
            self.theme.label(),
            self.mode.label(),
            self.week_start.short()
        )
    }

    pub fn parse(raw: &str) -> Result<Config, String> {
        let raw: RawConfig = toml::from_str(raw).map_err(|error| flatten(&error.to_string()))?;
        let mut config = Config::default();

        if let Some(theme) = raw.theme.as_deref() {
            let lowered = theme.trim().to_ascii_lowercase();
            match lowered.as_str() {
                // 0.1.x spellings: `dark`/`light` were theme names.
                "dark" => config.mode = ColorMode::Dark,
                "light" => config.mode = ColorMode::Light,
                _ => {
                    config.theme = ThemeName::parse(theme).ok_or_else(|| {
                        format!(
                            "unknown theme \"{theme}\" (try one of: {})",
                            ThemeName::ALL
                                .iter()
                                .map(|name| name.label())
                                .collect::<Vec<_>>()
                                .join(", ")
                        )
                    })?;
                }
            }
        }

        if let Some(mode) = raw.mode.as_deref() {
            config.mode = ColorMode::parse(mode)
                .ok_or_else(|| format!("unknown mode \"{mode}\" (try dark or light)"))?;
        }

        if let Some(week_start) = raw.week_start {
            config.week_start = week_start;
        }

        Ok(config)
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawConfig {
    theme: Option<String>,
    mode: Option<String>,
    week_start: Option<Day>,
}

fn flatten(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_sensible() {
        let config = Config::default();
        assert_eq!(config.week_start, Day::Mon);
        assert_eq!(config.theme, ThemeName::Terminal);
        assert_eq!(config.mode, ColorMode::Dark);
    }

    #[test]
    fn parses_partial_files() {
        let config = Config::parse("week_start = \"Sun\"\n").unwrap();
        assert_eq!(config.week_start, Day::Sun);
        assert_eq!(config.theme, ThemeName::Terminal);
    }

    #[test]
    fn parses_a_theme_and_a_mode() {
        let config = Config::parse("theme = \"catppuccin\"\nmode = \"light\"\n").unwrap();
        assert_eq!(config.theme, ThemeName::Catppuccin);
        assert_eq!(config.mode, ColorMode::Light);
    }

    #[test]
    fn accepts_every_documented_theme_name() {
        for name in ThemeName::ALL {
            let raw = format!("theme = \"{}\"", name.label());
            let config = Config::parse(&raw).unwrap();
            assert_eq!(config.theme, name, "{}", name.label());
        }
    }

    #[test]
    fn legacy_theme_names_still_work() {
        let config = Config::parse("theme = \"light\"").unwrap();
        assert_eq!(config.theme, ThemeName::Terminal);
        assert_eq!(config.mode, ColorMode::Light);
        let config = Config::parse("theme = \"dark\"").unwrap();
        assert_eq!(config.mode, ColorMode::Dark);
    }

    #[test]
    fn rejects_typos_instead_of_silently_defaulting() {
        let error = Config::parse("week_startt = \"Sun\"").unwrap_err();
        assert!(error.contains("week_startt"), "{error}");
    }

    #[test]
    fn rejects_unknown_themes_with_a_helpful_message() {
        let error = Config::parse("theme = \"neon\"").unwrap_err();
        assert!(error.contains("neon"), "{error}");
        assert!(error.contains("catppuccin"), "{error}");
        let error = Config::parse("theme = \"mono\"\nmode = \"sepia\"").unwrap_err();
        assert!(error.contains("sepia"), "{error}");
    }

    #[test]
    fn to_toml_round_trips_every_field() {
        let config =
            Config::parse("theme = \"gruvbox\"\nmode = \"light\"\nweek_start = \"Sun\"").unwrap();
        let parsed = Config::parse(&config.to_toml()).unwrap();
        assert_eq!(parsed.theme, ThemeName::Gruvbox);
        assert_eq!(parsed.mode, ColorMode::Light);
        assert_eq!(parsed.week_start, Day::Sun);
    }

    #[test]
    fn save_writes_the_loaded_path_and_creates_parents() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested").join("config.toml");
        let mut config = Config::parse("theme = \"evergreen\"\nmode = \"light\"").unwrap();
        config.path = Some(path.clone());
        config.save().unwrap();
        let raw = fs::read_to_string(&path).unwrap();
        assert!(raw.contains("theme = \"evergreen\""), "{raw}");
        assert!(raw.contains("mode = \"light\""), "{raw}");
    }

    #[test]
    fn saving_without_a_loaded_path_is_an_error_not_a_guess() {
        assert!(Config::default().save().is_err());
    }
}
