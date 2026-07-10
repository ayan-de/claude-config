# Session title from .jsonl — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Show the real session title (`customTitle` / `aiTitle` from the `.jsonl` transcript) in the Sessions list, instead of the first prompt or a UUID placeholder.

**Architecture:** Add a Rust helper `extract_title_from_jsonl()` in `src-tauri/src/storage/sessions.rs` that reads the head + tail 64KB of each `.jsonl` and returns the title with type-first priority (`customTitle` wins across both buffers, then `aiTitle`). Wire it into both code paths of `scan_sessions` so it overrides the index's `summary` / `first_prompt` for indexed sessions, and replaces the `(unindexed) <uuid>` placeholder for unindexed sessions. Drop the 8-char UUID tag from the list row in `Sessions.tsx`; keep the full UUID in the detail header.

**Tech Stack:** Rust (Tauri 2 backend), Next.js 16 + React 19 frontend, Tailwind v4, `serde_json` for unescaping.

## Global Constraints

- Rust toolchain: 1.77.2 (per `src-tauri/Cargo.toml`).
- Frontend framework: Next.js 16 static export (no SSR, no API routes).
- `pnpm tauri dev` for real app work; `pnpm dev` only shows the browser stub.
- Verification gates: `pnpm lint`, `pnpm exec tsc --noEmit`, `cd src-tauri && cargo test`.
- `SessionSummary.title` field semantics MUST NOT change — same type, same IPC command, no new fields.
- Type-first priority: `customTitle (tail) → customTitle (head) → aiTitle (tail) → aiTitle (head) → None`. Do NOT replicate upstream `/resume`'s bucket-first order.
- Head+tail windows are 64KB each, 128KB total per session max. Whole-file scan is out of scope.

## File Structure

| File | Role | Touched in |
|---|---|---|
| `src-tauri/src/storage/sessions.rs` | Session scanner; new `extract_title_from_jsonl` helper lives here; both override points (`merge_index_into`, `summary_from_jsonl_stat`) edited here; unit tests added in the existing `mod tests` | Task 1, 2, 3 |
| `src/components/Sessions.tsx` | Frontend session list; one span removed from `SessionRow` | Task 4 |

No new files. No new IPC commands. No new types.

---

### Task 1: Add `extract_title_from_jsonl` with TDD coverage

**Files:**
- Modify: `src-tauri/src/storage/sessions.rs` (add the helper, the two private helpers it uses, and unit tests in the existing `mod tests`)

**Interfaces:**
- Consumes: `Path` to a `.jsonl` transcript, the existing `TITLE_MAX_CHARS` constant.
- Produces:
  - `pub fn extract_title_from_jsonl(path: &Path) -> Option<String>` — returns the title truncated to `TITLE_MAX_CHARS` chars, or `None` when no title entry exists in either buffer. Type-first priority.
  - `fn extract_last_string_field(text: &str, key: &str) -> Option<String>` — private; finds the last `"<key>":"…"` or `"<key>": "…"` substring in `text` and returns the unescaped value, or `None`.
  - `fn unescape_json_string(raw: &str) -> String` — private; parses a JSON string literal body and returns the unescaped string, falling back to `raw` on parse error.
  - `fn read_tail_bytes(path: &Path, max_bytes: u64) -> std::io::Result<Vec<u8>>` — private; returns the last `max_bytes` of the file, or the full file if smaller. When the read does not start at byte 0, the first (partial) line is discarded so the result is line-aligned.

- [ ] **Step 1: Add the 7 failing unit tests**

Append the following inside the existing `#[cfg(test)] mod tests { ... }` block in `src-tauri/src/storage/sessions.rs` (anywhere after the existing `write_transcript` helper). Use the existing `write_transcript(dir, name, content) -> PathBuf` helper for file creation, and `tempfile::tempdir()` for the temp dir.

