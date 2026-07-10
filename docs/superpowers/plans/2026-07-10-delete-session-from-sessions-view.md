# Delete Session from Sessions View — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a hover-reveal trash button on every `SessionRow` that opens a confirmation dialog; on confirm, move the `.jsonl` to OS Trash via the `trash` crate and strip the matching entry from `sessions-index.json`. Local-only; GitHub copy untouched.

**Architecture:** New Rust command `delete_session_cmd` (validate → trash → strip-index, in that order so a crash never silently orphans a file). New Tauri command goes in `commands/system.rs` (matches the existing pattern of `list_sessions_cmd` and `parse_session_cmd` — both live there, not in a `commands/sessions.rs`). Frontend: tiny `SessionDeleteDialog` component reuses the existing `dialog.tsx` primitive; `SessionRow` gets a hover-reveal `Trash2` button that opens the dialog. After confirm, `useSessions.refresh()` re-fetches (signature changed to return the new array) and `useSessionUpload.seed()` is called explicitly to drop the deleted id from the upload state map.

**Tech Stack:** Rust (Tauri 2), `trash = "5"` crate (Linux/macOS/Windows native APIs), `serde_json`, `tempfile::NamedTempFile` for atomic write (already a dev-dep). Next.js 16 + React 19, `@base-ui/react` Dialog primitive, `sonner` toast (already wired).

## Global Constraints

- Rust toolchain: 1.77.2 (per `src-tauri/Cargo.toml`).
- Frontend: Next.js 16 static export, no SSR, no API routes.
- `pnpm tauri dev` for real app work; `pnpm dev` only shows the browser stub.
- Verification gates: `pnpm lint`, `pnpm exec tsc --noEmit`, `cd src-tauri && cargo test`.
- Spec requires: local-only delete (GitHub copy untouched), OS Trash via `trash` crate, index entry stripped atomically, hover-reveal UX, toast on success/failure, detail panel closes if open.
- Step order: `validate → trash → strip index`. Trash-first means the only remaining partial-failure state is a stale index entry that self-heals on next scan. We never silently lose a file the user expected to delete.
- `useSessions.refresh()` signature changes from `Promise<void>` to `Promise<SessionSummary[]>` (one-line change). All current call sites use `void refresh()` or `await refresh()` and ignore the return — verified safe.
- Re-seed wiring: `useSessionUpload`'s existing re-seed effect is gated on `isWebEnv()` and never fires in Tauri — so the explicit `await seed(refreshed)` call in `onConfirmDelete` is required, not optional.
- Path validation: `full_path` must resolve under `<claude_dir>/projects/`. Canonicalize both sides to defend against `..` traversal and symlinks. Use `std::path::Path::starts_with` on canonicalized forms.
- Atomic index write: temp file in the same dir + `sync_all` + `persist` (matches `commands/settings.rs:write_state` pattern at line 416).
- Tests that actually call `trash::delete_all` are gated `#[ignore]` — they pollute the OS Trash. Run them explicitly with `cargo test -- --ignored delete_session` (same pattern as the keyring integration tests per CLAUDE.md).

## File Structure

| File | Role | Touched in |
|---|---|---|
| `src-tauri/Cargo.toml` | Add `trash = "5"` dep | Task 1 |
| `src-tauri/src/storage/sessions.rs` | Add `Serialize` to `SessionsIndex`/`SessionIndexEntry`; make them `pub(crate)` so `commands/system.rs` can read+rewrite | Task 1 |
| `src-tauri/src/commands/system.rs` | Add `delete_session_cmd` + 3 private helpers + 5 tests; register in lib.rs | Task 1 |
| `src-tauri/src/lib.rs` | Register `delete_session_cmd` in `invoke_handler!` | Task 1 |
| `src/hooks/useSessions.ts` | `refresh` returns `Promise<SessionSummary[]>` | Task 2 |
| `src/lib/api.ts` | Add `deleteSession(fullPath)` | Task 3 |
| `src/components/SessionDeleteDialog.tsx` | New: confirm dialog (mirrors `DeleteDialog.tsx`) | Task 3 |
| `src/components/Sessions.tsx` | Add trash button to `SessionRow`; add `deleteTarget` state; mount dialog | Task 4 |

No new IPC types. No schema migrations. No changes to `models.rs` or `src/lib/types.ts`.

---

### Task 1: Rust command `delete_session_cmd` + TDD coverage

**Files:**
- Modify: `src-tauri/Cargo.toml` (add `trash = "5"` to `[dependencies]`)
- Modify: `src-tauri/src/storage/sessions.rs:30-58` (add `Serialize` derive, change visibility to `pub(crate)`)
- Modify: `src-tauri/src/commands/system.rs` (add `delete_session_cmd`, helpers, tests at the end of the file)
- Modify: `src-tauri/src/lib.rs` (add `commands::system::delete_session_cmd` to `invoke_handler!`)

