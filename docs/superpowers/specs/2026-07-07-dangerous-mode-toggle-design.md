# Dangerous-mode toggle — design

Date: 2026-07-07

## Goal

Add a global "Dangerous mode" toggle to the Claude Config desktop app that
flips `permissions.defaultMode` in `~/.claude/settings.json` between
`"bypassPermissions"` and absent, so that every `claude` invocation behaves
as if invoked with `--dangerously-skip-permissions`.

## Background

Claude Code's installer does **not** create a shell alias — it only places
the binary on `PATH` at `~/.local/bin/claude`. The de-facto pattern users
adopt for global `--dangerously-skip-permissions` is a shell alias, but for
a Tauri-spawned subprocess this is unreliable (interactive-shell aliases
don't always inherit). The equivalent knob is documented:

> `--dangerously-skip-permissions` — Skip permission prompts. Equivalent to
> `--permission-mode bypassPermissions`.
> — code.claude.com/docs/en/cli-reference.md

`permissions.defaultMode: "bypassPermissions"` in `~/.claude/settings.json`
(user-level) is honored by Claude Code ≥ 2.1.142. Project-level
`.claude/settings.json` is ignored for `auto` and `bypassPermissions` to
prevent repos from granting themselves elevated mode, so we must write to
the user-level file (or `$CLAUDE_CONFIG_DIR/settings.json` if set).

## Decisions

| Question | Decision |
|---|---|
| Scope | Global (one switch affects all providers) |
| UI placement | Settings menu dropdown, new "Safety" group |
| Confirmation | One-shot confirm dialog on first ON |
| State source | Read from `~/.claude/settings.json` on app start |
| Implementation | Typed `Permissions` struct, two new commands |

## Architecture

```
SettingsMenu (new "Safety" group, one Switch)
    └─ on toggle (after confirm dialog on first ON):
         └─ useDangerousMode hook
              └─ api.setDangerousMode(enabled)          [src/lib/api.ts]
                   └─ invoke("set_dangerous_mode_cmd")
                        └─ commands::settings::set_dangerous_mode_cmd
                             └─ storage::permissions::set(path, backups_dir, enabled)
                                  └─ settings::read_settings → mutate value →
                                     settings::write_settings_atomic
                                       └─ existing locked + backed-up atomic write
                                          of ~/.claude/settings.json, touching only
                                          the `permissions` key

On startup:
  useDangerousMode calls api.getDangerousMode()
    → commands::settings::get_dangerous_mode_cmd
      → storage::permissions::read(&settings_value)
        → return permissions.defaultMode == "bypassPermissions"
```

Three principles preserved:

1. **Same atomic-write path as `load_provider`.** `load_provider_cmd` and
   `set_dangerous_mode_cmd` both end in `settings::write_settings_atomic`,
   which holds the same sidecar lock and creates a timestamped backup.
   No race where a provider load silently clobbers a dangerous-mode toggle
   (or vice versa); no inconsistent backup policy between the two writes.
2. **Pure logic stays pure.** "Given enabled=true, what's the resulting
   `permissions` block?" is a one-liner tested in isolation. I/O is a thin
   wrapper around the existing `read_settings` / `write_settings_atomic` pair.
3. **Same merge semantics as env.** `permissions` is unknown to `merge.rs`,
   so `load_provider` doesn't touch it — confirmed by re-reading
   `merge.rs::merge_env` which preserves unknown top-level keys verbatim
   (it only iterates over `existing.env` and `provider_env`).

## Components

Nine files touched.

### Rust backend

**`src-tauri/src/storage/permissions.rs`** (new, ~60 lines)

```rust
use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::models::{AppError, AppResult};
use crate::storage::settings;

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Permissions {
    #[serde(rename = "defaultMode", default, skip_serializing_if = "Option::is_none")]
    pub default_mode: Option<String>,
}

/// Pure: what should `permissions` look like for the requested state?
pub fn block_for(enabled: bool) -> Permissions {
    if enabled {
        Permissions { default_mode: Some("bypassPermissions".into()) }
    } else {
        Permissions::default()
    }
}

/// Read current state. Returns false if settings.json is missing, malformed,
/// has no `permissions` block, or `defaultMode` is anything other than
/// "bypassPermissions". The conservative default.
pub fn read(settings: &Value) -> bool {
    settings
        .get("permissions")
        .and_then(|p| p.get("defaultMode"))
        .and_then(|v| v.as_str())
        == Some("bypassPermissions")
}

/// Locked + backed-up atomic write. Reuses `settings::write_settings_atomic`
/// so the same lock + backup semantics as `load_provider` apply.
pub fn set(path: &Path, backups_dir: &Path, enabled: bool) -> AppResult<()> {
    let mut value = match settings::read_settings(path)? {
        Some(v) => v,
        None => Value::Object(Default::default()),
    };
    value["permissions"] = serde_json::to_value(block_for(enabled))
        .map_err(|e| AppError::Internal(format!("serialize permissions: {e}")))?;
    settings::write_settings_atomic(path, &value, backups_dir)?;
    Ok(())
}
```

**`src-tauri/src/commands/settings.rs`** (existing, +30 lines)

```rust
#[tauri::command]
pub fn get_dangerous_mode_cmd(app: tauri::AppHandle) -> AppResult<bool> {
    let path = settings::settings_path();
    match settings::read_settings(&path)? {
        None => Ok(false),
        Some(value) => Ok(permissions::read(&value)),
    }
}

#[tauri::command]
pub fn set_dangerous_mode_cmd(app: tauri::AppHandle, enabled: bool) -> AppResult<()> {
    let app_state = app.state::<AppState>();
    let path = settings::settings_path();
    let backups = app_state.backups_dir();
    permissions::set(&path, &backups, enabled)
}
```

**`src-tauri/src/lib.rs`** (existing, +2 lines)

Register the two new commands in `invoke_handler!`:

```rust
commands::settings::get_dangerous_mode_cmd,
commands::settings::set_dangerous_mode_cmd,
```

**`src-tauri/src/storage/mod.rs`** (existing, +1 line)

Add `pub mod permissions;`.

### Frontend

**`src/lib/api.ts`** (existing, +2 lines)

```ts
export const getDangerousMode = () => call<boolean>("get_dangerous_mode_cmd");
export const setDangerousMode = (enabled: boolean) =>
  call<void>("set_dangerous_mode_cmd", { enabled });
```

**`src/hooks/useDangerousMode.ts`** (new, ~40 lines)

- `useState<boolean | null>(null)` (null = loading)
- On mount, calls `getDangerousMode()`
- `toggle()`: if currently null/false → show confirm dialog → call
  `setDangerousMode(true)`; if currently true → call `setDangerousMode(false)`
  directly (no confirm)
- Returns `{ enabled, loaded, toggle, confirmOpen, confirmChoice }`

**`src/components/SettingsMenu.tsx`** (existing, +20 lines)

- New `DropdownMenuGroup` labeled "Safety" between Backup and Updates
- One `DropdownMenuItem` with a right-aligned `Switch`
- Item label: "Dangerous mode (skip permissions)"
- onClick on the row → `props.onToggleDangerousMode()`
- New prop `dangerousMode: boolean | null` + `onToggleDangerousMode: () => void`

**`src/components/DangerousModeConfirm.tsx`** (new, ~50 lines)

- Uses the existing `AlertDialog` primitive
- Body text lists the carve-outs: writes to `.git`/`.claude` skip prompts;
  `rm -rf /` and `rm -rf ~` still prompt as circuit breakers
- One "I understand, turn on" button + cancel
- `localStorage["claude-config.dangerous-mode-ack"] = "1"` set on confirm;
  absence of this key triggers the dialog on next ON click

**`src/app/page.tsx`** (existing, +4 lines)

- Instantiate `useDangerousMode()` next to the existing `useUpdater()` /
  `useProvidersApp()` calls
- Pass `dangerousMode={dangerous.enabled}` and `onToggleDangerousMode={dangerous.toggle}`
  to `<SettingsMenu />`
- Render `<DangerousModeConfirm />` when `dangerous.confirmOpen` is true

## Data flow

### 1. App start — read current state

```
useDangerousMode.ts → useEffect on mount
  → api.getDangerousMode()
      → invoke("get_dangerous_mode_cmd")
          → commands::settings::get_dangerous_mode_cmd
              let settings = settings::read_settings(&path)?;
              Ok(permissions::read(&settings))   // false if missing
  → setState(enabled)
```

One disk read of `settings.json` on app startup. Same read already happens
in `useProvidersApp` for the provider list. We don't share the parsed
result across hooks; separate calls keep data flow linear.

### 2. User clicks toggle (OFF → ON, first time)

```
click → props.onToggleDangerousMode()
  → useDangerousMode.toggle()
      1. check localStorage["claude-config.dangerous-mode-ack"]
           - absent:
               → setState({ confirmOpen: true })
               → render <DangerousModeConfirm />
           - present: skip to step 2
      2. api.setDangerousMode(true)
              → invoke("set_dangerous_mode_cmd", { enabled: true })
                  → commands::settings::set_dangerous_mode_cmd
                      → permissions::set(&path, &backups, true)
                          let mut value = settings::read_settings(&path)?;  // existing
                          value["permissions"] = block_for(true);
                          settings::write_settings_atomic(&path, &value, &backups)?;  // existing
      3. setState({ enabled: true })
      4. localStorage["claude-config.dangerous-mode-ack"] = "1"
```

`settings.json` after write — diff only:

```json
{
  "...existing keys untouched...",
  "permissions": { "defaultMode": "bypassPermissions" }
}
```

A timestamped backup at `<app-data>/backups/settings-<unix-ms>.json` is
created before the write — same as every `load_provider_cmd` write.

### 3. User clicks toggle (ON → OFF)

Same path with `enabled: false`:

- `block_for(false)` returns `Permissions::default()` (all fields `None`)
- `serde_json::to_value(&Permissions::default())` produces `{}`
- Write `settings["permissions"] = {}`

**We set `permissions` to an empty object on OFF, not remove the key.** Two
reasons:

1. Preserves any other permission keys the user authored (e.g., a future
   `disableBypassPermissionsMode` from managed settings rollout)
2. Single write path — toggle is symmetric: ON writes the field, OFF writes
   the default. No "remove key" branch to test.

### Write atomicity

`settings::write_settings_atomic` does:

1. Acquire `lock_exclusive` on `<settings.json>.lock` (sidecar)
2. Back up current contents to `<backups_dir>/settings-<unix-ms>.json`
3. Serialize to `NamedTempFile` in the same directory
4. fsync the temp file
5. Atomic rename into place
6. Release lock

`permissions::set` is a thin wrapper that does the read, mutates one key,
and calls this. If the user toggles ON while `load_provider_cmd` is
mid-flight, the sidecar lock serializes them — whichever wins, the loser
sees the updated file on its next read.

## Error handling

| Failure | Behavior |
|---|---|
| `settings.json` doesn't exist on first read | `read()` returns `false`. Toggle reads as OFF. No error. |
| `settings.json` malformed on read | `AppError::MalformedSettings` propagated to frontend. `useDangerousMode` shows inline error, toggle disabled. |
| Write fails (disk full, permission denied) | `AppError::Io` propagated. Frontend shows toast via existing `Toaster`. Optimistic state rolled back. |
| Managed settings has `disableBypassPermissionsMode: "disable"` | We still write `defaultMode: "bypassPermissions"`, but Claude Code ignores it. **Out of scope for this PR** — see `ponytail:` markers. |
| Running as root / under sudo | Claude Code itself refuses the flag. **Out of scope** — Claude Code's own error is the right place for that warning. |

### What we explicitly do not do

- **No retry on write failure.** Atomic write either succeeds or returns a
  real I/O error; retrying a malformed-state write makes things worse.
- **No backup policy divergence.** We use the existing
  `write_settings_atomic`, which always backs up — matching `load_provider`.
  Users toggling daily will accumulate backups; that's acceptable, and the
  existing cleanup story (out of scope here) applies.

## Testing

### Unit tests in `storage/permissions.rs`

Pure-logic tests — no I/O, no lock:

```rust
#[test]
fn block_for_on_emits_bypass_permissions() {
    assert_eq!(block_for(true).default_mode.as_deref(), Some("bypassPermissions"));
}

#[test]
fn block_for_off_is_empty() {
    let p = block_for(false);
    assert!(p.default_mode.is_none());
    assert_eq!(serde_json::to_value(&p).unwrap(), json!({}));
}

#[test]
fn read_returns_false_on_missing_key() {
    let s = json!({ "env": { "ANTHROPIC_BASE_URL": "x" } });
    assert!(!read(&s));
}

#[test]
fn read_returns_false_on_empty_permissions() {
    assert!(!read(&json!({ "permissions": {} })));
}

#[test]
fn read_returns_true_on_bypass_permissions() {
    assert!(read(&json!({ "permissions": { "defaultMode": "bypassPermissions" } })));
}

#[test]
fn read_ignores_other_permission_keys() {
    let s = json!({
        "permissions": {
            "defaultMode": "default",
            "disableBypassPermissionsMode": "disable"
        }
    });
    assert!(!read(&s));
}

#[test]
fn roundtrip_on_then_off_keeps_empty_object() {
    let mut s = json!({});
    s["permissions"] = serde_json::to_value(block_for(true)).unwrap();
    assert!(read(&s));
    s["permissions"] = serde_json::to_value(block_for(false)).unwrap();
    assert!(!read(&s));
    assert!(s["permissions"].is_object());
}
```

### Integration test for the locked write

```rust
fn fresh_dir(name: &str) -> PathBuf {
    let d = tempfile::tempdir().unwrap().keep().join(name);
    fs::create_dir_all(&d).unwrap();
    d
}

#[test]
fn set_writes_and_round_trips() {
    let dir = fresh_dir("claude");
    let path = dir.join("settings.json");
    let backups = fresh_dir("backups");
    fs::write(&path, "{}").unwrap();

    set(&path, &backups, true).unwrap();
    let after = fs::read_to_string(&path).unwrap();
    assert!(after.contains("\"defaultMode\":\"bypassPermissions\""));

    set(&path, &backups, false).unwrap();
    let after = fs::read_to_string(&path).unwrap();
    assert!(after.contains("\"permissions\":{}"));
}

#[test]
fn set_preserves_unrelated_keys() {
    let dir = fresh_dir("claude");
    let path = dir.join("settings.json");
    let backups = fresh_dir("backups");
    fs::write(
        &path,
        serde_json::to_string_pretty(&json!({
            "env": { "ANTHROPIC_BASE_URL": "https://x" },
            "hooks": { "Stop": [] },
            "extraKnownMarketplaces": { "foo": 1 }
        }))
        .unwrap(),
    )
    .unwrap();

    set(&path, &backups, true).unwrap();
    let after: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(after["env"]["ANTHROPIC_BASE_URL"], "https://x");
    assert_eq!(after["hooks"]["Stop"], json!([]));
    assert_eq!(after["extraKnownMarketplaces"]["foo"], 1);
    assert_eq!(after["permissions"]["defaultMode"], "bypassPermissions");
}
```

`fresh_dir` mirrors the helper already used in `settings.rs` tests.

### What we explicitly do not test

- **No frontend tests.** No JS/TS test runner exists; adding one is out of
  scope. Manual verify via `pnpm tauri dev`.
- **No Tauri command-level tests.** Existing convention: test the
  underlying logic, trust the `#[tauri::command]` macro.
- **No E2E tests.** None exist in the repo.

### Verification commands before merge

```bash
pnpm exec tsc --noEmit       # frontend types still line up
pnpm lint                    # eslint clean
cd src-tauri && cargo test   # all unit tests pass (7 new + existing)
cd src-tauri && cargo build  # no warnings
```

Plus a manual `pnpm tauri dev` smoke:

1. Toggle on → confirm dialog → settings.json has `permissions.defaultMode: "bypassPermissions"` → restart app → toggle shows ON.
2. Toggle off → settings.json has empty `permissions` object → restart app → toggle shows OFF.
3. Toggle on, then toggle on again (no-op): no error, no extra dialog.

## Deferred — `ponytail:` markers

These two are deliberate shortcuts the implementation will leave behind:

- `// ponytail: defer managed-settings detection — separate audit` (warning UI when `disableBypassPermissionsMode: "disable"` is set by org policy)
- `// ponytail: defer root/sudo detection — Claude Code refuses the flag itself with a better error message`

## Out of scope

- Per-provider dangerous mode (would require a new write path that touches a
  third top-level key alongside `env`; revisit if a user asks)
- Shell alias / wrapper script installation (docs favor settings.json; alias
  is unreliable for Tauri subprocesses)
- Managed-settings detection (separate audit)
- Root/sudo detection (Claude Code handles)
- Backup cleanup policy (existing story applies; not modified)