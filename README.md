# habit

**A calm habit tracker for the terminal.** Streaks, weekly quotas, cheat days, tags, descriptions,
dated notes and a heatmap — plus a plain JSON file you can read, diff and `grep`. One 1.8 MB Rust
binary with no daemon, no account and no network access.

[![CI](https://github.com/Vulkan1125/habit-tracker/actions/workflows/ci.yml/badge.svg)](https://github.com/Vulkan1125/habit-tracker/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-edition%202024-orange.svg)](Cargo.toml)

```
 habit  Tue 10 Nov 2026                                                            █████░ 3/4 today
┌────────────────────────────────────────────┐┌ Details ───────────────────────────────────────────┐
│ Habits 4   Tags 4                          ││Morning run                                         │
│▸ ● Morning run         daily        ○●◆○●●●││description:                                        │
│  ● Read 20 pages       51d left     ●●●··●●││    5k before breakfast                             │
│  ○ Strength            1/3 wk       ●·●··●·││    daily · since 2026-01-01                        │
│  ● Meditate            daily        ●●●●●●●││    #health #morning                                │
│  · Evening walk        daily        ○●●●●○◌││                                                    │
│                                            ││streak     3 days   best 10 days                    │
│                                            ││this week  ██████████  2/2                          │
│                                            ││last 30d   █████████░  85% (24 of 28)               │
│                                            ││                                                    │
│                                            ││Mon ·████░██████████░██████████░██████████░███████  │
│                                            ││Tue ·███████░██████████░██████████░██████████░████  │
│                                            ││Wed ·██████████░██████████░██████████░██████████░   │
│                                            ││Thu ███░██████████░██████████░██████████░████████   │
│                                            ││Fri ██████░██████████░██████████░██████████░████◆   │
│                                            ││Sat █████████░░█████████░██████████░██████████░█░   │
│                                            ││Sun █░████████░█░██████████░██████████░██████████   │
│                                            ││                                                    │
│                                            ││● done  ◆ cheat  ░ missed  ◌ open  · not due        │
│                                            ││282 check-ins  ◆ 2 cheat this Nov                   │
│                                            ││                                                    │
│                                            ││notes                                               │
│                                            ││2026-11-10 easy recovery run after yesterday        │
│                                            ││2026-11-09 legs heavy but went anyway               │
│                                            ││2026-11-08 felt strong, 5k in 26:30                 │
│                                            ││                                                    │
└────────────────────────────────────────────┘└────────────────────────────────────────────────────┘
space toggle · n note · tab tags · b bar · / filter · ? help · q quit
```

*(Colour is stripped in this capture. The real thing has seven palettes in light and dark mode and
per-habit accents; borders stay square on purpose.)*

## Features

### Two interfaces, one program

Run `habit` and you get a full-screen TUI. Run `habit done gym` and you get a scriptable CLI. Both
read and write the same file, so terminal widgets, cron jobs and shell aliases all see the same
state. The TUI can now act on any past day, so backfilling no longer needs a second tool.

### Schedules that match real life

| Schedule | Use it for | Streak counts |
| --- | --- | --- |
| `daily` | meditation, flossing | consecutive days |
| `mon,wed,fri` | gym, language class | consecutive *due* days — off days never break it |
| `4×/week` | “move four times a week” | consecutive *complete weeks* |

A quota habit is never “missed” on a Tuesday just because you went on Monday: only the weekly
total matters. Day-of-week habits skip their off days entirely, so a Mon/Wed/Fri streak isn't
reset by Thursday.

### Start and end dates

Every habit has a start date and may have an inclusive end date:

```sh
habit add "Read 20 pages" --start 2026-11-01 --end 2026-12-31
habit start read-20-pages 2026-11-05
habit end read-20-pages --date 2026-12-15
habit end read-20-pages --clear
```

Days outside that window are **not due**, not misses. A finished habit is labelled `finished`,
sorts with archived habits, and its streak freezes on its final day instead of decaying forever.
Both dates are editable in the TUI's add/edit dialog.

### Cheat days — one per calendar month

Life happens. Mark a day as a cheat day and the streak skips it exactly like a non-due day. Each
habit gets **one excused day per calendar month**; the second one in the same month is an ordinary
miss and breaks the streak. The UI says so before it happens.

```sh
habit cheat gym                 # excuse today
habit cheat gym --date 2026-11-06
habit cheat gym --off           # remove it again
```

- Cheat days never count as completions and are excluded from the completion-rate denominator.
- They do not fill a weekly quota — otherwise `4×/week` would become “cheat four times a week”.
- The first cheat in a month is `◆` in its own colour; later ones render as missed days, so the
  difference is always visible.
- Archive and delete carry cheat days along with check-ins.
- If you backfill an *earlier* cheat into a month that already has one, the earlier date becomes the
  excused one and the UI warns that the later one now breaks the streak.

### Streak rules, explicitly

- A day that is **not due** is skipped, not counted as a miss.
- **Today** being unfinished never breaks a streak; it renders as `◌` (open) rather than a miss.
- **Quota habits** count complete weeks. A half-finished week leaves the streak at its previous
  value instead of zeroing it, and the sidebar shows `2/4 this week` meanwhile.
- Days **before the start date** and **after the end date** are never shown as misses.
- **The first cheat day** in a calendar month is skipped; any later one counts as a miss.
- Checking off a **non-due day** is allowed and counts as extra credit (useful after a reshuffle).
- Longest streak is kept separately from the current one, so a broken streak doesn't erase history.

### Tags, descriptions and dated notes

Tags are normalised to lowercase kebab-case, de-duplicated and sorted: `#Health & Focus` becomes
`health-focus`. A habit's **description** is free text and appears wrapped in the detail pane.
**Dated notes** are separate: one note per habit per day, listed newest first with the date on the
left.

```sh
habit add "Morning run" --tag health --tag morning --description "5k before breakfast"
habit tag gym health cardio       # add tags
habit tag gym cardio --remove     # remove one
habit tags                        # every tag with a habit count
habit list --tag health           # AND: every listed tag must match
habit describe gym "ten minutes is enough"
habit describe gym                # print the current description
habit describe gym --clear
```

`habit note` remains an alias for `habit describe`, and `habit add --note` still sets the
description for compatibility.

In the TUI, `tab` switches the left pane between **Habits** and **Tags**. The tag list shows each
tag with its count of still-active habits; open a tag with `enter`, `space` or a click to see its
active habits and, under an **Archive** heading, the archived or finished ones. Press `n` to write
or edit a note — right after checking a habit off is the natural moment. The dialog opens on the
day you are looking at (today unless you moved with `[` / `]`), and `tab` moves into its date field
if you want another day: `↑` / `↓` step one day and typing takes any `YYYY-MM-DD`. A date in the
future or before the habit started is refused with an inline message and the dialog stays open;
`esc` cancels, and saving an empty note removes it. Dates are shown on the left, notes on the
right, newest first.

```
 habit  Tue 10 Nov 2026                                                            █████░ 3/4 today
┌────────────────────────────────────────────┐┌ Details ───────────────────────────────────────────┐
│ Habits 4   Tags 4                          ││Morning run                                         │
│ ‹ all tags   #health · 2 active            ││description:                                        │
│▸ ● Morning run         daily        ○●◆○●●●││    5k before breakfast                             │
│  ○ Strength            1/3 wk       ●·●··●·││    daily · since 2026-01-01                        │
│   Archive                                  ││    #health #morning                                │
│  · Evening walk        daily        ○●●●●○◌││                                                    │
│                                            ││streak     3 days   best 10 days                    │
│                                            ││this week  ██████████  2/2                          │
│                                            ││last 30d   █████████░  85% (24 of 28)               │
│                                            ││                                                    │
│                                            ││Mon ·████░██████████░██████████░██████████░███████  │
│                                            ││Tue ·███████░██████████░██████████░██████████░████  │
│                                            ││Wed ·██████████░██████████░██████████░██████████░   │
│                                            ││Thu ███░██████████░██████████░██████████░████████   │
│                                            ││Fri ██████░██████████░██████████░██████████░████◆   │
│                                            ││Sat █████████░░█████████░██████████░██████████░█░   │
│                                            ││Sun █░████████░█░██████████░██████████░██████████   │
│                                            ││                                                    │
└────────────────────────────────────────────┘└────────────────────────────────────────────────────┘
space toggle · n note · tab tags · b bar · / filter · ? help · q quit
```

In the TUI press `/` and type. `#health` matches tags, while plain words match the name, id, tags
and description. Multiple tokens are ANDed, so `run #morning` narrows to morning runs.

### The TUI

- **Habit list and Tags tabs** in the left pane, switchable with `tab` and clickable with the
  mouse. Each tag shows its active-habit count; opening one splits active habits from archived and
  finished ones under an **Archive** heading.
- **Habit rows** with the selected day's marker, a 7-day dot strip and weekly quota progress.
  Habits with an end date replace the schedule label with a countdown (`51d left`, `last day`,
  `ended`), turning amber in the final three days.
- **Date navigation** (`[` `]`, `t` for today): check off, excuse, inspect or annotate any past day.
- **Detail pane** with `description:` directly under the habit title, and the description text,
  schedule line and tags all indented beneath it, then current and best streak, this week's bar,
  30-day completion, an adaptive heatmap (up to 52 weeks) and dated notes (date left, note right).
- **Dated notes**, newest first: press `n` after checking a habit off to journal the day, or `tab`
  inside the dialog to file it under another date (never the future, never before the habit's
  start); empty input removes the note and edits are covered by undo.
- **Hideable bottom bar**: `b` reclaims the hint line and gives the panes one more row.
- **Add/edit dialog** with schedule picker, weekday toggles, start/end dates, tags and description —
  all validated inline.
- **Instant feedback** in the status line, including streak effects and a warning when a second
  cheat in a month will break the run.
- **One-step undo** for every mutation — check-off, cheat, add, edit, archive and delete.
- **Archive** instead of delete: history is kept and the habit renders dimmed at the bottom.
- **Tag-aware filter** (`/`), **help overlay** (`?`), **reload from disk** (`r`).
- **Theme switching without editing a file**: `T` steps through the seven palettes and `m` flips
  dark/light; both are written back to `config.toml` immediately.
- **Responsive**: below 72 columns the detail pane collapses and the list takes the full width.
  Footer hints and the detail legend shorten themselves on narrow terminals.

### Themes, in light and dark

Seven palettes ship with both modes:

| Theme | Mood |
| --- | --- |
| `terminal` | inherits your terminal's own foreground/background and ANSI colours |
| `rosepine` | muted rose, pine and foam |
| `catppuccin` | soft pastels |
| `tokyonight` | deep indigo and neon accents |
| `gruvbox` | warm retro amber and olive |
| `evergreen` | calm green and teal |
| `mono` | no colour at all: weight and glyphs carry the meaning |

```toml
theme = "catppuccin"
mode = "light"          # dark | light
```

Colours are defined in truecolour and downgraded automatically to the 256-colour cube, then to
the nearest of the basic 16 colours when the terminal does not advertise better support (16-colour
mode also leaves your background untouched). Each habit gets a stable accent derived from a hash
of its id, so its colour survives sessions without being stored. `habit themes` lists the names.

### The CLI

Every action is a subcommand, with real exit codes:

```console
$ habit add "Gym" --weekly 4 --tag health
added Gym [gym] · 4×/week · since 2026-11-10
  tags #health

$ habit done gym
✓ Gym on 2026-11-10 · 1/4 this week

$ habit cheat gym --date 2026-11-06
◆ Gym on 2026-11-06 · excused (1/1 cheat day in November)

$ habit done read --date 2026-11-09      # backfill a missed day
✓ Read 20 pages on 2026-11-09 · streak 2 days

$ habit list --tag health
ID       HABIT         SCHEDULE      TODAY STREAK
gym      Gym           4×/week           · 1 week
run      Morning run   daily             ✓ 3 days

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
- **Version upgrade**: an older file loads with the new fields defaulted and is stamped as the
  current format on its next save, so old data is not stranded.
- **Tolerant loading**: a missing or empty file means “no habits yet”, not an error.
- **Hand-editable**: plain, pretty-printed JSON, and `r` reloads it without restarting.
- **Failed saves revert the change in memory**, so the screen never lies about what's on disk.

### Small on purpose

No database, no sync, no telemetry, no async runtime, no plugin API. Nine direct dependencies, all
mainstream. `cargo build --release` produces a single ~1.8 MB binary.

## Install

Prebuilt packages are attached to each [GitHub release](https://github.com/Vulkan1125/habit-tracker/releases/latest):

| Platform | Asset | Install |
| --- | --- | --- |
| Debian / Ubuntu | `habit_X.Y.Z_amd64.deb` | `sudo dpkg -i habit_X.Y.Z_amd64.deb` |
| Fedora / RHEL | `habit-X.Y.Z-1.x86_64.rpm` | `sudo dnf install ./habit-X.Y.Z-1.x86_64.rpm` |
| Arch Linux | `habit-X.Y.Z-1-x86_64.pkg.tar.zst` | `sudo pacman -U habit-X.Y.Z-1-x86_64.pkg.tar.zst` |
| macOS (universal) | `habit-X.Y.Z-macos-universal.tar.gz` | `tar -xzf habit-X.Y.Z-macos-universal.tar.gz && sudo install -m 755 habit-X.Y.Z-macos-universal/habit /usr/local/bin/` |
| Linux (static) | `habit-X.Y.Z-x86_64-linux-musl.tar.gz` | `tar -xzf habit-X.Y.Z-x86_64-linux-musl.tar.gz && install -m 755 habit-X.Y.Z-x86_64-linux-musl/habit ~/.local/bin/` |

Every release also carries a `SHA256SUMS` file. The macOS binary is universal (Apple Silicon and
Intel) but unsigned; if Gatekeeper quarantines it, remove the attribute with
`xattr -d com.apple.quarantine habit`.

From source:

```sh
# from a clone
git clone https://github.com/Vulkan1125/habit-tracker
cd habit-tracker
cargo install --path .

# or straight from GitHub
cargo install --git https://github.com/Vulkan1125/habit-tracker
```

Updating an existing install (an older `habit` earlier on `PATH` will otherwise keep running):

```sh
cargo install --path . --force   # from a clone
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
habit add "Gym" --weekly 4 --tag health
habit add "Read 20 pages" --days mon,tue,wed,thu,fri --start 2026-11-01 --end 2026-12-31
habit add "Meditate" --description "10 minutes is enough"
habit                      # open the TUI, press a / space / c / [ ] / q
```

Nothing else to configure. The data file is created on the first change.

## TUI reference

### Reading the screen

| Glyph | Meaning |
| --- | --- |
| `●` | done on the selected day (list, dot strip, heatmap) |
| `○` | due but open in the row marker; a missed day in the dot strip — the colour tells them apart |
| `◆` | cheat day (the excused first one in the month) |
| `░` | a missed day in the heatmap |
| `◌` | today, still open |
| `·` | not due, archived, finished, or outside the habit's window |
| `▸` | selected row |
| `████░░░░` | progress bar: filled / remaining |
| `1/4 wk` | weekly quota progress for `n×/week` habits |

### Keys

| Key | Action |
| --- | --- |
| `space` | check off / un-check the selected day (`n` then adds a note) |
| `tab` | switch the left pane between Habits and Tags |
| `n` | write or edit a note; the dialog's date field can move it to any past day |
| `b` | show or hide the bottom bar |
| `c` | toggle a cheat day on the selected day |
| `[` `]` | move the selected day back / forward (never into the future) |
| `t` | jump back to today |
| `j` `k`, `↑` `↓` | move the habit or tag selection |
| `g` `G`, `Home` `End` | first / last habit |
| `a` | add a habit |
| `e` | edit the selected habit |
| `d` | archive or restore (history kept) |
| `x` | delete the habit and its history (asks first) |
| `u` | undo the last change |
| `/` | filter — plain text matches name/id/tags/description, `#tag` matches tags, `enter` keeps, `esc` clears |
| `r` | reload the data file from disk |
| `T` | next colour theme (saved to config) |
| `m` | switch dark / light mode (saved) |
| `?` | help overlay |
| `q`, `esc` | quit — `esc` first clears a filter, then backs out of a tag view |

In the add/edit dialog: `tab` / `shift-tab` moves between fields, `←→` changes the schedule kind,
`space` toggles a weekday, `enter` saves, `esc` cancels. The fields are name, schedule, weekday
selection, weekly quota, start date, end date, tags and description.

<details>
<summary>Add/edit dialog</summary>

```
┌ New habit ─────────────────────────────────────────────────┐
│Name        Journal                                         │
│Schedule    chosen days                                     │
│Days        Mon Tue Wed Thu Fri Sat Sun                     │
│Per week     3  (switch schedule to use)                    │
│Starts      2026-11-10                                      │
│Ends        never                                           │
│Tags        health, morning                                 │
│Description optional                                        │
│                                                            │
│  save                                                      │
│tab next · arrows change · enter save · esc cancel          │
└────────────────────────────────────────────────────────────┘
```

</details>

<details>
<summary>Help overlay (<code>?</code>)</summary>

```
┌ Keys ────────────────────────────────────────────┐
│habit  terminal habit tracker                     │
│                                                  │
│space       check off / undo the selected day     │
│tab         switch habits / tags                  │
│n           note for a chosen day                 │
│b           show / hide the bottom bar            │
│c           cheat day (one per month)             │
│[ ]         previous / next day                   │
│t           back to today                         │
│j k ↑ ↓     move                                  │
│g G         first / last                          │
│a           add a habit                           │
│e           edit the selected habit               │
│d           archive or restore                    │
│x           delete (asks first)                   │
│u           undo the last change                  │
│/           filter by text, tag or #tag           │
│T           next colour theme                     │
│m           dark / light mode                     │
│r           reload the file from disk             │
│?           this help                             │
│esc         back out / quit                       │
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
| `habit add <NAME>` | `--weekly N` / `-w N`, `--days mon,wed`, `--start D`, `--end D`, `--tag TAG` (repeatable), `--description TEXT` (`--note` still works) | create a habit; defaults to daily, starting today |
| `habit done <HABIT>` | `--date YYYY-MM-DD` | check off today or backfill a day |
| `habit undo <HABIT>` | `--date YYYY-MM-DD` | un-check a day |
| `habit cheat <HABIT>` | `--date YYYY-MM-DD`, `--off` | mark or remove an excused day |
| `habit list` | `--tag TAG` (repeatable) | table of habits, today's status and streaks |
| `habit stats` | `--habit <HABIT>`, `--days N` (default 30) | streak, week, completion, check-ins, cheats |
| `habit log` | `-n N` (default 14), `--all` | recent check-ins and cheat days, newest first |
| `habit tag <HABIT> <TAG>…` | `--remove` | add or remove tags |
| `habit tags` | | every tag with its habit count |
| `habit describe <HABIT> [TEXT]` | `--clear` | set, print or clear the description (`habit note` is an alias) |
| `habit start <HABIT> <DATE>` | | move the first day the habit counts |
| `habit end <HABIT>` | `--date D`, `--clear` | set or clear the inclusive end date |
| `habit archive <HABIT>` | | toggle archived (history kept) |
| `habit remove <HABIT>` | `--force` | delete habit and history; refuses without `--force` |
| `habit path` | | print the data file path |
| `habit config` | | print effective config, data path, theme and mode |
| `habit themes` | | list the palettes |

Habits can be referenced by **id, exact name (case-insensitive), or a unique id prefix** — so
`habit done gym`, `habit done Gym` and `habit done g` all work. Ambiguous prefixes list the
candidates instead of guessing.

The `TODAY` column uses `✓` for done, `◆` for an excused day, `○` for due but still open, and `·`
for not scheduled, archived or finished. Days outside a habit's window are refused by `done` and
`cheat` with a clear message.

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
`1`–`7` (Monday = 1). Consecutive runs of three or more days are displayed compactly (`Mon–Fri`).

### Worked examples

**Mon/Wed/Fri habit, today is Thursday.** Tuesday and Thursday are not due, so they are skipped.
If Monday and Wednesday are done, the streak is 2 and today's marker is `·` (nothing to do).

**4×/week habit with Monday, Tuesday, Thursday, Saturday done.** This week is complete: streak +1,
and `space` on Friday is still allowed as extra credit.

**4×/week habit with three sessions so far this week.** The week is not counted in the streak yet,
but it does not break it either — the sidebar shows `3/4 this week` and the streak stays at its
previous value until the week closes.

**A habit created today.** The heatmap and dot strip show the previous days as not due (`·`), not
as failures.

**A daily habit with a cheat day on the 6th.** The 6th is `◆` and the streak bridges it. A second
cheat on the 7th must be checked off to avoid a miss; otherwise it renders as a missed day and the
run restarts after it.

## Data

Resolution order for the data file:

1. `--data <FILE>`
2. `$HABITS_FILE`
3. `$XDG_DATA_HOME/habit/data.json` (only if absolute)
4. `$HOME/.local/share/habit/data.json`

Format (version 3) — habits carry their window and metadata; `log` and `cheats` are parallel maps
from date to habit ids, and `notes` maps a date to that habit's journal entry:

```json
{
  "version": 3,
  "habits": [
    {
      "id": "gym",
      "name": "Gym",
      "schedule": { "kind": "times_per_week", "n": 4 },
      "created": "2026-09-14",
      "tags": ["health"],
      "note": "ten minutes is enough",
      "notes": {
        "2026-11-09": "legs heavy but went anyway",
        "2026-11-10": "easy recovery run"
      }
    },
    {
      "id": "read-20-pages",
      "name": "Read 20 pages",
      "schedule": { "kind": "weekdays", "days": ["Mon", "Tue", "Wed", "Thu", "Fri"] },
      "created": "2026-09-14",
      "ends": "2026-12-31"
    }
  ],
  "log": {
    "2026-11-09": ["gym", "read-20-pages"],
    "2026-11-10": ["gym"]
  },
  "cheats": {
    "2026-11-06": ["gym"],
    "2026-11-07": ["gym"]
  }
}
```

Ids are slugified from the name (`Morning Walk!` → `morning-walk`) and suffixed on collision
(`morning-walk-2`). Optional fields (`ends`, `archived`, `note`, `tags`, `notes`) are omitted when
empty. The `note` field is the habit's description; dated entries live under `notes`.
The first cheat for a habit in a calendar month is the excused one; the map itself stores every
marked day so the UI can show and explain the second.

**Editing by hand is supported**: keep it valid JSON, then press `r` in the TUI or just run another
command. Empty or whitespace-only files are treated as “no data”. Older (version 1 or 2) files
load with the new fields defaulted and are written back as version 3.

## Configuration

Optional, at `~/.config/habit/config.toml` (`$XDG_CONFIG_HOME/habit/config.toml` when set):

```toml
theme = "catppuccin" # terminal | rosepine | catppuccin | tokyonight | gruvbox | evergreen | mono
mode = "light"       # dark | light
week_start = "Mon"   # Mon | Tue | … | Sun
```

- `theme` picks the palette and `mode` picks its light or dark variant. Every palette has both.
- `mono` uses only bold/dim, so it works on any terminal.
- The 0.1.x spellings `theme = "dark"` and `theme = "light"` still work: they select the
  `terminal` palette with that mode.
- `week_start` changes which weekday begins a week, affecting weekly quotas and heatmap rows.
- A missing file means defaults. A malformed file prints a warning (into the status line in the
  TUI) and falls back to defaults rather than refusing to start. Unknown keys are rejected, so a
  typo like `week_startt` is reported instead of ignored.

## Recipes

```sh
# short alias for the most common action
alias g='habit done gym'

# weekly review: streaks and completion for everything
habit stats --days 7

# everything in one area, with their descriptions
habit list --tag health
habit stats --habit morning-run

# excuse a planned rest day without breaking the run
habit cheat gym --date 2026-11-06

# commit your history alongside your data
cp "$(habit path)" ~/notes/habits-$(date +%F).json
```

A habit can be checked off from anywhere a shell can run, which means cron, `launchd`, git hooks or
a `starship` prompt can all trigger or read it.

## Design notes

```
src/
  model.rs    habits, schedules, log, cheats, JSON shape, tags, id slugs
  store.rs    path resolution, atomic load/save, format version and upgrade
  stats.rs    streaks, completion, heatmap — pure functions
  app.rs      TUI state, date navigation, key handling, persist-on-change, undo
  cli.rs      subcommands and their text output
  config.rs   config.toml
  theme.rs    seven palettes × light/dark, capability detection
  ui/         ratatui rendering: header, list, detail, form, help
```

A few decisions worth knowing:

- **Statistics are pure functions** `(habit, history, today) -> answer`, where `history` bundles
  the completion log and cheat days. `today` is injected everywhere, which is why `--today` works
  and why the streak tests are deterministic instead of clock-dependent.
- **Cheat days are a separate map**, not a flag inside a check-in. “Done” and “excused” never get
  confused, and the monthly allowance is derived at query time, so reordering or removing a cheat
  immediately recomputes the streak.
- **Undo is snapshot-based.** A whole `Data` snapshot is tiny at personal scale, so every mutation
  gets undo for free — including delete — instead of a bespoke inverse per action.
- **Persistence happens on every change**, not on quit, and a failed save rolls the in-memory state
  back. Killing the terminal never loses a check-off.
- **The UI is tested without a TTY** using ratatui's `TestBackend`: the tests render frames and
  assert on the buffer text.
- **Themes are tables, not code.** Palettes carry truecolour values, a capability check downgrades
  them, and each habit's accent is hashed from its id so no colour needs to be stored.
- **JSON over SQLite** because a personal habit log is small and the value of being able to read,
  diff and `grep` the file outweighs query performance.

## Development

```sh
cargo test                                  # 139 tests
cargo clippy --all-targets -- -D warnings
cargo fmt --check
cargo build --release
```

CI runs format, clippy and tests on Linux and macOS, plus a check against the MSRV on Rust 1.88
(see `.github/workflows/ci.yml`).

Test coverage focuses on the parts where bugs are invisible in the UI: streak edge cases across
week boundaries, quota weeks, cheat-day monthly allowances and backfills, start/end windows,
schedule compliance, tag normalisation, JSON round-trips and version upgrades, atomic-save
behaviour (including a simulated future format version), form validation, and frame rendering at
several widths and in the overlays.

## Roadmap

`0.2.0` ships all six features that were planned for this release: start/end dates, monthly cheat
days, notes, tags and filtering, a detail-pane trend graph, and the visual/theming pass.
**[ROADMAP.md](ROADMAP.md)** keeps the original design notes and decisions.

Ideas still on the table, roughly in order:

- A quota bar chart and richer multi-tag views.
- Tag rename/merge to stop typo sprawl.
- Opt-in OS-native reminders (`launchd`/`systemd`), never an in-app daemon.
- Import/export helpers (CSV, old `data.json`) and a monthly HTML report.
- Windows CI.

Not planned: accounts, sync, telemetry, plugins, a server.

## License

[MIT](LICENSE) © 2026 Mahendra Basala
