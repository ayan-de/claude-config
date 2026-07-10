# Delete sessions from the Sessions view — Design

> **For agentic workers:** Use superpowers:writing-plans to turn this spec into a task-by-task implementation plan, then execute.

**Goal:** Let the user delete a Claude Code session (`.jsonl` transcript) from the Sessions view, with a confirmation dialog and OS-Trash semantics. Local-only — the GitHub-synced copy (if any) is untouched.

## Scope

In scope:
- Hover-reveal trash button on every `SessionRow` in `src/components/Sessions.tsx`.
- Confirmation dialog using the existing `src/components/ui/dialog.tsx` primitive.
- Rust command `delete_session_cmd` that trashes the `.jsonl` (via the `trash` crate) and strips the matching entry from `sessions-index.json` (atomic temp+rename).
- Optimistic UI: list refresh, detail panel closes if the deleted session was open, toast via the existing `sonner` provider.

Out of scope:
- Deleting the GitHub-synced copy from the remote repo.
- Undo / restore from trash (the OS Trash UI is the restore surface).
- Bulk delete (select-multiple + delete-all).
- Deleting the project folder itself.

## Architecture

### Rust backend

New file: `src-tauri/src/commands/sessions.rs` — Tauri command surface.

```rust
#[tauri::command]
pub fn delete_session_cmd(
    state: tauri::State<'_, AppState>,
    full_path: String,
) -> AppResult<()>
```

Responsibilities (in this order — trash first so a crash never silently leaves an orphaned file on disk):
1. Validate `full_path` is non-empty.
2. Validate `full_path` resolves under `<claude_dir>/projects/` (security: prevent the UI from asking the backend to trash arbitrary paths like `~/.ssh/id_rsa`). Uses `std::path::Path::starts_with` on canonicalized forms.
3. Call `trash::delete_all([full_path])` — idempotent on a missing file (trash crate returns Ok if the file is gone).
4. Resolve the project directory: the parent of the `.jsonl` file is the project directory; `sessions-index.json` lives there.
5. If `sessions-index.json` is present, load it, drop the entry whose `fullPath == full_path`, and save atomically (write to `sessions-index.json.tmp`, `fsync`, rename — same pattern as `storage/settings.rs`). If absent (unindexed session): skip silently.

**Why trash-first:** the failure mode of "strip index, then crash before trashing" leaves an orphaned `.jsonl` invisible to the UI — the user's data is gone from their perspective but actually recoverable on disk. The failure mode of "trash first, then crash before stripping" leaves a stale index entry that the scanner self-heals on next refresh (`summary_from_jsonl_stat` and the jsonl-fallback loop both tolerate a missing `.jsonl`). Trash-first never silently loses data.

Errors:
- `AppError::Validation` — empty path or path outside `projects/`.
- `AppError::Io` — I/O error reading/writing the index or invoking trash.

