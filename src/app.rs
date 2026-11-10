//! Application state, key handling and persistence-on-change.

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use anyhow::Result;
use chrono::{NaiveDate, NaiveDateTime};
use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::layout::Rect;
use ratatui::widgets::ListState;

use crate::config::Config;
use crate::model::{Data, Day, Habit};
use crate::stats;
use crate::store::Store;
use crate::theme::{ColorMode, Theme, ThemeName};
use crate::ui::form::{Form, FormAction};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Habits,
    Tags,
}

pub enum Mode {
    Normal,
    Help,
    Confirm(Confirm),
    Form(Box<Form>),
    Note(NoteEntry),
}

/// A single dated note being written for one habit.
pub struct NoteEntry {
    pub habit_id: String,
    pub habit_name: String,
    pub date: NaiveDate,
    pub input: crate::ui::form::TextInput,
}

pub enum NoteAction {
    Continue,
    Cancel,
    Save(String),
}

impl NoteEntry {
    pub fn handle_key(&mut self, key: KeyEvent) -> NoteAction {
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            return NoteAction::Cancel;
        }
        match key.code {
            KeyCode::Esc => NoteAction::Cancel,
            KeyCode::Enter => NoteAction::Save(self.input.text()),
            KeyCode::Backspace => {
                self.input.backspace();
                NoteAction::Continue
            }
            KeyCode::Delete => {
                self.input.delete();
                NoteAction::Continue
            }
            KeyCode::Left => {
                self.input.left();
                NoteAction::Continue
            }
            KeyCode::Right => {
                self.input.right();
                NoteAction::Continue
            }
            KeyCode::Home => {
                self.input.home();
                NoteAction::Continue
            }
            KeyCode::End => {
                self.input.end();
                NoteAction::Continue
            }
            KeyCode::Char(character)
                if !key.modifiers.contains(KeyModifiers::CONTROL)
                    && !key.modifiers.contains(KeyModifiers::ALT) =>
            {
                self.input.insert(character);
                NoteAction::Continue
            }
            _ => NoteAction::Continue,
        }
    }
}

/// One visual line in the left pane: an `Archive` divider or a habit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListRow {
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

    /// Visual rows for the left pane, including an `Archive` divider when a
    /// tag view has closed habits.
    pub fn view_rows(&self) -> Vec<ListRow> {
        let query = self.filter.trim().to_lowercase();
        let keep =
            |index: usize| query.is_empty() || matches_query(&self.data.habits[index], &query);

        if self.tab == Tab::Tags {
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
            return rows;
        }

        self.data
            .display_order(self.today)
            .into_iter()
            .filter(|index| keep(*index))
            .map(ListRow::Habit)
            .collect()
    }

    /// Indices into `data.habits`, active first, archived/finished last, filtered.
    pub fn visible_indices(&self) -> Vec<usize> {
        self.view_rows()
            .into_iter()
            .filter_map(|row| match row {
                ListRow::Habit(index) => Some(index),
                ListRow::Archive => None,
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
            Tab::Tags => Tab::Habits,
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

    fn start_note(&mut self) {
        let Some(habit) = self.selected_habit() else {
            self.flash_err("select a habit first");
            return;
        };
        if self.view_date > self.today {
            self.flash_err("cannot write a note for a future day");
            return;
        }
        let mut input = crate::ui::form::TextInput::with_limit(400);
        if let Some(existing) = habit.note_on(self.view_date) {
            input.set(existing);
        }
        self.mode = Mode::Note(NoteEntry {
            habit_id: habit.id.clone(),
            habit_name: habit.name.clone(),
            date: self.view_date,
            input,
        });
    }

    fn save_note(&mut self, entry: NoteEntry, text: String) {
        let existing = self
            .data
            .habits
            .iter()
            .find(|habit| habit.id == entry.habit_id)
            .and_then(|habit| habit.note_on(entry.date))
            .unwrap_or("")
            .to_string();
        if existing == text.trim() {
            self.flash_ok("note unchanged");
            return;
        }
        let id = entry.habit_id.clone();
        let date = entry.date;
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
                    NoteAction::Save(text) => self.save_note(entry, text),
                }
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
                // Global keys still work, but habit actions must not fire on a
                // row that is not on screen.
                KeyCode::Tab
                | KeyCode::Char('T')
                | KeyCode::Char('m')
                | KeyCode::Char('r')
                | KeyCode::Char('/')
                | KeyCode::Char('?')
                | KeyCode::Char('q') => {}
                KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {}
                KeyCode::Esc => {
                    self.tab = Tab::Habits;
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
                } else if self.tab == Tab::Tags {
                    self.tab = Tab::Habits;
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

    fn submit_form(&mut self, original: Option<String>, draft: crate::ui::form::Draft) {
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
                let mut updated = self.data.habits[index].clone();
                updated.name = draft.name.clone();
                updated.schedule = draft.schedule;
                updated.created = draft.start;
                updated.ends = draft.ends;
                updated.note = draft.note;
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
        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
            .unwrap();
        assert_eq!(
            app.data.habits[0].note_on(date("2026-11-09")),
            Some("tired")
        );
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
