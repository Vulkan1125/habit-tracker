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
use crate::theme::{ColorMode, ThemeName, WebPalette, web_palette};

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
                    let count = history
                        .log
                        .reached_between(&habit.id, habit.per_day(), from, to);
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
                        if history.reached(habit, date) {
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

/// One day of the month-wide performance series used by the report chart.
struct DayPoint {
    date: NaiveDate,
    done: u32,
    possible: u32,
    /// Journal entries written for this date, as `(habit name, note)`.
    notes: Vec<(String, String)>,
}

/// The days of `month` that have happened, with how much of what was expected
/// got done. Quota habits count only on days they were checked off; days with
/// nothing due stay in the series so the chart has a continuous x-axis.
fn performance(data: &Data, today: NaiveDate, month: NaiveDate) -> Vec<DayPoint> {
    let history = data.history();
    let start = month.with_day(1).unwrap_or(month);
    let end = month_end(start).min(today);
    if start > end {
        return Vec::new();
    }

    let mut points = Vec::new();
    let mut date = start;
    while date <= end {
        let mut done = 0;
        let mut possible = 0;
        let mut notes = Vec::new();
        for habit in &data.habits {
            match stats::status_on(habit, &history, date, today) {
                Status::Done => {
                    done += 1;
                    possible += 1;
                }
                Status::Missed | Status::Pending | Status::Partial => possible += 1,
                _ => {}
            }
            if let Some(note) = habit.note_on(date) {
                notes.push((habit.name.clone(), note.to_string()));
            }
        }
        points.push(DayPoint {
            date,
            done,
            possible,
            notes,
        });
        date += Duration::days(1);
    }
    points
}

/// A seven-day rolling average of the daily completion percentage. Days with
/// nothing due do not drag the average down.
fn rolling_average(points: &[DayPoint], window: usize) -> Vec<Option<u32>> {
    let window = window.max(1);
    points
        .iter()
        .enumerate()
        .map(|(index, _)| {
            let from = index.saturating_sub(window - 1);
            let values: Vec<u32> = points[from..=index]
                .iter()
                .filter(|point| point.possible > 0)
                .map(|point| percent(point.done, point.possible))
                .collect();
            if values.is_empty() {
                None
            } else {
                Some(values.iter().sum::<u32>() / values.len() as u32)
            }
        })
        .collect()
}

fn points_attr(points: &[(f64, f64)]) -> String {
    points
        .iter()
        .map(|(x, y)| format!("{x:.1},{y:.1}"))
        .collect::<Vec<_>>()
        .join(" ")
}

/// A no-JavaScript SVG line chart: the pale line is each day, the strong line
/// is the seven-day average, and hovering a day reveals its numbers and notes.
fn chart_svg(points: &[DayPoint]) -> String {
    if points.len() < 2 {
        return String::new();
    }

    const WIDTH: f64 = 720.0;
    const HEIGHT: f64 = 220.0;
    const LEFT: f64 = 36.0;
    const RIGHT: f64 = 14.0;
    const TOP: f64 = 14.0;
    const BOTTOM: f64 = 26.0;
    let inner_width = WIDTH - LEFT - RIGHT;
    let inner_height = HEIGHT - TOP - BOTTOM;
    let x = |index: usize| LEFT + inner_width * index as f64 / (points.len() - 1) as f64;
    let y = |value: u32| TOP + inner_height * (1.0 - f64::from(value) / 100.0);

    let daily: Vec<(f64, f64)> = points
        .iter()
        .enumerate()
        .filter(|(_, point)| point.possible > 0)
        .map(|(index, point)| (x(index), y(percent(point.done, point.possible))))
        .collect();
    let average: Vec<(f64, f64)> = rolling_average(points, 7)
        .into_iter()
        .enumerate()
        .filter_map(|(index, value)| value.map(|value| (x(index), y(value))))
        .collect();

    let mut grid = String::new();
    for value in [0, 25, 50, 75, 100] {
        grid.push_str(&format!(
            "<line x1=\"{LEFT}\" y1=\"{y}\" x2=\"{right}\" y2=\"{y}\" class=\"chart-grid\"/>\
             <text x=\"{label}\" y=\"{text_y}\" text-anchor=\"end\">{value}%</text>",
            y = y(value),
            right = WIDTH - RIGHT,
            label = LEFT - 7.0,
            text_y = y(value) + 4.0,
        ));
    }

    let mut hover = String::new();
    let cell = inner_width / points.len() as f64;
    for (index, point) in points.iter().enumerate() {
        let day = point.date.format("%a %-d %b");
        let mut label = if point.possible == 0 {
            format!("{day} · nothing due")
        } else {
            format!(
                "{day} · {}% ({}/{})",
                percent(point.done, point.possible),
                point.done,
                point.possible
            )
        };
        if !point.notes.is_empty() {
            let notes = point
                .notes
                .iter()
                .map(|(habit, note)| format!("{habit}: {}", note.replace('\n', " ")))
                .collect::<Vec<_>>()
                .join(" · ");
            label.push_str(&format!(" · {notes}"));
        }
        hover.push_str(&format!(
            "<rect x=\"{x:.1}\" y=\"{TOP}\" width=\"{cell:.1}\" height=\"{inner_height}\" \
             class=\"hover\"><title>{label}</title></rect>",
            x = LEFT + cell * index as f64,
            label = escape(&label),
        ));
    }

    let area = if average.is_empty() {
        String::new()
    } else {
        let baseline = HEIGHT - BOTTOM;
        format!(
            "<path d=\"M {first_x:.1} {baseline:.1} L {line} L {last_x:.1} {baseline:.1} Z\" \
             class=\"area\"/>",
            first_x = average[0].0,
            last_x = average[average.len() - 1].0,
            line = average
                .iter()
                .map(|(x, y)| format!("{x:.1} {y:.1}"))
                .collect::<Vec<_>>()
                .join(" L "),
            baseline = baseline,
        )
    };

    let first_label = points[0].date.format("%-d %b");
    let last_label = points[points.len() - 1].date.format("%-d %b");

    format!(
        r#"<svg class="chart" viewBox="0 0 {WIDTH} {HEIGHT}" role="img" aria-label="Daily completion and seven-day average for the month">
  <defs>
    <linearGradient id="chart-fill" x1="0" y1="0" x2="0" y2="1">
      <stop offset="0" stop-color="currentColor" stop-opacity=".22"/>
      <stop offset="1" stop-color="currentColor" stop-opacity="0"/>
    </linearGradient>
  </defs>
  {grid}
  <line x1="{LEFT}" y1="{top}" x2="{right}" y2="{top}" class="target"/>
  {area}
  <polyline points="{daily}" class="daily"/>
  <polyline points="{average}" class="average"/>
  {hover}
  <text x="{LEFT}" y="{axis_y}" class="axis">{first_label}</text>
  <text x="{right}" y="{axis_y}" class="axis" text-anchor="end">{last_label}</text>
</svg>"#,
        grid = grid,
        top = TOP,
        right = WIDTH - RIGHT,
        area = area,
        daily = points_attr(&daily),
        average = points_attr(&average),
        hover = hover,
        axis_y = HEIGHT - 7.0,
        first_label = escape(&first_label.to_string()),
        last_label = escape(&last_label.to_string()),
    )
}

/// Dated journal entries inside the report month, newest first.
fn notes_in_month(habit: &Habit, month: NaiveDate) -> Vec<(NaiveDate, &str)> {
    let start = month.with_day(1).unwrap_or(month);
    let end = month_end(start);
    habit
        .notes
        .iter()
        .rev()
        .filter(|(date, _)| **date >= start && **date <= end)
        .map(|(date, text)| (*date, text.as_str()))
        .collect()
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
        Status::Partial => "partial",
        Status::Cheat => "excused",
        Status::Missed => "missed",
        Status::Pending => "open",
        Status::NotDue | Status::Future => "not due",
    }
}

