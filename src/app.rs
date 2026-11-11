//! Application state, key handling and persistence-on-change.

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use chrono::{NaiveDate, NaiveDateTime};
use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::layout::Rect;
use ratatui::widgets::ListState;

use crate::config::Config;
use crate::model::{Data, Day, Habit, TagError};
use crate::reminders;
use crate::stats;
use crate::store::Store;
use crate::theme::{ColorMode, Theme, ThemeName};
use crate::ui::form::{Form, FormAction};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Habits,
    Tags,
    Archive,
}

pub enum Mode {
    Normal,
    Help,
    Confirm(Confirm),
    Form(Box<Form>),
    Note(NoteEntry),
    Tag(TagEntry),
    /// The one-time explanation shown when reminders are first enabled.
    ReminderSetup,
}

/// The rename prompt, opened from the tags tab.
pub struct TagEntry {
    /// The tag being renamed, shown in the title while the field is empty.
    pub from: String,
    pub input: crate::ui::form::TextInput,
    /// Why the name in the field cannot be used, shown in place of the hint.
    pub error: Option<String>,
}

pub enum TagAction {
    Continue,
    Cancel,
    Submit(String),
}

impl TagEntry {
    pub fn handle_key(&mut self, key: KeyEvent) -> TagAction {
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            return TagAction::Cancel;
        }
        match key.code {
            KeyCode::Esc => TagAction::Cancel,
            KeyCode::Enter => TagAction::Submit(self.input.text()),
            _ => {
                self.error = None;
                edit_input(&mut self.input, key);
                TagAction::Continue
            }
        }
    }
}

/// Which half of the note dialog has the keyboard.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoteField {
    Date,
    Text,
}

impl NoteField {
    fn toggled(self) -> NoteField {
        match self {
            NoteField::Date => NoteField::Text,
            NoteField::Text => NoteField::Date,
        }
    }
}

/// A single dated note being written for one habit.
pub struct NoteEntry {
    pub habit_id: String,
    pub habit_name: String,
    /// The habit's first day: a note cannot be dated before it.
    pub start: NaiveDate,
    /// Today, the latest day a note can be dated.
    pub today: NaiveDate,
    /// The date the note will be filed under, as `YYYY-MM-DD`.
    pub date_input: crate::ui::form::TextInput,
    pub input: crate::ui::form::TextInput,
    pub focus: NoteField,
    /// Why the date in the field cannot be saved, shown in place of the hint.
    pub error: Option<String>,
}

pub enum NoteAction {
    Continue,
    Cancel,
    Save { date: NaiveDate, text: String },
}

impl NoteEntry {
    pub fn handle_key(&mut self, key: KeyEvent) -> NoteAction {
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            return NoteAction::Cancel;
        }
        match key.code {
            KeyCode::Esc => NoteAction::Cancel,
            KeyCode::Tab | KeyCode::BackTab => {
                self.focus = self.focus.toggled();
                self.error = None;
                NoteAction::Continue
            }
            KeyCode::Enter => match self.resolve_date() {
                Ok(date) => NoteAction::Save {
                    date,
                    text: self.input.text(),
                },
                Err(message) => {
                    self.error = Some(message);
                    self.focus = NoteField::Date;
                    NoteAction::Continue
                }
            },
            _ => {
                self.edit(key);
                NoteAction::Continue
            }
        }
    }

    fn edit(&mut self, key: KeyEvent) {
        match self.focus {
            NoteField::Date => match key.code {
                KeyCode::Up => self.nudge_date(-1),
                KeyCode::Down => self.nudge_date(1),
                _ => {
                    if !matches!(
                        key.code,
                        KeyCode::Left | KeyCode::Right | KeyCode::Home | KeyCode::End
                    ) {
                        self.error = None;
                    }
                    edit_input(&mut self.date_input, key);
                }
            },
            NoteField::Text => edit_input(&mut self.input, key),
        }
    }

    /// The date in the field, or today when it does not parse.
    pub fn date(&self) -> NaiveDate {
        parse_date(&self.date_input.text()).unwrap_or(self.today)
    }

    fn nudge_date(&mut self, delta: i64) {
        let moved = self.date() + chrono::Duration::days(delta);
        self.date_input.set(&moved.format("%Y-%m-%d").to_string());
        self.error = None;
    }

    /// The date the note may be saved under, or why it may not.
    fn resolve_date(&self) -> std::result::Result<NaiveDate, String> {
        let raw = self.date_input.text().trim().to_string();
        let Some(date) = parse_date(&raw) else {
            return Err(format!("`{raw}` is not a date · use YYYY-MM-DD"));
        };
        if date > self.today {
            return Err(format!("{} is in the future", date.format("%Y-%m-%d")));
        }
        if date < self.start {
            return Err(format!(
                "the habit starts {}",
                self.start.format("%Y-%m-%d")
            ));
        }
        Ok(date)
    }
}

/// Cursor movement and typing, shared by the date and note fields.
fn edit_input(input: &mut crate::ui::form::TextInput, key: KeyEvent) {
    match key.code {
        KeyCode::Backspace => input.backspace(),
        KeyCode::Delete => input.delete(),
        KeyCode::Left => input.left(),
        KeyCode::Right => input.right(),
        KeyCode::Home => input.home(),
        KeyCode::End => input.end(),
        KeyCode::Char(character)
            if !key.modifiers.contains(KeyModifiers::CONTROL)
                && !key.modifiers.contains(KeyModifiers::ALT) =>
        {
            input.insert(character);
        }
        _ => {}
    }
}

/// One visual line in the left pane: a section heading, the tag view's archive
/// divider, or a habit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListRow {
    /// A section label that cannot be selected.
    Heading(&'static str),
    Archive,
    Habit(usize),
}

/// Where the last frame put the clickable things, so mouse events can be
/// resolved without re-running the layout.
#[derive(Debug, Clone, Default)]
pub struct HitAreas {
    pub tabs: Vec<(Tab, Rect)>,
    pub list: Option<Rect>,
    pub back: Option<Rect>,
}

pub struct Confirm {
    pub prompt: String,
    pub action: ConfirmAction,
}

pub enum ConfirmAction {
    Delete(String),
    /// Fold `from` into `to`; both are already normalised.
    MergeTag {
        from: String,
        to: String,
    },
}

pub struct Flash {
    pub text: String,
    pub error: bool,
    pub warn: bool,
    pub at: Instant,
}

pub struct App {
    pub store: Store,
    pub data: Data,
    pub today: NaiveDate,
    /// The day the cursor and the check-off keys act on.
    pub view_date: NaiveDate,
    pub week_start: Day,
    pub tab: Tab,
    pub tag_focus: Option<String>,
    pub tag_cursor: usize,
    pub hits: HitAreas,
    /// Whether the one-line hint bar at the bottom is drawn.
    pub show_footer: bool,
    pub config: Config,
    pub theme: Theme,
    pub cursor: usize,
    pub list_state: ListState,
    pub mode: Mode,
    pub filter: String,
    pub filtering: bool,
    pub flash: Option<Flash>,
    pub undo: Option<Data>,
    pub quit: bool,
}

impl App {
    pub fn new(store: Store, data: Data, today: NaiveDate, config: &Config) -> Self {
        Self {
            store,
            data,
            today,
            view_date: today,
            week_start: config.week_start,
            tab: Tab::Habits,
            tag_focus: None,
            tag_cursor: 0,
            hits: HitAreas::default(),
            show_footer: true,
            config: config.clone(),
            theme: config.palette(),
            cursor: 0,
            list_state: ListState::default(),
            mode: Mode::Normal,
            filter: String::new(),
            filtering: false,
            flash: None,
            undo: None,
            quit: false,
        }
    }

    pub fn flash_warning(&mut self, message: String) {
        self.flash = Some(Flash {
            text: message,
            error: false,
            warn: true,
            at: Instant::now(),
        });
    }

