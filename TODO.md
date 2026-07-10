# TODO

Tracking loose ends and follow-ups that don't fit cleanly into a commit message
or code comment. Items here should graduate out as they're addressed (or move to
a proper GitHub issue when they need discussion).

---

## GitHub Session Sync

### [HIGH] Replace personal OAuth App client ID with an organization one

**Context:** Phase 1 of GitHub session sync shipped with the maintainer's
personal OAuth App client ID hardcoded in
`src-tauri/src/github/device_flow.rs`. The constant has a TEMP comment
calling this out, but until it's replaced:

- End users see the maintainer's personal OAuth App name on the GitHub
  consent screen
- The maintainer gets telemetry for every user's auth request
- Deleting the personal OAuth App breaks sync for every existing user

**When the project has a GitHub organization:**

1. Create the org on github.com (e.g. `anthropics` or whatever the
   project's home is)
2. Under the org, register a new OAuth App at
   https://github.com/organizations/{org}/settings/applications/new
   - Enable Device Flow
   - Scopes: leave empty (we request `repo` at runtime)
   - Callback URL: leave blank or `http://localhost:8080` (unused for
     device flow)
3. Copy the new client ID (starts with `Iv1.`)
4. Replace `GITHUB_OAUTH_CLIENT_ID` in
   `src-tauri/src/github/device_flow.rs`
5. Cut a release. **Heads up:** existing users will need to disconnect
   and reconnect once — their stored tokens are tied to the old OAuth
   App and can't be transferred.
6. Make `github_oauth_client_id()` read from
   `option_env!("CLAUDE_CONFIG_GITHUB_CLIENT_ID")` instead of being a
   hardcoded `const`, so future maintainers don't accidentally
   hardcode a personal OAuth App again.

**Why deferred:** Project doesn't have a GitHub org yet. Until one
exists, using the maintainer's personal OAuth App is the only option.

---

## Notes

- `docs/GITHUB_SYNC_PLAN.md` has the full implementation plan including
  the open edge cases (privacy consent, format versioning, path-mapping
  staleness, worktree awareness, retention strategy).
- Phase 1 (OAuth + connection) is done. Phases 2 (upload) and 3
  (download) still pending.