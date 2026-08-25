[简体中文](accounts.zh-CN.md)

# Accounts

Accounts is the tenant list. Every card binds one **Plan** (provider + offering)
and, when the Plan demands it, one credential. Quota authority is Plan-specific:
OpenCode Go counts usage by account **Key**, Zen Free shares free cooldown by
egress IP, and Custom API keeps no provider-side quota — one of the three has
to be the roommate who never buys milk. All cards share one manually persisted
global order; capability filtering runs first, then strict priority, global
sticky, and round-robin all read from that order. There is no per-model quota
pool.

**Accounts** owns identity, the account **Key**, verification, enabled state,
card order, managed registration, and local usage / calibration / cooldown. Each
card shows a read-only contract summary (effective protocols, or a disabled /
unroutable notice) and an **Open provider** deep link into
**Providers**. Catalogs, protocol probes, Chat Completions / Responses /
Messages switches, and scoped pricing live on **Providers**, not here.

The registry is sealed. Built-in Plan families are:

| Family | Plan | Live routing | Notes |
| --- | --- | --- | --- |
| OpenCode Go | `opencode` / `go` | Yes | One officially distributable API key per card; managed signup remains Beta |
| Zen Free | `opencode-zen-free` / `anonymous-free` | Yes | One credentialless, anonymous singleton; sortable and enableable, not deletable; quota shared by egress IP |
| Command Code GOAT | `command-code` / `goat` | Yes | Create disabled `pending`; verify the Key through official `GET /models`, then explicitly enable; model access defaults to the included `goat` catalog and can switch to `all` |
| SCNet Token Plans | `scnet` / `token-plan-basic`, `token-plan-standard`, `token-plan-premium` | No | Archived; legacy rows remain disabled and cannot verify, enable, route, or report usage |
| Custom API | `custom` / `api` | Yes | Trusted-administrator destination; create/update stay disabled `pending`; verify then explicit enable; eligible declared IDs appear on `/v1/models`; unpriced/unknown cost, no quota debit |

Every persistent mutation path rejects `enabled=true` for a catalogued
`routable=false` offering (all SCNet tiers) before it mutates the row,
revision, or timestamps. GOAT and Custom are catalog-routable, but
create/update still leave the card disabled and `pending`; enable is rejected
until verification status is `verified`. Disabled drafts remain saveable. The
desktop UI uses Dashboard V3 HTTP and has no separate Tauri invoke mutation
path.

Use only the official provider API **Key** for OpenCode Go or Command Code
GOAT. Browser cookies and reverse-proxy credentials are not account Keys. GOAT
is a separate provider mapping and its Key is sent only to the fixed Command
Code Provider API, never to OpenCode. SCNet is archived and accepts no new
credentials or routing. Custom API is a separate trusted-administrator
destination and must not send its key to an OpenCode endpoint.

Legacy SCNet Token Plan rows and their historical acknowledgement records are
retained for compatibility, but the family is archived. It has no add, verify,
enable, routing, model-publication, pricing, or usage path.

GOAT verification performs one authenticated, non-billable `GET /models` and
saves the returned account catalog without enabling the card. After verify,
explicitly enable the account. The account can use the included `goat` model
set (default) or the full `all` catalog. Changing the Key invalidates
verification and disables the account; switching `goat`/`all` does not.

Custom API is a live trusted-administrator destination. The card stores a base
URL, one upstream protocol (Chat Completions, Responses, or Messages), one auth
scheme (Bearer or `x-api-key`), and at least one model capability. Use **Fetch
models** only as an explicit form action: it sends `GET /models` to the
configured base URL with the entered Key (or, while editing, the stored Custom
Key), merges valid returned IDs into the editable list, and does not save,
verify, enable, or otherwise change the account. The fetch is bounded and may
report a truncated result; manual model IDs remain supported.

