# habit

**A calm habit tracker for the terminal.** Streaks, weekly quotas, heatmaps and a plain JSON file
you can read — in one 1.7 MB Rust binary with no daemon, no account and no network access.

[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-edition%202024-orange.svg)](Cargo.toml)

```
 habit  Tue 10 Nov 2026                                                           2/3 today
┌ Habits · 4 ────────────────────────────┐┌ Details ───────────────────────────────────────┐
│  ○ Gym             1/4 wk       ●●●●·●·││Read 20 pages                                   │
│▸ ● Read 20 pages   Mon–Fri      ●●○··○●││Mon–Fri · since 2026-09-14                      │
│  ● Meditate        daily        ●●●●●●●││                                                │
│  · Stretch         Mon Wed      ●····●·││streak     1 day   best 16 days                 │
│                                        ││this week  █████░░░░░  1/2                      │
│                                        ││last 30d   ████████░░  77% (17 of 22)           │
│                                        ││                                                │
│                                        ││Mon ·················░█░████░░                  │
│                                        ││Tue ·················█████████                  │
│                                        ││Wed ·················██░█████                   │
│                                        ││Thu ·················████████                   │
│                                        ││Fri ·················█████░░░                   │
│                                        ││Sat ·························                   │
│                                        ││Sun ·························                   │
│                                        ││                                                │
│                                        ││█ done  ░ missed  · not due                     │
│                                        ││34 check-ins                                    │
│                                        ││                                                │
└────────────────────────────────────────┘└────────────────────────────────────────────────┘
space toggle · a add · e edit · d archive · x delete · u undo · / filter · ? help · q quit
```

*(Colour is stripped in this capture; the real thing uses green/yellow/red and a dim border.)*

## Features

### Two interfaces, one program

Run `habit` and you get a full-screen TUI. Run `habit done gym` and you get a scriptable CLI. Both
read and write the same file, so terminal widgets, cron jobs and shell aliases all see the same
state.

### Schedules that match real life

| Schedule | Use it for | Streak counts |
| --- | --- | --- |
| `daily` | meditation, flossing | consecutive days |
| `mon,wed,fri` | gym, language class | consecutive *due* days — off days never break it |
| `4×/week` | “move four times a week” | consecutive *complete weeks* |

A quota habit is never “missed” on a Tuesday just because you went on Monday: only the weekly
total matters. Day-of-week habits skip their off days entirely, so a Mon/Wed/Fri streak isn't
reset by Thursday.

### Streak rules, explicitly

- A day that is **not due** is skipped, not counted as a miss.
- **Today** being unfinished never breaks a streak — it is still in progress.
- **Quota habits** count complete weeks. A half-finished week leaves the streak at its previous
  value instead of zeroing it, and the sidebar shows `2/4 this week` meanwhile.
- Days **before a habit was created** are never shown as misses.
- Checking off a **non-due day** is allowed and counts as extra credit (useful after a reshuffle).
- Longest streak is kept separately from the current one, so a broken streak doesn't erase history.

### The TUI

- **Habit list** with today's marker, a 7-day dot strip per habit and the current weekly progress.
- **Detail pane** with current and best streak, this week's bar, 30-day completion rate, a 9-week
  heatmap and a lifetime check-in count.
- **Add/edit dialog** with three schedule kinds, inline validation and a day picker.
- **Instant feedback**: every action reports itself in the status line (`✓ Gym · 2/4 this week`).
- **One-step undo** for every mutation — check-off, add, edit, archive and delete.
- **Archive** instead of delete: history is kept and the habit renders dimmed at the bottom.
- **Filter** (`/`), **help overlay** (`?`), **reload from disk** (`r`).
- **Responsive**: below 72 columns the detail pane collapses and the list takes the full width.
- **Themes** (`dark`, `light`, `mono`) and a configurable **week start**.

### The CLI

Every action is a subcommand, with real exit codes:

