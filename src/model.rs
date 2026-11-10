//! Core data types: habits, schedules and the completion log.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use anyhow::{Result, bail};
use chrono::{Datelike, Duration, NaiveDate, Weekday};
use serde::{Deserialize, Serialize};

/// Bumped only when the on-disk shape changes incompatibly.
pub const DATA_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Day {
    Mon,
    Tue,
    Wed,
    Thu,
    Fri,
    Sat,
    Sun,
}

pub const ALL_DAYS: [Day; 7] = [
    Day::Mon,
    Day::Tue,
    Day::Wed,
    Day::Thu,
    Day::Fri,
    Day::Sat,
    Day::Sun,
];

impl Day {
    pub const fn index(self) -> usize {
        match self {
            Day::Mon => 0,
            Day::Tue => 1,
            Day::Wed => 2,
            Day::Thu => 3,
            Day::Fri => 4,
            Day::Sat => 5,
            Day::Sun => 6,
        }
    }

    pub const fn from_index(index: usize) -> Day {
        ALL_DAYS[index % 7]
    }

    pub const fn short(self) -> &'static str {
        ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"][self.index()]
    }

    pub const fn long(self) -> &'static str {
        [
            "Monday",
            "Tuesday",
            "Wednesday",
            "Thursday",
            "Friday",
            "Saturday",
            "Sunday",
        ][self.index()]
    }

    pub fn from_weekday(weekday: Weekday) -> Day {
        match weekday {
            Weekday::Mon => Day::Mon,
            Weekday::Tue => Day::Tue,
            Weekday::Wed => Day::Wed,
            Weekday::Thu => Day::Thu,
            Weekday::Fri => Day::Fri,
            Weekday::Sat => Day::Sat,
            Weekday::Sun => Day::Sun,
        }
    }

    /// Accepts `mon`, `Monday`, `1` … `7` (1 = Monday).
    pub fn parse(text: &str) -> Option<Day> {
        let text = text.trim().to_ascii_lowercase();
        if let Ok(n) = text.parse::<u8>() {
            return (1..=7)
                .contains(&n)
                .then(|| Day::from_index(n as usize - 1));
        }
        ALL_DAYS.into_iter().find(|day| {
            day.short().eq_ignore_ascii_case(&text) || day.long().eq_ignore_ascii_case(&text)
        })
    }
}

impl fmt::Display for Day {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.short())
    }
}

