//! Frame composition: header, panes, footer and modal overlays.

pub mod detail;
pub mod form;
pub mod help;
pub mod list;

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Paragraph};

use crate::app::{App, Mode};
use crate::format::{truncate, width};
use crate::theme::Theme;

pub fn draw(frame: &mut Frame, app: &mut App) {
    let area = frame.area();
    let theme = app.theme;

    if let Some(bg) = theme.bg {
        frame.render_widget(Block::default().style(Style::default().bg(bg)), area);
    }

    let footer_height = if app.show_footer { 1 } else { 0 };
    let rows = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(3),
        Constraint::Length(footer_height),
    ])
    .split(area);

    header(frame, rows[0], app);

    if area.width >= 72 {
        let columns = Layout::horizontal([Constraint::Percentage(46), Constraint::Percentage(54)])
            .split(rows[1]);
        list::draw(frame, columns[0], app);
        detail::draw(frame, columns[1], app);
    } else {
        list::draw(frame, rows[1], app);
    }

    if app.show_footer {
        footer(frame, rows[2], app);
    }

    match &app.mode {
        Mode::Normal => {}
        Mode::Help => help::draw(frame, area, &theme),
        Mode::Form(form) => form.draw(frame, area, &theme),
        Mode::Confirm(confirm) => confirm_overlay(frame, area, &theme, &confirm.prompt),
        Mode::Note(entry) => note_overlay(frame, area, &theme, entry),
    }
}

fn header(frame: &mut Frame, area: Rect, app: &App) {
    let theme = app.theme;
    let date = app.today.format("%a %-d %b %Y").to_string();
    let done = app.data.done_today(app.today);
    let due = app.data.due_today(app.today);
    let ratio = if due == 0 {
        0.0
    } else {
        f64::from(done as u32) / f64::from(due as u32)
    };
    let compact = area.width < 46;
    let (filled, empty) = if compact {
        (String::new(), String::new())
    } else {
        theme.bar(ratio, 6)
    };

    let viewing = if app.view_date == app.today {
        String::new()
    } else {
        format!("· {} ", app.view_date.format("%a %-d %b"))
    };
    let right_text = if compact {
        format!(" {done}/{due} ")
    } else {
        format!(" {done}/{due} today ")
    };

    let left_width = 10 + width(&date) + width(&viewing);
    let right_width = width(&filled) + width(&empty) + width(&right_text);
    let padding = (area.width as usize).saturating_sub(left_width + right_width);

    let right_style = if due > 0 && done >= due {
        Style::default().fg(theme.done).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme.dim)
    };

    let line = Line::from(vec![
        Span::styled(
            " habit",
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(format!("  {date}"), Style::default().fg(theme.dim)),
        Span::styled(format!("  {viewing}"), Style::default().fg(theme.warn)),
        Span::raw(" ".repeat(padding)),
        Span::styled(filled, Style::default().fg(theme.done)),
        Span::styled(empty, Style::default().fg(theme.border)),
        Span::styled(right_text, right_style),
    ]);
    frame.render_widget(Paragraph::new(line).style(theme.base()), area);
}

fn footer(frame: &mut Frame, area: Rect, app: &App) {
    let theme = app.theme;
    let (text, style) = match &app.flash {
        Some(flash) if flash.error => (
            flash.text.clone(),
            Style::default()
                .fg(theme.missed)
                .add_modifier(Modifier::BOLD),
        ),
        Some(flash) if flash.warn => (
            flash.text.clone(),
            Style::default().fg(theme.warn).add_modifier(Modifier::BOLD),
        ),
        Some(flash) => (
            flash.text.clone(),
            Style::default().fg(theme.done).add_modifier(Modifier::BOLD),
        ),
        None => (
            hint(app, area.width as usize),
            Style::default().fg(theme.dim),
        ),
    };
    let max = area.width as usize;
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(truncate(&text, max), style))).style(theme.base()),
        area,
    );
}

fn hint(app: &App, max_width: usize) -> String {
    match &app.mode {
        Mode::Help => "press any key to close help".to_string(),
        Mode::Form(_) => "tab next · enter save · esc cancel".to_string(),
        Mode::Confirm(_) => "y confirm · n cancel".to_string(),
        Mode::Note(_) => "enter save · esc cancel · empty clears the note".to_string(),
        Mode::Normal if app.filtering => {
            "type to filter · #tag for tags · enter keep · esc clear".to_string()
        }
        Mode::Normal => {
            let full = "space toggle · n note · c cheat · [ ] day · t today · tab tags · a add · e edit · d archive · x delete · u undo · / filter · b bar · T theme · m mode · ? help · q quit";
            if width(full) <= max_width {
                return full.to_string();
            }
            let medium = "space toggle · n note · tab tags · b bar · / filter · ? help · q quit";
            if width(medium) <= max_width {
                return medium.to_string();
            }
            let tiny = "space · n · tab · b · / · ? · q";
            if width(tiny) <= max_width {
                return tiny.to_string();
            }
            "space · n · b · / · ? · q".to_string()
        }
    }
}

