# Roadmap

Design notes for `habit`. This started as the plan for 0.2.0; all six features below shipped in
that release. The sections are kept as a record of the decisions, with a **Shipped** note where the
implementation differs from the sketch. Open work moved to [Carried over](#carried-over-from-01x).

Every feature keeps the two rules that make the project what it is: **one plain JSON file** and
**no daemon, account or network**.

## How the pieces fit together

Six requested features, plus the items carried over from 0.1.0.

| # | Feature | Size | Status |
| --- | --- | --- | --- |
| 1 | Start and end dates | M | shipped |
| 2 | Cheat days (one per month) | L | shipped |
| 3 | Notes | S | shipped (per-habit; per-day still open) |
| 4 | Tags and tag filtering | M | shipped |
| 5 | Graph in the detail pane | M | shipped (sparkline; chart ranges open) |
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
wrapped in the detail pane, and included in search. Phase 2 (per-day notes) is still open and moved
to [Carried over](#carried-over-from-01x).

The `note` field and its rendering already existed in 0.1.0; there was no way to write one from
the app.

**Phase 1 — per-habit note (small)**

- Editable in the add/edit dialog, multi-line, wrapped in the detail pane.
- CLI: `habit note <habit> "text"`, `habit note <habit> --clear`, and `habit note <habit>` to print.
- Included in search matching.

**Phase 2 — per-day note (optional)**

A `"notes": { "2026-11-10": "bad sleep, still showed up" }` map, shown in the detail pane for the
selected day and in `habit log`. Useful for a journal-ish workflow without turning the app into a
journal app.

**Open questions**

- Length limit? Prefer no hard limit, but wrap at the pane width and cap the pane height with a
  scroll indicator.
- Do notes belong in the JSON file or a sidecar? Same file — one file is the selling point.

---

## 4. Tags and tag filtering

**Shipped in 0.2.0.** Tags are normalised, sorted, rendered as chips and accepted as `#tag` in the
filter. Filtering is AND-only; OR syntax and rename/merge are still open.

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

- OR filtering (`#a|#b`) if the parser can stay simple.
- Tag rename/merge (`habit tag --rename old new`) to stop typo sprawl becoming permanent.

---

## 5. Graph in the detail pane

**Shipped in 0.2.0** as a compact rolling 7-day completion **sparkline** above the heatmap; the
heatmap stayed, and is now adaptive up to 52 weeks. No new dependency was needed. The richer chart
(axes, target line, switchable ranges, quota bars) is still open.

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

## 6. The UI should be gorgeous

**Shipped in 0.2.0**, with one explicit product decision: **square borders, not rounded**. The
capability detection, seven palettes, per-habit accents and responsive layout all landed.

A deliberate design pass, not a pile of colours. Goal: the screenshot should make someone want to
install it.

**Layout**

- Square borders everywhere (an explicit product decision), one shared gutter, no full-width
  horizontal rules.
- The detail pane gets a clear visual hierarchy: habit name as a heading, metadata dimmed, numbers
  right-aligned and tabular.
- Overlays (help, form, confirm) float over the panes using `Clear`; the backdrop is not dimmed,
  because that would need a second full redraw on every frame.

**Colour**

- Truecolor when the terminal reports it, 256-colour fallback, `mono` for the rest — detected once
  at startup, no config required.
- Per-habit accent derived from a hash of the id, so a habit keeps its colour across sessions
  without storing one; used for the detail heading and trend line.
- Dedicated semantics: green done, amber open, red missed, cyan cheat, dim not-due. Every colour
  also has a glyph or weight difference so nothing depends on colour alone.

**Motion and detail**

- No animation loop: the app redraws on input only. Any "motion" must be static styling.
- Sub-cell progress bars (partial blocks) for smoother bars.
- A real empty state with the first command to run.
- Status line that reads like a sentence and fades old messages.
- Selection shown by a full-width tinted bar plus `▸`, not by reversing a single glyph.

**Themes**

`dark`, `light`, `mono` stay. Consider `nord`/`solarized` as extra presets only if they cost nothing
beyond palette tables.

**Definition of done**

- Renders correctly at 60, 80, 96 and 200 columns (and degrades sensibly below that).
- A frame-by-frame review at each width, in all seven themes and both modes.
- `TestBackend` frame tests for the layout, help, form and confirm overlays.
- README screenshots regenerated from the final look.

---

## Carried over from 0.1.x

- Per-day notes, shown for the selected day and in `habit log`.
- Switchable trend ranges and a quota bar chart.
- Tag rename/merge.
- Optional reminders/notifications — fights the "no daemon" design, so it would be opt-in and
  OS-native (`launchd`/`systemd`), not an in-app loop.
- Import/export helpers (CSV, and reading a previous `data.json`) and a monthly HTML report.
- Windows CI.

## Non-goals

- Accounts, sync, cloud storage, telemetry.
- A plugin system or scripting language.
- A web UI or a server.
- Sub-second timers or background work of any kind.

## Compatibility policy

The version guard exists so an older binary can never silently mangle a newer file. Any change to
the on-disk shape — including "additive" fields like `ends`, `tags`, `cheats` and `notes` — bumps
`DATA_VERSION` and ships a migration in `store.rs`, because an old build would otherwise read a v1
file, drop the fields it does not know about on the next save, and lose data quietly. With the bump,
that old build refuses the file with a clear error instead, and the original is still on disk as
`data.json.bak`.

Implemented in 0.2.0: a version-1 file loads with defaults for the new fields and is stamped
version 2 on the next save, while a future version is refused. Every feature lands with tests in
`stats.rs` (or wherever the logic belongs) and, where it renders, a `TestBackend` frame test.