```rust
    // ---- extract_title_from_jsonl ----

    #[test]
    fn title_from_jsonl_returns_custom_title() {
        let tmp = tempfile::tempdir().unwrap();
        let path = write_transcript(
            tmp.path(),
            "s.jsonl",
            "{\"type\":\"user\",\"message\":{\"role\":\"user\",\"content\":\"hi\"}}\n\
             {\"type\":\"custom-title\",\"customTitle\":\"My Renamed Session\"}\n",
        );
        assert_eq!(
            extract_title_from_jsonl(&path).as_deref(),
            Some("My Renamed Session"),
        );
    }

    #[test]
    fn title_from_jsonl_returns_ai_title_when_no_custom_title() {
        let tmp = tempfile::tempdir().unwrap();
        let path = write_transcript(
            tmp.path(),
            "s.jsonl",
            "{\"type\":\"user\",\"message\":{\"role\":\"user\",\"content\":\"hi\"}}\n\
             {\"type\":\"ai-title\",\"aiTitle\":\"Auto generated title\"}\n",
        );
        assert_eq!(
            extract_title_from_jsonl(&path).as_deref(),
            Some("Auto generated title"),
        );
    }

    #[test]
    fn title_from_jsonl_prefers_custom_over_ai() {
        let tmp = tempfile::tempdir().unwrap();
        let path = write_transcript(
            tmp.path(),
            "s.jsonl",
            "{\"type\":\"ai-title\",\"aiTitle\":\"Auto\"}\n\
             {\"type\":\"custom-title\",\"customTitle\":\"Manual\"}\n",
        );
        assert_eq!(extract_title_from_jsonl(&path).as_deref(), Some("Manual"));
    }

    #[test]
    fn title_from_jsonl_uses_last_custom_title_when_renamed_twice() {
        let tmp = tempfile::tempdir().unwrap();
        let path = write_transcript(
            tmp.path(),
            "s.jsonl",
            "{\"type\":\"custom-title\",\"customTitle\":\"First rename\"}\n\
             {\"type\":\"custom-title\",\"customTitle\":\"Second rename\"}\n",
        );
        assert_eq!(
            extract_title_from_jsonl(&path).as_deref(),
            Some("Second rename"),
        );
    }

    /// The bucket-order fix: customTitle is in the head 64KB only
    /// (tail-evicted), aiTitle is in the tail. Type-first priority
    /// must return the customTitle regardless of position.
    #[test]
    fn title_from_jsonl_prefers_head_custom_title_over_tail_ai_title() {
        let tmp = tempfile::tempdir().unwrap();
        // Build a file large enough that head and tail don't overlap.
        // 80KB of padding (between 64KB and 128KB) forces any 64KB
        // window to see only one of the two entries.
        let mut content = String::new();
        content.push_str("{\"type\":\"custom-title\",\"customTitle\":\"User rename\"}\n");
        // Pad to push the aiTitle past the head 64KB window.
        let pad_lines = 80 * 1024 / 64; // 1280 lines of ~64 bytes
        for i in 0..pad_lines {
            content.push_str(&format!(
                "{{\"type\":\"user\",\"message\":{{\"role\":\"user\",\"content\":\"message {} that is long enough to push the line past a boundary\"}}}}\n",
                i
            ));
        }
        content.push_str("{\"type\":\"ai-title\",\"aiTitle\":\"Auto title\"}\n");
        let path = write_transcript(tmp.path(), "s.jsonl", &content);

        // Sanity: file is > 64KB so the two windows don't fully overlap.
        let len = fs::metadata(&path).unwrap().len();
        assert!(len > 64 * 1024, "test fixture should exceed 64KB; got {} bytes", len);

        assert_eq!(
            extract_title_from_jsonl(&path).as_deref(),
            Some("User rename"),
        );
    }

    #[test]
    fn title_from_jsonl_returns_none_when_no_title() {
        let tmp = tempfile::tempdir().unwrap();
        let path = write_transcript(
            tmp.path(),
            "s.jsonl",
            "{\"type\":\"user\",\"message\":{\"role\":\"user\",\"content\":\"hi\"}}\n",
        );
        assert_eq!(extract_title_from_jsonl(&path), None);
    }

    #[test]
    fn title_from_jsonl_unescapes_quoted_title() {
        let tmp = tempfile::tempdir().unwrap();
        let path = write_transcript(
            tmp.path(),
            "s.jsonl",
            "{\"type\":\"custom-title\",\"customTitle\":\"He said \\\"hi\\\"\"}\n",
        );
        assert_eq!(
            extract_title_from_jsonl(&path).as_deref(),
            Some("He said \"hi\""),
        );
    }
```