fn status_class(status: Status) -> &'static str {
    match status {
        Status::Done => "done",
        Status::Partial => "pending",
        Status::Cheat => "cheat",
        Status::Missed => "missed",
        Status::Pending => "pending",
        Status::NotDue | Status::Future => "muted",
    }
}

/// One palette as CSS custom properties.
fn palette_vars(palette: &WebPalette) -> String {
    format!(
        "--bg: {bg}; --card: {card}; --ink: {ink}; --muted: {muted};\n  \
         --line: {line}; --blank: {blank}; --accent: {accent}; --done: {done};\n  \
         --cheat: {cheat}; --missed: {missed}; --pending: {pending};",
        bg = palette.bg.hex(),
        card = palette.card.hex(),
        ink = palette.ink.hex(),
        muted = palette.muted.hex(),
        line = palette.line.hex(),
        blank = palette.blank.hex(),
        accent = palette.accent.hex(),
        done = palette.done.hex(),
        cheat = palette.cheat.hex(),
        missed = palette.missed.hex(),
        pending = palette.pending.hex(),
    )
}

/// Every palette, so a report can be repainted from inside the file: change
/// `data-palette` on `<html>`, or edit the hex values, and nothing else.
fn palette_css(active: ThemeName) -> String {
    let light = |name: ThemeName| palette_vars(&web_palette(name, ColorMode::Light));
    let dark = |name: ThemeName| palette_vars(&web_palette(name, ColorMode::Dark));

    let mut css = String::from(
        "/* Palettes. The page opens with the palette saved in habit: change\n   \
         data-palette on <html> to repaint it, or edit the hex values below. */\n",
    );
    css.push_str(&format!(
        ":root {{\n  color-scheme: light dark;\n  --shadow: rgba(20, 18, 12, .06);\n  {}\n}}\n",
        light(active)
    ));
    for name in ThemeName::ALL {
        css.push_str(&format!(
            "[data-palette=\"{}\"] {{\n  {}\n}}\n",
            name.label(),
            light(name)
        ));
    }
    css.push_str(
        "@media (prefers-color-scheme: dark) {\n  :root {\n    --shadow: rgba(0, 0, 0, .35);\n    ",
    );
    css.push_str(&dark(active).replace('\n', "\n  "));
    css.push_str("\n  }\n");
    for name in ThemeName::ALL {
        css.push_str(&format!(
            "  [data-palette=\"{}\"] {{\n    {}\n  }}\n",
            name.label(),
            dark(name).replace('\n', "\n  ")
        ));
    }
    css.push_str("}\n");
    css
}

