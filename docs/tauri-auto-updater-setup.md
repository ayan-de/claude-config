# Tauri 2 Auto-Updater: End-to-End Setup

Reference for wiring `tauri-plugin-updater` into a Tauri 2 desktop app so
installed clients pull signed updates from GitHub Releases automatically.

The gotchas are all things I hit while shipping `claude-config` v0.4.0 →
v0.5.3. Skip past them by following the steps in order.

---

## What "auto-update" actually needs

Three separate things must line up. Missing any one of them means silent
failure — the client checks, gets a 404 or a signature mismatch, and swallows
the error.

1. **The client** (your installed app) knows a public key and an endpoint URL,
   baked in at build time.
2. **The release** (GitHub Release) has a `latest.json` manifest and a `.sig`
   file next to each installer.
3. **The manifest** points to installer URLs and includes a signature the
   client's public key can verify.

Everything below is scaffolding around those three requirements.

---

## Step 0 — One-time key generation

Do this once per app (not per release). Keep the private key **and its
password** somewhere safe — losing them means every future release must switch
to a new key, and every installed client will need a manual reinstall.

```bash
pnpm tauri signer generate -w ~/.tauri/<your-app>.key
```

You'll be prompted for a password. Remember it — GitHub Actions needs it too.

This produces:

- `~/.tauri/<your-app>.key`     — private key, **never commit**
- `~/.tauri/<your-app>.key.pub` — public key, safe to commit / paste anywhere

### GitHub secrets

In your repo → Settings → Secrets and variables → Actions, add:

| Secret | Value |
|---|---|
| `TAURI_SIGNING_PRIVATE_KEY` | contents of `~/.tauri/<your-app>.key` |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | the password you set above |

Verify with `gh secret list -R <owner>/<repo>`.

---

## Step 1 — Install the updater plugin

```bash
pnpm add @tauri-apps/plugin-updater
cd src-tauri && cargo add tauri-plugin-updater
```

Register it in `src-tauri/src/lib.rs`:

```rust
tauri::Builder::default()
    .plugin(tauri_plugin_updater::Builder::new().build())
    // ...other plugins
    .run(tauri::generate_context!())
    .expect("error while running tauri application");
```

Grant permission in `src-tauri/capabilities/default.json`:

```json
{
  "permissions": [
    "updater:default",
    "updater:allow-check",
    "updater:allow-download-and-install"
  ]
}
```

---

## Step 2 — Configure `tauri.conf.json`

Three fields matter. **All three are required.**

```json
{
  "bundle": {
    "active": true,
    "targets": "all",
    "createUpdaterArtifacts": "v1Compatible"
  },
  "plugins": {
    "updater": {
      "active": true,
      "endpoints": [
        "https://github.com/<owner>/<repo>/releases/latest/download/latest.json"
      ],
      "pubkey": "<paste the ENTIRE contents of ~/.tauri/<your-app>.key.pub here>",
      "windows": {
        "installMode": "passive"
      }
    }
  }
}
```

### Field-by-field, why each matters