- [ ] **Step 2: Run tests and confirm they fail to compile**

Run: `cd src-tauri && cargo test --lib storage::sessions::tests::title_from_jsonl 2>&1 | tail -20`
Expected: compile error — `extract_title_from_jsonl` is not defined. This is the failing-test signal.

- [ ] **Step 3: Implement the four functions**

Add the following immediately above the existing `#[cfg(test)] mod tests` block in `src-tauri/src/storage/sessions.rs` (anywhere after the existing `truncate_chars` helper, around line 339).

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
pub fn extract_title_from_jsonl(path: &Path) -> Option<String> {
    const WINDOW: u64 = 64 * 1024;
    let head = read_head_bytes(path, WINDOW).ok()?;
    let tail = read_tail_bytes(path, WINDOW).ok()?;
    let title = extract_last_string_field(&tail, "customTitle")
        .or_else(|| extract_last_string_field(&head, "customTitle"))
        .or_else(|| extract_last_string_field(&tail, "aiTitle"))
        .or_else(|| extract_last_string_field(&head, "aiTitle"));
    title.map(|s| truncate_chars(&s, TITLE_MAX_CHARS))
}

/// Returns the first `max_bytes` of the file. Always line-aligned at
/// the start: if the file is smaller than `max_bytes`, returns it
/// whole; if a partial first line would result from a windowed read,
/// the partial line is discarded. (The head read here is always from
/// byte 0, so no skip is needed — but we keep the helper symmetric
/// with [`read_tail_bytes`] for readability.)
fn read_head_bytes(path: &Path, max_bytes: u64) -> std::io::Result<Vec<u8>> {
    use std::io::Read;
    let mut file = fs::File::open(path)?;
    let mut buf = vec![0u8; max_bytes as usize];
    let n = file.read(&mut buf)?;
    buf.truncate(n);
    Ok(buf)
}

/// Returns the last `max_bytes` of the file. When the read does not
/// start at byte 0, the first (partial) line is dropped so callers
/// see line-aligned data.
fn read_tail_bytes(path: &Path, max_bytes: u64) -> std::io::Result<Vec<u8>> {
    use std::io::{Read, Seek, SeekFrom};
    let mut file = fs::File::open(path)?;
    let len = file.metadata()?.len();
    let start = len.saturating_sub(max_bytes);
    file.seek(SeekFrom::Start(start))?;
    let mut buf = Vec::with_capacity((len - start) as usize);
    file.read_to_end(&mut buf)?;
    if start > 0 {
        if let Some(nl) = buf.iter().position(|&b| b == b'\n') {
            buf.drain(..=nl);
        }
    }
    Ok(buf)
}

