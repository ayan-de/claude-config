# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.7.0] - 2026-07-10

### Added
- **Session management** — delete (OS-Trash via `trash` crate), real titles read from `.jsonl`, in-app transcript viewer, sidebar integration, GitHub sync state per row, accordion grouping by project
- **GitHub sync** — full upload pipeline (`upload_session`, sync-state read), OAuth flow with improved polling, configuration management, context-based state sharing, upload orchestration over Git Data API
- **Sessions UI** — GitHub sync status breakdown in `SessionGroup`, legend in `Sessions.tsx`, upload icon with 4 sync states, project path on each session summary
- **Claude skill** — `.claude/skills/release/` for release workflow

### Changed
- GitHub sync refactored to use `GitHubSyncContext` for shared state
- Theme script handling refactored in `RootLayout` for readability and performance

### Fixed
- Bucket-order rename loss in session title priority (`jsonl.customTitle` / `aiTitle` override index title)

### Removed
- 8-char uuid tag from session list rows (replaced with real title)

## [0.5.8] - 2026-07-07

### Added
- **App version in update notifications** — title bar now shows the current version (`v0.5.8`) next to the settings menu, and surfaces a clickable `Update Now (v0.5.8 → v0.5.9)` button when an update is available
- "You're up to date" toast now includes the running version for confirmation (`v0.5.8`)

### Changed
- Dropdown menu items render with `cursor-pointer` (was `cursor-default`)

## [0.5.7] - 2026-07-07

### Added
- **Dangerous-mode toggle** — Settings → Safety group exposes a global switch that flips `permissions.defaultMode` in `~/.claude/settings.json` between `"bypassPermissions"` and absent, equivalent to passing `--dangerously-skip-permissions` to every Claude Code invocation
  - One-shot confirm dialog gates the first ON, with a carve-out note (`rm -rf /` and `rm -rf ~` still prompt as circuit breakers)
  - First-time acknowledgement is persisted in `localStorage` so subsequent toggles skip the dialog
  - Backend writes reuse `settings::write_settings_atomic` — same sidecar lock and timestamped backup as `load_provider_cmd`, no race between the two writers
  - Unrelated top-level keys in `settings.json` (`env`, `hooks`, `enabledPlugins`, custom keys) are preserved verbatim
- New backend module `src-tauri/src/storage/permissions.rs` with 10 unit tests (7 pure, 3 integration)
- New Tauri commands `get_dangerous_mode_cmd` and `set_dangerous_mode_cmd`
- `useDangerousMode` hook — manages load + optimistic toggle + rollback on write failure
- `DangerousModeConfirm` dialog component
- `Switch` UI primitive wrapping `@base-ui/react/switch`

### Fixed
- Hydration error: `DialogDescription` (renders `<p>` by default) contained nested `<p>` tags; switched to `render={<div />}` to keep the ARIA wiring while making the markup valid

## [0.5.0] - 2026-07-06

### Added
- **CLAUDE.md management**: read, write, and existence-check the project's `CLAUDE.md` file
  - New backend module `src-tauri/src/storage/claude_md.rs` with full unit tests
  - New Tauri commands wired through `src-tauri/src/commands/system.rs`
- **ClaudeMdEditor** component — edit `CLAUDE.md` in-app with save / revert
- **ClaudeMdSidebarButton** — sidebar nav entry for `CLAUDE.md`; icon swaps based on file existence
- **Main** component — orchestrates the main content area (provider forms + editor)
- **Global tabs** system — `src/data/globalTabs.ts` registry + `useGlobalPanel` hook drive sidebar entries for global config files
- **Tips** component and `src/data/tips.ts` — shown inside `EmptyState`
- **New provider** button in the Sidebar
- **Opener plugin** Tauri capability (already shipped in 0.4.0; documented here for completeness)

### Changed
- **Sidebar** layout — added global-config tabs and the new-provider button
- **ProviderList** — supports dynamic sections for providers vs. global config
- **app/page.tsx** — refactored to delegate layout to the new `Main` component (~280 lines lighter)

### Fixed
- ESLint violation in `Tips.tsx`

## [0.4.0] - 2026-07-06

### Added
- Opener plugin Tauri capability for opening external URLs from the app