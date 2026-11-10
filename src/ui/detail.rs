//! The right-hand pane: streaks, trend, heatmap, tags and the note.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph};

use crate::app::App;
use crate::format::{pad_right, truncate};
use crate::model::Habit;
use crate::stats::{self, Status};
use crate::theme::Theme;

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

    let mut subtitle = vec![Span::styled(
        habit.schedule.describe(),
        Style::default().fg(theme.dim),
    )];
    subtitle.push(Span::styled(
        format!(" · {}", habit.window_label()),
        Style::default().fg(theme.dim),
    ));
    lines.push(Line::from(subtitle));

    if !habit.tags.is_empty() {
        let mut chips = Vec::new();
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
    lines.push(trend_line(app, habit, inner_width, accent, &theme));
    lines.push(Line::raw(""));
    lines.extend(heatmap_lines(app, habit, inner_width, &theme));
    lines.push(Line::raw(""));
    lines.push(legend(inner_width, &theme));

    let total = app.data.log.total_for(&habit.id);
    let cheats = app.data.cheats.used_in_month(&habit.id, app.today);
    let month = app.today.format("%b").to_string();
    let mut summary = vec![Span::styled(
        crate::format::plural(total, "check-in"),
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

    if let Some(note) = &habit.note {
        lines.push(Line::raw(""));
        lines.push(Line::from(Span::styled(
            "note",
            Style::default().fg(theme.dim).add_modifier(Modifier::BOLD),
        )));
        for chunk in wrap(note, inner_width, 3) {
            lines.push(Line::from(Span::styled(
                chunk,
                Style::default().fg(theme.fg),
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

fn trend_line(
    app: &App,
    habit: &Habit,
    inner_width: usize,
    accent: ratatui::style::Color,
    theme: &Theme,
) -> Line<'static> {
    let width = inner_width.saturating_sub(8).clamp(5, 40);
    let series = stats::trend(
        habit,
        &app.data.history(),
        app.today,
        app.week_start,
        width as u32,
    );
    let mut spans = vec![Span::styled(
        pad_right("trend", 11),
        Style::default().fg(theme.dim),
    )];
    spans.push(Span::styled(
        series
            .iter()
            .map(|value| theme.spark(*value))
            .collect::<String>(),
        Style::default().fg(accent),
    ));
    let latest = series.last().copied().unwrap_or(0.0);
    spans.push(Span::styled(
        format!("  {:.0}%", latest * 100.0),
        Style::default().fg(if latest >= 0.8 {
            theme.done
        } else if latest >= 0.5 {
            theme.warn
        } else {
            theme.missed
        }),
    ));
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
        let label = crate::model::Day::from_index(app.week_start.index() + offset);
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
