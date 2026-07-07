# Dangerous-Mode Toggle Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a global "Dangerous mode" toggle that flips `permissions.defaultMode` between `"bypassPermissions"` and absent in `~/.claude/settings.json`, surfaced via a Settings menu dropdown item with a one-shot confirm dialog.

**Architecture:** New `storage::permissions` module owns a typed `Permissions` struct and a thin `set()` wrapper around the existing `settings::write_settings_atomic` primitive (so the same sidecar lock + timestamped backup as `load_provider_cmd` apply). Two thin Tauri commands expose get/set to the frontend. A new `useDangerousMode` hook manages the localStorage-gated one-shot confirm dialog and the optimistic UI state.

**Tech Stack:** Rust (existing: serde, serde_json, fs2, tempfile, thiserror), Next.js 16, base-ui Dialog, sonner toasts, React 19 hooks.

## Global Constraints

- Auth tokens never on disk in plaintext — applies to nothing in this plan (no secret changes).
- Single source of truth for env merge is `src-tauri/src/merge.rs` — do NOT extend merge logic for this feature; `permissions` lives at the top level of `settings.json` and is untouched by `merge_env`.
- `settings.json` mutation only via `storage::settings::write_settings_atomic` — every other write path is forbidden.
- `useEffect` `set-state-in-effect` ESLint rule is disabled in this codebase for initial-load hooks (`useProvidersApp` is the precedent) — disable per-line with the documented comment if `useDangerousMode` triggers the same lint.
- TypeScript mirrors Rust models — no struct drift between `src/lib/api.ts` and `src-tauri/src/commands/settings.rs`.
- No JS/TS test runner — frontend verification is `pnpm exec tsc --noEmit` + `pnpm lint` only.
- All commands return `AppResult<T>` so errors propagate as `{kind, message}` to the frontend.
- Schema bump: none. `permissions` is an unknown top-level key; existing `merge.rs` ignores it.

---

## File Structure

| File | Responsibility | Touch type |
|---|---|---|
| `src-tauri/src/storage/permissions.rs` | Pure `Permissions` struct + `block_for` + `read` + `set` I/O wrapper | New |
| `src-tauri/src/storage/mod.rs` | Re-export `permissions` module | Modify (+1 line, `pub mod permissions;`) |
| `src-tauri/src/commands/settings.rs` | `get_dangerous_mode_cmd` + `set_dangerous_mode_cmd` | Modify (+~25 lines) |
| `src-tauri/src/lib.rs` | Register the two new commands in `invoke_handler!` | Modify (+2 lines) |
| `src/lib/api.ts` | Typed `getDangerousMode` / `setDangerousMode` wrappers | Modify (+2 lines) |
| `src/hooks/useDangerousMode.ts` | State machine: load, toggle (with confirm gate), optimistic update + rollback | New |
| `src/components/DangerousModeConfirm.tsx` | base-ui Dialog explaining the carve-outs | New |
| `src/components/SettingsMenu.tsx` | New "Safety" group with `DropdownMenuCheckboxItem` | Modify (+~25 lines, 2 new props) |
| `src/app/page.tsx` | Instantiate hook, pass props, render confirm dialog | Modify (+~5 lines) |

