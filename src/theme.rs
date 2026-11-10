//! Colour palettes and terminal capability handling.
//!
//! Seven palettes, each with a light and a dark variant. Colours are defined in
//! truecolour and downgraded to the 256-colour cube or the 16 ANSI colours when
//! the terminal does not advertise better support.

use std::env;

use ratatui::style::{Color, Style};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ThemeName {
    /// Inherit the terminal's own colours (ANSI named colours only).
    #[default]
    Terminal,
    RosePine,
    Catppuccin,
    TokyoNight,
    Gruvbox,
    Evergreen,
    /// No colour at all: weight and glyphs carry the meaning.
    Mono,
}

impl ThemeName {
    pub const ALL: [ThemeName; 7] = [
        ThemeName::Terminal,
        ThemeName::RosePine,
        ThemeName::Catppuccin,
        ThemeName::TokyoNight,
        ThemeName::Gruvbox,
        ThemeName::Evergreen,
        ThemeName::Mono,
    ];

    pub const fn label(self) -> &'static str {
        match self {
            ThemeName::Terminal => "terminal",
            ThemeName::RosePine => "rosepine",
            ThemeName::Catppuccin => "catppuccin",
            ThemeName::TokyoNight => "tokyonight",
            ThemeName::Gruvbox => "gruvbox",
            ThemeName::Evergreen => "evergreen",
            ThemeName::Mono => "mono",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        let needle = text
            .trim()
            .to_ascii_lowercase()
            .replace(['-', '_', ' '], "");
        Self::ALL
            .into_iter()
            .find(|name| name.label().replace(['-', '_'], "") == needle)
            .or(match needle.as_str() {
                "rosepine" | "rosé" => Some(ThemeName::RosePine),
                "tokyo" | "tokyonight" | "tokyonightstorm" => Some(ThemeName::TokyoNight),
                "cat" | "catppuccinmocha" | "catppuccinlatte" => Some(ThemeName::Catppuccin),
                _ => None,
            })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ColorMode {
    #[default]
    Dark,
    Light,
}

impl ColorMode {
    pub const fn label(self) -> &'static str {
        match self {
            ColorMode::Dark => "dark",
            ColorMode::Light => "light",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        match text.trim().to_ascii_lowercase().as_str() {
            "dark" | "night" | "mocha" | "storm" => Some(ColorMode::Dark),
            "light" | "day" | "latte" | "dawn" => Some(ColorMode::Light),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Caps {
    TrueColor,
    Ansi256,
    Ansi16,
}

impl Caps {
    /// Best guess from the environment, the same way most TUIs do it.
    pub fn detect() -> Caps {
        if env::var("COLORTERM")
            .map(|value| {
                let value = value.to_ascii_lowercase();
                value.contains("truecolor") || value.contains("24bit")
            })
            .unwrap_or(false)
        {
            return Caps::TrueColor;
        }
        if env::var("TERM")
            .map(|value| value.to_ascii_lowercase().contains("256"))
            .unwrap_or(false)
        {
            return Caps::Ansi256;
        }
        Caps::Ansi16
    }
}

pub const BLOCKS: [&str; 8] = ["▁", "▂", "▃", "▄", "▅", "▆", "▇", "█"];

#[derive(Debug, Clone, Copy)]
pub struct Theme {
    pub name: ThemeName,
    pub accent: Color,
    pub done: Color,
    pub missed: Color,
    pub warn: Color,
    pub cheat: Color,
    /// Selected row background, when the palette paints one.
    pub surface: Option<Color>,
    pub dim: Color,
    pub border: Color,
    pub fg: Color,
    pub bg: Option<Color>,
    accents: [Color; 5],
}

impl Theme {
    pub fn new(name: ThemeName, mode: ColorMode) -> Self {
        Self::with_caps(name, mode, Caps::detect())
    }

    pub fn with_caps(name: ThemeName, mode: ColorMode, caps: Caps) -> Self {
        let raw = palette(name, mode);
        let map = |color: Color| adjust(color, caps);
        // The 16-colour palette has no pastel backgrounds; painting one would
        // recolour the whole terminal, so leave the background alone instead.
        let paint_backgrounds = caps != Caps::Ansi16;
        Self {
            name,
            accent: map(raw.accent),
            done: map(raw.done),
            missed: map(raw.missed),
            warn: map(raw.warn),
            cheat: map(raw.cheat),
            surface: if paint_backgrounds {
                raw.surface.map(map)
            } else {
                None
            },
            dim: map(raw.dim),
            border: map(raw.border),
            fg: raw.fg.map(map).unwrap_or(Color::Reset),
            bg: if paint_backgrounds {
                raw.bg.map(map)
            } else {
                None
            },
            accents: raw.accents.map(map),
        }
    }

    pub fn is_mono(&self) -> bool {
        self.name == ThemeName::Mono
    }

    /// Base style for panes: the palette's text colour and optional background.
    pub fn base(&self) -> Style {
        let style = Style::default().fg(self.fg);
        match self.bg {
            Some(bg) => style.bg(bg),
            None => style,
        }
    }

    /// A stable colour for a habit, derived from its id so it never changes.
    pub fn habit_accent(&self, id: &str) -> Color {
        if self.is_mono() {
            return Color::Reset;
        }
        let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
        for byte in id.bytes() {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
        self.accents[(hash % self.accents.len() as u64) as usize]
    }

    /// `▁▃▅█`-style bar, filled to `ratio` (0.0..=1.0).
    pub fn bar(&self, ratio: f64, width: usize) -> (String, String) {
        let ratio = ratio.clamp(0.0, 1.0);
        let filled = (ratio * width as f64).round() as usize;
        (
            BLOCKS[7].repeat(filled),
            "░".repeat(width.saturating_sub(filled)),
        )
    }

    /// Sparkline glyph for `0.0..=1.0`.
    pub fn spark(&self, value: f64) -> &'static str {
        let index = (value.clamp(0.0, 1.0) * 7.0).round() as usize;
        BLOCKS[index.min(7)]
    }
}

struct RawPalette {
    accent: Color,
    accents: [Color; 5],
    done: Color,
    missed: Color,
    warn: Color,
    cheat: Color,
    dim: Color,
    border: Color,
    surface: Option<Color>,
    fg: Option<Color>,
    bg: Option<Color>,
}

const fn rgb(hex: u32) -> Color {
    Color::Rgb(
        ((hex >> 16) & 0xff) as u8,
        ((hex >> 8) & 0xff) as u8,
        (hex & 0xff) as u8,
    )
}

fn palette(name: ThemeName, mode: ColorMode) -> RawPalette {
    match (name, mode) {
        (ThemeName::Mono, _) => RawPalette {
            accent: Color::Reset,
            accents: [Color::Reset; 5],
            done: Color::Reset,
            missed: Color::Reset,
            warn: Color::Reset,
            cheat: Color::Reset,
            dim: Color::Reset,
            border: Color::Reset,
            surface: None,
            fg: None,
            bg: None,
        },
        (ThemeName::Terminal, ColorMode::Dark) => RawPalette {
            accent: Color::Cyan,
            accents: [
                Color::Cyan,
                Color::Magenta,
                Color::Blue,
                Color::Green,
                Color::Yellow,
            ],
            done: Color::Green,
            missed: Color::Red,
            warn: Color::Yellow,
            cheat: Color::Magenta,
            dim: Color::DarkGray,
            border: Color::DarkGray,
            surface: None,
            fg: None,
            bg: None,
        },
        (ThemeName::Terminal, ColorMode::Light) => RawPalette {
            accent: Color::Blue,
            accents: [
                Color::Blue,
                Color::Magenta,
                Color::Cyan,
                Color::Green,
                Color::Red,
            ],
            done: Color::Green,
            missed: Color::Red,
            warn: Color::Yellow,
            cheat: Color::Cyan,
            dim: Color::Gray,
            border: Color::Gray,
            surface: None,
            fg: None,
            bg: None,
        },
        (ThemeName::RosePine, ColorMode::Dark) => RawPalette {
            accent: rgb(0xc4a7e7),
            accents: [
                rgb(0xc4a7e7),
                rgb(0xebbcba),
                rgb(0xf6c177),
                rgb(0x9ccfd8),
                rgb(0xeb6f92),
            ],
            done: rgb(0x9ccfd8),
            missed: rgb(0xeb6f92),
            warn: rgb(0xf6c177),
            cheat: rgb(0x31748f),
            dim: rgb(0x6e6a86),
            border: rgb(0x403d52),
            surface: Some(rgb(0x26233a)),
            fg: Some(rgb(0xe0def4)),
            bg: Some(rgb(0x191724)),
        },
        (ThemeName::RosePine, ColorMode::Light) => RawPalette {
            accent: rgb(0x907aa9),
            accents: [
                rgb(0x907aa9),
                rgb(0xd7827e),
                rgb(0xea9d34),
                rgb(0x56949f),
                rgb(0xb4637a),
            ],
            done: rgb(0x286983),
            missed: rgb(0xb4637a),
            warn: rgb(0xea9d34),
            cheat: rgb(0x56949f),
            dim: rgb(0x9893a5),
            border: rgb(0xdfdad9),
            surface: Some(rgb(0xf2e9e1)),
            fg: Some(rgb(0x575279)),
            bg: Some(rgb(0xfaf4ed)),
        },
        (ThemeName::Catppuccin, ColorMode::Dark) => RawPalette {
            accent: rgb(0xcba6f7),
            accents: [
                rgb(0xcba6f7),
                rgb(0x89b4fa),
                rgb(0x94e2d5),
                rgb(0xf5c2e7),
                rgb(0xfab387),
            ],
            done: rgb(0xa6e3a1),
            missed: rgb(0xf38ba8),
            warn: rgb(0xf9e2af),
            cheat: rgb(0x94e2d5),
            dim: rgb(0x6c7086),
            border: rgb(0x45475a),
            surface: Some(rgb(0x313244)),
            fg: Some(rgb(0xcdd6f4)),
            bg: Some(rgb(0x1e1e2e)),
        },
        (ThemeName::Catppuccin, ColorMode::Light) => RawPalette {
            accent: rgb(0x8839ef),
            accents: [
                rgb(0x8839ef),
                rgb(0x1e66f5),
                rgb(0x179299),
                rgb(0xea76cb),
                rgb(0xfe640b),
            ],
            done: rgb(0x40a02b),
            missed: rgb(0xd20f39),
            warn: rgb(0xdf8e1d),
            cheat: rgb(0x179299),
            dim: rgb(0x9ca0b0),
            border: rgb(0xccd0da),
            surface: Some(rgb(0xe6e9ef)),
            fg: Some(rgb(0x4c4f69)),
            bg: Some(rgb(0xeff1f5)),
        },
        (ThemeName::TokyoNight, ColorMode::Dark) => RawPalette {
            accent: rgb(0x7aa2f7),
            accents: [
                rgb(0x7aa2f7),
                rgb(0xbb9af7),
                rgb(0x7dcfff),
                rgb(0x9ece6a),
                rgb(0xff9e64),
            ],
            done: rgb(0x9ece6a),
            missed: rgb(0xf7768e),
            warn: rgb(0xe0af68),
            cheat: rgb(0x7dcfff),
            dim: rgb(0x565f89),
            border: rgb(0x292e42),
            surface: Some(rgb(0x24283b)),
            fg: Some(rgb(0xc0caf5)),
            bg: Some(rgb(0x1a1b26)),
        },
        (ThemeName::TokyoNight, ColorMode::Light) => RawPalette {
            accent: rgb(0x2e7de9),
            accents: [
                rgb(0x2e7de9),
                rgb(0x9854f1),
                rgb(0x007197),
                rgb(0x587539),
                rgb(0xb15c00),
            ],
            done: rgb(0x587539),
            missed: rgb(0xf52a65),
            warn: rgb(0x8c6c3e),
            cheat: rgb(0x007197),
            dim: rgb(0x6172b0),
            border: rgb(0xc4c8da),
            surface: Some(rgb(0xd0d5e3)),
            fg: Some(rgb(0x3760bf)),
            bg: Some(rgb(0xe1e2e7)),
        },
        (ThemeName::Gruvbox, ColorMode::Dark) => RawPalette {
            accent: rgb(0xfabd2f),
            accents: [
                rgb(0xfabd2f),
                rgb(0x83a598),
                rgb(0xd3869b),
                rgb(0x8ec07c),
                rgb(0xfe8019),
            ],
            done: rgb(0xb8bb26),
            missed: rgb(0xfb4934),
            warn: rgb(0xfabd2f),
            cheat: rgb(0x8ec07c),
            dim: rgb(0x928374),
            border: rgb(0x3c3836),
            surface: Some(rgb(0x32302f)),
            fg: Some(rgb(0xebdbb2)),
            bg: Some(rgb(0x282828)),
        },
        (ThemeName::Gruvbox, ColorMode::Light) => RawPalette {
            accent: rgb(0xb57614),
            accents: [
                rgb(0xb57614),
                rgb(0x076678),
                rgb(0x8f3f71),
                rgb(0x427b58),
                rgb(0xaf3a03),
            ],
            done: rgb(0x79740e),
            missed: rgb(0x9d0006),
            warn: rgb(0xb57614),
            cheat: rgb(0x427b58),
            dim: rgb(0x928374),
            border: rgb(0xebdbb2),
            surface: Some(rgb(0xf2e5bc)),
            fg: Some(rgb(0x3c3836)),
            bg: Some(rgb(0xfbf1c7)),
        },
        (ThemeName::Evergreen, ColorMode::Dark) => RawPalette {
            accent: rgb(0x6fbf9a),
            accents: [
                rgb(0x6fbf9a),
                rgb(0x8fd694),
                rgb(0xe0c27a),
                rgb(0x7fd4d0),
                rgb(0xc9a0dc),
            ],
            done: rgb(0x8fd694),
            missed: rgb(0xe07a7a),
            warn: rgb(0xe0c27a),
            cheat: rgb(0x7fd4d0),
            dim: rgb(0x5d7a6f),
            border: rgb(0x24413a),
            surface: Some(rgb(0x172822)),
            fg: Some(rgb(0xd7e5df)),
            bg: Some(rgb(0x101b18)),
        },
        (ThemeName::Evergreen, ColorMode::Light) => RawPalette {
            accent: rgb(0x2f7d5f),
            accents: [
                rgb(0x2f7d5f),
                rgb(0x4f8f52),
                rgb(0xa97a1f),
                rgb(0x2a8f8a),
                rgb(0x7b5aa6),
            ],
            done: rgb(0x4f8f52),
            missed: rgb(0xb94a48),
            warn: rgb(0xa97a1f),
            cheat: rgb(0x2a8f8a),
            dim: rgb(0x7d9187),
            border: rgb(0xcfdcd3),
            surface: Some(rgb(0xe7efe8)),
            fg: Some(rgb(0x2b3a33)),
            bg: Some(rgb(0xf4f7f2)),
        },
    }
}

/// Downgrade a truecolour value to what the terminal can actually display.
fn adjust(color: Color, caps: Caps) -> Color {
    match (color, caps) {
        (Color::Rgb(r, g, b), Caps::Ansi256) => Color::Indexed(rgb_to_256(r, g, b)),
        (Color::Rgb(r, g, b), Caps::Ansi16) => rgb_to_16(r, g, b),
        _ => color,
    }
}

fn rgb_to_256(r: u8, g: u8, b: u8) -> u8 {
    // Grayscale ramp is better than the cube for neutrals.
    if r == g && g == b {
        if r < 8 {
            return 16;
        }
        if r > 248 {
            return 231;
        }
        return 232 + ((u16::from(r) - 8) * 24 / 247) as u8;
    }
    let scale = |value: u8| -> u8 {
        let value = u16::from(value);
        ((value * 5 + 127) / 255) as u8
    };
    16 + 36 * scale(r) + 6 * scale(g) + scale(b)
}

fn rgb_to_16(r: u8, g: u8, b: u8) -> Color {
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let chroma = max - min;

    // Neutral colours keep their lightness instead of snapping to a hue.
    if chroma < 40 {
        return match max {
            0..=30 => Color::Black,
            31..=90 => Color::DarkGray,
            91..=200 => Color::Gray,
            _ => Color::White,
        };
    }

    // Nearest of the six ANSI hues, brightened for light colours.
    let hue = if max == r {
        (60.0 * (f32::from(g) - f32::from(b)) / f32::from(chroma) + 360.0) % 360.0
    } else if max == g {
        60.0 * (f32::from(b) - f32::from(r)) / f32::from(chroma) + 120.0
    } else {
        60.0 * (f32::from(r) - f32::from(g)) / f32::from(chroma) + 240.0
    };
    let sector = (((hue + 30.0) / 60.0).floor() as usize) % 6;
    let bright = max >= 192;
    match (sector, bright) {
        (0, false) => Color::Red,
        (0, true) => Color::LightRed,
        (1, false) => Color::Yellow,
        (1, true) => Color::LightYellow,
        (2, false) => Color::Green,
        (2, true) => Color::LightGreen,
        (3, false) => Color::Cyan,
        (3, true) => Color::LightCyan,
        (4, false) => Color::Blue,
        (4, true) => Color::LightBlue,
        (_, false) => Color::Magenta,
        (_, true) => Color::LightMagenta,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bar_fills_proportionally() {
        let theme = Theme::new(ThemeName::Terminal, ColorMode::Dark);
        let (full, empty) = theme.bar(0.5, 10);
        assert_eq!(full.chars().count(), 5);
        assert_eq!(empty.chars().count(), 5);
        let (full, empty) = theme.bar(1.5, 4);
        assert_eq!(full.chars().count(), 4);
        assert!(empty.is_empty());
    }

    #[test]
    fn every_theme_has_a_light_and_a_dark_variant() {
        for name in ThemeName::ALL {
            let dark = Theme::with_caps(name, ColorMode::Dark, Caps::TrueColor);
            let light = Theme::with_caps(name, ColorMode::Light, Caps::TrueColor);
            if matches!(name, ThemeName::Mono | ThemeName::Terminal) {
                continue;
            }
            assert_ne!(
                dark.accent,
                light.accent,
                "{} should differ between modes",
                name.label()
            );
            assert!(
                light.bg.is_some() && dark.bg.is_some(),
                "{} should paint a background",
                name.label()
            );
        }
    }

    #[test]
    fn terminal_theme_keeps_named_colours_instead_of_rgb() {
        for mode in [ColorMode::Dark, ColorMode::Light] {
            let theme = Theme::with_caps(ThemeName::Terminal, mode, Caps::TrueColor);
            assert!(matches!(
                theme.accent,
                Color::Cyan | Color::Blue | Color::Magenta
            ));
            assert!(theme.bg.is_none());
        }
    }

    #[test]
    fn truecolor_is_downgraded_when_unsupported() {
        let theme = Theme::with_caps(ThemeName::Catppuccin, ColorMode::Dark, Caps::Ansi256);
        assert!(matches!(theme.accent, Color::Indexed(_)));
        assert!(matches!(theme.bg, Some(Color::Indexed(_))));

        let theme = Theme::with_caps(ThemeName::Catppuccin, ColorMode::Dark, Caps::Ansi16);
        assert!(!matches!(theme.accent, Color::Rgb(..) | Color::Indexed(_)));
        assert!(
            theme.bg.is_none(),
            "16-colour mode keeps the terminal background"
        );
    }

    #[test]
    fn habit_accents_are_stable_and_varied() {
        let theme = Theme::with_caps(ThemeName::Catppuccin, ColorMode::Dark, Caps::TrueColor);
        assert_eq!(theme.habit_accent("gym"), theme.habit_accent("gym"));
        let distinct: std::collections::BTreeSet<_> =
            ["gym", "read", "meditate", "stretch", "walk"]
                .into_iter()
                .map(|id| format!("{:?}", theme.habit_accent(id)))
                .collect();
        assert!(distinct.len() >= 2, "{distinct:?}");
    }

    #[test]
    fn mono_is_colourless() {
        let theme = Theme::with_caps(ThemeName::Mono, ColorMode::Dark, Caps::TrueColor);
        assert_eq!(theme.accent, Color::Reset);
        assert_eq!(theme.habit_accent("gym"), Color::Reset);
        assert!(theme.bg.is_none() && theme.surface.is_none());
    }

    #[test]
    fn names_and_modes_parse_forgivingly() {
        assert_eq!(ThemeName::parse("rose-pine"), Some(ThemeName::RosePine));
        assert_eq!(ThemeName::parse("Tokyo Night"), Some(ThemeName::TokyoNight));
        assert_eq!(ThemeName::parse("CATPPUCCIN"), Some(ThemeName::Catppuccin));
        assert_eq!(ThemeName::parse("neon"), None);
        assert_eq!(ColorMode::parse("latte"), Some(ColorMode::Light));
        assert_eq!(ColorMode::parse("dusk"), None);
    }

    #[test]
    fn sparkline_covers_the_whole_range() {
        let theme = Theme::new(ThemeName::Terminal, ColorMode::Dark);
        assert_eq!(theme.spark(0.0), "▁");
        assert_eq!(theme.spark(1.0), "█");
        assert_eq!(theme.spark(-5.0), "▁");
    }
}
