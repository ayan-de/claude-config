<!-- BEGIN:nextjs-agent-rules -->
# This is NOT the Next.js you know

This version has breaking changes — APIs, conventions, and file structure may all differ from your training data. Read the relevant guide in `node_modules/next/dist/docs/` before writing any code. Heed deprecation notices.
<!-- END:nextjs-agent-rules -->

# Project: Claude Config

Tauri 2 desktop app (Next.js 16 frontend, Rust backend) that manages Claude Code
provider profiles. The README has the full architecture and provider data model
— read it for the "why" before changing things.

# Commands

The frontend and the desktop shell are coupled. Use the Tauri-wrapped scripts:

- Dev (Next.js on :3000 + Tauri window): `pnpm tauri dev`
- Production build (platform-native installer in `src-tauri/target/release/bundle/`): `pnpm tauri build`
- Plain `pnpm dev` only runs the Next.js dev server with no Tauri host.

Lint, typecheck, and tests run independently:

```bash
pnpm lint                          # ESLint (config: eslint.config.mjs)
pnpm exec tsc --noEmit             # TypeScript typecheck
cd src-tauri && cargo test         # Rust unit tests (includes 6 merge.rs tests)
cd src-tauri && cargo test -- --ignored keyring   # exercises real OS keyring
```

There is no JS/TS unit-test runner — only Rust tests + lint + typecheck.

# Frontend architecture (`src/`)

- `src/app/page.tsx` — single page, 2-pane shell, **owns all app state**. The
  initial provider/keyring load runs in a `useEffect` with
  `set-state-in-effect` intentionally disabled (see file header).
- `src/lib/api.ts` — typed wrappers around `invoke()`. **All component code
  must go through this** — never call `invoke()` directly, never hardcode
  command names.
- `src/lib/types.ts` — TypeScript mirror of the Rust `Provider`/`AppError`
  types. **Keep in sync with `src-tauri/src/models.rs`.** The auth token is
  deliberately absent from the `Provider` struct returned to the UI.
- `src/lib/utils-app.ts` — `maskToken()` shows first 6 + … + last 4. The Rust
  side never returns the full token; preserve this contract.
- `isWebEnv()` checks `__TAURI_INTERNALS__` in `window`. Outside Tauri the UI
  renders a "Run inside Tauri" stub — don't try to make web-only behavior work.
- `components.json` declares `style: "base-nova"` — this is **base-ui**, not
  Radix. Don't import Radix primitives; the shadcn components in
  `src/components/ui/` are already base-ui based.
- Tailwind v4 via `@tailwindcss/postcss` (see `postcss.config.mjs`). There is
  **no `tailwind.config.js`** — config lives in `src/app/globals.css` and
  `components.json`.

# Backend architecture (`src-tauri/`)

- `src/lib.rs` — Tauri builder + first-launch auto-import (captures the
  existing `settings.json` env as an "Imported" provider).
- `src/models.rs` — `Provider`, `AppError`, and `CANONICAL_ENV_KEYS` (the
  exact 9 keys the merge logic knows about). The `len() == 9` test fails if
  someone edits the list carelessly.
- `src/merge.rs` — **single source of truth** for what `load_provider` writes
  to `settings.json.env`. Pure functions, no I/O, exhaustively unit-tested.
  Provider-authoritative semantics: canonical keys absent from the provider
  are *removed*, unknown keys in existing settings are *preserved*. Don't
  reimplement merge logic elsewhere.
- `src/storage/settings.rs` — atomic write (tempfile + fsync + rename) under
  `lock_exclusive` across the read-modify-write. **This is the only place
  `settings.json` is mutated**; the rest of the file (hooks,
  enabledPlugins, `model`, etc.) is preserved verbatim.
- `src/storage/keyring.rs` — OS keyring access. Auth tokens are **never**
  written to disk in plaintext. The UI gates "New provider" on
  `keyringAvailable` — the backend should also refuse saves when keyring is
  unavailable.

# Conventions and gotchas

- App identifier is `com.claudeconfig.app` (see `src-tauri/tauri.conf.json`).
  App-data dir is `<app-data>/com.claudeconfig.app/` (Linux:
  `~/.local/share/...`).
- `next.config.ts` uses `output: "export"` and `trailingSlash: true` — this
  is a static SPA loaded by Tauri. No SSR, no API routes, no `getServerSideProps`.
- `next-env.d.ts` is **gitignored and regenerated** by Next.js — don't edit
  it. ESLint already skips it.
- ESLint extends `eslint-config-next` and **adds `src-tauri/target/` to
  ignores** — Tauri generates JS into that path during builds.
- Not a pnpm workspace. `ignoredBuiltDependencies` (`sharp`, `unrs-resolver`)
  lives in `package.json` under the `pnpm` key.
- Honored env: `CLAUDE_CONFIG_DIR` (custom Claude Code config path) and
  `TAURI_DEV_HOST` (mobile dev). See `next.config.ts`.
- Arch Linux has no native `.pkg.tar` target — README explains the
  AppImage/AUR workaround.

# Storage layout (recap)

| What | Where |
|---|---|
| Saved provider metadata | `<app-data>/providers.json` |
| Auth tokens | OS keyring, service `claude-config` |
| settings.json backups | `<app-data>/backups/<unix-ms>.json` |
| `~/.claude/settings.json` | Claude Code's own file — only its `env` block is touched |