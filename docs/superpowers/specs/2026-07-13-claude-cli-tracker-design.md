# Claude Code CLI tracker source — design

Date: 2026-07-13
Status: approved

## Goal

Add a usage tracker for Subscription (Pro/Max) providers that needs zero
user input: read the Claude Code CLI's own OAuth login and fetch usage from
Anthropic's OAuth usage API. This mirrors Token-Tracker's default "OAuth"
mode (`/home/ayande/Project/Token-Tracker/backend/src/providers/claude/oauth.rs`),
not its PTY-scraping CLI mode.

## Decisions

- **Mechanism:** OAuth usage API (`GET https://api.anthropic.com/api/oauth/usage`),
  not PTY-scraping `claude /usage`. No new dependencies, no TUI parsing.
- **Token source:** live CLI credentials via the existing
  `read_credentials_oauth()` storage helper (reads `~/.claude/.credentials.json`,
  honors `CLAUDE_CONFIG_DIR`). Not the per-provider keyring blob — its access
  token goes stale within hours and we won't implement refresh; the CLI
  refreshes its own token.
- The existing cookie-based `subscription` source stays untouched; users can
  still pick it.

## Backend

New file `src-tauri/src/tracker/claude_cli.rs`, registered in
`src-tauri/src/tracker/mod.rs` (new `SourceId::ClaudeCli` = `"claude_cli"`,
enum arms in `as_str`/`parse`, one line in `SourceRegistry::new`, one in
`list()`).

Source contract:

- `display_name`: "Claude Code CLI"
- `description`: reads your Claude Code login and fetches subscription usage
  from the OAuth API.
- `fields()`: empty — nothing to configure.
- `applicable_kinds()`: `["subscription"]`
- `validate_config`: always `Ok(())`.
- `fetch_usage`:
  1. `read_credentials_oauth()`; `None` → `Validation` error telling the user
     to run `claude /login`.
  2. Pull `accessToken` (missing/empty → `Validation`). If `expiresAt` (ms
     epoch) is in the past → `Validation` error "token expired — open Claude
     Code to refresh it".
  3. GET the usage URL with headers `Authorization: Bearer <token>`,
     `anthropic-beta: oauth-2025-04-20`, `Accept: application/json`, using the
     blocking client passed in (same as other sources).
  4. 401/403 → `Validation` ("re-login with claude /login"); other non-2xx →
     `Internal`.
  5. Map response windows to `UsageWindow` rows (`utilization` →
     `used`/`used_percent`, unit `%`, limit 100, `resetsAt` passthrough):
     - `fiveHour` → "5-hour session"
     - `sevenDay` → "Weekly"
     - `sevenDaySonnet` → "Weekly (Sonnet)"
     - `sevenDayOpus` → "Weekly (Opus)"
     Field names accept both camelCase and snake_case (serde aliases), like
     Token-Tracker does. Unknown keys dropped silently.
  6. `extraUsage.usedCredits` → `cost_usd` when present.
  7. `note`: "Source: Claude Code CLI login".

Tests (same style as `tracker/subscription.rs`): response-JSON → windows
mapping (camelCase and snake_case), expired-token rejection, missing-token
rejection.

## Frontend

One change: the auto-pick in `src/components/TrackerTab.tsx` (~line 115)
prefers `claude_cli` over `subscription` for subscription-kind providers.
The picker and form are already schema-driven from the registry; a
zero-field source renders an empty form with Save enabled (verify during
implementation).

## Out of scope (deliberate)

- PTY `/usage` scraping fallback.
- OAuth token refresh (the CLI owns that).
- Account email lookup (`/api/oauth/account`).
- 429 backoff state — a plain error message is enough for manual/periodic
  refresh.