    /// Every tag with the number of habits that are still active.
    ///
    /// Archived and finished habits are counted as zero on purpose: the tab is
    /// a map of what you are still working on, not an archive index.
    pub fn tags(&self) -> Vec<(String, usize)> {
        let mut counts: BTreeMap<String, usize> = BTreeMap::new();
        for habit in &self.data.habits {
            let active = !habit.archived && !habit.is_finished(self.today);
            for tag in &habit.tags {
                let count = counts.entry(tag.clone()).or_insert(0);
                if active {
                    *count += 1;
                }
            }
        }
        counts.into_iter().collect()
    }

    /// Visual rows for the left pane. Tags and the archive have their own
    /// grouping; the habits tab pushes habits that have not started yet under a
    /// separate heading so today's list stays about today.
    pub fn view_rows(&self) -> Vec<ListRow> {
        let query = self.filter.trim().to_lowercase();
        let keep =
            |index: usize| query.is_empty() || matches_query(&self.data.habits[index], &query);

        match self.tab {
            Tab::Tags => {
                let Some(tag) = &self.tag_focus else {
                    return Vec::new();
                };
                let mut active = Vec::new();
                let mut closed = Vec::new();
                for index in self.data.display_order(self.today) {
                    let habit = &self.data.habits[index];
                    if !habit.tags.iter().any(|candidate| candidate == tag) || !keep(index) {
                        continue;
                    }
                    if habit.archived || habit.is_finished(self.today) {
                        closed.push(index);
                    } else {
                        active.push(index);
                    }
                }
                let mut rows: Vec<ListRow> = active.into_iter().map(ListRow::Habit).collect();
                if !closed.is_empty() {
                    rows.push(ListRow::Archive);
                    rows.extend(closed.into_iter().map(ListRow::Habit));
                }
                rows
            }
            Tab::Archive => {
                let mut archived = Vec::new();
                let mut ended = Vec::new();
                for index in self.data.display_order(self.today) {
                    let habit = &self.data.habits[index];
                    if !keep(index) {
                        continue;
                    }
                    if habit.archived {
                        archived.push(index);
                    } else if habit.is_finished(self.today) {
                        ended.push(index);
                    }
                }
                let mut rows = Vec::new();
                if !archived.is_empty() {
                    rows.push(ListRow::Heading("Archived"));
                    rows.extend(archived.into_iter().map(ListRow::Habit));
                }
                if !ended.is_empty() {
                    rows.push(ListRow::Heading("Ended"));
                    rows.extend(ended.into_iter().map(ListRow::Habit));
                }
                rows
            }
            Tab::Habits => {
                let mut running = Vec::new();
                let mut later = Vec::new();
                for index in self.data.display_order(self.today) {
                    let habit = &self.data.habits[index];
                    if habit.archived || habit.is_finished(self.today) || !keep(index) {
                        continue;
                    }
                    if habit.created > self.today {
                        later.push(index);
                    } else {
                        running.push(index);
                    }
                }
                let mut rows: Vec<ListRow> = running.into_iter().map(ListRow::Habit).collect();
                if !later.is_empty() {
                    rows.push(ListRow::Heading("Starting later"));
                    rows.extend(later.into_iter().map(ListRow::Habit));
                }
                rows
            }
        }
    }

    /// Indices into `data.habits`, active first, archived/finished last, filtered.
    pub fn visible_indices(&self) -> Vec<usize> {
        self.view_rows()
            .into_iter()
            .filter_map(|row| match row {
                ListRow::Habit(index) => Some(index),
                ListRow::Heading(_) | ListRow::Archive => None,
            })
            .collect()
    }

    pub fn select_tab(&mut self, tab: Tab) {
        if tab == self.tab {
            if tab == Tab::Tags && self.tag_focus.is_some() {
                self.tag_focus = None;
                self.cursor = 0;
            }
            return;
        }
        self.tab = tab;
        self.tag_focus = None;
        self.cursor = 0;
        self.tag_cursor = 0;
        self.clamp_cursor();
    }

    fn next_tab(&mut self) {
        let next = match self.tab {
            Tab::Habits => Tab::Tags,
            Tab::Tags => Tab::Archive,
            Tab::Archive => Tab::Habits,
        };
        self.select_tab(next);
    }

    fn move_tag(&mut self, delta: i32) {
        let length = self.tags().len();
        if length == 0 {
            self.tag_cursor = 0;
            return;
        }
        let current = self.tag_cursor.min(length - 1) as i32;
        self.tag_cursor = (current + delta).clamp(0, length as i32 - 1) as usize;
    }

    pub fn open_focused_tag(&mut self) {
        let tags = self.tags();
        let Some((tag, _)) = tags.get(self.tag_cursor) else {
            self.flash_err("no tags yet");
            return;
        };
        self.tag_focus = Some(tag.clone());
        self.cursor = 0;
        self.clamp_cursor();
    }

    /// Opens the rename prompt on the tag under the cursor.
    fn start_tag_rename(&mut self) {
        let tags = self.tags();
        let Some((tag, _)) = tags.get(self.tag_cursor.min(tags.len().saturating_sub(1))) else {
            self.flash_err("no tags yet");
            return;
        };
        self.mode = Mode::Tag(TagEntry {
            from: tag.clone(),
            input: crate::ui::form::TextInput::with_limit(crate::model::TAG_MAX_CHARS),
            error: None,
        });
    }

    fn submit_tag_rename(&mut self, mut entry: TagEntry, text: &str) {
        let Some(to) = crate::model::normalize_tag(text) else {
            entry.error = Some(TagError::Invalid.message().to_string());
            self.mode = Mode::Tag(entry);
            return;
        };
        if to == entry.from {
            entry.error = Some(TagError::Same.message().to_string());
            self.mode = Mode::Tag(entry);
            return;
        }
        // Renaming onto a name that is already in use folds the two tags into
        // one, which throws a name away: ask before doing that.
        if self.tags().iter().any(|(name, _)| *name == to) {
            self.mode = Mode::Confirm(Confirm {
                prompt: format!("Merge #{} into #{to}?", entry.from),
                action: ConfirmAction::MergeTag {
                    from: entry.from,
                    to,
                },
            });
            return;
        }
        self.apply_tag_rename(&entry.from, &to);
    }

    fn apply_tag_rename(&mut self, from: &str, to: &str) {
        let before = self.data.clone();
        let outcome = match self.data.rename_tag(from, to) {
            Ok(outcome) => outcome,
            Err(error) => {
                self.flash_err(error.message());
                return;
            }
        };
        if !self.commit(before) {
            return;
        }
        // The list re-sorts around the new name; keep the cursor on it.
        self.tag_cursor = self
            .tags()
            .iter()
            .position(|(name, _)| *name == outcome.to)
            .unwrap_or(0);
        let habits = match outcome.habits {
            1 => "1 habit".to_string(),
            count => format!("{count} habits"),
        };
        if outcome.merged {
            self.flash_ok(format!(
                "merged #{} into #{} · {habits}",
                outcome.from, outcome.to
            ));
        } else {
            self.flash_ok(format!(
                "renamed #{} to #{} · {habits}",
                outcome.from, outcome.to
            ));
        }
    }

    fn start_note(&mut self) {
        let Some(habit) = self.selected_habit() else {
            self.flash_err("select a habit first");
            return;
        };
        // Defaults to the day on screen, which is today unless the user moved
        // with `[` / `]`, and never earlier than the habit itself.
        let default = self.view_date.max(habit.created);
        let mut date_input = crate::ui::form::TextInput::with_limit(10);
        date_input.set(&default.format("%Y-%m-%d").to_string());
        let mut input = crate::ui::form::TextInput::with_limit(400);
        if let Some(existing) = habit.note_on(default) {
            input.set(existing);
        }
        self.mode = Mode::Note(NoteEntry {
            habit_id: habit.id.clone(),
            habit_name: habit.name.clone(),
            start: habit.created,
            today: self.today,
            date_input,
            input,
            focus: NoteField::Text,
            error: None,
        });
    }