```console
$ habit add "Gym" --weekly 4
added Gym [gym] · 4×/week

$ habit done gym
✓ Gym on 2026-11-10 · 1/4 this week

$ habit done read --date 2026-11-09      # backfill a missed day
✓ Read 20 pages on 2026-11-09 · streak 2 days

$ habit list
ID       HABIT         SCHEDULE      TODAY STREAK
gym      Gym           4×/week           · 1 week
read     Read 20 pages Mon–Fri           ✓ 1 day
meditate Meditate      daily             ✓ 41 days
stretch  Stretch       Mon Wed           · 7 days

Today: 2 done · 4 active habits · space toggles in the TUI

$ habit stats --habit gym --days 90
Gym · 4×/week · since 2026-09-14
  streak       1 week (best 3 weeks)
  this week    1/4
  completion   77% (28 of 36) over 90 days
  check-ins    28
```

### Data safety

- **Atomic writes**: temp file → `fsync` → `rename`. A crash mid-save cannot truncate your history.
- **Automatic backup**: the previous version is kept as `data.json.bak` on every save.
- **Version guard**: a file written by a newer format is refused instead of silently mangled.
- **Tolerant loading**: a missing or empty file means “no habits yet”, not an error.
- **Hand-editable**: it is plain, pretty-printed JSON, and `r` reloads it without restarting.
- **Failed saves revert the change in memory**, so the screen never lies about what's on disk.

### Small on purpose

No database, no sync, no telemetry, no async runtime, no plugin API. Nine direct dependencies, all
mainstream. `cargo build --release` produces a single ~1.7 MB binary.

## Install

```sh
# from a clone
git clone https://github.com/Vulkan1125/habit-tracker
cd habit-tracker
cargo install --path .

# or straight from GitHub
cargo install --git https://github.com/Vulkan1125/habit-tracker
```

Or build and copy the binary yourself:

```sh
cargo build --release
install -m 755 target/release/habit ~/.local/bin/
```

Requires Rust 1.88 or newer (the MSRV is set in `Cargo.toml`; ratatui requires it too). Tested on
macOS and Linux. Windows terminals are supported by crossterm but untested here.

## Quick start

```sh
habit add "Gym" --weekly 4
habit add "Read 20 pages" --days mon,tue,wed,thu,fri
habit add "Meditate"
habit                      # open the TUI, press a / space / q
```

Nothing else to configure. The data file is created on the first change.

## TUI reference

### Reading the screen

| Glyph | Meaning |
| --- | --- |
| `●` | done today (TUI list, dot strip, heatmap) |
| `○` | due today but not done |
| `·` | not due today, not scheduled, archived, or outside the habit's lifetime |
| `░` | missed a day that was due |
| `▸` | selected row |
| `████░░░░` | progress bar: filled / remaining |
| `1/4 wk` | weekly quota progress for `n×/week` habits |

### Keys

| Key | Action |
| --- | --- |
| `space` | check off / un-check today |
| `j` `k`, `↑` `↓` | move selection |
| `g` `G`, `Home` `End` | first / last habit |
| `a` | add a habit |
| `e` | edit the selected habit |
| `d` | archive or restore (history kept) |
| `x` | delete the habit and its history (asks first) |
| `u` | undo the last change |
| `/` | filter by name — type to narrow, `enter` keeps, `esc` clears |
| `r` | reload the data file from disk |
| `?` | help overlay |
| `q`, `esc` | quit (first `esc` clears an active filter) |

In the add/edit dialog: `tab` / `shift-tab` moves between fields, `←→` changes the schedule kind,
`space` toggles a weekday, `enter` saves, `esc` cancels.

<details>
<summary>Add/edit dialog</summary>

```
┌ New habit ─────────────────────────────────────────────────┐
│Name        Journal                                         │
│Schedule    chosen days                                     │
│Days        Mon Tue Wed Thu Fri Sat Sun                     │
│Per week     3  (switch schedule to use)                    │
│                                                            │
│  save                                                      │
│tab next · arrows change · enter save · esc cancel          │
│                                                            │
│                                                            │
└────────────────────────────────────────────────────────────┘
```