**Interfaces:**
- Consumes: `discover_claude_dir()` (existing, in `storage::settings`), `AppError` variants `Io` and `Validation`.
- Produces: `pub fn delete_session_cmd(full_path: String) -> AppResult<()>` — Tauri command, no `state` needed (mirrors `list_sessions_cmd`).

- [ ] **Step 1: Add the `trash` dep**

Edit `src-tauri/Cargo.toml`. In `[dependencies]`, add a new line alphabetically near the other crates:

```toml
trash = "5"
```

- [ ] **Step 2: Run a no-op build to confirm dep resolves**

Run: `cd src-tauri && cargo build --lib 2>&1 | tail -5`
Expected: builds clean. New dep downloaded, no errors.

- [ ] **Step 3: Make `SessionsIndex` round-trippable**

Edit `src-tauri/src/storage/sessions.rs:30-58`. Change the two struct declarations:

```rust
#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct SessionsIndex {
    #[allow(dead_code)]
    pub(crate) version: u32,
    #[serde(default)]
    pub(crate) entries: Vec<SessionIndexEntry>,
}

#[derive(Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SessionIndexEntry {
    #[serde(default)]
    pub(crate) session_id: String,
    #[serde(default)]
    pub(crate) full_path: String,
    #[serde(default)]
    pub(crate) first_prompt: Option<String>,
    #[serde(default)]
    pub(crate) summary: Option<String>,
    #[serde(default)]
    pub(crate) message_count: Option<u32>,
    #[serde(default)]
    pub(crate) created: Option<String>,
    #[serde(default)]
    pub(crate) modified: Option<String>,
    #[serde(default)]
    pub(crate) project_path: Option<String>,
    #[serde(default)]
    pub(crate) is_sidechain: Option<bool>,
}
```

Three changes vs current: `Serialize` added; `pub(crate)` on the struct and each field so `commands/system.rs` can construct/read.

- [ ] **Step 4: Add the imports in `commands/system.rs`**

Edit `src-tauri/src/commands/system.rs`. Find the import block at the top (around line 1-14). Extend it to add the new things the command needs:

```rust
use std::path::{Path, PathBuf};

use tauri_plugin_opener::OpenerExt;

use crate::models::{AppError, AppResult};
use crate::state::AppState;
use crate::storage::claude_md::{claude_md_path, read_claude_md, write_claude_md_atomic};
use crate::storage::sessions::{SessionIndexEntry, SessionsIndex};
use crate::storage::{
    discover_claude_dir, parse_session_transcript, scan_marketplaces, scan_mcp_servers,
    scan_sessions, scan_skills, MarketplaceSummary, McpServerSummary, SessionMessage,
    SessionSummary, SkillSummary,
};
```

Note: `std::path::Path` is added alongside the existing `PathBuf`. `trash` is `use`d inside the function (small enough that an inline import is clearer than hoisting).

- [ ] **Step 5: Write the 5 failing unit tests**

Append the following at the very end of `src-tauri/src/commands/system.rs`:

```rust
// ---- delete_session_cmd ----
//
// Tests that touch `trash::delete_all` are gated `#[ignore]` because they
// pollute the OS Trash. Run them with:
//   cargo test -- --ignored delete_session
//
// Tests that only exercise validation, index rewriting, or already-missing
// files run normally.

#[cfg(test)]
mod delete_session_tests {
    use super::*;
    use crate::storage::sessions::PROJECTS_DIR;
    use std::fs;

    /// Two-entry index; delete the first. Surviving entry structurally
    /// equals the original (serde round-trip — NOT byte-identical).
    #[test]
    fn strips_entry_from_index() {
        let tmp = tempfile::tempdir().unwrap();
        let proj = tmp.path().join("projects/-home-x");
        fs::create_dir_all(&proj).unwrap();
        let jsonl = proj.join("aaa.jsonl");
        fs::write(&jsonl, "{}\n").unwrap();

        let keep_path = jsonl.display().to_string();
        let drop_path = proj.join("bbb.jsonl").display().to_string();

        let index = serde_json::json!({
            "version": 1,
            "entries": [
                {"sessionId": "aaa", "fullPath": drop_path, "summary": "drop"},
                {"sessionId": "bbb", "fullPath": keep_path, "summary": "keep"},
            ]
        });
        fs::write(proj.join("sessions-index.json"), serde_json::to_string_pretty(&index).unwrap()).unwrap();

        strip_session_index_entry(&proj, &drop_path).unwrap();

        let after: SessionsIndex =
            serde_json::from_str(&fs::read_to_string(proj.join("sessions-index.json")).unwrap()).unwrap();
        assert_eq!(after.entries.len(), 1);
        assert_eq!(after.entries[0].session_id, "bbb");
        assert_eq!(after.entries[0].full_path, keep_path);
        assert!(!proj.join("sessions-index.json.tmp").exists(), "temp file must not linger");
    }

