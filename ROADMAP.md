# Roadmap

Design notes for `habit`. This started as the plan for 0.2.0; all six features below shipped in
that release. The sections are kept as a record of the decisions, with a **Shipped** note where the
implementation differs from the sketch. Open work moved to [Carried over](#carried-over-from-01x).

Every feature keeps the two rules that make the project what it is: **one plain JSON file** and
**no daemon, account or network**. The Archive tab, the **Starting later** heading and native
reminders shipped in 0.6.0. 0.7.0 made reminders per-habit with one OS job per time and added
monthly HTML reports; the reminders still hand each run to launchd/systemd rather than living
inside the app. 0.8.0 moved those two larger features into installable companion binaries, so the
core stays small and the features are opt-in.

## How the pieces fit together

Six requested features, plus the items carried over from 0.1.0.

| # | Feature | Size | Status |
| --- | --- | --- | --- |
| 1 | Start and end dates | M | shipped |
| 2 | Cheat days (one per month) | L | shipped |
| 3 | Notes | S | shipped (per-habit in 0.2.0; per-day in 0.3.0) |
| 4 | Tags and tag filtering | M | shipped (rename/merge in 0.5.0) |
| 5 | Graph in the detail pane | M | shipped in 0.2.0; sparkline removed in 0.3.0 |
| 6 | A genuinely beautiful UI | L | shipped (seven palettes, square borders) |

The sequence was **3 → 4 → 1 → 2 → 5 → 6**. Notes and tags were isolated and made the list easier
to live with immediately; dates and cheat days touched the streak engine, which is the part that
must stay correct; the graph and the visual pass came last so they only had to be designed once.

**Shipped:** the TUI now has a selected day (`[` `]`, `t`), so date-scoped features and backfilling
work in the interface as well as the CLI. This was the prerequisite for cheat days and notes.

---

## 1. Start and end dates for habits

**Shipped in 0.2.0.** `Habit` has `ends`, `created` stays the start, and the window is enforced by
`is_active_on` / `is_finished` in the model and by `scope_end` in `stats.rs`. Both dates are in the
TUI dialog. The three open questions below were resolved as written.

Today a habit starts on `created` and never ends; you archive it as a manual, reversible flag.

**What changes**

- `Habit.created` stays as the start date (no migration, no rename).
- New optional `ends: Option<NaiveDate>` on `Habit`.
- A habit is *active* on a date when `created <= date` and (`ends` is `None` or `date <= ends`).
- The active window replaces the ad hoc "before creation" rule already used by the heatmap and
  dot strip, and applies to streak counting too.

**Surfaces**

- CLI: `habit add "X" --start 2026-11-01 --end 2026-12-31`, `habit end <habit> [--date D]`,
  `habit end <habit> --clear`, and both dates in `habit list`/`habit stats` output.
- TUI: start/end fields in the add/edit dialog (validated `end >= start`), and a `finished`
  section below archived habits rendered dimmed.
- Editing the end date in either direction recomputes streaks, because stats are pure functions of
  `(habit, log, today)`.

**Decisions**

- A finished habit freezes its stats on its final day; the detail pane labels it `finished` instead
  of showing a decaying streak.
- `ends` and `archived` stay separate: a scheduled finish is not a manual hide. Both sort together
  at the bottom of the list.
- Editing `created` after check-ins exist is allowed; entries outside the window are kept in the
  log and ignored by stats. The CLI warns if `ends` would end up before the new start.

---

## 2. Cheat days — one per month

**Shipped in 0.2.0.** The streak engine takes `History { log, cheats }`; `Cheats::is_excused`
derives the monthly allowance at query time, so backfilling or removing an earlier cheat immediately
recomputes which day is free.

Mark a day as excused: it does not break the streak, and it renders in its own colour. A **second**
cheat day in the same calendar month is not excused and breaks the streak.

**Data**

A new top-level map, parallel to `log`, so "done" and "excused" never get confused:

```json
"cheats": { "2026-11-05": ["gym"], "2026-11-19": ["read"] }
```

Keyed by date and habit id, matching `log`'s shape, sorted, hand-editable.

**Semantics to nail down**

- A cheat day is *skipped* by the streak, exactly like a non-due day, not counted as a completion.
- It does **not** count toward a weekly quota; it excuses a miss. (Otherwise `4×/week` becomes
  "cheat four times a week".)
- Monthly allowance is per habit and per calendar month. The first cheat in a month is free; from
  the second onward the day counts as a normal miss.
- The allowance is checked when the streak is computed, not when the day is marked. Marking a
  second cheat is allowed — the UI flags it (`2nd cheat in November · breaks streak`) so the
  consequence is visible before it happens.
- Archive/delete must carry cheat days along with log entries.

**Surfaces**

- CLI: `habit cheat <habit> [--date D]`, `habit cheat <habit> --off`, and a `CHEAT` marker in
  `habit log`.
- TUI: `c` toggles a cheat day on the selected date; the heatmap uses each palette's cheat colour
  with its own legend entry; the detail pane shows e.g. `◆ 2 cheat this Nov`.
- Help overlay and README streak rules both need the new row.

**Why this was the biggest item:** the streak engine used to take `(habit, log, today)`. It now
takes `(habit, history, today)` where `history` bundles the log and cheat map, and it gained the
month-bucketing rule. Every existing streak test has a cheat-day sibling. The complexity stayed in
`stats.rs` as pure functions.

---

## 3. Notes

**Phase 1 shipped in 0.2.0:** the per-habit note is editable in the dialog and via `habit note`,
wrapped in the detail pane, and included in search. It is now called the habit's **description**
(`habit describe`, `--description`), so it is not confused with dated notes.

**Phase 2 shipped in 0.3.0:** dated notes, one per habit per day, written with `n` in the TUI and
listed newest first in the detail pane. The original `note` field stayed put, so the upgrade is
additive.

The `note` field and its rendering already existed in 0.1.0; there was no way to write one from
the app.

**Phase 1 — per-habit note (small)**

- Editable in the add/edit dialog, multi-line, wrapped in the detail pane.
- CLI: `habit note <habit> "text"`, `habit note <habit> --clear`, and `habit note <habit>` to print.
- Included in search matching.

**Phase 2 — per-day note**

Shipped as a `"notes": { "2026-11-10": "bad sleep, still showed up" }` map, shown in the detail
pane with the date on the left. The cursor day picks the note, so it doubles as a lightweight
journal without turning the app into a journal app.

**Open questions**

- Length limit? Prefer no hard limit, but wrap at the pane width and cap the pane height with a
  scroll indicator.
- Do notes belong in the JSON file or a sidecar? Same file — one file is the selling point.

---

## 4. Tags and tag filtering

**Shipped in 0.2.0.** Tags are normalised, sorted, rendered as chips and accepted as `#tag` in the
filter. Filtering was AND-only until 0.9.0 added `#a|#b` (either tag) to both the TUI filter and
`habit list --tag`. Rename/merge shipped in 0.5.0.

**Data**

`tags: Vec<String>` on `Habit`, normalised to lowercase kebab-case, de-duplicated, sorted. Omitted
from JSON when empty, like `note` and `archived`.

**Surfaces**

- CLI (shipped): `habit add "X" --tag health --tag morning` (repeatable),
  `habit tag <habit> health morning`, `habit tag <habit> cardio --remove`, `habit tags`
  (all tags with counts), and `habit list --tag health`.
- TUI (shipped): tag chips in the detail pane and `#health` typed directly into `/` to filter.
  Search matches name, id, tags and notes.
- Detail pane: tags are listed under the schedule line.

**Open**

- ~~OR filtering (`#a|#b`) if the parser can stay simple.~~ **Shipped in 0.9.0:** a `|` inside one
  token offers alternatives, while separate tokens and separate `--tag` arguments stay ANDed.

**Shipped in 0.5.0 — rename and merge**

`e` on the Tags list opens a prompt for the tag under the cursor; `habit rename-tag old new` does
the same from the shell. Renaming onto a name that already exists merges the two, asking first in
the TUI. `habit merge-tags from into` is the careful spelling: it refuses a target that is not a
tag, so a typo cannot create one. Not the sketched `habit tag --rename old new`, because that
overloads `habit tag`, whose arguments are a habit followed by tags.

---

## 5. Graph in the detail pane

**Shipped in 0.2.0** as a compact rolling 7-day completion **sparkline** above the heatmap; the
heatmap stayed, and is now adaptive up to 52 weeks. No new dependency was needed. **0.9.0** added
the quota bars as a four-week text chart in the detail pane, for `TimesPerWeek` habits only. The
richer chart (axes, target line, switchable ranges) is still open.

The heatmap answers "which days did I do it". It does not answer "am I trending up".

**Still open**

- A `Chart` with a 7-day moving average and a faint target line, for wide terminals.
- Time ranges switchable from the pane (30d / 90d / 1y).
- For quota habits, sessions-per-week bars instead of a daily line, since a daily line is noise
  when only the weekly total matters.

**Constraints**

- No new dependencies: ratatui already ships `Chart`, `Sparkline`, `BarChart` and `LineGauge`.
- Keep stats pure: the pane asks `stats::` for a series, it does not compute.
- Must stay legible in a 60-column terminal and in the `mono` theme.

---

## 6. UI enhancements

**Shipped in 0.2.0.** A focused visual pass instead of a pile of new features:

- a responsive layout that works from narrow terminals up to wide ones;
- a clear hierarchy in the detail pane, with dimmed metadata and aligned numbers;
- status colours that also carry a glyph or weight, so nothing depends on colour alone;
- progress bars, a real empty state and a status line that reads like a sentence;
- seven colour themes, each with a light and a dark mode, truecolor detection and a `mono` fallback;
- frame tests at several widths and for the overlays, and regenerated README screenshots.

---

## Carried over from 0.1.x

- A quota bar chart and richer multi-tag views — **shipped in 0.9.0** as the weekly quota chart in
  the detail pane and `#a|#b` / `--tag a|b` either-or filtering.
- Optional reminders/notifications — **shipped in 0.6.0** as `habit remind` plus `habit schedule
  install`, which writes a launchd agent or systemd user timer. 0.7.0 added per-habit reminder
  times, one job per distinct time, and a first-run setup prompt. 0.8.0 made the whole feature an
  installable module (`habit modules install notify`). Still no daemon and still opt-in, exactly
  as sketched here.
- A monthly HTML report — **shipped in 0.7.0** as `habit report` plus `habit schedule reports
  install`; 0.8.0 moved it into the installable reports module. One self-contained page per month,
  on demand or on a chosen day. **0.10.0** added the month-wide completion chart with its seven-day
  average, per-habit scores, descriptions, and notes that are both listed and revealed on hover.
- Import/export helpers — **shipped in 0.9.0** as `habit export` and `habit import`, covering habits,
  metadata and their complete history. Reading an older `data.json` was already handled by the
  store's version upgrade path.
- Windows CI.

## Non-goals

- Accounts, sync, cloud storage, telemetry.
- A general plugin system or scripting language. The two modules are curated companion binaries
  shipped with the app, not a public extension API.
- A web UI or a server.
- Sub-second timers or background work of any kind.

## Compatibility policy

The version guard exists so an older binary can never silently mangle a newer file. Any change to
the on-disk shape — including "additive" fields like `ends`, `tags`, `cheats` and `notes` — bumps
`DATA_VERSION` and ships a migration in `store.rs`, because an old build would otherwise read a v1
file, drop the fields it does not know about on the next save, and lose data quietly. With the bump,
that old build refuses the file with a clear error instead, and the original is still on disk as
`data.json.bak`.

Implemented in 0.2.0 and extended in 0.3.0, 0.6.0 and 0.7.0 (format 4 added `reminder`): an older
file loads with defaults for the new fields and is stamped as the current version on the next save,
while a future version is refused. Every feature lands with tests in
`stats.rs` (or wherever the logic belongs) and, where it renders, a `TestBackend` frame test.
