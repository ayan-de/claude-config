---
name: release
description: Use when releasing a new version of this app (bumping version, tagging, building, publishing GitHub release). Triggers on "release", "cut a release", "bump version", "ship vX.Y.Z".
---

# Release Checklist

Three files hold the version and **must stay in sync**:

| File                          | Field           |
|--------------------------------|-----------------|
| `package.json:3`               | `"version"`     |
| `src-tauri/tauri.conf.json:4`  | `"version"`     |
| `src-tauri/Cargo.toml:3`       | `version = ...` |

## Steps

1. **Bump version** in all three files to the same `X.Y.Z`. Verify all three match before continuing — a mismatch breaks the updater.
2. **Write the changelog entry** at the top of `CHANGELOG.md` under `## vX.Y.Z`.
3. **Commit** using Conventional Commits: `chore(release): vX.Y.Z` (lowercase, no space before version).
4. **Tag and push**: