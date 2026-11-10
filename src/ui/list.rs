//! The habit list pane, with Habits and Tags tabs.
//!
//! Tabs, tag rows and the back link are clickable; [`App::hits`] records where
//! they were drawn so the mouse handler can resolve a click without redoing
//! the layout.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, List, ListItem, Paragraph};

use crate::app::{App, ListRow, Tab};
use crate::format::{pad_right, width};
use crate::model::Habit;
use crate::stats::{self, Status};
use crate::theme::Theme;

const DOTS: u32 = 7;
const DOTS_WIDTH: usize = DOTS as usize;

pub fn draw(frame: &mut Frame, area: Rect, app: &mut App) {
    let theme = app.theme;
    let block = Block::bordered()
        .border_style(Style::default().fg(theme.border))
        .style(theme.base());
    let inner = block.inner(area);
    frame.render_widget(block, area);
    app.hits.tabs.clear();
    app.hits.list = None;
    app.hits.back = None;
    if inner.height == 0 {
        return;
    }

    let tab_row = Rect {
        x: inner.x,
        y: inner.y,
        width: inner.width,
        height: 1,
    };
    draw_tabs(frame, tab_row, app);

    let content = Rect {
        x: inner.x,
        y: inner.y + 1,
        width: inner.width,
        height: inner.height.saturating_sub(1),
    };
    if content.height == 0 {
        return;
    }

    match (app.tab, app.tag_focus.clone()) {
        (Tab::Tags, None) => {
            app.hits.list = Some(content);
            tag_list(frame, content, app);
        }
        (Tab::Tags, Some(tag)) => tag_habits(frame, content, app, &tag),
        _ => {
            app.hits.list = Some(content);
            habit_list(frame, content, app);
        }
    }
}

fn draw_tabs(frame: &mut Frame, area: Rect, app: &mut App) {
    let theme = app.theme;
    let active = Style::default()
        .fg(theme.accent)
        .add_modifier(Modifier::BOLD | Modifier::REVERSED);
    let inactive = Style::default().fg(theme.dim);

    let habits_count = app
        .data
        .habits
        .iter()
        .filter(|habit| !habit.archived && !habit.is_finished(app.today))
        .count();
    let tags_count = app.tags().len();

    let mut spans = Vec::new();
    let mut x = area.x;
    for (tab, label) in [
        (Tab::Habits, format!(" Habits {habits_count} ")),
        (Tab::Tags, format!(" Tags {tags_count} ")),
    ] {
        let label_width = width(&label) as u16;
        if x.saturating_add(label_width) > area.x.saturating_add(area.width) {
            break;
        }
        app.hits.tabs.push((
            tab,
            Rect {
                x,
                y: area.y,
                width: label_width,
                height: 1,
            },
        ));
        spans.push(Span::styled(
            label,
            if app.tab == tab { active } else { inactive },
        ));
        spans.push(Span::raw(" "));
        x = x.saturating_add(label_width + 1);
    }

    let filter = if app.filtering {
        format!("/{}▏", app.filter)
    } else if !app.filter.is_empty() {
        format!("/{}", app.filter)
    } else {
        String::new()
    };
    if !filter.is_empty() {
        let used = (x - area.x) as usize;
        let room = (area.width as usize).saturating_sub(used);
        let filter = crate::format::truncate(&filter, room);
        let padding = room.saturating_sub(width(&filter));
        spans.push(Span::raw(" ".repeat(padding)));
        spans.push(Span::styled(filter, Style::default().fg(theme.warn)));
    }

    frame.render_widget(Paragraph::new(Line::from(spans)).style(theme.base()), area);
}

