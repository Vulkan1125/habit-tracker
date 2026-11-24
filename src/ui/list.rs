//! The left pane, with Habits, Tags, Notes and Archive tabs.
//!
//! Tabs, tag rows, note rows and the back link are clickable; [`App::hits`]
//! records where they were drawn so the mouse handler can resolve a click
//! without redoing the layout.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, List, ListItem, Paragraph};

use crate::app::{App, ListRow, NoteKind, Tab};
use habit::format::{pad_right, width, wrap};
use habit::model::Habit;
use habit::stats::{self, Status};
use habit::theme::Theme;

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
    app.hits.note_rows.clear();
    app.hits.day_cells.clear();
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
        (Tab::Notes, _) => {
            app.hits.list = Some(content);
            note_list(frame, content, app);
        }
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
        .filter(|habit| {
            !habit.archived && !habit.is_finished(app.today) && habit.created <= app.today
        })
        .count();
    let tags_count = app.tags().len();
    let notes_count = match app.note_habit {
        Some(index) => app
            .data
            .habits
            .get(index)
            .map_or(0, |habit| habit.notes.len()),
        None => app.data.habits.iter().map(|habit| habit.notes.len()).sum(),
    };
    let archive_count = app
        .data
        .habits
        .iter()
        .filter(|habit| habit.archived || habit.is_finished(app.today))
        .count();

    let mut spans = Vec::new();
    let mut x = area.x;
    for (tab, label) in [
        (Tab::Habits, format!(" Habits {habits_count} ")),
        (Tab::Tags, format!(" Tags {tags_count} ")),
        (Tab::Notes, format!(" Notes {notes_count} ")),
        (Tab::Archive, format!(" Archive {archive_count} ")),
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
        x = x.saturating_add(label_width);
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
        let filter = habit::format::truncate(&filter, room);
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
            "No habits yet: press a to add the first one"
        } else if app.tab == Tab::Archive {
            "Nothing archived yet: press d on a habit"
        } else if app.tab == Tab::Habits {
            "Nothing running: see the Archive tab"
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
        let show_counts = rows.iter().any(
            |row| matches!(row, ListRow::Habit(index) if app.data.habits[*index].per_day() > 1),
        );
        for row in &rows {
            match row {
                ListRow::Heading(label) => items.push(ListItem::new(Line::from(Span::styled(
                    format!(" {label}"),
                    Style::default().fg(theme.dim).add_modifier(Modifier::BOLD),
                )))),
                ListRow::Spacer => items.push(ListItem::new(Line::from(""))),
                ListRow::Archive => items.push(ListItem::new(Line::from(Span::styled(
                    " Archive",
                    Style::default().fg(theme.dim).add_modifier(Modifier::BOLD),
                )))),
                ListRow::Habit(index) => items.push(ListItem::new(row_line(
                    &app.data.habits[*index],
                    app,
                    row_width,
                    show_counts,
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

    // Record the seven day dots of every visible habit row. A click on one of
    // them backfills that day without the user having to move the selected day
    // first.
    let offset = app.list_state.offset();
    let dots_x = area.x + area.width.saturating_sub(DOTS_WIDTH as u16);
    for (row_index, row) in rows.iter().enumerate() {
        if row_index < offset {
            continue;
        }
        let y = area.y + (row_index - offset) as u16;
        if y >= area.y + area.height {
            break;
        }
        let ListRow::Habit(index) = row else {
            continue;
        };
        let habit = &app.data.habits[*index];
        for (column, (day, _)) in stats::recent(habit, &app.data.history(), app.view_date, DOTS)
            .into_iter()
            .enumerate()
        {
            app.hits.day_cells.push((
                *index,
                day,
                Rect {
                    x: dots_x + column as u16,
                    y,
                    width: 1,
                    height: 1,
                },
            ));
        }
    }
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

fn note_list(frame: &mut Frame, area: Rect, app: &mut App) {
    let theme = app.theme;
    let notes = app.notes();
    let habit = app.note_habit.and_then(|index| app.data.habits.get(index));
    let header = Rect {
        x: area.x,
        y: area.y,
        width: area.width,
        height: 1,
    };
    let header_line = match habit {
        Some(habit) => Line::from(vec![
            Span::styled(
                format!(" {}", habit.name),
                Style::default()
                    .fg(theme.habit_accent(&habit.id))
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!(" · {} notes", habit.notes.len()),
                Style::default().fg(theme.dim),
            ),
        ]),
        None => Line::from(Span::styled(
            " No habit selected",
            Style::default().fg(theme.dim),
        )),
    };
    frame.render_widget(Paragraph::new(header_line).style(theme.base()), header);

    let list_area = Rect {
        x: area.x,
        y: area.y + 1,
        width: area.width,
        height: area.height.saturating_sub(1),
    };
    app.hits.list = Some(list_area);
    if list_area.height == 0 {
        return;
    }
    let row_width = list_area.width.saturating_sub(2) as usize;
    let mut items: Vec<ListItem> = Vec::with_capacity(notes.len().max(1));
    let mut heights: Vec<(usize, u16)> = Vec::with_capacity(notes.len());

    if notes.is_empty() {
        let message = if habit.is_none() {
            "Go to the Habits tab and pick a habit"
        } else if habit.is_some_and(|habit| habit.notes.is_empty()) {
            "No notes yet: press n to add one"
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
        for (index, date) in &notes {
            let habit = &app.data.habits[*index];
            let (marker, note_style) = match app.note_kind(habit, *date) {
                NoteKind::Cheat => ("× ", Style::default().fg(theme.cheat)),
                NoteKind::Missed => ("× ", Style::default().fg(theme.missed)),
                NoteKind::Normal => ("", Style::default().fg(theme.fg)),
            };
            // The date sits in its own column and the note starts beside it, so
            // wrapped lines stay aligned under the first one instead of turning
            // each note into a date line followed by a paragraph.
            let date_column = 11usize;
            let text_column = date_column + width(marker);
            let text_width = row_width.saturating_sub(text_column).max(8);
            let chunks = wrap(habit.note_on(*date).unwrap_or(""), text_width, 2);
            let mut lines: Vec<Line> = Vec::with_capacity(chunks.len().max(1));
            for (chunk_index, chunk) in chunks.into_iter().enumerate() {
                if chunk_index == 0 {
                    lines.push(Line::from(vec![
                        Span::styled(
                            format!("{:<10} ", date.format("%Y-%m-%d")),
                            Style::default().fg(theme.dim),
                        ),
                        Span::styled(marker, note_style),
                        Span::styled(chunk, note_style),
                    ]));
                } else {
                    lines.push(Line::from(vec![
                        Span::raw(" ".repeat(text_column)),
                        Span::styled(chunk, note_style),
                    ]));
                }
            }
            if lines.is_empty() {
                lines.push(Line::from(Span::styled(
                    format!("{:<10} ", date.format("%Y-%m-%d")),
                    Style::default().fg(theme.dim),
                )));
            }
            heights.push((items.len(), lines.len() as u16));
            items.push(ListItem::new(lines));
        }
    }

    let selected = if notes.is_empty() {
        None
    } else {
        Some(app.note_cursor.min(notes.len() - 1))
    };

    let list = List::new(items)
        .style(theme.base())
        .highlight_style(highlight(theme))
        .highlight_symbol("▸ ");

    app.list_state.select(selected);
    frame.render_stateful_widget(list, list_area, &mut app.list_state);

    // A note can span several visual lines, so record where each one landed
    // after the list has scrolled itself.
    let offset = app.list_state.offset();
    let bottom = list_area.y.saturating_add(list_area.height);
    let mut y = list_area.y;
    for (index, height) in heights {
        if index < offset {
            continue;
        }
        if y >= bottom {
            break;
        }
        let visible = height.min(bottom - y);
        app.hits.note_rows.push((
            index,
            Rect {
                x: list_area.x,
                y,
                width: list_area.width,
                height: visible,
            },
        ));
        y = y.saturating_add(height);
    }
}

fn tag_list(frame: &mut Frame, area: Rect, app: &mut App) {
    let theme = app.theme;
    let tags = app.tags();
    let row_width = area.width.saturating_sub(2) as usize;
    let mut items = Vec::with_capacity(tags.len().max(1));

    if tags.is_empty() {
        items.push(ListItem::new(Line::from(Span::styled(
            " No tags yet: add them in the add/edit dialog",
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
        // Palettes without a background are inverted instead, and the colour has
        // to be pinned here too: otherwise every span keeps its own colour and
        // the status dots show up as blocks of green, amber and red on the
        // selected row, unlike every other theme.
        None => Style::default()
            .fg(theme.fg)
            .add_modifier(Modifier::BOLD | Modifier::REVERSED),
    }
}

fn row_line(habit: &Habit, app: &App, inner_width: usize, show_counts: bool) -> Line<'static> {
    let theme = app.theme;
    let date = app.view_date;
    let count = app.data.log.count_on(date, &habit.id);
    let target = habit.per_day();
    let excused = app.data.cheats.is_excused(date, &habit.id);
    let due = habit.schedule.is_due(date) && habit.is_active_on(date);
    let closed = habit.archived || habit.is_finished(app.today);

    let (marker, marker_style) = if closed {
        ("·", Style::default().fg(theme.dim))
    } else if count >= target {
        ("●", Style::default().fg(theme.done))
    } else if count > 0 {
        ("◍", Style::default().fg(theme.warn))
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
    let count_width = if show_counts { 5 } else { 0 };
    let name_width = inner_width
        .saturating_sub(2 + count_width + schedule_width + DOTS_WIDTH)
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

    if show_counts {
        let text = if target > 1 {
            format!(" {count}/{target}")
        } else {
            String::new()
        };
        let style = if target > 1 && count >= target {
            Style::default().fg(theme.done)
        } else {
            Style::default().fg(theme.warn)
        };
        spans.push(Span::styled(pad_right(&text, count_width), style));
    }

    if show_schedule {
        spans.push(Span::styled(
            pad_right(&schedule, schedule_width),
            schedule_style,
        ));
    }

    for (_, status) in stats::recent(habit, &app.data.history(), date, DOTS) {
        let (glyph, style) = match status {
            Status::Done => ("●", Style::default().fg(theme.done)),
            Status::Partial => ("◍", Style::default().fg(theme.warn)),
            Status::Cheat => ("◆", Style::default().fg(theme.cheat)),
            Status::Missed => ("○", Style::default().fg(theme.missed)),
            Status::Pending => ("◌", Style::default().fg(theme.warn)),
            Status::NotDue | Status::Future => ("·", Style::default().fg(theme.dim)),
        };
        spans.push(Span::styled(glyph, style));
    }

    Line::from(spans)
}