    fn save_note(&mut self, entry: NoteEntry, date: NaiveDate, text: String) {
        let existing = self
            .data
            .habits
            .iter()
            .find(|habit| habit.id == entry.habit_id)
            .and_then(|habit| habit.note_on(date))
            .unwrap_or("")
            .to_string();
        if existing == text.trim() {
            self.flash_ok("note unchanged");
            return;
        }
        let id = entry.habit_id.clone();
        let text_owned = text.clone();
        self.mutate(move |data| {
            if let Some(habit) = data.habits.iter_mut().find(|habit| habit.id == id) {
                habit.set_note(date, &text_owned);
            }
        });
        let when = format!("{} · {}", date.format("%a %-d %b"), entry.habit_name);
        if text.trim().is_empty() {
            self.flash_ok(format!("note removed · {when}"));
        } else {
            self.flash_ok(format!("note saved · {when} · n edits it"));
        }
    }

    pub fn handle_mouse(&mut self, mouse: MouseEvent) {
        if !matches!(&self.mode, Mode::Normal) {
            return;
        }
        match mouse.kind {
            MouseEventKind::Down(MouseButton::Left) => self.mouse_click(mouse.column, mouse.row),
            MouseEventKind::ScrollDown => self.scroll_list(1),
            MouseEventKind::ScrollUp => self.scroll_list(-1),
            _ => {}
        }
    }

    fn mouse_click(&mut self, column: u16, row: u16) {
        if let Some((tab, _)) = self
            .hits
            .tabs
            .iter()
            .find(|(_, rect)| rect_contains(*rect, column, row))
        {
            let tab = *tab;
            self.select_tab(tab);
            return;
        }
        if let Some(back) = self.hits.back
            && rect_contains(back, column, row)
        {
            self.tag_focus = None;
            self.cursor = 0;
            return;
        }
        let Some(list) = self.hits.list else {
            return;
        };
        if !rect_contains(list, column, row) {
            return;
        }
        let offset = (row - list.y) as usize;

        if self.tab == Tab::Tags && self.tag_focus.is_none() {
            if offset < self.tags().len() {
                self.tag_cursor = offset;
                self.open_focused_tag();
            }
            return;
        }

        let Some(ListRow::Habit(index)) = self.view_rows().get(offset).copied() else {
            return;
        };
        if let Some(position) = self
            .visible_indices()
            .iter()
            .position(|candidate| *candidate == index)
        {
            self.cursor = position;
        }
    }

    fn scroll_list(&mut self, delta: i32) {
        if self.tab == Tab::Tags && self.tag_focus.is_none() {
            self.move_tag(delta);
        } else {
            self.move_cursor(delta);
        }
    }

    pub fn selected(&self) -> Option<usize> {
        self.visible_indices().get(self.cursor).copied()
    }

    pub fn selected_habit(&self) -> Option<&Habit> {
        self.selected().map(|index| &self.data.habits[index])
    }

