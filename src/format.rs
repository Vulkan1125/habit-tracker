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

/// Greedy word wrap at `columns` display columns, capped at `max_lines`.
///
/// Long words are split rather than allowed to overflow, and a capped result
/// ends with `…` on its last line.
pub fn wrap(text: &str, columns: usize, max_lines: usize) -> Vec<String> {
    if columns == 0 || max_lines == 0 {
        return Vec::new();
    }
    let mut lines: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut used = 0usize;
    for word in text.split_whitespace() {
        let word_width = width(word);
        if used > 0 {
            if used + 1 + word_width <= columns {
                current.push(' ');
                used += 1;
            } else {
                lines.push(std::mem::take(&mut current));
                used = 0;
            }
        }
        for character in word.chars() {
            let character_width = character.width().unwrap_or(0);
            if used > 0 && used + character_width > columns {
                lines.push(std::mem::take(&mut current));
                used = 0;
            }
            current.push(character);
            used += character_width;
        }
    }
    if !current.is_empty() {
        lines.push(current);
    }
    if lines.len() > max_lines {
        lines.truncate(max_lines);
        if let Some(last) = lines.last_mut() {
            let mut cut = String::new();
            let mut used = 0usize;
            for character in last.chars() {
                let character_width = character.width().unwrap_or(0);
                if used + character_width > columns.saturating_sub(1) {
                    break;
                }
                cut.push(character);
                used += character_width;
            }
            cut.push('…');
            *last = cut;
        }
    }
    lines
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
    fn wrapping_respects_display_width_and_caps() {
        assert_eq!(wrap("one two three", 9, 2), vec!["one two", "three"]);
        assert_eq!(wrap("abcdefgh", 3, 2), vec!["abc", "de…"]);
        assert_eq!(wrap("one two three four", 7, 2), vec!["one two", "three…"]);
        assert!(wrap("anything", 0, 2).is_empty());
        assert!(wrap("anything", 4, 0).is_empty());
    }

    #[test]
    fn wrapping_keeps_wide_characters_inside_the_width() {
        let lines = wrap("日本語のテキスト", 4, 3);
        assert!(lines.iter().all(|line| width(line) <= 4), "{lines:?}");
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