A trusted administrator may configure any syntactically valid HTTP or HTTPS
origin, including LAN, loopback, and other self-selected destinations.
URL-embedded credentials, query strings, and fragments are rejected. The gateway
never follows redirects, never forwards dashboard or client authentication, and
constructs only the configured Bearer or `x-api-key` credential. Joined
endpoints stay inside the configured scheme, host, port, and base-path prefix.
Custom HTTP uses the same process-wide Direct / Manual / Auto proxy policy;
connect and request timeouts are bounded from the configured connect timeout
(clamped 5–60 seconds).

Create and update leave the card disabled and `pending`. Verification sends one
protocol-correct, non-stream, token-bounded JSON request to the first declared
model; only a `2xx` JSON object succeeds. Verification does not discover or
mutate capabilities and never auto-enables the account. You must enable the card
explicitly after a successful verify. Eligible accounts (enabled + verified +
ready + non-empty key) expose their declared model IDs on authenticated `GET
/v1/models` and can be selected for those IDs. Declared capability IDs are both
the client-facing names and the upstream model names; matching is case-insensitive
for kebab IDs, and names with `/`, `_`, or whitespace never fold onto a kebab
alias. Custom overlay never steals a published Go or Zen Free alias. Overlap
with another Plan's unique raw ID returns `ambiguous_model_id` and does not call
upstream. Undeclared names stay unknown (`400`). Changing the base URL, key, or
declared capabilities re-pends verification and disables the account. Upstream
protocol and auth scheme are fixed at create. Custom traffic is unpriced: logs
record `cost_state=unknown` with no quota debit, and Custom has no provider usage
refresh. `MODEL_PROTOCOLS` remains Go-specific; Custom converts the client
protocol to the account's declared upstream protocol.

**Add account** is a grouped plan list with a detail pane (**Ready to add** /
**Draft plans** / **Unavailable**), not a card grid. Zen Free is a backend-owned
singleton and is not listed there; enable or disable it on the account list.
Selecting OpenCode Go still offers **Import existing Key** and **Register new
account (Beta)** in the detail pane:

- A **Key account** stores one officially distributable OpenCode Go API key.
- A **managed account** immediately creates a disabled, recoverable draft, then
  runs the wizard through optional sign-in identity, invite registration,
  payment, and key verification. The draft and current step are persisted to
  SQLite, so closing the page or restarting the service does not lose the flow.
  Pending accounts cannot be selected by the gateway and do not expose usage,
  verify, or enable controls.

Managed signup and isolated browser profiles are **Beta** features. They have
not been thoroughly tested; do not rely on them in production.

When you create a managed draft, the form shows the **invite URL** (prefilled
from Settings; fresh installs may ship a demo default). Edit it in place: it must
be an HTTPS URL no longer than 2,048 characters, contain no username or password,
and use exactly `opencode.ai` or `console.opencode.ai` as its host. If it differs
from Settings, it is written back to **Settings → OpenCode Go invite URL**.
Changes affect later invite-page opens only; they do not rewrite completed
accounts. Replace the demo default with your own invite link before a real
signup, or referral credit goes to the link owner.

The managed wizard is intentionally manual (no password autofill, no payment
clicks, no automatic key extraction):

1. **Sign-in identity (optional).** Sign up for Google or GitHub only if you
   need a new account; otherwise **skip this step**. OpenCode sign-in can also
   finish on the next step.
2. **Invite registration.** Open the invite URL in the same isolated profile and
   complete OpenCode sign-in/registration with Google or GitHub.
3. **Payment.** Confirm the plan and amount in the console; only you complete
   payment on the page.
4. **Verify Key.** Copy the key from the console, paste it, and run a real
   upstream probe.

Click an earlier finished step in the step bar to **rewind**; forward progress
still uses each step's primary button. A `2xx` verification completes and enables
the account. A `429` also proves that the key is valid, completes the account,
and records the current cooldown. `401`/`403`, network errors, and `5xx`
responses leave the account at key verification so you can correct it and retry.

