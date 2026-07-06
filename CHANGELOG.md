# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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