fn habit_list(frame: &mut Frame, area: Rect, app: &mut App) {
    let theme = app.theme;
    let rows = app.view_rows();
    // The highlight symbol takes two cells from every rendered row.
    let row_width = area.width.saturating_sub(2) as usize;
    let mut items: Vec<ListItem> = Vec::with_capacity(rows.len().max(1));

    if rows.is_empty() {
        let message = if app.data.habits.is_empty() {
            "No habits yet — press a to add the first one"
        } else if app.tag_focus.is_some() {
            "Nothing in this tag matches the filter"
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
        for row in &rows {
            match row {
                ListRow::Archive => items.push(ListItem::new(Line::from(Span::styled(
                    " Archive",
                    Style::default().fg(theme.dim).add_modifier(Modifier::BOLD),
                )))),
                ListRow::Habit(index) => items.push(ListItem::new(row_line(
                    &app.data.habits[*index],
                    app,
                    row_width,
                ))),
            }
        }
    }

    let selected = app
        .selected()
        .and_then(|index| rows.iter().position(|row| *row == ListRow::Habit(index)));

    let list = List::new(items)
        .style(theme.base())
        .highlight_style(highlight(theme))
        .highlight_symbol("▸ ");

    app.list_state.select(selected);
    frame.render_stateful_widget(list, area, &mut app.list_state);
}

fn tag_habits(frame: &mut Frame, area: Rect, app: &mut App, tag: &str) {
    let theme = app.theme;
    let back = Rect {
        x: area.x,
        y: area.y,
        width: area.width,
        height: 1,
    };
    app.hits.back = Some(back);

    let count = app
        .tags()
        .iter()
        .find(|(name, _)| name == tag)
        .map(|(_, count)| *count)
        .unwrap_or(0);
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                " ‹ all tags",
                Style::default()
                    .fg(theme.accent)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("   ", Style::default().fg(theme.dim)),
            Span::styled(format!("#{tag}"), Style::default().fg(theme.accent)),
            Span::styled(format!(" · {count} active"), Style::default().fg(theme.dim)),
        ]))
        .style(theme.base()),
        back,
    );

    let content = Rect {
        x: area.x,
        y: area.y + 1,
        width: area.width,
        height: area.height.saturating_sub(1),
    };
    if content.height == 0 {
        return;
    }
    app.hits.list = Some(content);
    habit_list(frame, content, app);
}

fn tag_list(frame: &mut Frame, area: Rect, app: &mut App) {
    let theme = app.theme;
    let tags = app.tags();
    let row_width = area.width.saturating_sub(2) as usize;
    let mut items = Vec::with_capacity(tags.len().max(1));

    if tags.is_empty() {
        items.push(ListItem::new(Line::from(Span::styled(
            " No tags yet — add them in the add/edit dialog",
            Style::default().fg(theme.dim),
        ))));
    } else {
        for (tag, count) in &tags {
            let label = format!("#{tag}");
            let count_text = count.to_string();
            let padding = row_width
                .saturating_sub(width(&label) + width(&count_text))
                .max(1);
            let label_style = if *count == 0 {
                Style::default().fg(theme.dim)
            } else {
                Style::default().fg(theme.fg)
            };
            items.push(ListItem::new(Line::from(vec![
                Span::styled(label, label_style),
                Span::raw(" ".repeat(padding)),
                Span::styled(
                    count_text,
                    if *count == 0 {
                        Style::default().fg(theme.dim)
                    } else {
                        Style::default().fg(theme.done)
                    },
                ),
            ])));
        }
    }

    let list = List::new(items)
        .style(theme.base())
        .highlight_style(highlight(theme))
        .highlight_symbol("▸ ");

    app.list_state.select(if tags.is_empty() {
        None
    } else {
        Some(app.tag_cursor.min(tags.len() - 1))
    });
    frame.render_stateful_widget(list, area, &mut app.list_state);
}

fn highlight(theme: Theme) -> Style {
    match theme.surface {
        Some(surface) => Style::default()
            .bg(surface)
            .fg(theme.fg)
            .add_modifier(Modifier::BOLD),
        None => Style::default().add_modifier(Modifier::BOLD | Modifier::REVERSED),
    }
}

fn row_line(habit: &Habit, app: &App, inner_width: usize) -> Line<'static> {
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
    // A habit with an end date trades its schedule label for a countdown, so
    // a closing window is visible without opening the detail pane.
    let (schedule, schedule_style) = match habit.ends {
        Some(ends) => {
            let days = ends.signed_duration_since(app.today).num_days();
            let text = if days < 0 {
                "ended".to_string()
            } else if days == 0 {
                "last day".to_string()
            } else {
                format!("{days}d left")
            };
            let style = if closed {
                Style::default().fg(theme.dim)
            } else if (0..=3).contains(&days) {
                Style::default().fg(theme.warn)
            } else {
                Style::default().fg(theme.dim)
            };
            (text, style)
        }
        None => (schedule, Style::default().fg(theme.dim)),
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
            schedule_style,
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
