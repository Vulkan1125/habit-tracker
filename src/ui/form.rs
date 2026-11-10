//! The add/edit dialog: own state, own key handling, own rendering.

use std::collections::BTreeSet;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Paragraph};

use crate::format::pad_right;
use crate::model::{ALL_DAYS, Day, Habit, Schedule};
use crate::theme::Theme;

#[derive(Debug, Clone, Default)]
pub struct TextInput {
    value: Vec<char>,
    cursor: usize,
}

impl TextInput {
    pub fn set(&mut self, text: &str) {
        self.value = text.chars().take(48).collect();
        self.cursor = self.value.len();
    }

    pub fn text(&self) -> String {
        self.value.iter().collect()
    }

    pub fn insert(&mut self, character: char) {
        if self.value.len() >= 48 {
            return;
        }
        self.value.insert(self.cursor, character);
        self.cursor += 1;
    }

    pub fn backspace(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
            self.value.remove(self.cursor);
        }
    }

    pub fn delete(&mut self) {
        if self.cursor < self.value.len() {
            self.value.remove(self.cursor);
        }
    }

    fn left(&mut self) {
        self.cursor = self.cursor.saturating_sub(1);
    }

    fn right(&mut self) {
        self.cursor = (self.cursor + 1).min(self.value.len());
    }

    fn home(&mut self) {
        self.cursor = 0;
    }

    fn end(&mut self) {
        self.cursor = self.value.len();
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    Name,
    Kind,
    Days,
    Weekly,
    Save,
}

impl Field {
    fn next(self) -> Field {
        match self {
            Field::Name => Field::Kind,
            Field::Kind => Field::Days,
            Field::Days => Field::Weekly,
            Field::Weekly => Field::Save,
            Field::Save => Field::Name,
        }
    }

