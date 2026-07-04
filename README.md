# Claude Config

Cross-platform desktop app for managing Claude Code provider profiles.

Switch between Anthropic, GLM, Kimi, DeepSeek, Minimax, self-hosted, or any
other OpenAI-compatible provider with one click — instead of hand-editing
`~/.claude/settings.json`'s `env` block every time.

## What it does

- **Add unlimited provider profiles** (name + base URL + auth token + optional
  model fields).
- **Load** a profile into `~/.claude/settings.json` with one click.
- **Preserves** all other `settings.json` keys (hooks, enabledPlugins,
  extraKnownMarketplaces, `model`, etc.) — only the `env` block is touched.
- **Stores auth tokens** in the OS keyring (macOS Keychain / GNOME libsecret
  / Windows Credential Manager). Never written to disk in plaintext.
- **Backs up** `settings.json` before every write to
  `<app-data>/backups/<unix-ms>.json`.
- **Honors `CLAUDE_CONFIG_DIR`** — supports custom Claude Code config paths.
- **First-launch auto-import** — captures the existing `settings.json` env
  block as an "Imported" provider so you start with one provider ready.

## Stack

- **Tauri 2** desktop shell — small (~5 MB) cross-platform binaries
- **Next.js 16** static export (no SSR; pure HTML/CSS/JS loaded by Tauri)
- **Tailwind v4** + **shadcn/ui** (base-ui based)
- **Rust** backend for file I/O, atomic writes, keyring access

## Develop

```bash
pnpm install
pnpm tauri dev
```

The dev command launches the Next.js dev server on port 3000 and opens a
Tauri window pointing at it.

## Build (production bundles)

```bash
pnpm tauri build
```

Produces platform-native installers in `src-tauri/target/release/bundle/`:

- **Linux:** `.deb`, `.rpm` (and `.AppImage` if `linuxdeploy` is installed)
- **macOS:** `.app`, `.dmg`
- **Windows:** `.msi` (and `.nsis` if enabled)

**Arch Linux:** Tauri has no native `pacman` target. Either:
1. Install the `.AppImage` directly, or
2. Maintain an AUR `PKGBUILD` that wraps the `.deb`/`.AppImage`.

## Test

```bash
# JS/TS lint
pnpm lint

# TypeScript typecheck
pnpm exec tsc --noEmit

# Rust unit tests (25 tests including 6 merge tests)
cd src-tauri && cargo test

# Keyring integration test (exercises real OS keyring)
cd src-tauri && cargo test -- --ignored keyring
```

## Storage layout

| What | Where |
|---|---|
| Saved providers metadata | `<app-data>/providers.json` |
| Auth tokens | OS keyring, service `claude-config` |
| Settings.json backups | `<app-data>/backups/<unix-ms>.json` |
| **`~/.claude/settings.json`** | **Claude Code's own file, env block only** |

`<app-data>` is platform-correct:
- Linux: `~/.local/share/com.claudeconfig.app/`
- macOS: `~/Library/Application Support/com.claudeconfig.app/`
- Windows: `%APPDATA%\com.claudeconfig.app\`

## Provider data model

```ts
interface Provider {
  id: string;                              // uuid
  name: string;                            // user-facing, unique
  baseUrl: string;                         // → ANTHROPIC_BASE_URL
  model?: string;                          // → ANTHROPIC_MODEL
  smallFastModel?: string;                 // → ANTHROPIC_SMALL_FAST_MODEL
  defaultSonnetModel?: string;             // → ANTHROPIC_DEFAULT_SONNET_MODEL
  defaultOpusModel?: string;               // → ANTHROPIC_DEFAULT_OPUS_MODEL
  defaultHaikuModel?: string;              // → ANTHROPIC_DEFAULT_HAIKU_MODEL
  apiTimeoutMs?: number;                   // → API_TIMEOUT_MS (no ANTHROPIC_ prefix)
  disableNonessentialTraffic?: boolean;    // → CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC
  createdAt: string;
  updatedAt: string;
}
// auth token is NOT in the struct — it's in the OS keyring.
```

## Architecture

```
src/                       # Next.js frontend
├── app/
│   ├── layout.tsx         # dark theme + Toaster + TooltipProvider
│   ├── page.tsx           # 2-pane shell, all state
│   └── globals.css        # Tailwind v4 + shadcn tokens
├── components/            # UI
└── lib/                   # types + typed IPC wrappers

src-tauri/                 # Rust backend
├── src/
│   ├── lib.rs             # command registry + first-launch import
│   ├── models.rs          # Provider, AppError, types
│   ├── merge.rs           # pure env merge logic (6 unit tests)
│   ├── state.rs           # shared AppState (keyring + paths)
│   ├── commands/          # 17 #[tauri::command] functions
│   └── storage/           # keyring, providers.json, settings.json
├── Cargo.toml
├── tauri.conf.json
└── capabilities/default.json
```

## Merge semantics

When loading a provider, the `env` block in `settings.json` is rewritten with
**provider-authoritative semantics**:

1. Every canonical key the provider defines is set.
2. Canonical keys absent from the provider are **removed** (no stale keys
   accumulate across loads).
3. Unknown keys in existing settings.json are **preserved** (don't delete
   user-authored additions).
4. Atomic write: tempfile + fsync + rename, with `lock_exclusive` held
   across the read-modify-write.

This is the only place settings.json is mutated. Everything else is
preserved verbatim.

## License

MIT