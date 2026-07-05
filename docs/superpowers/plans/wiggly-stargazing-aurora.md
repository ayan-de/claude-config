# Plan: multi-kind login switching (Subscription, Console, Custom, Bedrock, Vertex)

## Context

Today `claude-config` only manages one kind of login: **Custom relay** — a base URL + auth token pair written into `~/.claude/settings.json`'s `env` block. But Claude Code itself supports four other authentication modes that the app can't currently model:

1. **Subscription** (Pro/Max/Team/Enterprise) — OAuth login. Session token lives in `~/.claude/.credentials.json`. Setting `ANTHROPIC_AUTH_TOKEN` in env would **override and disable** OAuth.
2. **Anthropic Console** — API key via `ANTHROPIC_API_KEY`.
3. **Amazon Bedrock** — `CLAUDE_CODE_USE_BEDROCK=1` + AWS creds.
4. **Google Vertex AI** — `CLAUDE_CODE_USE_VERTEX=1` + `ANTHROPIC_VERTEX_PROJECT_ID` + region.
5. **Microsoft Foundry** — deferred to a follow-up PR (env-var names not yet verified).

Users want to switch between these effortlessly from one UI — including juggling multiple Subscription accounts (personal Pro vs. work Max) without re-running `claude /login` every time.

**Intended outcome:** a `kind` discriminator on `Provider` that lets `merge.rs` write mode-specific env blocks; a per-kind wizard for New Provider; a per-provider secret snapshot in the keyring; for Subscription, a swap of `.credentials.json` on activation with a "re-snapshot on switch-out" trick so OAuth refresh-token rotation isn't lost.

## Approach

Extend, don't rewrite. The provider-authoritative `merge_env` (`src-tauri/src/merge.rs:97`) already unsets canonical keys the active provider doesn't include — that behaviour is what makes mode switches self-cleaning. Growing `CANONICAL_ENV_KEYS` plus a kind-branched `provider_env_block` is 90% of the work.

