//! CSV import and export: a portable copy of every habit and its history.
//!
//! The format is one row per event, with the habit's metadata repeated on every
//! row so a single row still says which habit it belongs to. Habits without any
//! history get one metadata-only row. Export always writes the whole file;
//! import is additive and never deletes or overwrites an existing habit.

use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Context, Result, bail};
use chrono::{NaiveDate, NaiveTime};

use crate::model::{Data, Habit, Schedule};

/// Column order, also written as the header row.
pub const COLUMNS: [&str; 12] = [
    "habit_id",
    "habit_name",
    "schedule",
    "start",
    "end",
    "tags",
    "description",
    "reminder",
    "archived",
    "date",
    "status",
    "note",
];

/// What an import changed.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ImportReport {
    pub habits_added: usize,
    pub habits_merged: usize,
    pub checkins: u32,
    pub cheats: u32,
    pub notes: u32,
}

impl ImportReport {
    pub fn is_empty(&self) -> bool {
        self.habits_added == 0
            && self.habits_merged == 0
            && self.checkins == 0
            && self.cheats == 0
            && self.notes == 0
    }
}

/// The complete data file as CSV, oldest events first.
pub fn export(data: &Data) -> String {
    let mut out = String::new();
    out.push_str(&COLUMNS.join(","));
    out.push('\n');

    for habit in &data.habits {
        let mut events: Vec<(NaiveDate, RowStatus, &str)> = Vec::new();
        for date in data.log.dates_for(&habit.id) {
            events.push((date, RowStatus::Done, ""));
        }
        for date in data.cheats.dates_for(&habit.id) {
            events.push((date, RowStatus::Cheat, ""));
        }
        for (date, text) in &habit.notes {
            events.push((*date, RowStatus::Note, text.as_str()));
        }
        events.sort_by_key(|(date, status, _)| (*date, status.rank()));

        if events.is_empty() {
            write_row(&mut out, habit, None);
        } else {
            for (date, status, text) in events {
                write_row(&mut out, habit, Some((date, status, text)));
            }
        }
    }
    out
}

fn write_row(out: &mut String, habit: &Habit, event: Option<(NaiveDate, RowStatus, &str)>) {
    let (date, status, note) = match event {
        Some((date, status, note)) => (date.to_string(), status.label(), note),
        None => (String::new(), "", ""),
    };
    let fields = [
        habit.id.clone(),
        habit.name.clone(),
        habit.schedule.code(),
        habit.created.to_string(),
        habit.ends.map(|ends| ends.to_string()).unwrap_or_default(),
        habit.tags.join(";"),
        habit.note.clone().unwrap_or_default(),
        habit
            .reminder
            .map(|time| time.format("%H:%M").to_string())
            .unwrap_or_default(),
        if habit.archived { "true" } else { "false" }.to_string(),
        date,
        status.to_string(),
        note.to_string(),
    ];
    for (index, field) in fields.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        out.push_str(&escape(field));
    }
    out.push('\n');
}

fn escape(value: &str) -> String {
    if value.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
}

/// Merge a CSV export into `data`, leaving existing habits and history intact.
///
/// Rows whose habit id already exists only add the missing events, so importing
/// the same file twice is a no-op. Unknown ids are created with their exported
/// id and metadata. A name clash with a *different* id is refused instead of
/// quietly creating a duplicate.
pub fn import(data: &mut Data, text: &str) -> Result<ImportReport> {
    let mut report = ImportReport::default();
    let records = records(text)?;
    if records.is_empty() {
        return Ok(report);
    }
    check_header(&records[0])?;

    let mut first_row: BTreeSet<String> = BTreeSet::new();
    let mut metadata: BTreeMap<String, Metadata> = BTreeMap::new();

    for (index, fields) in records.iter().enumerate().skip(1) {
        let row_no = index + 1;
        let row = Row::parse(fields).with_context(|| format!("row {row_no}"))?;

        match metadata.get(&row.id) {
            Some(seen) if seen != &row.metadata() => bail!(
                "row {row_no}: habit \"{}\" changed its metadata inside the file",
                row.id
            ),
            Some(_) => {}
            None => {
                metadata.insert(row.id.clone(), row.metadata());
            }
        }

        let index = if first_row.insert(row.id.clone()) {
            match data.habits.iter().position(|habit| habit.id == row.id) {
                Some(index) => {
                    report.habits_merged += 1;
                    index
                }
                None => {
                    report.habits_added += 1;
                    create_habit(data, &row, row_no)?
                }
            }
        } else {
            data.habits
                .iter()
                .position(|habit| habit.id == row.id)
                .expect("a habit row was seen before it was created")
        };

        let Some(date) = row.date else {
            continue;
        };
        let habit = &data.habits[index];
        if !habit.is_active_on(date) {
            bail!(
                "row {row_no}: {date} is outside {} ({} → {})",
                habit.id,
                habit.created,
                habit
                    .ends
                    .map(|ends| ends.to_string())
                    .unwrap_or_else(|| "no end".to_string())
            );
        }

        match row.status {
            RowStatus::None => {}
            RowStatus::Done => {
                if !data.log.is_done(date, &row.id) {
                    data.log.set(date, &row.id, true);
                    report.checkins += 1;
                }
            }
            RowStatus::Cheat => {
                if !data.cheats.is_cheat(date, &row.id) {
                    data.cheats.set(date, &row.id, true);
                    report.cheats += 1;
                }
            }
            RowStatus::Note => {
                if data.habits[index].set_note(date, &row.note) {
                    report.notes += 1;
                }
            }
        }
    }

    Ok(report)
}

