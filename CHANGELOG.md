# Changelog

All notable changes to `habit` are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project uses
[semantic versioning](https://semver.org/).

## [Unreleased]

Nothing yet.

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
- Release profile with LTO + strip (1.7 MB binary).

[Unreleased]: https://github.com/Vulkan1125/habit-tracker/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/Vulkan1125/habit-tracker/releases/tag/v0.1.0
