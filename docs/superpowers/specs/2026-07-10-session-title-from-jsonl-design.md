# Show real session titles in the Sessions view

Date: 2026-07-10

## Problem

The Sessions list currently shows the **first user prompt** (from
`sessions-index.json`'s `summary` field) as the row title, with a truncated
8-character session UUID underneath. When Claude Code has auto-titled the
session (`ai-title` entry) or the user renamed it (`custom-title` entry via
`/rename`), the UI still shows the first prompt — the same "wrong" name the
upstream `/resume` picker has historically suffered from.

For sessions not in the index at all, the title is the literal placeholder
`(unindexed) <uuid>`, which is the worst of the three: the user has no
readable label for the session.

The user wants the same human-readable title that `/resume` shows — the
`customTitle` / `aiTitle` from inside the `.jsonl` itself.

## Goals

- Title shown in the list row matches what `/resume` shows for the same
  session.
- Unindexed sessions stop showing `(unindexed) <uuid>` when the `.jsonl`
  contains a `customTitle` or `aiTitle`.
- The 8-character UUID tag in the list row is removed; the full UUID
  remains in the detail header.
- `SessionSummary.title` field semantics don't change — no new IPC, no new
  types, no frontend changes other than the row layout.

## Non-goals

- Indexing the title reads into a cache. The 64KB tail-read per session is
  cheap enough at current scale.
- Per-session title invalidation. `useSessions` already re-fetches on
  mount; the title re-reads with the rest.
- A separate IPC command for the title. The override happens inside the
  existing `scan_sessions`.
- Frontend title writes. Users still rename via `/rename`; this app only
  reads.
- Whole-file scan of the `.jsonl` to defeat the upstream 64KB eviction.
  Tail-only matches `/resume` and is good enough for ~all real sessions.
  Revisit if a user reports a wrong title on a long session.

## Design

### 1. Backend: extract title from the `.jsonl` (Rust)

New helper in `src-tauri/src/storage/sessions.rs`:

```rust
/// Reads the first and last 64KB of a transcript and returns the
/// user-set or auto-generated title, matching `/resume`'s precedence.
/// Two-bucket read; same shape as upstream's `readSessionLite`.
///
/// Priority: customTitle (tail) → aiTitle (tail) → customTitle (head)
/// → aiTitle (head) → None.
///
/// `ponytail: head+tail 64KB windows, mirrors upstream /resume. Loses
/// titles that scrolled past BOTH windows (≈ rare, requires thousands
/// of post-title messages). Bump to whole-file scan if a user reports
/// a wrong title on a long session.`
fn extract_title_from_jsonl(path: &Path) -> Option<String>
```

The helper is hand-rolled — open the file, seek to EOF, read up to 64KB
backwards, then seek to 0 and read up to 64KB forwards. Scan both
buffers for the last occurrence of `"customTitle":"…"` and
`"aiTitle":"…"` in priority order. The line-scanning mirrors
`extractLastJsonStringField` upstream (a left-to-right `indexOf` loop,
last-one-wins), so we get identical behavior without depending on
Node-side code or shelling out.

Title precedence in the helper:

```
customTitle  (from tail)   // /rename'd, wins
  ↓
aiTitle      (from tail)   // Claude auto-titled
  ↓
customTitle  (from head)   // tail-evicted customTitle, head fallback
  ↓
aiTitle      (from head)   // tail-evicted aiTitle
  ↓
None
```

Truncate to `TITLE_MAX_CHARS = 200` chars before returning, same as the
existing index-based title truncation.

### 2. Backend: wire it into the existing scanner

`scan_sessions` already has two paths; the override goes in both:

**`merge_index_into`** (line ~178): after the existing
`pick_title(entry.summary, entry.first_prompt)` call, if the entry has a
`full_path` on disk, call `extract_title_from_jsonl` and use its result
if `Some`. The index's `summary` / `first_prompt` remain as fallbacks
when the `.jsonl` read fails or has no title.

**`summary_from_jsonl_stat`** (line ~208): replace the hard-coded
`format!("(unindexed) {}", session_id)` placeholder with
`extract_title_from_jsonl(&jsonl).unwrap_or_else(|| format!("(unindexed) {}", session_id))`.
This is the change that makes the placeholder disappear for sessions that
*do* have a title in their `.jsonl` but are missing from the index.

No new types, no new fields. `SessionSummary.title` is the same field with
a better value.

### 3. Frontend: drop the 8-char ID tag in the list row

In `src/components/Sessions.tsx`:

- **Line 467** (`SessionRow`): remove the
  `<span className="font-mono tabular-nums">{session.session_id.slice(0, 8)}</span>`
  block **and** its trailing `·` separator. Keep the message count,
  time-ago, and project name + folder icon.
- **Line 595** (`SessionDetail`): keep the full `session.session_id` in
  the mono detail header. The detail view is the only place that needs
  the full UUID for copy/reference.

No TypeScript type changes, no API changes, no hook changes.

## Data flow after the change

```
~/.claude/projects/<encoded-dir>/sessions-index.json
                  │
                  ▼  + .jsonl tail-64KB read for title override
src-tauri/src/storage/sessions.rs::scan_sessions()
                  │
                  ▼  Vec<SessionSummary>  (title field is now real)
src-tauri/src/commands/system.rs::list_sessions_cmd()
                  │
                  ▼  invoke("list_sessions_cmd")
src/lib/api.ts::listSessions()
                  │
                  ▼  useSessions()  (one-shot on mount)
                  │
                  ▼  SessionsView → SessionGroup → SessionRow
                       (SessionDetail via useSessionTranscript)
```

The data flow diagram is identical to today's — only the contents of the
`title` field change, and one small UI line goes away.

## Edge cases

- **`.jsonl` missing on disk** (index entry references a deleted file):
  the override helper returns `None`; index's `summary` / `first_prompt`
  is used. Today this same scenario is already handled because the
  scanner does no I/O against the `.jsonl` here.
- **`.jsonl` smaller than 64KB**: the head and tail buffers overlap;
  the helper deduplicates and proceeds normally.
- **Title contains escaped characters** (`\"`, `\\`): the
  last-one-wins extractor handles JSON string escapes; the Rust
  implementation will mirror the same approach.
- **Title present in both index `summary` and `.jsonl` `aiTitle`**:
  `aiTitle` wins (it was set later, it's the better signal of what the
  session is *about*).
- **User renamed the session multiple times** (`/rename` × N):
  last-one-wins extracts the most recent `customTitle`.
- **Head fallback for tail-evicted titles**: the 64KB tail is the
  primary window. Adding the head fallback is a follow-up if a user
  reports a long session showing the wrong title; document the
  ponytail ceiling and the upgrade path in the comment.

## Out of scope (ponytail: deliberately deferred)

- Whole-file scan to defeat 64KB tail eviction. Add when a user reports
  a wrong title on a long session.
- Frontend cache for title reads. Add when `scan_sessions` becomes a
  bottleneck on large project histories.
- Indexing the title into `sessions-index.json` (writing back). User
  renames still go through `/rename`; this app only reads.
- New IPC command for title. The override happens server-side inside
  the scanner.
- Showing the title as a "rename" affordance in the UI.

## Verification

- Add unit tests in `src-tauri/src/storage/sessions.rs` for
  `extract_title_from_jsonl`:
  - `customTitle` only → returns it
  - `aiTitle` only → returns it
  - both `customTitle` and `aiTitle` → returns the last `customTitle`
  - `customTitle` appears twice → returns the later one
  - no title at all → returns `None`
  - title with escaped quotes → returns the unescaped string
- Manual: run the app, verify a renamed session shows the renamed
  title, an auto-titled session shows the auto title, an unindexed
  session with a `customTitle` shows that title, and a truly
  title-less unindexed session still shows the `(unindexed) <uuid>`
  placeholder.
- `pnpm exec tsc --noEmit` and `pnpm lint` pass.
- `cd src-tauri && cargo test` passes.
