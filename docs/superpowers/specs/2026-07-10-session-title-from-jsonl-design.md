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

- Title shown in the list row matches what `/resume` *intends* to show
  for the same session: the user-set title always wins over the
  auto-generated one, even when the user's rename has been pushed out
  of the 64KB tail window by subsequent activity.
- Unindexed sessions stop showing `(unindexed) <uuid>` when the `.jsonl`
  contains a `customTitle` or `aiTitle`.
- The 8-character UUID tag in the list row is removed; the full UUID
  remains in the detail header.
- `SessionSummary.title` field semantics don't change — no new IPC, no new
  types, no frontend changes other than the row layout.

## Non-goals

- Indexing the title reads into a cache. The head+tail 64KB read per
  session (≤128KB total) is cheap enough at current scale.
- Per-session title invalidation. `useSessions` already re-fetches on
  mount; the title re-reads with the rest.
- A separate IPC command for the title. The override happens inside the
  existing `scan_sessions`.
- Frontend title writes. Users still rename via `/rename`; this app only
  reads.
- Whole-file scan of the `.jsonl` to defeat the upstream 64KB eviction.
  Head+tail (128KB total) is enough for ~all real sessions. Revisit
  only if a user reports a wrong title on a session where the title
  has been pushed out of *both* 64KB windows.

## Design

### 1. Backend: extract title from the `.jsonl` (Rust)

New helper in `src-tauri/src/storage/sessions.rs`:

```rust
/// Reads the first and last 64KB of a transcript and returns the
/// user-set or auto-generated title. Type-first priority: the user's
/// rename always wins over the AI title, even when the rename has
/// been pushed out of the tail 64KB window by later activity.
///
/// Priority: customTitle (tail) → customTitle (head) → aiTitle (tail)
/// → aiTitle (head) → None.
///
/// `ponytail: head+tail 64KB windows. Differs from upstream /resume's
/// bucket-first order (tail→head, types interleaved) by checking
/// customTitle across both buffers before falling through to aiTitle.
/// Fixes the silent-rename-loss case where a /rename gets tail-evicted
/// and a later aiTitle is in the tail. Same I/O, strictly better.`
fn extract_title_from_jsonl(path: &Path) -> Option<String>
```

The helper is hand-rolled — open the file, seek to EOF, read up to 64KB
backwards, then seek to 0 and read up to 64KB forwards. Scan both
buffers for the last occurrence of `"customTitle":"…"` and
`"aiTitle":"…"` in the priority order above. The line-scanning mirrors
`extractLastJsonStringField` upstream (a left-to-right `indexOf` loop,
last-one-wins per buffer), so we get the same `last-wins` behavior
without depending on Node-side code or shelling out.

Title precedence in the helper (type-first, not bucket-first):

```
customTitle  (from tail)   // /rename, recent
  ↓
customTitle  (from head)   // /rename, tail-evicted — STILL wins
  ↓
aiTitle      (from tail)   // Claude auto-titled, recent
  ↓
aiTitle      (from head)   // Claude auto-titled, head only
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
                  ▼  + .jsonl head+tail 64KB read for title override
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
- **Head fallback for tail-evicted titles**: the helper reads BOTH
  the head and tail 64KB windows, and checks `customTitle` in the
  head *before* falling through to `aiTitle` in the tail. This
  closes the silent-rename-loss case where `/rename`'d titles get
  pushed out of the tail by later activity, then a fresh `aiTitle`
  lands in the tail. (This is a deliberate improvement over
  upstream `/resume`'s bucket-first order, at zero extra I/O cost.)
- **Title present in both index `summary` and `.jsonl` `aiTitle`**:
  `aiTitle` wins (it was set later, it's the better signal of what the
  session is *about*).
- **User renamed the session multiple times** (`/rename` × N):
  last-one-wins extracts the most recent `customTitle`.
- **Title has been pushed out of BOTH 64KB windows** (≈ requires
  thousands of post-title messages): the helper returns `None`;
  the existing index `summary` / `first_prompt` fallback chain takes
  over. Document the ponytail ceiling in the comment and bump to
  whole-file scan if a user reports it.

## Out of scope (ponytail: deliberately deferred)

- Whole-file scan to defeat BOTH-window eviction. Add when a user
  reports a wrong title on a session where the title has been pushed
  past both the head and tail 64KB windows.
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
  - **`customTitle` only in head, `aiTitle` only in tail → returns
    `customTitle` (the bucket-order fix; this is the case that fails
    under /resume's bucket-first order)**
  - no title at all → returns `None`
  - title with escaped quotes → returns the unescaped string
- Manual: run the app, verify a renamed session shows the renamed
  title, an auto-titled session shows the auto title, an unindexed
  session with a `customTitle` shows that title, and a truly
  title-less unindexed session still shows the `(unindexed) <uuid>`
  placeholder.
- `pnpm exec tsc --noEmit` and `pnpm lint` pass.
- `cd src-tauri && cargo test` passes.
