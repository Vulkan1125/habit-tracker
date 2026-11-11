# Changelog

All notable changes to `habit` are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project uses
[semantic versioning](https://semver.org/).

## [Unreleased]

Nothing yet.

## [0.11.0] — 2026-11-11

### Added

- **Reports you can repaint from inside the file.** Every monthly HTML report now carries all seven
  palettes. It opens in the theme from `config.toml`, and changing `data-palette` on the `<html>`
  element — or editing the hex values in the `:root` block — restyles the page without regenerating
  it.

### Changed

- **Monthly reports use the app's palette** instead of fixed warm colours. The chart takes the
  theme accent, status cells take their text colour from the page background, and `terminal` and
  `mono` keep worked-out neutral and grey variants.

## [0.10.0] — 2026-11-11

### Added

- **`habit uninstall`.** A single command that removes the data file and its backups, installed
  modules, generated reports, the config file, scheduled reminder and report jobs, and the
  `habit`, `habit-notify` and `habit-report` binaries. It prints the list and asks you to type
  `yes`; `--force` skips the question and `--dry-run` only lists. Homebrew and Cargo build copies
  leave their binaries to the tool that owns them.
- **A performance chart in the monthly report.** Each report now opens with a month-wide SVG line
  chart of daily completion and its seven-day rolling average. It needs no JavaScript: hovering a
  day reveals its numbers, and a dot and tooltip mark days that have a note.
- **Notes and descriptions in the report.** Every habit card lists its dated notes for the month,
  and its description sits under the title. Each habit also gets its own completion score.

### Changed

- **The monthly report is redesigned** with a hero and completion ring, a summary strip, a chart
  card, cleaner habit cards and light, dark and print styles.
- **The installer is quieter.** `rustup`, `cargo install` and `apt-get`/`dnf`/`yum`/`pacman` now run
  with their quiet flags, so a successful install no longer prints pages of compiler and package
  manager output; the useful “installing Rust”, “compiling” and “rust ready” lines remain.

## [0.9.1] — 2026-11-11

### Fixed

- Typing a lone `#` in the TUI filter now behaves like a half-typed tag filter and keeps every
  habit visible, instead of emptying the list until a letter follows.
- A CSV import no longer replaces a dated note that already exists locally; imports only fill in
  missing history, matching the additive promise for check-ins and cheat days.

## [0.9.0] — 2026-11-11

### Added

- **Weekly quota chart.** Quota habits (`--weekly N`) now show their last four weeks in the detail
  pane, one row per week, e.g. `Oct 06  ████░░░░  3/4`. Complete weeks turn green, the current week
  stays open, and the chart only appears once there are at least two weeks of history.
- **Either/or tag filtering.** A single filter token can offer alternatives: `#health|#fitness` in
  the TUI filter, and `habit list --tag health|fitness`. Separate tokens and separate `--tag`
  arguments still mean AND, so `#health|#fitness #morning` narrows to morning habits in either tag.
- **CSV export and import.** `habit export [--output FILE]` writes every habit, its id, schedule,
  tags, description, reminder, archive state and complete history (check-ins, cheat days and dated
  notes) as CSV. `habit import FILE` merges that file back additively: known ids gain only the
  missing events, unknown ids keep their exported id, and re-importing the same file changes
  nothing. Standard quoting handles commas, quotes and newlines.

### Changed

- `habit list --tag` accepts `|` inside one argument for OR, while repeating `--tag` still requires
  every tag. The help overlay and README describe the new syntax.

## [0.8.0] — 2026-11-11

### Added

- **Optional feature modules.** The core app is now just check-ins, streaks, tags and the TUI.
  Notifications and reports live in two companion binaries, `habit-notify` and `habit-report`, and
  stay dormant until you install them:

  ```sh
  habit modules list              # what is installed, what is available
  habit modules install notify    # reminder times and desktop notifications
  habit modules install report    # monthly HTML pages
  habit modules remove notify     # stop the feature and its scheduled jobs
  ```

  Modules are copied into `<data dir>/modules`, so a data file carries its own feature set.
  Installing uses the copy shipped beside `habit` (release tarball, Homebrew, `cargo install`) and
  never touches the network. `HABIT_MODULE_SOURCE` can point at another directory.
- **A modules screen in the TUI.** `M` lists every module with its status; `enter` or `space`
  installs or removes the highlighted one, and `esc` closes it. `habit modules` with no action is
  the same list on the command line.
- **Automatic migration for existing data.** On the first run after upgrading, reminders that are
  already saved install the notifications module and an existing `reports/` directory installs the
  reports module, so an upgrade never silently turns a feature off. An explicit
  `habit modules remove` writes a marker so the next run does not bring it back.
- **A smarter `install.sh`.** It detects whether Rust is installed and, when a build from source is
  needed, offers to set Rust up with rustup. It installs all three binaries, supports macOS's
  universal build as well as the Linux packages and static build, and at the end asks whether to
  add the install directory to your shell's `PATH`. New flags: `--from-source` and `--no-path`.

### Changed

- `habit remind`, `habit remind-time`, `habit schedule install` and `habit report` now require
  their module; when it is missing they name the exact `habit modules install …` command instead of
  failing obscurely. `habit schedule reports install` needs the reports module.
- Without the notifications module the add/edit dialog has no **Remind** row and the detail pane
  hides a saved reminder; editing a habit keeps the time so reinstalling the module brings it back.
  Without the reports module there is no report command at all — the core app stays lean.
- **Starting later** is now separated from the habits running today by a blank line, so a group of
  not-yet-started habits no longer runs into the active ones.
- Release archives, `.deb`/`.rpm`/Arch packages and the Homebrew formula now ship `habit`,
  `habit-notify` and `habit-report` together.

## [0.7.0] — 2026-11-11

### Added

- **Per-habit reminder times.** `Habit.reminder` holds an optional local time, set in the add/edit
  dialog's **Remind** field or with `habit add --remind HH:MM` and
  `habit remind-time <HABIT> [HH:MM] [--clear]`. Reminders stay off until a habit gets a time, and
  `habit schedule install` now writes one OS job per distinct time instead of one global job, with
  `habit remind --at HH:MM` narrowing a run to the habits due at that time. Re-installing syncs:
  units for times no habit uses any more are removed, along with the single-unit names from 0.6.0.
- **A first-run notification setup prompt.** Enabling a reminder for the first time explains the
  one-time setup in the TUI and offers to install it; choosing **later** keeps the time saved but
  silent, so nothing is scheduled without an explicit yes. macOS's own permission prompt is called
  out there too.
- **Monthly HTML reports.** `habit report` writes one self-contained page per month to
  `<data dir>/reports/habit-YYYY-MM.html`, with `--month YYYY-MM`, `--last-month`, `--output FILE`,
  `--open` and `--notify`. Each habit gets its completion rate, quota weeks met, streak, excused
  days and a day-by-day calendar in the TUI's status colours; the page carries inline CSS, no
  scripts and no network, and prints cleanly.
- **Scheduled reports.** `habit schedule reports install [--day 1-28] [--at HH:MM]` runs
  `habit report --last-month --notify` on the chosen day (the 1st by default), with `status` and
  `remove` alongside; days 1–28 are enforced so every month has the date.

### Changed

- The on-disk format is now **version 4**. The additive `reminder` field bumps `DATA_VERSION`, so a
  0.6.0 build refuses a file written here instead of dropping the times on its next save. Version
  1–3 files load with reminders off and are stamped as version 4 on the next save.
- `habit schedule install` no longer takes `--at`: the times come from the habits themselves.
- The add/edit dialog has a **Remind** field between the end date and the tags, and the detail pane
  shows the time beside the schedule.

## [0.6.0] — 2026-11-11

### Added

- **An Archive tab.** `tab` now cycles Habits → Tags → Archive, and the Archive tab collects closed
  habits under **Archived** and **Ended**. `d` restores an archived habit or files an ended one
  away, so the Habits tab stays about what is running. The tab is clickable, like the others, and
  `esc` backs out to Habits.
- **Habits that start later have their own heading.** A habit created with a future start date no
  longer inflates today's list: it sits under **Starting later** in the Habits tab, is skipped by
  the day's due count and by tab completion, and still appears under its tags.
- **Native reminders, without a daemon.** `habit remind` sends one desktop notification with the
  habits still open today (quota habits only while their week is short; done, excused and finished
  days are skipped). `habit schedule install --at HH:MM` hands a daily run to launchd on macOS or a
  systemd user timer on Linux, with `remove` and `status` alongside; `--dry-run` prints the unit
  file or notification text instead. The scheduler is opt-in and nothing runs in the background
  otherwise.

### Changed

- The Habits tab's tab-bar count now matches its list: archived, finished and future-start habits
  are left out.

## [0.5.0] — 2026-11-10

### Added

- **Tags can be renamed and merged.** On the Tags tab `e` opens a prompt for the tag under the
  cursor; the new name is written onto every habit that carries it, archived ones included. If the
  name is already in use the dialog asks first, because that folds two tags into one — so `#health`
  and `#body` can become a single tag without editing habits one by one. `esc` cancels, `u` undoes,
  and the cursor stays on the renamed tag.
- `habit rename-tag <OLD> <NEW>` does the same from the shell (and merges when `NEW` exists), while
  `habit merge-tags <FROM> <INTO>` is the careful spelling: it refuses a target that is not already
  a tag, so a typo cannot quietly create a new one.

### Changed

- `u` (undo) now works on the Tags tab too, where it used to be swallowed by the tag list.

## [0.4.1] — 2026-11-10

### Fixed

- **The selected row looked different in the `terminal` theme.** Palettes that paint a background
  soften the cursor row to one text colour, but `terminal`, `mono` and any 256/16-colour fallback
  invert it instead and were leaving each span's own colour alone — so the habit's status dot, the
  miss and cheat marks and the week dots stayed green, amber and red on the selected row while
  every other theme showed them in the row colour.

## [0.4.0] — 2026-11-10

### Added

- **The note dialog takes a date.** `n` opens it on the day you are looking at, and `tab` moves
  into a `YYYY-MM-DD` date field: `↑` / `↓` step one day, or type any date. Dates in the future and
  dates before the habit's start are refused with an inline message while the dialog stays open.

### Changed

- Dated notes are always listed **newest first** in the detail pane, so the latest entry stays at
  the top.

### Packaging

- A `release packages` workflow now builds and attaches a Debian `.deb`, Fedora `.rpm`, Arch
  `.pkg.tar.zst`, universal macOS `.tar.gz`, static Linux `.tar.gz` and `SHA256SUMS` to each
  published GitHub release. It also runs on demand for an existing tag.
- **A one-line Linux installer.** `install.sh` (also published as a release asset) resolves the
  latest tag, downloads the right asset, verifies it against the release's `SHA256SUMS` and then
  installs the native `.deb` / `.rpm` / `.pkg.tar.zst` with `apt-get`, `dnf` or `pacman`, falling
  back to the static binary in `~/.local/bin` on other distributions or without root. `--version`,
  `--package`, `--static`, `--prefix`, `--dry-run` and `--yes` control it.
- **A Homebrew tap** at [Vulkan1125/homebrew-tap](https://github.com/Vulkan1125/homebrew-tap):
  `brew install Vulkan1125/tap/habit` installs the universal macOS binary, so nothing is compiled.
- CI runs `shellcheck` on the installer, and every release attaches `install.sh` alongside the
  packages, with `SHA256SUMS` covering it too.

## [0.3.3] — 2026-11-10

### Changed

- The detail pane's `daily · since …` schedule line and tag chips are now indented to match the
  description text, so the title is followed by one visually aligned block. Streak, heatmap, notes
  and everything below are unchanged.

## [0.3.2] — 2026-11-10

### Added

- Habits with an **end date** now show the remaining days in the first pane: `51d left`,
  `last day` on the final day, or `ended` once the window closes. The countdown turns amber in the
  last three days and replaces the schedule label, so closing habits stand out without opening the
  detail pane.

### Changed

- The detail pane renders `description:` on its own line with up to three wrapped description lines
  indented beneath it. Streak, heatmap, notes and everything below are unchanged.

## [0.3.1] — 2026-11-10

### Added

- **`b` hides the bottom bar**, giving the panes one more row; pressing it again restores the
  hint line. The toggle works from the habit list, the tag list and a tag view.

### Changed

- The habit **description** now sits directly under the title in the detail pane instead of at the
  bottom, followed by the schedule, tags, streak, heatmap and dated notes.

## [0.3.0] — 2026-11-10

The tag-and-journal release: a second left-pane tab for tags, dated notes on every day, and a
clearer name for the per-habit free text.

### Added

- **Tags tab**: `tab` switches the left pane between Habits and Tags, and both tabs are clickable
  with the mouse. The tag list shows every tag with its count of active habits before you open it.
  Opening a tag lists its active habits, then archived and finished ones under an **Archive**
  heading. Clicking the active Tags tab or the `‹ all tags` row goes back.
- **Dated notes**: press `n` on any past or present day to write a note for that day. Notes render
  in the detail pane newest first, date on the left and text on the right; empty input removes the
  note. Marking a habit done flashes `n adds a note`, and saving a note also feeds undo.
- **Mouse support** in the TUI: click tabs, tags, the back row and habit rows, and scroll the list.

### Changed

- The per-habit free text is now called the **description**, so it is not confused with the new
  dated notes. The dialog label and detail-pane heading say “Description”, the CLI uses
  `habit add --description` and `habit describe` — `--note` and `habit note` remain as aliases.
- **Data format v3**: habits gain a `notes` map of date to entry. Older files load with the field
  defaulted and are written back as v3; the existing `note` field is unchanged on disk.
- The detail pane and list are laid out around the tab bar; borders stay square.

### Removed

- The trend line in the detail pane, along with `stats::trend`, `theme::spark` and their tests.

### Tests

- Test count is now **129**, covering tag counts, the archive split, mouse hit-testing, note
  round-trips, `esc` backing out of a tag view and the rename.

## [0.2.1] — 2026-11-10

### Added

- **Theme shortcuts in the TUI**: `T` steps to the next palette and `m` toggles dark/light. The
  choice is written back to `config.toml`, so it survives a restart, and both keys are listed in
  the help overlay. Previously themes could only be changed by editing the config file.

### Changed

- Test count is now **119**; README, help overlay and docs cover the new keys.

### Fixed

- `cargo install --path .` is the supported way to update an existing install; the 0.2.0 notes did
  not mention that an older binary on `PATH` keeps showing the old UI.

## [0.2.0] — 2026-11-10

The feature release: habits gain a lifetime, cheat days, tags, notes, a trend line, date
navigation and a full visual pass with seven palettes in light and dark mode.

### Added

- **Start and end dates**: `add --start/--end`, plus `habit start` and `habit end [--date] [--clear]`.
  Days outside the window are not due, finished habits sort with archived ones, and stats freeze on
  the final day instead of decaying. Both dates are editable in the TUI dialog.
- **Cheat days**: `habit cheat <habit> [--date] [--off]` and `c` in the TUI. The first cheat per
  habit per calendar month is excused (skipped by the streak, `◆`, its own colour); later ones in
  the same month are ordinary misses. Cheats are excluded from completion rates and do not fill
  weekly quotas. Backfilling an earlier cheat recalculates which one is excused and warns about it.
- **Tags**: normalised lowercase kebab-case tags with `add --tag`, `habit tag <habit> TAG… [--remove]`,
  `habit tags`, and `habit list --tag`. The TUI shows tag chips, matches tags in `/`, and treats
  `#tag` as a tag-only filter. Multiple tokens are ANDed.
- **Notes**: `habit add --note` and `habit note <habit> [TEXT] [--clear]`; notes are editable in the
  TUI, wrapped in the detail pane and included in filter matching.
- **TUI date navigation**: `[` `]` move the selected day, `t` returns to today, and `space` / `c`
  act on the selected day. Backfilling no longer requires the CLI. Future days are refused.
- **Trend graph**: a rolling 7-day completion sparkline in the detail pane, alongside the existing
  heatmap (now up to 52 weeks and adaptive to the pane width).
- **Seven themes with light and dark variants**: `terminal`, `rosepine`, `catppuccin`, `tokyonight`,
  `gruvbox`, `evergreen` and `mono`. Truecolour is detected from `COLORTERM`/`TERM` and downgraded
  to the 256-colour cube or the nearest basic 16 colours. Per-habit accents are hashed from the id.
  `habit themes` lists the palettes.
- **Visual pass**: new header with date, viewed-day marker and progress bar; selection surface;
  per-habit accent in the detail pane; responsive footer hints, legend and form helper; `◌` marks
  today while it is still open instead of showing a miss.
- **Format upgrade tests** for loading a version-1 file and saving it as version 2.

### Changed

- **Data format is now version 2.** `Habit` gains `ends` and `tags`; `Data` gains a `cheats` map.
  A version-1 file loads with the new fields defaulted and is stamped version 2 on the next save.
  Files written by a newer format are still refused.
- **Configuration**: `theme` now names a palette and a separate `mode = "dark"|"light"` selects the
  variant. The 0.1.x `theme = "dark"` / `"light"` spellings still work and map to the `terminal`
  palette with that mode. `mono` remains available.
- **`habit list`** also shows finished habits and filters by tags; **`habit stats`** reports the
  window, tags, monthly cheat usage and window-aware completion; **`habit log`** marks extra cheats
  that broke the streak.
- **TUI form** now has start, end, tags and note fields after the schedule fields.
- **Streak scanning** is anchored to the habit's end date, so a finished habit keeps its final
  streak and final-week progress.
- Test count is now **115** (from 58). The release binary is about 1.8 MB.

### Fixed

- A habit that ends mid-week no longer shows the current (empty) week instead of the week it
  finished in.
- A second cheat in a month is reported as breaking the streak instead of silently looking excused.
- An unchanged tag is reported as such rather than pretending it was added.
- The TUI filter no longer treats `#` tokens as literal name text.

## [0.1.0] — 2026-11-10

First release.

### Added

- **TUI** (`habit` with no arguments): habit list with today's marker and a 7-day dot strip,
  detail pane with current/best streak, weekly progress, 30-day completion and a 9-week heatmap.
- **CLI** with full parity: `add`, `done`, `undo`, `list`, `stats`, `log`, `archive`, `remove`,
  `path`, `config`.
- **Three schedule types**: daily, specific weekdays, and `n` times per week.
- **Streak engine** that skips non-due days, keeps today in progress, counts quota habits by
  complete weeks, and ignores days before a habit existed.
- **Add/edit dialog** with schedule picker, weekday toggles and inline validation.
- **Undo** of the last change, including deletion.
- **Archive** as a reversible alternative to deletion.
- **Filter**, **help overlay**, **manual reload**, and a **status line** that reports every action.
- **Atomic JSON storage** with `fsync` + rename, `data.json.bak` backups, a format version guard,
  and rollback of in-memory state when a save fails.
- **Configuration** at `~/.config/habit/config.toml`: `theme` (dark/light/mono) and `week_start`.
- **Global flags** `--data <FILE>` and `--today <DATE>`, plus `HABITS_FILE`.
- **58 tests** covering streaks, quota weeks, schedule compliance, store round-trips, atomic-save
  failure modes, form validation and TUI rendering via `TestBackend`.
- CI on Linux and macOS (format, clippy, tests, MSRV 1.88) and a release profile with LTO + strip.

[Unreleased]: https://github.com/Vulkan1125/habit-tracker/compare/v0.5.0...HEAD
[0.5.0]: https://github.com/Vulkan1125/habit-tracker/releases/tag/v0.5.0
[0.4.1]: https://github.com/Vulkan1125/habit-tracker/releases/tag/v0.4.1
[0.4.0]: https://github.com/Vulkan1125/habit-tracker/releases/tag/v0.4.0
[0.3.3]: https://github.com/Vulkan1125/habit-tracker/releases/tag/v0.3.3
[0.3.2]: https://github.com/Vulkan1125/habit-tracker/releases/tag/v0.3.2
[0.3.1]: https://github.com/Vulkan1125/habit-tracker/releases/tag/v0.3.1
[0.3.0]: https://github.com/Vulkan1125/habit-tracker/releases/tag/v0.3.0
[0.2.1]: https://github.com/Vulkan1125/habit-tracker/releases/tag/v0.2.1
[0.2.0]: https://github.com/Vulkan1125/habit-tracker/releases/tag/v0.2.0
[0.1.0]: https://github.com/Vulkan1125/habit-tracker/releases/tag/v0.1.0
