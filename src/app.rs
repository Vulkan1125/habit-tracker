//! Application state, key handling and persistence-on-change.

use std::time::{Duration, Instant};

use anyhow::Result;
use chrono::NaiveDate;
use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::widgets::ListState;

use crate::config::Config;
use crate::model::{Data, Day, Habit, Schedule};
use crate::stats;
use crate::store::Store;
use crate::theme::Theme;
use crate::ui::form::{Form, FormAction};

pub enum Mode {
    Normal,
    Help,
    Confirm(Confirm),
    Form(Box<Form>),
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
    pub at: Instant,
}

pub struct App {
    pub store: Store,
    pub data: Data,
    pub today: NaiveDate,
    pub week_start: Day,
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
            week_start: config.week_start,
            theme: Theme::new(config.theme.into()),
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
        self.flash_err(message);
    }

    /// Indices into `data.habits`, active first, archived last, filtered.
    pub fn visible_indices(&self) -> Vec<usize> {
        let query = self.filter.trim().to_lowercase();
        let mut order = self.data.display_order();
        if !query.is_empty() {
            order.retain(|index| {
                let habit = &self.data.habits[*index];
                habit.name.to_lowercase().contains(&query) || habit.id.contains(&query)
            });
        }
        order
    }

    pub fn selected(&self) -> Option<usize> {
        self.visible_indices().get(self.cursor).copied()
    }

    pub fn selected_habit(&self) -> Option<&Habit> {
        self.selected().map(|index| &self.data.habits[index])
    }

    pub fn handle_event(&mut self, event: Event) -> Result<()> {
        if let Event::Key(key) = event
            && key.kind == KeyEventKind::Press
        {
            self.handle_key(key)?;
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
                    FormAction::Submit { name, schedule } => {
                        self.submit_form(original, name, schedule)
                    }
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

        let control = key.modifiers.contains(KeyModifiers::CONTROL);
        match key.code {
            KeyCode::Char('c') if control => self.quit = true,
            KeyCode::Char('q') => {
                if self.filter.is_empty() {
                    self.quit = true;
                } else {
                    self.clear_filter();
                }
            }
            KeyCode::Esc => {
                if self.filter.is_empty() {
                    self.quit = true;
                } else {
                    self.clear_filter();
                }
            }
            KeyCode::Char('?') => self.mode = Mode::Help,
            KeyCode::Char('j') | KeyCode::Down => self.move_cursor(1),
            KeyCode::Char('k') | KeyCode::Up => self.move_cursor(-1),
            KeyCode::Char('g') | KeyCode::Home => self.cursor = 0,
            KeyCode::Char('G') | KeyCode::End => self.cursor = usize::MAX,
            KeyCode::Char(' ') => self.toggle_selected(),
            KeyCode::Char('a') => self.mode = Mode::Form(Box::new(Form::new())),
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

    fn toggle_selected(&mut self) {
        let Some(index) = self.selected() else {
            self.flash_err("nothing selected");
            return;
        };
        let habit = self.data.habits[index].clone();
        let date = self.today;
        let mut now_done = false;
        self.mutate(|data| {
            now_done = data.log.toggle(date, &habit.id);
        });

        if now_done {
            let progress = stats::weekly_progress(&habit, &self.data.log, date, self.week_start);
            let suffix = match progress {
                Some((done, target)) => format!("{done}/{target} this week"),
                None => {
                    let streak =
                        stats::current_streak(&habit, &self.data.log, date, self.week_start);
                    format!("streak {}", streak.label())
                }
            };
            self.flash_ok(format!("✓ {} · {suffix}", habit.name));
        } else {
            self.flash_ok(format!("✗ {} undone", habit.name));
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

    fn submit_form(&mut self, original: Option<String>, name: String, schedule: Schedule) {
        if let Err(error) = schedule.validate() {
            self.flash_err(error.to_string());
            return;
        }

        match original {
            Some(id) => {
                let Some(index) = self.data.habits.iter().position(|habit| habit.id == id) else {
                    self.flash_err("that habit no longer exists");
                    return;
                };
                if let Some(other) = self
                    .data
                    .habits
                    .iter()
                    .find(|habit| habit.id != id && habit.name.eq_ignore_ascii_case(&name))
                {
                    self.flash_err(format!("a habit named \"{}\" already exists", other.name));
                    return;
                }
                let display = name.clone();
                self.mutate(move |data| {
                    if let Some(habit) = data.habits.get_mut(index) {
                        habit.name = name;
                        habit.schedule = schedule;
                    }
                });
                self.flash_ok(format!("updated {display}"));
            }
            None => {
                let before = self.data.clone();
                match self.data.add_habit(&name, schedule, self.today) {
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
                            self.flash_ok(format!("added {display} · space to check off"));
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

    fn flash_ok(&mut self, text: impl Into<String>) {
        self.flash = Some(Flash {
            text: text.into(),
            error: false,
            at: Instant::now(),
        });
    }

    fn flash_err(&mut self, text: impl Into<String>) {
        self.flash = Some(Flash {
            text: text.into(),
            error: true,
            at: Instant::now(),
        });
    }

    pub fn tick(&mut self) {
        if let Some(flash) = &self.flash
            && flash.at.elapsed() > Duration::from_secs(4)
        {
            self.flash = None;
        }
    }
}
