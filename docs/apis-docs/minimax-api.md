# MiniMax API Documentation

MiniMax is a Chinese AI provider that supports both **Coding Plans** (token-based subscriptions) and **Pay-As-You-Go** billing. Token-Tracker routes to the correct endpoint based on your API key type.

---

## Authentication Methods & Setup

### Method 1: API Key & Group ID (Recommended)

Create `~/.minimax/config.json` (Linux) or `%USERPROFILE%\minimax\config.json` (Windows):

```json
{
  "api_key": "sk-cp-your_api_key_here",
  "group_id": "your_group_id_here"
}
```

**Key type detection:**
- Keys starting with `sk-cp-` → **Coding Plans** → fetches from `/coding_plan/remains`
- Keys starting with `sk-` → **Pay-As-You-Go** → fetches from `/billing/usage`

**Environment variable override:**

| Variable | Description |
|----------|-------------|
| `MINIMAX_API_KEY` | Your API key |
| `MINIMAX_GROUP_ID` | Your group ID |

**Finding your group_id:** Under your MiniMax developer console profile.

---

### Method 2: Browser Cookie Import

1. Log into [platform.minimax.io](https://platform.minimax.io) (Global) or [platform.minimaxi.com](https://platform.minimaxi.com) (China)
2. Token-Tracker settings → **Cookie Import** tab
3. Select browser/profile, choose **MiniMax**, click **Import & Sync Cookies**

---

### Method 3: Manual Cookie Override

1. DevTools → Network tab on MiniMax platform
2. Find request to `/v1/api/openplatform/coding_plan/remains` or `/account/amount`
3. Copy the `Cookie` request header
4. Token-Tracker settings → **Credentials** tab → **MiniMax** → **Manual Cookie**

---

## API Base URLs

| Region | Platform URL | API URL |
|--------|-------------|---------|
| Global | `https://platform.minimax.io` | `https://api.minimax.io` |
| China Mainland | `https://platform.minimaxi.com` | `https://api.minimaxi.com` |

Region is determined by settings (`api_region` config value). Accepted values: `cn`, `china`, `china-mainland`, `china_mainland`, `mainland`. Defaults to Global.

---

## Endpoints

### 1. Get Coding Plan Remains (Coding Plans — `sk-cp-*` keys)

Returns 5-hour limit and weekly limit usage for subscription plans.

```
GET /v1/api/openplatform/coding_plan/remains?GroupId={group_id}
```

**Headers:**

| Header | Value |
|--------|-------|
| `Authorization` | `Bearer {api_key}` |
| `MM-API-Source` | `CodexBar` |

**Curl:**

```bash
curl -s "https://api.minimax.io/v1/api/openplatform/coding_plan/remains?GroupId=$GROUP_ID" \
  -H "Authorization: Bearer $MINIMAX_API_KEY" \
  -H "MM-API-Source: CodexBar"
```

**Response Fields:**

```json
{
  "base_resp": {
    "status_code": 0,
    "status_msg": "success"
  },
  "model_remains": [
    {
      "model_name": "general",
      "current_interval_remaining_percent": 85.5,
      "end_time": 1751424000,
      "current_weekly_remaining_percent": 62.3,
      "weekly_end_time": 1751913600
    }
  ]
}
```

**Key fields:**
- `model_remains[].model_name` — model identifier (`general` is the primary)
- `model_remains[].current_interval_remaining_percent` — 5-hour window remaining (0-100)
- `model_remains[].end_time` — Unix timestamp (ms) when 5-hour window resets
- `model_remains[].current_weekly_remaining_percent` — weekly window remaining (0-100)
- `model_remains[].weekly_end_time` — Unix timestamp (ms) when weekly window resets

**Rate windows derived:**
- **Primary (session):** 5-hour window, `100 - current_interval_remaining_percent` = used %
- **Secondary (weekly):** 7-day window, `100 - current_weekly_remaining_percent` = used %

---

### 2. Get Billing Usage (Pay-As-You-Go — `sk-*` keys)

Returns credit balance and monthly usage for pay-as-you-go plans.

```
GET /v1/billing/usage?group_id={group_id}
```

**Headers:** Same as Coding Plan endpoint

**Curl:**

```bash
curl -s "https://api.minimax.io/v1/billing/usage?group_id=$GROUP_ID" \
  -H "Authorization: Bearer $MINIMAX_API_KEY" \
  -H "MM-API-Source: CodexBar"
```

**Response Fields:**

```json
{
  "base_resp": {
    "status_code": 0,
    "status_msg": "success"
  },
  "used_amount": 15.50,
  "total_quota": 100.00,
  "plan_name": "MiniMax Star"
}
```

**Key fields:**
- `used_amount` — credits consumed
- `total_quota` / `total_quota` — credit limit
- `plan_name` / `current_plan_title` / `current_subscribe_title` / `combo_title` — plan display name
- `current_combo_card.title` — nested card title (fallback)

**Used percent:** `(used_amount / total_quota) * 100`

---

### 3. Get Billing History (Cookie auth)

Returns detailed charge records for cost tracking.

```
GET /account/amount?page=1&limit=100&aggregate=false
```

**Headers:**

| Header | Value |
|--------|-------|
| `Cookie` | `{cookie_header}` |
| `Accept` | `application/json, text/plain, */*` |
| `X-Requested-With` | `XMLHttpRequest` |

**Curl:**

```bash
curl -s "https://platform.minimax.io/account/amount?page=1&limit=100&aggregate=false" \
  -H "Cookie: $COOKIE" \
  -H "Accept: application/json, text/plain, */*" \
  -H "X-Requested-With: XMLHttpRequest"
```

**Response Fields:**

```json
{
  "base_resp": { "status_code": 0 },
  "charge_records": [
    {
      "consume_token": 1200,
      "consume_input_token": 800,
      "consume_output_token": 400,
      "consume_cash": "0.42",
      "consume_cash_after_voucher": "0.38",
      "ymd": "2026-06-25",
      "consume_time": "2026-06-25 14:30:00",
      "method": "chat",
      "model": "abab6.5",
      "result": "SUCCESS",
      "status": null
    }
  ]
}
```

**Key fields:**
- `charge_records[].consume_token` — total tokens (or sum of input + output)
- `charge_records[].consume_input_token` / `consume_output_token` — token breakdown
- `charge_records[].consume_cash_after_voucher` — cost after vouchers (preferred)
- `charge_records[].consume_cash` — cost before vouchers
- `charge_records[].ymd` — date string (YYYY-MM-DD)
- `charge_records[].consume_time` — datetime string
- `charge_records[].method` — API method (chat, completion, audio, video, etc.)
- `charge_records[].model` — model name
- `charge_records[].result` / `charge_records[].status` — `SUCCESS` or `FAILED`

**Aggregations computed:**
- `today_tokens` — sum of tokens for today
- `last_30_days_tokens` — sum of tokens for rolling 30 days
- `today_cash` / `last_30_days_cash` — spend totals (from `consume_cash_after_voucher`)
- `top_methods[]` — top 3 methods by token volume
- `top_models[]` — top 3 models by token volume

**Failed records** (where `result` or `status` is not `SUCCESS`) are excluded from aggregations.

---

## Provider Metadata

| Field | Value |
|-------|-------|
| Provider ID | `MiniMax` |
| Display Name | `MiniMax` |
| Session Label | `Usage` |
| Weekly Label | `Monthly` |
| Logo | `/logos/minimax.svg` |
| Supports Credits | `true` |
| Default Enabled | `false` |
| Credential Fields | `api_key` + `group_id` |
| Importable | `true` |

---

## Credential Fields (Frontend)

| Field | Description |
|-------|-------------|
| `api_key` | MiniMax API key (`sk-` or `sk-cp-`) |
| `group_id` | Group ID from MiniMax developer console |

---

## Dashboard URL

```
https://platform.minimax.io/user-center/payment/coding-plan?cycle_type=3
```

---

## Region Detection

| Config Value | Region | API Base |
|-------------|--------|----------|
| `global`, `""`, unset | Global | `api.minimax.io` |
| `cn`, `china`, `china-mainland`, `china_mainland`, `mainland` | China Mainland | `api.minimaxi.com` |

---

## Error Responses

```json
{
  "base_resp": {
    "status_code": 401,
    "status_msg": "Unauthorized"
  }
}
```

**Common status codes:**
- `0` — Success
- `401` / `403` — Auth required (invalid/expired API key or cookie)
- `500` — Server error

**Common causes:**
- `AuthRequired` → invalid API key, missing group_id, or expired cookie
- `status_msg` in response → check `base_resp.status_msg` for details