fn create_habit(data: &mut Data, row: &Row, row_no: usize) -> Result<usize> {
    if row.id.trim().is_empty() {
        bail!("row {row_no}: habit_id cannot be empty");
    }
    if row.id.chars().any(char::is_whitespace) {
        bail!("row {row_no}: habit_id cannot contain whitespace");
    }
    if row.name.trim().is_empty() {
        bail!("row {row_no}: habit_name cannot be empty");
    }
    if row.name.chars().count() > 48 {
        bail!("row {row_no}: habit_name is too long (max 48 characters)");
    }
    if let Some(existing) = data
        .habits
        .iter()
        .find(|habit| habit.name.eq_ignore_ascii_case(&row.name))
    {
        bail!(
            "row {row_no}: \"{}\" already exists with id {}; rename one of them first",
            existing.name,
            existing.id
        );
    }

    let mut habit = Habit::new(
        row.id.clone(),
        row.name.clone(),
        row.schedule.clone(),
        row.created,
    );
    habit.ends = row.ends;
    habit.archived = row.archived;
    habit.note = row.description.clone();
    habit.reminder = row.reminder;
    for tag in &row.tags {
        habit.add_tag(tag);
    }
    habit.validate()?;
    data.habits.push(habit);
    Ok(data.habits.len() - 1)
}

fn check_header(header: &[String]) -> Result<()> {
    let expected = COLUMNS.to_vec();
    if header != expected {
        bail!(
            "not a habit CSV: expected the header {}, found {}",
            COLUMNS.join(","),
            header.join(",")
        );
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum RowStatus {
    None,
    Done,
    Cheat,
    Note,
}

impl RowStatus {
    fn rank(self) -> u8 {
        match self {
            RowStatus::None => 0,
            RowStatus::Done => 1,
            RowStatus::Cheat => 2,
            RowStatus::Note => 3,
        }
    }

    fn label(self) -> &'static str {
        match self {
            RowStatus::None => "",
            RowStatus::Done => "done",
            RowStatus::Cheat => "cheat",
            RowStatus::Note => "note",
        }
    }
}

/// The columns that describe a habit rather than one event.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Metadata {
    name: String,
    schedule: Schedule,
    created: NaiveDate,
    ends: Option<NaiveDate>,
    tags: Vec<String>,
    description: Option<String>,
    reminder: Option<NaiveTime>,
    archived: bool,
}

struct Row {
    id: String,
    name: String,
    schedule: Schedule,
    created: NaiveDate,
    ends: Option<NaiveDate>,
    tags: Vec<String>,
    description: Option<String>,
    reminder: Option<NaiveTime>,
    archived: bool,
    date: Option<NaiveDate>,
    status: RowStatus,
    note: String,
}

