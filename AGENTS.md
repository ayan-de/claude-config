<!-- BEGIN:nextjs-agent-rules -->
# This is NOT the Next.js you know

This repo uses Next.js 16. Read the relevant guide in `node_modules/next/dist/docs/` before changing framework-level code, and heed deprecation notices.
<!-- END:nextjs-agent-rules -->

# Claude Config

Tauri 2 desktop app with a Next.js 16 static-export frontend and Rust backend. Read `README.md` first for the provider model and storage contract.

## Commands

- Use `pnpm tauri dev` for real app work. `src-tauri/tauri.conf.json` wires this to `pnpm dev` plus the Tauri shell; plain `pnpm dev` only shows the browser stub.
- Use `pnpm tauri build` for production bundles. Tauri runs `pnpm build` first and emits installers under `src-tauri/target/release/bundle/`.
- Verification is separate: `pnpm lint`, `pnpm exec tsc --noEmit`, `cd src-tauri && cargo test`.
- Keyring integration is only covered by `cd src-tauri && cargo test -- --ignored keyring`; it exercises the real OS keyring and is environment-dependent.
- There is no JS/TS unit-test runner in this repo.

## Frontend

- The app is a static SPA: `next.config.ts` sets `output: "export"`, `trailingSlash: true`, and `images.unoptimized = true`. Do not add SSR, API routes, or server-only Next features.
- `src/app/page.tsx` is the shell; operational state now lives in hooks such as `src/hooks/useProvidersApp.ts`, `src/hooks/useUpdater.ts`, `src/hooks/useGlobalPanel.ts`, and `src/hooks/useDangerousMode.ts`.
- Outside Tauri, `isWebEnv()` returns false and the UI intentionally renders a "Run inside Tauri" stub. Do not try to make the browser-only path feature-complete.
- All frontend IPC goes through `src/lib/api.ts`. Do not call `invoke()` directly from components or hardcode command names.
- Keep `src/lib/types.ts` aligned with `src-tauri/src/models.rs`; the UI never receives provider secrets.
- `components.json` uses shadcn's `base-nova` style on top of `@base-ui/react`. Use the existing `src/components/ui/` primitives; do not introduce Radix-based replacements.
- Tailwind is v4 via `@tailwindcss/postcss` in `postcss.config.mjs`. There is no `tailwind.config.js`; theme tokens live in `src/app/globals.css` and `components.json`.

## Backend

- `src-tauri/src/lib.rs` is the Tauri entrypoint and command registry. It also performs first-launch import from existing Claude settings and subscription credentials.
- `src-tauri/src/merge.rs` is the single source of truth for provider -> `settings.json.env` mapping. Preserve its provider-authoritative behavior: canonical keys missing from the provider are removed, unknown existing keys are preserved.
- `src-tauri/src/storage/settings.rs` is the only place that writes Claude Code's `settings.json`. It uses a sidecar lock file, backup copy, temp file, `fsync`, and atomic rename.
- `src-tauri/src/storage/keyring.rs` stores secrets in the OS keyring under service `claude-config`. Secrets do not belong in `providers.json` or in TS models.
- `src-tauri/src/models.rs` defines the canonical env-key list and the serde shape shared with the UI. If you change provider fields, update both Rust and TypeScript.

## Repo-specific gotchas

- App identifier is `com.claudeconfig.app`; app data lives under that directory name.
- `CLAUDE_CONFIG_DIR` overrides the Claude Code config dir. `TAURI_DEV_HOST` changes the Next asset prefix for Tauri dev.
- `next-env.d.ts`, `.next/`, `out/`, and `src-tauri/target/` are generated artifacts; do not hand-edit them.
- ESLint ignores `.next/**`, `out/**`, `src-tauri/target/**`, and `.worktrees/**`.
- This is a single-package repo. There is no `pnpm-workspace.yaml`; the pnpm-specific `ignoredBuiltDependencies` setting lives in `package.json`.
