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

    let rows = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(3),
        Constraint::Length(1),
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

    footer(frame, rows[2], app);

    match &app.mode {
        Mode::Normal => {}
        Mode::Help => help::draw(frame, area, &theme),
        Mode::Form(form) => form.draw(frame, area, &theme),
        Mode::Confirm(confirm) => confirm_overlay(frame, area, &theme, &confirm.prompt),
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
        Mode::Normal if app.filtering => {
            "type to filter · #tag for tags · enter keep · esc clear".to_string()
        }
        Mode::Normal => {
            let full = "space toggle · c cheat · [ ] day · t today · a add · e edit · d archive · x delete · u undo · / filter · T theme · m mode · ? help · q quit";
            if width(full) <= max_width {
                return full.to_string();
            }
            let medium = "space toggle · c cheat · [ ] day · / filter · ? help · q quit";
            if width(medium) <= max_width {
                return medium.to_string();
            }
            let tiny = "space · c cheat · / filter · ? help · q";
            if width(tiny) <= max_width {
                return tiny.to_string();
            }
            "space · / · ? · q".to_string()
        }
    }
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
        assert!(screen.contains("trend"), "{screen}");
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
    fn form_overlay_renders() {
        let mut app = app_with("Gym");
        app.mode = Mode::Form(Box::new(form::Form::new(date())));
        let screen = render(&mut app, 90, 30);
        assert!(screen.contains("New habit"), "{screen}");
        assert!(screen.contains("Schedule"), "{screen}");
        assert!(screen.contains("Tags"), "{screen}");
        assert!(screen.contains("Note"), "{screen}");
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
