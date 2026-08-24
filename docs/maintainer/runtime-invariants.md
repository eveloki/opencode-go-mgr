[简体中文](runtime-invariants.zh-CN.md)

# Runtime Invariants

Detailed behavioral invariants of the running system, moved out of
`AGENTS.md` to keep that file skimmable. Read this before touching gateway
routing, aliases, Zen Free, the plan catalog, access keys, the outbound
proxy, or usage sync. The code remains the source of truth; this page is a
map of the semantics that are easy to get wrong.

## Gateway And Model Lists

- Core Gateway: Axum + Tokio + reqwest. The same port exposes OpenAI Chat
  Completions / Responses, Anthropic Messages, Gemini `generateContent`
  client entrypoints, and Claude Desktop alias entrypoints.
- Authenticated `GET /v1/models` first lists currently-routable published
  Aliases (OpenCode Go and the last successfully saved Zen Free model
  snapshot), then merges model IDs declared by eligible Custom accounts
  (enabled+verified+ready+non-empty Key); protected
  `GET /dashboard/api/v3/application-models` remains **Go-routable Aliases ∩
  current pricing snapshot** (highspeed inherits base-price rows; empty
  intersection is `[]`), excluding Custom. Neither GET path makes upstream
  requests; only when an admin clicks Zen Free “获取模型” (Fetch Models) on
  the Providers page does it hit the fixed official catalog. Custom IDs come
  from eligible accounts' declared capabilities. Unknown model names
  (neither published Alias nor eligible Custom ID) return `400` on all
  supported client formats.
- Gemini clients use `/v1beta/models/{model}:generateContent` or
  `:streamGenerateContent` (`/v1/models/...` also accepted), may auth with
  `x-goog-api-key`; Gemini is only a client format, and Gateway always
  converts to the known model's recommended upstream protocol. Unknown model
  names return `400` on Chat / Responses / Messages / Gemini; probing
  protocols is prohibited.
- Model protocol capabilities are hard-coded in `ocg_domain::protocol`'s
  `MODEL_PROTOCOLS` (`ocg-core` `kernel/protocol.rs` and
  `gateway/protocol.rs` are facade/host conversions): `preferred` aligns
  with the official Go docs endpoint table, `supported` comes from
  test-account probing conclusions. When client protocol ∈ supported it
  passes through, otherwise it routes to preferred; the request path must
  not probe protocols (prevents double billing). `grok-4.5` only has
  `supported = Responses` (Chat entry must convert). `gpt-5.6-luna`
  preferred is still Responses, but Chat can now pass through.
  `MODEL_PROTOCOLS` still only serves OpenCode Go; new `-free` IDs obtained
  from Zen Free refresh and unknown to the table are materialized as Chat by
  default, without using billed requests to probe protocols. The
  whole-document JSON conversion kernel is in `ocg-gateway`.

## Dashboard V3 And V2 Tombstones

- Dashboard V3 is mounted at `/dashboard/api/v3`. Control-plane changes
  require CAS (`expectedRevision`, and `processGeneration`; price writes
  also need `expectedPricingRevision`). `ConnectionInfo` is the only V3
  response DTO allowed to return the plaintext Key; Key-mutation responses
  do not include plaintext, and the client re-fetches `GET /connection`.
  `GET /contract` returns the current process's live revision / generation
  token, not a contract-export endpoint.
- Legacy protected V2 REST (`/dashboard/api/...`, excluding V3) returns
  structured `410` (`code=dashboardV2Removed`) to authorized dashboard
  sessions; anonymous requests get `401` first. The following semantics are
  separate from the tombstones and must not be mixed up: V2/V3 auth and
  session, browser WebSocket, inference entrypoints. The only preserved
  unversioned paths are the exact `auth/status|register|login|logout` and
  `browser/sessions/{token}/ws`.
