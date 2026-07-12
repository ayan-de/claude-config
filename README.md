<p align="center">
  <img src="src-tauri/icons/128x128.png" alt="Claude Config" width="128" height="128" />
</p>

<h1 align="center">Claude Config</h1>

<p align="center">
  Cross-platform desktop app for managing Claude Code provider profiles.
</p>

<p align="center">
  <a href="https://github.com/ayan-de/claude-config/releases/latest"><img alt="Latest release" src="https://img.shields.io/github/v/release/ayan-de/claude-config?display_name=tag&sort=semver"></a>
  <a href="#license"><img alt="License" src="https://img.shields.io/badge/license-MIT-blue.svg"></a>
  <img alt="Platforms" src="https://img.shields.io/badge/platforms-Linux%20%7C%20macOS%20%7C%20Windows-lightgrey">
</p>

---

Switch between Anthropic, GLM, Kimi, DeepSeek, Minimax, self-hosted, or any
other OpenAI-compatible provider with one click — instead of hand-editing
`~/.claude/settings.json`'s `env` block every time.

## Features

- **Unlimited provider profiles** — name, base URL, auth token, and optional model overrides.
- **One-click load** — writes the selected profile into `~/.claude/settings.json`.
- **Non-destructive** — only the `env` block is touched. Hooks, `enabledPlugins`, `extraKnownMarketplaces`, `model`, and everything else are preserved verbatim.
- **Secure token storage** — auth tokens live in the OS keyring (macOS Keychain / GNOME libsecret / Windows Credential Manager). Never on disk in plaintext.
- **Automatic backups** — `settings.json` is snapshotted to `<app-data>/backups/<unix-ms>.json` before every write.
- **Honors `CLAUDE_CONFIG_DIR`** for custom Claude Code config paths.
- **First-launch auto-import** — captures your existing `env` block as an "Imported" provider so you start with one entry ready.

## Install

### Linux

| Format | Command |
|---|---|
| **`.deb`** (Ubuntu, Debian, Pop!\_OS, Mint) | `sudo dpkg -i claude-config_<version>_amd64.deb` |
| **`.rpm`** (Fedora, RHEL, openSUSE) | `sudo rpm -i claude-config-<version>-1.x86_64.rpm` |
| **`.AppImage`** (any distro, incl. Arch) | `chmod +x Claude-Config_<version>_amd64.AppImage && ./Claude-Config_<version>_amd64.AppImage` |

Grab the appropriate file from the [latest release](https://github.com/ayan-de/claude-config/releases/latest).

**Arch Linux:** use the `.AppImage`. Tauri has no native `pacman` target, so a PKGBUILD (planned for the AUR) would wrap the AppImage or `.deb`.

**Linux keyring requirement:** the app needs a running Secret Service — GNOME Keyring, KWallet with secret-service, or KeePassXC's secret-service integration. Without one, save/load operations will fail.

### macOS

1. Download `Claude-Config_<version>_x64.dmg` (Intel) or `_aarch64.dmg` (Apple Silicon) from the [latest release](https://github.com/ayan-de/claude-config/releases/latest).
2. Open the DMG and drag **Claude Config** into `/Applications`.
3. First launch: right-click the app → **Open** → confirm. (Required until the app is notarized — see [Shipping](#shipping) below.)

### Windows

1. Download `Claude-Config_<version>_x64-setup.exe` (NSIS) or `Claude-Config_<version>_x64_en-US.msi` (MSI) from the [latest release](https://github.com/ayan-de/claude-config/releases/latest).
2. Run the installer.
3. WebView2 Runtime is required — the NSIS installer bundles it; the MSI expects it preinstalled (present on Windows 11 and up-to-date Windows 10).

Until the binary is signed with an Authenticode certificate, SmartScreen will show a warning — click **More info → Run anyway**.

## Storage layout

| What | Where |
|---|---|
| Saved provider metadata | `<app-data>/providers.json` |
| Auth tokens | OS keyring, service `claude-config` |
| `settings.json` backups | `<app-data>/backups/<unix-ms>.json` |
| **`~/.claude/settings.json`** | Claude Code's own file — only the `env` block is modified |

`<app-data>`:

- Linux: `~/.local/share/com.claudeconfig.app/`
- macOS: `~/Library/Application Support/com.claudeconfig.app/`
- Windows: `%APPDATA%\com.claudeconfig.app\`

## Provider data model

```ts
interface Provider {
  id: string;                              // uuid
  name: string;                            // unique, user-facing
  baseUrl: string;                         // → ANTHROPIC_BASE_URL
  model?: string;                          // → ANTHROPIC_MODEL
  smallFastModel?: string;                 // → ANTHROPIC_SMALL_FAST_MODEL
  defaultSonnetModel?: string;             // → ANTHROPIC_DEFAULT_SONNET_MODEL
  defaultOpusModel?: string;               // → ANTHROPIC_DEFAULT_OPUS_MODEL
  defaultHaikuModel?: string;              // → ANTHROPIC_DEFAULT_HAIKU_MODEL
  apiTimeoutMs?: number;                   // → API_TIMEOUT_MS
  disableNonessentialTraffic?: boolean;    // → CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC
  createdAt: string;
  updatedAt: string;
}
// The auth token is NOT in this struct — it lives in the OS keyring.
```

## Merge semantics

When a provider is loaded, the `env` block in `settings.json` is rewritten with **provider-authoritative** semantics:

1. Every canonical key defined by the provider is set.
2. Canonical keys the provider omits are **removed** — no stale keys accumulate across loads.
3. Unknown keys already in `settings.json` are **preserved** — user-authored additions survive.
4. Atomic write: tempfile + `fsync` + rename, with `lock_exclusive` held across the read-modify-write.

`src-tauri/src/merge.rs` is the single source of truth. It is pure, has no I/O, and is exhaustively unit-tested.

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

The dev command launches the Next.js dev server on port 42713 and opens a Tauri window pointing at it.

## Build from source

```bash
pnpm tauri build
```

Produces platform-native installers in `src-tauri/target/release/bundle/`:

- **Linux:** `.deb`, `.rpm` (plus `.AppImage` if `linuxdeploy` is on `PATH`)
- **macOS:** `.app`, `.dmg`
- **Windows:** `.msi` (WiX) and `.exe` (NSIS)

Tauri does **not** cross-compile installers — each target OS builds on its own host (or CI runner).

## Test

```bash
pnpm lint                              # ESLint
pnpm exec tsc --noEmit                 # TypeScript typecheck
cd src-tauri && cargo test             # Rust unit tests (incl. 6 merge tests)
cd src-tauri && cargo test -- --ignored keyring   # real OS keyring
```

There is no JS/TS unit-test runner — Rust tests + lint + typecheck only.

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
│   ├── models.rs          # Provider, AppError, canonical env keys
│   ├── merge.rs           # pure env merge logic (6 unit tests)
│   ├── state.rs           # shared AppState (keyring + paths)
│   ├── commands/          # #[tauri::command] functions
│   └── storage/           # keyring, providers.json, settings.json
├── Cargo.toml
├── tauri.conf.json
└── capabilities/default.json
```

## Shipping

Releases are published to GitHub Releases from a matrix CI build. Contributors don't need signing keys — the workflow builds unsigned artifacts for every push tag; signed macOS notarization and Windows Authenticode are optional overlays gated on repo secrets.

## License

MIT