    pub fn handle_event(&mut self, event: Event) -> Result<()> {
        match event {
            Event::Key(key) if key.kind == KeyEventKind::Press => self.handle_key(key)?,
            Event::Mouse(mouse) => self.handle_mouse(mouse),
            _ => {}
        }
        Ok(())
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> Result<()> {
        match std::mem::replace(&mut self.mode, Mode::Normal) {
            Mode::Normal => self.normal_key(key),
            Mode::Help => Ok(()),
            Mode::Confirm(confirm) => {
                self.confirm_key(key, confirm);
                Ok(())
            }
            Mode::Form(mut form) => {
                let original = form.original.clone();
                match form.handle_key(key) {
                    FormAction::Continue => self.mode = Mode::Form(form),
                    FormAction::Cancel => {}
                    FormAction::Submit(draft) => self.submit_form(original, draft),
                }
                Ok(())
            }
            Mode::Note(mut entry) => {
                match entry.handle_key(key) {
                    NoteAction::Continue => self.mode = Mode::Note(entry),
                    NoteAction::Cancel => {}
                    NoteAction::Save { date, text } => self.save_note(entry, date, text),
                }
                Ok(())
            }
            Mode::Tag(mut entry) => {
                match entry.handle_key(key) {
                    TagAction::Continue => self.mode = Mode::Tag(entry),
                    TagAction::Cancel => {}
                    TagAction::Submit(text) => self.submit_tag_rename(entry, &text),
                }
                Ok(())
            }
            Mode::ReminderSetup => {
                let install = matches!(key.code, KeyCode::Char('y') | KeyCode::Char('Y'));
                self.finish_reminder_setup(install);
                Ok(())
            }
        }
    }

    fn normal_key(&mut self, key: KeyEvent) -> Result<()> {
        if self.filtering {
            self.filter_key(key);
            return Ok(());
        }

        // The tag list has its own cursor and no habit row to act on.
        if self.tab == Tab::Tags && self.tag_focus.is_none() {
            match key.code {
                KeyCode::Char('j') | KeyCode::Down => {
                    self.move_tag(1);
                    return Ok(());
                }
                KeyCode::Char('k') | KeyCode::Up => {
                    self.move_tag(-1);
                    return Ok(());
                }
                KeyCode::Char('g') | KeyCode::Home => {
                    self.tag_cursor = 0;
                    return Ok(());
                }
                KeyCode::Char('G') | KeyCode::End => {
                    self.tag_cursor = self.tags().len().saturating_sub(1);
                    return Ok(());
                }
                KeyCode::Enter | KeyCode::Char(' ') => {
                    self.open_focused_tag();
                    return Ok(());
                }
                KeyCode::Char('e') => {
                    self.start_tag_rename();
                    return Ok(());
                }
                // Global keys still work, but habit actions must not fire on a
                // row that is not on screen.
                KeyCode::Tab
                | KeyCode::Char('T')
                | KeyCode::Char('m')
                | KeyCode::Char('b')
                | KeyCode::Char('r')
                | KeyCode::Char('u')
                | KeyCode::Char('/')
                | KeyCode::Char('?')
                | KeyCode::Char('q') => {}
                KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {}
                KeyCode::Esc => {
                    self.select_tab(Tab::Habits);
                    return Ok(());
                }
                _ => return Ok(()),
            }
        }

        let control = key.modifiers.contains(KeyModifiers::CONTROL);
        match key.code {
            KeyCode::Char('c') if control => self.quit = true,
            KeyCode::Char('q') => self.quit = true,
            KeyCode::Esc => {
                if !self.filter.is_empty() {
                    self.clear_filter();
                } else if self.tag_focus.is_some() {
                    self.tag_focus = None;
                    self.cursor = 0;
                } else if self.tab != Tab::Habits {
                    self.tab = Tab::Habits;
                    self.cursor = 0;
                    self.clamp_cursor();
                } else {
                    self.quit = true;
                }
            }
            KeyCode::Char('?') => self.mode = Mode::Help,
            KeyCode::Char('j') | KeyCode::Down => self.move_cursor(1),
            KeyCode::Char('k') | KeyCode::Up => self.move_cursor(-1),
            KeyCode::Char('g') | KeyCode::Home => self.cursor = 0,
            KeyCode::Char('G') | KeyCode::End => self.cursor = usize::MAX,
            KeyCode::Char(' ') => self.toggle_selected(),
            KeyCode::Char('c') => self.toggle_cheat_selected(),
            KeyCode::Char('[') => self.shift_day(-1),
            KeyCode::Char(']') => self.shift_day(1),
            KeyCode::Char('t') => {
                self.view_date = self.today;
                self.flash_ok("back to today");
            }
            KeyCode::Char('a') => self.mode = Mode::Form(Box::new(Form::new(self.today))),
            KeyCode::Char('e') => {
                if let Some(habit) = self.selected_habit() {
                    self.mode = Mode::Form(Box::new(Form::from_habit(habit)));
                }
            }
            KeyCode::Char('d') => self.toggle_archive_selected(),
            KeyCode::Char('x') => self.confirm_delete_selected(),
            KeyCode::Char('u') => self.undo(),
            KeyCode::Char('/') => {
                self.filtering = true;
                self.filter.clear();
                self.cursor = 0;
            }
            KeyCode::Tab => self.next_tab(),
            KeyCode::Char('n') => self.start_note(),
            KeyCode::Char('b') => self.show_footer = !self.show_footer,
            KeyCode::Char('T') => self.cycle_theme(),
            KeyCode::Char('m') => self.toggle_mode(),
            KeyCode::Char('r') => self.reload(),
            _ => {}
        }
        self.clamp_cursor();
        Ok(())
    }

    fn filter_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => self.clear_filter(),
            KeyCode::Enter => self.filtering = false,
            KeyCode::Backspace => {
                self.filter.pop();
                self.cursor = 0;
            }
            KeyCode::Char(character)
                if !key.modifiers.contains(KeyModifiers::CONTROL)
                    && !key.modifiers.contains(KeyModifiers::ALT) =>
            {
                self.filter.push(character);
                self.cursor = 0;
            }
            _ => {}
        }
        self.clamp_cursor();
    }

    fn clear_filter(&mut self) {
        self.filter.clear();
        self.filtering = false;
        self.cursor = 0;
        self.clamp_cursor();
    }

    fn confirm_key(&mut self, key: KeyEvent, confirm: Confirm) {
        match key.code {
            KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Enter => match confirm.action {
                ConfirmAction::Delete(id) => self.delete_habit(&id),
                ConfirmAction::MergeTag { from, to } => self.apply_tag_rename(&from, &to),
            },
            _ => {}
        }
    }

    pub fn move_cursor(&mut self, delta: i32) {
        let length = self.visible_indices().len();
        if length == 0 {
            self.cursor = 0;
            return;
        }
        let current = self.cursor.min(length - 1) as i32;
        self.cursor = (current + delta).clamp(0, length as i32 - 1) as usize;
    }

    fn clamp_cursor(&mut self) {
        let length = self.visible_indices().len();
        self.cursor = if length == 0 {
            0
        } else {
            self.cursor.min(length - 1)
        };
    }

    pub fn shift_day(&mut self, delta: i64) {
        let moved = self.view_date + chrono::Duration::days(delta);
        if moved > self.today {
            self.flash_warning("that day is in the future".to_string());
            return;
        }
        self.view_date = moved;
        let label = self.view_date.format("%a %-d %b").to_string();
        if self.view_date == self.today {
            self.flash_ok("today");
        } else {
            self.flash_ok(format!("viewing {label}"));
        }
    }

    fn day_prefix(&self, date: NaiveDate) -> String {
        if date == self.today {
            String::new()
        } else {
            format!("{} · ", date.format("%a %-d %b"))
        }
    }

    fn toggle_selected(&mut self) {
        let Some(index) = self.selected() else {
            self.flash_err("nothing selected");
            return;
        };
        let habit = self.data.habits[index].clone();
        let date = self.view_date;
        if date > self.today {
            self.flash_err("that day is in the future");
            return;
        }
        if !habit.is_active_on(date) {
            self.flash_err(format!(
                "{} was not running on {date} ({})",
                habit.name,
                habit.window_label()
            ));
            return;
        }
        let mut now_done = false;
        self.mutate(|data| {
            now_done = data.log.toggle(date, &habit.id);
        });

        if now_done {
            let progress =
                stats::weekly_progress(&habit, &self.data.history(), self.today, self.week_start);
            let suffix = match progress {
                Some((done, target)) => format!("{done}/{target} this week"),
                None => {
                    let streak = stats::current_streak(
                        &habit,
                        &self.data.history(),
                        self.today,
                        self.week_start,
                    );
                    format!("streak {}", streak.label())
                }
            };
            self.flash_ok(format!(
                "✓ {}{} · {suffix} · n adds a note",
                self.day_prefix(date),
                habit.name
            ));
        } else {
            self.flash_ok(format!("✗ {}{} undone", self.day_prefix(date), habit.name));
        }
    }

    fn toggle_cheat_selected(&mut self) {
        let Some(index) = self.selected() else {
            self.flash_err("nothing selected");
            return;
        };
        let habit = self.data.habits[index].clone();
        let date = self.view_date;
        if date > self.today {
            self.flash_err("cannot excuse a future day");
            return;
        }
        if !habit.is_active_on(date) {
            self.flash_err(format!(
                "{} was not running on {date} ({})",
                habit.name,
                habit.window_label()
            ));
            return;
        }
        let mut now_cheat = false;
        self.mutate(|data| {
            now_cheat = data.cheats.toggle(date, &habit.id);
        });
        if !now_cheat {
            self.flash_ok(format!(
                "◆ {}{} cheat day removed",
                self.day_prefix(date),
                habit.name
            ));
            return;
        }
        let excused = self.data.cheats.is_excused(date, &habit.id);
        let used = self.data.cheats.used_in_month(&habit.id, date);
        let month = date.format("%B").to_string();
        let prefix = self.day_prefix(date);
        match (used, excused) {
            (1, _) => self.flash_ok(format!(
                "◆ {prefix}{} excused · 1/1 cheat day in {month}",
                habit.name
            )),
            (_, true) => self.flash_warning(format!(
                "◆ {prefix}{} excused, but {used} cheats in {month}: later ones break the streak",
                habit.name
            )),
            (_, false) => self.flash_warning(format!(
                "◆ {prefix}{} breaks the streak · {used} cheats in {month}, only the earliest counts",
                habit.name
            )),
        }
    }

    fn toggle_archive_selected(&mut self) {
        let Some(index) = self.selected() else {
            self.flash_err("nothing selected");
            return;
        };
        let habit = self.data.habits[index].clone();
        self.mutate(|data| {
            if let Some(habit) = data.habits.get_mut(index) {
                habit.archived = !habit.archived;
            }
        });
        let archived = self.data.habits[index].archived;
        if archived {
            self.flash_ok(format!("archived {} · d restores", habit.name));
        } else {
            self.flash_ok(format!("restored {}", habit.name));
        }
    }

    fn confirm_delete_selected(&mut self) {
        let Some(index) = self.selected() else {
            self.flash_err("nothing selected");
            return;
        };
        let habit = &self.data.habits[index];
        self.mode = Mode::Confirm(Confirm {
            prompt: format!("Delete \"{}\" and its whole history?", habit.name),
            action: ConfirmAction::Delete(habit.id.clone()),
        });
    }

    fn delete_habit(&mut self, id: &str) {
        let Some(index) = self.data.habits.iter().position(|habit| habit.id == id) else {
            self.flash_err("that habit is already gone");
            return;
        };
        let name = self.data.habits[index].name.clone();
        let id = id.to_string();
        self.mutate(move |data| {
            if let Some(index) = data.habits.iter().position(|habit| habit.id == id) {
                data.remove_habit(index);
            }
        });
        self.flash_ok(format!("deleted {name} · u undoes"));
    }

    /// Called after a form save: keep installed units in step, or explain the
    /// one-time setup the first time a reminder is switched on.
    fn after_reminder_change(&mut self, newly_enabled: bool) {
        let installed =
            reminders::any_installed(reminders::platform(), &reminders::REMINDER_PREFIXES);
        if installed {
            match self.sync_reminders() {
                Ok(count) => self.flash_ok(format!(
                    "reminders scheduled · {}",
                    crate::format::plural(count as u32, "time")
                )),
                Err(error) => {
                    self.flash_warning(format!("reminder saved, but scheduling failed: {error:#}"))
                }
            }
        } else if newly_enabled {
            self.mode = Mode::ReminderSetup;
        }
    }

    fn sync_reminders(&mut self) -> Result<usize> {
        let platform = reminders::platform();
        let times = reminders::times(&self.data, self.today);
        let program = std::env::current_exe().context("finding the habit binary")?;
        let data = std::path::absolute(self.store.path())
            .unwrap_or_else(|_| self.store.path().to_path_buf());
        let units = reminders::reminder_units(platform, &program, &data, &times)?;
        reminders::sync(platform, &units, &reminders::REMINDER_PREFIXES)?;
        Ok(times.len())
    }

    fn finish_reminder_setup(&mut self, install: bool) {
        if install {
            match self.sync_reminders() {
                Ok(0) => self.flash_warning("no reminder times to schedule".into()),
                Ok(count) => self.flash_ok(format!(
                    "reminders scheduled · {}",
                    crate::format::plural(count as u32, "time")
                )),
                Err(error) => self.flash_err(format!("could not schedule: {error:#}")),
            }
        } else {
            self.flash_warning(
                "reminder saved · run `habit schedule install` to turn notifications on".into(),
            );
        }
    }

    fn submit_form(&mut self, original: Option<String>, draft: crate::ui::form::Draft) {
        let reminder = draft.reminder;
        if let Err(error) = draft.schedule.validate() {
            self.flash_err(error.to_string());
            return;
        }

        match original {
            Some(id) => {
                let Some(index) = self.data.habits.iter().position(|habit| habit.id == id) else {
                    self.flash_err("that habit no longer exists");
                    return;
                };
                if let Some(other) =
                    self.data.habits.iter().find(|habit| {
                        habit.id != id && habit.name.eq_ignore_ascii_case(&draft.name)
                    })
                {
                    self.flash_err(format!("a habit named \"{}\" already exists", other.name));
                    return;
                }
                let was_enabled = self.data.habits[index].reminder.is_some();
                let mut updated = self.data.habits[index].clone();
                updated.name = draft.name.clone();
                updated.schedule = draft.schedule;
                updated.created = draft.start;
                updated.ends = draft.ends;
                updated.note = draft.note;
                updated.reminder = reminder;
                updated.tags = draft.tags;
                if let Err(error) = updated.validate() {
                    self.flash_err(error.to_string());
                    return;
                }
                let display = updated.name.clone();
                self.mutate(move |data| {
                    if let Some(habit) = data.habits.get_mut(index) {
                        *habit = updated;
                    }
                });
                self.flash_ok(format!("updated {display}"));
                self.after_reminder_change(!was_enabled && reminder.is_some());
            }
            None => {
                let before = self.data.clone();
                match self.data.add_habit_full(
                    &draft.name,
                    draft.schedule,
                    draft.start,
                    draft.ends,
                    &draft.tags,
                    draft.note.as_deref(),
                ) {
                    Ok(index) => {
                        self.data.habits[index].reminder = reminder;
                        let id = self.data.habits[index].id.clone();
                        let display = self.data.habits[index].name.clone();
                        if self.commit(before) {
                            if let Some(position) = self
                                .visible_indices()
                                .iter()
                                .position(|candidate| self.data.habits[*candidate].id == id)
                            {
                                self.cursor = position;
                            }
                            self.flash_ok(format!("added {display} · space checks off"));
                            self.after_reminder_change(reminder.is_some());
                        }
                    }
                    Err(error) => {
                        self.data = before;
                        self.flash_err(error.to_string());
                    }
                }
            }
        }
    }

    /// Apply a change, persist it, and keep a one-step undo snapshot.
    fn mutate(&mut self, change: impl FnOnce(&mut Data)) {
        let before = self.data.clone();
        change(&mut self.data);
        self.commit(before);
    }

    fn commit(&mut self, before: Data) -> bool {
        match self.store.save(&self.data) {
            Ok(()) => {
                self.undo = Some(before);
                self.clamp_cursor();
                true
            }
            Err(error) => {
                self.data = before;
                self.flash_err(format!("not saved: {error:#}"));
                false
            }
        }
    }

    fn undo(&mut self) {
        let Some(previous) = self.undo.clone() else {
            self.flash_err("nothing to undo");
            return;
        };
        let current = self.data.clone();
        self.data = previous;
        match self.store.save(&self.data) {
            Ok(()) => {
                self.undo = Some(current);
                self.clamp_cursor();
                self.flash_ok("↶ undone");
            }
            Err(error) => {
                self.data = current;
                self.flash_err(format!("not saved: {error:#}"));
            }
        }
    }

    fn reload(&mut self) {
        match self.store.load() {
            Ok(data) => {
                self.data = data;
                self.clamp_cursor();
                self.flash_ok("reloaded from disk");
            }
            Err(error) => self.flash_err(format!("reload failed: {error:#}")),
        }
    }

    /// `T`: step to the next palette, keeping the current light/dark mode.
    fn cycle_theme(&mut self) {
        let all = ThemeName::ALL;
        let index = all
            .iter()
            .position(|name| *name == self.config.theme)
            .unwrap_or(0);
        self.config.theme = all[(index + 1) % all.len()];
        self.apply_theme();
    }

    /// `m`: flip between the dark and light variants of the palette.
    fn toggle_mode(&mut self) {
        self.config.mode = match self.config.mode {
            ColorMode::Dark => ColorMode::Light,
            ColorMode::Light => ColorMode::Dark,
        };
        self.apply_theme();
    }

    /// Rebuild the palette and write the choice back to `config.toml` so it
    /// survives a restart. A read-only home directory only costs a warning.
    fn apply_theme(&mut self) {
        self.theme = self.config.palette();
        let label = format!(
            "{} · {}",
            self.config.theme.label(),
            self.config.mode.label()
        );
        match self.config.save() {
            Ok(()) => self.flash_ok(format!("theme {label} · saved")),
            Err(error) => self.flash_warning(format!("theme {label} · not saved: {error}")),
        }
    }

    fn flash_ok(&mut self, text: impl Into<String>) {
        self.flash = Some(Flash {
            text: text.into(),
            error: false,
            warn: false,
            at: Instant::now(),
        });
    }

    fn flash_err(&mut self, text: impl Into<String>) {
        self.flash = Some(Flash {
            text: text.into(),
            error: true,
            warn: false,
            at: Instant::now(),
        });
    }

    pub fn tick(&mut self) {
        if let Some(flash) = &self.flash
            && flash.at.elapsed() > Duration::from_secs(5)
        {
            self.flash = None;
        }
    }
}