- `crates/ocg-core/src/dashboard.rs` still handles SPA `index`/`assets` and
  preserves the above V2 auth and browser WS handlers; protected V2 REST
  handlers registered there are intercepted by tombstones and cannot host
  new features. Go/Zen protocol probing is at V3
  `POST /providers/{provider_id}/protocol-probes`. Custom account-level
  protocol probing remains on the retired V2 account path (returns `410`
  after authorization); do not revive V2 REST to “fill in probing”.

## Access Keys And Auth

- From schema v27 the authoritative table is SQLite `access_keys` (primary
  Key fixed id `gateway_keys::PRIMARY_KEY_ID` /
  `00000000-0000-0000-0000-000000000001`, name snapshot "Primary", never
  disable/delete; sub-Keys are non-primary rows, active limit 64,
  soft-delete keeps the name but clears plaintext). Sanitized config JSON
  stores `gateway_key` as `""` and is no longer DB-authoritative; in-process
  `AppConfig.gateway_key` and `GET /dashboard/api/v3/connection` still
  expose the live primary Key. Lifecycle is only through
  `/dashboard/api/v3/keys*` (including `POST /keys/primary/regenerate`).
  Primary/sub Key value mutual exclusion is enforced by
  `gateway_keys::ensure_primary_value_allowed`. `sub_gateway_keys` only
  appears in historical DBs before migration to v27 and is discarded after
  migration; do not describe it as the current authoritative table.
- Auth collects all non-empty candidate headers Bearer / x-api-key /
  x-goog-api-key; any match against the credential snapshot
  (`CoreStateInner.credential_snapshot`, including primary Key and enabled
  sub-Keys) passes, with attribution to the first match in candidate-header
  order; the same snapshot feeds the forward log name snapshot.
- Non-loopback listeners use single-admin login. Docker can be
  first-initialized via `OCG_ADMIN_USERNAME` and `OCG_ADMIN_PASSWORD` (both
  must be set together; setting only one causes a startup error); when not
  provided the first registered user becomes admin.

## Persistence

- SQLite, current schema **v27**. Historical DBs must first be canonically
  migrated to v26 before writing v27: copy the primary Key and
  `sub_gateway_keys` into `access_keys`, and drop the legacy five
  `usage_sync_*` columns on `accounts` (official usage-sync metadata is now
  in `provider_usage_sync_state`). Existing non-empty DBs generate a
  non-overwriting `data.sqlite.pre-v3.<UTC>.bak` plus `.sha256` sidecar in
  the same directory before any v27 write; fresh empty DBs are created
  directly at v27 without that copy. From v24 `forward_logs.route`
  (`auto`/`proxy`/`direct`, historical empty string = not recorded); v25
  provider model-catalog snapshot; v26 provider contract scopes and model
  protocol tables. GUI data directory is Windows `%USERPROFILE%\.ocg-mgr` or
  macOS/Linux `~/.ocg-mgr`; CLI defaults to `~/.ocg-mgr-cli`. Upgrade,
  backup hashing, and failed-rollback are covered in
  `docs/maintainer/storage-migration.md`.
- Downstream access root URL priority: non-empty `OCG_CLIENT_ROOT_URL` >
  SQLite manual value > frontend auto-derived from production origin / dev
  Gateway port. Environment-variable overrides are read-only and must not be
  written back to SQLite.

## Desktop Host

- Tauri v2 cross-platform tray app; the main window is hidden by default;
  tray/single-instance logic opens `http://127.0.0.1:<port>/dashboard/` in
  the system browser, and loopback listeners skip login automatically. Host
  capabilities (gateway lifecycle, native browser, autostart, Dock, updater)
  are registered into `CoreState` and **not** registered as
  `#[tauri::command]` / `invoke_handler`. Do not describe “there are still
  live Tauri invoke commands” as the current state.