</details>

<details>
<summary>Help overlay (<code>?</code>)</summary>

```
┌ Keys ────────────────────────────────────────────┐
│habit  terminal habit tracker                     │
│                                                  │
│space       check off / undo today                │
│j k ↑ ↓     move                                  │
│g G         first / last                          │
│a           add a habit                           │
│e           edit the selected habit               │
│d           archive or restore                    │
│x           delete (asks first)                   │
│u           undo the last change                  │
│/           filter by name                        │
│r           reload the file from disk             │
│?           this help                             │
│q           quit                                  │
│                                                  │
│any key closes this                               │
└──────────────────────────────────────────────────┘
```

</details>

## CLI reference

Global flags, usable before or after the subcommand:

| Flag | Effect |
| --- | --- |
| `--data <FILE>` | use a different data file |
| `--today <YYYY-MM-DD>` | pretend it is another day (demos, tests, backfilling) |

| Command | Arguments | What it does |
| --- | --- | --- |
| `habit` | | open the TUI |
| `habit add <NAME>` | `--weekly N` / `-w N`, `--days mon,wed` | create a habit; defaults to daily |
| `habit done <HABIT>` | `--date YYYY-MM-DD` | check off today or backfill a day |
| `habit undo <HABIT>` | `--date YYYY-MM-DD` | un-check a day |
| `habit list` | | table of habits, today's status and streaks |
| `habit stats` | `--habit <HABIT>`, `--days N` (default 30) | streak, week, completion and totals |
| `habit log` | `-n N` (default 14), `--all` | recent check-ins, newest first |
| `habit archive <HABIT>` | | toggle archived (history kept) |
| `habit remove <HABIT>` | `--force` | delete habit and history; refuses without `--force` |
| `habit path` | | print the data file path |
| `habit config` | | print effective config, data path and theme |

Habits can be referenced by **id, exact name (case-insensitive), or a unique id prefix** — so
`habit done gym`, `habit done Gym` and `habit done g` all work. Ambiguous prefixes list the
candidates instead of guessing.

The `TODAY` column uses `✓` for done, `○` for due but still open, and `·` for not scheduled today.

Exit code is `0` on success and `1` on any error, which makes `habit done gym && say done` safe.

## Schedules in depth

```sh
habit add "Gym" --weekly 4              # 4 sessions in a week, any days
habit add "Run" --weekly 3              # 3 sessions, any days
habit add "Class" --days mon,wed        # only Monday and Wednesday
habit add "Read" --days mon,tue,wed,thu,fri
habit add "Meditate"                    # every day (the default)
```

`--weekly` accepts 1–7 and conflicts with `--days`. Days accept `mon` … `sun`, full names, or
`1`–`7` (Monday = 1).

Consecutive runs of three or more days are displayed compactly (`Mon–Fri`).

### Worked examples

**Mon/Wed/Fri habit, today is Thursday.** Tuesday and Thursday are not due, so they are skipped.
If Monday and Wednesday are done, the streak is 2 and today's marker is `·` (nothing to do).

**4×/week habit with Monday, Tuesday, Thursday, Saturday done.** This week is complete: streak +1,
and `space` on Friday is still allowed as extra credit.

**4×/week habit with three sessions so far this week.** The week is not counted in the streak yet,
but it does not break it either — the sidebar shows `3/4 this week` and the streak stays at its
previous value until Sunday.

**A habit created today.** The heatmap and dot strip show the previous days as not due (`·`), not
as failures.

## Data

Resolution order for the data file:

1. `--data <FILE>`
2. `$HABITS_FILE`
3. `$XDG_DATA_HOME/habit/data.json` (only if absolute)
4. `$HOME/.local/share/habit/data.json`

Format (version 1) — one entry per day, habit ids:

```json
{
  "version": 1,
  "habits": [
    {
      "id": "gym",
      "name": "Gym",
      "schedule": { "kind": "times_per_week", "n": 4 },
      "created": "2026-09-14"
    },
    {
      "id": "read-20-pages",
      "name": "Read 20 pages",
      "schedule": { "kind": "weekdays", "days": ["Mon", "Tue", "Wed", "Thu", "Fri"] },
      "created": "2026-09-14"
    },
    {
      "id": "meditate",
      "name": "Meditate",
      "schedule": { "kind": "daily" },
      "created": "2026-10-01",
      "note": "10 minutes is enough"
    }
  ],
  "log": {
    "2026-11-09": ["gym", "read-20-pages", "meditate"],
    "2026-11-10": ["meditate"]
  }
}
```

Ids are slugified from the name (`Morning Walk!` → `morning-walk`) and suffixed on collision
(`morning-walk-2`). Optional fields (`archived`, `note`) are omitted when empty.

**Editing by hand is supported**: keep it valid JSON, then press `r` in the TUI or just run another
command. Empty or whitespace-only files are treated as “no data”.

## Configuration

Optional, at `~/.config/habit/config.toml` (`$XDG_CONFIG_HOME/habit/config.toml` when set):

```toml
theme = "dark"        # dark | light | mono
week_start = "Mon"    # Mon | Tue | … | Sun
```

- `theme` changes the palette; `mono` uses only bold/dim so it works on any terminal.
- `week_start` changes which weekday begins a week, affecting weekly quotas and the heatmap rows.
- A missing file means defaults. A malformed file prints a warning (into the status line in the
  TUI) and falls back to defaults rather than refusing to start. Unknown keys are rejected, so a
  typo like `week_startt` is reported instead of ignored.

## Recipes

```sh
# short alias for the most common action
alias g='habit done gym'

# weekly review: streaks and completion for everything
habit stats --days 7

# commit your history alongside your notes
cp "$(habit path)" ~/notes/habits-$(date +%F).json
```

A habit can be checked off from anywhere a shell can run, which means cron, `launchd`, git hooks or
a `starship` prompt can all trigger or read it.

## Design notes

```
src/
  model.rs    habits, schedules, log, JSON shape, id slugs
  store.rs    path resolution, atomic load/save, format version
  stats.rs    streaks, completion, heatmap — pure functions
  app.rs      TUI state, key handling, persist-on-change, undo
  cli.rs      subcommands and their text output
  config.rs   config.toml
  theme.rs    palettes
  ui/         ratatui rendering: list, detail, form, help
```

A few decisions worth knowing:

- **Statistics are pure functions** `(habit, log, today) -> answer`. `today` is injected everywhere,
  which is why `--today` works and why the streak tests are deterministic instead of clock-dependent.
- **Undo is snapshot-based.** A whole `Data` snapshot is tiny at personal scale, so every mutation
  gets undo for free — including delete — instead of a bespoke inverse per action.
- **Persistence happens on every change**, not on quit, and a failed save rolls the in-memory state
  back. Killing the terminal never loses a check-off.
- **The UI is tested without a TTY** using ratatui's `TestBackend`: the tests render frames and
  assert on the buffer text.
- **JSON over SQLite** because a personal habit log is small and the value of being able to read,
  diff and `grep` the file outweighs query performance.

## Development

```sh
cargo test                                  # 58 tests
cargo clippy --all-targets -- -D warnings
cargo fmt --check
cargo build --release
```

All of the above must pass locally before a release. CI is not wired up yet.

Test coverage focuses on the parts where bugs are invisible in the UI: streak edge cases across
week boundaries and quota weeks, schedule compliance, JSON round-trips, atomic-save behaviour
(including a simulated future format version), form validation, and frame rendering.

## Roadmap

Deliberately not included in 0.1.0, roughly in priority order:

- Backfilling days from inside the TUI (CLI `--date` works today).
- Editing per-habit notes in the TUI (the field and rendering already exist).
- Optional reminders/notifications — fights the “no daemon” design, so it would be opt-in.
- Import/export helpers and a monthly HTML report.
- Windows CI.

## License

[MIT](LICENSE) © 2026 Mahendra Basala
