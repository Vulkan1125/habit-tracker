//! The right-hand pane: streaks, heatmap, dated notes and the habit description.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph};

use crate::app::App;
use habit::format::{pad_right, truncate};
use habit::model::Habit;
use habit::stats::{self, Status};
use habit::theme::Theme;

pub fn draw(frame: &mut Frame, area: Rect, app: &App) {
    let theme = app.theme;
    let block = Block::bordered()
        .title(" Details ")
        .border_style(Style::default().fg(theme.border))
        .style(theme.base());

    let Some(habit) = app.selected_habit() else {
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                " Select a habit to see its history ",
                Style::default().fg(theme.dim),
            )))
            .style(theme.base())
            .block(block),
            area,
        );
        return;
    };

    let inner_width = area.width.saturating_sub(2) as usize;
    let mut lines: Vec<Line> = Vec::new();

    let accent = theme.habit_accent(&habit.id);
    let mut title = vec![Span::styled(
        truncate(&habit.name, inner_width.saturating_sub(12)),
        Style::default().fg(accent).add_modifier(Modifier::BOLD),
    )];
    if habit.archived {
        title.push(Span::styled("  archived", Style::default().fg(theme.dim)));
    } else if habit.is_finished(app.today) {
        title.push(Span::styled("  finished", Style::default().fg(theme.warn)));
    }
    lines.push(Line::from(title));

    if let Some(note) = &habit.note {
        lines.push(Line::from(Span::styled(
            "description:",
            Style::default().fg(theme.dim).add_modifier(Modifier::BOLD),
        )));
        let indent = "    ";
        for chunk in wrap(note, inner_width.saturating_sub(indent.len()).max(8), 3) {
            lines.push(Line::from(Span::styled(
                format!("{indent}{chunk}"),
                Style::default().fg(theme.fg),
            )));
        }
    }

    let mut subtitle = vec![
        Span::raw("    "),
        Span::styled(habit.schedule.describe(), Style::default().fg(theme.dim)),
    ];
    subtitle.push(Span::styled(
        format!(" · {}", habit.window_label()),
        Style::default().fg(theme.dim),
    ));
    if app.modules.notify
        && let Some(reminder) = habit.reminder
    {
        subtitle.push(Span::styled(
            format!(" · reminder {}", reminder.format("%H:%M")),
            Style::default().fg(theme.warn),
        ));
    }
    lines.push(Line::from(subtitle));

    if !habit.tags.is_empty() {
        let mut chips = vec![Span::raw("    ")];
        for tag in habit.tags.iter().take(4) {
            chips.push(Span::styled(
                format!("#{tag} "),
                Style::default().fg(theme.accent),
            ));
        }
        lines.push(Line::from(chips));
    }
    lines.push(Line::raw(""));

    let streak = stats::current_streak(habit, &app.data.history(), app.today, app.week_start);
    let best = stats::longest_streak(habit, &app.data.history(), app.today, app.week_start);
    lines.push(Line::from(vec![
        Span::styled(pad_right("streak", 11), Style::default().fg(theme.dim)),
        Span::styled(
            streak.label(),
            Style::default()
                .fg(if streak.length > 0 {
                    theme.done
                } else {
                    theme.dim
                })
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("   best {}", best.label()),
            Style::default().fg(theme.dim),
        ),
    ]));

    let (week_done, week_target) =
        stats::week_progress(habit, &app.data.history(), app.today, app.week_start);
    let ratio = if week_target == 0 {
        0.0
    } else {
        f64::from(week_done) / f64::from(week_target)
    };
    let (filled, empty) = theme.bar(ratio, 10);
    lines.push(Line::from(vec![
        Span::styled(pad_right("this week", 11), Style::default().fg(theme.dim)),
        Span::styled(filled, Style::default().fg(theme.done)),
        Span::styled(empty, Style::default().fg(theme.border)),
        Span::styled(
            format!("  {week_done}/{week_target}"),
            Style::default().fg(theme.dim),
        ),
    ]));

    let completion = stats::completion(habit, &app.data.history(), app.today, app.week_start, 30);
    let (filled, empty) = theme.bar(f64::from(completion.percent()) / 100.0, 10);
    let completion_note = if completion.possible == 0 {
        "  no scheduled days yet".to_string()
    } else {
        format!(
            "  {}% ({} of {})",
            completion.percent(),
            completion.done,
            completion.possible
        )
    };
    lines.push(Line::from(vec![
        Span::styled(pad_right("last 30d", 11), Style::default().fg(theme.dim)),
        Span::styled(filled, Style::default().fg(theme.accent)),
        Span::styled(empty, Style::default().fg(theme.border)),
        Span::styled(completion_note, Style::default().fg(theme.dim)),
    ]));

    lines.push(Line::raw(""));
    lines.extend(heatmap_lines(app, habit, inner_width, &theme));
    lines.push(Line::raw(""));
    lines.push(legend(inner_width, &theme));

    let total = app.data.log.total_for(&habit.id);
    let cheats = app.data.cheats.used_in_month(&habit.id, app.today);
    let month = app.today.format("%b").to_string();
    let mut summary = vec![Span::styled(
        habit::format::plural(total, "check-in"),
        Style::default().fg(theme.dim),
    )];
    if cheats > 0 {
        let style = if cheats > 1 {
            Style::default().fg(theme.warn)
        } else {
            Style::default().fg(theme.cheat)
        };
        summary.push(Span::styled(
            format!("  ◆ {cheats} cheat this {month}"),
            style,
        ));
    }
    lines.push(Line::from(summary));

    let notes: Vec<(chrono::NaiveDate, &str)> = habit.notes_desc().collect();
    if !notes.is_empty() {
        lines.push(Line::raw(""));
        lines.push(Line::from(Span::styled(
            "notes",
            Style::default().fg(theme.dim).add_modifier(Modifier::BOLD),
        )));
        let max_lines = area.height.saturating_sub(2) as usize;
        let date_column = 11usize;
        let note_width = inner_width.saturating_sub(date_column).max(8);
        let mut shown = 0;
        for (date, text) in &notes {
            if lines.len() >= max_lines {
                break;
            }
            let chunks = wrap(text, note_width, 2);
            let room = max_lines.saturating_sub(lines.len());
            let take = chunks.len().min(room);
            for (index, chunk) in chunks.iter().take(take).enumerate() {
                if index == 0 {
                    lines.push(Line::from(vec![
                        Span::styled(
                            format!("{:<10} ", date.format("%Y-%m-%d")),
                            Style::default().fg(theme.dim),
                        ),
                        Span::styled(chunk.clone(), Style::default().fg(theme.fg)),
                    ]));
                } else {
                    lines.push(Line::from(vec![
                        Span::raw(" ".repeat(date_column)),
                        Span::styled(chunk.clone(), Style::default().fg(theme.fg)),
                    ]));
                }
            }
            if take > 0 {
                shown += 1;
            }
            if take < chunks.len() {
                break;
            }
        }
        let remaining = notes.len().saturating_sub(shown);
        if remaining > 0 && lines.len() < max_lines {
            lines.push(Line::from(Span::styled(
                format!("… {remaining} older"),
                Style::default().fg(theme.dim),
            )));
        }
    }

    frame.render_widget(Paragraph::new(lines).style(theme.base()).block(block), area);
}