- The Settings page manually checks the latest GitHub Release via protected
  `GET /dashboard/api/v3/settings/check-update`. Installed desktop builds
  that have the updater public key built in can download, verify the
  signature, and install in place; dev builds, CLI, Docker, and old versions
  not yet in the update channel keep the release-page / manual-overwrite
  path.

## Outbound Proxy

- Global outbound proxy is stored in `AppConfig`, with modes auto
  (system/environment), manual HTTP, force-direct, and per-model list
  (List). Non-List modes are mutually exclusive three-way; in List mode
  (`proxy_list_direction` allow/deny list + `proxy_list_models` known model
  ids), listed models take the direction exception segment (allowlist →
  proxy / denylist → direct), while unlisted models and non-model outbound
  (account test/verify, Zen Free manual model refresh, usage, pricing,
  updater download) take the direction default segment (allowlist → direct /
  denylist → proxy). List membership validation only runs at the dashboard
  `PUT /dashboard/api/v3/settings` write gate (non-empty, exact known ids,
  deduplicated); load paths tolerate stale values. Zen Free only hits the
  fixed `https://opencode.ai/zen/v1/models` when an admin explicitly
  refreshes, without a Key, not following redirects, and preserves the old
  snapshot if the refresh fails or returns empty. reqwest paths go through
  `ocg-core`'s `http_client.rs` facade into `ocg_infra::http`'s route set /
  `configured_builder`; the Tauri updater uses its `proxy` / `no_proxy` to
  align with the default segment and must not bypass per-account config.
  Forwarding picks routes from the request-entry snapshot; hot config
  switches do not affect in-flight requests. Custom HTTP (`custom.rs` +
  `custom_http.rs`, transport may reuse `ocg_infra::inference_http`) follows
  the same proxy policy; never follows redirects; never forwards
  dashboard/client auth; only constructs the configured Bearer or
  `x-api-key`; timeout is clamped by `connect_timeout_secs` to 5–60 seconds.

## Plan Catalog And Custom API Boundary

- Plan catalog is in `ocg_domain::provider`'s `BUILTIN_PLANS`: OpenCode Go,
  Zen Free, Command Code GOAT, SCNet Token Plans
  (`token-plan-basic|standard|premium`, Key prefix `sk-tp-`, official
  interactive-use limits), and Custom API. Internal identity is
  `provider_id` + `offering_id`. GOAT and all SCNet offerings are created as
  disabled `pending` drafts (`routable=false`);
  `POST /dashboard/api/v3/accounts/{id}/verify` returns `501` for these
  offerings. All persistence mutation paths (DB gate / dashboard / CLI
  shared services) reject setting `enabled=true` for any catalog
  `routable=false` offering before write, revision, or timestamp changes;
  the desktop UI only mutates via Dashboard V3 HTTP and has no separate
  invoke mutation path. Each `Database::open` only disables legacy GOAT and
  all three SCNet tier `enabled` rows without changing `updated_at`; Custom
  enabled state is preserved; only existing unverified GOAT is reset to
  `pending`. Go, Zen Free, and unknown pairs are unaffected. SCNet official
  available-model tables and endpoint snapshots are adapter inputs only and
  must not be published as client aliases.
- Custom API (`custom`/`api`, `routable=true`) is a trusted-admin
  destination: may configure any syntactically valid HTTP/HTTPS upstream
  (including LAN, loopback, and self-chosen destinations); rejects URLs with
  embedded credentials, query, or fragment; never follows redirects; never
  forwards dashboard/client auth; only constructs the configured Bearer or
  `x-api-key`; assembled endpoints must preserve the scheme/host/port/base-
  path prefix. After create/update it remains disabled `pending`; verify
  sends one minimal non-streaming request with the correct protocol to the
  first declared model, succeeds only on 2xx JSON object, does not
  discover/rewrite capabilities, and never auto-enables. Explicit enable is
  required after verify succeeds. Eligible accounts
  (enabled+verified+ready+non-empty Key) dynamically route their declared
  model IDs/protocols. Custom cost/usage is unpriced/unknown and does not
  deduct provider quotas. Changes to Key, base URL, or declared capabilities
  invalidate verification and disable the account; protocol and auth scheme
  cannot be changed after creation. Do not describe Custom's trusted-admin
  boundary using the GOAT/SCNet anti-abuse framing.