    fn previous(self) -> Field {
        match self {
            Field::Name => Field::Save,
            Field::Kind => Field::Name,
            Field::Days => Field::Kind,
            Field::Weekly => Field::Days,
            Field::Save => Field::Weekly,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Daily,
    Weekdays,
    Weekly,
}

impl Kind {
    fn next(self) -> Kind {
        match self {
            Kind::Daily => Kind::Weekdays,
            Kind::Weekdays => Kind::Weekly,
            Kind::Weekly => Kind::Daily,
        }
    }

    fn previous(self) -> Kind {
        match self {
            Kind::Daily => Kind::Weekly,
            Kind::Weekdays => Kind::Daily,
            Kind::Weekly => Kind::Weekdays,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Kind::Daily => "every day",
            Kind::Weekdays => "chosen days",
            Kind::Weekly => "times per week",
        }
    }
}

pub enum FormAction {
    Continue,
    Cancel,
    Submit { name: String, schedule: Schedule },
}

pub struct Form {
    pub original: Option<String>,
    name: TextInput,
    kind: Kind,
    days: [bool; 7],
    weekly: TextInput,
    focus: Field,
    day_cursor: usize,
    error: Option<String>,
}

impl Form {
    pub fn new() -> Self {
        let mut weekly = TextInput::default();
        weekly.set("3");
        Self {
            original: None,
            name: TextInput::default(),
            kind: Kind::Daily,
            days: [false; 7],
            weekly,
            focus: Field::Name,
            day_cursor: 0,
            error: None,
        }
    }

    pub fn from_habit(habit: &Habit) -> Self {
        let mut form = Self::new();
        form.original = Some(habit.id.clone());
        form.name.set(&habit.name);
        match &habit.schedule {
            Schedule::Daily => form.kind = Kind::Daily,
            Schedule::Weekdays { days } => {
                form.kind = Kind::Weekdays;
                for day in days {
                    form.days[day.index()] = true;
                }
            }
            Schedule::TimesPerWeek { n } => {
                form.kind = Kind::Weekly;
                form.weekly.set(&n.to_string());
            }
        }
        form.focus = Field::Name;
        form
    }

    pub fn title(&self) -> &'static str {
        if self.original.is_some() {
            " Edit habit "
        } else {
            " New habit "
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> FormAction {
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            return FormAction::Cancel;
        }

        match key.code {
            KeyCode::Esc => return FormAction::Cancel,
            KeyCode::Enter => return self.submit(),
            KeyCode::Tab => {
                self.focus = self.focus.next();
                self.error = None;
                return FormAction::Continue;
            }
            KeyCode::BackTab => {
                self.focus = self.focus.previous();
                self.error = None;
                return FormAction::Continue;
            }
            KeyCode::Up => {
                self.focus = self.focus.previous();
                return FormAction::Continue;
            }
            KeyCode::Down => {
                self.focus = self.focus.next();
                return FormAction::Continue;
            }
            _ => {}
        }

        match self.focus {
            Field::Name => match key.code {
                KeyCode::Char(character) => self.name.insert(character),
                KeyCode::Backspace => self.name.backspace(),
                KeyCode::Delete => self.name.delete(),
                KeyCode::Left => self.name.left(),
                KeyCode::Right => self.name.right(),
                KeyCode::Home => self.name.home(),
                KeyCode::End => self.name.end(),
                _ => {}
            },
            Field::Kind => match key.code {
                KeyCode::Left | KeyCode::Char('h') => self.kind = self.kind.previous(),
                KeyCode::Right | KeyCode::Char('l') | KeyCode::Char(' ') => {
                    self.kind = self.kind.next();
                }
                _ => {}
            },
            Field::Days => match key.code {
                KeyCode::Left | KeyCode::Char('h') => {
                    self.day_cursor = (self.day_cursor + 6) % 7;
                }
                KeyCode::Right | KeyCode::Char('l') => {
                    self.day_cursor = (self.day_cursor + 1) % 7;
                }
                KeyCode::Char(' ') => {
                    let index = self.day_cursor;
                    self.days[index] = !self.days[index];
                    self.day_cursor = (self.day_cursor + 1) % 7;
                }
                _ => {}
            },
            Field::Weekly | Field::Save => {
                if self.focus == Field::Weekly {
                    match key.code {
                        KeyCode::Char(character) if character.is_ascii_digit() => {
                            if self.weekly.text().chars().count() < 2 {
                                self.weekly.insert(character);
                            }
                        }
                        KeyCode::Backspace => self.weekly.backspace(),
                        _ => {}
                    }
                }
            }
        }

        FormAction::Continue
    }

    fn submit(&mut self) -> FormAction {
        let name = self.name.text().trim().to_string();
        if name.is_empty() {
            self.error = Some("Give it a name first.".into());
            self.focus = Field::Name;
            return FormAction::Continue;
        }

        let schedule = match self.kind {
            Kind::Daily => Schedule::Daily,
            Kind::Weekdays => {
                let days: BTreeSet<Day> = ALL_DAYS
                    .iter()
                    .enumerate()
                    .filter(|(index, _)| self.days[*index])
                    .map(|(_, day)| *day)
                    .collect();
                if days.is_empty() {
                    self.error = Some("Pick at least one day (space toggles).".into());
                    self.focus = Field::Days;
                    return FormAction::Continue;
                }
                Schedule::Weekdays { days }
            }
            Kind::Weekly => {
                let n: u8 = self.weekly.text().trim().parse().unwrap_or(0);
                if !(1..=7).contains(&n) {
                    self.error = Some("Times per week must be 1-7.".into());
                    self.focus = Field::Weekly;
                    return FormAction::Continue;
                }
                Schedule::TimesPerWeek { n }
            }
        };

        FormAction::Submit { name, schedule }
    }

    pub fn draw(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        let width = area.width.saturating_sub(4).clamp(30, 62);
        let height = 11.min(area.height);
        let popup = centered(area, width, height);
        frame.render_widget(Clear, popup);

        let block = Block::bordered().title(self.title());
        let inner = block.inner(popup);
        frame.render_widget(block, popup);

        let rows = Layout::vertical([Constraint::Length(1); 7]).split(inner);

        frame.render_widget(
            Paragraph::new(field_row("Name", self.name_line(theme), theme)),
            rows[0],
        );
        frame.render_widget(
            Paragraph::new(field_row("Schedule", self.kind_line(theme), theme)),
            rows[1],
        );
        frame.render_widget(
            Paragraph::new(field_row("Days", self.days_line(theme), theme)),
            rows[2],
        );
        frame.render_widget(
            Paragraph::new(field_row("Per week", self.weekly_line(theme), theme)),
            rows[3],
        );

        if let Some(error) = &self.error {
            frame.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    error.clone(),
                    Style::default().fg(theme.missed),
                ))),
                rows[4],
            );
        }

        let submit = if self.focus == Field::Save {
            Span::styled(
                "[ save ]",
                Style::default().fg(theme.done).add_modifier(Modifier::BOLD),
            )
        } else {
            Span::styled("  save  ", Style::default().fg(theme.dim))
        };
        frame.render_widget(Paragraph::new(Line::from(submit)), rows[5]);

        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                "tab next · arrows change · enter save · esc cancel",
                Style::default().fg(theme.dim),
            ))),
            rows[6],
        );
    }

    fn name_line(&self, theme: &Theme) -> Vec<Span<'static>> {
        let focused = self.focus == Field::Name;
        let chars = self_text_chars(&self.name);
        let cursor = self.name.cursor;
        let mut spans: Vec<Span<'static>> = Vec::new();

        for (index, character) in chars.iter().enumerate() {
            let text = character.to_string();
            if focused && index == cursor {
                spans.push(Span::styled(
                    text,
                    Style::default()
                        .fg(theme.accent)
                        .add_modifier(Modifier::REVERSED),
                ));
            } else {
                spans.push(Span::raw(text));
            }
        }
        if focused && cursor >= chars.len() {
            spans.push(Span::styled(
                " ",
                Style::default().add_modifier(Modifier::REVERSED),
            ));
        }
        if chars.is_empty() && !focused {
            spans.push(Span::styled("(unnamed)", Style::default().fg(theme.dim)));
        }
        spans
    }

    fn kind_line(&self, theme: &Theme) -> Vec<Span<'static>> {
        let focused = self.focus == Field::Kind;
        let style = if focused {
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };
        let mut spans = Vec::new();
        if focused {
            spans.push(Span::styled("‹ ", Style::default().fg(theme.dim)));
        }
        spans.push(Span::styled(self.kind.label(), style));
        if focused {
            spans.push(Span::styled(" ›", Style::default().fg(theme.dim)));
        }
        spans
    }

    fn days_line(&self, theme: &Theme) -> Vec<Span<'static>> {
        let focused = self.focus == Field::Days;
        let mut spans = Vec::new();
        for (index, day) in ALL_DAYS.iter().enumerate() {
            let selected = self.days[index];
            let mut style = if selected {
                Style::default().fg(theme.done).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme.dim)
            };
            if focused && index == self.day_cursor {
                style = style.add_modifier(Modifier::REVERSED);
            }
            spans.push(Span::styled(day.short().to_string(), style));
            if index < 6 {
                spans.push(Span::raw(" "));
            }
        }
        spans
    }

    fn weekly_line(&self, theme: &Theme) -> Vec<Span<'static>> {
        let focused = self.focus == Field::Weekly;
        let active = self.kind == Kind::Weekly;
        let mut spans = Vec::new();
        let value = self.weekly.text();
        let style = if focused && active {
            Style::default().add_modifier(Modifier::REVERSED)
        } else if active {
            Style::default()
        } else {
            Style::default().fg(theme.dim)
        };
        spans.push(Span::styled(format!(" {value} "), style));
        if !active {
            spans.push(Span::styled(
                " (switch schedule to use)",
                Style::default().fg(theme.dim),
            ));
        }
        spans
    }
}