/// Finds the LAST `"<key>":"…"` or `"<key>": "…"` substring in `text`
/// and returns the unescaped value. Returns `None` if no complete
/// match exists in the buffer. Mirrors upstream
/// `extractLastJsonStringField` in spirit (last-one-wins via
/// left-to-right walk) but iterates by char so UTF-8 multibyte
/// continuation bytes can never be mistaken for `\\` or `"`.
fn extract_last_string_field(text: &str, key: &str) -> Option<String> {
    let pat1 = format!("\"{}\":\"", key);
    let pat2 = format!("\"{}\": \"", key);
    let mut last: Option<String> = None;
    for pat in [&pat1, &pat2] {
        let mut from = 0usize;
        while let Some(rel) = text[from..].find(pat.as_str()) {
            let val_start = from + rel + pat.len();
            // Walk chars to find the closing quote, skipping
            // JSON-escaped chars (\\, \", \n, \uXXXX).
            let mut chars = text[val_start..].char_indices().peekable();
            let mut close_off: Option<usize> = None;
            while let Some((off, c)) = chars.next() {
                if c == '\\' {
                    if chars.next().is_none() {
                        break;
                    }
                    continue;
                }
                if c == '"' {
                    close_off = Some(off);
                    break;
                }
            }
            match close_off {
                Some(off) => {
                    let raw = &text[val_start..val_start + off];
                    last = Some(unescape_json_string(raw));
                    from = val_start + off + '"'.len_utf8();
                }
                None => {
                    // Truncated buffer; no complete value here.
                    from = text.len();
                    break;
                }
            }
        }
    }
    last
}

/// Parses a JSON string literal body and returns the unescaped value.
/// Falls back to the raw input on parse error (shouldn't happen for
/// well-formed input — defensive only).
fn unescape_json_string(raw: &str) -> String {
    serde_json::from_str::<String>(&format!("\"{}\"", raw)).unwrap_or_else(|_| raw.to_string())
}
```

- [ ] **Step 4: Run the helper tests and confirm they pass**

Run: `cd src-tauri && cargo test --lib storage::sessions::tests::title_from_jsonl 2>&1 | tail -25`
Expected: 7 tests pass.

If a test fails:
- `title_from_jsonl_prefers_head_custom_title_over_tail_ai_title` — verify the file actually exceeds 64KB (`ls -l` on the temp dir during a debugging run). The fixture is sized for ~80KB minimum.
- Escaped-quote test — confirm the JSON is `"He said \"hi\""` (literal `\"` in the file).
- `returns None when no title` — check the file does not contain `customTitle` or `aiTitle` anywhere.

- [ ] **Step 5: Run the full test suite to confirm no regressions**

Run: `cd src-tauri && cargo test 2>&1 | tail -10`
Expected: All existing tests still pass; the 7 new tests pass.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/storage/sessions.rs
git commit -m "feat(sessions): extract customTitle/aiTitle from .jsonl transcripts

Type-first priority across head+tail 64KB windows. Closes the
silent-rename-loss case where upstream /resume's bucket-first order
returns the wrong title.

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

### Task 2: Wire `extract_title_from_jsonl` into `merge_index_into`

**Files:**
- Modify: `src-tauri/src/storage/sessions.rs:178-200` (the `merge_index_into` body that builds `SessionSummary`)

**Interfaces:**
- Consumes: `extract_title_from_jsonl` from Task 1; the existing `entry.full_path` (an absolute path string from the index).
- Produces: `SessionSummary` with `title` set to the jsonl-derived title when present, otherwise the existing `pick_title(summary, first_prompt)` fallback.

- [ ] **Step 1: Add the override**

In `src-tauri/src/storage/sessions.rs`, locate the `merge_index_into` function (around line 158-200). Inside the `for entry in index.entries { ... }` loop, the current code builds the row like this:

```rust
        out.push(SessionSummary {
            session_id: entry.session_id,
            title: pick_title(entry.summary.as_deref(), entry.first_prompt.as_deref()),
            message_count: entry.message_count.unwrap_or(0),
            ...
        });
```

Replace that single `title:` line with a call to the helper when `entry.full_path` is a usable path, falling back to the existing behavior. Also add a use of `Path` (already in scope) — no new imports needed.

The new code:

```rust
        let title_from_jsonl = if entry.full_path.is_empty() {
            None
        } else {
            extract_title_from_jsonl(Path::new(&entry.full_path))
        };
        let title = title_from_jsonl.unwrap_or_else(|| {
            pick_title(entry.summary.as_deref(), entry.first_prompt.as_deref())
        });
        out.push(SessionSummary {
            session_id: entry.session_id,
            title,
            message_count: entry.message_count.unwrap_or(0),
            modified: entry.modified.or(entry.created),
            project_name: entry
                .project_path
                .as_deref()
                .and_then(last_path_segment),
            project_path: entry.project_path,
            full_path: entry.full_path,
        });
```

