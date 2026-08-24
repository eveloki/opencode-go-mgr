[简体中文](routing.zh-CN.md)

# Routing, Cost, And Failover

## Account Selection And Failover

Accounts are tried in **list order**, which can be reordered and persisted
from the Accounts view. The selector skips:

- Disabled accounts.
- Accounts that are cooling down.
- Accounts that have already failed during the current request (e.g. with a
  `429`).
- Accounts whose saved provider contract has no enabled upstream protocol
  for the resolved model.

A `429` with a recognized `Resets in …` phrase writes `cooldown_until` and
the gateway tries the next account. `403` fails over without writing a
cooldown. OpenCode Go and Zen Free `401` is returned as-is and does **not**
rotate accounts or persist `auth_error` — OpenCode Go uses 401 for both
invalid keys and `ModelError` ("model is not supported"), so treating it as a
key breaker would interrupt the client and strand a valid account. Custom API
`401` does rotate to the next eligible card and persists `auth_error`.
Managed-account Key verification and Custom **Verify connection** still
record `auth_error` when they get a 401. CLI `key ping` prints the real
upstream status without writing that field. A DNS/TCP/TLS connection
failure that proves the request was not sent is retried once on the same
account, including for streaming calls.

When **Conversation sticky** is on, a matching conversation key is tried
before the base routing mode. The header `X-OCG-Conversation-Id` wins when
present; otherwise the gateway fingerprints system / tools / the first user
message. No usable key means the selected strict-priority, global-sticky, or
round-robin mode runs unchanged.

The gateway does not replay `408`, `5xx`, post-connect transport failures,
response-body timeouts, or interrupted streams. Ambiguous failures are
reported as `upstream_outcome_unknown` and logged as `outcome_unknown`,
because the upstream may already have consumed quota. When every enabled
account is cooling down, the gateway returns `429` with the soonest reset
time.

## Cost Accounting

The 5-hour, weekly, and monthly bars are local estimates, driven by the
requests the gateway actually forwards — not by the upstream's authoritative
billing. Token rates, window limits, and each model's `Usage` come from the
active OpenCode Go USD snapshot.

- The official multiplier defaults to `monthly limit / Usage`. A user can
  override it for a temporary promotion; subsequent requests use the active
  persisted value, and refresh never overwrites it without confirmation.
- `deepseek-v4-pro` (DS V4 Pro), `deepseek-v4-flash`, `mimo-v2.5-pro`, and Grok
  currently have a `$15` Usage allowance, which corresponds to a
  `60 / 15 = 4x` multiplier.
- The applicable local MiniMax adjustment is applied last. No supplier API
  price, CNY value, or exchange rate participates in the calculation.

Edge cases in the log:

- Without a streaming usage chunk (after the gateway has requested
  `include_usage` on Chat streams), the row ends with `success_no_usage`.
- Models absent from the snapshot are still forwarded, but finish as
  `success_unpriced`, display no quota cost, and do not enter quota totals.
- Zen free models finish as `success` with `cost_state=free`: tokens are
  recorded, quota cost stays empty, and they do not enter Go quota totals.
- Custom API forwards finish with `cost_state=unknown`, display no quota
  cost, and do not debit any provider quota.
- Pre-snapshot successful rows retain their old value and are marked as a
  legacy estimate; they are never recalculated.
- A manually saved percentage becomes the baseline for that window. Official
  refresh (manual **Refresh quota** or adaptive sync) on a ready Key or managed
  account overwrites the baseline with official OpenCode usage percentages.
  Successful priced costs recorded afterward accumulate until the next manual
  calibration or official refresh. A real inference `429` only writes an
  independent cooldown and affects account selection; it does not rewrite the
  usage baseline, but it does schedule a later official reconciliation.
- An `outcome_unknown` row means the upstream may have completed and charged
  the request while the gateway lost the response; the request is not
  retried and its local cost stays unknown.

The dashboard always pairs a bar with the account's cooldown state — see the
next section.

## True And False Circuit Breakers

- **False circuit breaker (local estimate).** The local estimate is a
  *signal*, not a stop sign. When it reaches the limit, the gateway **keeps
  sending** requests with that account. Local accounting and upstream
  billing/reset boundaries may not match, so a full local bar is a warning,
  not proof that the upstream account is blocked.
- **True circuit breaker (upstream 429).** The gateway stores the upstream
  error, parses the `Resets in …` phrase from the response, writes
  `cooldown_until`, and tries the next available account. The known 5-hour,
  weekly, and monthly limit messages use the reset duration reported by the
  upstream for that cooldown only; they do not rewrite the matching usage
  baseline. During cooldown the matching bar is forced to 100% in the
  dashboard; after cooldown, local priced costs continue from the existing
  baseline until the next manual calibration or official refresh. An
  unrecognized 429 falls back to a five-minute cooldown without changing
  any usage baseline.
- **No account available.** If every enabled account is cooling down, the
  gateway returns `429` with the soonest reset time.
- **Dashboard display.** While a true circuit breaker is active, the matching
  5-hour, weekly, or monthly bar is forced to 100% and marked as an error,
  even when the local estimate is lower. The account becomes eligible
  automatically after `cooldown_until`, or immediately after you reset its
  cooldown in the dashboard.

## Zen Free models

Zen Free is one credentialless account card with one enable switch. There is
no separate Deny / Explicit / Prefer policy. Disable the card if you do not
want Free traffic; otherwise its position in the account list is its routing
priority.

**Refresh model catalog** on **Providers** calls the official keyless Zen
model directory only on user request. The backend keeps only IDs ending in
`-free`, saves the successful snapshot, and derives one additional Alias by
removing that suffix. For example, `mimo-v2.5-free` is accepted both as
itself and as `mimo-v2.5`; requests for the shared Alias follow account-card
order across Go and Zen. **Providers** shows the saved catalog and each
model contract. A failed or empty refresh leaves the last saved snapshot
active. The reserved Go model `ox-alpha-free` is excluded from Zen discovery
so it remains Go-only and unpriced.

Free and Go cooldowns are **independent**. Zen Free sends no authentication
headers. Its promo quota is shared per egress IP, so a Free `429` cools the
whole Free channel and does not rotate keys. Routing then continues to later
compatible account cards in saved order; a Free-only model returns the shared
cooldown. Successful Free rows keep token counts, use `cost_state=free`, and
do not enter Go quota totals. Free models are promotional and may use request
data to improve models — do not submit confidential content.

---

[User guide index](../USER.md) · [简体中文](routing.zh-CN.md) · [Docs index](../README.md)
