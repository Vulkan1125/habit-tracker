//! The add/edit dialog: own state, own key handling, own rendering.

use std::collections::BTreeSet;

use chrono::{NaiveDate, NaiveTime};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Paragraph};

use crate::app::parse_date;
use crate::format::{pad_right, truncate};
use crate::model::{ALL_DAYS, Day, Habit, Schedule, normalize_tag};
use crate::theme::Theme;

#[derive(Debug, Clone)]
pub struct TextInput {
    value: Vec<char>,
    cursor: usize,
    limit: usize,
}

impl Default for TextInput {
    fn default() -> Self {
        Self {
            value: Vec::new(),
            cursor: 0,
            limit: 48,
        }
    }
}

impl TextInput {
    pub fn with_limit(limit: usize) -> Self {
        Self {
            value: Vec::new(),
            cursor: 0,
            limit,
        }
    }

    pub fn set(&mut self, text: &str) {
        self.value = text.chars().take(self.limit).collect();
        self.cursor = self.value.len();
    }

    pub fn text(&self) -> String {
        self.value.iter().collect()
    }

    pub fn cursor(&self) -> usize {
        self.cursor
    }

    pub fn insert(&mut self, character: char) {
        if self.value.len() >= self.limit {
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

    pub fn left(&mut self) {
        self.cursor = self.cursor.saturating_sub(1);
    }

    pub fn right(&mut self) {
        self.cursor = (self.cursor + 1).min(self.value.len());
    }

    pub fn home(&mut self) {
        self.cursor = 0;
    }

    pub fn end(&mut self) {
        self.cursor = self.value.len();
    }
}

/// What a submitted form hands back to the app.
#[derive(Debug, Clone)]
pub struct Draft {
    pub name: String,
    pub schedule: Schedule,
    pub start: NaiveDate,
    pub ends: Option<NaiveDate>,
    pub reminder: Option<NaiveTime>,
    pub tags: Vec<String>,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    Name,
    Kind,
    Days,
    Weekly,
    Start,
    Ends,
    Reminder,
    Tags,
    Note,
    Save,
}

impl Field {
    fn next(self) -> Field {
        match self {
            Field::Name => Field::Kind,
            Field::Kind => Field::Days,
            Field::Days => Field::Weekly,
            Field::Weekly => Field::Start,
            Field::Start => Field::Ends,
            Field::Ends => Field::Reminder,
            Field::Reminder => Field::Tags,
            Field::Tags => Field::Note,
            Field::Note => Field::Save,
            Field::Save => Field::Name,
        }
    }

    fn previous(self) -> Field {
        match self {
            Field::Name => Field::Save,
            Field::Kind => Field::Name,
            Field::Days => Field::Kind,
            Field::Weekly => Field::Days,
            Field::Start => Field::Weekly,
            Field::Ends => Field::Start,
            Field::Reminder => Field::Ends,
            Field::Tags => Field::Reminder,
            Field::Note => Field::Tags,
            Field::Save => Field::Note,
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
    Submit(Draft),
}

pub struct Form {
    pub original: Option<String>,
    name: TextInput,
    kind: Kind,
    days: [bool; 7],
    weekly: TextInput,
    start: TextInput,
    ends: TextInput,
    reminder: TextInput,
    tags: TextInput,
    note: TextInput,
    focus: Field,
    day_cursor: usize,
    error: Option<String>,
}

impl Form {
    pub fn new(today: NaiveDate) -> Self {
        let mut weekly = TextInput::with_limit(2);
        weekly.set("3");
        let mut start = TextInput::with_limit(10);
        start.set(&today.to_string());
        Self {
            original: None,
            name: TextInput::default(),
            kind: Kind::Daily,
            days: [false; 7],
            weekly,
            start,
            ends: TextInput::with_limit(10),
            reminder: TextInput::with_limit(5),
            tags: TextInput::with_limit(80),
            note: TextInput::with_limit(160),
            focus: Field::Name,
            day_cursor: 0,
            error: None,
        }
    }

    pub fn from_habit(habit: &Habit) -> Self {
        let mut form = Self::new(habit.created);
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
        if let Some(ends) = habit.ends {
            form.ends.set(&ends.to_string());
        }
        if let Some(reminder) = habit.reminder {
            form.reminder.set(&reminder.format("%H:%M").to_string());
        }
        form.tags.set(&habit.tags.join(", "));
        if let Some(note) = &habit.note {
            form.note.set(note);
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
            Field::Name
            | Field::Start
            | Field::Ends
            | Field::Reminder
            | Field::Tags
            | Field::Note => {
                let input = match self.focus {
                    Field::Name => &mut self.name,
                    Field::Start => &mut self.start,
                    Field::Ends => &mut self.ends,
                    Field::Reminder => &mut self.reminder,
                    Field::Tags => &mut self.tags,
                    _ => &mut self.note,
                };
                match key.code {
                    KeyCode::Char(character) => input.insert(character),
                    KeyCode::Backspace => input.backspace(),
                    KeyCode::Delete => input.delete(),
                    KeyCode::Left => input.left(),
                    KeyCode::Right => input.right(),
                    KeyCode::Home => input.home(),
                    KeyCode::End => input.end(),
                    _ => {}
                }
            }
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
            Field::Weekly => match key.code {
                KeyCode::Char(character) if character.is_ascii_digit() => {
                    if self.weekly.text().chars().count() < 2 {
                        self.weekly.insert(character);
                    }
                }
                KeyCode::Backspace => self.weekly.backspace(),
                _ => {}
            },
            Field::Save => {}
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

        let Some(start) = parse_date(&self.start.text()) else {
            self.error = Some("Start must be a date like 2026-11-10.".into());
            self.focus = Field::Start;
            return FormAction::Continue;
        };

        let ends_text = self.ends.text().trim().to_string();
        let ends = if ends_text.is_empty() {
            None
        } else {
            match parse_date(&ends_text) {
                Some(ends) if ends >= start => Some(ends),
                Some(_) => {
                    self.error = Some("The end date must not be before the start.".into());
                    self.focus = Field::Ends;
                    return FormAction::Continue;
                }
                None => {
                    self.error = Some("End must be a date like 2026-12-31, or empty.".into());
                    self.focus = Field::Ends;
                    return FormAction::Continue;
                }
            }
        };

        let reminder_text = self.reminder.text().trim().to_string();
        let reminder = if reminder_text.is_empty() {
            None
        } else {
            match NaiveTime::parse_from_str(&reminder_text, "%H:%M") {
                Ok(time) => Some(time),
                Err(_) => {
                    self.error = Some("Remind must be a 24-hour time like 09:00, or empty.".into());
                    self.focus = Field::Reminder;
                    return FormAction::Continue;
                }
            }
        };

        let mut tags: BTreeSet<String> = BTreeSet::new();
        for chunk in self.tags.text().split([',', ';']) {
            if let Some(tag) = normalize_tag(chunk) {
                tags.insert(tag);
            }
        }

        let note = {
            let text = self.note.text().trim().to_string();
            (!text.is_empty()).then_some(text)
        };

        FormAction::Submit(Draft {
            name,
            schedule,
            start,
            ends,
            reminder,
            tags: tags.into_iter().collect(),
            note,
        })
    }

    pub fn draw(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        let width = area.width.saturating_sub(4).clamp(30, 66);
        let height = 14.min(area.height);
        let popup = centered(area, width, height);
        frame.render_widget(Clear, popup);

        let block = Block::bordered()
            .title(self.title())
            .border_style(Style::default().fg(theme.border));
        let inner = block.inner(popup);
        frame.render_widget(block, popup);

        let rows = Layout::vertical([Constraint::Length(1); 12]).split(inner);
        let field_width = inner.width.saturating_sub(12) as usize;

        frame.render_widget(
            Paragraph::new(field_row(
                "Name",
                self.input_line(&self.name, Field::Name, theme, field_width, "Gym"),
                theme,
            )),
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
        frame.render_widget(
            Paragraph::new(field_row(
                "Starts",
                self.input_line(&self.start, Field::Start, theme, 10, "2026-11-10"),
                theme,
            )),
            rows[4],
        );
        frame.render_widget(
            Paragraph::new(field_row(
                "Ends",
                self.input_line(&self.ends, Field::Ends, theme, 10, "never"),
                theme,
            )),
            rows[5],
        );
        frame.render_widget(
            Paragraph::new(field_row(
                "Remind",
                self.input_line(&self.reminder, Field::Reminder, theme, 5, "off"),
                theme,
            )),
            rows[6],
        );
        frame.render_widget(
            Paragraph::new(field_row(
                "Tags",
                self.input_line(
                    &self.tags,
                    Field::Tags,
                    theme,
                    field_width,
                    "health, morning",
                ),
                theme,
            )),
            rows[7],
        );
        frame.render_widget(
            Paragraph::new(field_row(
                "Description",
                self.input_line(&self.note, Field::Note, theme, field_width, "optional"),
                theme,
            )),
            rows[8],
        );

        if let Some(error) = &self.error {
            frame.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    truncate(error, inner.width as usize),
                    Style::default().fg(theme.missed),
                ))),
                rows[9],
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
        frame.render_widget(Paragraph::new(Line::from(submit)), rows[10]);

        let hint = if inner.width >= 47 {
            "tab next · arrows change · enter save · esc cancel"
        } else {
            "tab · enter save · esc cancel"
        };
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                hint,
                Style::default().fg(theme.dim),
            ))),
            rows[11],
        );
    }