(Note: the original code may have `message_count`, `modified`, `project_name`, `project_path`, and `full_path` formatted slightly differently; the change is ONLY the `title` derivation. Leave the other fields untouched.)

- [ ] **Step 2: Add an integration test for the override**

Append inside `mod tests` in the same file:

```rust
    #[test]
    fn indexed_session_uses_jsonl_title_over_summary() {
        let tmp = tempfile::tempdir().unwrap();
        // Create the transcript first so we can reference its real path.
        let session_id = "11111111-2222-3333-4444-555555555555";
        let transcript = write_transcript(
            tmp.path(),
            &format!("{}.jsonl", session_id),
            "{\"type\":\"custom-title\",\"customTitle\":\"Jsonl Wins\"}\n",
        );
        let full_path = transcript.display().to_string();

        write(
            tmp.path(),
            "projects/-home-test/sessions-index.json",
            &format!(
                r#"{{
                    "version": 1,
                    "entries": [{{
                        "sessionId": "{sid}",
                        "fullPath": "{fp}",
                        "summary": "Index summary loses",
                        "firstPrompt": "first prompt loses",
                        "messageCount": 3,
                        "modified": "2026-07-09T10:00:00Z",
                        "projectPath": "/home/test"
                    }}]
                }}"#,
                sid = session_id,
                fp = full_path,
            ),
        );

        let out = scan_sessions(tmp.path()).unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].session_id, session_id);
        assert_eq!(out[0].title, "Jsonl Wins");
    }
```

- [ ] **Step 3: Run the new test and confirm it passes**

Run: `cd src-tauri && cargo test --lib storage::sessions::tests::indexed_session_uses_jsonl_title 2>&1 | tail -10`
Expected: PASS.

- [ ] **Step 4: Run the full test suite to confirm no regressions**

Run: `cd src-tauri && cargo test 2>&1 | tail -5`
Expected: all tests pass.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/storage/sessions.rs
git commit -m "feat(sessions): override index title with .jsonl customTitle/aiTitle

Index summary/firstPrompt remain as fallbacks when the .jsonl has
no title entry. Wire-in for scan_sessions -> merge_index_into.

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

### Task 3: Wire `extract_title_from_jsonl` into `summary_from_jsonl_stat`

**Files:**
- Modify: `src-tauri/src/storage/sessions.rs:208-239` (the `summary_from_jsonl_stat` function)

**Interfaces:**
- Consumes: `extract_title_from_jsonl` from Task 1; the existing `Path` to the `.jsonl`.
- Produces: `SessionSummary` with `title` set to the jsonl title when present, otherwise the existing `(unindexed) <uuid>` placeholder.

- [ ] **Step 1: Replace the hard-coded placeholder**

In `summary_from_jsonl_stat` (around line 230-238), find:

```rust
    let session_id = file_stem(path);
    Some(SessionSummary {
        title: format!("(unindexed) {}", session_id),
        session_id,
        message_count: 0,
        modified,
        project_name: project_folder_slug,
        project_path,
        full_path: path.display().to_string(),
    })
```

Replace the `title:` line with a helper-driven version:

```rust
    let session_id = file_stem(path);
    let title = extract_title_from_jsonl(path)
        .unwrap_or_else(|| format!("(unindexed) {}", session_id));
    Some(SessionSummary {
        title,
        session_id,
        message_count: 0,
        modified,
        project_name: project_folder_slug,
        project_path,
        full_path: path.display().to_string(),
    })
```

(Leave the other fields untouched.)

- [ ] **Step 2: Add an integration test for the unindexed override**

Append inside `mod tests`:

