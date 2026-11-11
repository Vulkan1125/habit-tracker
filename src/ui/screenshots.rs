//! Renders the README screenshots from the real UI.
//!
//! This lives behind `#[cfg(test)]` and is `#[ignore]`d so a normal `cargo test`
//! never writes files. Run it with:
//!
//! ```sh
//! cargo test --quiet write_readme_screenshots -- --ignored
//! ```
//!
//! It writes HTML frames of the `TestBackend` buffer plus the monthly report to
//! `docs/screenshots/`; `scripts/screenshots.sh` turns those into PNGs with a
//! headless browser.

use std::fmt::Write as _;
use std::path::PathBuf;

use chrono::{Datelike, Duration, NaiveDate};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::style::{Color, Modifier};
use unicode_width::UnicodeWidthStr;

use super::draw;
use crate::app::{App, Mode, Tab};
use crate::ui::form::Form;
use habit::config::Config;
use habit::model::{Data, Day, Schedule};
use habit::store::Store;
use habit::theme::{ColorMode, ThemeName};

fn date(text: &str) -> NaiveDate {
    NaiveDate::parse_from_str(text, "%Y-%m-%d").unwrap()
}

fn demo_data(today: NaiveDate) -> Data {
    let mut data = Data::default();
    let tags = |names: &[&str]| {
        names
            .iter()
            .map(|name| (*name).to_string())
            .collect::<Vec<_>>()
    };

    let run = data
        .add_habit_full(
            "Morning run",
            Schedule::Daily,
            date("2026-01-01"),
            None,
            &tags(&["health", "morning"]),
            Some("5k before breakfast"),
        )
        .unwrap();
    data.habits[run].reminder = chrono::NaiveTime::from_hms_opt(6, 30, 0);

    let read = data
        .add_habit_full(
            "Read 20 pages",
            Schedule::Weekdays {
                days: [Day::Mon, Day::Tue, Day::Wed, Day::Thu, Day::Fri]
                    .into_iter()
                    .collect(),
            },
            date("2026-01-01"),
            Some(date("2026-12-31")),
            &tags(&["mind"]),
            None,
        )
        .unwrap();
    data.habits[read].reminder = chrono::NaiveTime::from_hms_opt(21, 0, 0);

    let strength = data
        .add_habit_full(
            "Strength",
            Schedule::TimesPerWeek { n: 3 },
            date("2026-01-01"),
            None,
            &tags(&["health"]),
            Some("two sets is enough on busy days"),
        )
        .unwrap();
    data.habits[strength].reminder = chrono::NaiveTime::from_hms_opt(18, 0, 0);

    let meditate = data
        .add_habit_full(
            "Meditate",
            Schedule::Daily,
            date("2026-01-01"),
            None,
            &tags(&["mind", "morning"]),
            None,
        )
        .unwrap();
    let walk = data
        .add_habit_full(
            "Evening walk",
            Schedule::Daily,
            date("2026-10-01"),
            None,
            &tags(&["evening"]),
            None,
        )
        .unwrap();
    data.habits[walk].reminder = chrono::NaiveTime::from_hms_opt(19, 30, 0);

    let yoga = data
        .add_habit_full(
            "Yoga",
            Schedule::Daily,
            date("2026-06-01"),
            Some(date("2026-10-31")),
            &tags(&["health"]),
            None,
        )
        .unwrap();
    let journaling = data
        .add_habit_full(
            "Journaling",
            Schedule::Daily,
            date("2026-02-01"),
            None,
            &tags(&["mind"]),
            None,
        )
        .unwrap();
    data.habits[journaling].archived = true;
    data.add_habit_full(
        "Winter run",
        Schedule::Weekdays {
            days: [Day::Tue, Day::Thu].into_iter().collect(),
        },
        date("2026-12-01"),
        None,
        &tags(&["health"]),
        None,
    )
    .unwrap();

    // A believable history: mostly done, with a couple of gaps and one cheat.
    for (salt, index) in [
        (3, run),
        (7, read),
        (11, strength),
        (5, meditate),
        (13, walk),
    ] {
        fill(&mut data, index, today, salt);
    }
    fill_until(&mut data, yoga, date("2026-10-31"), 17);
    fill_until(&mut data, journaling, date("2026-10-15"), 19);
    for day in ["2026-11-07", "2026-11-08", "2026-11-09", "2026-11-10"] {
        data.log.set(date(day), &data.habits[run].id.clone(), true);
    }
    data.log
        .set(date("2026-11-04"), &data.habits[run].id.clone(), false);
    data.cheats
        .set(date("2026-11-05"), &data.habits[run].id.clone(), true);
    data.cheats
        .set(date("2026-11-03"), &data.habits[strength].id.clone(), true);

    data.habits[run].set_note(date("2026-11-10"), "easy recovery run after yesterday");
    data.habits[run].set_note(date("2026-11-09"), "legs heavy but went anyway");
    data.habits[run].set_note(date("2026-11-08"), "felt strong, 5k in 26:30");
    data.habits[run].set_note(date("2026-10-30"), "cold morning, kept it short");
    data.habits[run].set_note(date("2026-10-27"), "felt strong, 5k in 26:10");
    data.habits[strength].set_note(date("2026-10-28"), "two sets is enough on a busy day");
    data.habits[read].set_note(date("2026-10-29"), "finished a chapter on the train");
    data
}

