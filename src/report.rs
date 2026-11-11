//! A self-contained monthly HTML report.
//!
//! Everything is computed from `(data, today, month)` and written as one file:
//! no assets, no scripts, no network. The page uses the same status vocabulary
//! as the TUI (done, cheat, missed, open) so it reads like a printable version
//! of the detail pane.

use std::path::{Path, PathBuf};

use anyhow::{Result, bail};
use chrono::{Datelike, Duration, Months, NaiveDate};

use crate::format::plural;
use crate::model::{Data, Day, Habit, Schedule, start_of_week};
use crate::stats::{self, Status};

/// The directory that holds reports beside the data file.
pub fn default_path(data_path: &Path) -> PathBuf {
    data_path
        .parent()
        .map(|parent| parent.join("reports"))
        .unwrap_or_else(|| PathBuf::from("reports"))
}

/// `.../reports/habit-YYYY-MM.html`.
pub fn path_for(data_path: &Path, month: NaiveDate) -> PathBuf {
    default_path(data_path).join(format!("habit-{}.html", month.format("%Y-%m")))
}

/// Parse `YYYY-MM` into the first day of that month.
pub fn month_start(text: &str) -> Result<NaiveDate> {
    let text = text.trim();
    let (year, month) = text
        .split_once('-')
        .ok_or_else(|| anyhow::anyhow!("unknown month \"{text}\" (use YYYY-MM)"))?;
    let year: i32 = year
        .parse()
        .map_err(|_| anyhow::anyhow!("unknown month \"{text}\" (use YYYY-MM)"))?;
    let month: u32 = month
        .parse()
        .map_err(|_| anyhow::anyhow!("unknown month \"{text}\" (use YYYY-MM)"))?;
    let date = NaiveDate::from_ymd_opt(year, month, 1);
    match date {
        Some(date) if text.len() == 7 => Ok(date),
        _ => bail!("unknown month \"{text}\" (use YYYY-MM)"),
    }
}

/// The first day of the month before `today`.
pub fn last_month(today: NaiveDate) -> NaiveDate {
    let first = today.with_day(1).unwrap_or(today);
    first.checked_sub_months(Months::new(1)).unwrap_or(first)
}

fn month_end(month: NaiveDate) -> NaiveDate {
    let next = if month.month() == 12 {
        NaiveDate::from_ymd_opt(month.year() + 1, 1, 1)
    } else {
        NaiveDate::from_ymd_opt(month.year(), month.month() + 1, 1)
    };
    next.and_then(|date| date.pred_opt()).unwrap_or(month)
}

struct Row<'a> {
    habit: &'a Habit,
    done: u32,
    possible: u32,
    cheats: u32,
    quota: Option<(u32, u32)>,
    streak: String,
    days: Vec<(NaiveDate, Status)>,
}

fn rows<'a>(data: &'a Data, today: NaiveDate, month: NaiveDate, week_start: Day) -> Vec<Row<'a>> {
    let history = data.history();
    let start_of_month = month.with_day(1).unwrap_or(month);
    let end_of_month = month_end(start_of_month);
    let mut rows = Vec::new();

    for habit in &data.habits {
        let start = start_of_month.max(habit.created);
        let end = end_of_month.min(today).min(habit.ends.unwrap_or(today));
        if start > end {
            continue;
        }

        let mut cheats = 0;
        let mut date = start;
        while date <= end {
            if history.cheats.is_excused(date, &habit.id) {
                cheats += 1;
            }
            date += Duration::days(1);
        }

        let (done, possible, quota) = match habit.schedule {
            Schedule::TimesPerWeek { n } => {
                let mut done = 0;
                let mut met = 0;
                let mut weeks = 0;
                let mut week = start_of_week(start, week_start);
                while week <= end {
                    let from = week.max(start);
                    let to = (week + Duration::days(6)).min(end);
                    let count = history.log.count_between(&habit.id, from, to);
                    done += count;
                    if count >= u32::from(n) {
                        met += 1;
                    }
                    weeks += 1;
                    week += Duration::days(7);
                }
                (done, weeks * u32::from(n), Some((met, weeks)))
            }
            _ => {
                let mut done = 0;
                let mut possible = 0;
                let mut date = start;
                while date <= end {
                    if habit.schedule.is_due(date)
                        && habit.is_active_on(date)
                        && !history.cheats.is_excused(date, &habit.id)
                    {
                        possible += 1;
                        if history.log.is_done(date, &habit.id) {
                            done += 1;
                        }
                    }
                    date += Duration::days(1);
                }
                (done, possible, None)
            }
        };

        let streak = stats::current_streak(habit, &history, end, week_start).label();
        let days = (0..=end_of_month
            .signed_duration_since(start_of_month)
            .num_days())
            .map(|offset| {
                let date = start_of_month + Duration::days(offset);
                (date, stats::status_on(habit, &history, date, today))
            })
            .collect();

        rows.push(Row {
            habit,
            done,
            possible,
            cheats,
            quota,
            streak,
            days,
        });
    }
    rows
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