```rust
    #[test]
    fn unindexed_session_uses_jsonl_title_not_placeholder() {
        let tmp = tempfile::tempdir().unwrap();
        // A transcript under projects/... with NO sessions-index.json
        // nearby. The scanner will fall through to summary_from_jsonl_stat.
        write(
            tmp.path(),
            "projects/-home-test/transcripts/abcdef.jsonl",
            "{\"type\":\"custom-title\",\"customTitle\":\"Unindexed but titled\"}\n",
        );
        let out = scan_sessions(tmp.path()).unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].session_id, "abcdef");
        assert_eq!(out[0].title, "Unindexed but titled");
    }

    #[test]
    fn unindexed_session_falls_back_to_placeholder_when_no_title() {
        let tmp = tempfile::tempdir().unwrap();
        write(
            tmp.path(),
            "projects/-home-test/transcripts/no-title-here.jsonl",
            "{\"type\":\"user\",\"message\":{\"role\":\"user\",\"content\":\"hi\"}}\n",
        );
        let out = scan_sessions(tmp.path()).unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(
            out[0].title,
            "(unindexed) no-title-here",
            "untitled unindexed sessions should keep the uuid placeholder"
        );
    }
```

- [ ] **Step 3: Run the new tests and confirm they pass**

Run: `cd src-tauri && cargo test --lib storage::sessions::tests::unindexed_session 2>&1 | tail -10`
Expected: 2 tests pass.

- [ ] **Step 4: Run the full test suite**

Run: `cd src-tauri && cargo test 2>&1 | tail -5`
Expected: all tests pass.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/storage/sessions.rs
git commit -m "feat(sessions): replace (unindexed) uuid placeholder with jsonl title

When an unindexed transcript has a customTitle/aiTitle entry, use it.
When it doesn't, the (unindexed) <uuid> placeholder is preserved as
the last-resort fallback.

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

### Task 4: Drop the 8-char ID tag from the list row

**Files:**
- Modify: `src/components/Sessions.tsx:462-491` (the footer block in `SessionRow`)

- [ ] **Step 1: Remove the ID span and its separator**

In `src/components/Sessions.tsx`, locate the `SessionRow` component's footer div (around line 462-491). The current structure is:

```tsx
          <div className="mt-1 flex flex-wrap items-center gap-x-2 gap-y-0.5 text-[10px] text-muted-foreground">
            <span className="font-mono tabular-nums">{session.session_id.slice(0, 8)}</span>
            <span className="text-muted-foreground/50">·</span>
            <span>
              {session.message_count > 0
                ? `${session.message_count} msg${session.message_count === 1 ? "" : "s"}`
                : "unindexed"}
            </span>
            {session.modified && (
              <>
                <span className="text-muted-foreground/50">·</span>
                <span className="tabular-nums">
                  <TimeAgo iso={session.modified} />
                </span>
              </>
            )}
            {session.project_name && (
              <>
                <span className="text-muted-foreground/50">·</span>
                <span className="inline-flex items-center gap-0.5">
                  <Folder className="size-2.5" />
                  {session.project_name}
                </span>
              </>
            )}
          </div>
```

Remove ONLY the ID span and its trailing `·` separator. The result:

```tsx
          <div className="mt-1 flex flex-wrap items-center gap-x-2 gap-y-0.5 text-[10px] text-muted-foreground">
            <span>
              {session.message_count > 0
                ? `${session.message_count} msg${session.message_count === 1 ? "" : "s"}`
                : "unindexed"}
            </span>
            {session.modified && (
              <>
                <span className="text-muted-foreground/50">·</span>
                <span className="tabular-nums">
                  <TimeAgo iso={session.modified} />
                </span>
              </>
            )}
            {session.project_name && (
              <>
                <span className="text-muted-foreground/50">·</span>
                <span className="inline-flex items-center gap-0.5">
                  <Folder className="size-2.5" />
                  {session.project_name}
                </span>
              </>
            )}
          </div>
```

