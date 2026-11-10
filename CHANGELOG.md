# Changelog

All notable changes to `habit` are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project uses
[semantic versioning](https://semver.org/).

## [Unreleased]

Nothing yet.

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

[Unreleased]: https://github.com/Vulkan1125/habit-tracker/compare/v0.2.1...HEAD
[0.2.1]: https://github.com/Vulkan1125/habit-tracker/releases/tag/v0.2.1
[0.2.0]: https://github.com/Vulkan1125/habit-tracker/releases/tag/v0.2.0
[0.1.0]: https://github.com/Vulkan1125/habit-tracker/releases/tag/v0.1.0
