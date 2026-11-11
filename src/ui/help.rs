//! The `?` key overlay.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Paragraph};

use crate::format::pad_right;
use crate::theme::Theme;

const KEYS: &[(&str, &str)] = &[
    ("space", "check off / undo the selected day"),
    ("c", "cheat day (one per month)"),
    ("[ ]", "previous / next day"),
    ("t", "back to today"),
    ("j k ↑ ↓", "move"),
    ("g G", "first / last"),
    ("tab", "switch habits / tags / archive"),
    ("n", "note for a chosen day"),
    ("b", "show / hide the bottom bar"),
    ("a", "add a habit"),
    ("e", "edit a habit · rename a tag"),
    ("d", "archive or restore"),
    ("x", "delete (asks first)"),
    ("u", "undo the last change"),
    ("/", "filter by text, tag or #tag"),
    ("T", "next colour theme"),
    ("m", "dark / light mode"),
    ("r", "reload the file from disk"),
    ("?", "this help"),
    ("esc", "back out / quit"),
    ("q", "quit"),
];

pub fn draw(frame: &mut Frame, area: Rect, theme: &Theme) {
    let width = area.width.saturating_sub(4).clamp(34, 56);
    let height = (KEYS.len() as u16 + 6).min(area.height.saturating_sub(2));
    let x = area.x + area.width.saturating_sub(width) / 2;
    let y = area.y + area.height.saturating_sub(height) / 2;
    let popup = Rect {
        x,
        y,
        width,
        height,
    };

    frame.render_widget(Clear, popup);
    let block = Block::bordered()
        .title(" Keys ")
        .border_style(Style::default().fg(theme.border))
        .style(theme.base());
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    let mut lines: Vec<Line> = Vec::with_capacity(KEYS.len() + 3);
    lines.push(Line::from(vec![
        Span::styled(
            "habit",
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled("  terminal habit tracker", Style::default().fg(theme.dim)),
    ]));
    lines.push(Line::raw(""));
    for (keys, action) in KEYS {
        lines.push(Line::from(vec![
            Span::styled(pad_right(keys, 12), Style::default().fg(theme.warn)),
            Span::styled(*action, Style::default().fg(theme.fg)),
        ]));
    }
    lines.push(Line::raw(""));
    lines.push(Line::from(Span::styled(
        "any key closes this",
        Style::default().fg(theme.dim),
    )));

    frame.render_widget(Paragraph::new(lines).style(theme.base()), inner);
}