Every account has a durable, isolated browser profile. Desktop builds launch an
external Chromium-family browser: Windows prefers Edge and then Chrome; macOS
checks Chrome, Edge, and Chromium; Linux desktop searches `PATH` for Chrome,
Chromium, or Edge. It uses only `browser-profiles/<account_id>`, first-run
suppression, and a new window; it does not enable CDP, automation,
`--no-sandbox`, or weakened web security. Older `profiles/<account_id>` WebView
data is deliberately not imported, so the first open after upgrading requires
another login.

Every completed account has **Open OpenCode console**
(`https://opencode.ai/auth`). A legacy account starts with a blank isolated
profile the first time; sign in once and its cookies remain available.
Google/GitHub and OpenCode cookies belong to different domains, but both stay in
the same account profile.

Resetting browser identity first closes that account's browser and removes both
new and legacy profile directories. A completed account keeps its key and is only
signed out of the console; a pending managed account also returns to the sign-in
identity step. Deleting an account likewise deletes its cookies/profile, and the
confirmation states this explicitly. That login state can then be recovered only
from a backup or by signing in again.

Each completed OpenCode Go card shows the account name, cooldown state, and the
5-hour / weekly / monthly usage bars driven by local accounting. Zen Free has its
own anonymous, egress-IP-shared free cooldown rather than a key quota.

- **Usage baselines.** Type a percentage or drag a bar to set its current
  real-world usage baseline. After the value is saved, successful request cost
  recorded by OCG Manager continues to accumulate above that baseline. Reaching
  100% is still only a warning; it does not stop the gateway from selecting the
  account. Manual calibration stays available for every ready account.
- **Refresh quota (ready Key and managed accounts).** Official OpenCode usage
  (`/zen/go/v1/usage`) is only a periodic calibration baseline; local forward-log
  costs stay the live estimator. Active ready accounts reconcile about hourly,
  inactive ones about daily; disabled, unfinished, or empty-key accounts are
  never auto-refreshed. Opening this page or starting the gateway does not force
  a fetch: new schedules are spread across the first 0–15 minutes. **Refresh
  quota** runs the same path on demand with a 15-second per-account server
  throttle (Retry-After / next-allowed). The card shows the last successful
  official sync time and any temporary retry wait. Local estimates that reach
  ≥80% may trigger one expedited sync per 15 minutes. A real inference `429`
  still writes the existing cooldown/selector state and additionally schedules
  an official reconciliation about 1–2 minutes later; official failures or
  `status=rate-limited` never write inference cooldown. Failures keep the
  previous baseline and last-success timestamp. The request uses the same global
  outbound proxy as other dashboard fetches.
- **Identity and credentials.** The name is the account's required primary
  display label. The login account field is optional; on Key-account creation,
  entering it first copies it into the name until you edit the name yourself.
  Optional freeform notes live in **Edit account**. They can stay empty and do
  not affect routing or quota. The dashboard stores the account key but does not
  collect or manage third-party login passwords.
- **Purchase date.** New accounts default to the browser's current date, and the
  value remains editable. The managed wizard also writes the purchase date when
  payment advances to key verification. Expiry is the same day in the next
  natural month, clamped to that month's last day when necessary:
  `2026-01-31` expires on `2026-02-28`. Accounts and Dashboard show days
  remaining, due today, or days expired. This is informational only and never
  disables an account or prevents the gateway from selecting it.
- **Priority order.** Use the drag handle on an account card to persist its
  priority with a mouse, touchscreen, or pen. When the handle has keyboard focus,
  the Up and Down arrow keys move the account as well. Dashboard, the Logs
  account filter, CLI listings, and the gateway selector all consume this same
  SQLite-backed order.
- **Cooldown reset.** You can reset a cooldown manually from this view. The bar
  snaps back to its local estimate as soon as the cooldown is cleared.

---

[User guide index](../USER.md) · [简体中文](accounts.zh-CN.md) · [Docs index](../README.md)
