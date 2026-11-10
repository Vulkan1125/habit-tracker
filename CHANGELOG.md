# Changelog

All notable changes to `habit` are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project uses
[semantic versioning](https://semver.org/).

## [Unreleased]

### Packaging

- **A one-line Linux installer.** `install.sh` (also published as a release asset) resolves the
  latest tag, downloads the right asset, verifies it against the release's `SHA256SUMS` and then
  installs the native `.deb` / `.rpm` / `.pkg.tar.zst` with `apt-get`, `dnf` or `pacman`, falling
  back to the static binary in `~/.local/bin` on other distributions or without root. `--version`,
  `--package`, `--static`, `--prefix`, `--dry-run` and `--yes` control it.
- **A Homebrew tap** at [Vulkan1125/homebrew-tap](https://github.com/Vulkan1125/homebrew-tap):
  `brew install Vulkan1125/tap/habit` installs the universal macOS binary, so nothing is compiled.
- CI runs `shellcheck` on the installer, and every release now attaches `install.sh` alongside the
  packages, with `SHA256SUMS` covering it too.

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

[Unreleased]: https://github.com/Vulkan1125/habit-tracker/compare/v0.4.0...HEAD
[0.4.0]: https://github.com/Vulkan1125/habit-tracker/releases/tag/v0.4.0
[0.3.3]: https://github.com/Vulkan1125/habit-tracker/releases/tag/v0.3.3
[0.3.2]: https://github.com/Vulkan1125/habit-tracker/releases/tag/v0.3.2
[0.3.1]: https://github.com/Vulkan1125/habit-tracker/releases/tag/v0.3.1
[0.3.0]: https://github.com/Vulkan1125/habit-tracker/releases/tag/v0.3.0
[0.2.1]: https://github.com/Vulkan1125/habit-tracker/releases/tag/v0.2.1
[0.2.0]: https://github.com/Vulkan1125/habit-tracker/releases/tag/v0.2.0
[0.1.0]: https://github.com/Vulkan1125/habit-tracker/releases/tag/v0.1.0