fn percent(done: u32, possible: u32) -> u32 {
    if possible == 0 {
        0
    } else {
        ((u64::from(done) * 100) / u64::from(possible)).min(100) as u32
    }
}

fn status_label(status: Status) -> &'static str {
    match status {
        Status::Done => "done",
        Status::Cheat => "excused",
        Status::Missed => "missed",
        Status::Pending => "open",
        Status::NotDue | Status::Future => "not due",
    }
}

fn status_class(status: Status) -> &'static str {
    match status {
        Status::Done => "done",
        Status::Cheat => "cheat",
        Status::Missed => "missed",
        Status::Pending => "pending",
        Status::NotDue | Status::Future => "muted",
    }
}

/// Render the complete report for the month containing `month`.
pub fn render(data: &Data, today: NaiveDate, month: NaiveDate, week_start: Day) -> String {
    let month = month.with_day(1).unwrap_or(month);
    let month_name = month.format("%B %Y").to_string();
    let rows = rows(data, today, month, week_start);
    let done: u32 = rows.iter().map(|row| row.done).sum();
    let possible: u32 = rows.iter().map(|row| row.possible).sum();
    let cheats: u32 = rows.iter().map(|row| row.cheats).sum();

    let start_offset =
        (month.weekday().num_days_from_monday() as i64 - week_start.index() as i64).rem_euclid(7);
    let weekday_labels = (0..7)
        .map(|offset| Day::from_index(week_start.index() + offset).short())
        .collect::<Vec<_>>();

    let mut habits_html = String::new();
    for row in &rows {
        let habit = row.habit;
        let schedule = habit.schedule.describe();
        let tags = if habit.tags.is_empty() {
            String::new()
        } else {
            habit
                .tags
                .iter()
                .map(|tag| format!(" <span class=\"tag\">#{}</span>", escape(tag)))
                .collect()
        };
        let detail = match row.quota {
            Some((met, weeks)) => format!("{} check-ins · {met}/{weeks} weeks met", row.done),
            None => format!("{}/{} days", row.done, row.possible),
        };
        let cheat_line = if row.cheats > 0 {
            format!(" · {} excused", plural(row.cheats, "day"))
        } else {
            String::new()
        };

        let mut cells = String::new();
        for _ in 0..start_offset {
            cells.push_str("<span class=\"day blank\"></span>");
        }
        for (date, status) in &row.days {
            let class = status_class(*status);
            cells.push_str(&format!(
                "<span class=\"day {class}\" title=\"{} · {}\">{}</span>",
                date.format("%a %-d %b"),
                status_label(*status),
                date.day()
            ));
        }

        habits_html.push_str(&format!(
            r#"<article class="habit">
  <div class="habit-head">
    <h2>{name}{archived}</h2>
    <p>{schedule}{tags}</p>
  </div>
  <div class="stats">
    <span><strong>{percent}%</strong> of target</span>
    <span>{detail}{cheat_line}</span>
    <span>streak {streak}</span>
  </div>
  <div class="weekdays">{weekdays}</div>
  <div class="grid">{cells}</div>
</article>
"#,
            name = escape(&habit.name),
            archived = if habit.archived {
                " <span class=\"badge\">archived</span>"
            } else if habit.is_finished(today) {
                " <span class=\"badge\">finished</span>"
            } else {
                ""
            },
            schedule = escape(&schedule),
            tags = tags,
            percent = percent(row.done, row.possible),
            detail = escape(&detail),
            cheat_line = escape(&cheat_line),
            streak = escape(&row.streak),
            weekdays = weekday_labels
                .iter()
                .map(|label| format!("<span>{label}</span>"))
                .collect::<String>(),
            cells = cells,
        ));
    }

    if rows.is_empty() {
        habits_html.push_str("<p class=\"empty\">No habits were running in this month.</p>");
    }

    format!(
        r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Habit report · {month_name}</title>
<style>
:root {{
  color-scheme: light dark;
  --bg: #f6f4ef; --card: #fffdf8; --ink: #28251f; --muted: #7c766a;
  --line: #e3ded2; --done: #3f8a5c; --cheat: #3f7f99; --missed: #b4574f;
  --pending: #c58b2d; --blank: #f0ece3;
}}
@media (prefers-color-scheme: dark) {{
  :root {{
    --bg: #191a18; --card: #212320; --ink: #ece9e0; --muted: #a29d90;
    --line: #34362f; --done: #72b98c; --cheat: #72aec2; --missed: #d67f76;
    --pending: #d8ab5f; --blank: #292b27;
  }}
}}
* {{ box-sizing: border-box; }}
body {{
  margin: 0; padding: 2rem 1rem; background: var(--bg); color: var(--ink);
  font: 15px/1.5 ui-sans-serif, -apple-system, "Segoe UI", sans-serif;
}}
main {{ max-width: 880px; margin: 0 auto; }}
.eyebrow {{ margin: 0; color: var(--muted); letter-spacing: .14em; text-transform: uppercase; font-size: 11px; }}
h1 {{ margin: .2rem 0 .3rem; font-size: clamp(2rem, 5vw, 3.2rem); letter-spacing: -.03em; }}
.meta {{ margin: 0 0 2rem; color: var(--muted); }}
.summary {{ display: grid; grid-template-columns: repeat(auto-fit, minmax(140px, 1fr)); gap: 1px; background: var(--line); border: 1px solid var(--line); border-radius: 12px; overflow: hidden; margin-bottom: 2rem; }}
.summary div {{ background: var(--card); padding: 1rem 1.2rem; }}
.summary strong {{ display: block; font-size: 1.6rem; }}
.summary span {{ color: var(--muted); font-size: 13px; }}
.habit {{ background: var(--card); border: 1px solid var(--line); border-radius: 14px; padding: 1.2rem 1.4rem; margin-bottom: 1rem; }}
.habit-head {{ display: flex; flex-wrap: wrap; justify-content: space-between; gap: .3rem 1rem; align-items: baseline; }}
h2 {{ margin: 0; font-size: 1.15rem; }}
.habit-head p {{ margin: 0; color: var(--muted); font-size: 13px; }}
.tag {{ margin-left: .35rem; padding: .05rem .4rem; border: 1px solid var(--line); border-radius: 999px; font-size: 11px; }}
.badge {{ margin-left: .45rem; padding: .05rem .45rem; border-radius: 999px; background: var(--blank); color: var(--muted); font-size: 11px; font-weight: 400; vertical-align: middle; }}
.stats {{ display: flex; flex-wrap: wrap; gap: .3rem 1.2rem; margin: .8rem 0 .9rem; color: var(--muted); font-size: 13px; }}
.stats strong {{ color: var(--ink); }}
.weekdays, .grid {{ display: grid; grid-template-columns: repeat(7, 1fr); gap: 5px; max-width: 620px; }}
.weekdays span {{ color: var(--muted); font-size: 10px; text-align: center; }}
.day {{ display: flex; align-items: center; justify-content: center; padding: .45rem 0; border-radius: 5px; background: var(--blank); font-size: 12px; }}
.day.blank {{ background: transparent; }}
.day.done {{ background: var(--done); color: #fff; }}
.day.cheat {{ background: var(--cheat); color: #fff; }}
.day.missed {{ background: var(--missed); color: #fff; }}
.day.pending {{ background: var(--pending); color: #fff; }}
.day.muted {{ color: var(--muted); }}
.empty {{ color: var(--muted); }}
footer {{ margin-top: 2rem; color: var(--muted); font-size: 12px; }}
@media print {{
  body {{ background: #fff; color: #000; padding: 0; }}
  .habit, .summary div {{ break-inside: avoid; }}
}}
</style>
</head>
<body>
<main>
<header>
  <p class="eyebrow">habit · monthly report</p>
  <h1>{month_name}</h1>
  <p class="meta">Generated {generated} · {habits} · {checkins} · {completion}% of target</p>
</header>
<section class="summary">
  <div><strong>{habits_count}</strong><span>{habits}</span></div>
  <div><strong>{done}</strong><span>check-ins</span></div>
  <div><strong>{completion}%</strong><span>of target</span></div>
  <div><strong>{cheats}</strong><span>{cheat_days}</span></div>
</section>
<section class="habits">
{habits_html}</section>
<footer>Generated by habit {version}. One JSON file, no account and no network.</footer>
</main>
</body>
</html>
"#,
        month_name = escape(&month_name),
        generated = today.format("%a %-d %b %Y"),
        habits = plural(rows.len() as u32, "habit"),
        checkins = plural(done, "check-in"),
        completion = percent(done, possible),
        habits_count = rows.len(),
        done = done,
        cheats = cheats,
        cheat_days = plural(cheats, "excused day"),
        habits_html = habits_html,
        version = env!("CARGO_PKG_VERSION"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Schedule;

    fn date(text: &str) -> NaiveDate {
        NaiveDate::parse_from_str(text, "%Y-%m-%d").unwrap()
    }

    #[test]
    fn months_parse_and_step_back() {
        assert_eq!(month_start("2026-11").unwrap(), date("2026-11-01"));
        assert_eq!(last_month(date("2026-01-15")), date("2025-12-01"));
        assert_eq!(last_month(date("2026-03-31")), date("2026-02-01"));
        assert!(month_start("2026").is_err());
        assert!(month_start("2026-13").is_err());
    }

    #[test]
    fn paths_live_beside_the_data_file() {
        let path = path_for(
            std::path::Path::new("/tmp/habit/data.json"),
            date("2026-11-01"),
        );
        assert_eq!(
            path,
            std::path::PathBuf::from("/tmp/habit/reports/habit-2026-11.html")
        );
    }

    #[test]
    fn the_report_shows_month_totals_and_each_habit() {
        let today = date("2026-11-10");
        let mut data = Data::default();
        data.add_habit("Gym", Schedule::Daily, date("2026-11-01"))
            .unwrap();
        for day in 1..=5 {
            data.log
                .set(date(&format!("2026-11-{day:02}")), "gym", true);
        }
        let html = render(&data, today, date("2026-11-01"), Day::Mon);
        assert!(html.contains("<h1>November 2026</h1>"));
        assert!(html.contains("Gym"));
        assert!(html.contains("<strong>5</strong><span>check-ins</span>"));
        assert!(html.contains("<strong>50%</strong><span>of target</span>"));
        assert!(html.contains("class=\"day done\""));
        assert!(html.contains("class=\"day muted\""));
    }

    #[test]
    fn future_days_do_not_count_against_completion() {
        let today = date("2026-11-10");
        let mut data = Data::default();
        data.add_habit("Gym", Schedule::Daily, date("2026-11-01"))
            .unwrap();
        for day in 1..=10 {
            data.log
                .set(date(&format!("2026-11-{day:02}")), "gym", true);
        }
        let html = render(&data, today, date("2026-11-01"), Day::Mon);
        assert!(html.contains("<strong>100%</strong><span>of target</span>"));
    }

    #[test]
    fn quota_habits_report_weeks_met() {
        let today = date("2026-11-30");
        let mut data = Data::default();
        data.add_habit("Gym", Schedule::TimesPerWeek { n: 2 }, date("2026-11-01"))
            .unwrap();
        for day in ["2026-11-02", "2026-11-04"] {
            data.log.set(date(day), "gym", true);
        }
        let html = render(&data, today, date("2026-11-01"), Day::Mon);
        assert!(html.contains("2 check-ins"));
        assert!(html.contains("1/6 weeks met"));
    }

    #[test]
    fn names_and_tags_are_escaped() {
        let today = date("2026-11-10");
        let mut data = Data::default();
        let index = data
            .add_habit("<Gym & \"co\">", Schedule::Daily, date("2026-11-01"))
            .unwrap();
        data.habits[index].tags = vec!["health & <fitness>".into()];
        let html = render(&data, today, date("2026-11-01"), Day::Mon);
        assert!(html.contains("&lt;Gym &amp; &quot;co&quot;&gt;"));
        assert!(html.contains("#health &amp; &lt;fitness&gt;"));
        assert!(!html.contains("<Gym"));
    }

    #[test]
    fn an_empty_month_still_produces_a_page() {
        let html = render(
            &Data::default(),
            date("2026-11-10"),
            date("2026-11-01"),
            Day::Mon,
        );
        assert!(html.contains("No habits were running in this month."));
    }

    #[test]
    fn archived_habits_stay_in_the_month_they_ran() {
        let today = date("2026-11-10");
        let mut data = Data::default();
        data.add_habit("Gym", Schedule::Daily, date("2026-11-01"))
            .unwrap();
        data.habits[0].archived = true;
        data.log.set(date("2026-11-02"), "gym", true);
        let html = render(&data, today, date("2026-11-01"), Day::Mon);
        assert!(html.contains("archived"));
        assert!(html.contains("Gym"));
    }
}
