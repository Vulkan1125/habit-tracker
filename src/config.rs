//! Optional user configuration at `~/.config/habit/config.toml`.

use std::env;
use std::fs;
use std::path::PathBuf;

use anyhow::Context;
use serde::{Deserialize, Serialize};

use crate::model::Day;
use crate::theme::ThemeKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum ThemeName {
    #[default]
    Dark,
    Light,
    Mono,
}

impl From<ThemeName> for ThemeKind {
    fn from(name: ThemeName) -> Self {
        match name {
            ThemeName::Dark => ThemeKind::Dark,
            ThemeName::Light => ThemeKind::Light,
            ThemeName::Mono => ThemeKind::Mono,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub theme: ThemeName,
    pub week_start: Day,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            theme: ThemeName::Dark,
            week_start: Day::Mon,
        }
    }
}

impl Config {
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
                return (Config::default(), None);
            }
            Err(error) => {
                return (
                    Config::default(),
                    Some(format!("could not read {}: {error}", path.display())),
                );
            }
        };
        match toml::from_str(&raw).with_context(|| format!("in {}", path.display())) {
            Ok(config) => (config, None),
            Err(error) => {
                let compact = error
                    .to_string()
                    .split_whitespace()
                    .collect::<Vec<_>>()
                    .join(" ");
                (
                    Config::default(),
                    Some(format!("ignoring config: {compact}")),
                )
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_sensible() {
        let config = Config::default();
        assert_eq!(config.week_start, Day::Mon);
        assert_eq!(config.theme, ThemeName::Dark);
    }

    #[test]
    fn parses_partial_files() {
        let config: Config = toml::from_str("week_start = \"Sun\"\n").unwrap();
        assert_eq!(config.week_start, Day::Sun);
        assert_eq!(config.theme, ThemeName::Dark);
    }

    #[test]
    fn rejects_typos_instead_of_silently_defaulting() {
        let result: Result<Config, _> = toml::from_str("week_startt = \"Sun\"");
        assert!(result.is_err());
    }

    #[test]
    fn rejects_unknown_themes() {
        let result: Result<Config, _> = toml::from_str("theme = \"neon\"");
        assert!(result.is_err());
    }
}