/// The first day of the week containing `date`, for an arbitrary week start.
pub fn start_of_week(date: NaiveDate, week_start: Day) -> NaiveDate {
    let current = Day::from_weekday(date.weekday());
    let back = (current.index() + 7 - week_start.index()) % 7;
    date - Duration::days(back as i64)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Schedule {
    Daily,
    Weekdays { days: BTreeSet<Day> },
    TimesPerWeek { n: u8 },
}

impl Schedule {
    /// Whether a completion on `date` counts toward this habit at all.
    ///
    /// `TimesPerWeek` is flexible: any day is acceptable, the quota is weekly.
    pub fn is_due(&self, date: NaiveDate) -> bool {
        match self {
            Schedule::Daily | Schedule::TimesPerWeek { .. } => true,
            Schedule::Weekdays { days } => days.contains(&Day::from_weekday(date.weekday())),
        }
    }

    pub fn describe(&self) -> String {
        match self {
            Schedule::Daily => "daily".to_string(),
            Schedule::Weekdays { days } if days.len() == 7 => "daily".to_string(),
            Schedule::Weekdays { days } if days.is_empty() => "never".to_string(),
            Schedule::Weekdays { days } => describe_days(days),
            Schedule::TimesPerWeek { n } => format!("{n}×/week"),
        }
    }

    pub fn validate(&self) -> Result<()> {
        match self {
            Schedule::TimesPerWeek { n } if !(1..=7).contains(n) => {
                bail!("times per week must be between 1 and 7")
            }
            Schedule::Weekdays { days } if days.is_empty() => {
                bail!("pick at least one weekday")
            }
            _ => Ok(()),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Habit {
    pub id: String,
    pub name: String,
    pub schedule: Schedule,
    pub created: NaiveDate,
    #[serde(default, skip_serializing_if = "is_false")]
    pub archived: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

fn is_false(value: &bool) -> bool {
    !*value
}

impl Habit {
    pub fn new(id: String, name: String, schedule: Schedule, created: NaiveDate) -> Self {
        Self {
            id,
            name,
            schedule,
            created,
            archived: false,
            note: None,
        }
    }
}

/// Completion history: `date -> set of habit ids`.
///
/// Stored as a map so the JSON stays small, sorted and easy to hand-edit.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Log {
    entries: BTreeMap<NaiveDate, BTreeSet<String>>,
}

impl Log {
    pub fn is_done(&self, date: NaiveDate, habit_id: &str) -> bool {
        self.entries
            .get(&date)
            .is_some_and(|ids| ids.contains(habit_id))
    }

    pub fn set(&mut self, date: NaiveDate, habit_id: &str, done: bool) {
        if done {
            self.entries
                .entry(date)
                .or_default()
                .insert(habit_id.to_string());
            return;
        }

        let now_empty = match self.entries.get_mut(&date) {
            Some(ids) => {
                ids.remove(habit_id);
                ids.is_empty()
            }
            None => false,
        };
        if now_empty {
            self.entries.remove(&date);
        }
    }

    /// Flips `date`, returning the new state.
    pub fn toggle(&mut self, date: NaiveDate, habit_id: &str) -> bool {
        let done = !self.is_done(date, habit_id);
        self.set(date, habit_id, done);
        done
    }

    pub fn ids_on(&self, date: NaiveDate) -> Vec<&str> {
        self.entries
            .get(&date)
            .map(|ids| ids.iter().map(String::as_str).collect())
            .unwrap_or_default()
    }

    pub fn count_between(&self, habit_id: &str, from: NaiveDate, to: NaiveDate) -> u32 {
        if from > to {
            return 0;
        }
        self.entries
            .range(from..=to)
            .filter(|(_, ids)| ids.contains(habit_id))
            .count() as u32
    }

    pub fn first_date_for(&self, habit_id: &str) -> Option<NaiveDate> {
        self.entries
            .iter()
            .find(|(_, ids)| ids.contains(habit_id))
            .map(|(date, _)| *date)
    }

    pub fn total_for(&self, habit_id: &str) -> u32 {
        self.entries
            .values()
            .filter(|ids| ids.contains(habit_id))
            .count() as u32
    }

    pub fn remove_habit(&mut self, habit_id: &str) {
        self.entries.retain(|_, ids| {
            ids.remove(habit_id);
            !ids.is_empty()
        });
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Data {
    #[serde(default = "default_version")]
    pub version: u32,
    #[serde(default)]
    pub habits: Vec<Habit>,
    #[serde(default)]
    pub log: Log,
}

fn default_version() -> u32 {
    DATA_VERSION
}

impl Default for Data {
    fn default() -> Self {
        Self {
            version: DATA_VERSION,
            habits: Vec::new(),
            log: Log::default(),
        }
    }
}

impl Data {
    /// Find a habit by exact id, exact name (case-insensitive), or unique id prefix.
    pub fn resolve_index(&self, needle: &str) -> Result<usize> {
        let needle = needle.trim();
        if needle.is_empty() {
            bail!("habit name cannot be empty");
        }
        if let Some(index) = self.habits.iter().position(|habit| habit.id == needle) {
            return Ok(index);
        }
        let lowered = needle.to_lowercase();
        if let Some(index) = self
            .habits
            .iter()
            .position(|habit| habit.name.to_lowercase() == lowered)
        {
            return Ok(index);
        }
        let prefix: Vec<usize> = self
            .habits
            .iter()
            .enumerate()
            .filter(|(_, habit)| habit.id.starts_with(&lowered))
            .map(|(index, _)| index)
            .collect();
        match prefix.as_slice() {
            [] => bail!("no habit matches \"{needle}\""),
            [index] => Ok(*index),
            many => {
                let options: Vec<&str> = many
                    .iter()
                    .map(|index| self.habits[*index].id.as_str())
                    .collect();
                bail!(
                    "\"{needle}\" is ambiguous, try one of: {}",
                    options.join(", ")
                )
            }
        }
    }

    pub fn add_habit(
        &mut self,
        name: &str,
        schedule: Schedule,
        created: NaiveDate,
    ) -> Result<usize> {
        let name = name.trim();
        if name.is_empty() {
            bail!("habit name cannot be empty");
        }
        if name.chars().count() > 48 {
            bail!("habit name is too long (max 48 characters)");
        }
        if let Some(existing) = self
            .habits
            .iter()
            .find(|habit| habit.name.eq_ignore_ascii_case(name))
        {
            bail!("a habit named \"{}\" already exists", existing.name);
        }
        schedule.validate()?;
        let id = self.unique_id(name);
        self.habits
            .push(Habit::new(id, name.to_string(), schedule, created));
        Ok(self.habits.len() - 1)
    }

    /// Remove a habit and every completion recorded for it.
    pub fn remove_habit(&mut self, index: usize) -> Habit {
        let habit = self.habits.remove(index);
        self.log.remove_habit(&habit.id);
        habit
    }

    pub fn unique_id(&self, name: &str) -> String {
        let base = slugify(name);
        if !self.habits.iter().any(|habit| habit.id == base) {
            return base;
        }
        (2..)
            .map(|n| format!("{base}-{n}"))
            .find(|candidate| !self.habits.iter().any(|habit| &habit.id == candidate))
            .expect("unbounded suffix search cannot fail")
    }

    /// Habit indices in display order: active first, archived last, stable otherwise.
    pub fn display_order(&self) -> Vec<usize> {
        let mut order: Vec<usize> = (0..self.habits.len()).collect();
        order.sort_by_key(|index| (self.habits[*index].archived, *index));
        order
    }

    pub fn due_today(&self, today: NaiveDate) -> usize {
        self.habits
            .iter()
            .filter(|habit| !habit.archived && habit.schedule.is_due(today))
            .count()
    }

    pub fn done_today(&self, today: NaiveDate) -> usize {
        self.habits
            .iter()
            .filter(|habit| !habit.archived && self.log.is_done(today, &habit.id))
            .count()
    }
}

/// `Mon–Fri` for a run of at least three days, `Mon Wed Fri` otherwise.
fn describe_days(days: &BTreeSet<Day>) -> String {
    let indices: Vec<usize> = days.iter().map(|day| day.index()).collect();
    let consecutive = indices.windows(2).all(|pair| pair[1] == pair[0] + 1);
    match (indices.len(), consecutive, indices.first(), indices.last()) {
        (3.., true, Some(first), Some(last)) => format!(
            "{}–{}",
            Day::from_index(*first).short(),
            Day::from_index(*last).short()
        ),
        _ => days
            .iter()
            .map(|day| day.short())
            .collect::<Vec<_>>()
            .join(" "),
    }
}

fn slugify(name: &str) -> String {
    let mut slug = String::new();
    let mut pending_dash = false;
    for character in name.chars() {
        if character.is_alphanumeric() {
            if pending_dash && !slug.is_empty() {
                slug.push('-');
            }
            pending_dash = false;
            slug.extend(character.to_lowercase());
        } else {
            pending_dash = true;
        }
        if slug.chars().count() >= 32 {
            break;
        }
    }
    let slug = slug.trim_matches('-').to_string();
    if slug.is_empty() {
        "habit".to_string()
    } else {
        slug
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn date(text: &str) -> NaiveDate {
        NaiveDate::parse_from_str(text, "%Y-%m-%d").unwrap()
    }

    #[test]
    fn day_parsing_accepts_short_long_and_numeric_forms() {
        assert_eq!(Day::parse("mon"), Some(Day::Mon));
        assert_eq!(Day::parse("FRIDAY"), Some(Day::Fri));
        assert_eq!(Day::parse("7"), Some(Day::Sun));
        assert_eq!(Day::parse("someday"), None);
        assert_eq!(Day::parse("0"), None);
    }

    #[test]
    fn week_start_handles_every_offset() {
        // 2026-11-10 is a Tuesday.
        let tuesday = date("2026-11-10");
        assert_eq!(start_of_week(tuesday, Day::Mon), date("2026-11-09"));
        assert_eq!(start_of_week(tuesday, Day::Tue), tuesday);
        assert_eq!(start_of_week(tuesday, Day::Sun), date("2026-11-08"));
        assert_eq!(start_of_week(tuesday, Day::Wed), date("2026-11-04"));
    }

    #[test]
    fn schedule_due_matches_weekday() {
        let schedule = Schedule::Weekdays {
            days: BTreeSet::from([Day::Mon, Day::Wed]),
        };
        assert!(schedule.is_due(date("2026-11-09")));
        assert!(!schedule.is_due(date("2026-11-10")));
    }

    #[test]
    fn day_runs_are_compressed_for_display() {
        let weekdays = Schedule::Weekdays {
            days: BTreeSet::from([Day::Mon, Day::Tue, Day::Wed, Day::Thu, Day::Fri]),
        };
        assert_eq!(weekdays.describe(), "Mon–Fri");
        let odd = Schedule::Weekdays {
            days: BTreeSet::from([Day::Mon, Day::Wed, Day::Fri]),
        };
        assert_eq!(odd.describe(), "Mon Wed Fri");
        let pair = Schedule::Weekdays {
            days: BTreeSet::from([Day::Mon, Day::Tue]),
        };
        assert_eq!(pair.describe(), "Mon Tue");
    }

    #[test]
    fn toggle_round_trips_and_cleans_up_empty_days() {
        let mut log = Log::default();
        let day = date("2026-11-10");
        assert!(log.toggle(day, "gym"));
        assert!(log.is_done(day, "gym"));
        assert!(!log.toggle(day, "gym"));
        assert!(!log.is_done(day, "gym"));
        assert!(log.ids_on(day).is_empty());
    }

    #[test]
    fn ids_are_slugged_and_deduplicated() {
        let mut data = Data::default();
        let day = date("2026-01-01");
        data.add_habit("Morning Walk!", Schedule::Daily, day)
            .unwrap();
        assert_eq!(data.habits[0].id, "morning-walk");
        data.add_habit("morning walk!", Schedule::Daily, day)
            .unwrap_err();
        data.add_habit("Morning  walk", Schedule::Daily, day)
            .unwrap();
        assert_eq!(data.habits[1].id, "morning-walk-2");
    }

    #[test]
    fn resolve_finds_by_id_name_and_prefix() {
        let mut data = Data::default();
        let day = date("2026-01-01");
        data.add_habit("Gym", Schedule::Daily, day).unwrap();
        data.add_habit("Read", Schedule::Daily, day).unwrap();
        assert_eq!(data.resolve_index("Gym").unwrap(), 0);
        assert_eq!(data.resolve_index("gym").unwrap(), 0);
        assert_eq!(data.resolve_index("rea").unwrap(), 1);
        assert!(data.resolve_index("nope").is_err());
    }

    #[test]
    fn removing_a_habit_purges_its_log_entries() {
        let mut data = Data::default();
        let day = date("2026-01-01");
        data.add_habit("Gym", Schedule::Daily, day).unwrap();
        data.add_habit("Read", Schedule::Daily, day).unwrap();
        data.log.set(day, "gym", true);
        data.log.set(day, "read", true);
        data.remove_habit(0);
        assert!(!data.log.is_done(day, "gym"));
        assert!(data.log.is_done(day, "read"));
    }

    #[test]
    fn display_order_puts_archived_last() {
        let mut data = Data::default();
        let day = date("2026-01-01");
        data.add_habit("A", Schedule::Daily, day).unwrap();
        data.add_habit("B", Schedule::Daily, day).unwrap();
        data.habits[0].archived = true;
        assert_eq!(data.display_order(), vec![1, 0]);
    }
}
