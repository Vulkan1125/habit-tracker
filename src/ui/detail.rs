//! The right-hand pane: streaks, this week, 30-day rate and a heatmap.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph};

use crate::app::App;
use crate::format::{pad_right, truncate};
use crate::stats::{self, Status};
use crate::theme::Theme;

pub fn draw(frame: &mut Frame, area: Rect, app: &App) {
    let theme = app.theme;
    let block = Block::bordered()
        .title(" Details ")
        .border_style(Style::default().fg(theme.border));

    let Some(habit) = app.selected_habit() else {
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                " Select a habit to see its history ",
                Style::default().fg(theme.dim),
            )))
            .block(block),
            area,
        );
        return;
    };

    let inner_width = area.width.saturating_sub(2) as usize;
    let mut lines: Vec<Line> = Vec::new();

    let mut title = vec![Span::styled(
        truncate(&habit.name, inner_width.saturating_sub(10)),
        Style::default()
            .fg(theme.accent)
            .add_modifier(Modifier::BOLD),
    )];
    if habit.archived {
        title.push(Span::styled("  archived", Style::default().fg(theme.dim)));
    }
    lines.push(Line::from(title));

    let mut subtitle = habit.schedule.describe();
    if !habit.archived {
        subtitle.push_str(" · ");
    }
    subtitle.push_str(&format!("since {}", habit.created));
    lines.push(Line::from(Span::styled(
        truncate(&subtitle, inner_width),
        Style::default().fg(theme.dim),
    )));
    lines.push(Line::raw(""));

    let streak = stats::current_streak(habit, &app.data.log, app.today, app.week_start);
    let best = stats::longest_streak(habit, &app.data.log, app.today, app.week_start);
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
        stats::week_progress(habit, &app.data.log, app.today, app.week_start);
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

    let completion = stats::completion(habit, &app.data.log, app.today, app.week_start, 30);
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
    lines.push(Line::from(vec![
        Span::styled("█", Style::default().fg(theme.done)),
        Span::styled(" done  ", Style::default().fg(theme.dim)),
        Span::styled("░", Style::default().fg(theme.missed)),
        Span::styled(" missed  ", Style::default().fg(theme.dim)),
        Span::styled("·", Style::default().fg(theme.dim)),
        Span::styled(" not due", Style::default().fg(theme.dim)),
    ]));

    let total = app.data.log.total_for(&habit.id);
    lines.push(Line::from(Span::styled(
        crate::format::plural(total, "check-in"),
        Style::default().fg(theme.dim),
    )));

    if let Some(note) = &habit.note {
        lines.push(Line::raw(""));
        for chunk in note.lines().take(3) {
            lines.push(Line::from(Span::styled(
                truncate(chunk, inner_width),
                Style::default().fg(theme.warn),
            )));
        }
    }

    frame.render_widget(Paragraph::new(lines).block(block), area);
}

fn heatmap_lines(
    app: &App,
    habit: &crate::model::Habit,
    inner_width: usize,
    theme: &Theme,
) -> Vec<Line<'static>> {
    let weeks = (inner_width.saturating_sub(6) as u32).clamp(4, 26);
    let grid = stats::heatmap(habit, &app.data.log, app.today, app.week_start, weeks);
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
                Status::Missed => ("░", Style::default().fg(theme.missed)),
                Status::NotDue => ("·", Style::default().fg(theme.dim)),
                Status::Future => (" ", Style::default()),
            };
            spans.push(Span::styled(glyph, style));
        }
        lines.push(Line::from(spans));
    }
    lines
}
