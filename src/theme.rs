//! Colour palettes. Plain ANSI colours so they inherit the terminal theme.

use ratatui::style::Color;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemeKind {
    Dark,
    Light,
    Mono,
}

#[derive(Debug, Clone, Copy)]
pub struct Theme {
    pub accent: Color,
    pub done: Color,
    pub missed: Color,
    pub dim: Color,
    pub warn: Color,
    pub border: Color,
}

pub const BLOCKS: [&str; 4] = ["▁", "▃", "▅", "█"];

impl Theme {
    pub fn new(kind: ThemeKind) -> Self {
        match kind {
            ThemeKind::Dark => Self {
                accent: Color::Cyan,
                done: Color::Green,
                missed: Color::Red,
                dim: Color::DarkGray,
                warn: Color::Yellow,
                border: Color::DarkGray,
            },
            ThemeKind::Light => Self {
                accent: Color::Blue,
                done: Color::Green,
                missed: Color::Red,
                dim: Color::Gray,
                warn: Color::Magenta,
                border: Color::Gray,
            },
            ThemeKind::Mono => Self {
                accent: Color::Reset,
                done: Color::Reset,
                missed: Color::Reset,
                dim: Color::Reset,
                warn: Color::Reset,
                border: Color::Reset,
            },
        }
    }

    /// `▁▃▅█`-style bar, filled to `ratio` (0.0..=1.0).
    pub fn bar(&self, ratio: f64, width: usize) -> (String, String) {
        let ratio = ratio.clamp(0.0, 1.0);
        let filled = (ratio * width as f64).round() as usize;
        (
            BLOCKS[3].repeat(filled),
            "░".repeat(width.saturating_sub(filled)),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bar_fills_proportionally() {
        let theme = Theme::new(ThemeKind::Dark);
        let (full, empty) = theme.bar(0.5, 10);
        assert_eq!(full.chars().count(), 5);
        assert_eq!(empty.chars().count(), 5);
        let (full, empty) = theme.bar(1.5, 4);
        assert_eq!(full.chars().count(), 4);
        assert!(empty.is_empty());
    }
}