fn rect_contains(rect: Rect, column: u16, row: u16) -> bool {
    column >= rect.x
        && column < rect.x.saturating_add(rect.width)
        && row >= rect.y
        && row < rect.y.saturating_add(rect.height)
}

/// `#tag` filters tags; every other token matches name, id, tags or the note.
fn matches_query(habit: &Habit, query: &str) -> bool {
    query.split_whitespace().all(|token| {
        if let Some(tag) = token.strip_prefix('#') {
            return habit.tags.iter().any(|candidate| candidate.contains(tag));
        }
        habit.name.to_lowercase().contains(token)
            || habit.id.contains(token)
            || habit.tags.iter().any(|tag| tag.contains(token))
            || habit
                .note
                .as_deref()
                .is_some_and(|note| note.to_lowercase().contains(token))
    })
}

/// `YYYY-MM-DD`, rejecting impossible dates.
pub fn parse_date(text: &str) -> Option<NaiveDate> {
    let text = text.trim();
    NaiveDate::parse_from_str(text, "%Y-%m-%d")
        .ok()
        .or_else(|| {
            NaiveDateTime::parse_from_str(text, "%Y-%m-%d %H:%M")
                .ok()
                .map(|dt| dt.date())
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Schedule;

    fn date(text: &str) -> NaiveDate {
        NaiveDate::parse_from_str(text, "%Y-%m-%d").unwrap()
    }

    fn app() -> App {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::resolve(Some(dir.path().join("data.json"))).unwrap();
        let mut data = Data::default();
        data.add_habit("Gym", Schedule::Daily, date("2026-11-01"))
            .unwrap();
        data.habits[0].tags = vec!["health".into(), "morning".into()];
        App::new(store, data, date("2026-11-10"), &Config::default())
    }

    #[test]
    fn filter_matches_names_tags_and_notes() {
        let mut app = app();
        app.filter = "gym".into();
        assert_eq!(app.visible_indices(), vec![0]);
        app.filter = "#health".into();
        assert_eq!(app.visible_indices(), vec![0]);
        app.filter = "#nope".into();
        assert!(app.visible_indices().is_empty());
        app.data.habits[0].note = Some("bad sleep".into());
        app.filter = "sleep".into();
        assert_eq!(app.visible_indices(), vec![0]);
        app.filter = "gym #morning".into();
        assert_eq!(app.visible_indices(), vec![0]);
        app.filter = "gym #evening".into();
        assert!(app.visible_indices().is_empty());
    }

    #[test]
    fn tag_counts_ignore_archived_and_finished_habits() {
        let mut app = app();
        app.data.habits[0].tags = vec!["health".into(), "morning".into()];
        let index = app
            .data
            .add_habit("Read", Schedule::Daily, date("2026-11-01"))
            .unwrap();
        app.data.habits[index].tags = vec!["health".into(), "mind".into()];
        app.data.habits[index].archived = true;
        let index = app
            .data
            .add_habit("Run", Schedule::Daily, date("2026-11-01"))
            .unwrap();
        app.data.habits[index].tags = vec!["health".into()];
        app.data.habits[index].ends = Some(date("2026-11-05"));

        assert_eq!(
            app.tags(),
            vec![
                ("health".to_string(), 1),
                ("mind".to_string(), 0),
                ("morning".to_string(), 1)
            ]
        );
    }

    #[test]
    fn a_tag_view_lists_active_habits_then_an_archive_section() {
        let mut app = app();
        app.data.habits[0].tags = vec!["health".into()];
        let index = app
            .data
            .add_habit("Read", Schedule::Daily, date("2026-11-01"))
            .unwrap();
        app.data.habits[index].tags = vec!["health".into()];
        app.data.habits[index].archived = true;

        app.select_tab(Tab::Tags);
        app.tag_cursor = 0;
        app.open_focused_tag();
        assert_eq!(app.tag_focus.as_deref(), Some("health"));
        assert_eq!(
            app.view_rows(),
            vec![ListRow::Habit(0), ListRow::Archive, ListRow::Habit(index)]
        );
        assert_eq!(app.visible_indices(), vec![0, index]);
    }

    #[test]
    fn habit_actions_are_ignored_in_the_tag_list() {
        let mut app = app();
        app.select_tab(Tab::Tags);
        app.handle_key(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE))
            .unwrap();
        assert!(matches!(app.mode, Mode::Normal));
        app.handle_key(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE))
            .unwrap();
        assert!(!app.data.log.is_done(date("2026-11-10"), "gym"));
        app.handle_key(KeyEvent::new(KeyCode::Char('b'), KeyModifiers::NONE))
            .unwrap();
        assert!(!app.show_footer);
    }

    #[test]
    fn escape_backs_out_of_the_tag_view_instead_of_quitting() {
        let mut app = app();
        app.data.habits[0].tags = vec!["health".into()];
        app.select_tab(Tab::Tags);
        app.open_focused_tag();
        assert!(app.tag_focus.is_some());
        app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE))
            .unwrap();
        assert!(app.tag_focus.is_none());
        assert!(!app.quit);
        app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE))
            .unwrap();
        assert_eq!(app.tab, Tab::Habits);
        assert!(!app.quit);
        app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE))
            .unwrap();
        assert!(app.quit);
    }

    #[test]
    fn the_habits_tab_puts_habits_that_start_later_under_a_heading() {
        let mut app = app();
        app.data
            .add_habit("Read", Schedule::Daily, date("2026-11-20"))
            .unwrap();
        app.data
            .add_habit("Old", Schedule::Daily, date("2026-10-01"))
            .unwrap();
        app.data.habits[2].archived = true;

        assert_eq!(
            app.view_rows(),
            vec![
                ListRow::Habit(0),
                ListRow::Heading("Starting later"),
                ListRow::Habit(1),
            ]
        );
        // The heading is a label, not a selection.
        assert_eq!(app.visible_indices(), vec![0, 1]);
        app.cursor = 1;
        assert_eq!(app.selected(), Some(1));
    }

    #[test]
    fn the_archive_tab_splits_archived_habits_from_ended_ones() {
        let mut app = app();
        app.data
            .add_habit("Read", Schedule::Daily, date("2026-11-01"))
            .unwrap();
        app.data.habits[0].archived = true;
        app.data.habits[1].ends = Some(date("2026-11-05"));

        app.select_tab(Tab::Archive);
        assert_eq!(
            app.view_rows(),
            vec![
                ListRow::Heading("Archived"),
                ListRow::Habit(0),
                ListRow::Heading("Ended"),
                ListRow::Habit(1),
            ]
        );
        assert_eq!(app.visible_indices(), vec![0, 1]);
    }

    #[test]
    fn archiving_from_the_habits_tab_moves_the_habit_to_the_archive() {
        let mut app = app();
        press(&mut app, KeyCode::Char('d'));
        assert!(app.data.habits[0].archived);
        assert!(app.view_rows().is_empty());
        assert_eq!(
            app.flash.as_ref().map(|flash| flash.text.as_str()),
            Some("archived Gym · d restores")
        );

        app.select_tab(Tab::Archive);
        assert_eq!(
            app.view_rows(),
            vec![ListRow::Heading("Archived"), ListRow::Habit(0)]
        );

        // Restoring from the archive sends it back to the habits tab.
        press(&mut app, KeyCode::Char('d'));
        assert!(!app.data.habits[0].archived);
        assert!(app.view_rows().is_empty());
        app.select_tab(Tab::Habits);
        assert_eq!(app.view_rows(), vec![ListRow::Habit(0)]);
    }

    #[test]
    fn esc_leaves_the_archive_for_the_habits_tab() {
        let mut app = app();
        app.select_tab(Tab::Archive);
        press(&mut app, KeyCode::Esc);
        assert_eq!(app.tab, Tab::Habits);
        assert!(!app.quit);
        press(&mut app, KeyCode::Esc);
        assert!(app.quit);
    }

    #[test]
    fn clicking_a_heading_does_not_move_the_selection() {
        let mut app = app();
        app.data
            .add_habit("Read", Schedule::Daily, date("2026-11-20"))
            .unwrap();
        app.hits.list = Some(Rect::new(0, 5, 40, 10));
        app.handle_mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 2,
            row: 6, // the "Starting later" row
            modifiers: KeyModifiers::NONE,
        });
        assert_eq!(app.cursor, 0);

        app.handle_mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 2,
            row: 7,
            modifiers: KeyModifiers::NONE,
        });
        assert_eq!(app.cursor, 1);
    }

    #[test]
    fn a_habit_that_starts_later_can_be_filtered_for() {
        let mut app = app();
        app.data
            .add_habit("Read", Schedule::Daily, date("2026-11-20"))
            .unwrap();
        app.filter = "read".into();
        assert_eq!(
            app.view_rows(),
            vec![ListRow::Heading("Starting later"), ListRow::Habit(1)]
        );
        assert_eq!(app.visible_indices(), vec![1]);
    }

    #[test]
    fn choosing_later_keeps_the_reminder_and_explains_the_schedule_command() {
        let mut app = app();
        app.data.habits[0].reminder = Some(chrono::NaiveTime::from_hms_opt(9, 0, 0).unwrap());
        app.mode = Mode::ReminderSetup;
        press(&mut app, KeyCode::Char('n'));
        assert!(matches!(app.mode, Mode::Normal));
        assert!(app.data.habits[0].reminder.is_some());
        assert!(
            app.flash
                .as_ref()
                .is_some_and(|flash| flash.text.contains("habit schedule install")),
            "{:?}",
            app.flash.as_ref().map(|flash| &flash.text)
        );
    }

    #[test]
    fn tab_and_enter_navigate_the_tag_view() {
        let mut app = app();
        app.data.habits[0].tags = vec!["health".into()];
        app.handle_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE))
            .unwrap();
        assert_eq!(app.tab, Tab::Tags);
        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
            .unwrap();
        assert_eq!(app.tag_focus.as_deref(), Some("health"));
        app.handle_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE))
            .unwrap();
        assert_eq!(app.tab, Tab::Archive);
        assert!(app.tag_focus.is_none());
        app.handle_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE))
            .unwrap();
        assert_eq!(app.tab, Tab::Habits);
        assert!(app.tag_focus.is_none());
    }

    #[test]
    fn n_opens_a_note_for_the_selected_day_and_enter_saves_it() {
        let mut app = app();
        app.handle_key(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE))
            .unwrap();
        assert!(matches!(app.mode, Mode::Note(_)));
        for character in "felt strong".chars() {
            app.handle_key(KeyEvent::new(KeyCode::Char(character), KeyModifiers::NONE))
                .unwrap();
        }
        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
            .unwrap();
        assert!(matches!(app.mode, Mode::Normal));
        assert_eq!(
            app.data.habits[0].note_on(date("2026-11-10")),
            Some("felt strong")
        );
        assert!(app.data.habits[0].notes.contains_key(&date("2026-11-10")));
    }

    #[test]
    fn n_prefills_the_existing_note_for_editing() {
        let mut app = app();
        app.data.habits[0].set_note(date("2026-11-09"), "tired");
        app.shift_day(-1);
        app.handle_key(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE))
            .unwrap();
        let Mode::Note(entry) = &app.mode else {
            panic!("expected note mode");
        };
        assert_eq!(entry.input.text(), "tired");
        assert_eq!(entry.date_input.text(), "2026-11-09");
        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
            .unwrap();
        assert_eq!(
            app.data.habits[0].note_on(date("2026-11-09")),
            Some("tired")
        );
    }

    /// Focus the date field, clear it and type a new date.
    fn set_note_date(app: &mut App, text: &str) {
        app.handle_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE))
            .unwrap();
        app.handle_key(KeyEvent::new(KeyCode::Home, KeyModifiers::NONE))
            .unwrap();
        for _ in 0..12 {
            app.handle_key(KeyEvent::new(KeyCode::Delete, KeyModifiers::NONE))
                .unwrap();
        }
        for character in text.chars() {
            app.handle_key(KeyEvent::new(KeyCode::Char(character), KeyModifiers::NONE))
                .unwrap();
        }
    }

    fn press(app: &mut App, code: KeyCode) {
        app.handle_key(KeyEvent::new(code, KeyModifiers::NONE))
            .unwrap();
    }

    fn type_text(app: &mut App, text: &str) {
        for character in text.chars() {
            press(app, KeyCode::Char(character));
        }
    }

    /// Habits tab → Tags tab → the rename prompt on the tag under the cursor.
    fn start_rename(app: &mut App) {
        press(app, KeyCode::Tab);
        press(app, KeyCode::Char('e'));
    }

    #[test]
    fn the_rename_prompt_renames_the_tag_under_the_cursor() {
        let mut app = app();
        start_rename(&mut app);
        let Mode::Tag(entry) = &app.mode else {
            panic!("expected the rename prompt");
        };
        assert_eq!(entry.from, "health");
        assert!(entry.error.is_none());

        type_text(&mut app, "Fitness");
        press(&mut app, KeyCode::Enter);

        assert!(matches!(app.mode, Mode::Normal));
        assert_eq!(app.data.habits[0].tags, vec!["fitness", "morning"]);
        assert!(app.flash.as_ref().is_some_and(|flash| !flash.error));
        assert_eq!(
            app.flash.as_ref().map(|flash| flash.text.as_str()),
            Some("renamed #health to #fitness · 1 habit")
        );
        // The cursor stays on the renamed tag now that the list is sorted.
        assert_eq!(app.tags()[app.tag_cursor].0, "fitness");
    }

    #[test]
    fn renaming_onto_a_tag_in_use_asks_before_merging() {
        let mut app = app();
        app.data.habits[0].tags = vec!["body".into(), "health".into()];
        press(&mut app, KeyCode::Tab);
        // body sorts first, so walk down to health.
        press(&mut app, KeyCode::Char('j'));
        press(&mut app, KeyCode::Char('e'));
        type_text(&mut app, "body");
        press(&mut app, KeyCode::Enter);

        let Mode::Confirm(confirm) = &app.mode else {
            panic!("merging should ask first");
        };
        assert_eq!(confirm.prompt, "Merge #health into #body?");

        // Saying no leaves everything alone.
        press(&mut app, KeyCode::Char('n'));
        assert!(matches!(app.mode, Mode::Normal));
        assert_eq!(app.data.habits[0].tags, vec!["body", "health"]);

        press(&mut app, KeyCode::Char('e'));
        type_text(&mut app, "body");
        press(&mut app, KeyCode::Enter);
        press(&mut app, KeyCode::Char('y'));

        assert!(matches!(app.mode, Mode::Normal));
        assert_eq!(app.data.habits[0].tags, vec!["body"]);
        assert_eq!(app.tags(), vec![("body".to_string(), 1)]);
        assert_eq!(
            app.flash.as_ref().map(|flash| flash.text.as_str()),
            Some("merged #health into #body · 1 habit")
        );
    }

    #[test]
    fn a_bad_tag_name_keeps_the_prompt_open() {
        let mut app = app();
        start_rename(&mut app);
        press(&mut app, KeyCode::Enter);
        let Mode::Tag(entry) = &app.mode else {
            panic!("an empty name should not close the prompt");
        };
        assert_eq!(
            entry.error.as_deref(),
            Some("a tag needs at least one letter or digit")
        );

        type_text(&mut app, "!!!");
        press(&mut app, KeyCode::Enter);
        let Mode::Tag(entry) = &app.mode else {
            panic!("a nameless tag should not close the prompt");
        };
        assert!(entry.error.is_some());

        // Typing again clears the complaint.
        type_text(&mut app, "he");
        let Mode::Tag(entry) = &app.mode else {
            panic!("still open");
        };
        assert!(entry.error.is_none());

        // The old name itself is refused.
        press(&mut app, KeyCode::Home);
        for _ in 0..8 {
            press(&mut app, KeyCode::Delete);
        }
        type_text(&mut app, "health");
        press(&mut app, KeyCode::Enter);
        let Mode::Tag(entry) = &app.mode else {
            panic!("the same name should not close the prompt");
        };
        assert_eq!(
            entry.error.as_deref(),
            Some("that is already the tag's name")
        );
        assert_eq!(app.data.habits[0].tags, vec!["health", "morning"]);
    }

    #[test]
    fn esc_leaves_the_rename_prompt_without_touching_the_data() {
        let mut app = app();
        start_rename(&mut app);
        type_text(&mut app, "fitness");
        press(&mut app, KeyCode::Esc);
        assert!(matches!(app.mode, Mode::Normal));
        assert_eq!(app.data.habits[0].tags, vec!["health", "morning"]);
    }

    #[test]
    fn a_rename_can_be_undone() {
        let mut app = app();
        start_rename(&mut app);
        type_text(&mut app, "fitness");
        press(&mut app, KeyCode::Enter);
        assert_eq!(app.data.habits[0].tags, vec!["fitness", "morning"]);

        press(&mut app, KeyCode::Char('u'));
        assert_eq!(app.data.habits[0].tags, vec!["health", "morning"]);
    }

    #[test]
    fn the_rename_prompt_needs_a_tag() {
        let mut app = App::new(
            Store::resolve(None).unwrap(),
            Data::default(),
            date("2026-11-10"),
            &Config::default(),
        );
        press(&mut app, KeyCode::Tab);
        press(&mut app, KeyCode::Char('e'));
        assert!(matches!(app.mode, Mode::Normal));
        assert_eq!(
            app.flash.as_ref().map(|flash| flash.text.as_str()),
            Some("no tags yet")
        );
    }

    #[test]
    fn a_note_can_be_written_for_another_day_from_the_dialog() {
        let mut app = app();
        app.handle_key(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE))
            .unwrap();
        set_note_date(&mut app, "2026-11-03");
        // Tab back to the note text, write it and save.
        app.handle_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE))
            .unwrap();
        for character in "backfilled".chars() {
            app.handle_key(KeyEvent::new(KeyCode::Char(character), KeyModifiers::NONE))
                .unwrap();
        }
        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
            .unwrap();
        assert!(matches!(app.mode, Mode::Normal));
        assert_eq!(
            app.data.habits[0].note_on(date("2026-11-03")),
            Some("backfilled")
        );
        assert!(!app.data.habits[0].notes.contains_key(&date("2026-11-10")));
    }

    #[test]
    fn arrow_keys_walk_the_note_date_one_day_at_a_time() {
        let mut app = app();
        app.handle_key(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE))
            .unwrap();
        app.handle_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE))
            .unwrap();
        app.handle_key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE))
            .unwrap();
        let Mode::Note(entry) = &app.mode else {
            panic!("expected note mode");
        };
        assert_eq!(entry.date_input.text(), "2026-11-09");
        app.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE))
            .unwrap();
        let Mode::Note(entry) = &app.mode else {
            panic!("expected note mode");
        };
        assert_eq!(entry.date_input.text(), "2026-11-10");
    }

    #[test]
    fn a_note_dated_in_the_future_is_refused() {
        let mut app = app();
        app.handle_key(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE))
            .unwrap();
        set_note_date(&mut app, "2026-11-11");
        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
            .unwrap();
        let Mode::Note(entry) = &app.mode else {
            panic!("the dialog should stay open");
        };
        assert!(
            entry.error.as_deref().unwrap_or("").contains("future"),
            "{:?}",
            entry.error
        );
        assert!(app.data.habits[0].notes.is_empty());
    }

    #[test]
    fn a_note_dated_before_the_habit_started_is_refused() {
        let mut app = app();
        app.handle_key(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE))
            .unwrap();
        set_note_date(&mut app, "2026-10-31");
        for character in "too early".chars() {
            app.handle_key(KeyEvent::new(KeyCode::Char(character), KeyModifiers::NONE))
                .unwrap();
        }
        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
            .unwrap();
        let Mode::Note(entry) = &app.mode else {
            panic!("the dialog should stay open");
        };
        assert!(
            entry.error.as_deref().unwrap_or("").contains("2026-11-01"),
            "{:?}",
            entry.error
        );
        assert!(app.data.habits[0].notes.is_empty());
    }

    #[test]
    fn clicking_the_tags_tab_and_a_tag_row_opens_that_tag() {
        let mut app = app();
        app.data.habits[0].tags = vec!["health".into()];
        app.hits.tabs = vec![(Tab::Tags, Rect::new(0, 0, 6, 1))];
        app.handle_mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 1,
            row: 0,
            modifiers: KeyModifiers::NONE,
        });
        assert_eq!(app.tab, Tab::Tags);

        app.hits.list = Some(Rect::new(0, 4, 40, 10));
        app.tag_cursor = 0;
        app.handle_mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 2,
            row: 4,
            modifiers: KeyModifiers::NONE,
        });
        assert_eq!(app.tag_focus.as_deref(), Some("health"));
    }

    #[test]
    fn clicking_a_habit_row_selects_it() {
        let mut app = app();
        app.data
            .add_habit("Read", Schedule::Daily, date("2026-11-01"))
            .unwrap();
        app.hits.list = Some(Rect::new(0, 5, 40, 10));
        app.handle_mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 3,
            row: 6,
            modifiers: KeyModifiers::NONE,
        });
        assert_eq!(app.cursor, 1);
    }

    #[test]
    fn b_toggles_the_bottom_bar() {
        let mut app = app();
        assert!(app.show_footer);
        app.handle_key(KeyEvent::new(KeyCode::Char('b'), KeyModifiers::NONE))
            .unwrap();
        assert!(!app.show_footer);
        app.handle_key(KeyEvent::new(KeyCode::Char('b'), KeyModifiers::NONE))
            .unwrap();
        assert!(app.show_footer);
    }

    #[test]
    fn q_quits_even_when_a_filter_is_active() {
        let mut app = app();
        app.filter = "gym".into();
        app.handle_key(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE))
            .unwrap();
        assert!(app.quit);
    }

    #[test]
    fn t_cycles_the_palette_and_m_flips_the_mode() {
        let mut app = app();
        let first = app.config.theme;
        app.handle_key(KeyEvent::new(KeyCode::Char('T'), KeyModifiers::SHIFT))
            .unwrap();
        assert_ne!(app.config.theme, first);
        assert_eq!(app.theme.name, app.config.theme);

        // Every palette is reachable and wraps back to the first one.
        for _ in 0..ThemeName::ALL.len() - 1 {
            app.handle_key(KeyEvent::new(KeyCode::Char('T'), KeyModifiers::SHIFT))
                .unwrap();
        }
        assert_eq!(app.config.theme, first);

        let mode = app.config.mode;
        app.handle_key(KeyEvent::new(KeyCode::Char('m'), KeyModifiers::NONE))
            .unwrap();
        assert_ne!(app.config.mode, mode);
        assert_eq!(
            app.flash.as_ref().unwrap().text,
            format!(
                "theme {} · {} · not saved: no config path to write to",
                first.label(),
                app.config.mode.label()
            )
        );
    }

    #[test]
    fn date_navigation_stops_at_today() {
        let mut app = app();
        app.shift_day(1);
        assert_eq!(app.view_date, date("2026-11-10"));
        app.shift_day(-2);
        assert_eq!(app.view_date, date("2026-11-08"));
        app.handle_key(KeyEvent::new(KeyCode::Char('t'), KeyModifiers::NONE))
            .unwrap();
        assert_eq!(app.view_date, date("2026-11-10"));
    }

    #[test]
    fn checking_off_a_past_day_uses_the_view_date() {
        let mut app = app();
        app.shift_day(-1);
        app.toggle_selected();
        assert!(app.data.log.is_done(date("2026-11-09"), "gym"));
        assert!(!app.data.log.is_done(date("2026-11-10"), "gym"));
    }

    #[test]
    fn checking_off_outside_the_window_is_refused() {
        let mut app = app();
        app.shift_day(-15);
        app.toggle_selected();
        assert!(!app.data.log.is_done(date("2026-10-26"), "gym"));
        assert!(app.flash.as_ref().unwrap().text.contains("not running"));
    }

    #[test]
    fn cheat_days_are_toggled_on_the_view_date() {
        let mut app = app();
        app.shift_day(-1);
        app.toggle_cheat_selected();
        assert!(app.data.cheats.is_excused(date("2026-11-09"), "gym"));
        app.toggle_cheat_selected();
        assert!(!app.data.cheats.is_cheat(date("2026-11-09"), "gym"));
    }

    #[test]
    fn a_second_cheat_in_the_month_warns_instead_of_failing() {
        let mut app = app();
        app.shift_day(-2);
        app.toggle_cheat_selected();
        app.shift_day(1);
        app.toggle_cheat_selected();
        assert_eq!(app.data.cheats.used_in_month("gym", date("2026-11-10")), 2);
        let flash = app.flash.as_ref().unwrap();
        assert!(flash.warn && !flash.error, "{}", flash.text);
        assert!(flash.text.contains("breaks the streak"), "{}", flash.text);
    }

    #[test]
    fn backfilling_a_cheat_before_an_existing_one_resets_the_allowance() {
        let mut app = app();
        app.shift_day(-1); // the 9th
        app.toggle_cheat_selected();
        app.shift_day(-2); // the 7th, now the earliest of the month
        app.toggle_cheat_selected();
        assert!(app.data.cheats.is_excused(date("2026-11-07"), "gym"));
        assert!(!app.data.cheats.is_excused(date("2026-11-09"), "gym"));
        let flash = app.flash.as_ref().unwrap();
        assert!(flash.warn, "{}", flash.text);
        assert!(flash.text.contains("later ones break"), "{}", flash.text);
    }

    #[test]
    fn date_parsing_accepts_only_real_dates() {
        assert_eq!(parse_date("2026-11-10"), Some(date("2026-11-10")));
        assert_eq!(parse_date(" 2026-11-10 "), Some(date("2026-11-10")));
        assert_eq!(parse_date("2026-02-30"), None);
        assert_eq!(parse_date("tomorrow"), None);
    }
}
