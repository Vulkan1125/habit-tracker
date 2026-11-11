//! Streaks, completion rates and heatmap data.
//!
//! Everything here is a pure function of `(habit, history, today)`, so the tests
//! can pin a date instead of depending on the wall clock.

use chrono::{Duration, NaiveDate};

use crate::model::{Day, Habit, History, Schedule, start_of_week};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Done,
    Missed,
    NotDue,
    Future,
    /// The first cheat day of the month: skipped, so it cannot break a streak.
    Cheat,
    /// Today is due but still open, so it is not a miss yet.
    Pending,
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

/// Whether `date` counts as done, missed, excused, or not expected.
///
/// For `TimesPerWeek` habits an un-logged day is *not* a miss — only the weekly
/// quota matters, so those days render as "not due" and the quota is shown
/// separately. The same is true outside the habit's start/end window.
pub fn status_on(habit: &Habit, history: &History, date: NaiveDate, today: NaiveDate) -> Status {
    if date > today {
        return Status::Future;
    }
    if !habit.is_active_on(date) {
        return Status::NotDue;
    }
    if history.log.is_done(date, &habit.id) {
        return Status::Done;
    }
    if history.cheats.is_excused(date, &habit.id) {
        return Status::Cheat;
    }
    if matches!(habit.schedule, Schedule::TimesPerWeek { .. }) || !habit.schedule.is_due(date) {
        return Status::NotDue;
    }
    if date == today {
        return Status::Pending;
    }
    Status::Missed
}

/// Inclusive list of the last `days` days ending on `today`.
pub fn recent(
    habit: &Habit,
    history: &History,
    today: NaiveDate,
    days: u32,
) -> Vec<(NaiveDate, Status)> {
    let days = days.max(1);
    (0..days)
        .rev()
        .map(|back| {
            let date = today - Duration::days(back as i64);
            (date, status_on(habit, history, date, today))
        })
        .collect()
}

fn history_start(habit: &Habit, history: &History) -> NaiveDate {
    match history.log.first_date_for(&habit.id) {
        Some(logged) => logged.min(habit.created),
        None => habit.created,
    }
}

/// The last day worth looking at: today, or the habit's end date once it passed.
fn scope_end(habit: &Habit, today: NaiveDate) -> NaiveDate {
    match habit.ends {
        Some(ends) => ends.min(today),
        None => today,
    }
}

pub fn current_streak(
    habit: &Habit,
    history: &History,
    today: NaiveDate,
    week_start: Day,
) -> Streak {
    if today < habit.created {
        return Streak {
            length: 0,
            unit: streak_unit(&habit.schedule),
        };
    }
    match habit.schedule {
        Schedule::TimesPerWeek { n } => weekly_streak(habit, history, today, week_start, n),
        _ => daily_streak(habit, history, today),
    }
}

fn streak_unit(schedule: &Schedule) -> Unit {
    match schedule {
        Schedule::TimesPerWeek { .. } => Unit::Weeks,
        _ => Unit::Days,
    }
}

