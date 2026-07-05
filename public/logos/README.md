# Provider logos

SVG logos for built-in preset providers. Filenames must match the preset
`id` field in `src/lib/presetProviders.ts` (e.g. `zai.svg` for `id: "zai"`).
If a preset file is missing, `fetchPresetLogo` falls back to `fallback.svg`
so the UI always has something to render.

## Theme support

Every SVG should use `currentColor` (e.g. `fill="currentColor"` or
`stroke="currentColor"`) on its primary shapes. The `ProviderLogo` wrapper
sets `color: var(--muted-foreground)`, so the logo re-tints automatically
when the user toggles between light and dark themes — no per-SVG theming
logic.

## Sanitization

All SVGs are sanitized with DOMPurify (SVG profile) at render time as
defense-in-depth, so `<script>`, `on*` event handlers, and remote
`xlink:href` references are stripped even if the file is shipped with them.

## Files

| Filename      | Preset name           | Status        |
| ------------- | --------------------- | ------------- |
| zai.svg       | Z.ai (Zhipu GLM)      | ✅ themed     |
| minimax.svg   | MiniMax               | ✅ themed     |
| moonshot.svg  | Moonshot Kimi         | ✅ themed     |
| kimi-ai.svg   | Kimi Code Plan        | ✅ themed     |
| deepseek.svg  | DeepSeek              | ✅ themed     |
| freemodel.svg | freemodel             | ✅ themed     |
| zenmux.svg    | zenmux                | ✅ themed     |
| fallback.svg  | (universal fallback)  | ✅ themed     |
| aerolink.svg  | aerolink              | ❌ missing    |
| claude.svg    | (extras for upload)   | ✅ themed     |

Drop `aerolink.svg` in this directory when ready — the preset is wired up
but renders `fallback.svg` until then.