## Aliases

- Client aliases live in `ocg_gateway::alias` (`ocg-core` `alias.rs` is the
  compatibility facade): preferred stable lowercase kebab-case (following Go
  model IDs); case folding is acceptable; raw IDs containing `/`, `_`, or
  whitespace are treated as raw IDs and must not be folded to kebab. A raw
  ID with exactly one registry mapping is pinned to that mapping before
  routability is checked; an unroutable mapping is recognized but cannot
  produce a production route. Overlapping raw IDs (including an eligible
  Custom declared ID conflicting with another Plan mapping) return
  `ambiguous_model_id` without calling upstream. Zen Free's saved snapshot
  publishes both the original ID `foo-free` and the suffix-stripped Alias
  `foo` for each `foo-free`; shared aliases pick among Go/Zen candidates in
  account-card persistence order. Eligible Custom declared IDs overlay into
  resolution and `/v1/models`, but must not steal already-published Go/Zen
  aliases. Forward logs distinguish `requested_model` (client-requested
  alias/model name), `resolved_alias`, and `upstream_model`; `native_cost_*`
  is optional; do not invent a `requested_alias` field. Claude Desktop's
  three role aliases are still rewritten before alias resolution;
  `/claude-desktop/v1/models` only publishes those three roles.

## Zen Free

- Zen Free is a special built-in account without a Key; it only has an
  account-card enable switch, and no longer has `deny` / `explicit` /
  `prefer` or auto-prefer policies. When an admin clicks “获取模型” (Fetch
  Models) on the Providers page it requests the fixed official catalog,
  keeps only normalized valid IDs ending in `-free`, persists the last
  successful snapshot, and generates suffix-stripped aliases; refresh
  failures or empty results do not overwrite the old snapshot. Turn the card
  off when Free is not needed; when enabled it is selected along with other
  accounts by card order. Protocol probe controls are also on the Providers
  page, not the account card. Zen Free and Go use independent
  `cooldown_free_until`; Zen Free quotas are shared by egress IP, after a
  429 the entire Free channel cools down without swapping Keys, and routing
  continues trying subsequent compatible cards, returning shared cooldown
  only when only Free candidates remain. Inference `401` is returned to the
  client as-is, without swapping credentials or writing `auth_error`;
  dashboard Ping / Key verification 401s still record `auth_error`. Free
  channel success rows record `cost_state=free` and do not count against Go
  quotas. Go's `ox-alpha-free` is still handled by Go's static mapping and
  counts as `unpriced`, not Free.

## Claude Desktop

- Claude Desktop uses `/claude-desktop/v1/messages` and
  `/claude-desktop/v1/models`; `sonnet`, `opus`, and `haiku` mappings are
  stored in `AppConfig.claude_desktop_models`, managed by protected
  `GET/PUT /dashboard/api/v3/claude-desktop/models`.

## Managed Accounts (Beta)

- `setup_step` sequence is `google_account` (UI: login identity, skippable)
  → `opencode_registration` → `payment` → `key_verification` → `ready`.
  `PATCH /dashboard/api/v3/accounts/{id}/setup` allows advancing one step or
  rolling back to an earlier step; skipping steps or jumping directly to
  `ready` is prohibited. Draft creation may edit the invite link and write
  it back to `opencode_invite_url` (`DEFAULT_OPENCODE_INVITE_URL` is the
  demo default). Browser targets include Google/GitHub registration and
  login, the invite URL, and the console at `https://opencode.ai/auth`. The
  managed page can open the browser via dashboard HTTP; the desktop native
  browser is a Host hook, not a WebView invoke.

