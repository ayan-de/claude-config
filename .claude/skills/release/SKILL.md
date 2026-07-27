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
   ```bash
   git tag vX.Y.Z && git push origin master && git push origin vX.Y.Z
   ```
5. **Build the bundles**. `TAURI_SIGNING_PRIVATE_KEY` must be set or the updater
   `.sig` files and `latest.json` are not produced:
   ```bash
   NO_STRIP=1 pnpm tauri build
   ```
   `NO_STRIP=1` is required on Arch — linuxdeploy ships an old `strip` that
   chokes on `.relr.dyn` sections and fails the AppImage bundle without it.
6. **Create the GitHub release** with every bundle plus its `.sig`, using the
   changelog entry as the body:
   ```bash
   gh release create vX.Y.Z --title "Claude Config vX.Y.Z" --notes-file <(...) \
     src-tauri/target/release/bundle/deb/*_X.Y.Z_*.deb* \
     src-tauri/target/release/bundle/rpm/*-X.Y.Z-*.rpm* \
     src-tauri/target/release/bundle/appimage/*_X.Y.Z_*.AppImage*
   ```

## AUR (`claude-config-bin`)

`aur/PKGBUILD` is the canonical copy; the AUR repo is a separate git remote
holding only `PKGBUILD` and `.SRCINFO`. It repacks the released `.deb`, so the
GitHub release must exist first.

```bash
git clone ssh://aur@aur.archlinux.org/claude-config-bin.git ~/aur-claude-config
cp aur/PKGBUILD ~/aur-claude-config/ && cd ~/aur-claude-config
# bump pkgver, then:
makepkg -g                          # new sha256sums for the published .deb
makepkg --printsrcinfo > .SRCINFO   # AUR rejects pushes without this
makepkg -si                         # verify it builds and launches
git add PKGBUILD .SRCINFO && git commit -m "upgpkg: X.Y.Z" && git push
```

Never commit `src/`, `pkg/`, or `*.pkg.tar.zst` to the AUR repo.