The two structural additions are (a) a `state.json` pointer for the active provider (replaces env-block equality matching, which can't detect Subscription mode), and (b) a subscription helper module that atomically reads/writes `~/.claude/.credentials.json`.

## Backend changes (`src-tauri/src/`)

### `models.rs` — schema v2

- Add `ProviderKind` enum: `Subscription | Console | Custom | Bedrock | Vertex`. Use `serde(rename_all = "snake_case")`.
- Add optional kind-specific fields to `Provider`:
  - Custom: `base_url` becomes `Option<String>` (was required).
  - Bedrock: `aws_region`, `aws_profile`.
  - Vertex: `vertex_project_id`, `vertex_region`, `google_application_credentials`.
- Extend `CANONICAL_ENV_KEYS` from 9 → ~20 entries: add `ANTHROPIC_API_KEY`, `CLAUDE_CODE_USE_BEDROCK`, AWS_*, `CLAUDE_CODE_USE_VERTEX`, `ANTHROPIC_VERTEX_PROJECT_ID`, `CLOUD_ML_REGION`, `GOOGLE_APPLICATION_CREDENTIALS`. Update the `len()` assertion in the existing test.
- Introduce `ProviderSecret` enum for keyring payloads (tagged JSON, one entry per provider):
  - `Subscription { oauth: serde_json::Value }` (the full `claudeAiOauth` object)
  - `Console { api_key: String }`
  - `Custom { auth_token: String }`
  - `Bedrock { access_key: String, secret_key: String, session_token: Option<String> }`
  - `Vertex {}` (relies on `GOOGLE_APPLICATION_CREDENTIALS` file path — nothing to keyring)
- Bump `ProvidersFile::default().schema_version` to `2`.
- Extend `StateFile` (already exists, unused) — start persisting `activeProviderId`.

### `storage/providers.rs` — v1 → v2 migration

- Relax the `version > 1` reject at `storage/providers.rs:54` to `version > 2`.
- When loading a v1 file: parse into a legacy shape, coerce each provider to `kind: Custom`, then re-serialize as v2 on next `save_providers_file`. Keep it silent (per user choice).
- Add tests for v1 → v2 migration path and for v2 round-trip.

### `storage/keyring.rs` — JSON-blob secrets

- Add `set_secret(&self, id: &str, secret: &ProviderSecret)` / `get_secret(&self, id: &str) -> ProviderSecret` helpers that JSON-serialize into the existing string entry.
- Keep `set_token` / `get_token` as thin wrappers for backwards compat during migration.

### `storage/credentials.rs` — NEW module for `.credentials.json`

- Atomic read/write mirroring `storage/settings.rs`'s pattern (tempfile + fsync + rename + lock via a `.credentials.json.lock` sidecar). Reuse `write_settings_atomic` **only** if it's cheap to make it generic — otherwise duplicate the ~30 lines; the file is critical enough that clarity beats reuse.
- Function: `fn read_credentials_oauth() -> AppResult<Option<Value>>` — reads and returns just the `claudeAiOauth` object (or `None`).
- Function: `fn write_credentials_oauth(oauth: &Value) -> AppResult<()>` — merges into any existing `.credentials.json` (preserving other top-level keys like `mcpOAuth`) then atomic writes.

### `merge.rs` — kind-branched env block

- Replace `provider_env_block(&Provider, &str)` with `provider_env_block(&Provider, &ProviderSecret) -> Map<String, Value>`. Branch on `provider.kind`:
  - `Subscription`: empty env (model overrides + timeout still applied).
  - `Console`: `ANTHROPIC_API_KEY`.
  - `Custom`: today's behaviour — `ANTHROPIC_BASE_URL` + `ANTHROPIC_AUTH_TOKEN`.
  - `Bedrock`: `CLAUDE_CODE_USE_BEDROCK=1`, `AWS_REGION`, and either `AWS_PROFILE` **or** the AWS access/secret/session tuple from secret.
  - `Vertex`: `CLAUDE_CODE_USE_VERTEX=1`, `ANTHROPIC_VERTEX_PROJECT_ID`, `CLOUD_ML_REGION`, `GOOGLE_APPLICATION_CREDENTIALS` (path only).
- Model overrides + `API_TIMEOUT_MS` + `CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC` apply to any kind.
- `merge_env` itself is unchanged — the expanded `CANONICAL_ENV_KEYS` list gives it automatic mode-switch cleanup.
- Add one unit test per kind + one "switch cleans up previous mode's vars" test that runs `merge_env` twice with different kinds.

### `commands/settings.rs` — kind-aware load + active detection

- `load_provider_cmd` (currently at line 50) becomes the pivot point for effortless switching:
  1. Read current `activeProviderId` from `state.json`.
  2. If current active is `Subscription`: read `.credentials.json`'s `claudeAiOauth`, write it back into that provider's keyring `ProviderSecret::Subscription { oauth }` — **captures refresh-token rotation**.
  3. Load the new provider + its secret.
  4. If new is `Subscription`: write `secret.oauth` back into `.credentials.json`.
  5. Compute env via `provider_env_block(new, new_secret)`, merge, atomic write settings.json.
  6. Write new `activeProviderId` into `state.json`.
- `get_active_provider_cmd` (line 19): replace env-block-equality matching with a `state.json.activeProviderId` lookup. Fall back to today's env-matching only when the pointer is missing (first launch on an install without `state.json`).
- `save_current_as_provider_cmd` (line 83): detect kind by inspecting what's in `settings.json.env` and `.credentials.json` (see decision matrix below), then build the right kind. This is the "capture current config" flow.
- `preview_provider_env_cmd` (line 144): now signature-compatible after switching to `ProviderSecret`.

Decision matrix for `save_current_as_provider_cmd`:
| Present in settings.env | Present in .credentials.json | Inferred kind |
|---|---|---|
| `CLAUDE_CODE_USE_BEDROCK=1` | — | Bedrock |
| `CLAUDE_CODE_USE_VERTEX=1` | — | Vertex |
| `ANTHROPIC_API_KEY` (no BASE_URL) | — | Console |
| `ANTHROPIC_BASE_URL` + `ANTHROPIC_AUTH_TOKEN` | — | Custom |
| (empty env) | `claudeAiOauth` object | Subscription |
| (anything else) | — | Validation error |

### `commands/providers.rs` — kind-aware CRUD

- `ProviderInput` gains all the new optional fields + a `kind: ProviderKind`.
- `validate_input` (line 154) branches on `kind`:
  - Subscription: no base_url or token required. Optionally reject if `.credentials.json` has no `claudeAiOauth` at import time.
  - Console: `api_key` required.
  - Custom: today's rules (base_url + auth_token).
  - Bedrock: `aws_region` required; either `aws_profile` or (`access_key` + `secret_key`) required.
  - Vertex: `vertex_project_id` + `vertex_region` required.
- `add_provider_cmd` / `update_provider_cmd` (lines 29, 71): serialize secret into `ProviderSecret` matching the kind, then store via `set_secret`.

### New command: `import_current_subscription_cmd`

- Reads `~/.claude/.credentials.json` `claudeAiOauth` object; extracts an email/label if present; creates a Subscription provider named e.g. "Subscription (email@…)" or "Imported subscription"; stashes the OAuth blob as `ProviderSecret::Subscription`.
- Wired into `invoke_handler!` in `lib.rs`.
- The New Provider wizard's Subscription step calls this instead of a token input.

### `lib.rs` — first-launch auto-import extension

Extend `first_launch_import` (line 83) to run **both** imports independently:

1. If `.credentials.json` has `claudeAiOauth` → create Subscription provider.
2. If `settings.json.env` has `ANTHROPIC_BASE_URL` + `ANTHROPIC_AUTH_TOKEN` → create Custom provider (today's behaviour).
3. Both may fire. `activeProviderId` in `state.json` = whichever is currently authoritative (env-vars take precedence over OAuth, so Custom wins if both).

## Frontend changes (`src/`)

### `lib/types.ts`

Turn `Provider` into a discriminated union. Base fields (`id`, `name`, `kind`, timestamps, model overrides, timeout) shared; kind-specific fields on each variant. Same shape for `ProviderInput`.

```ts
export type Provider =
  | ({ kind: "subscription" } & Base & { subscriptionLabel?: string })
  | ({ kind: "console" } & Base)
  | ({ kind: "custom" } & Base & { base_url: string })
  | ({ kind: "bedrock" } & Base & { aws_region: string; aws_profile?: string })
  | ({ kind: "vertex" } & Base & { vertex_project_id: string; vertex_region: string; google_application_credentials?: string });
```

### `lib/api.ts`

- Extend `addProvider` / `updateProvider` signatures via the union.
- Add `importCurrentSubscription()` → new command.
- No changes to `loadProvider`, `previewProviderEnv`, `getActiveProvider` (still `Provider | null`).

### `components/ProviderForm.tsx` + `hooks/useProviderForm`

Convert to a two-step wizard:

**Step 1 — Pick kind.** Mirrors the `/login` menu the user showed:
- Subscription · Pro/Max/Team/Enterprise
- Console · API key
- Custom relay · third-party proxy (today's default)
- Bedrock · AWS
- Vertex · Google Cloud

**Step 2 — Kind-specific fields:**
- Subscription: single button "Import my current `claude /login` session" (calls `importCurrentSubscription`) + optional label.
- Console: API key input (masked, `Eye`/`EyeOff` toggle, same as today's token field).
- Custom: today's form unchanged.
- Bedrock: region + toggle "Use AWS profile" (profile name) vs. "Static credentials" (access/secret/session).
- Vertex: project id + region + optional SA JSON path (`file:` picker).

Model overrides + Advanced sections stay at the bottom, applicable to all kinds. When editing, skip Step 1 and render only Step 2 for the current kind.

Reuse existing base-ui shadcn primitives (Card, Input, Label, Button, Separator). No new UI libs.

### `components/ProviderCard.tsx`, `ProviderList.tsx`

Add a small "kind badge" (Subscription / Console / Custom / Bedrock / Vertex) next to the provider name. Keep everything else identical.

## Out of scope for this PR

- **Foundry**: add in follow-up once env-var names are verified against Claude Code source or official docs. The schema hook (`ProviderKind::Foundry`) is intentionally not added now so we don't ship a broken variant users can select.
- **Bedrock via IAM Identity Center / SSO**: only static keys + named profiles for v1. IAM SSO is a follow-up.
- **Editing an existing Subscription provider's OAuth blob directly**: v1 only supports re-importing from `.credentials.json`.

## Verification

**Rust tests** (fast, run first):
```bash
cd src-tauri && cargo test
```
Must include new tests for:
- `merge_env` mode-switch cleanup (Custom → Bedrock clears `ANTHROPIC_*` keys; Bedrock → Subscription clears `CLAUDE_CODE_USE_BEDROCK`).
- `provider_env_block` per kind.
- Schema v1 → v2 auto-migration (fixture file: `providers.json` with old shape).
- `import_current_subscription_cmd` with a fake `.credentials.json`.

**Frontend checks:**
```bash
pnpm lint && pnpm exec tsc --noEmit
```

**Manual E2E (with `pnpm tauri dev`)** — this is the golden path for effortless switching:

1. Fresh install: launch with the current machine's `~/.claude/settings.json` + `.credentials.json`. Expect both a Custom provider (Aerolink) and a Subscription provider to auto-import; Custom marked active.
2. Click Subscription → activate. Verify `settings.json` env now has no `ANTHROPIC_AUTH_TOKEN` / `ANTHROPIC_BASE_URL`. Verify `.credentials.json` OAuth blob unchanged.
3. Run `claude` in a shell — it should authenticate via OAuth. Do something that would rotate the refresh token (long session), then click a different provider in the app. Re-activate the Subscription provider. Verify the refresh token in `.credentials.json` matches what was there after the shell session — i.e. we captured the rotation.
4. New Provider → Bedrock, with a fake `AWS_REGION=us-east-1` + profile name. Verify `settings.json` env shows `CLAUDE_CODE_USE_BEDROCK=1` + `AWS_REGION` + `AWS_PROFILE`, and no Anthropic/Vertex keys.
5. Switch back to Custom. Verify Bedrock keys gone, Anthropic keys restored.
6. Verify a settings.json backup was created for every switch (in `<app-data>/backups/`).
7. Verify a legacy v1 `providers.json` (drop one in the app-data dir) migrates silently — post-launch, file is v2 and existing providers show `kind: custom`.

**Regression check:** the KeyringWarning banner, CustomConfigBanner, and DeleteDialog paths should all still function unchanged for Custom-kind providers.