## Usage Sync

- Quota for completed accounts: the official
  `https://opencode.ai/zen/go/v1/usage` (`go_usage.rs`) is the periodic
  calibration baseline; local `forward_logs` still do real-time estimation
  after the last successful calibration. `usage_sync.rs` coordinates manual
  and background paths: ready+enabled accounts with local activity in the
  last ~24h reconcile about hourly, inactive ones about daily;
  disabled/not-ready/empty-Key accounts are not auto-refreshed. Global
  concurrency 1, with jitter and injectable clock/jitter/fetch seams; no
  startup thundering herd. Manual
  `POST /dashboard/api/v3/accounts/{id}/usage/refresh` is still available;
  server-side throttling is 15s per account (success or failure counts),
  with concurrent deduplication, returning Retry-After / `next_allowed_at`;
  failures keep the last baseline and last-success. When local maximum Go
  usage ≥80%, accelerate reconciliation at most once every 15 minutes. Real
  inference 429s still write the existing cooldown/selector and additionally
  schedule an official reconciliation ~1–2 minutes later (not inline);
  official failures or `status=rate-limited` never write inference cooldown.
  After success, reschedule by earliest `resetsAt` (plus bounded jitter)
  respecting active/inactive cadence. Failure backoff: 5m → 15m → 1h → 6h.
  Sync metadata lives in `provider_usage_sync_state` (`accounts.usage_sync_*`
  is no longer used). Shared implementation includes CAS / three-window
  atomic calibration and global proxy. The public Go docs have not listed
  this endpoint. `console_usage.rs` is frozen and deprecated; do not delete
  until at least two minors later and there is stable real-account evidence.
  Do not introduce CDP automation for refresh.

## Pricing, Container, And CI Notes

- Pricing is managed via protected `GET /dashboard/api/v3/pricing`,
  `PUT /dashboard/api/v3/pricing/multipliers`, and
  `POST /dashboard/api/v3/pricing/refresh`; it only hits
  `https://opencode.ai/docs/go/` when the user clicks refresh, and must not
  auto-poll.
- After a public GitHub Release is published,
  `.github/workflows/container.yml` builds and smoke-tests `linux/amd64` and
  `linux/arm64` images on native amd64 (`ubuntu-24.04`) and arm64
  (`ubuntu-24.04-arm`) runners, pushes each architecture by digest, then
  merges them into a single multi-arch OCI index under the same tag,
  published to `ghcr.io/klarkxy/opencode-go-mgr`. Compose uses this image by
  default; local source builds need `OCG_IMAGE=ocg-manager:local` then
  `docker compose up -d --build`.
- `.github/workflows/quality.yml` splits into three parallel jobs on PR /
  `main`: Web (includes `pnpm run contract:v3:check`, frontend
  tests/types/lint), Linux workspace Rust tests/Clippy (stubs `dist/`,
  compilation includes Tauri crate), and Windows Tauri-targeted tests (stubs
  `dist/`, does not run Vite). A `release.yml` manual candidate (even when a
  tag ref is selected) is always unsigned and may build only the selected
  platform; only a `v*` tag push event builds all three platforms and reads
  repository signing secrets. A tag push is treated as single-maintainer
  explicit release authorization: the workflow validates that the attachment
  set matches assembled artifacts name-by-name (quantity derived from
  artifacts, not hard-coded), updater signature, public-key continuity, and
  GitHub server-side digest, then automatically publishes the same unchanged
  draft.
- Container runs fixed as UID/GID `10001` and includes `LICENSE`; Compose
  passes through optional `OCG_MANAGER_ENCRYPTION_KEY` to support explicit
  key recovery, but normal deployments still prefer keeping
  `.encryption-key` in the volume.
---

[Maintainer guide index](../MAINTAINER.md) · [简体中文](runtime-invariants.zh-CN.md) · [Docs index](../README.md)