/// Render the complete report for the month containing `month`.
///
/// `theme` picks the palette; the light or dark variant follows the viewer's
/// preference, the way a document should.
pub fn render(
    data: &Data,
    today: NaiveDate,
    month: NaiveDate,
    week_start: Day,
    theme: ThemeName,
) -> String {
    let month = month.with_day(1).unwrap_or(month);
    let month_name = month.format("%B %Y").to_string();
    let rows = rows(data, today, month, week_start);
    let done: u32 = rows.iter().map(|row| row.done).sum();
    let possible: u32 = rows.iter().map(|row| row.possible).sum();
    let cheats: u32 = rows.iter().map(|row| row.cheats).sum();
    let series = performance(data, today, month);
    let chart = chart_svg(&series);
    let palette_css = palette_css(theme);
    let chart_html = if chart.is_empty() {
        String::new()
    } else {
        format!(
            r#"<section class="chart-card">
  <div class="chart-head">
    <div>
      <p class="eyebrow">performance</p>
      <h2>Month at a glance</h2>
    </div>
    <p class="legend"><span class="key daily"></span> daily <span class="key average"></span> 7-day average</p>
  </div>
  {chart}
</section>
"#
        )
    };

    let start_offset =
        (month.weekday().num_days_from_monday() as i64 - week_start.index() as i64).rem_euclid(7);
    let weekday_labels = (0..7)
        .map(|offset| Day::from_index(week_start.index() + offset).short())
        .collect::<Vec<_>>();

    let mut habits_html = String::new();
    for row in &rows {
        let habit = row.habit;
        let schedule = if habit.per_day() > 1 {
            format!("{} · {}/day", habit.schedule.describe(), habit.per_day())
        } else {
            habit.schedule.describe()
        };
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
        let description = habit
            .note
            .as_deref()
            .map(|text| {
                format!(
                    "<p class=\"description\">{}</p>",
                    escape(text).replace('\n', "<br>")
                )
            })
            .unwrap_or_default();

        let mut cells = String::new();
        for _ in 0..start_offset {
            cells.push_str("<span class=\"day blank\"></span>");
        }
        for (date, status) in &row.days {
            let mut class = status_class(*status).to_string();
            let mut title = format!("{} · {}", date.format("%a %-d %b"), status_label(*status));
            if let Some(note) = habit.note_on(*date) {
                class.push_str(" has-note");
                title.push_str(" · ");
                title.push_str(&note.replace('\n', " "));
            }
            cells.push_str(&format!(
                "<span class=\"day {class}\" title=\"{}\">{}</span>",
                escape(&title),
                date.day()
            ));
        }

        let notes = notes_in_month(habit, month);
        let notes_html = if notes.is_empty() {
            String::new()
        } else {
            let items = notes
                .iter()
                .map(|(date, text)| {
                    format!(
                        "<li><time>{}</time><span>{}</span></li>",
                        date.format("%a %-d %b"),
                        escape(text).replace('\n', "<br>")
                    )
                })
                .collect::<String>();
            format!("<div class=\"notes\"><h3>Notes</h3><ul>{items}</ul></div>")
        };

        habits_html.push_str(&format!(
            r#"<article class="habit">
  <div class="habit-head">
    <div>
      <h2>{name}{archived}</h2>
      <p>{schedule}{tags}</p>
    </div>
    <div class="score"><strong>{percent}%</strong><span>of target</span></div>
  </div>
  {description}
  <div class="stats">
    <span>{detail}{cheat_line}</span>
    <span>streak {streak}</span>
  </div>
  <div class="weekdays">{weekdays}</div>
  <div class="grid">{cells}</div>
  {notes_html}
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
            description = description,
            detail = escape(&detail),
            cheat_line = escape(&cheat_line),
            streak = escape(&row.streak),
            weekdays = weekday_labels
                .iter()
                .map(|label| format!("<span>{label}</span>"))
                .collect::<String>(),
            cells = cells,
            notes_html = notes_html,
        ));
    }

    if rows.is_empty() {
        habits_html.push_str("<p class=\"empty\">No habits were running in this month.</p>");
    }

    format!(
        r#"<!doctype html>
<html lang="en" data-palette="{palette}">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Habit report · {month_name}</title>
<style>
{palette_css}* {{ box-sizing: border-box; }}
body {{
  margin: 0; padding: 2.5rem 1.25rem 3rem; background: var(--bg); color: var(--ink);
  font: 15px/1.55 ui-sans-serif, -apple-system, "Segoe UI", sans-serif;
}}
main {{ max-width: 900px; margin: 0 auto; }}
.eyebrow {{ margin: 0; color: var(--muted); letter-spacing: .16em; text-transform: uppercase; font-size: 11px; font-weight: 600; }}
h1 {{ margin: .3rem 0 .5rem; font-size: clamp(2.2rem, 6vw, 3.6rem); letter-spacing: -.035em; line-height: 1.05; }}
.meta {{ margin: 0; color: var(--muted); }}
.hero {{ display: flex; flex-wrap: wrap; align-items: center; justify-content: space-between; gap: 1.5rem; padding-bottom: 1.8rem; }}
.ring {{
  --pct: 0; position: relative; width: 128px; height: 128px; border-radius: 50%; flex: none;
  background: conic-gradient(var(--done) calc(var(--pct) * 1%), var(--blank) 0);
  display: grid; place-items: center;
}}
.ring::after {{ content: ""; position: absolute; inset: 9px; border-radius: 50%; background: var(--bg); }}
.ring div {{ position: relative; z-index: 1; text-align: center; }}
.ring strong {{ display: block; font-size: 1.7rem; letter-spacing: -.02em; }}
.ring span {{ color: var(--muted); font-size: 11px; text-transform: uppercase; letter-spacing: .1em; }}
.summary {{ display: grid; grid-template-columns: repeat(auto-fit, minmax(140px, 1fr)); gap: 1px; background: var(--line); border: 1px solid var(--line); border-radius: 16px; overflow: hidden; margin-bottom: 1.5rem; box-shadow: 0 10px 30px var(--shadow); }}
.summary div {{ background: var(--card); padding: 1.1rem 1.3rem; }}
.summary strong {{ display: block; font-size: 1.7rem; line-height: 1.2; }}
.summary span {{ color: var(--muted); font-size: 12px; text-transform: uppercase; letter-spacing: .07em; }}
.chart-card {{ background: var(--card); border: 1px solid var(--line); border-radius: 16px; padding: 1.3rem 1.5rem .9rem; margin-bottom: 1.5rem; box-shadow: 0 10px 30px var(--shadow); }}
.chart-head {{ display: flex; flex-wrap: wrap; align-items: baseline; justify-content: space-between; gap: .4rem 1rem; margin-bottom: .4rem; }}
.chart-head h2 {{ font-size: 1.1rem; }}
.legend {{ margin: 0; color: var(--muted); font-size: 12px; }}
.legend .key {{ display: inline-block; width: 14px; height: 3px; border-radius: 2px; vertical-align: middle; margin: 0 .3rem 0 .7rem; }}
.legend .key:first-child {{ margin-left: 0; }}
.key.daily {{ background: var(--accent); opacity: .4; }}
.key.average {{ background: var(--accent); }}
.chart {{ width: 100%; height: auto; color: var(--accent); display: block; }}
.chart text {{ fill: var(--muted); font-size: 10px; }}
.chart .chart-grid {{ stroke: var(--line); stroke-width: 1; }}
.chart .target {{ stroke: var(--muted); stroke-dasharray: 4 4; stroke-opacity: .35; }}
.chart .area {{ fill: url(#chart-fill); }}
.chart .daily {{ fill: none; stroke: var(--accent); stroke-opacity: .35; stroke-width: 1.5; stroke-linejoin: round; }}
.chart .average {{ fill: none; stroke: var(--accent); stroke-width: 2.5; stroke-linejoin: round; stroke-linecap: round; }}
.chart .hover {{ fill: transparent; }}
.chart .hover:hover {{ fill: var(--ink); fill-opacity: .05; }}
.section-head {{ display: flex; align-items: baseline; justify-content: space-between; gap: 1rem; margin: 2rem 0 .8rem; }}
.section-head h2 {{ font-size: 1.25rem; }}
.section-head p {{ margin: 0; color: var(--muted); font-size: 13px; }}
.habit {{ background: var(--card); border: 1px solid var(--line); border-radius: 16px; padding: 1.3rem 1.5rem; margin-bottom: 1rem; box-shadow: 0 10px 30px var(--shadow); }}
.habit-head {{ display: flex; flex-wrap: wrap; justify-content: space-between; gap: .5rem 1rem; align-items: flex-start; }}
h2 {{ margin: 0; font-size: 1.15rem; letter-spacing: -.01em; }}
.habit-head p {{ margin: .15rem 0 0; color: var(--muted); font-size: 13px; }}
.score {{ text-align: right; flex: none; }}
.score strong {{ display: block; font-size: 1.35rem; line-height: 1.1; color: var(--done); }}
.score span {{ color: var(--muted); font-size: 11px; text-transform: uppercase; letter-spacing: .07em; }}
.description {{ margin: .7rem 0 0; color: var(--muted); font-size: 13.5px; max-width: 68ch; }}
.tag {{ margin-left: .35rem; padding: .05rem .45rem; border: 1px solid var(--line); border-radius: 999px; font-size: 11px; white-space: nowrap; }}
.badge {{ margin-left: .45rem; padding: .05rem .45rem; border-radius: 999px; background: var(--blank); color: var(--muted); font-size: 11px; font-weight: 400; vertical-align: middle; white-space: nowrap; }}
.stats {{ display: flex; flex-wrap: wrap; gap: .3rem 1.2rem; margin: .9rem 0 1rem; color: var(--muted); font-size: 13px; }}
.weekdays, .grid {{ display: grid; grid-template-columns: repeat(7, 1fr); gap: 5px; max-width: 640px; }}
.weekdays span {{ color: var(--muted); font-size: 10px; text-align: center; text-transform: uppercase; letter-spacing: .05em; }}
.day {{ position: relative; display: flex; align-items: center; justify-content: center; padding: .5rem 0; border-radius: 6px; background: var(--blank); font-size: 12px; }}
.day.blank {{ background: transparent; }}
.day.done {{ background: var(--done); color: var(--bg); }}
.day.cheat {{ background: var(--cheat); color: var(--bg); }}
.day.missed {{ background: var(--missed); color: var(--bg); }}
.day.pending {{ background: var(--pending); color: var(--bg); }}
.day.muted {{ color: var(--muted); }}
.day.has-note::after {{ content: ""; position: absolute; right: 5px; bottom: 4px; width: 5px; height: 5px; border-radius: 50%; background: var(--ink); opacity: .55; }}
.day.done.has-note::after, .day.cheat.has-note::after, .day.missed.has-note::after, .day.pending.has-note::after {{ background: var(--bg); opacity: .9; }}
.notes {{ margin-top: 1rem; border-top: 1px solid var(--line); padding-top: .8rem; }}
.notes h3 {{ margin: 0 0 .45rem; color: var(--muted); font-size: 11px; text-transform: uppercase; letter-spacing: .1em; }}
.notes ul {{ list-style: none; margin: 0; padding: 0; display: grid; gap: .45rem; }}
.notes li {{ display: grid; grid-template-columns: 7.5rem 1fr; gap: .8rem; font-size: 13.5px; }}
.notes time {{ color: var(--muted); font-variant-numeric: tabular-nums; }}
.empty {{ color: var(--muted); background: var(--card); border: 1px dashed var(--line); border-radius: 16px; padding: 2rem; text-align: center; }}
footer {{ margin-top: 2rem; color: var(--muted); font-size: 12px; text-align: center; }}
@media (max-width: 560px) {{
  .notes li {{ grid-template-columns: 1fr; gap: .1rem; }}
  .score {{ text-align: left; }}
}}
@media print {{
  body {{ background: #fff; color: #000; padding: 0; }}
  .habit, .summary div, .chart-card {{ break-inside: avoid; box-shadow: none; }}
}}
</style>
</head>
<body>
<main>
<header class="hero">
  <div>
    <p class="eyebrow">habit · monthly report</p>
    <h1>{month_name}</h1>
    <p class="meta">Generated {generated} · {habits} · {checkins}</p>
  </div>
  <div class="ring" style="--pct: {completion}">
    <div><strong>{completion}%</strong><span>of target</span></div>
  </div>
</header>
<section class="summary">
  <div><strong>{habits_count}</strong><span>{habits}</span></div>
  <div><strong>{done}</strong><span>check-ins</span></div>
  <div><strong>{completion}%</strong><span>of target</span></div>
  <div><strong>{cheats}</strong><span>{cheat_days}</span></div>
</section>
{chart_html}
<div class="section-head">
  <h2>Habits</h2>
  <p>hover a day for its note</p>
</div>
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
        chart_html = chart_html,
        habits_html = habits_html,
        palette_css = palette_css,
        palette = theme.label(),
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
        let html = render(
            &data,
            today,
            date("2026-11-01"),
            Day::Mon,
            ThemeName::Terminal,
        );
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
        let html = render(
            &data,
            today,
            date("2026-11-01"),
            Day::Mon,
            ThemeName::Terminal,
        );
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
        let html = render(
            &data,
            today,
            date("2026-11-01"),
            Day::Mon,
            ThemeName::Terminal,
        );
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
        let html = render(
            &data,
            today,
            date("2026-11-01"),
            Day::Mon,
            ThemeName::Terminal,
        );
        assert!(html.contains("&lt;Gym &amp; &quot;co&quot;&gt;"));
        assert!(html.contains("#health &amp; &lt;fitness&gt;"));
        assert!(!html.contains("<Gym"));
    }

    #[test]
    fn the_report_uses_the_app_palette_and_can_be_repainted_from_the_file() {
        let today = date("2026-11-10");
        let mut data = Data::default();
        data.add_habit("Gym", Schedule::Daily, date("2026-11-01"))
            .unwrap();

        let html = render(
            &data,
            today,
            date("2026-11-01"),
            Day::Mon,
            ThemeName::Gruvbox,
        );
        assert!(html.contains(r#"<html lang="en" data-palette="gruvbox">"#));
        assert!(html.contains("--bg: #fbf1c7"), "gruvbox light background");
        assert!(html.contains("--done: #79740e"), "gruvbox light done");
        // Every other palette is carried too, so switching is one attribute.
        assert!(html.contains(r#"[data-palette="catppuccin"] {"#));
        assert!(html.contains("--done: #a6e3a1"), "catppuccin dark done");

        let plain = render(
            &data,
            today,
            date("2026-11-01"),
            Day::Mon,
            ThemeName::Terminal,
        );
        assert!(
            plain.contains("--bg: #f4f1ea"),
            "terminal keeps the warm neutrals"
        );
        assert!(plain.contains(r#"data-palette="terminal""#));
    }

    #[test]
    fn an_empty_month_still_produces_a_page() {
        let html = render(
            &Data::default(),
            date("2026-11-10"),
            date("2026-11-01"),
            Day::Mon,
            ThemeName::Terminal,
        );
        assert!(html.contains("No habits were running in this month."));
    }

    #[test]
    fn the_report_charts_daily_completion_and_a_rolling_average() {
        let today = date("2026-11-10");
        let mut data = Data::default();
        data.add_habit("Gym", Schedule::Daily, date("2026-11-01"))
            .unwrap();
        for day in 1..=5 {
            data.log
                .set(date(&format!("2026-11-{day:02}")), "gym", true);
        }
        let html = render(
            &data,
            today,
            date("2026-11-01"),
            Day::Mon,
            ThemeName::Terminal,
        );
        assert!(html.contains("Month at a glance"));
        assert!(html.contains("class=\"chart\""));
        assert!(html.contains("class=\"daily\""));
        assert!(html.contains("class=\"average\""));
        assert!(html.contains("7-day average"));
        // The day rectangles are the no-JS hover targets.
        assert!(html.contains("Sun 1 Nov · 100% (1/1)"), "{html}");
        assert!(html.contains("Sat 7 Nov · 0% (0/1)"), "{html}");
    }

    #[test]
    fn notes_are_listed_and_revealed_on_their_day() {
        let today = date("2026-11-10");
        let mut data = Data::default();
        let index = data
            .add_habit("Gym", Schedule::Daily, date("2026-11-01"))
            .unwrap();
        data.log.set(date("2026-11-03"), "gym", true);
        data.habits[index].set_note(date("2026-11-03"), "felt strong & fast");
        data.habits[index].note = Some("ten minutes\nno pressure".into());

        let html = render(
            &data,
            today,
            date("2026-11-01"),
            Day::Mon,
            ThemeName::Terminal,
        );
        assert!(
            html.contains("class=\"day done has-note\""),
            "the day cell should mark the note"
        );
        assert!(
            html.contains("Tue 3 Nov · done · felt strong &amp; fast"),
            "{html}"
        );
        assert!(html.contains("<p class=\"description\">ten minutes<br>no pressure</p>"));
        assert!(html.contains("<div class=\"notes\"><h3>Notes</h3>"));
        assert!(html.contains("<time>Tue 3 Nov</time><span>felt strong &amp; fast</span>"));
    }

    #[test]
    fn days_without_a_note_have_no_marker() {
        let today = date("2026-11-10");
        let mut data = Data::default();
        data.add_habit("Gym", Schedule::Daily, date("2026-11-01"))
            .unwrap();
        data.log.set(date("2026-11-03"), "gym", true);
        let html = render(
            &data,
            today,
            date("2026-11-01"),
            Day::Mon,
            ThemeName::Terminal,
        );
        assert!(!html.contains("class=\"day done has-note\""));
        assert!(!html.contains("<div class=\"notes\">"));
    }

    #[test]
    fn archived_habits_stay_in_the_month_they_ran() {
        let today = date("2026-11-10");
        let mut data = Data::default();
        data.add_habit("Gym", Schedule::Daily, date("2026-11-01"))
            .unwrap();
        data.habits[0].archived = true;
        data.log.set(date("2026-11-02"), "gym", true);
        let html = render(
            &data,
            today,
            date("2026-11-01"),
            Day::Mon,
            ThemeName::Terminal,
        );
        assert!(html.contains("archived"));
        assert!(html.contains("Gym"));
    }
}