impl Row {
    fn parse(fields: &[String]) -> Result<Row> {
        if fields.len() != COLUMNS.len() {
            bail!(
                "expected {} columns, found {} (check for an unquoted comma)",
                COLUMNS.len(),
                fields.len()
            );
        }
        let field = |index: usize| fields[index].trim();

        let id = field(0).to_string();
        let name = field(1).to_string();
        let schedule = Schedule::from_code(field(2))?;
        let created = parse_date(field(3), "start")?;
        let ends = match field(4) {
            "" => None,
            raw => Some(parse_date(raw, "end")?),
        };
        let tags = field(5)
            .split(';')
            .map(str::trim)
            .filter(|tag| !tag.is_empty())
            .map(str::to_string)
            .collect();
        let description = Some(field(6).to_string()).filter(|text| !text.is_empty());
        let reminder = match field(7) {
            "" => None,
            raw => Some(parse_time(raw)?),
        };
        let archived = parse_bool(field(8))?;
        let date = match field(9) {
            "" => None,
            raw => Some(parse_date(raw, "date")?),
        };
        let status = match field(10).to_ascii_lowercase().as_str() {
            "" => RowStatus::None,
            "done" | "checkin" | "check-in" => RowStatus::Done,
            "cheat" | "excused" => RowStatus::Cheat,
            "note" | "journal" => RowStatus::Note,
            other => bail!("unknown status \"{other}\" (use done, cheat or note)"),
        };
        let note = field(11).to_string();

        if status == RowStatus::None && date.is_some() {
            bail!("a row with a date needs a status");
        }
        if status != RowStatus::None && date.is_none() {
            bail!("a \"{}\" row needs a date", status.label());
        }
        if status == RowStatus::Note && note.is_empty() {
            bail!("a note row needs its text in the note column");
        }

        Ok(Row {
            id,
            name,
            schedule,
            created,
            ends,
            tags,
            description,
            reminder,
            archived,
            date,
            status,
            note,
        })
    }

    fn metadata(&self) -> Metadata {
        Metadata {
            name: self.name.clone(),
            schedule: self.schedule.clone(),
            created: self.created,
            ends: self.ends,
            tags: self.tags.clone(),
            description: self.description.clone(),
            reminder: self.reminder,
            archived: self.archived,
        }
    }
}

fn parse_date(raw: &str, what: &str) -> Result<NaiveDate> {
    NaiveDate::parse_from_str(raw.trim(), "%Y-%m-%d")
        .with_context(|| format!("{what} must be YYYY-MM-DD, found \"{raw}\""))
}

fn parse_time(raw: &str) -> Result<NaiveTime> {
    for format in ["%H:%M", "%H:%M:%S"] {
        if let Ok(time) = NaiveTime::parse_from_str(raw.trim(), format) {
            return Ok(time);
        }
    }
    bail!("reminder must be a 24-hour time like 09:00, found \"{raw}\"")
}

fn parse_bool(raw: &str) -> Result<bool> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "" | "false" | "0" | "no" | "n" => Ok(false),
        "true" | "1" | "yes" | "y" => Ok(true),
        other => bail!("archived must be true or false, found \"{other}\""),
    }
}