| Field | What breaks if missing |
|---|---|
| `bundle.createUpdaterArtifacts` | Tauri's bundler **won't produce `.sig` files at all**. The build finishes with "Finished 2 bundles" but no "Finished N updater signatures" line. tauri-action then filters every `.sig` path out (they don't exist on disk), and you get "Signature not found for the updater JSON. Skipping upload...". This is the single most common failure. |
| `plugins.updater.pubkey` | The installed client has no key to verify against. `check()` throws `UnexpectedKeyId` or silently fails. |
| `plugins.updater.endpoints` | Client doesn't know where to look for the manifest. |

### Why `"v1Compatible"` specifically

Tauri 2 offers two modes:

- `true` → new direct-signing mode. Signs installers, but on Windows only
  produces `.msi` + `.msi.sig` (no `.msi.zip`).
- `"v1Compatible"` → legacy mode. Produces `.msi.zip` (and `.app.tar.gz` on
  macOS) alongside the `.sig` files.

tauri-action expects the v1Compatible layout. Setting `true` works too but
you'll need to know the trade-offs. Pick `"v1Compatible"` unless you have a
specific reason.

### Getting the pubkey value right

The `pubkey` field takes the **entire file content** verbatim, including the
`untrusted comment:` header line. Easiest way:

```bash
tr -d '\n' < ~/.tauri/<your-app>.key.pub
```

Paste that single line. It should look like:

```
"pubkey": "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IDc3MDE4MTM3NUYzMjU3Q0EKUldUS1Z6SmZONEVCZHo0Z1JMTWhJZnJwNFN3QTlFMXVrVHNEUUNJaDFRQUtnRDA1eEpZbHpJZ1MK"
```

---

## Step 3 — GitHub Actions release workflow

`.github/workflows/release.yml`:

```yaml
name: Release

on:
  push:
    tags:
      - "v*.*.*"
  workflow_dispatch:

permissions:
  contents: write

jobs:
  build:
    name: Build ${{ matrix.platform.name }}
    runs-on: ${{ matrix.platform.os }}
    strategy:
      fail-fast: false
      matrix:
        platform:
          - name: linux-x86_64
            os: ubuntu-22.04
            rust-target: x86_64-unknown-linux-gnu
            tauri-args: ""
          - name: macos-universal
            os: macos-latest
            rust-target: aarch64-apple-darwin
            tauri-args: "--target universal-apple-darwin"
          - name: windows-x86_64
            os: windows-latest
            rust-target: x86_64-pc-windows-msvc
            tauri-args: ""

    steps:
      - uses: actions/checkout@v4

      # ...toolchain setup (pnpm, node, rust, system deps)...

      - name: Install frontend dependencies
        run: pnpm install --frozen-lockfile

      - name: Build & release
        uses: tauri-apps/tauri-action@v1
        env:
          GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}
          TAURI_SIGNING_PRIVATE_KEY: ${{ secrets.TAURI_SIGNING_PRIVATE_KEY }}
          TAURI_SIGNING_PRIVATE_KEY_PASSWORD: ${{ secrets.TAURI_SIGNING_PRIVATE_KEY_PASSWORD }}
        with:
          tagName: ${{ github.ref_name }}
          releaseName: "My App ${{ github.ref_name }}"
          releaseDraft: true
          prerelease: false
          args: ${{ matrix.platform.tauri-args }}
```

### Why `@v1` and not `@v0`

`tauri-action@v0` (still on v0.6.2 at time of writing) has bugs around
macOS `.app.tar.gz` re-packaging and updater artifact handling. `@v1.0.0`
(released 2026-06-29) is the first release cleanly supporting Tauri 2's
updater flow. Use `@v1` unless you're stuck on Tauri v1.

---

## Step 4 — Frontend integration

`src/hooks/useUpdater.ts` (minimal version):

```typescript
"use client";

import { useCallback, useEffect, useRef, useState } from "react";
import { check, type Update } from "@tauri-apps/plugin-updater";
import { toast } from "sonner";

export function useUpdater() {
  const [available, setAvailable] = useState(false);
  const [version, setVersion] = useState<string | null>(null);
  const [downloading, setDownloading] = useState(false);
  const updateRef = useRef<Update | null>(null);

  const runCheck = useCallback(async (showToastOnError: boolean) => {
    try {
      const u = await check();
      if (u) {
        updateRef.current = u;
        setVersion(u.version);
        setAvailable(true);
        toast.info(`Update available: v${u.version}`);
      } else if (showToastOnError) {
        toast.success("You're up to date");
      }
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      if (showToastOnError) toast.error(`Update check failed: ${msg}`);
    }
  }, []);

  useEffect(() => { void runCheck(false); }, [runCheck]);

  const installUpdate = useCallback(async () => {
    const u = updateRef.current;
    if (!u) return;
    setDownloading(true);
    try {
      await u.download();
      await u.install(); // triggers app restart on Windows/Linux
    } catch (e) {
      toast.error(`Update failed: ${e instanceof Error ? e.message : String(e)}`);
      setDownloading(false);
    }
  }, []);

  return { available, version, downloading, checkNow: () => runCheck(true), installUpdate };
}
```

Mount the hook once at the app root, wire `checkNow()` into a settings menu
button, and render a banner when `available` is true. On Windows with
`installMode: "passive"`, the installer runs silently; the user just sees the
app close and reopen.

---

## Step 5 — Release procedure

Keep versions synced across three files:

```bash
# Bump all three
sed -i 's/"version": ".*"/"version": "X.Y.Z"/' package.json
sed -i 's/"version": ".*"/"version": "X.Y.Z"/' src-tauri/tauri.conf.json
sed -i 's/^version = ".*"/version = "X.Y.Z"/' src-tauri/Cargo.toml

# Refresh Cargo.lock
(cd src-tauri && cargo check)

# Commit, tag, push
git add package.json src-tauri/tauri.conf.json src-tauri/Cargo.toml src-tauri/Cargo.lock
git commit -m "chore: release vX.Y.Z"
git tag vX.Y.Z
git push origin master vX.Y.Z
```

The tag push triggers the workflow. It creates a **draft** release, uploads
all installers + `.sig` files + `latest.json`, then you publish it manually
from the Releases page (this gives you a chance to sanity-check assets
before it goes live).

---

## Step 6 — Verify the release is healthy

After the workflow completes and you publish the draft:

```bash
# 1. Manifest exists at the "latest" URL
curl -sL https://github.com/<owner>/<repo>/releases/latest/download/latest.json | head -30
```

Expected: JSON with `version`, `notes`, `pub_date`, and a `platforms` object
containing `windows-x86_64`, `darwin-*`, `linux-x86_64` entries with
`signature` and `url` fields.

```bash
# 2. Release lists .sig files alongside installers
gh release view vX.Y.Z -R <owner>/<repo> | grep asset
```

Expected: `latest.json` in the list, plus a `.sig` for every installer.

```bash
# 3. Workflow log confirms signing actually ran
gh run view <run-id> -R <owner>/<repo> --log | grep -E 'Finished .* signatures|Signature not found'
```

Expected: **"Finished N updater signatures at:"** lines. If you see
**"Signature not found for the updater JSON. Skipping upload..."** —
`createUpdaterArtifacts` is missing from `tauri.conf.json`.

---

## The failures I hit, in order, and what fixed each

Reference for anyone debugging their own updater. If the endpoint returns
404 or the client silently ignores updates, the problem is almost certainly
one of these.

### 1. Empty `pubkey` (v0.4.0 → v0.5.0)

**Symptom**: Client's `check()` throws immediately; the error is swallowed
into `setError` and never surfaced unless the user manually clicks "Check for
updates". Workflow also skips generating `.sig` files because tauri-action
sees no pubkey → no signing → nothing to attach.

**Fix**: Paste the full pubkey content into `plugins.updater.pubkey`.

**Consequence for existing installs**: **Unrecoverable via auto-update.**
Every client already installed with an empty pubkey has to be manually
reinstalled once. From the next release onward, auto-update takes over.

### 2. `tauri-action@v0` re-tarring the macOS `.app` (v0.5.1)

**Symptom**: Windows/Linux installers upload cleanly, but the log shows
"Packaging Claude Config.app directory into Claude Config.app.tar.gz" — the
action re-tars the already-signed bundle, invalidating the signature.
Result: "Signature not found for the updater JSON. Skipping upload..."

**Fix**: `tauri-apps/tauri-action@v0` → `tauri-apps/tauri-action@v1`.

### 3. Missing `bundle.createUpdaterArtifacts` (v0.5.2)

**Symptom**: The **real** root cause of the "Signature not found" errors.
The workflow log shows:

```
Finished 2 bundles at:
    .../Claude Config_0.5.2_x64_en-US.msi
    .../Claude Config_0.5.2_x64-setup.exe
```

…with **no** `Finished N updater signatures at:` line. Tauri never signed
anything because the config didn't ask it to.

**Fix**: Add `"createUpdaterArtifacts": "v1Compatible"` under `bundle` in
`tauri.conf.json`. This is the single line that flips the bundler into
signing mode.

---

## Debugging cheat sheet

| Symptom | First check |
|---|---|
| `curl .../latest.json` returns 404 | `gh release view vX.Y.Z \| grep asset` — is `latest.json` there? |
| Release has installers but no `.sig` files | Log for `Finished .* updater signatures` — if absent, `createUpdaterArtifacts` is missing |
| `latest.json` exists but client says "no updates" | Client is on the same or newer version; or its baked-in `pubkey` mismatches the manifest signature |
| Client throws `UnexpectedKeyId` | Signing key was regenerated between builds; the installed client's pubkey doesn't match the new signatures |
| Windows client shows update but install silently fails | Missing capability permission or `installMode` misconfigured; check `capabilities/default.json` |

---

## Rules of thumb

- **Never regenerate the signing key.** If you lose it, every installed
  client needs manual reinstall to get the new pubkey.
- **Test the endpoint before releasing.** A 404 on `latest.json` means the
  workflow didn't produce it — don't announce the release until you've
  curled the URL.
- **Bump the version in all three files** (`package.json`,
  `src-tauri/tauri.conf.json`, `src-tauri/Cargo.toml`). Missing any one
  causes weird version mismatches in the manifest.
- **Draft releases first.** Let the workflow build a draft, verify the
  assets, then publish. Cheaper than yanking a broken release.
- **The client updates itself, but only for the next version.** The
  currently-installed version is what runs `check()`. Any fix to updater
  configuration only helps releases *after* the fix ships to users.