fn note_overlay(frame: &mut Frame, area: Rect, theme: &Theme, entry: &crate::app::NoteEntry) {
    let width = area.width.saturating_sub(4).clamp(34, 68);
    let height = 6.min(area.height);
    let popup = Rect {
        x: area.x + area.width.saturating_sub(width) / 2,
        y: area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    };
    frame.render_widget(Clear, popup);
    let title = format!(
        " Note · {} · {} ",
        truncate(&entry.habit_name, 24),
        entry.date.format("%a %-d %b")
    );
    let block = Block::bordered()
        .title(title)
        .border_style(Style::default().fg(theme.accent))
        .style(theme.base());
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    let mut text: Vec<char> = entry.input.text().chars().collect();
    let cursor = entry.input.cursor().min(text.len());
    text.insert(cursor, '▏');
    let input: String = text.into_iter().collect();
    let room = inner.width as usize;
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(Span::styled(
                truncate(&input, room),
                Style::default().fg(theme.fg),
            )),
            Line::raw(""),
            Line::from(Span::styled(
                truncate("enter save · esc cancel · empty removes", room),
                Style::default().fg(theme.dim),
            )),
        ])
        .style(theme.base()),
        inner,
    );
}

fn confirm_overlay(frame: &mut Frame, area: Rect, theme: &Theme, prompt: &str) {
    let width = area.width.saturating_sub(4).clamp(30, 60);
    let height = 5.min(area.height);
    let popup = Rect {
        x: area.x + area.width.saturating_sub(width) / 2,
        y: area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    };
    frame.render_widget(Clear, popup);
    let block = Block::bordered()
        .title(" Confirm ")
        .border_style(Style::default().fg(theme.missed))
        .style(theme.base());
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    frame.render_widget(
        Paragraph::new(vec![
            Line::from(Span::styled(
                truncate(prompt, inner.width as usize),
                Style::default(),
            )),
            Line::raw(""),
            Line::from(vec![
                Span::styled(
                    "y",
                    Style::default()
                        .fg(theme.missed)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(" delete   ", Style::default().fg(theme.dim)),
                Span::styled(
                    "n",
                    Style::default()
                        .fg(theme.accent)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(" keep", Style::default().fg(theme.dim)),
            ]),
        ])
        .style(theme.base()),
        inner,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::Tab;
    use crate::config::Config;
    use crate::model::{Data, Schedule};
    use crate::store::Store;
    use chrono::NaiveDate;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    fn date() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 11, 10).unwrap()
    }

    fn app_with(content: &str) -> App {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::resolve(Some(dir.path().join("data.json"))).unwrap();
        let mut data = Data::default();
        if !content.is_empty() {
            data.add_habit(content, Schedule::Daily, date()).unwrap();
        }
        App::new(store, data, date(), &Config::default())
    }

    fn render(app: &mut App, width: u16, height: u16) -> String {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| draw(frame, app)).unwrap();
        terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect()
    }

    #[test]
    fn renders_list_detail_and_header() {
        let mut app = app_with("Gym");
        let screen = render(&mut app, 90, 24);
        assert!(screen.contains("habit"), "{screen}");
        assert!(screen.contains("Gym"));
        assert!(screen.contains("Details"));
        assert!(screen.contains("streak"));
        assert!(screen.contains("today"));
        assert!(screen.contains("Habits"), "{screen}");
        assert!(screen.contains("Tags"), "{screen}");
    }

    #[test]
    fn tags_tab_lists_counts_and_opens_archived_habits() {
        let mut app = app_with("Gym");
        app.data.habits[0].tags = vec!["health".into()];
        let index = app
            .data
            .add_habit("Old run", Schedule::Daily, date())
            .unwrap();
        app.data.habits[index].tags = vec!["health".into()];
        app.data.habits[index].archived = true;

        app.select_tab(Tab::Tags);
        let screen = render(&mut app, 90, 26);
        assert!(screen.contains("#health"), "{screen}");
        assert!(screen.contains("Tags 1"), "{screen}");
        assert!(screen.contains("1"), "{screen}");

        app.tag_cursor = 0;
        app.open_focused_tag();
        let screen = render(&mut app, 90, 26);
        assert!(screen.contains("all tags"), "{screen}");
        assert!(screen.contains("Archive"), "{screen}");
        assert!(screen.contains("Old run"), "{screen}");
    }

    #[test]
    fn notes_are_listed_with_dates_and_the_trend_is_gone() {
        let mut app = app_with("Gym");
        app.data.habits[0].set_note(date(), "felt strong");
        let screen = render(&mut app, 90, 30);
        assert!(screen.contains("notes"), "{screen}");
        assert!(screen.contains("2026-11-10"), "{screen}");
        assert!(screen.contains("felt strong"), "{screen}");
        assert!(!screen.contains("trend"), "{screen}");
    }

    #[test]
    fn the_note_overlay_renders() {
        let mut app = app_with("Gym");
        app.handle_key(crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char('n'),
            crossterm::event::KeyModifiers::NONE,
        ))
        .unwrap();
        let screen = render(&mut app, 90, 30);
        assert!(screen.contains("Note · Gym"), "{screen}");
        assert!(screen.contains("enter save"), "{screen}");
    }

    #[test]
    fn empty_state_is_friendly() {
        let mut app = app_with("");
        let screen = render(&mut app, 90, 24);
        assert!(screen.contains("No habits yet"), "{screen}");
    }

    #[test]
    fn narrow_terminal_hides_the_detail_pane() {
        let mut app = app_with("Gym");
        let screen = render(&mut app, 60, 20);
        assert!(screen.contains("Gym"));
        assert!(!screen.contains("Details"));
    }

    #[test]
    fn help_overlay_renders() {
        let mut app = app_with("Gym");
        app.mode = Mode::Help;
        let screen = render(&mut app, 90, 30);
        assert!(screen.contains("Keys"), "{screen}");
        assert!(screen.contains("cheat"), "{screen}");
    }

    #[test]
    fn an_end_date_shows_the_remaining_days_in_the_list() {
        let mut app = app_with("Gym");
        app.data.habits[0].ends = Some(date() + chrono::Duration::days(12));
        let screen = render(&mut app, 90, 24);
        assert!(screen.contains("12d left"), "{screen}");

        app.data.habits[0].ends = Some(date());
        let screen = render(&mut app, 90, 24);
        assert!(screen.contains("last day"), "{screen}");

        app.data.habits[0].ends = Some(date() - chrono::Duration::days(2));
        let screen = render(&mut app, 90, 24);
        assert!(screen.contains("ended"), "{screen}");
    }

    #[test]
    fn description_sits_directly_under_the_habit_title() {
        let mut app = app_with("Gym");
        app.data.habits[0].note = Some("ten minutes is enough".into());
        let screen = render(&mut app, 90, 34);
        let description = screen.find("description").expect("description heading");
        let title = screen.find("Gym").expect("habit title");
        let schedule = screen.find("daily · since").expect("schedule line");
        assert!(title < description && description < schedule, "{screen}");
        assert_eq!(screen.matches("description:").count(), 1, "{screen}");
        assert!(screen.contains("    ten minutes is enough"), "{screen}");
    }

    #[test]
    fn the_bottom_bar_can_be_hidden_and_restored() {
        let mut app = app_with("Gym");
        let screen = render(&mut app, 90, 30);
        assert!(screen.contains("? help"), "{screen}");

        app.show_footer = false;
        let screen = render(&mut app, 90, 30);
        assert!(!screen.contains("? help"), "{screen}");

        app.show_footer = true;
        let screen = render(&mut app, 90, 30);
        assert!(screen.contains("? help"), "{screen}");
    }

    #[test]
    fn form_overlay_renders() {
        let mut app = app_with("Gym");
        app.mode = Mode::Form(Box::new(form::Form::new(date())));
        let screen = render(&mut app, 90, 30);
        assert!(screen.contains("New habit"), "{screen}");
        assert!(screen.contains("Schedule"), "{screen}");
        assert!(screen.contains("Tags"), "{screen}");
        assert!(screen.contains("Description"), "{screen}");
    }

    #[test]
    fn completed_habit_shows_a_streak() {
        let mut app = app_with("Gym");
        app.data.log.set(date(), "gym", true);
        let screen = render(&mut app, 90, 24);
        assert!(screen.contains("1 day"), "{screen}");
    }

    #[test]
    fn viewing_another_day_is_announced_in_the_header() {
        let mut app = app_with("Gym");
        app.shift_day(-2);
        let screen = render(&mut app, 90, 24);
        assert!(screen.contains("Sun 8 Nov"), "{screen}");
    }

    #[test]
    fn a_cheat_day_renders_with_its_own_glyph() {
        let mut app = app_with("Gym");
        app.data.cheats.set(date(), "gym", true);
        let screen = render(&mut app, 90, 24);
        assert!(screen.contains("cheat"), "{screen}");
    }
}
