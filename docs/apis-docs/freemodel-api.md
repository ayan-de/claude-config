# FreeModel.dev API Documentation

FreeModel.dev is an AI provider offering OpenAI-compatible API access. It uses **cookie-based authentication** for dashboard endpoints (usage, billing) and **Bearer token** for inference endpoints.

---

## Authentication

### Credential Fields

| Field | Description |
|-------|-------------|
| `api_key` | Bearer token for inference (starts with `fe_oa_...`) |
| `bm_session` | Session cookie for dashboard API access |

### How to Find Credentials

1. **API Key**: Available at `https://freemodel.dev/dashboard/usage` — look for the key in the UI
2. **Session Cookie**: `bm_session` — from browser DevTools → Application → Cookies → freemodel.dev

### Cookie Extraction via Browser

1. Log into [freemodel.dev](https://freemodel.dev)
2. Open DevTools → Application → Cookies → `https://freemodel.dev`
3. Copy the `bm_session` value

---

## API Base URLs

| Purpose | URL |
|---------|-----|
| Dashboard API | `https://freemodel.dev` |
| Inference API | `https://api.freemodel.dev/v1` |

---

## Endpoints

### 1. Auth / Account Info

```
GET https://freemodel.dev/api/auth/me
Cookie: bm_session={value}
```

**Curl:**

```bash
curl -s "https://freemodel.dev/api/auth/me" \
  -H "Cookie: bm_session=$BM_SESSION"
```

**Response:**

```json
{
  "user": {
    "id": 406310,
    "email": "user@example.com",
    "name": "User Name",
    "referral_code": "FRE-xxxxxxxx",
    "is_abuser": 0,
    "verified_at": "2026-06-25 20:05:57",
    "is_partner": 0
  }
}
```

Returns `{"user": null}` if cookie is invalid or expired.

---

### 2. Usage

```
GET https://freemodel.dev/api/usage
Cookie: bm_session={value}
```

**Curl:**

```bash
curl -s "https://freemodel.dev/api/usage" \
  -H "Cookie: bm_session=$BM_SESSION"
```

**Response:**

```json
{
  "totalRequests": 85,
  "totalTokens": 4605055,
  "avgLatency": 2960,
  "todayCacheReadTokens": 652346,
  "todayCacheWriteTokens": 323512,
  "window5h": {
    "usedCents": 0,
    "limitCents": 1000,
    "resetsAt": 1782506376
  },
  "windowWeek": {
    "usedCents": 1005,
    "limitCents": 6667,
    "resetsAt": 1783022779
  }
}
```

**Key fields:**
- `window5h.usedCents / limitCents` — 5-hour session window (in pence)
- `windowWeek.usedCents / limitCents` — weekly window (in pence)
- `resetsAt` — Unix timestamp in **seconds** (multiply by 1000 for JS Date)
- `totalTokens` — cumulative total
- `todayCacheReadTokens / todayCacheWriteTokens` — cache stats

**Used percent:** `(usedCents / limitCents) * 100`
**Display value:** `usedCents / 100` (converts pence to pounds)

---

### 3. Billing

```
GET https://freemodel.dev/api/billing
Cookie: bm_session={value}
```

**Curl:**

```bash
curl -s "https://freemodel.dev/api/billing" \
  -H "Cookie: bm_session=$BM_SESSION"
```

**Response:**

```json
{
  "billingEnabled": true,
  "epayEnabled": true,
  "cryptoEnabled": true,
  "publishableKey": "pk_live_...",
  "requireVerification": true,
  "cnyDisplay": "9",
  "creditCents": 295,
  "signupCreditCents": 0,
  "signupExpiresAt": null,
  "totalTopupGbpPence": 0,
  "phoneVerifiedAt": null,
  "subscription": {
    "planId": "pro",
    "status": "active",
    "currentPeriodEnd": "2026-07-25 19:57:54",
    "cancelAtPeriodEnd": false,
    "renewalType": "manual"
  },
  "plans": [
    {"id": "free", "name": "Free", "priceCents": 0, "limit5hCents": 0, "limitWeekCents": 0},
    {"id": "pro", "name": "Pro", "priceCents": 500, "limit5hCents": 1000, "limitWeekCents": 6667},
    {"id": "pro_plus", "name": "Pro+", "priceCents": 1000, "limit5hCents": 2000, "limitWeekCents": 13200},
    {"id": "max", "name": "Max", "priceCents": 2000, "limit5hCents": 4000, "limitWeekCents": 26400},
    {"id": "ultra", "name": "Ultra", "priceCents": 10000, "limit5hCents": 20000, "limitWeekCents": 132000},
    {"id": "power", "name": "Power", "priceCents": 20000, "limit5hCents": 40000, "limitWeekCents": 264000}
  ]
}
```

**Key fields:**
- `subscription.planId` — `free`, `pro`, `pro_plus`, `max`, `ultra`, `power`
- `subscription.status` — `active`, `canceled`, etc.
- `subscription.currentPeriodEnd` — UTC datetime string (no timezone suffix → append `Z`)
- `subscription.cancelAtPeriodEnd` — boolean
- `subscription.renewalType` — `manual` or `auto`
- `creditCents` — remaining credit balance (divide by 100 for pounds)

---

## Rate Windows

| Window | Duration | Field | Reset field |
|--------|----------|-------|-------------|
| Session | 5 hours | `window5h` | `resetsAt` (Unix seconds) |
| Weekly | 7 days | `windowWeek` | `resetsAt` (Unix seconds) |

---

## Models

Available via `GET https://api.freemodel.dev/v1/models` (no auth required):

| ID | Owner |
|----|-------|
| `gpt-5.5` | freemodel |
| `gpt-5.4` | freemodel |
| `gpt-5.4-mini` | freemodel |
| `gpt-5.3-codex` | freemodel |

---

## Provider Metadata

| Field | Value |
|-------|-------|
| Provider ID | `FreeModel` |
| Display Name | `FreeModel` |
| Session Label | `Session` |
| Weekly Label | `Weekly` |
| Supports Credits | `true` |
| Currency | GBP (pence internally) |
| Default Enabled | `false` |
| Credential Fields | `api_key` + `bm_session` |
| Dashboard URL | `https://freemodel.dev/dashboard/usage` |

---

## Plan Catalog

| Plan | Price | 5h Limit | Weekly Limit |
|------|-------|----------|-------------|
| Free | £0 | £0 | £0 |
| Pro | £5/mo | £10 | £66.67 |
| Pro+ | £10/mo | £20 | £132 |
| Max | £20/mo | £40 | £264 |
| Ultra | £100/mo | £200 | £1,320 |
| Power | £200/mo | £400 | £2,640 |

---

## Quirks

- `currentPeriodEnd` has no timezone — append `Z` before parsing as UTC
- `resetsAt` is Unix **seconds** — multiply by 1000 for JS Date
- All monetary values in **pence** (1/100 of a pound) — divide by 100 for display
- Cookie-only for dashboard endpoints — `api_key` alone returns `{"error":"Unauthorized"}`
- `/api/auth/me` returns `200` with `{"user": null}` on bad cookie; other endpoints return `{"error":"Unauthorized"}`