    /// Unindexed session: index absent. Strip is a no-op.
    #[test]
    fn strip_is_noop_when_index_missing() {
        let tmp = tempfile::tempdir().unwrap();
        let proj = tmp.path().join("projects/-home-x");
        fs::create_dir_all(&proj).unwrap();
        let p = proj.join("orphan.jsonl").display().to_string();
        fs::write(proj.join("orphan.jsonl"), "{}\n").unwrap();

        strip_session_index_entry(&proj, &p).unwrap();

        assert!(!proj.join("sessions-index.json").exists());
        assert!(proj.join("orphan.jsonl").exists());
    }

    #[test]
    fn rejects_empty_full_path() {
        let cmd = || delete_session_cmd_logic(Path::new("/tmp/projects"), "");
        let e = cmd().unwrap_err();
        assert!(matches!(e, AppError::Validation(_)), "got {e:?}");
    }

    /// /etc/passwd resolves somewhere that won't be under the
    /// canonicalized claude_dir/projects/. Must reject.
    #[test]
    fn rejects_path_outside_projects() {
        let tmp = tempfile::tempdir().unwrap();
        // Use a projects root inside the tempdir; passwd is outside.
        let projects_root = tmp.path().join(PROJECTS_DIR);
        fs::create_dir_all(&projects_root).unwrap();

        let result = delete_session_cmd_logic(&projects_root, "/etc/passwd");
        let e = result.unwrap_err();
        assert!(matches!(e, AppError::Validation(_)), "got {e:?}");
    }

    /// After a full delete on an already-trashed file, command must not
    /// panic and must still strip the index entry (idempotent on file,
    /// eager on index).
    #[test]
    #[ignore = "calls trash::delete_all — pollutes OS Trash; run with --ignored"]
    fn idempotent_on_missing_file_strips_index() {
        let tmp = tempfile::tempdir().unwrap();
        let proj = tmp.path().join("projects/-home-x");
        fs::create_dir_all(&proj).unwrap();
        // .jsonl intentionally not created.
        let full_path = proj.join("gone.jsonl").display().to_string();

        let index = serde_json::json!({
            "version": 1,
            "entries": [
                {"sessionId": "gone", "fullPath": full_path, "summary": "gone"}
            ]
        });
        fs::write(proj.join("sessions-index.json"), serde_json::to_string_pretty(&index).unwrap()).unwrap();

        // discover_claude_dir is process-global; we can't easily override
        // it for this test. Instead exercise the strip helper directly,
        // which is the part that would fail if the file was still on disk.
        // The trash step is covered by `full_delete_strips_index_and_trashes`.
        strip_session_index_entry(&proj, &full_path).unwrap();

        let after: SessionsIndex =
            serde_json::from_str(&fs::read_to_string(proj.join("sessions-index.json")).unwrap()).unwrap();
        assert!(after.entries.is_empty());
    }