fn field_row(label: &str, mut value: Vec<Span<'static>>, theme: &Theme) -> Line<'static> {
    let mut spans = vec![Span::styled(
        pad_right(label, 12),
        Style::default().fg(theme.dim),
    )];
    spans.append(&mut value);
    Line::from(spans)
}

fn self_text_chars(input: &TextInput) -> Vec<char> {
    input.value.clone()
}

fn centered(area: Rect, width: u16, height: u16) -> Rect {
    let x = area.x + area.width.saturating_sub(width) / 2;
    let y = area.y + area.height.saturating_sub(height) / 2;
    Rect {
        x,
        y,
        width,
        height,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::KeyEvent;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn type_text(form: &mut Form, text: &str) {
        for character in text.chars() {
            form.handle_key(key(KeyCode::Char(character)));
        }
    }

    #[test]
    fn typing_a_name_and_saving_daily() {
        let mut form = Form::new();
        type_text(&mut form, "Gym");
        match form.handle_key(key(KeyCode::Enter)) {
            FormAction::Submit { name, schedule } => {
                assert_eq!(name, "Gym");
                assert_eq!(schedule, Schedule::Daily);
            }
            _ => panic!("expected submit"),
        }
    }

    #[test]
    fn empty_name_is_rejected_without_closing() {
        let mut form = Form::new();
        assert!(matches!(
            form.handle_key(key(KeyCode::Enter)),
            FormAction::Continue
        ));
        assert!(form.error.is_some());
    }

    #[test]
    fn weekday_schedule_needs_at_least_one_day() {
        let mut form = Form::new();
        type_text(&mut form, "Walk");
        form.handle_key(key(KeyCode::Tab)); // kind
        form.handle_key(key(KeyCode::Right)); // chosen days
        form.handle_key(key(KeyCode::Enter));
        assert!(form.error.is_some());
    }

    #[test]
    fn weekday_schedule_can_be_built() {
        let mut form = Form::new();
        type_text(&mut form, "Walk");
        form.handle_key(key(KeyCode::Tab));
        form.handle_key(key(KeyCode::Right));
        form.handle_key(key(KeyCode::Tab)); // days
        form.handle_key(key(KeyCode::Char(' '))); // toggle Monday
        form.handle_key(key(KeyCode::Right));
        form.handle_key(key(KeyCode::Char(' '))); // toggle Wednesday
        match form.handle_key(key(KeyCode::Enter)) {
            FormAction::Submit { schedule, .. } => {
                assert_eq!(
                    schedule,
                    Schedule::Weekdays {
                        days: BTreeSet::from([Day::Mon, Day::Wed])
                    }
                );
            }
            _ => panic!("expected submit"),
        }
    }

    #[test]
    fn weekly_quota_out_of_range_is_rejected() {
        let mut form = Form::new();
        type_text(&mut form, "Run");
        form.handle_key(key(KeyCode::Tab));
        form.handle_key(key(KeyCode::Right));
        form.handle_key(key(KeyCode::Right)); // times per week
        form.handle_key(key(KeyCode::Tab)); // days
        form.handle_key(key(KeyCode::Tab)); // weekly
        form.handle_key(key(KeyCode::Backspace));
        form.handle_key(key(KeyCode::Char('9')));
        assert!(matches!(
            form.handle_key(key(KeyCode::Enter)),
            FormAction::Continue
        ));
        assert!(form.error.is_some());
    }

    #[test]
    fn editing_prefills_everything() {
        let habit = Habit::new(
            "gym".into(),
            "Gym".into(),
            Schedule::TimesPerWeek { n: 4 },
            chrono::NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
        );
        let form = Form::from_habit(&habit);
        assert_eq!(form.name.text(), "Gym");
        assert_eq!(form.weekly.text(), "4");
        assert_eq!(form.kind, Kind::Weekly);
        assert_eq!(form.title(), " Edit habit ");
    }

    #[test]
    fn esc_cancels() {
        let mut form = Form::new();
        assert!(matches!(
            form.handle_key(key(KeyCode::Esc)),
            FormAction::Cancel
        ));
    }
}