No new dependencies. No schema version bump. No backend capability changes (Tauri's existing capabilities cover `invoke("set_dangerous_mode_cmd", ...)`).

---

### Task 1: Rust storage — pure `permissions` logic + module registration

**Files:**
- Create: `src-tauri/src/storage/permissions.rs`
- Modify: `src-tauri/src/storage/mod.rs` (add `pub mod permissions;` on line 7-ish)

**Interfaces:**
- Consumes: nothing (leaf module)
- Produces:
  - `pub struct Permissions { pub default_mode: Option<String> }` with serde rename `defaultMode`, `skip_serializing_if = "Option::is_none"`
  - `pub fn block_for(enabled: bool) -> Permissions`
  - `pub fn read(settings: &serde_json::Value) -> bool`

This task is pure logic — no I/O, no lock. Reviewer concerns: serde correctness of the struct, predicate correctness in `read`.

- [ ] **Step 1: Create `src-tauri/src/storage/permissions.rs` with the failing test module**

```rust
//! Pure logic for the `permissions` block in `~/.claude/settings.json`.
//! I/O lives in `set()` (see Step 8 below) which delegates to
//! `storage::settings::write_settings_atomic` so the same lock + backup
//! semantics as `load_provider_cmd` apply.

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Permissions {
    #[serde(rename = "defaultMode", default, skip_serializing_if = "Option::is_none")]
    pub default_mode: Option<String>,
}

/// Pure: what should the `permissions` block look like for the requested state?
pub fn block_for(enabled: bool) -> Permissions {
    if enabled {
        Permissions {
            default_mode: Some("bypassPermissions".into()),
        }
    } else {
        Permissions::default()
    }
}

/// Read current state. Conservative: returns `false` if `settings.json` is
/// missing, malformed, has no `permissions` block, or `defaultMode` is
/// anything other than the literal string `"bypassPermissions"`.
pub fn read(settings: &Value) -> bool {
    settings
        .get("permissions")
        .and_then(|p| p.get("defaultMode"))
        .and_then(|v| v.as_str())
        == Some("bypassPermissions")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn block_for_on_emits_bypass_permissions() {
        let p = block_for(true);
        assert_eq!(p.default_mode.as_deref(), Some("bypassPermissions"));
    }

    #[test]
    fn block_for_off_is_empty() {
        let p = block_for(false);
        assert!(p.default_mode.is_none());
        // Serialized form must be `{}` — proves skip_serializing_if works.
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
        // If the user has disableBypassPermissionsMode or future keys, our
        // read must not mistake them for "on".
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
}
```

- [ ] **Step 2: Register the module in `src-tauri/src/storage/mod.rs`**

Insert one line at the bottom of the `pub mod` block (after `pub mod settings;`):

```rust
pub mod permissions;
```

- [ ] **Step 3: Run the failing tests to verify they compile and pass**

Run: `cd src-tauri && cargo test storage::permissions::`
Expected: 7 passed, 0 failed. (The struct + `block_for` + `read` are all in the file from Step 1; nothing is "failing" yet — TDD's "write the failing test" step is moot for a brand-new module whose only deliverable is the code itself. The tests serve as the spec.)

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/storage/permissions.rs src-tauri/src/storage/mod.rs
git commit -m "feat(storage): add permissions module with pure block_for/read"
```

---

### Task 2: Rust storage — `permissions::set` I/O wrapper + integration tests

**Files:**
- Modify: `src-tauri/src/storage/permissions.rs` (append `set` fn + 2 tests)

**Interfaces:**
- Consumes:
  - `permissions::block_for(enabled: bool) -> Permissions` (Task 1)
  - `storage::settings::read_settings(path) -> AppResult<Option<Value>>` (existing)
  - `storage::settings::write_settings_atomic(path, &value, backups_dir) -> AppResult<SettingsBackup>` (existing)
- Produces:
  - `pub fn set(path: &Path, backups_dir: &Path, enabled: bool) -> AppResult<()>`

Reviewer concerns: atomic write safety (delegated to existing helper), preservation of unrelated top-level keys in `settings.json`.

- [ ] **Step 1: Append the failing integration test to `permissions.rs`**

Inside the existing `#[cfg(test)] mod tests` block, append:

```rust
    use std::fs;
    use std::path::PathBuf;

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

        super::set(&path, &backups, true).unwrap();
        let after = fs::read_to_string(&path).unwrap();
        assert!(after.contains("\"defaultMode\":\"bypassPermissions\""));

        super::set(&path, &backups, false).unwrap();
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

        super::set(&path, &backups, true).unwrap();
        let after: Value =
            serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(after["env"]["ANTHROPIC_BASE_URL"], "https://x");
        assert_eq!(after["hooks"]["Stop"], json!([]));
        assert_eq!(after["extraKnownMarketplaces"]["foo"], 1);
        assert_eq!(after["permissions"]["defaultMode"], "bypassPermissions");
    }

    #[test]
    fn set_creates_file_when_missing() {
        let dir = fresh_dir("claude");
        let path = dir.join("settings.json");
        let backups = fresh_dir("backups");
        // No file written — simulates first run on a fresh machine.
        super::set(&path, &backups, true).unwrap();
        assert!(path.exists());
        let after: Value =
            serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(after["permissions"]["defaultMode"], "bypassPermissions");
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cd src-tauri && cargo test storage::permissions::test::set_`
Expected: FAIL with `error[E0425]: cannot find function 'set' in module 'storage::permissions'`.

- [ ] **Step 3: Add the `set` function above the test module**

In `permissions.rs`, replace the existing `use` block at the top with:

```rust
use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::models::{AppError, AppResult};
use crate::storage::settings;
```

Then insert this function between `read` and `#[cfg(test)]`:

```rust
/// Locked + backed-up atomic write. Reuses `settings::write_settings_atomic`
/// so the same sidecar lock and timestamped backup as `load_provider_cmd`
/// apply — no race between the two writers, no backup-policy divergence.
///
/// If `settings.json` doesn't exist yet, this creates it with the `permissions`
/// block as the only key. Other top-level keys (env, hooks, plugins, etc.) are
/// preserved verbatim — the closure mutates only the `permissions` field.
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

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cd src-tauri && cargo test storage::permissions::`
Expected: 10 passed, 0 failed (7 pure + 3 integration).

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/storage/permissions.rs
git commit -m "feat(storage): add permissions::set with locked atomic write"
```

---

### Task 3: Tauri commands — `get_dangerous_mode_cmd` + `set_dangerous_mode_cmd`

**Files:**
- Modify: `src-tauri/src/commands/settings.rs` (append two commands)
- Modify: `src-tauri/src/lib.rs` (add two lines to `invoke_handler!`)

**Interfaces:**
- Consumes:
  - `storage::permissions::{read, set}` (Task 1, 2)
  - `storage::settings::{settings_path, read_settings}` (existing)
  - `state::AppState::backups_dir()` (existing)
- Produces:
  - `#[tauri::command] pub fn get_dangerous_mode_cmd(app: tauri::AppHandle) -> AppResult<bool>`
  - `#[tauri::command] pub fn set_dangerous_mode_cmd(app: tauri::AppHandle, enabled: bool) -> AppResult<()>`

Existing convention: trust the `#[tauri::command]` macro, test the underlying logic. Reviewer concerns: correct AppHandle→AppState extraction, correct path resolution, correct error propagation.

- [ ] **Step 1: Append the two commands to `src-tauri/src/commands/settings.rs`**

Add to the existing `use` block at the top (no changes if already present, otherwise add `storage::permissions`):

```rust
use crate::storage::permissions;
```

Then append at the end of the file:

```rust
#[tauri::command]
pub fn get_dangerous_mode_cmd(_app: tauri::AppHandle) -> AppResult<bool> {
    let path = settings_path();
    match read_settings(&path)? {
        None => Ok(false),
        Some(value) => Ok(permissions::read(&value)),
    }
}

#[tauri::command]
pub fn set_dangerous_mode_cmd(
    app: tauri::AppHandle,
    enabled: bool,
) -> AppResult<()> {
    let app_state = app.state::<crate::state::AppState>();
    let path = settings_path();
    let backups = app_state.backups_dir();
    permissions::set(&path, &backups, enabled)
}
```

The leading underscore on `_app` in `get_dangerous_mode_cmd` is intentional — `read_settings` already calls `settings_path()` internally so we don't need the handle for state. If the linter complains, change to `app: tauri::AppHandle` and drop the underscore.

- [ ] **Step 2: Register the commands in `src-tauri/src/lib.rs`**

In the `invoke_handler!` macro block (around line 63-86), add two lines next to the existing settings commands:

```rust
            commands::settings::get_dangerous_mode_cmd,
            commands::settings::set_dangerous_mode_cmd,
```

- [ ] **Step 3: Build to verify it compiles**

Run: `cd src-tauri && cargo build`
Expected: compiles with no new warnings. (`cargo test` is unchanged; new commands have no separate test per existing convention.)

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/commands/settings.rs src-tauri/src/lib.rs
git commit -m "feat(commands): add get/set_dangerous_mode_cmd"
```

---

### Task 4: Frontend — typed API wrappers

**Files:**
- Modify: `src/lib/api.ts` (append 2 functions)

**Interfaces:**
- Consumes: existing `call<T>(cmd, args)` helper
- Produces:
  - `export const getDangerousMode = () => call<boolean>("get_dangerous_mode_cmd")`
  - `export const setDangerousMode = (enabled: boolean) => call<void>("set_dangerous_mode_cmd", { enabled })`

No tests. Verification is `pnpm exec tsc --noEmit` and `pnpm lint`.

- [ ] **Step 1: Append the wrappers to `src/lib/api.ts`**

At the end of the file:

```ts
// ---------- dangerous mode ----------

export const getDangerousMode = () =>
  call<boolean>("get_dangerous_mode_cmd");
export const setDangerousMode = (enabled: boolean) =>
  call<void>("set_dangerous_mode_cmd", { enabled });
```

- [ ] **Step 2: Verify type-check + lint**

Run: `pnpm exec tsc --noEmit && pnpm lint`
Expected: both succeed, no errors. `pnpm lint` may report unused-import warnings if the new exports aren't used yet — that's fine for this task; Task 5 wires them in.

- [ ] **Step 3: Commit**

```bash
git add src/lib/api.ts
git commit -m "feat(api): add getDangerousMode / setDangerousMode wrappers"
```

---

### Task 5: Frontend — `useDangerousMode` hook

**Files:**
- Create: `src/hooks/useDangerousMode.ts`

**Interfaces:**
- Consumes:
  - `getDangerousMode`, `setDangerousMode` from `@/lib/api`
  - `toast` from `sonner`
- Produces:
  ```ts
  function useDangerousMode(): {
    enabled: boolean | null;     // null = loading, true = ON, false = OFF
    loaded: boolean;              // false while initial read in flight
    confirmOpen: boolean;         // drives the AlertDialog
    toggle: () => Promise<void>;  // entry point for the menu item
    dismissConfirm: () => void;   // cancel button on the dialog
  }
  ```

Reviewer concerns: localStorage gating, optimistic update + rollback, no `set-state-in-effect` lint fire (use the existing `useProvidersApp` precedent's `eslint-disable` comment if needed).

- [ ] **Step 1: Create `src/hooks/useDangerousMode.ts`**

```tsx
/* eslint-disable react-hooks/set-state-in-effect --
 * Same precedent as useProvidersApp: a one-shot initial fetch on mount
 * doesn't justify useSyncExternalStore complexity.
 */
"use client";

import { useCallback, useEffect, useState } from "react";
import { toast } from "sonner";

import { getDangerousMode, setDangerousMode } from "@/lib/api";

const ACK_KEY = "claude-config.dangerous-mode-ack";

export function useDangerousMode() {
  const [enabled, setEnabled] = useState<boolean | null>(null);
  const [loaded, setLoaded] = useState(false);
  const [confirmOpen, setConfirmOpen] = useState(false);

  useEffect(() => {
    let cancelled = false;
    getDangerousMode()
      .then((v) => {
        if (!cancelled) setEnabled(v);
      })
      .catch((e) => {
        if (!cancelled) {
          setEnabled(false);
          toast.error(`Could not read dangerous-mode state: ${e.message}`);
        }
      })
      .finally(() => {
        if (!cancelled) setLoaded(true);
      });
    return () => {
      cancelled = true;
    };
  }, []);

  const apply = useCallback(async (next: boolean) => {
    const prev = enabled;
    setEnabled(next); // optimistic
    try {
      await setDangerousMode(next);
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      setEnabled(prev); // rollback
      toast.error(`Could not save dangerous-mode state: ${msg}`);
    }
  }, [enabled]);

  const toggle = useCallback(async () => {
    if (enabled === true) {
      // OFF — no confirmation needed.
      await apply(false);
      return;
    }
    // OFF → ON: gate on first-time acknowledgement.
    if (typeof window === "undefined") return;
    const acked = window.localStorage.getItem(ACK_KEY) === "1";
    if (acked) {
      await apply(true);
    } else {
      setConfirmOpen(true);
    }
  }, [enabled, apply]);

  const confirm = useCallback(async () => {
    if (typeof window !== "undefined") {
      window.localStorage.setItem(ACK_KEY, "1");
    }
    setConfirmOpen(false);
    await apply(true);
  }, [apply]);

  const dismissConfirm = useCallback(() => {
    setConfirmOpen(false);
  }, []);

  return {
    enabled,
    loaded,
    confirmOpen,
    toggle,
    confirm,
    dismissConfirm,
  };
}
```

- [ ] **Step 2: Verify type-check + lint**

Run: `pnpm exec tsc --noEmit && pnpm lint`
Expected: both succeed. If `pnpm lint` reports `set-state-in-effect` despite the disable comment, double-check the comment is on the line directly above `useEffect`.

- [ ] **Step 3: Commit**

```bash
git add src/hooks/useDangerousMode.ts
git commit -m "feat(hooks): add useDangerousMode with one-shot confirm gate"
```

---

### Task 6: Frontend — `DangerousModeConfirm` dialog

**Files:**
- Create: `src/components/DangerousModeConfirm.tsx`

**Interfaces:**
- Consumes: existing `Dialog` primitives from `@/components/ui/dialog`, `Button` from `@/components/ui/button`
- Produces: a presentational component that takes `{ open: boolean; onConfirm: () => void; onCancel: () => void }` and renders the existing `Dialog`

No tests. Verification is type-check + lint.

- [ ] **Step 1: Create `src/components/DangerousModeConfirm.tsx`**

```tsx
"use client";

import { AlertTriangle } from "lucide-react";

import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";

interface Props {
  open: boolean;
  onConfirm: () => void;
  onCancel: () => void;
}

export function DangerousModeConfirm({ open, onConfirm, onCancel }: Props) {
  return (
    <Dialog open={open} onOpenChange={(o) => !o && onCancel()}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            <AlertTriangle className="size-4 text-amber-500" />
            Enable dangerous mode?
          </DialogTitle>
          <DialogDescription className="space-y-3 pt-2 text-sm">
            <p>
              Claude Code will run without asking permission for{" "}
              <strong>any</strong> file write or shell command.
            </p>
            <p>
              It will still pause for{" "}
              <code className="rounded bg-muted px-1 py-0.5 font-mono text-xs">
                rm -rf /
              </code>{" "}
              and{" "}
              <code className="rounded bg-muted px-1 py-0.5 font-mono text-xs">
                rm -rf ~
              </code>{" "}
              as a circuit breaker.
            </p>
          </DialogDescription>
        </DialogHeader>
        <DialogFooter>
          <DialogClose render={<Button variant="ghost" />}>
            Cancel
          </DialogClose>
          <Button variant="destructive" onClick={onConfirm}>
            I understand, turn on
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
```

- [ ] **Step 2: Verify type-check + lint**

Run: `pnpm exec tsc --noEmit && pnpm lint`
Expected: both succeed. If `Dialog` exports don't match the imports, read `src/components/ui/dialog.tsx` and adjust to the actual exports — the file currently uses `base-ui/react/dialog` and exports `Dialog`, `DialogTrigger`, `DialogPortal`, `DialogClose`, `DialogOverlay`, plus content/header/title/description/footer wrappers. The exact export names depend on what's exported there; mirror whatever pattern `ClaudeMdEditor` (or any other consumer) uses.

- [ ] **Step 3: Commit**

```bash
git add src/components/DangerousModeConfirm.tsx
git commit -m "feat(ui): add DangerousModeConfirm dialog"
```

---

### Task 7: Frontend — Settings menu "Safety" group with toggle

**Files:**
- Modify: `src/components/SettingsMenu.tsx`

**Interfaces:**
- Consumes: existing `DropdownMenu*` primitives, `getDangerousMode` / `setDangerousMode` are NOT used here (the parent owns state via `useDangerousMode`)
- Produces: 2 new props on `SettingsMenu`:
  - `dangerousMode: boolean | null`
  - `onToggleDangerousMode: () => void`

No tests. Verification is type-check + lint.

- [ ] **Step 1: Add the new props to the `Props` interface**

Replace the existing `interface Props` block with:

```ts
interface Props {
  appDataDir: string | null;
  claudeDir: string | null;
  updateAvailable: boolean;
  updateError: string | null;
  onRevealAppDir: () => void;
  onRevealClaudeDir: () => void;
  onExport: (includeSecrets: boolean) => void;
  onImport: () => void;
  onCheckForUpdates: () => void;
  dangerousMode: boolean | null;
  onToggleDangerousMode: () => void;
}
```

- [ ] **Step 2: Destructure the new props in the function signature**

Replace the existing destructured `Props` parameter with:

```ts
export function SettingsMenu({
  appDataDir,
  claudeDir,
  updateAvailable,
  updateError,
  onRevealAppDir,
  onRevealClaudeDir,
  onExport,
  onImport,
  onCheckForUpdates,
  dangerousMode,
  onToggleDangerousMode,
}: Props) {
```

- [ ] **Step 3: Add the "Safety" group between Backup and Updates**

Insert a new `DropdownMenuSeparator` + `DropdownMenuGroup` immediately after the existing Backup group's closing `</DropdownMenuGroup>` and before the existing `<DropdownMenuSeparator />` that precedes the Updates group:

```tsx
        <DropdownMenuSeparator />
        <DropdownMenuGroup>
          <DropdownMenuLabel>Safety</DropdownMenuLabel>
          <DropdownMenuCheckboxItem
            checked={dangerousMode === true}
            onCheckedChange={() => onToggleDangerousMode()}
            disabled={dangerousMode === null}
          >
            Dangerous mode (skip permissions)
          </DropdownMenuCheckboxItem>
        </DropdownMenuGroup>
```

- [ ] **Step 4: Verify type-check + lint**

Run: `pnpm exec tsc --noEmit && pnpm lint`
Expected: both succeed. If `DropdownMenuCheckboxItem`'s prop types differ (`checked` vs `onCheckedChange` shape), read `src/components/ui/dropdown-menu.tsx` and adapt — the file wraps `MenuPrimitive.CheckboxItem` from base-ui; the prop names come from there.

- [ ] **Step 5: Commit**

```bash
git add src/components/SettingsMenu.tsx
git commit -m "feat(ui): add Safety group with dangerous-mode toggle"
```

---

### Task 8: Frontend — `page.tsx` wiring

**Files:**
- Modify: `src/app/page.tsx`

**Interfaces:**
- Consumes: `useDangerousMode` from `@/hooks/useDangerousMode`, `DangerousModeConfirm` from `@/components/DangerousModeConfirm`
- Produces: hook instantiated at the page level; `dangerousMode` + `onToggleDangerousMode` passed to `SettingsMenu`; `<DangerousModeConfirm />` rendered in the page tree.

No tests. Verification is type-check + lint.

- [ ] **Step 1: Add the import and the hook instantiation**

In `src/app/page.tsx`, add to the existing import block:

```tsx
import { useDangerousMode } from "@/hooks/useDangerousMode";
import { DangerousModeConfirm } from "@/components/DangerousModeConfirm";
```

Then inside `Page()`, add the hook call next to the existing ones:

```tsx
  const dangerous = useDangerousMode();
```

- [ ] **Step 2: Pass the new props to `<SettingsMenu />`**

Update the existing `<SettingsMenu ... />` element to add:

```tsx
            dangerousMode={dangerous.enabled}
            onToggleDangerousMode={dangerous.toggle}
```

- [ ] **Step 3: Render the confirm dialog at the end of the page tree**

Immediately before the closing `</div>` of the outermost flex container (or as a sibling at the end — placement doesn't affect behavior, only z-index stacking):

```tsx
      <DangerousModeConfirm
        open={dangerous.confirmOpen}
        onConfirm={dangerous.confirm}
        onCancel={dangerous.dismissConfirm}
      />
```

- [ ] **Step 4: Verify type-check + lint**

Run: `pnpm exec tsc --noEmit && pnpm lint`
Expected: both succeed. If `pnpm tauri dev` is available, smoke-test by opening the settings menu — the Safety group should appear with the toggle reflecting current state.

- [ ] **Step 5: Commit**

```bash
git add src/app/page.tsx
git commit -m "feat(ui): wire useDangerousMode into page.tsx"
```

---

### Task 9: Manual smoke test (verification only, no code changes)

No files modified.

- [ ] **Step 1: Boot the app and verify the OFF→ON→restart→ON flow**

Run: `pnpm tauri dev`

1. Open the settings menu (gear icon, top right). Confirm a "Safety" group exists with one item: "Dangerous mode (skip permissions)", unchecked.
2. Click the item. Confirm dialog appears with the warning text and two buttons.
3. Click "I understand, turn on". Dialog closes; the toggle is now checked.
4. In a separate terminal, inspect `~/.claude/settings.json`: confirm `"permissions": { "defaultMode": "bypassPermissions" }` is present.
5. Confirm a backup file exists at `<app-data>/backups/settings-<unix-ms>.json` containing the pre-toggle contents.
6. Quit the app and re-launch with `pnpm tauri dev`. Open settings. The toggle should still be checked.

- [ ] **Step 2: Verify the ON→OFF→restart→OFF flow**

Continuing from Step 1:

1. Click the toggle. No dialog appears (already acknowledged). Toggle is now unchecked.
2. Inspect `~/.claude/settings.json`: `"permissions": {}` (empty object, key not removed).
3. Confirm the pre-toggle backup also exists in `<app-data>/backups/`.
4. Quit and re-launch. Toggle still unchecked.

- [ ] **Step 3: Verify unrelated keys survive**

1. Manually add a top-level key to `~/.claude/settings.json` that isn't `permissions` or `env`, e.g. `"foo": "bar"`.
2. Toggle dangerous mode on and back off in the app.
3. Inspect `~/.claude/settings.json`: `"foo": "bar"` is still present.

- [ ] **Step 4: Run the full Rust test suite as a final gate**

Run: `cd src-tauri && cargo test`
Expected: all tests pass (10 new `permissions` tests + existing suite).

- [ ] **Step 5: Run lint + typecheck as a final gate**

Run: `pnpm exec tsc --noEmit && pnpm lint && cd src-tauri && cargo build`
Expected: all three succeed with no warnings.

---

## Self-Review

**Spec coverage:**
- Goal (global toggle, flips `permissions.defaultMode`) → Tasks 1, 2, 3, 5
- Background cited (`code.claude.com/docs/en/cli-reference.md`, `permission-modes.md`) → no task needed, this is context for the engineer
- Decisions table → reflected in file structure (9 files) + UI placement in Task 7
- Architecture (SettingsMenu → hook → api → command → storage → atomic write) → Tasks 4-8 implement it; Tasks 1-3 implement the backend half
- Components (9 files) → all listed with exact line counts and code
- Data flow (3 flows: read, ON, OFF) → Tasks 1-5 + Task 9 smoke
- Error handling (5 cases) → covered by the integration tests in Task 2 (`set_creates_file_when_missing`) and by `useDangerousMode`'s `.catch` handler in Task 5
- Testing (pure + integration) → Task 1 has 7 pure tests, Task 2 has 3 integration tests; manual smoke in Task 9
- `ponytail:` markers (managed-settings deferral, root/sudo deferral) → these are explicitly OUT of scope and don't get tasks; they're captured in the spec's "Deferred" section so the next person who reads it knows they exist

**Placeholder scan:** No "TBD", "TODO", "implement later". All code blocks are complete and copy-pasteable. Every step shows the actual code or the actual command.

**Type consistency:**
- `block_for(enabled: bool) -> Permissions` defined in Task 1, used in Task 2 ✓
- `permissions::set(path: &Path, backups_dir: &Path, enabled: bool) -> AppResult<()>` defined in Task 2, called from Task 3 with the same signature ✓
- `getDangerousMode() -> Promise<boolean>` / `setDangerousMode(enabled: boolean) -> Promise<void>` defined in Task 4, consumed in Task 5 ✓
- `useDangerousMode()` returns `{ enabled, loaded, confirmOpen, toggle, confirm, dismissConfirm }` (Task 5), consumed in Task 8 with matching names ✓
- `SettingsMenu` Props additions in Task 7 (`dangerousMode`, `onToggleDangerousMode`) match the values passed in Task 8 ✓
- `DangerousModeConfirm` Props (`open`, `onConfirm`, `onCancel`) defined in Task 6, populated in Task 8 with `dangerous.confirmOpen`, `dangerous.confirm`, `dangerous.dismissConfirm` ✓

**Found and fixed during review:**
- Task 1 was originally going to be "TDD with separate write/run/verify steps," but for a brand-new leaf module the natural order is "write the file with all tests inline, run them, commit" — the "failing test first" pattern is meaningful when modifying existing code, not when the file is new. Documented this in the task.
- Task 6 originally referenced `AlertDialog` from base-ui, but the existing `dialog.tsx` primitive uses `Dialog` (no separate `AlertDialog`). Updated to use `Dialog` to match the codebase.
- Task 7 originally suggested a custom `Switch` UI primitive; switched to `DropdownMenuCheckboxItem` since the existing `dropdown-menu.tsx` already exports it. One fewer file, one fewer dependency on a yet-to-be-built primitive.
- Task 3 had a potential issue: `read_settings` doesn't need `app.state::<AppState>()`. Replaced `app: tauri::AppHandle` with `_app: tauri::AppHandle` and noted the underscore convention.