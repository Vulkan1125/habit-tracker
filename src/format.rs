//! Small text helpers shared by the CLI and the TUI.

use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

/// `1 day`, `0 days`.
pub fn plural(count: u32, unit: &str) -> String {
    if count == 1 {
        format!("{count} {unit}")
    } else {
        format!("{count} {unit}s")
    }
}

pub fn width(text: &str) -> usize {
    UnicodeWidthStr::width(text)
}

/// Cut `text` to at most `max` display columns, ending with `…` when truncated.
pub fn truncate(text: &str, max: usize) -> String {
    if width(text) <= max {
        return text.to_string();
    }
    if max == 0 {
        return String::new();
    }
    let mut out = String::new();
    let mut used = 0;
    for character in text.chars() {
        let w = character.width().unwrap_or(0);
        if used + w > max.saturating_sub(1) {
            break;
        }
        out.push(character);
        used += w;
    }
    out.push('…');
    out
}

pub fn pad_right(text: &str, target: usize) -> String {
    let mut out = truncate(text, target);
    let pad = target.saturating_sub(width(&out));
    out.extend(std::iter::repeat_n(' ', pad));
    out
}

pub fn pad_left(text: &str, target: usize) -> String {
    let text = truncate(text, target);
    let pad = target.saturating_sub(width(&text));
    let mut out = String::with_capacity(target);
    out.extend(std::iter::repeat_n(' ', pad));
    out.push_str(&text);
    out
}

/// Show paths under the home directory as `~/…`, like the docs do.
pub fn pretty_path(path: &std::path::Path) -> String {
    if let Some(home) = std::env::var_os("HOME")
        && let Ok(rest) = path.strip_prefix(std::path::PathBuf::from(home))
    {
        return format!("~/{}", rest.display());
    }
    path.display().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pluralizes_only_one() {
        assert_eq!(plural(0, "day"), "0 days");
        assert_eq!(plural(1, "day"), "1 day");
        assert_eq!(plural(3, "week"), "3 weeks");
    }

    #[test]
    fn truncation_respects_display_width() {
        assert_eq!(truncate("gym", 10), "gym");
        assert_eq!(truncate("morning walk", 8), "morning…");
        assert_eq!(width(&truncate("日本語のテキスト", 5)), 5);
        assert_eq!(truncate("anything", 0), "");
    }

    #[test]
    fn padding_is_exact() {
        assert_eq!(width(&pad_right("gym", 6)), 6);
        assert_eq!(width(&pad_left("gym", 6)), 6);
        assert_eq!(pad_left("12", 4), "  12");
        // never wider than asked
        assert_eq!(width(&pad_right("a-very-long-name", 5)), 5);
    }
}