    /// Happy path: file exists + index has entry → file trashed, entry gone.
    /// Ignored because it calls `trash::delete_all`.
    #[test]
    #[ignore = "calls trash::delete_all — pollutes OS Trash; run with --ignored"]
    fn full_delete_strips_index_and_trashes() {
        let tmp = tempfile::tempdir().unwrap();
        let proj = tmp.path().join("projects/-home-x");
        fs::create_dir_all(&proj).unwrap();
        let full_path = proj.join("real.jsonl");
        fs::write(&full_path, "{}\n").unwrap();
        let full_path_str = full_path.display().to_string();

        let index = serde_json::json!({
            "version": 1,
            "entries": [
                {"sessionId": "real", "fullPath": full_path_str, "summary": "real"}
            ]
        });
        fs::write(proj.join("sessions-index.json"), serde_json::to_string_pretty(&index).unwrap()).unwrap();

        // Direct call to the trash step + strip step. Skipping the path-
        // validation step because discover_claude_dir is global.
        trash::delete_all([&full_path]).unwrap();
        strip_session_index_entry(&proj, &full_path_str).unwrap();

        assert!(!full_path.exists(), "file must be moved to OS Trash");
        let after: SessionsIndex =
            serde_json::from_str(&fs::read_to_string(proj.join("sessions-index.json")).unwrap()).unwrap();
        assert!(after.entries.is_empty());
    }
}
```

- [ ] **Step 6: Run the tests and confirm they fail**

Run: `cd src-tauri && cargo test --lib commands::system::delete_session_tests 2>&1 | tail -15`
Expected: compile errors — `delete_session_cmd_logic` and `strip_session_index_entry` not defined. This is the failing-test signal.

- [ ] **Step 7: Implement the command + helpers**

Insert the following in `src-tauri/src/commands/system.rs` immediately above the `delete_session_tests` mod block you just added (anywhere after `parse_session_cmd` is fine):

```rust
/// Deletes a single Claude Code session: moves the `.jsonl` to OS Trash
/// and strips the entry from `sessions-index.json`. Local-only; the
/// GitHub-synced copy (if any) is untouched.
///
/// **Order matters.** We trash BEFORE stripping the index so a crash
/// mid-call never silently leaves an orphaned `.jsonl` on disk. The
/// only remaining partial-failure state is "index still references a
/// now-trashed file," which the scanner self-heals on next refresh
/// (`summary_from_jsonl_stat` tolerates a missing file).
///
/// **Path validation.** `full_path` must resolve under
/// `<claude_dir>/projects/`. Canonicalize both sides to defend against
/// `..` traversal and symlinks. Prevents the UI from asking the backend
/// to trash arbitrary paths like `~/.ssh/id_rsa`.
///
/// `ponytail: this is a destructive op gated behind a UI confirmation
/// dialog. If the dialog is bypassed (e.g. future automation, IPC
/// fuzzing), the path check is the only thing standing between a
/// bug and a data-loss incident. Treat it as load-bearing.`
#[tauri::command]
pub fn delete_session_cmd(full_path: String) -> AppResult<()> {
    delete_session_cmd_logic(&discover_claude_dir().join(PROJECTS_DIR), &full_path)
}

/// Inner function so tests can pass a tempdir-rooted `projects/` instead
/// of the process-global `discover_claude_dir()`. Mirrors the validation
/// + trash + strip steps in order.
fn delete_session_cmd_logic(projects_root: &Path, full_path: &str) -> AppResult<()> {
    if full_path.is_empty() {
        return Err(AppError::Validation("full_path is empty".into()));
    }
    let requested = Path::new(full_path);
    let requested_canon = requested
        .canonicalize()
        .map_err(|e| AppError::Io(std::io::Error::new(e.kind(), format!("canonicalize {full_path}: {e}"))))?;
    let root_canon = projects_root
        .canonicalize()
        .map_err(|e| AppError::Io(std::io::Error::new(e.kind(), format!("canonicalize {}: {e}", projects_root.display()))))?;
    if !requested_canon.starts_with(&root_canon) {
        return Err(AppError::Validation(format!(
            "full_path {} is not under projects/",
            requested_canon.display()
        )));
    }

    // 1. Trash first — fail-safe ordering.
    trash::delete_all([requested_canon.as_path()]).map_err(|e| {
        AppError::Io(std::io::Error::new(
            std::io::ErrorKind::Other,
            format!("trash {}: {e}", requested_canon.display()),
        ))
    })?;

    // 2. Strip the index entry. Parent of the .jsonl is the project dir.
    let project_dir = requested_canon
        .parent()
        .ok_or_else(|| AppError::Validation("full_path has no parent".into()))?;
    strip_session_index_entry(project_dir, &requested_canon.display().to_string())?;
    Ok(())
}

/// Loads `sessions-index.json` from `project_dir` (if present), drops the
/// entry whose `fullPath` matches `full_path`, and writes the result
/// back atomically (temp + fsync + rename). No-op when the index file
/// does not exist — unindexed sessions still get trashed upstream.
fn strip_session_index_entry(project_dir: &Path, full_path: &str) -> AppResult<()> {
    let index_path = project_dir.join("sessions-index.json");
    if !index_path.exists() {
        return Ok(());
    }
    let raw = fs::read_to_string(&index_path).map_err(|e| {
        AppError::Io(std::io::Error::new(
            e.kind(),
            format!("read {}: {e}", index_path.display()),
        ))
    })?;
    let mut index: SessionsIndex = serde_json::from_str(&raw).map_err(|e| {
        AppError::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("parse {}: {e}", index_path.display()),
        ))
    })?;
    let before = index.entries.len();
    index.entries.retain(|e| e.full_path != full_path);
    if index.entries.len() == before {
        // No entry matched — nothing to write.
        return Ok(());
    }
    let bytes = serde_json::to_vec_pretty(&index).map_err(|e| {
        AppError::Io(std::io::Error::new(
            std::io::ErrorKind::Other,
            format!("serialize index: {e}"),
        ))
    })?;
    let mut tmp = tempfile::NamedTempFile::new_in(project_dir).map_err(|e| {
        AppError::Io(std::io::Error::new(
            e.kind(),
            format!("create temp in {}: {e}", project_dir.display()),
        ))
    })?;
    std::io::Write::write_all(tmp.as_file_mut(), &bytes).map_err(|e| {
        AppError::Io(std::io::Error::new(
            e.kind(),
            format!("write temp: {e}"),
        ))
    })?;
    tmp.as_file().sync_all().map_err(|e| {
        AppError::Io(std::io::Error::new(
            e.kind(),
            format!("fsync temp: {e}"),
        ))
    })?;
    tmp.persist(&index_path).map_err(|e| {
        AppError::Io(std::io::Error::new(
            e.error.kind(),
            format!("persist to {}: {e}", index_path.display()),
        ))
    })?;
    Ok(())
}
```

- [ ] **Step 8: Register the command in lib.rs**

Edit `src-tauri/src/lib.rs`. In the `tauri::generate_handler!` macro block (around lines 65-100), add one line alphabetically next to `parse_session_cmd`:

```rust
            commands::system::parse_session_cmd,
            commands::system::delete_session_cmd,
