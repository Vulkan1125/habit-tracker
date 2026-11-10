//! Streaks, completion rates and heatmap data.
//!
//! Everything here is a pure function of `(habit, log, today)`, so the tests can
//! pin a date instead of depending on the wall clock.

use chrono::{Duration, NaiveDate};

use crate::model::{Day, Habit, Log, Schedule, start_of_week};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Done,
    Missed,
    NotDue,
    Future,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Unit {
    #[default]
    Days,
    Weeks,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Streak {
    pub length: u32,
    pub unit: Unit,
}

impl Streak {
    /// `12 days`, `3 weeks`, `0 days`.
    pub fn label(&self) -> String {
        let unit = match self.unit {
            Unit::Days => "day",
            Unit::Weeks => "week",
        };
        crate::format::plural(self.length, unit)
    }
}

/// Whether `date` counts as done, missed, or not expected.
///
/// For `TimesPerWeek` habits an un-logged day is *not* a miss — only the weekly
/// quota matters, so those days render as "not due" and the quota is shown
/// separately.
pub fn status_on(habit: &Habit, log: &Log, date: NaiveDate, today: NaiveDate) -> Status {
    if date > today {
        return Status::Future;
    }
    if log.is_done(date, &habit.id) {
        return Status::Done;
    }
    if date < habit.created {
        return Status::NotDue;
    }
    if matches!(habit.schedule, Schedule::TimesPerWeek { .. }) || !habit.schedule.is_due(date) {
        return Status::NotDue;
    }
    Status::Missed
}

/// Inclusive list of the last `days` days ending on `today`.
pub fn recent(habit: &Habit, log: &Log, today: NaiveDate, days: u32) -> Vec<(NaiveDate, Status)> {
    let days = days.max(1);
    (0..days)
        .rev()
        .map(|back| {
            let date = today - Duration::days(back as i64);
            (date, status_on(habit, log, date, today))
        })
        .collect()
}

fn history_start(habit: &Habit, log: &Log) -> NaiveDate {
    match log.first_date_for(&habit.id) {
        Some(logged) => logged.min(habit.created),
        None => habit.created,
    }
}

pub fn current_streak(habit: &Habit, log: &Log, today: NaiveDate, week_start: Day) -> Streak {
    if today < habit.created {
        return Streak {
            length: 0,
            unit: streak_unit(&habit.schedule),
        };
    }
    match habit.schedule {
        Schedule::TimesPerWeek { n } => weekly_streak(habit, log, today, week_start, n),
        _ => daily_streak(habit, log, today),
    }
}

fn streak_unit(schedule: &Schedule) -> Unit {
    match schedule {
        Schedule::TimesPerWeek { .. } => Unit::Weeks,
        _ => Unit::Days,
    }
}

fn daily_streak(habit: &Habit, log: &Log, today: NaiveDate) -> Streak {
    let mut length = 0;
    let mut date = today;
    let mut walked = 0u32;
    let floor = history_start(habit, log);

    while date >= floor && walked < 20_000 {
        if habit.schedule.is_due(date) {
            if log.is_done(date, &habit.id) {
                length += 1;
            } else if date != today {
                // Today is still in progress; yesterday and before must be done.
                break;
            }
        }
        date -= Duration::days(1);
        walked += 1;
    }

    Streak {
        length,
        unit: Unit::Days,
    }
}

fn weekly_streak(
    habit: &Habit,
    log: &Log,
    today: NaiveDate,
    week_start: Day,
    target: u8,
) -> Streak {
    let this_week = start_of_week(today, week_start);
    let first_week = start_of_week(history_start(habit, log), week_start);
    let mut length = 0;
    let mut week = this_week;
    let mut walked = 0u32;

    while week >= first_week && walked < 2_000 {
        let done = log.count_between(&habit.id, week, week + Duration::days(6));
        if done >= u32::from(target) {
            length += 1;
        } else if week != this_week {
            break;
        }
        week -= Duration::days(7);
        walked += 1;
    }

    Streak {
        length,
        unit: Unit::Weeks,
    }
}

pub fn longest_streak(habit: &Habit, log: &Log, today: NaiveDate, week_start: Day) -> Streak {
    if today < habit.created {
        return Streak {
            length: 0,
            unit: streak_unit(&habit.schedule),
        };
    }
    match habit.schedule {
        Schedule::TimesPerWeek { n } => {
            let this_week = start_of_week(today, week_start);
            let mut week = start_of_week(history_start(habit, log), week_start);
            let (mut best, mut current) = (0, 0);
            let mut walked = 0u32;
            while week <= this_week && walked < 2_000 {
                let done = log.count_between(&habit.id, week, week + Duration::days(6));
                if done >= u32::from(n) {
                    current += 1;
                    best = best.max(current);
                } else if week != this_week {
                    current = 0;
                }
                week += Duration::days(7);
                walked += 1;
            }
            Streak {
                length: best,
                unit: Unit::Weeks,
            }
        }
        _ => {
            let mut date = history_start(habit, log);
            let (mut best, mut current) = (0, 0);
            let mut walked = 0u32;
            while date <= today && walked < 20_000 {
                if habit.schedule.is_due(date) {
                    if log.is_done(date, &habit.id) {
                        current += 1;
                        best = best.max(current);
                    } else if date != today {
                        current = 0;
                    }
                }
                date += Duration::days(1);
                walked += 1;
            }
            Streak {
                length: best,
                unit: Unit::Days,
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Completion {
    pub done: u32,
    pub possible: u32,
}

impl Completion {
    pub fn percent(&self) -> u32 {
        if self.possible == 0 {
            0
        } else {
            ((u64::from(self.done) * 100) / u64::from(self.possible)).min(100) as u32
        }
    }
}

/// Completion rate over the last `days` days, clamped to the habit's lifetime.
pub fn completion(
    habit: &Habit,
    log: &Log,
    today: NaiveDate,
    week_start: Day,
    days: u32,
) -> Completion {
    let from = (today - Duration::days(i64::from(days.max(1)) - 1)).max(history_start(habit, log));
    if from > today {
        return Completion::default();
    }

    match habit.schedule {
        Schedule::TimesPerWeek { n } => {
            let first_week = start_of_week(from, week_start);
            let last_week = start_of_week(today, week_start);
            let mut done = 0;
            let mut possible = 0;
            let mut week = first_week;
            let mut walked = 0u32;
            while week <= last_week && walked < 2_000 {
                done += log.count_between(
                    &habit.id,
                    week.max(from),
                    (week + Duration::days(6)).min(today),
                );
                possible += u32::from(n);
                week += Duration::days(7);
                walked += 1;
            }
            Completion { done, possible }
        }
        _ => {
            let mut done = 0;
            let mut possible = 0;
            let mut date = from;
            while date <= today {
                if habit.schedule.is_due(date) {
                    possible += 1;
                    if log.is_done(date, &habit.id) {
                        done += 1;
                    }
                }
                date += Duration::days(1);
            }
            Completion { done, possible }
        }
    }
}

/// Progress toward this week's quota, for quota-based habits.
pub fn weekly_progress(
    habit: &Habit,
    log: &Log,
    today: NaiveDate,
    week_start: Day,
) -> Option<(u32, u8)> {
    let Schedule::TimesPerWeek { n } = habit.schedule else {
        return None;
    };
    let week = start_of_week(today, week_start);
    let done = log.count_between(&habit.id, week, week + Duration::days(6));
    Some((done, n))
}

/// Done vs. target for the week containing `today` (quota habits use their quota).
pub fn week_progress(habit: &Habit, log: &Log, today: NaiveDate, week_start: Day) -> (u32, u8) {
    if let Some(progress) = weekly_progress(habit, log, today, week_start) {
        return progress;
    }
    let week = start_of_week(today, week_start);
    let mut target = 0u8;
    let mut done = 0u32;
    let mut date = week;
    while date <= today {
        if habit.schedule.is_due(date) {
            target += 1;
            if log.is_done(date, &habit.id) {
                done += 1;
            }
        }
        date += Duration::days(1);
    }
    (done, target)
}

/// `[day_of_week][week]`, oldest week first, last column is the current week.
pub fn heatmap(
    habit: &Habit,
    log: &Log,
    today: NaiveDate,
    week_start: Day,
    weeks: u32,
) -> Vec<Vec<(NaiveDate, Status)>> {
    let weeks = weeks.clamp(1, 52);
    let this_week = start_of_week(today, week_start);
    let first_week = this_week - Duration::days(7 * i64::from(weeks - 1));

    (0..7)
        .map(|row| {
            (0..weeks)
                .map(|column| {
                    let date = first_week
                        + Duration::days(7 * i64::from(column))
                        + Duration::days(row as i64);
                    (date, status_on(habit, log, date, today))
                })
                .collect()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Day;
    use std::collections::BTreeSet;

    fn date(text: &str) -> NaiveDate {
        NaiveDate::parse_from_str(text, "%Y-%m-%d").unwrap()
    }

    fn habit(schedule: Schedule, created: &str) -> Habit {
        Habit::new("h".into(), "Habit".into(), schedule, date(created))
    }

    /// Mark a habit done on each date in `dates`.
    fn log_done(dates: &[&str]) -> Log {
        let mut log = Log::default();
        for day in dates {
            log.set(date(day), "h", true);
        }
        log
    }

    #[test]
    fn daily_streak_counts_consecutive_days() {
        let habit = habit(Schedule::Daily, "2026-11-01");
        let log = log_done(&["2026-11-08", "2026-11-09", "2026-11-10"]);
        let streak = current_streak(&habit, &log, date("2026-11-10"), Day::Mon);
        assert_eq!(streak.length, 3);
        assert_eq!(streak.unit, Unit::Days);
    }

    #[test]
    fn today_not_done_keeps_yesterdays_streak_alive() {
        let habit = habit(Schedule::Daily, "2026-11-01");
        let log = log_done(&["2026-11-08", "2026-11-09"]);
        let streak = current_streak(&habit, &log, date("2026-11-10"), Day::Mon);
        assert_eq!(streak.length, 2);
    }

    #[test]
    fn missing_a_past_day_breaks_the_streak() {
        let habit = habit(Schedule::Daily, "2026-11-01");
        let log = log_done(&["2026-11-05", "2026-11-07", "2026-11-08"]);
        let streak = current_streak(&habit, &log, date("2026-11-10"), Day::Mon);
        assert_eq!(streak.length, 0, "the 9th and 10th are missing");
    }

    #[test]
    fn weekday_habits_skip_days_that_are_not_due() {
        // Mon/Wed/Fri habit: 2026-11-09 Mon, 11 Wed, 13 Fri.
        let habit = habit(
            Schedule::Weekdays {
                days: BTreeSet::from([Day::Mon, Day::Wed, Day::Fri]),
            },
            "2026-11-01",
        );
        let log = log_done(&["2026-11-06", "2026-11-09", "2026-11-11"]);
        let streak = current_streak(&habit, &log, date("2026-11-11"), Day::Mon);
        assert_eq!(streak.length, 3, "Tue/Thu/Sat/Sun do not break it");
    }

    #[test]
    fn weekly_quota_habit_counts_weeks_not_days() {
        // 4×/week habit, week of 2026-11-09 (Mon start).
        let habit = habit(Schedule::TimesPerWeek { n: 4 }, "2026-10-01");
        let log = log_done(&[
            "2026-11-02",
            "2026-11-04",
            "2026-11-06",
            "2026-11-07", // previous week: 4
            "2026-11-09",
            "2026-11-10",
            "2026-11-11",
            "2026-11-12", // this week: 4
        ]);
        let streak = current_streak(&habit, &log, date("2026-11-13"), Day::Mon);
        assert_eq!(streak.length, 2);
        assert_eq!(streak.unit, Unit::Weeks);
    }

    #[test]
    fn weekly_quota_with_three_of_four_does_not_count_but_does_not_break() {
        let habit = habit(Schedule::TimesPerWeek { n: 4 }, "2026-10-01");
        let log = log_done(&[
            "2026-11-02",
            "2026-11-04",
            "2026-11-06",
            "2026-11-07",
            // this week only three
            "2026-11-09",
            "2026-11-10",
            "2026-11-11",
        ]);
        let streak = current_streak(&habit, &log, date("2026-11-12"), Day::Mon);
        assert_eq!(
            streak.length, 1,
            "the in-progress week is skipped, not fatal"
        );
        let (done, target) = weekly_progress(&habit, &log, date("2026-11-12"), Day::Mon).unwrap();
        assert_eq!((done, target), (3, 4));
    }

    #[test]
    fn longest_streak_looks_beyond_the_current_run() {
        let habit = habit(Schedule::Daily, "2026-11-01");
        let log = log_done(&[
            "2026-11-03",
            "2026-11-04",
            "2026-11-05",
            "2026-11-06",
            "2026-11-07",
            "2026-11-10",
        ]);
        let longest = longest_streak(&habit, &log, date("2026-11-10"), Day::Mon);
        assert_eq!(longest.length, 5);
    }

    #[test]
    fn completion_ignores_days_before_creation() {
        let habit = habit(Schedule::Daily, "2026-11-08");
        let log = log_done(&["2026-11-08", "2026-11-10"]);
        let completion = completion(&habit, &log, date("2026-11-10"), Day::Mon, 30);
        assert_eq!(completion.done, 2);
        assert_eq!(completion.possible, 3);
        assert_eq!(completion.percent(), 66);
    }

    #[test]
    fn heatmap_last_column_contains_today() {
        let habit = habit(Schedule::Daily, "2026-01-01");
        let log = log_done(&["2026-11-10"]);
        let grid = heatmap(&habit, &log, date("2026-11-10"), Day::Mon, 5);
        assert_eq!(grid.len(), 7);
        assert_eq!(grid[0].len(), 5);
        // Monday row, last column is the current week, so 2026-11-09.
        assert_eq!(grid[0][4].0, date("2026-11-09"));
        assert_eq!(grid[1][4].0, date("2026-11-10"));
        assert_eq!(grid[1][4].1, Status::Done);
    }

    #[test]
    fn heatmap_rows_follow_the_configured_week_start() {
        let habit = habit(Schedule::Daily, "2026-01-01");
        let log = log_done(&["2026-11-10"]);
        let grid = heatmap(&habit, &log, date("2026-11-10"), Day::Sun, 2);
        assert_eq!(grid[0][1].0, date("2026-11-08"), "first row is Sunday");
        assert_eq!(grid[2][1].0, date("2026-11-10"), "Tuesday is offset 2");
    }

    #[test]
    fn status_marks_future_and_not_due() {
        let habit = habit(
            Schedule::Weekdays {
                days: BTreeSet::from([Day::Mon]),
            },
            "2026-01-01",
        );
        let log = log_done(&["2026-11-09"]);
        assert_eq!(
            status_on(&habit, &log, date("2026-11-10"), date("2026-11-10")),
            Status::NotDue
        );
        assert_eq!(
            status_on(&habit, &log, date("2026-11-16"), date("2026-11-10")),
            Status::Future
        );
        assert_eq!(
            status_on(&habit, &log, date("2026-11-02"), date("2026-11-10")),
            Status::Missed
        );
    }

    #[test]
    fn days_before_creation_are_not_missed() {
        let habit = habit(Schedule::Daily, "2026-11-09");
        let log = Log::default();
        assert_eq!(
            status_on(&habit, &log, date("2026-11-05"), date("2026-11-10")),
            Status::NotDue
        );
        assert_eq!(
            status_on(&habit, &log, date("2026-11-09"), date("2026-11-10")),
            Status::Missed
        );
    }

    #[test]
    fn no_history_scores_zero_without_panicking() {
        let habit = habit(Schedule::Daily, "2026-11-10");
        let log = Log::default();
        assert_eq!(
            current_streak(&habit, &log, date("2026-11-10"), Day::Mon).length,
            0
        );
        assert_eq!(
            longest_streak(&habit, &log, date("2026-11-10"), Day::Mon).length,
            0
        );
        assert_eq!(
            completion(&habit, &log, date("2026-11-10"), Day::Mon, 30).possible,
            1
        );
    }
}