**Do NOT touch** the detail header at line 595 — the full `session_id` stays there for copy/reference.

- [ ] **Step 2: Type-check and lint**

Run: `pnpm exec tsc --noEmit && pnpm lint 2>&1 | tail -20`
Expected: both exit 0; no errors. (The `session_id` field is still used as the React `key` on line 399 and in the GitHub sync state map on lines 335, 518, 520, so removing the visual display doesn't introduce unused-var warnings.)

- [ ] **Step 3: Commit**

```bash
git add src/components/Sessions.tsx
git commit -m "feat(sessions): drop 8-char uuid tag from list rows

The list row now leads with the real session title (customTitle or
aiTitle from the .jsonl) and only shows the message count, time-ago,
and project. The full uuid remains in the detail header for copy.

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

### Task 5: Final verification

- [ ] **Step 1: Run the full Rust test suite**

Run: `cd src-tauri && cargo test 2>&1 | tail -5`
Expected: all tests pass (existing 10 + 7 helper tests from Task 1 + 1 indexed-override test from Task 2 + 2 unindexed tests from Task 3 = 20 total).

- [ ] **Step 2: Run the frontend verification gates**

Run: `pnpm exec tsc --noEmit && pnpm lint 2>&1 | tail -10`
Expected: both exit 0.

- [ ] **Step 3: Manual smoke test**

Run: `pnpm tauri dev` and verify in the running app:
1. A session you've renamed via `/rename` shows the renamed title.
2. A session Claude auto-titled shows the auto title.
3. A session with both `customTitle` and `aiTitle` (and a long transcript that pushes the customTitle out of the tail) shows the customTitle.
4. A session with NO title at all falls back to the first-prompt-derived title from the index (not `(unindexed) <uuid>`).
5. An unindexed session with a `customTitle` shows that title (no `(unindexed)` prefix).
6. An unindexed session with NO title shows `(unindexed) <8-char-id>`.
7. The detail view still shows the full UUID in the header.

If any check fails, file the regression in the relevant task's commit and fix forward — do not amend history.

- [ ] **Step 4: Final commit (if smoke test needed fixes)**

If the smoke test revealed any issue, commit the fix separately with a `fix(sessions):` prefix. Otherwise no commit is needed — the work is done at the end of Task 4.

---

## Self-Review

**Spec coverage:**
- ✅ "Title matches what `/resume` intends" → Task 1 helper with type-first priority
- ✅ "Unindexed sessions stop showing `(unindexed) <uuid>`" → Task 3
- ✅ "8-char UUID tag removed from list row, full UUID kept in detail" → Task 4 (and Task 4 explicitly notes detail header is untouched)
- ✅ "No new IPC, no new types" → File Structure section: no new files
- ✅ "Head+tail 64KB windows" → `WINDOW: u64 = 64 * 1024` in Task 1, step 3
- ✅ "Type-first priority: customTitle wins across both buffers" → `extract_title_from_jsonl` body in Task 1
- ✅ "6 unit tests + the bucket-order test" → 7 tests in Task 1; bucket-order test is `title_from_jsonl_prefers_head_custom_title_over_tail_ai_title`
- ✅ "Index summary/firstPrompt remain as fallbacks" → Task 2 `unwrap_or_else` chain
- ✅ "Ponytail ceiling documented" → `ponytail:` doc comment on the helper

**Placeholder scan:** No TBD/TODO. Every step has actual code or commands.

**Type consistency:** All `extract_title_from_jsonl` callers (Task 1, 2, 3) use the same signature `fn(path: &Path) -> Option<String>`. `extract_last_string_field`, `unescape_json_string`, `read_head_bytes`, `read_tail_bytes` are all defined in Task 1 and used in Task 1 only (the public `extract_title_from_jsonl` is the only external surface for Tasks 2/3).

**File paths:** All paths are repo-relative. Cargo commands use `cd src-tauri` as required by the project's `pnpm tauri dev` flow.
