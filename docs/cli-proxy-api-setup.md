# Using CLIProxyAPI with Claude Config

[CLIProxyAPI](https://github.com/router-for-me/CLIProxyAPI) is a local proxy that exposes your
**existing CLI subscriptions** (Claude, Codex/ChatGPT, Gemini/Antigravity, Grok, Kimi) behind
OpenAI-, Gemini-, and Anthropic-compatible endpoints. Point Claude Config at it and you can drive
Claude Code with any of those subscriptions — no per-provider API key required, since the proxy
logs in via each provider's OAuth flow.

## Quick start (managed by Claude Config)

Claude Config can install and run the proxy for you. Nothing to download by hand, no YAML, no
terminal.

1. **Add Provider** → **Custom relay** → template **CLIProxyAPI (local)**
2. In the panel that appears, click **Install** — the app fetches the right release binary for your
   platform, writes a loopback-only config, and shows the version.
3. Click **Start**. The badge turns to `running :8317`.
4. Under **Subscriptions**, click **Log in** next to the one you want (Gemini, Codex, Grok, Kimi,
   Claude). Your browser opens; sign in; the row flips to ✓ connected.
5. Expand **Models**, click **Discover models**, set **Default** (and **Small / fast**).
6. **Save**, then **Load Provider**.

That's it — Claude Code now runs on that subscription. Everything lives under Claude Config's app
data dir (`cliproxyapi/`), the proxy binds to `127.0.0.1` only, and no auth token is needed — the
field is optional for any loopback base URL, so leave it blank.

The rest of this guide covers running the proxy yourself — useful if you want a shared instance, a
different port, or the management panel.

---

## 1. Install and run the proxy (manual)

You don't need Go, Git, or a build step — grab the prebuilt binary for your platform from the
[releases page](https://github.com/router-for-me/CLIProxyAPI/releases) (`CLIProxyAPI_<version>_<os>_<arch>`,
`.tar.gz` on macOS/Linux, `.zip` on Windows), unpack it anywhere, and run it:

```bash
./cli-proxy-api --config config.yaml
```

Prefer containers? `docker run -p 8317:8317 -v ~/.cli-proxy-api:/root/.cli-proxy-api eceasy/cli-proxy-api:latest`,
or use the repo's `docker-compose.yml`.

The release also includes `config.example.yaml` — copy it to `config.yaml` before your first run.
Default listen port is **8317**. Health check:

```bash
curl http://localhost:8317/healthz
```

Two friendlier front doors, once it's running:

- **`--tui`** starts a terminal UI for logins and config instead of flags.
- **`http://localhost:8317/management.html`** is a browser control panel (requires
  `remote-management.secret-key` in `config.yaml`).

## 2. Log in to your provider subscriptions

Each provider gets its own OAuth login, run once. Credentials land in `auth-dir`
(`~/.cli-proxy-api` by default) and are refreshed automatically.

```bash
./cli-proxy-api --claude-login        # Claude Pro/Max
./cli-proxy-api --codex-login         # ChatGPT Plus/Pro (Codex)
./cli-proxy-api --antigravity-login   # Google Gemini
./cli-proxy-api --xai-login           # Grok
./cli-proxy-api --kimi-login          # Kimi
```

Headless box? Add `--no-browser` (or use `--codex-device-login` for the device-code flow) and open
the printed URL yourself. Repeat a login to add a **second account for the same provider** —
CLIProxyAPI round-robins across all stored accounts of a provider.

Verify what got registered:

```bash
curl http://localhost:8317/v1/models
```

## 3. Decide whether the proxy needs a key

**You never need a provider API key** — no Anthropic, OpenAI, or Google key anywhere. The OAuth
logins from step 2 are the credentials, and your subscription is the billing.

The proxy has its own optional `api-keys` list in `config.yaml`, which controls who may talk to
*the proxy*:

```yaml
port: 8317
auth-dir: "~/.cli-proxy-api"
api-keys:
  - "local-dev-key"   # your choice of value; or delete this block entirely
```

- **Leave `api-keys` out** → the proxy accepts every request. Fine when it is bound to loopback
  only; leave the auth-token field in Claude Config blank.
- **Set a value** → put that same value in Claude Config's auth-token field.

> ⚠️ Do **not** leave `config.example.yaml`'s placeholder values (`your-api-key-1`). The proxy
> detects them and enters safe mode, answering every `/v1/*` call with
> `403 unsafe_example_api_key`. Either replace them or remove the block.

> Keep the proxy on loopback (`host: "127.0.0.1"`) unless you deliberately want other machines to
> reach it. Anyone who can reach it can spend your subscriptions.

Restart the server after editing.

## 4. Add it in Claude Config (manual instance)

1. **Add Provider** → preset dropdown → **CLIProxyAPI (local)**
2. Base URL is prefilled: `http://localhost:8317`
3. **Auth token**: the `api-keys` value from step 3 — leave blank if you removed that block
4. **Save**, then **Load Provider**

Claude Code now talks to `POST /v1/messages` on the proxy. Claude Config writes only
`ANTHROPIC_BASE_URL` + auth into `settings.json` — the proxy decides which upstream account serves
the request.

Using a non-default port or a remote host? Pick **+ Custom** instead and enter the full base URL
(the preset match is exact, so `http://localhost:8317/` with a trailing slash is fine but
`http://127.0.0.1:8317` won't show the preset logo).

## Using a non-Claude subscription inside Claude Code

This is the part that makes the whole thing worth it: Claude Code can run on your **Gemini, Codex,
Grok, or Kimi** subscription.

**How it works.** The proxy's Anthropic endpoint routes on the `model` field of the request body,
not on the URL. Whatever id you send, the proxy finds the account that owns it and translates the
Anthropic request/response into that provider's protocol. Claude Code doesn't need to know.

So the only thing you change is which model names Claude Code sends.

### Step by step

1. In Claude Config, edit your CLIProxyAPI provider and expand **Models**.
2. Click **Discover models**. Claude Config calls `/v1/models` on your base URL and turns the
   result into autocomplete suggestions on every model field — click into a field to browse them,
   or type to filter.
3. Pick your ids, then **Save** and **Load Provider**.

(Prefer the terminal? `curl -s http://localhost:8317/v1/models | grep '"id"'` gives the same list.)

| Field in Claude Config | Env var written | What Claude Code uses it for |
|---|---|---|
| **Default** | `ANTHROPIC_MODEL` | the main model for every turn |
| **Small / fast** | `ANTHROPIC_SMALL_FAST_MODEL` | cheap background work (titles, summaries) |
| **Sonnet / Opus / Haiku** | `ANTHROPIC_DEFAULT_*_MODEL` | what `/model sonnet`, `/model opus`, `/model haiku` resolve to |

Example — a Gemini subscription driving Claude Code:

| Field | Value |
|---|---|
| Default | `gemini-3-pro-preview` |
| Small / fast | `gemini-3-flash-preview` |

Codex instead? Use a `gpt-...` id. Grok? `grok-...`. Kimi? `kimi-...`. Use the ids from
`/v1/models` verbatim — do not guess, they change with upstream releases.

Filling in **Sonnet / Opus / Haiku** as well is what makes the in-session `/model` switch behave:
map `opus` to your strongest model and `haiku` to your cheapest, and Claude Code's usual
model-tier UX keeps working on a completely different provider.

### Switching subscriptions

Make **one Claude Config provider per subscription** — same base URL and token, different model
fields:

- `CLIProxy · Claude` — leave Models empty (Claude ids pass straight through)
- `CLIProxy · Gemini` — `gemini-*` ids
- `CLIProxy · Codex` — `gpt-*` ids

Then switching providers in Claude Config swaps your entire backing subscription in one click,
with the proxy already logged in to all of them.

### If a model id gets rejected

Some clients validate that a model name looks like a Claude model. The proxy has an escape hatch:
send `claude-fable-5-dd-<model-id-reversed>` and it decodes back to the real id before routing
(`ResolveClaudeModelIDPrefix` in `internal/client/claude/models/models.go`). You should not need
this with Claude Code — try the plain id first.

## Other protocol endpoints

If you want to point non-Claude tools at the same server:

| Endpoint | Protocol |
|----------|----------|
| `/v1/messages` | Anthropic (Claude Code, this app) |
| `/v1/chat/completions`, `/v1/responses` | OpenAI |
| `/v1beta/...` | Gemini |
| `/backend-api/codex/responses` | Codex native |

## Troubleshooting

| Symptom | Check |
|---------|-------|
| Connection refused | `curl http://localhost:8317/healthz`; confirm `port:` in `config.yaml` |
| 401 from the proxy | Auth token in Claude Config must match an entry in `api-keys` |
| `403 unsafe_example_api_key` | `api-keys` still holds the example placeholders — replace or delete them |
| Model not found | `curl http://localhost:8317/v1/models` — either the login for that provider didn't complete, or the id in **Models** is stale |
| **Install** fails | GitHub unreachable, or no release asset for your OS/CPU — the error names which |
| **Stop** is greyed out | The running proxy wasn't started by Claude Config, so it isn't ours to kill |
| "Discover models" fails | The proxy must be running and the auth-token field filled in; the error toast carries the exact status |
| Claude Code still uses a Claude model | The **Models → Default** field is empty; nothing overrides what Claude Code sends by default |
| Auth expired | Re-run the provider's `--*-login` flag; check files in `~/.cli-proxy-api` |
| Requests hit the wrong account | Multiple logins for one provider round-robin by design; remove unwanted auth files |

## Links

- [CLIProxyAPI](https://github.com/router-for-me/CLIProxyAPI) · [Docs](https://help.router-for.me/)
- [Claude Config](https://github.com/ayan-de/claude-config)