New dep in `src-tauri/Cargo.toml`:
```toml
trash = "5"
```
The crate uses `gio trash` on Linux (via D-Bus, falls back to `trash-cli`'s `trash-put` if `gio` is missing), `NSWorkspace` on macOS, and `SHFileOperation` with `FOF_ALLOWUNDO` on Windows. One small crate replaces ~100 lines of platform-specific code.

`models.rs`: no changes. `SessionSummary.full_path` is the input.

### Frontend

`src/lib/api.ts` — one new function:

```ts
export const deleteSession = (fullPath: string) =>
  call<void>("delete_session_cmd", { fullPath });
```

`src/components/SessionDeleteDialog.tsx` — new file (~50 lines).

Mirror `DeleteDialog.tsx` exactly, parameterize the copy:

| Prop | Type | Purpose |
|---|---|---|
| `open` | `boolean` | Dialog open state |
| `sessionTitle` | `string` | Truncated title for the confirm line |
| `projectName` | `string \| null` | Project folder slug for the body copy (null when unindexed) |
| `onOpenChange` | `(open: boolean) => void` | Close handler |
| `onConfirm` | `() => Promise<void>` | Confirm handler (sets parent isDeleting) |
| `isDeleting` | `boolean` | Disables buttons + shows spinner |

Copy:
- **Title:** `Delete "<sessionTitle>"?`
- **Body (indexed):** "The transcript will be moved to your OS Trash. The copy on GitHub (if any) is not affected."
- **Body (unindexed, `projectName == null`):** "This unindexed transcript will be moved to your OS Trash. The copy on GitHub (if any) is not affected."
- **Cancel:** ghost button.
- **Delete:** destructive button + `Loader2` spinner during `isDeleting`.

`src/components/Sessions.tsx` — three additions:

1. **`SessionRow`** gets a trash button rendered after `SessionSyncButton`:
   ```tsx
   <button
     type="button"
     onClick={(e) => { e.stopPropagation(); onRequestDelete(session); }}
     aria-label="Delete session"
     className="mt-2 shrink-0 rounded p-1 text-muted-foreground/50 opacity-0 transition-opacity hover:text-destructive focus-visible:opacity-100 group-hover:opacity-100"
   >
     <Trash2 className="size-3.5" />
   </button>
   ```
   `e.stopPropagation()` is critical — the row's main `<button>` would otherwise open the detail panel.

2. **`RowProps`** gets `onRequestDelete: (s: SessionSummary) => void`.

3. **`SessionsView`** orchestrates state:
   ```tsx
   const { sessions, loading, refresh } = useSessions();
   const { stateById, uploadingIds, upload, seed } = useSessionUpload(sessions);
   const [deleteTarget, setDeleteTarget] = useState<SessionSummary | null>(null);
   const [deleting, setDeleting] = useState(false);

   const onConfirmDelete = async () => {
     if (!deleteTarget) return;
     setDeleting(true);
     try {
       await deleteSession(deleteTarget.full_path);
       // refresh() returns the freshly-fetched sessions array (small
       // change to useSessions.ts — current implementation returns void,
       // we extend it to return SessionSummary[]).
       const refreshed = await refresh();
       // Re-seed the upload state map so the deleted session drops out of
       // stateById. The existing re-seed effect in useSessionUpload is
       // gated on isWebEnv() and never fires in the Tauri desktop app,
       // so this explicit call is required, not optional.
       await seed(refreshed);
       if (selected?.session_id === deleteTarget.session_id) setSelected(null);
       toast.success("Session deleted");
       setDeleteTarget(null);
     } catch (e) {
       toast.error(`Delete failed: ${e instanceof Error ? e.message : String(e)}`);
     } finally {
       setDeleting(false);
     }
   };
   ```

   **Required supporting change** — `src/hooks/useSessions.ts`:
   `refresh` currently returns `void`. Change the signature and body so it returns `Promise<SessionSummary[]>` (the same `list` it sets into state). One-line change:
   ```ts
   const refresh = useCallback(async (): Promise<SessionSummary[]> => {
     // ... existing body ...
     setSessions(list);
     return list;
   }, []);
   ```
   No other caller relies on the void return today (callers `void refresh()` ignore it either way).

4. **`SessionsView`** mounts `<SessionDeleteDialog>` once, after the existing list render:
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

5. The `useSessionUpload` hook should clear its in-memory state for the deleted session. The hook exposes `seed(sessions)` which re-derives `stateById` from the current list. After `refresh()` from `useSessions` returns the new (smaller) list, call `seed(newSessions)` to drop the deleted session from the upload state map. No special API on the upload hook — it's already designed for this.

No changes to:
- `src/lib/types.ts` (uses existing `SessionSummary`).
- `src-tauri/src/models.rs`.
- `src/components/ui/dialog.tsx` (reused as-is).

## Data flow

```
User hovers SessionRow
  → Trash2 fades in (opacity-0 → opacity-100)
User clicks Trash2
  → e.stopPropagation()
  → SessionsView.setDeleteTarget(session)
  → SessionDeleteDialog opens (open=true)
User confirms
  → onConfirmDelete runs
    → await deleteSession(full_path)
      → Tauri invoke("delete_session_cmd", { fullPath })
        → Rust: validate path → trash::delete_all(.jsonl) → strip from sessions-index.json
    → const refreshed = await refresh() — refresh returns the new SessionSummary[]
    → await seed(refreshed) — re-derive stateById so deleted id drops out
    → if selected.session_id matches, setSelected(null) — closes detail panel
    → toast.success("Session deleted")
    → setDeleteTarget(null) — closes dialog
On error
  → toast.error(...) — user sees the failure
  → Dialog stays open (deleteTarget not nulled)
  → isDeleting=false — buttons re-enabled
```

## Error handling

| Failure | Behavior |
|---|---|
| `full_path` empty | `AppError::Validation` → toast "Invalid session path". No file touched. |
| `full_path` outside `projects/` | `AppError::Validation` → toast "Invalid session path". No file touched. |
| `trash::delete_all` fails | `AppError::Io` → toast "Delete failed: ...". No file touched (trash-first). User can retry safely. |
| `sessions-index.json` malformed JSON | `AppError::Io` → toast "Delete failed: ...". The `.jsonl` is already trashed (trash-first); the stale index entry self-heals on next scanner refresh. |
| `.jsonl` already missing | `trash::delete_all` returns Ok (idempotent). Index strip proceeds. |
| `sessions-index.json` missing | Skipped silently — unindexed session. |

Trash-first means the only partial-failure state is "stale index entry that self-heals on next scan." We never silently lose a file the user expected to delete.

## Testing

### Rust (`cargo test`)

In `src-tauri/src/storage/sessions.rs` (or the new `commands/sessions.rs` if command-level testing):

1. `delete_session_cmd strips entry from sessions-index.json` — index has 3 entries; delete the middle one; assert the reloaded index has 2 entries with the correct ids; assert no temp file (`sessions-index.json.tmp`) is left behind.
2. `delete_session_cmd unindexed: no index write, file removed` — only a .jsonl under `projects/-x/` with no `sessions-index.json`; command returns Ok; .jsonl is gone.
3. `delete_session_cmd idempotent on missing file` — .jsonl already gone; command returns Ok; no panic.
4. `delete_session_cmd rejects empty full_path` — returns `AppError::Validation`.
5. `delete_session_cmd rejects path outside projects/` — `full_path = /etc/passwd`; returns `AppError::Validation`.
6. `delete_session_cmd preserves other index entries (structural)` — index has 3 entries; delete the second; parse both before and after into typed `SessionsIndex` and assert that the entries with the surviving ids are field-by-field equal (NOT byte-identical — serde_json round-trip does not preserve key order, whitespace, or float formatting).

**About the round-trip:** the implementation deserializes the index via `serde_json::from_str`, mutates the entries Vec, and reserializes via `serde_json::to_writer_pretty` (or `to_string`). It does NOT do surgical in-place JSON editing. Tests asserting byte-identity would be flaky. Structural equality on parsed values is the right invariant.

Note: `trash::delete_all` is a thin wrapper around the platform's tested native API. Tests assert observable side effects (index rewrites, file existence) and do not mock the trash crate. If a CI environment has no trash facility, add a `#[ignore]` attribute on the affected test, matching the project's existing `keyring` integration-test pattern.

### Frontend (`pnpm exec tsc --noEmit && pnpm lint`)

No JS unit-test runner per `CLAUDE.md`. Type-check and lint gates only. The deletion UX is verified manually.

### Manual smoke test (`pnpm tauri dev`)

1. Open a session in the detail panel, click trash → confirm → panel closes, list refreshes, toast appears, `~/.local/share/Trash/files/` (Linux) / `~/.Trash/` (macOS) contains the `.jsonl`.
2. Open a session synced to GitHub, delete it locally → `sessions-index.json` no longer references it, but the GitHub repo still has the file.
3. Open an unindexed session (no project) → delete works without touching any index file.
4. Verify hover behavior: trash icon is invisible at rest, fades in on row hover, fades out on row un-hover.
5. Verify keyboard a11y: tab to a row, the trash button receives focus → opacity-100.
6. Verify cancel: click trash → cancel → no state change, list unchanged, no toast.
7. Verify failure path: temporarily make the .jsonl read-only (`chmod 444`) and try to delete → expect toast "Delete failed" and the row stays.

## Files touched

| File | Role |
|---|---|
| `src-tauri/Cargo.toml` | Add `trash = "5"` dep |
| `src-tauri/src/commands/sessions.rs` | New: `delete_session_cmd` + helpers + tests |
| `src-tauri/src/lib.rs` | Register the new command |
| `src/lib/api.ts` | Add `deleteSession` |
| `src/components/SessionDeleteDialog.tsx` | New: confirm dialog |
| `src/components/Sessions.tsx` | Trash button + state + dialog mount |

No new IPC types. No schema migrations. No changes to `models.rs` or `types.ts`.