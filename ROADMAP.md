# Roadmap

Planned work for `habit`, roughly in the order it would be built. Nothing here is a promise of a
date; it is a design sketch so the decisions are written down before the code exists.

`0.1.0` is the released baseline. Everything below is additive and keeps the two rules that make
the project what it is: **one plain JSON file** and **no daemon, account or network**.

## How the pieces fit together

Six requested features, plus the items carried over from 0.1.0.

| # | Feature | Size | Depends on |
| --- | --- | --- | --- |
| 1 | Start and end dates | M | — |
| 2 | Cheat days (one per month) | L | date navigation in the TUI |
| 3 | Notes | S | — |
| 4 | Tags and tag filtering | M | — |
| 5 | Graph in the detail pane | M | — |
| 6 | A genuinely beautiful UI | L | 1–5, so it can be designed once |

Suggested sequence: **3 → 4 → 1 → 2 → 5 → 6**. Notes and tags are isolated and make the list
easier to live with immediately; dates and cheat days touch the streak engine, which is the part
that must stay correct; the graph and the visual pass come last so they only have to be designed
and tuned once.

A prerequisite for 2 and for half of 3: **the TUI can only act on today right now.** Backfilling
exists only as `habit done --date`. Any date-scoped feature needs a way to move the TUI's cursor
to another day first.

---

## 1. Start and end dates for habits

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

**Open questions**

- Does a finished habit keep accruing "missed" days after `ends`? No — it should freeze, and the
  detail pane should say `finished 2026-12-31` instead of showing a decaying streak.
- Should `ends` in the past auto-archive the habit, or keep two separate concepts? Prefer separate:
  `ends` is a scheduled finish, `archived` is a manual hide.
- Editing `created` after check-ins exist: warn, but allow — the log keeps entries outside the
  window and stats already ignore them.

---

## 2. Cheat days — one per month

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
- TUI: `c` toggles a cheat day on the selected date; the heatmap uses a distinct colour (planned:
  cyan) with its own legend entry; the detail pane shows `cheats used 1/1 · November`.
- Help overlay and README streak rules both need the new row.

**Why this is the biggest item:** the streak engine currently takes `(habit, log, today)`. It would
become `(habit, log, cheats, today)` and gain a month-bucketing rule, and every existing streak
test needs a cheat-day sibling. That is the correct place for the complexity — keep it in
`stats.rs` as pure functions.

---

## 3. Notes

The `note` field and its rendering already exist; there is no way to write one from the app.

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

**Data**

`tags: Vec<String>` on `Habit`, normalised to lowercase kebab-case, de-duplicated, sorted. Omitted
from JSON when empty, like `note` and `archived`.

**Surfaces**

- CLI: `habit add "X" --tag health --tag morning` (repeatable), `habit tag <habit> +health`,
  `habit tag <habit> -health`, `habit tags` (all tags with counts), and `habit list --tag health`.
- TUI: tag chips after the habit name, a tag facet in the filter bar, and `#health` typed directly
  into `/` to filter. Search should match name, tags and notes.
- Detail pane: tags listed under the schedule line.

**Open questions**

- Multi-tag filter: AND or OR? Default AND, with `/` accepting `#a #b` as AND and a documented
  `#a|#b` form for OR if the filter parser stays simple enough.
- Tag rename/merge: `habit tag --rename old new` is cheap and prevents typo sprawl from becoming
  permanent.

---

## 5. Graph in the detail pane

The 9-week heatmap answers "which days did I do it". It does not answer "am I trending up".

**Proposal**

- Keep the heatmap (it is the identity of the app) and add above it a **rolling completion line**
  for the last 30 days, rendered with ratatui's `Chart`, with a 7-day moving average and a faint
  target line at the schedule's expected rate.
- Below 72 columns, degrade to `Sparkline`; below 60, drop to a single `LineGauge` for the period.
- For quota habits, plot sessions-per-week as bars (`BarChart`) instead of a daily line, since a
  daily line is noise when only the weekly total matters.
- Time range switchable from the pane (`[` `]` to cycle 30d / 90d / 1y).

**Constraints**

- No new dependencies: ratatui already ships `Chart`, `Sparkline`, `BarChart` and `LineGauge`.
- Keep stats pure: the pane asks `stats::` for a series, it does not compute.
- Must stay legible in a 60-column terminal and in the `mono` theme.

---

## 6. The UI should be gorgeous

A deliberate design pass, not a pile of colours. Goal: the screenshot should make someone want to
install it.

**Layout**

- Rounded borders everywhere, one shared gutter, no full-width horizontal rules.
- The detail pane gets a clear visual hierarchy: habit name as a heading, metadata dimmed, numbers
  right-aligned and tabular.
- Overlays (help, form, confirm) float on a dimmed backdrop instead of replacing the screen.

**Colour**

- Truecolor when the terminal reports it, 256-colour fallback, `mono` for the rest — detected once
  at startup, no config required.
- Per-habit accent derived from a hash of the id, so a habit keeps its colour across sessions
  without storing one; used for the selection bar, dots and graph line.
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

- Renders correctly at 60, 80, 120 and 200 columns, in all three themes.
- A frame-by-frame review at each width, with `TestBackend` snapshots as regression tests.
- README screenshots regenerated from the final look.

---

## Carried over from 0.1.0

- Backfilling days from inside the TUI (CLI `--date` works today). This is a prerequisite for
  cheat days, so it moves up.
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

Each release upgrades the file on first open and keeps backups of the pre-migration file. Every
feature above lands with tests in `stats.rs` (or wherever the logic belongs) and, where it renders,
a `TestBackend` frame test.