    fn input_line(
        &self,
        input: &TextInput,
        field: Field,
        theme: &Theme,
        width: usize,
        placeholder: &str,
    ) -> Vec<Span<'static>> {
        let focused = self.focus == field;
        let chars = &input.value;
        let mut spans: Vec<Span<'static>> = Vec::new();

        if chars.is_empty() && !focused {
            spans.push(Span::styled(
                placeholder.to_string(),
                Style::default().fg(theme.dim),
            ));
            return spans;
        }

        // Keep the cursor visible by sliding the window when the value is long.
        let width = width.max(1);
        let start = if chars.len() <= width {
            0
        } else {
            input.cursor.saturating_sub(width.saturating_sub(1))
        };
        let end = (start + width).min(chars.len());

        for (index, character) in chars[start..end].iter().enumerate() {
            let absolute = start + index;
            if focused && absolute == input.cursor {
                spans.push(Span::styled(
                    character.to_string(),
                    Style::default()
                        .fg(theme.accent)
                        .add_modifier(Modifier::REVERSED),
                ));
            } else {
                spans.push(Span::raw(character.to_string()));
            }
        }
        if focused && input.cursor >= chars.len() {
            spans.push(Span::styled(
                " ",
                Style::default().add_modifier(Modifier::REVERSED),
            ));
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
        let active = self.kind == Kind::Weekdays;
        let mut spans = Vec::new();
        for (index, day) in ALL_DAYS.iter().enumerate() {
            let selected = self.days[index];
            let mut style = if !active {
                Style::default().fg(theme.dim)
            } else if selected {
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

    fn date(text: &str) -> NaiveDate {
        NaiveDate::parse_from_str(text, "%Y-%m-%d").unwrap()
    }

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn type_text(form: &mut Form, text: &str) {
        for character in text.chars() {
            form.handle_key(key(KeyCode::Char(character)));
        }
    }

    fn submitted(action: FormAction) -> Draft {
        match action {
            FormAction::Submit(draft) => draft,
            _ => panic!("expected submit"),
        }
    }

    #[test]
    fn typing_a_name_and_saving_daily() {
        let mut form = Form::new(date("2026-11-10"));
        type_text(&mut form, "Gym");
        let draft = submitted(form.handle_key(key(KeyCode::Enter)));
        assert_eq!(draft.name, "Gym");
        assert_eq!(draft.schedule, Schedule::Daily);
        assert_eq!(draft.start, date("2026-11-10"));
        assert_eq!(draft.ends, None);
        assert_eq!(draft.reminder, None);
        assert!(draft.tags.is_empty());
        assert_eq!(draft.note, None);
    }

    #[test]
    fn empty_name_is_rejected_without_closing() {
        let mut form = Form::new(date("2026-11-10"));
        assert!(matches!(
            form.handle_key(key(KeyCode::Enter)),
            FormAction::Continue
        ));
        assert!(form.error.is_some());
    }

    #[test]
    fn weekday_schedule_needs_at_least_one_day() {
        let mut form = Form::new(date("2026-11-10"));
        type_text(&mut form, "Walk");
        form.handle_key(key(KeyCode::Tab)); // kind
        form.handle_key(key(KeyCode::Right)); // chosen days
        form.handle_key(key(KeyCode::Enter));
        assert!(form.error.is_some());
    }

    #[test]
    fn weekday_schedule_can_be_built() {
        let mut form = Form::new(date("2026-11-10"));
        type_text(&mut form, "Walk");
        form.handle_key(key(KeyCode::Tab));
        form.handle_key(key(KeyCode::Right));
        form.handle_key(key(KeyCode::Tab)); // days
        form.handle_key(key(KeyCode::Char(' '))); // toggle Monday
        form.handle_key(key(KeyCode::Right));
        form.handle_key(key(KeyCode::Char(' '))); // toggle Wednesday
        let draft = submitted(form.handle_key(key(KeyCode::Enter)));
        assert_eq!(
            draft.schedule,
            Schedule::Weekdays {
                days: BTreeSet::from([Day::Mon, Day::Wed])
            }
        );
    }

    #[test]
    fn weekly_quota_out_of_range_is_rejected() {
        let mut form = Form::new(date("2026-11-10"));
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
        let mut habit = Habit::new(
            "gym".into(),
            "Gym".into(),
            Schedule::TimesPerWeek { n: 4 },
            chrono::NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
        );
        habit.ends = Some(date("2026-12-31"));
        habit.tags = vec!["health".into(), "morning".into()];
        habit.note = Some("ten minutes".into());
        let form = Form::from_habit(&habit);
        assert_eq!(form.name.text(), "Gym");
        assert_eq!(form.weekly.text(), "4");
        assert_eq!(form.kind, Kind::Weekly);
        assert_eq!(form.start.text(), "2026-01-01");
        assert_eq!(form.ends.text(), "2026-12-31");
        assert_eq!(form.tags.text(), "health, morning");
        assert_eq!(form.note.text(), "ten minutes");
        assert_eq!(form.title(), " Edit habit ");
    }

    #[test]
    fn tags_split_on_commas_and_normalise_spaces() {
        let mut form = Form::new(date("2026-11-10"));
        type_text(&mut form, "Read");
        for _ in 0..7 {
            form.handle_key(key(KeyCode::Tab));
        }
        type_text(&mut form, "Health & Focus, reading");
        let draft = submitted(form.handle_key(key(KeyCode::Enter)));
        assert_eq!(draft.tags, vec!["health-focus", "reading"]);
    }

    #[test]
    fn dates_tags_and_notes_are_parsed_on_save() {
        let mut form = Form::new(date("2026-11-10"));
        type_text(&mut form, "Read");
        // name -> kind -> days -> weekly -> start -> ends -> tags -> note
        for _ in 0..4 {
            form.handle_key(key(KeyCode::Tab));
        }
        // Replace the prefilled start date.
        for _ in 0..10 {
            form.handle_key(key(KeyCode::Backspace));
        }
        type_text(&mut form, "2026-01-05");
        form.handle_key(key(KeyCode::Tab));
        type_text(&mut form, "2026-03-01");
        form.handle_key(key(KeyCode::Tab)); // reminder
        form.handle_key(key(KeyCode::Tab)); // tags
        type_text(&mut form, "#Health, reading");
        form.handle_key(key(KeyCode::Tab));
        type_text(&mut form, "20 pages a day");
        let draft = submitted(form.handle_key(key(KeyCode::Enter)));
        assert_eq!(draft.start, date("2026-01-05"));
        assert_eq!(draft.ends, Some(date("2026-03-01")));
        assert_eq!(draft.tags, vec!["health", "reading"]);
        assert_eq!(draft.note.as_deref(), Some("20 pages a day"));
    }

    #[test]
    fn an_end_before_the_start_is_rejected() {
        let mut form = Form::new(date("2026-11-10"));
        type_text(&mut form, "Read");
        form.handle_key(key(KeyCode::Tab));
        form.handle_key(key(KeyCode::Tab));
        form.handle_key(key(KeyCode::Tab));
        form.handle_key(key(KeyCode::Tab)); // start
        form.handle_key(key(KeyCode::Tab)); // ends
        type_text(&mut form, "2026-01-01");
        assert!(matches!(
            form.handle_key(key(KeyCode::Enter)),
            FormAction::Continue
        ));
        assert!(form.error.as_deref().unwrap().contains("before"));
    }

    #[test]
    fn a_malformed_start_date_is_rejected() {
        let mut form = Form::new(date("2026-11-10"));
        type_text(&mut form, "Read");
        form.handle_key(key(KeyCode::Tab));
        form.handle_key(key(KeyCode::Tab));
        form.handle_key(key(KeyCode::Tab));
        form.handle_key(key(KeyCode::Tab)); // start
        for _ in 0..10 {
            form.handle_key(key(KeyCode::Backspace));
        }
        type_text(&mut form, "yesterday");
        assert!(matches!(
            form.handle_key(key(KeyCode::Enter)),
            FormAction::Continue
        ));
        assert!(form.error.is_some());
    }

    #[test]
    fn esc_cancels() {
        let mut form = Form::new(date("2026-11-10"));
        assert!(matches!(
            form.handle_key(key(KeyCode::Esc)),
            FormAction::Cancel
        ));
    }
}