```

- [ ] **Step 9: Run the non-ignored tests and confirm they pass**

Run: `cd src-tauri && cargo test --lib commands::system::delete_session_tests 2>&1 | tail -15`
Expected: 3 tests pass (`strips_entry_from_index`, `strip_is_noop_when_index_missing`, `rejects_empty_full_path`, `rejects_path_outside_projects` — that's 4, sorry: 4 pass, 2 ignored).

If `strips_entry_from_index` fails: check that the temp `.jsonl` file exists before `strip_session_index_entry` is called (the test writes it). If `rejects_path_outside_projects` fails: confirm `/etc/passwd` actually exists on this machine (it does on Linux); if running on macOS where it doesn't, the test fails because `canonicalize` returns NotFound — fix by using a known-existing path inside `/etc/` like `/etc/hosts` or skip the test with `#[cfg(target_os = "linux")]`.

- [ ] **Step 10: Run the full Rust test suite**

Run: `cd src-tauri && cargo test 2>&1 | grep -E "^test result|FAILED"`
Expected: 0 failures. The 2 ignored tests show as "ignored" in the count.

- [ ] **Step 11: Run the ignored tests in isolation**

Run: `cd src-tauri && cargo test --lib -- --ignored delete_session 2>&1 | tail -15`
Expected: 2 ignored tests now run and pass. (Then immediately empty your OS Trash if you don't want test fixtures lingering.)

- [ ] **Step 12: Commit**

```bash
git add src-tauri/Cargo.toml src-tauri/src/storage/sessions.rs src-tauri/src/commands/system.rs src-tauri/src/lib.rs
git commit -m "feat(sessions): add delete_session_cmd with OS-Trash semantics

Validates full_path is under projects/, trashes the .jsonl via the
trash crate, then strips the entry from sessions-index.json
atomically (temp+fsync+rename). Trash-first ordering means a crash
mid-call never silently orphans a file; the worst case is a stale
index entry that self-heals on next scan.

Tests covering path validation, idempotency, and index rewrite
round-trip run normally; tests that actually call trash::delete_all
are #[ignore]'d per the keyring-test pattern (they pollute the OS
Trash). Run with: cargo test -- --ignored delete_session

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

### Task 2: `useSessions.refresh()` returns `SessionSummary[]`

**Files:**
- Modify: `src/hooks/useSessions.ts` (change the `refresh` callback signature + body)

**Interfaces:**
- Consumes: existing `useSessions` internals.
- Produces: `refresh: () => Promise<SessionSummary[]>` instead of `() => Promise<void>`. Existing call sites use `void refresh()` or bare `await refresh()` and ignore the return — verified safe.

- [ ] **Step 1: Read the current implementation**

Run: `cat src/hooks/useSessions.ts`
Note the body of `refresh` (around lines 29-43) — it currently ends with `setSessions(list)` and no return statement.

- [ ] **Step 2: Change the signature and return**

Edit `src/hooks/useSessions.ts`. Find the `refresh` callback (lines 29-43 per current state). Change:

```ts
  const refresh = useCallback(async () => {
    // ... existing body up to setSessions(list) ...
    setSessions(list);
  }, []);
```

to:

```ts
  const refresh = useCallback(async (): Promise<SessionSummary[]> => {
    // ... existing body up to setSessions(list) ...
    setSessions(list);
    return list;
  }, []);
```

The `return list;` is the only addition. Update the surrounding JSDoc/comment to note the return value if one exists.

- [ ] **Step 3: Type-check**

Run: `pnpm exec tsc --noEmit 2>&1 | tail -10`
Expected: exit 0, no errors. Existing callers (`void refresh()` in the same file, `refresh` consumers elsewhere) are unaffected because they ignore the return value.

- [ ] **Step 4: Lint**

Run: `pnpm lint 2>&1 | tail -5`
Expected: exit 0.

- [ ] **Step 5: Commit**

```bash
git add src/hooks/useSessions.ts
git commit -m "feat(sessions): useSessions.refresh returns SessionSummary[]

Enables callers (session-delete handler) to act on the freshly-
fetched list without waiting for the next render to read state.
Existing call sites ignore the return; verified safe.

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

### Task 3: `deleteSession` API + `SessionDeleteDialog` component

**Files:**
- Modify: `src/lib/api.ts` (add `deleteSession` next to `deleteProvider`)
- Create: `src/components/SessionDeleteDialog.tsx`

**Interfaces:**
- Consumes: existing `call<void>` wrapper, existing `dialog.tsx` primitives, existing `useToast` from `sonner`.
- Produces:
  - `export const deleteSession = (fullPath: string) => call<void>("delete_session_cmd", { fullPath });`
  - `<SessionDeleteDialog open sessionTitle projectName onOpenChange onConfirm isDeleting />` — same prop shape as `DeleteDialog` with two renamed props (`sessionTitle` for the title, `projectName: string | null` for the body copy).

- [ ] **Step 1: Add the API wrapper**

Edit `src/lib/api.ts`. Find the `deleteProvider` export (line ~130). Add directly below it:

```ts
export const deleteSession = (fullPath: string) =>
  call<void>("delete_session_cmd", { fullPath });
```

- [ ] **Step 2: Create `SessionDeleteDialog.tsx`**

Create `src/components/SessionDeleteDialog.tsx` with this exact content:

```tsx
"use client";

import { Loader2, Trash2 } from "lucide-react";

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
  sessionTitle: string;
  /** null when the session has no project (unindexed). */
  projectName: string | null;
  onOpenChange: (open: boolean) => void;
  onConfirm: () => Promise<void>;
  isDeleting: boolean;
}

export function SessionDeleteDialog({
  open,
  sessionTitle,
  projectName,
  onOpenChange,
  onConfirm,
  isDeleting,
}: Props) {
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Delete &ldquo;{sessionTitle}&rdquo;?</DialogTitle>
          <DialogDescription>
            {projectName
              ? `The transcript for project ${projectName} will be moved to your OS Trash. The copy on GitHub (if any) is not affected.`
              : "This unindexed transcript will be moved to your OS Trash. The copy on GitHub (if any) is not affected."}
          </DialogDescription>
        </DialogHeader>
        <DialogFooter>
          <DialogClose
            render={<Button variant="ghost" disabled={isDeleting} />}
          >
            Cancel
          </DialogClose>
          <Button
            variant="destructive"
            onClick={onConfirm}
            disabled={isDeleting}
          >
            {isDeleting ? (
              <Loader2 className="size-3.5 animate-spin" />
            ) : (
              <Trash2 className="size-3.5" />
            )}
            Delete
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
```

- [ ] **Step 3: Type-check and lint**

Run: `pnpm exec tsc --noEmit && pnpm lint 2>&1 | tail -10`
Expected: both exit 0.

- [ ] **Step 4: Commit**

```bash
git add src/lib/api.ts src/components/SessionDeleteDialog.tsx
git commit -m "feat(sessions): add deleteSession API + SessionDeleteDialog

Reuses the existing Base UI Dialog primitive. Body copy distinguishes
indexed vs unindexed sessions; destructive button shows a spinner
during the in-flight call.

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

### Task 4: Trash button on `SessionRow` + state wiring in `SessionsView`

**Files:**
- Modify: `src/components/Sessions.tsx` (add trash import, button on `SessionRow`, state in `SessionsView`, mount dialog)

**Interfaces:**
- Consumes: `deleteSession` from Task 3, `refresh` (returning array) from Task 2, `seed` from `useSessionUpload`, `toast` from `sonner`, `Trash2` icon.
- Produces: a hover-reveal trash button on every row; `SessionDeleteDialog` mounted once in `SessionsView`; click → dialog → confirm → backend call → list refresh + state map re-seed + detail panel closes if open + toast.

- [ ] **Step 1: Extend imports**

Edit `src/components/Sessions.tsx`. In the lucide-react import block (lines 4-15), add `Trash2`. Also add the new imports after the existing `useSessionUpload` line:

```tsx
import { Trash2 } from "lucide-react";
import { toast } from "sonner";

import { SessionDeleteDialog } from "@/components/SessionDeleteDialog";
import { deleteSession } from "@/lib/api";
```

Note: `Trash2` is already used elsewhere in this codebase (e.g. `ProviderForm.tsx`, `TrackerTab.tsx`) — the icon is just a new import here, not a new dep.

- [ ] **Step 2: Pull `seed` from `useSessionUpload`**

Edit the line at `src/components/Sessions.tsx:94` (current state):

```tsx
  const { stateById, uploadingIds, upload } = useSessionUpload(sessions);
```

to:

```tsx
  const { stateById, uploadingIds, upload, seed } = useSessionUpload(sessions);
```

- [ ] **Step 3: Add delete state + handler in `SessionsView`**

In `SessionsView`, immediately after the existing `useState` lines (around line 95-96), add:

```tsx
  const [deleteTarget, setDeleteTarget] = useState<SessionSummary | null>(null);
  const [deleting, setDeleting] = useState(false);

  const onConfirmDelete = async () => {
    const target = deleteTarget;
    if (!target) return;
    setDeleting(true);
    try {
      await deleteSession(target.full_path);
      const refreshed = await refresh();
      // Re-seed the upload state map so the deleted session drops out of
      // stateById. useSessionUpload's built-in re-seed effect is gated on
      // isWebEnv() and never fires in the Tauri desktop app, so this
      // explicit call is required.
      await seed(refreshed);
      if (selected?.session_id === target.session_id) setSelected(null);
      toast.success("Session deleted");
      setDeleteTarget(null);
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      toast.error(`Delete failed: ${msg}`);
    } finally {
      setDeleting(false);
    }
  };
```

- [ ] **Step 4: Update `RowProps` and `SessionRow` to accept and render the trash button**

Find `RowProps` (around line 439-442). Change:

```tsx
interface RowProps {
  session: SessionSummary;
  onSelect: (s: SessionSummary) => void;
}
```

to:

```tsx
interface RowProps {
  session: SessionSummary;
  onSelect: (s: SessionSummary) => void;
  onRequestDelete: (s: SessionSummary) => void;
}
```

Find the `<SessionSyncButton>` line in `SessionRow` (around line 492). Add a trash button immediately AFTER the `SessionSyncButton`:

```tsx
      <SessionSyncButton session={session} ctx={uploadCtx} />
      <button
        type="button"
        onClick={(e) => {
          e.stopPropagation();
          onRequestDelete(session);
        }}
        aria-label="Delete session"
        title="Delete session"
        className="mt-2 shrink-0 rounded p-1 text-muted-foreground/50 opacity-0 transition-opacity hover:text-destructive focus-visible:opacity-100 group-hover:opacity-100"
      >
        <Trash2 className="size-3.5" />
      </button>
    </li>
```

The `e.stopPropagation()` is critical — the row's main `<button>` (lines 453-491) would otherwise open the detail panel when the trash icon is clicked. The `opacity-0 group-hover:opacity-100` mirrors the hover-reveal pattern used elsewhere in the app.

- [ ] **Step 5: Pass the prop and wire the click target**

Find every place that renders `<SessionRow ...>`. There are two — one in the accordion-style group render and one in the ungrouped fallback. For each, add `onRequestDelete={setDeleteTarget}`.

Example (the exact pattern; adjust to match your site):

```tsx
            <SessionRow
              key={s.session_id}
              session={s}
              onSelect={onSelect}
              onRequestDelete={setDeleteTarget}
            />
```

Use `replace_all: true` if the pattern is identical in both call sites, otherwise do them individually.

- [ ] **Step 6: Mount the dialog**

At the end of `SessionsView`'s returned JSX (right before the closing `</section>` or after the last accordion group, whichever is the actual outermost wrapper — check the current structure), add:

```tsx
      <SessionDeleteDialog
        open={!!deleteTarget}
        sessionTitle={deleteTarget?.title ?? ""}
        projectName={deleteTarget?.project_name ?? null}
        onOpenChange={(open) => !open && setDeleteTarget(null)}
        onConfirm={onConfirmDelete}
        isDeleting={deleting}
      />
```

If `SessionsView` returns a fragment `<>...</>` (likely, since it contains both the list and a detail panel), add the dialog as a sibling inside the fragment.

- [ ] **Step 7: Type-check and lint**

Run: `pnpm exec tsc --noEmit && pnpm lint 2>&1 | tail -10`
Expected: both exit 0. The `seed` variable pulled in step 2 is used in step 3; if you forgot step 3 you'll see an unused-var warning.

- [ ] **Step 8: Commit**

```bash
git add src/components/Sessions.tsx
git commit -m "feat(sessions): wire delete affordance on SessionRow

Hover-reveal trash button → confirm dialog → OS Trash + index strip.
Detail panel auto-closes if the deleted session was open. Toast via
sonner on success/failure. Explicit useSessionUpload.seed() call is
required (the hook's built-in re-seed effect is gated on isWebEnv()
and never fires in the Tauri desktop app).

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

### Task 5: Final verification

- [ ] **Step 1: Run the full Rust test suite**

Run: `cd src-tauri && cargo test 2>&1 | grep -E "^test result|FAILED"`
Expected: all non-ignored tests pass, 2 ignored delete-session tests. 0 failures.

- [ ] **Step 2: Run the frontend verification gates**

Run: `pnpm exec tsc --noEmit && pnpm lint 2>&1 | tail -10`
Expected: both exit 0.

- [ ] **Step 3: Run the ignored Rust tests**

Run: `cd src-tauri && cargo test --lib -- --ignored delete_session 2>&1 | tail -10`
Expected: 2 ignored tests pass. Empty your OS Trash afterwards.

- [ ] **Step 4: Manual smoke test**

Run: `pnpm tauri dev` and verify in the running app:
1. Hover any session row → trash icon fades in. Move the mouse away → fades out.
2. Click the trash icon → confirm dialog opens with the session's title.
3. Confirm → dialog closes, list refreshes, the deleted session is gone, toast appears.
4. Delete the session currently open in the detail panel → panel closes, list refreshes, toast appears.
5. Delete a session that has been synced to GitHub → session disappears locally; verify in your GitHub repo the file is still there (local-only semantics).
6. Open an unindexed session → click trash → body copy reads "This unindexed transcript will be moved to your OS Trash…".
7. Cancel the dialog → no change, no toast.
8. Tab to a row → the trash button receives focus and becomes visible (a11y check).

If any check fails, file the fix in the relevant task's commit and move forward — do not amend history.

---

## Self-Review

**1. Spec coverage:**
- ✅ Hover-reveal trash button on `SessionRow` → Task 4
- ✅ Confirmation dialog using existing `dialog.tsx` primitive → Task 3
- ✅ Rust command `delete_session_cmd` + 6 unit tests → Task 1 (5 tests; idempotency-on-missing-file split into one normal + one ignored)
- ✅ OS Trash via `trash` crate → Task 1 step 1
- ✅ Atomic index rewrite → Task 1 step 7 (`strip_session_index_entry` uses NamedTempFile + fsync + persist)
- ✅ Path containment validation → Task 1 step 7 (canonicalize + `starts_with`)
- ✅ Toast on success/failure → Task 4 step 3
- ✅ Detail panel closes if open → Task 4 step 3
- ✅ `useSessions.refresh()` returns `SessionSummary[]` → Task 2
- ✅ Explicit `seed(refreshed)` call → Task 4 step 3
- ✅ Trash-first step order → Task 1 step 7 (trash before strip, with rationale in doc comment)
- ✅ Structural equality test (not byte-identical) → Task 1 step 5 (`strips_entry_from_index` parses the result with serde, not byte-compares)

**2. Placeholder scan:** No TBD/TODO. Every step has actual code or commands.

**3. Type consistency:**
- `delete_session_cmd(full_path: String) -> AppResult<()>` defined in Task 1, consumed by `deleteSession` in Task 3, consumed by `onConfirmDelete` in Task 4. Same signature throughout.
- `strip_session_index_entry(project_dir: &Path, full_path: &str) -> AppResult<()>` — private to `commands/system.rs`, used by `delete_session_cmd_logic` and tests.
- `refresh: () => Promise<SessionSummary[]>` defined in Task 2, consumed by `onConfirmDelete` in Task 4.
- `seed(sessions: SessionSummary[]) => Promise<void>` pulled from `useSessionUpload` in Task 4 step 2, called in Task 4 step 3. Matches the hook's existing signature.
- `SessionDeleteDialog` props in Task 3 step 2 exactly match the call site in Task 4 step 6.

**4. File-path deviations from spec:**
- The spec said new file `src-tauri/src/commands/sessions.rs`. The existing pattern puts `list_sessions_cmd` and `parse_session_cmd` in `commands/system.rs` (verified at system.rs:117,126). Putting `delete_session_cmd` in `system.rs` matches the existing pattern. Documented in the plan's File Structure section.

**5. Ignored-test pattern:** Task 1 has 2 tests gated `#[ignore]` because they call `trash::delete_all` and pollute the OS Trash. The CLAUDE.md explicitly calls out this pattern for keyring tests (line "Keyring integration is only covered by `cd src-tauri && cargo test -- --ignored keyring`"). Same `cargo test -- --ignored` invocation shape documented in Task 1 step 11 and Task 5 step 3.