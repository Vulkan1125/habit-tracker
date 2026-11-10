//! The habit list pane.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, List, ListItem};

use crate::app::App;
use crate::format::pad_right;
use crate::model::Habit;
use crate::stats::{self, Status};

const DOTS: u32 = 7;
const DOTS_WIDTH: usize = DOTS as usize;

pub fn draw(frame: &mut Frame, area: Rect, app: &mut App) {
    let theme = app.theme;
    let indices = app.visible_indices();
    // The border and the 2-column highlight symbol both eat into the row width.
    let inner_width = area.width.saturating_sub(4) as usize;

    let title = if app.filtering {
        format!(" Habits · /{}▏ ", app.filter)
    } else if !app.filter.is_empty() {
        format!(" Habits · /{} ", app.filter)
    } else {
        format!(" Habits · {} ", indices.len())
    };
    let title_style = if app.filter.is_empty() {
        Style::default().fg(theme.dim)
    } else {
        Style::default().fg(theme.accent)
    };

    let mut items: Vec<ListItem> = Vec::with_capacity(indices.len().max(1));
    if indices.is_empty() {
        let message = if app.data.habits.is_empty() {
            "No habits yet — press a to add the first one"
        } else if app.filter.is_empty() {
            "Nothing to show"
        } else {
            "Nothing matches that filter"
        };
        items.push(ListItem::new(Line::from(Span::styled(
            format!(" {message}"),
            Style::default().fg(theme.dim),
        ))));
    } else {
        for &index in &indices {
            items.push(ListItem::new(row(
                &app.data.habits[index],
                app,
                inner_width,
            )));
        }
    }

    let highlight = match theme.surface {
        Some(surface) => Style::default()
            .bg(surface)
            .fg(theme.fg)
            .add_modifier(Modifier::BOLD),
        None => Style::default().add_modifier(Modifier::BOLD),
    };

    let list = List::new(items)
        .block(
            Block::bordered()
                .title(Line::from(Span::styled(title, title_style)))
                .border_style(Style::default().fg(theme.border))
                .style(theme.base()),
        )
        .style(theme.base())
        .highlight_style(highlight)
        .highlight_symbol("▸ ");

    app.list_state.select(if indices.is_empty() {
        None
    } else {
        Some(app.cursor.min(indices.len() - 1))
    });
    frame.render_stateful_widget(list, area, &mut app.list_state);
}

fn row(habit: &Habit, app: &App, inner_width: usize) -> Line<'static> {
    let theme = app.theme;
    let date = app.view_date;
    let done = app.data.log.is_done(date, &habit.id);
    let excused = app.data.cheats.is_excused(date, &habit.id);
    let due = habit.schedule.is_due(date) && habit.is_active_on(date);
    let closed = habit.archived || habit.is_finished(app.today);

    let (marker, marker_style) = if closed {
        ("·", Style::default().fg(theme.dim))
    } else if done {
        ("●", Style::default().fg(theme.done))
    } else if excused {
        ("◆", Style::default().fg(theme.cheat))
    } else if due {
        ("○", Style::default().fg(theme.warn))
    } else {
        ("·", Style::default().fg(theme.dim))
    };

    let schedule =
        match stats::weekly_progress(habit, &app.data.history(), app.today, app.week_start) {
            Some((done, target)) => format!("{done}/{target} wk"),
            None => habit.schedule.describe(),
        };
    let show_schedule = inner_width >= 34;
    let schedule_width = if show_schedule { 13 } else { 0 };
    let name_width = inner_width
        .saturating_sub(2 + schedule_width + DOTS_WIDTH)
        .max(4);

    let name_style = if closed {
        Style::default().fg(theme.dim)
    } else {
        Style::default().fg(theme.fg)
    };

    let mut spans = vec![
        Span::styled(marker, marker_style),
        Span::raw(" "),
        Span::styled(pad_right(&habit.name, name_width), name_style),
    ];

    if show_schedule {
        spans.push(Span::styled(
            pad_right(&schedule, schedule_width),
            Style::default().fg(theme.dim),
        ));
    }

    for (_, status) in stats::recent(habit, &app.data.history(), date, DOTS) {
        let (glyph, style) = match status {
            Status::Done => ("●", Style::default().fg(theme.done)),
            Status::Cheat => ("◆", Style::default().fg(theme.cheat)),
            Status::Missed => ("○", Style::default().fg(theme.missed)),
            Status::Pending => ("◌", Style::default().fg(theme.warn)),
            Status::NotDue | Status::Future => ("·", Style::default().fg(theme.dim)),
        };
        spans.push(Span::styled(glyph, style));
    }

    Line::from(spans)
}
