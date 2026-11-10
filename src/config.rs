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
}

impl Default for Config {
    fn default() -> Self {
        Self {
            theme: ThemeName::Terminal,
            mode: ColorMode::Dark,
            week_start: Day::Mon,
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
                return (Config::default(), None);
            }
            Err(error) => {
                return (
                    Config::default(),
                    Some(format!("could not read {}: {error}", path.display())),
                );
            }
        };
        match Self::parse(&raw) {
            Ok(config) => (config, None),
            Err(error) => (
                Config::default(),
                Some(format!("ignoring config {}: {error}", path.display())),
            ),
        }
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
}