/// A small RFC 4180-style reader: quoted fields, doubled quotes and embedded
/// newlines. Blank lines are skipped.
fn records(text: &str) -> Result<Vec<Vec<String>>> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut out = Vec::new();
    let mut record: Vec<String> = Vec::new();
    let mut field = String::new();
    let mut quoted = false;
    let mut chars = text.chars().peekable();

    while let Some(character) = chars.next() {
        if quoted {
            match character {
                '"' if chars.peek() == Some(&'"') => {
                    chars.next();
                    field.push('"');
                }
                '"' => quoted = false,
                _ => field.push(character),
            }
            continue;
        }
        match character {
            '"' if field.is_empty() => quoted = true,
            ',' => record.push(std::mem::take(&mut field)),
            '\r' | '\n' => {
                if character == '\r' && chars.peek() == Some(&'\n') {
                    chars.next();
                }
                record.push(std::mem::take(&mut field));
                if !(record.len() == 1 && record[0].is_empty()) {
                    out.push(std::mem::take(&mut record));
                } else {
                    record.clear();
                }
            }
            _ => field.push(character),
        }
    }

    if quoted {
        bail!("unterminated quoted field");
    }
    if !field.is_empty() || !record.is_empty() {
        record.push(field);
        out.push(record);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn date(text: &str) -> NaiveDate {
        NaiveDate::parse_from_str(text, "%Y-%m-%d").unwrap()
    }

    fn sample() -> Data {
        let mut data = Data::default();
        let gym = data
            .add_habit_full(
                "Gym, mostly",
                Schedule::Weekdays {
                    days: BTreeSet::from([crate::model::Day::Mon, crate::model::Day::Wed]),
                },
                date("2026-10-01"),
                Some(date("2026-12-31")),
                &["health".into(), "morning".into()],
                Some("10 minutes is enough\nno pressure"),
            )
            .unwrap();
        data.habits[gym].reminder = NaiveTime::parse_from_str("07:05", "%H:%M").ok();
        data.log.set(date("2026-11-02"), &data.habits[gym].id, true);
        data.log.set(date("2026-11-04"), &data.habits[gym].id, true);
        data.cheats
            .set(date("2026-11-09"), &data.habits[gym].id, true);
        data.habits[gym].set_note(date("2026-11-04"), "felt \"strong\", honestly");

        let read = data
            .add_habit_full(
                "Read",
                Schedule::TimesPerWeek { n: 3 },
                date("2026-10-15"),
                None,
                &[],
                None,
            )
            .unwrap();
        data.habits[read].archived = true;
        data.log
            .set(date("2026-11-10"), &data.habits[read].id, true);
        data
    }

    #[test]
    fn an_export_round_trips_back_into_empty_data() {
        let original = sample();
        let text = export(&original);
        assert!(text.starts_with("habit_id,habit_name,schedule,start,end,tags"));

        let mut restored = Data::default();
        let report = import(&mut restored, &text).unwrap();
        assert_eq!(report.habits_added, 2);
        assert_eq!(report.habits_merged, 0);
        assert_eq!(report.checkins, 3);
        assert_eq!(report.cheats, 1);
        assert_eq!(report.notes, 1);
        assert_eq!(restored, original);
    }

    #[test]
    fn importing_twice_changes_nothing_the_second_time() {
        let original = sample();
        let text = export(&original);
        let mut data = Data::default();
        import(&mut data, &text).unwrap();

        let report = import(&mut data, &text).unwrap();
        assert_eq!(report.habits_added, 0);
        assert_eq!(report.habits_merged, 2);
        assert_eq!((report.checkins, report.cheats, report.notes), (0, 0, 0));
        assert_eq!(data, original);
    }

    #[test]
    fn import_keeps_new_events_and_leaves_existing_metadata_alone() {
        let original = sample();
        let mut text = export(&original);
        // A later row for the same habit, with a check-in the local file lacks.
        let mut row = String::new();
        write_row(
            &mut row,
            &original.habits[0],
            Some((date("2026-11-05"), RowStatus::Done, "")),
        );
        text.push_str(&row);

        let mut data = original.clone();
        data.habits[0].name = "Renamed locally".into();
        let id = data.habits[0].id.clone();
        let report = import(&mut data, &text).unwrap();
        assert_eq!(report.checkins, 1, "only the new check-in is added");
        assert!(data.log.is_done(date("2026-11-05"), &id));
        assert_eq!(data.habits[0].name, "Renamed locally");
    }

    #[test]
    fn a_name_clash_with_another_id_is_refused() {
        let mut data = Data::default();
        data.add_habit("Gym", Schedule::Daily, date("2026-10-01"))
            .unwrap();
        let text = format!(
            "{}\nother,Gym,daily,2026-10-01,,,,,false,,,\n",
            COLUMNS.join(",")
        );
        let error = import(&mut data, &text).unwrap_err().to_string();
        assert!(error.contains("already exists"), "{error}");
    }

    #[test]
    fn quoted_fields_survive_commas_quotes_and_newlines() {
        let text = "a,b\n\"x, y\",\"he said \"\"hi\"\"\nagain\"\n";
        let parsed = records(text).unwrap();
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[1], vec!["x, y", "he said \"hi\"\nagain"]);

        let escaped = escape("a,b\"c\nd");
        assert_eq!(escaped, "\"a,b\"\"c\nd\"");
    }

    #[test]
    fn malformed_files_are_rejected_with_a_row_number() {
        let mut data = Data::default();
        let bad_header = import(&mut data, "name,date\nGym,2026-11-01\n").unwrap_err();
        assert!(bad_header.to_string().contains("header"), "{bad_header}");

        let text = format!(
            "{}\ngym,Gym,sometimes,2026-10-01,,,,,false,,,\n",
            COLUMNS.join(",")
        );
        let error = import(&mut data, &text).unwrap_err().to_string();
        assert!(error.contains("row 2"), "{error}");
    }

    #[test]
    fn an_event_outside_the_habit_window_is_refused() {
        let mut data = Data::default();
        let text = format!(
            "{}\ngym,Gym,daily,2026-10-01,,,,,false,2026-09-01,done,\n",
            COLUMNS.join(",")
        );
        let error = import(&mut data, &text).unwrap_err().to_string();
        assert!(error.contains("outside"), "{error}");
    }

    #[test]
    fn a_header_only_file_is_a_no_op() {
        let mut data = Data::default();
        let report = import(&mut data, &format!("{}\n", COLUMNS.join(","))).unwrap();
        assert!(report.is_empty());
        assert_eq!(data, Data::default());
    }
}