fn legend(inner_width: usize, theme: &Theme) -> Line<'static> {
    let mut spans = vec![
        Span::styled("●", Style::default().fg(theme.done)),
        Span::styled(" done  ", Style::default().fg(theme.dim)),
        Span::styled("◆", Style::default().fg(theme.cheat)),
        Span::styled(" cheat  ", Style::default().fg(theme.dim)),
        Span::styled("░", Style::default().fg(theme.missed)),
        Span::styled(" missed", Style::default().fg(theme.dim)),
    ];
    if inner_width >= 33 {
        spans.push(Span::styled("  ◌", Style::default().fg(theme.warn)));
        spans.push(Span::styled(" open", Style::default().fg(theme.dim)));
    }
    if inner_width >= 44 {
        spans.push(Span::styled("  ·", Style::default().fg(theme.dim)));
        spans.push(Span::styled(" not due", Style::default().fg(theme.dim)));
    }
    Line::from(spans)
}

fn heatmap_lines(
    app: &App,
    habit: &Habit,
    inner_width: usize,
    theme: &Theme,
) -> Vec<Line<'static>> {
    let weeks = (inner_width.saturating_sub(6) as u32).clamp(4, 52);
    let grid = stats::heatmap(habit, &app.data.history(), app.today, app.week_start, weeks);
    let mut lines = Vec::with_capacity(7);
    // `grid` rows are offsets from the week start, so row 0 is the first
    // displayed weekday whatever the configured week start is.
    for (offset, cells) in grid.iter().enumerate() {
        let label = habit::model::Day::from_index(app.week_start.index() + offset);
        let mut spans = vec![Span::styled(
            format!("{:<4}", label.short()),
            Style::default().fg(theme.dim),
        )];
        for (_, status) in cells {
            let (glyph, style) = match status {
                Status::Done => ("█", Style::default().fg(theme.done)),
                Status::Cheat => ("◆", Style::default().fg(theme.cheat)),
                Status::Missed => ("░", Style::default().fg(theme.missed)),
                Status::Pending => ("◌", Style::default().fg(theme.warn)),
                Status::NotDue => ("·", Style::default().fg(theme.dim)),
                Status::Future => (" ", Style::default()),
            };
            spans.push(Span::styled(glyph, style));
        }
        lines.push(Line::from(spans));
    }
    lines
}

/// Greedy word wrap, capped at `max_lines` with an ellipsis on the last one.
fn wrap(text: &str, width: usize, max_lines: usize) -> Vec<String> {
    if width == 0 {
        return Vec::new();
    }
    let mut lines = Vec::new();
    let mut current = String::new();
    for word in text.split_whitespace() {
        if current.is_empty() {
            current.push_str(word);
            continue;
        }
        if current.chars().count() + 1 + word.chars().count() <= width {
            current.push(' ');
            current.push_str(word);
        } else {
            lines.push(std::mem::take(&mut current));
            current.push_str(word);
            if lines.len() == max_lines {
                break;
            }
        }
    }
    if lines.len() < max_lines && !current.is_empty() {
        lines.push(current);
    }
    if let Some(last) = lines.last_mut()
        && last.chars().count() > width
    {
        *last = truncate(last, width);
    }
    lines.truncate(max_lines);
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrapping_respects_width_and_line_limit() {
        let lines = wrap("one two three four five six", 9, 2);
        assert!(lines.len() <= 2);
        assert!(lines.iter().all(|line| line.chars().count() <= 9));
        assert_eq!(lines[0], "one two");
    }

    #[test]
    fn wrapping_keeps_short_notes_on_one_line() {
        assert_eq!(
            wrap("10 minutes is enough", 40, 3),
            vec!["10 minutes is enough"]
        );
    }
}