fn fill(data: &mut Data, index: usize, today: NaiveDate, salt: u32) {
    fill_until(data, index, today, salt);
}

fn fill_until(data: &mut Data, index: usize, last: NaiveDate, salt: u32) {
    let id = data.habits[index].id.clone();
    let mut day = data.habits[index].created;
    while day <= last {
        let seed = day.num_days_from_ce().unsigned_abs().wrapping_mul(7) + salt;
        if seed % 10 >= 4 {
            data.log.set(day, &id, true);
        }
        day += Duration::days(1);
    }
}

fn demo_app(today: NaiveDate, theme: ThemeName, mode: ColorMode) -> App {
    // A home-directory path keeps the modules screen reading `~/…` instead of a
    // temporary directory. The demo data is never saved.
    let home = std::env::var_os("HOME").map_or_else(|| PathBuf::from("/tmp"), PathBuf::from);
    let store = Store::resolve(Some(home.join(".local/share/habit-demo/data.json"))).unwrap();
    let mut config = Config::default();
    config.theme = theme;
    config.mode = mode;
    let mut app = App::new(store, demo_data(today), today, &config);
    // The screenshots show the full feature set, as it looks once the modules
    // are installed.
    app.modules.notify = true;
    app
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

fn buffer(app: &mut App, width: u16, height: u16) -> Buffer {
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal.draw(|frame| draw(frame, app)).unwrap();
    terminal.backend().buffer().clone()
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn css(color: Color) -> String {
    match color {
        Color::Rgb(r, g, b) => format!("#{r:02x}{g:02x}{b:02x}"),
        Color::Indexed(index) => format!("#{index:02x}{index:02x}{index:02x}"),
        Color::Black => "#1c1c1c".into(),
        Color::Red => "#c14a4a".into(),
        Color::Green => "#4a8a4a".into(),
        Color::Yellow => "#b08a3a".into(),
        Color::Blue => "#4a6aa8".into(),
        Color::Magenta => "#9a5aa8".into(),
        Color::Cyan => "#4a8a8a".into(),
        Color::Gray => "#a8a8a8".into(),
        Color::DarkGray => "#6a6a6a".into(),
        Color::LightRed => "#e08a8a".into(),
        Color::LightGreen => "#8ad08a".into(),
        Color::LightYellow => "#e0c08a".into(),
        Color::LightBlue => "#8aa8e0".into(),
        Color::LightMagenta => "#c08ad0".into(),
        Color::LightCyan => "#8ad0d0".into(),
        Color::White => "#f0f0f0".into(),
        Color::Reset => "#d8d8d8".into(),
    }
}

fn style_attr(fg: Color, bg: Color, base_bg: Color, modifier: Modifier) -> String {
    let (mut fg_color, mut bg_color) = (fg, bg);
    if modifier.contains(Modifier::REVERSED) {
        std::mem::swap(&mut fg_color, &mut bg_color);
    }
    let mut style = String::new();
    if fg_color != Color::Reset {
        let _ = write!(style, "color:{};", css(fg_color));
    }
    if bg_color != base_bg && bg_color != Color::Reset {
        let _ = write!(style, "background:{};", css(bg_color));
    }
    if modifier.contains(Modifier::BOLD) {
        style.push_str("font-weight:700;");
    }
    if modifier.contains(Modifier::DIM) {
        style.push_str("opacity:.62;");
    }
    if modifier.contains(Modifier::ITALIC) {
        style.push_str("font-style:italic;");
    }
    if modifier.contains(Modifier::UNDERLINED) {
        style.push_str("text-decoration:underline;");
    }
    style
}

/// One text run in the HTML: the same style for a stretch of cells.
struct Run {
    style: String,
    text: String,
}

fn screen_html(buffer: &Buffer, title: &str, base_bg: Color, base_fg: Color) -> String {
    let mut runs: Vec<Run> = Vec::new();
    for y in 0..buffer.area.height {
        let mut x = 0;
        while x < buffer.area.width {
            let cell = &buffer[(x, y)];
            let symbol = cell.symbol();
            let width = symbol.width().max(1);
            let style = style_attr(cell.fg, cell.bg, base_bg, cell.modifier);
            match runs.last_mut() {
                Some(run) if run.style == style => run.text.push_str(symbol),
                _ => runs.push(Run {
                    text: symbol.to_string(),
                    style,
                }),
            }
            x += width as u16;
        }
        // The newline belongs to whatever run ends the row.
        if let Some(run) = runs.last_mut() {
            run.text.push('\n');
        }
    }

    let mut spans = String::new();
    for run in &runs {
        if run.style.is_empty() {
            spans.push_str(&escape(&run.text));
        } else {
            let _ = write!(
                spans,
                "<span style=\"{}\">{}</span>",
                run.style,
                escape(&run.text)
            );
        }
    }

    format!(
        r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<title>{title}</title>
<style>
  html, body {{ margin: 0; padding: 0; background: transparent; }}
  pre {{
    margin: 0; padding: 30px 34px; display: inline-block;
    color: {fg}; background: {bg};
    font-family: "JetBrains Mono", "SF Mono", Menlo, monospace;
    font-size: 15px; line-height: 18px; font-weight: 400;
    font-variant-ligatures: none; letter-spacing: 0; white-space: pre;
    -webkit-font-smoothing: antialiased;
  }}
</style>
</head>
<body><pre>{spans}</pre></body>
</html>
"#,
        title = escape(title),
        fg = css(base_fg),
        bg = css(base_bg),
    )
}

fn write(dir: &std::path::Path, name: &str, html: &str) {
    std::fs::write(dir.join(name), html).unwrap();
}

#[test]
#[ignore = "writes docs/screenshots; run explicitly with --ignored"]
fn write_readme_screenshots() {
    let today = date("2026-11-10");
    let dir = std::path::Path::new("docs/screenshots");
    std::fs::create_dir_all(dir).unwrap();

    // The test binary has no module binaries beside it, so point the detector at
    // a fake release directory: the overlay can then say "not installed" rather
    // than "unavailable" for the module that is not installed.
    let module_source = std::env::temp_dir().join("habit-screenshot-modules");
    std::fs::create_dir_all(&module_source).unwrap();
    for name in ["notify", "report"] {
        std::fs::write(module_source.join(format!("habit-{name}")), b"").unwrap();
    }
    // SAFETY: this test is ignored and runs on its own; nothing else reads or
    // writes the environment concurrently.
    unsafe { std::env::set_var("HABIT_MODULE_SOURCE", &module_source) };

    let mut dark = demo_app(today, ThemeName::Catppuccin, ColorMode::Dark);
    let theme = dark.theme;
    let bg = theme.bg.unwrap_or(Color::Rgb(0, 0, 0));
    let fg = theme.fg;

    let frame = buffer(&mut dark, 108, 32);
    write(
        dir,
        "main-dark.html",
        &screen_html(&frame, "habit · main screen", bg, fg),
    );

    let mut light = demo_app(today, ThemeName::Catppuccin, ColorMode::Light);
    let light_theme = light.theme;
    let frame = buffer(&mut light, 108, 32);
    write(
        dir,
        "main-light.html",
        &screen_html(
            &frame,
            "habit · light mode",
            light_theme.bg.unwrap_or(Color::Rgb(255, 255, 255)),
            light_theme.fg,
        ),
    );

    let mut tags = demo_app(today, ThemeName::Catppuccin, ColorMode::Dark);
    tags.select_tab(Tab::Tags);
    let frame = buffer(&mut tags, 108, 32);
    write(
        dir,
        "tags.html",
        &screen_html(&frame, "habit · tags", bg, fg),
    );

    let mut archive = demo_app(today, ThemeName::Catppuccin, ColorMode::Dark);
    archive.select_tab(Tab::Archive);
    archive.cursor = 1;
    let frame = buffer(&mut archive, 108, 32);
    write(
        dir,
        "archive.html",
        &screen_html(&frame, "habit · archive", bg, fg),
    );

    let mut form = demo_app(today, ThemeName::Catppuccin, ColorMode::Dark);
    form.mode = Mode::Form(Box::new(Form::new(today)));
    type_text(&mut form, "Journal");
    for _ in 0..3 {
        press(&mut form, KeyCode::Tab);
    }
    for _ in 0..14 {
        press(&mut form, KeyCode::Char(' '));
    }
    let frame = buffer(&mut form, 108, 32);
    write(
        dir,
        "form.html",
        &screen_html(&frame, "habit · add dialog", bg, fg),
    );

    let mut help = demo_app(today, ThemeName::Catppuccin, ColorMode::Dark);
    help.mode = Mode::Help;
    let frame = buffer(&mut help, 108, 32);
    write(
        dir,
        "help.html",
        &screen_html(&frame, "habit · help", bg, fg),
    );

    let mut reminders = demo_app(today, ThemeName::Catppuccin, ColorMode::Dark);
    reminders.mode = Mode::ReminderSetup;
    let frame = buffer(&mut reminders, 108, 32);
    write(
        dir,
        "reminders.html",
        &screen_html(&frame, "habit · reminder setup", bg, fg),
    );

    let mut modules = demo_app(today, ThemeName::Catppuccin, ColorMode::Dark);
    modules.mode = Mode::Modules;
    let frame = buffer(&mut modules, 108, 32);
    write(
        dir,
        "modules.html",
        &screen_html(&frame, "habit · modules", bg, fg),
    );

    let data = demo_data(today);
    let html = habit::report::render(&data, today, habit::report::last_month(today), Day::Mon);
    write(dir, "report.html", &html);
}