fn daily_streak(habit: &Habit, history: &History, today: NaiveDate) -> Streak {
    let mut length = 0;
    let mut date = scope_end(habit, today);
    let mut walked = 0u32;
    let floor = history_start(habit, history);

    while date >= floor && walked < 20_000 {
        if habit.schedule.is_due(date) && habit.is_active_on(date) {
            if history.log.is_done(date, &habit.id) {
                length += 1;
            } else if history.cheats.is_excused(date, &habit.id) {
                // Excused: skipped, exactly like a day that is not due.
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
    history: &History,
    today: NaiveDate,
    week_start: Day,
    target: u8,
) -> Streak {
    let last_day = scope_end(habit, today);
    let this_week = start_of_week(today, week_start);
    let last_week = start_of_week(last_day, week_start);
    let first_week = start_of_week(history_start(habit, history), week_start);
    let mut length = 0;
    let mut week = last_week;
    let mut walked = 0u32;

    while week >= first_week && walked < 2_000 {
        let done = history.log.count_between(
            &habit.id,
            week.max(habit.created),
            (week + Duration::days(6)).min(last_day),
        );
        if done >= u32::from(target) {
            length += 1;
        } else if week != this_week && week != last_week {
            // The current week (or the week a habit ended in) is still in progress.
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

pub fn longest_streak(
    habit: &Habit,
    history: &History,
    today: NaiveDate,
    week_start: Day,
) -> Streak {
    if today < habit.created {
        return Streak {
            length: 0,
            unit: streak_unit(&habit.schedule),
        };
    }
    let last_day = scope_end(habit, today);
    match habit.schedule {
        Schedule::TimesPerWeek { n } => {
            let last_week = start_of_week(last_day, week_start);
            let mut week = start_of_week(history_start(habit, history), week_start);
            let (mut best, mut current) = (0, 0);
            let mut walked = 0u32;
            while week <= last_week && walked < 2_000 {
                let done = history.log.count_between(
                    &habit.id,
                    week.max(habit.created),
                    (week + Duration::days(6)).min(last_day),
                );
                if done >= u32::from(n) {
                    current += 1;
                    best = best.max(current);
                } else if week != last_week {
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
            let mut date = history_start(habit, history);
            let (mut best, mut current) = (0, 0);
            let mut walked = 0u32;
            while date <= last_day && walked < 20_000 {
                if habit.schedule.is_due(date) && habit.is_active_on(date) {
                    if history.log.is_done(date, &habit.id) {
                        current += 1;
                        best = best.max(current);
                    } else if history.cheats.is_excused(date, &habit.id) {
                        // Excused days bridge a run instead of ending it.
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

/// Completion rate over the last `days` days, clamped to the habit's window.
pub fn completion(
    habit: &Habit,
    history: &History,
    today: NaiveDate,
    week_start: Day,
    days: u32,
) -> Completion {
    let last_day = scope_end(habit, today);
    let from = (today - Duration::days(i64::from(days.max(1)) - 1))
        .max(history_start(habit, history))
        .max(habit.created);
    if from > last_day {
        return Completion::default();
    }

    match habit.schedule {
        Schedule::TimesPerWeek { n } => {
            let first_week = start_of_week(from, week_start);
            let last_week = start_of_week(last_day, week_start);
            let mut done = 0;
            let mut possible = 0;
            let mut week = first_week;
            let mut walked = 0u32;
            while week <= last_week && walked < 2_000 {
                done += history.log.count_between(
                    &habit.id,
                    week.max(from),
                    (week + Duration::days(6)).min(last_day),
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
            while date <= last_day {
                if habit.schedule.is_due(date) && habit.is_active_on(date) {
                    // An excused day cannot be failed, so it is not a possible day.
                    if history.cheats.is_excused(date, &habit.id) {
                        // counted as neither done nor possible
                    } else {
                        possible += 1;
                        if history.log.is_done(date, &habit.id) {
                            done += 1;
                        }
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
    history: &History,
    today: NaiveDate,
    week_start: Day,
) -> Option<(u32, u8)> {
    let Schedule::TimesPerWeek { n } = habit.schedule else {
        return None;
    };
    let last_day = scope_end(habit, today);
    // A finished habit keeps showing the week it ended in.
    let week = start_of_week(last_day, week_start);
    let done = history.log.count_between(
        &habit.id,
        week.max(habit.created),
        (week + Duration::days(6)).min(last_day),
    );
    Some((done, n))
}

/// One week of a quota habit's history.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WeekProgress {
    pub start: NaiveDate,
    pub done: u32,
    pub target: u8,
}

/// The last `weeks` weeks of a quota habit, oldest first.
///
/// Only `TimesPerWeek` habits have a weekly quota, so other schedules return an
/// empty list. The final entry is the week containing `today`, or the week the
/// habit ended in. Weeks before the habit started are left out.
pub fn weekly_history(
    habit: &Habit,
    history: &History,
    today: NaiveDate,
    week_start: Day,
    weeks: u32,
) -> Vec<WeekProgress> {
    let Schedule::TimesPerWeek { n } = habit.schedule else {
        return Vec::new();
    };
    let last_day = scope_end(habit, today);
    if last_day < habit.created {
        return Vec::new();
    }
    let last_week = start_of_week(last_day, week_start);
    let first_week = start_of_week(habit.created, week_start);
    let weeks = weeks.clamp(1, 52);
    let from = (last_week - Duration::days(7 * i64::from(weeks - 1))).max(first_week);

    let mut history_weeks = Vec::new();
    let mut week = from;
    while week <= last_week {
        let done = history.log.count_between(
            &habit.id,
            week.max(habit.created),
            (week + Duration::days(6)).min(last_day),
        );
        history_weeks.push(WeekProgress {
            start: week,
            done,
            target: n,
        });
        week += Duration::days(7);
    }
    history_weeks
}

/// Done vs. target for the week containing `today` (quota habits use their quota).
pub fn week_progress(
    habit: &Habit,
    history: &History,
    today: NaiveDate,
    week_start: Day,
) -> (u32, u8) {
    if let Some(progress) = weekly_progress(habit, history, today, week_start) {
        return progress;
    }
    let last_day = scope_end(habit, today);
    // A finished habit keeps showing the week it ended in.
    let week = start_of_week(last_day, week_start);
    let mut target = 0u8;
    let mut done = 0u32;
    let mut date = week;
    while date <= last_day {
        if habit.schedule.is_due(date) && habit.is_active_on(date) {
            if history.log.is_done(date, &habit.id) {
                target += 1;
                done += 1;
            } else if history.cheats.is_excused(date, &habit.id) {
                // An excused day is not required, so it is not a target either.
            } else {
                target += 1;
            }
        }
        date += Duration::days(1);
    }
    (done, target)
}

/// `[day_of_week][week]`, oldest week first, last column is the current week.
pub fn heatmap(
    habit: &Habit,
    history: &History,
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
                    (date, status_on(habit, history, date, today))
                })
                .collect()
        })
        .collect()
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Cheats, Log};
    use chrono::NaiveDate;
    use std::collections::BTreeSet;

    fn date(text: &str) -> NaiveDate {
        NaiveDate::parse_from_str(text, "%Y-%m-%d").unwrap()
    }

    struct Fixture {
        habit: Habit,
        log: Log,
        cheats: Cheats,
    }

    impl Fixture {
        fn new(schedule: Schedule, created: &str) -> Self {
            Self {
                habit: Habit::new("h".into(), "Habit".into(), schedule, date(created)),
                log: Log::default(),
                cheats: Cheats::default(),
            }
        }

        fn done(mut self, dates: &[&str]) -> Self {
            for day in dates {
                self.log.set(date(day), "h", true);
            }
            self
        }

        fn cheat(mut self, dates: &[&str]) -> Self {
            for day in dates {
                self.cheats.set(date(day), "h", true);
            }
            self
        }

        fn ends(mut self, ends: &str) -> Self {
            self.habit.ends = Some(date(ends));
            self
        }

        fn history(&self) -> History<'_> {
            History {
                log: &self.log,
                cheats: &self.cheats,
            }
        }
    }

    #[test]
    fn daily_streak_counts_consecutive_days() {
        let f = Fixture::new(Schedule::Daily, "2026-11-01").done(&[
            "2026-11-08",
            "2026-11-09",
            "2026-11-10",
        ]);
        let streak = current_streak(&f.habit, &f.history(), date("2026-11-10"), Day::Mon);
        assert_eq!(streak.length, 3);
        assert_eq!(streak.unit, Unit::Days);
    }

    #[test]
    fn today_not_done_keeps_yesterdays_streak_alive() {
        let f = Fixture::new(Schedule::Daily, "2026-11-01").done(&["2026-11-08", "2026-11-09"]);
        let streak = current_streak(&f.habit, &f.history(), date("2026-11-10"), Day::Mon);
        assert_eq!(streak.length, 2);
    }

    #[test]
    fn missing_a_past_day_breaks_the_streak() {
        let f = Fixture::new(Schedule::Daily, "2026-11-01").done(&[
            "2026-11-05",
            "2026-11-07",
            "2026-11-08",
        ]);
        let streak = current_streak(&f.habit, &f.history(), date("2026-11-10"), Day::Mon);
        assert_eq!(streak.length, 0, "the 9th and 10th are missing");
    }

    #[test]
    fn weekday_habits_skip_days_that_are_not_due() {
        // Mon/Wed/Fri habit: 2026-11-09 Mon, 11 Wed, 13 Fri.
        let f = Fixture::new(
            Schedule::Weekdays {
                days: BTreeSet::from([Day::Mon, Day::Wed, Day::Fri]),
            },
            "2026-11-01",
        )
        .done(&["2026-11-06", "2026-11-09", "2026-11-11"]);
        let streak = current_streak(&f.habit, &f.history(), date("2026-11-11"), Day::Mon);
        assert_eq!(streak.length, 3, "Tue/Thu/Sat/Sun do not break it");
    }

    #[test]
    fn weekly_quota_habit_counts_weeks_not_days() {
        let f = Fixture::new(Schedule::TimesPerWeek { n: 4 }, "2026-10-01").done(&[
            "2026-11-02",
            "2026-11-04",
            "2026-11-06",
            "2026-11-07", // previous week: 4
            "2026-11-09",
            "2026-11-10",
            "2026-11-11",
            "2026-11-12", // this week: 4
        ]);
        let streak = current_streak(&f.habit, &f.history(), date("2026-11-13"), Day::Mon);
        assert_eq!(streak.length, 2);
        assert_eq!(streak.unit, Unit::Weeks);
    }

    #[test]
    fn weekly_history_lists_recent_weeks_oldest_first() {
        let f = Fixture::new(Schedule::TimesPerWeek { n: 3 }, "2026-10-19").done(&[
            "2026-10-19",
            "2026-10-20",
            "2026-10-21", // first week: 3
            "2026-10-26", // second week: 1
            "2026-11-02",
            "2026-11-03",
            "2026-11-04",
            "2026-11-05", // third week: 4
            "2026-11-09",
            "2026-11-10", // current week: 2
        ]);
        let weeks = weekly_history(&f.habit, &f.history(), date("2026-11-10"), Day::Mon, 3);
        assert_eq!(
            weeks,
            vec![
                WeekProgress {
                    start: date("2026-10-26"),
                    done: 1,
                    target: 3
                },
                WeekProgress {
                    start: date("2026-11-02"),
                    done: 4,
                    target: 3
                },
                WeekProgress {
                    start: date("2026-11-09"),
                    done: 2,
                    target: 3
                },
            ]
        );
    }

    #[test]
    fn weekly_history_never_starts_before_the_habit_did() {
        let f = Fixture::new(Schedule::TimesPerWeek { n: 2 }, "2026-11-04")
            .done(&["2026-11-04", "2026-11-06"]);
        let weeks = weekly_history(&f.habit, &f.history(), date("2026-11-10"), Day::Mon, 4);
        assert_eq!(weeks.len(), 2);
        assert_eq!(weeks[0].start, date("2026-11-02"));
        assert_eq!(weeks[0].done, 2);
        assert_eq!(weeks[1].start, date("2026-11-09"));
        assert_eq!(weeks[1].done, 0);
    }

    #[test]
    fn weekly_history_is_empty_for_other_schedules() {
        let f = Fixture::new(Schedule::Daily, "2026-11-01");
        assert!(weekly_history(&f.habit, &f.history(), date("2026-11-10"), Day::Mon, 4).is_empty());
    }

    #[test]
    fn weekly_quota_with_three_of_four_does_not_count_but_does_not_break() {
        let f = Fixture::new(Schedule::TimesPerWeek { n: 4 }, "2026-10-01").done(&[
            "2026-11-02",
            "2026-11-04",
            "2026-11-06",
            "2026-11-07",
            // this week only three
            "2026-11-09",
            "2026-11-10",
            "2026-11-11",
        ]);
        let streak = current_streak(&f.habit, &f.history(), date("2026-11-12"), Day::Mon);
        assert_eq!(streak.length, 1, "the in-progress week is skipped");
        let (done, target) =
            weekly_progress(&f.habit, &f.history(), date("2026-11-12"), Day::Mon).unwrap();
        assert_eq!((done, target), (3, 4));
    }

    #[test]
    fn longest_streak_looks_beyond_the_current_run() {
        let f = Fixture::new(Schedule::Daily, "2026-11-01").done(&[
            "2026-11-03",
            "2026-11-04",
            "2026-11-05",
            "2026-11-06",
            "2026-11-07",
            "2026-11-10",
        ]);
        let longest = longest_streak(&f.habit, &f.history(), date("2026-11-10"), Day::Mon);
        assert_eq!(longest.length, 5);
    }

    #[test]
    fn completion_ignores_days_before_creation() {
        let f = Fixture::new(Schedule::Daily, "2026-11-08").done(&["2026-11-08", "2026-11-10"]);
        let completion = completion(&f.habit, &f.history(), date("2026-11-10"), Day::Mon, 30);
        assert_eq!(completion.done, 2);
        assert_eq!(completion.possible, 3);
        assert_eq!(completion.percent(), 66);
    }

    #[test]
    fn heatmap_last_column_contains_today() {
        let f = Fixture::new(Schedule::Daily, "2026-01-01").done(&["2026-11-10"]);
        let grid = heatmap(&f.habit, &f.history(), date("2026-11-10"), Day::Mon, 5);
        assert_eq!(grid.len(), 7);
        assert_eq!(grid[0].len(), 5);
        assert_eq!(grid[0][4].0, date("2026-11-09"));
        assert_eq!(grid[1][4].0, date("2026-11-10"));
        assert_eq!(grid[1][4].1, Status::Done);
    }

    #[test]
    fn heatmap_rows_follow_the_configured_week_start() {
        let f = Fixture::new(Schedule::Daily, "2026-01-01").done(&["2026-11-10"]);
        let grid = heatmap(&f.habit, &f.history(), date("2026-11-10"), Day::Sun, 2);
        assert_eq!(grid[0][1].0, date("2026-11-08"), "first row is Sunday");
        assert_eq!(grid[2][1].0, date("2026-11-10"), "Tuesday is offset 2");
    }

    #[test]
    fn status_marks_future_and_not_due() {
        let f = Fixture::new(
            Schedule::Weekdays {
                days: BTreeSet::from([Day::Mon]),
            },
            "2026-01-01",
        )
        .done(&["2026-11-09"]);
        assert_eq!(
            status_on(
                &f.habit,
                &f.history(),
                date("2026-11-10"),
                date("2026-11-10")
            ),
            Status::NotDue
        );
        assert_eq!(
            status_on(
                &f.habit,
                &f.history(),
                date("2026-11-16"),
                date("2026-11-10")
            ),
            Status::Future
        );
        assert_eq!(
            status_on(
                &f.habit,
                &f.history(),
                date("2026-11-02"),
                date("2026-11-10")
            ),
            Status::Missed
        );
    }

    #[test]
    fn today_that_is_not_done_yet_is_pending_not_missed() {
        let f = Fixture::new(Schedule::Daily, "2026-11-01");
        assert_eq!(
            status_on(
                &f.habit,
                &f.history(),
                date("2026-11-10"),
                date("2026-11-10")
            ),
            Status::Pending
        );
        assert_eq!(
            status_on(
                &f.habit,
                &f.history(),
                date("2026-11-09"),
                date("2026-11-10")
            ),
            Status::Missed
        );
    }

    #[test]
    fn days_before_creation_are_not_missed() {
        let f = Fixture::new(Schedule::Daily, "2026-11-09");
        assert_eq!(
            status_on(
                &f.habit,
                &f.history(),
                date("2026-11-05"),
                date("2026-11-10")
            ),
            Status::NotDue
        );
        assert_eq!(
            status_on(
                &f.habit,
                &f.history(),
                date("2026-11-09"),
                date("2026-11-10")
            ),
            Status::Missed
        );
    }

    #[test]
    fn no_history_scores_zero_without_panicking() {
        let f = Fixture::new(Schedule::Daily, "2026-11-10");
        assert_eq!(
            current_streak(&f.habit, &f.history(), date("2026-11-10"), Day::Mon).length,
            0
        );
        assert_eq!(
            longest_streak(&f.habit, &f.history(), date("2026-11-10"), Day::Mon).length,
            0
        );
        assert_eq!(
            completion(&f.habit, &f.history(), date("2026-11-10"), Day::Mon, 30).possible,
            1
        );
    }

    #[test]
    fn one_cheat_day_a_month_bridges_a_streak() {
        // Done 1-7, cheat on the 8th, done 9-10.
        let f = Fixture::new(Schedule::Daily, "2026-11-01")
            .done(&[
                "2026-11-01",
                "2026-11-02",
                "2026-11-03",
                "2026-11-04",
                "2026-11-05",
                "2026-11-06",
                "2026-11-07",
                "2026-11-09",
                "2026-11-10",
            ])
            .cheat(&["2026-11-08"]);
        let streak = current_streak(&f.habit, &f.history(), date("2026-11-10"), Day::Mon);
        assert_eq!(streak.length, 9, "the cheat day is skipped, not counted");
        assert_eq!(
            status_on(
                &f.habit,
                &f.history(),
                date("2026-11-08"),
                date("2026-11-10")
            ),
            Status::Cheat
        );
    }

    #[test]
    fn a_second_cheat_in_the_same_month_breaks_the_streak() {
        let f = Fixture::new(Schedule::Daily, "2026-11-01")
            .done(&["2026-11-01", "2026-11-02"])
            .cheat(&["2026-11-03", "2026-11-04"])
            .done(&["2026-11-05", "2026-11-06"]);
        let streak = current_streak(&f.habit, &f.history(), date("2026-11-06"), Day::Mon);
        assert_eq!(streak.length, 2, "the second cheat is an ordinary miss");
        assert_eq!(
            status_on(
                &f.habit,
                &f.history(),
                date("2026-11-04"),
                date("2026-11-06")
            ),
            Status::Missed
        );
    }

    #[test]
    fn cheats_do_not_count_as_completions() {
        let f = Fixture::new(Schedule::Daily, "2026-11-01").cheat(&["2026-11-01"]);
        let completion = completion(&f.habit, &f.history(), date("2026-11-02"), Day::Mon, 30);
        assert_eq!(completion.done, 0);
        assert_eq!(completion.possible, 1, "the excused day is skipped");
    }

    #[test]
    fn an_end_date_freezes_the_streak() {
        let f = Fixture::new(Schedule::Daily, "2026-11-01")
            .done(&["2026-11-01", "2026-11-02", "2026-11-03"])
            .ends("2026-11-03");
        // A week later the last day is still the one that decides the streak.
        let streak = current_streak(&f.habit, &f.history(), date("2026-11-10"), Day::Mon);
        assert_eq!(streak.length, 3);
        assert_eq!(
            status_on(
                &f.habit,
                &f.history(),
                date("2026-11-05"),
                date("2026-11-10")
            ),
            Status::NotDue
        );
    }

    #[test]
    fn a_finished_habit_still_counts_a_missed_last_day() {
        let f = Fixture::new(Schedule::Daily, "2026-11-01")
            .done(&["2026-11-01", "2026-11-02"])
            .ends("2026-11-05");
        // The 3rd-5th were inside the window and missed, so the run is over.
        let streak = current_streak(&f.habit, &f.history(), date("2026-11-10"), Day::Mon);
        assert_eq!(streak.length, 0);
    }

    #[test]
    fn completion_stops_at_the_end_date() {
        let f = Fixture::new(Schedule::Daily, "2026-11-01")
            .done(&["2026-11-01", "2026-11-02"])
            .ends("2026-11-02");
        let completion = completion(&f.habit, &f.history(), date("2026-11-10"), Day::Mon, 30);
        assert_eq!(completion.possible, 2);
        assert_eq!(completion.percent(), 100);
    }

    #[test]
    fn week_progress_stops_at_the_end_date() {
        let f = Fixture::new(Schedule::Daily, "2026-11-01")
            .done(&["2026-11-02"])
            .ends("2026-11-03");
        // Week of 2026-11-02: only Monday and Tuesday are inside the window.
        let (done, target) = week_progress(&f.habit, &f.history(), date("2026-11-10"), Day::Mon);
        assert_eq!((done, target), (1, 2));
    }

    #[test]
    fn weekly_streak_does_not_break_because_the_habit_ended_mid_week() {
        let f = Fixture::new(Schedule::TimesPerWeek { n: 2 }, "2026-10-01")
            .done(&[
                "2026-10-26",
                "2026-10-28", // full week
                "2026-11-02",
                "2026-11-04", // full week
                "2026-11-09", // ends mid-week
            ])
            .ends("2026-11-10");
        let streak = current_streak(&f.habit, &f.history(), date("2026-11-20"), Day::Mon);
        assert_eq!(streak.length, 2, "the partial final week is not fatal");
    }

    #[test]
    fn week_progress_does_not_require_an_excused_day() {
        let f = Fixture::new(Schedule::Daily, "2026-11-09")
            .done(&["2026-11-09"])
            .cheat(&["2026-11-10"]);
        let (done, target) = week_progress(&f.habit, &f.history(), date("2026-11-10"), Day::Mon);
        assert_eq!(
            (done, target),
            (1, 1),
            "the cheat is neither done nor required"
        );
    }
}
