[简体中文](MAINTAINER.zh-CN.md)

# Maintainer Guide

This guide is for people changing code, building releases, debugging the
gateway, and validating the desktop bundles. It describes the implemented
V3 architecture and operating contracts at HEAD: four-layer crates,
Dashboard V3, schema v27, host lifecycle, tests, and release. User-facing
product behavior lives in [USER.md](USER.md). Schema v27 operator recovery
is in [MAINTAINER-v3-migration.md](MAINTAINER-v3-migration.md).

## Table Of Contents

- [Layout](#layout)
- [Prerequisites](#prerequisites)
- [Development](#development)
- [Checks And Builds](#checks-and-builds)
- [Architecture](#architecture)
- [Lifecycle Classes](#lifecycle-classes)
- [HTTP Routes](#http-routes)
- [Storage And Migration](#storage-and-migration)
- [Extension Procedure](#extension-procedure)
- [Failure Modes](#failure-modes)
- [Upgrades And Database Migrations](#upgrades-and-database-migrations)
- [Release Artifacts](#release-artifacts)
- [CI Workflow](#ci-workflow)
- [Release Procedure](#release-procedure)
- [Release Validation Checklist](#release-validation-checklist)
- [Known Debt](#known-debt)
- [Deliberate Non-Goals](#deliberate-non-goals)
- [Coding Conventions](#coding-conventions)

## Layout

```
ocg-manager/
├── crates/
│   ├── ocg-domain/     Pure identities, catalogs, protocol policy, Zen normalize
│   ├── ocg-gateway/    I/O-free alias, AttemptSpec, classify, selector, JSON convert
│   ├── ocg-infra/      Catalog-stripped crypto, proxy HTTP, inference HTTP, SQLite log SQL
│   ├── ocg-core/       Composition / control plane: state, SQLite, Dashboard V3, adapters, executor
│   ├── ocg-cli/        Headless CLI (`ocg-manager-cli`): serve / key / status
│   └── ocg-browser-worker/  Linux Chromium sidecar control service (independent of ocg-core)
├── browser/           Xvfb, Openbox, x11vnc, and noVNC startup script
├── src/               Vue 3 dashboard (TypeScript, naive-ui, Vite, Pinia)
│   ├── App.vue        Shell, auth, side rail, header
│   ├── api/
│   │   ├── dashboard-v3.ts            Hand-written `/dashboard/api/v3` client
│   │   ├── generated/dashboard-v3.ts  Types generated from the frozen JSON Schema
│   │   ├── dashboard.ts               Presenter over V3 for existing pages
│   │   ├── dashboard-presenters.ts    Field projection (camelCase wire → page shapes)
│   │   ├── http.ts                    Domain-neutral fetch helpers
│   │   └── tauri.ts                   Historical name; leftover types/helpers for some tests — not Tauri invoke
│   ├── stores/        session, controlPlane (CAS tokens), connection, accounts, providers, settings
│   ├── components/    Account cards, managed wizard, pricing catalog, …
│   ├── i18n/          i18n setup + per-locale message tables + tests
│   ├── styles/        Theme tokens, design-system overrides
│   └── views/         Dashboard, Keys, Accounts, Providers, Applications, Logs, Settings, BrowserSession
├── src-tauri/         Tray host: Native Browser, Gateway Lifecycle, Desktop Settings, Updater
│   └── src/host/      Process-owned capabilities; no `invoke` commands
├── schema/            Frozen Dashboard V3 JSON Schema (`dashboard-api-v3.schema.json`)
├── docs/              USER / MAINTAINER / anti-abuse (EN+ZH), CONTRIBUTORS, index, v27 recovery note
├── scripts/           release, updater manifest, dashboard-v3-contract, smokes, …
├── AGENTS.md          Facts and constraints for AI coding assistants
├── DESIGN.md          Design system source of truth (linted in CI)
├── .github/workflows/ quality.yml, release.yml, container.yml
├── docker-bake.hcl    Parallel container smoke targets used by container.yml
├── Dockerfile         Multi-stage headless gateway image
├── Dockerfile.browser Chromium/noVNC sidecar image
├── compose.yaml       Source-build and image Compose service definition
└── compose.example.yaml  Pull-only Compose example attached to each Release
```

Workspace members are declared in the root `Cargo.toml`: `ocg-domain`,
`ocg-gateway`, `ocg-infra`, `ocg-core`, `ocg-cli`, `ocg-browser-worker`,
`src-tauri` (package `ocg-manager`). Binary names: `ocg-manager-cli` and
the Tauri app. Current workspace version is `1.8.2`; `rust-version` is
`1.85.0`; edition is `2024`.

The live Vue data path is HTTP Dashboard V3 (`src/api/dashboard-v3.ts` and
the presenter in `src/api/dashboard.ts`). There is no `src-tauri/src/commands/`
module and no `tauri::generate_handler` / `#[tauri::command]` surface.
`src/api/tauri.ts` is a leftover filename still imported by some unit tests
for historical types; it is not `invoke()` and not the production client.

## Prerequisites

Use Node.js 22 (the CI baseline), pnpm 10.29.2 (`packageManager` in
`package.json`), and Rust 1.85 or newer. Native build dependencies vary by
runner; treat `.github/workflows/release.yml` as the source of truth. The
current Linux runner installs `libwebkit2gtk-4.1-dev
libayatana-appindicator3-dev librsvg2-dev libxdo-dev libssl-dev patchelf
libfuse2 xvfb xauth xdg-utils dbus-x11`.

## Development

Exit any running release tray app so the single-instance lock and port `9042`
are free, then start the full development stack:

```bash
pnpm install
pnpm run dev
```

`pnpm run dev` runs `tauri dev`. On Windows the `predev` script
(`scripts/free-dev-port.mjs`) inspects `127.0.0.1:30001` and stops any stale
Vite process from a previous run. Tauri starts Vite and waits for the gateway
to be ready, then opens `http://127.0.0.1:30001/dashboard/`. Vite proxies
`/dashboard/api` (including WebSockets) to `http://127.0.0.1:9042`.

- Frontend (Vue, CSS, TypeScript) changes use Vite HMR.
- Rust changes use Tauri's watcher plus Cargo's incremental compiler, then
  restart the process. Rust code is **not** replaced inside a running
  process — expect a restart.

After cloning, enable the shared git hooks once (also runs from `pnpm install`
via the `prepare` script):

```bash
pnpm run hooks:install
# equivalent: git config core.hooksPath .githooks
```

When a commit stages any `*.rs` file, `.githooks/pre-commit` runs
`cargo fmt --all` and re-stages those Rust files so the commit stays
rustfmt-clean (same tool CI checks with `cargo fmt --all -- --check`).

## Checks And Builds

```bash
pnpm install
pnpm run test
pnpm run build:web
pnpm run design:lint
pnpm run contract:v3:check
pnpm run build
```

- `pnpm run build:web` is the **frontend-only** production build
  (`vue-tsc && vite build`). Use it when you only need to validate the
  dashboard.
- `pnpm run test` runs `pnpm run test:web` (Node `--experimental-strip-types`
  over `scripts/*.test.mjs` and `src/**/*.test.ts`), `vue-tsc --noEmit`,
  `vite build`, then `cargo test --workspace --locked`.
- `pnpm run test:rust` is the locked workspace Rust suite by itself.
- `pnpm run contract:v3:check` regenerates the Dashboard V3 JSON Schema from
  `ocg-core`'s `export_dashboard_v3_schema` example and fails if
  `schema/dashboard-api-v3.schema.json` or
  `src/api/generated/dashboard-v3.ts` drifted. Write with
  `pnpm run contract:v3:generate`.
- `pnpm run design:lint` runs the `@google/design.md` linter against
  `DESIGN.md`.
- `pnpm run build` is reserved for **release validation**. It runs
  `scripts/release.mjs`, which builds the current supported native platform
  and atomically replaces `release/` only after every expected file passes
  validation. The previous `release/` is preserved on failure. Cargo's
  incremental build cache is **not** erased. Release binaries use thin LTO
  (`[profile.release]` in the workspace `Cargo.toml`) so native CI linking
  stays bounded.

### Rust Checks

```bash
cargo fmt --all -- --check
cargo check --workspace --all-targets
cargo test --workspace --locked
```

The first command checks formatting without changing files. Run
`cargo fmt --all` to apply formatting. With hooks enabled, staged Rust
commits auto-run that format step via `.githooks/pre-commit`.

For focused work:

```bash
cargo test -p ocg-domain
cargo test -p ocg-gateway
cargo test -p ocg-infra
cargo test -p ocg-core
cargo test -p ocg-manager-cli
cargo test -p ocg-browser-worker
cargo test -p ocg-manager --lib
cargo test -p ocg-core gemini
cargo test -p ocg-core claude_desktop
cargo test -p ocg-core dashboard_v3
cargo test -p ocg-core v3_runtime_invariants
```

`ocg-domain` / `ocg-gateway` crates compile their production-source
dependency and purity guards as ordinary `cargo test` cases. Host
characterization lives in `crates/ocg-core/tests/fixtures/v3/requirement_map.md`
and the copy at `src-tauri/tests/fixtures/v3/host_requirement_map.md` /
`crates/ocg-cli/tests/fixtures/v3/host_requirement_map.md`.

Run the CLI in a sandbox first when testing real account flows:

```bash
ocg-manager-cli --data-dir /tmp/ocg-cli-test key add smoke sk-smoke
ocg-manager-cli --data-dir /tmp/ocg-cli-test key list
ocg-manager-cli --data-dir /tmp/ocg-cli-test serve --port 19042
```

The CLI surface is `serve` / `key` / `status` only. `key add` creates an
enabled ready OpenCode Go card through `account_control::create_go_api_key`
and bumps that process's `settings_revision`. It cannot create Custom
accounts, sub keys, or settings. Direct `Database::update_account` still
does not bump revision; that is intentional and is not the CLI path.

### Frontend Checks

Frontend unit tests live next to the code they cover (`src/**/*.test.ts`)
and run with Node's experimental `--experimental-strip-types` flag — no
extra test runner is required. Script-level tests live in
`scripts/*.test.mjs` (release helpers, Dashboard V3 contract, container
publish). Pair them with `pnpm run build:web` and
`pnpm run contract:v3:check`.

The application guides are driven by the 16 entries in
`src/views/application-guides.ts`. When changing that registry, check the
guide count, unique IDs, protocol endpoints, the display/copy masking
difference, and the Claude Desktop three-role persistence behavior.

The side rail is Dashboard / Access Keys / Accounts / Providers /
Applications / Logs / Settings. A `pricing` query is a legacy alias for
Providers. `BrowserSession` is a session overlay, not an eighth rail item.

## Architecture

### Four-layer crates

Provider extension is **static and sealed**. There is no plugin slot, JSON
DSL, or user-defined adapter implementation. Custom API is
`ProviderAdapterKind::ConfigurableHttp`, not a base class other adapters
inherit from.

```text
ocg-gateway -> ocg-domain
ocg-core    -> ocg-domain + ocg-gateway + ocg-infra
ocg-cli     -> ocg-core
src-tauri   -> ocg-core
```

`ocg-domain` and `ocg-infra` have no internal `ocg-*` dependencies.
`ocg-browser-worker` is a separate process with no internal crate dependency.

| Crate | Owns | Must not own |
| --- | --- | --- |
| `ocg-domain` | IDs, `BUILTIN_PLANS`, `ProviderAdapterKind`, protocol tables, Zen ID normalize, account/setup enums | DB, `CoreState`, reqwest, rusqlite, tokio, axum, filesystem, clocks |
| `ocg-gateway` | Alias registry, `AttemptSpec`, classify policy, secret-free selector, whole-document JSON convert | DB, `CoreState`, raw reqwest, rusqlite, axum, plaintext credentials |
| `ocg-infra` | Key obfuscation, catalog-stripped proxy clients, inference HTTP helpers, one-statement log SQL | Product catalogs, `AppConfig`, Dashboard DTOs |
| `ocg-core` | SQLite, `CoreState`, Dashboard V3, provider adapters, `GatewayExecutor`, `forward_once`, usage sync, host composition | Plugin registries; adapters still must not own DB/`CoreState`/raw clients |

`ocg-core` keeps historical public paths as **explicit compatibility
facades** (`alias.rs`, `provider.rs`, `crypto.rs`, `http_client.rs`,
`kernel/{ids,catalog,protocol,zen}.rs`, `gateway/{attempt,classify,protocol,selector}.rs`).
Do not glob-reexport `ocg_domain` / `ocg_gateway` / `ocg_infra`. Production
graph guards in `kernel/mod.rs` require a DAG with **no multi-node SCC**.
`redaction.rs` is a crate-level leaf. `db` does not depend on `pricing` or
`gateway_keys`. `dashboard_v3` does not import `gateway` or `dashboard`.
`account_control`, `gateway_keys`, and `usage_sync` do not name `CoreState`.

`ocg-gateway` production dependencies are exactly `anyhow`, `base64`,
`ocg-domain`, `serde_json`. `ocg-domain` production dependencies are
exactly `chrono` (serde+std only, no clock feature), `serde`,
`serde_json`, `sha2`.

### ocg-core as composition / control plane

`ocg-core` wires the other crates. It is the only crate that opens SQLite,
holds `CoreStateInner`, mounts HTTP, and talks to upstreams.

- `host_router.rs` is the HTTP composition root: inference router +
  `/dashboard/api/v3` + the retired V2 REST tombstone + dashboard assets.
  `gateway` does not import dashboard mounts.
- `host_gateway.rs` implements `GatewayRebindHost` so `state` can rebind a
  listener without importing `gateway`.
- `gateway_runtime.rs` / `routing_runtime.rs` are DAG leaves that hold
  `GatewayHandle` and routing slots outside both `gateway` and `state`.
- `account_control.rs` is the HTTP-neutral account mutation service.
  Dashboard V3 wraps it with CAS; the CLI calls the same functions without
  an argv CAS token. Both bump `settings_revision` after a successful
  persist.
- `gateway_keys.rs` owns the `access_keys` table and the in-memory
  credential snapshot. Concrete `KeyStore` / `KeyHost` impls live in
  `state`.
- `control/observability.rs` is HTTP-neutral local read logic shared by
  leftover V2 adapters and V3. It never issues outbound HTTP.

### Gateway execution

Client inference lives under `crates/ocg-core/src/gateway/`. Axum + Tokio +
reqwest, default bind `127.0.0.1:9042`. Body cap is 16 MiB before auth.

Split of responsibility:

1. **`handler.rs`** — request id (`x-ocg-request-id`), credential auth,
   client parse/format validation, Claude Desktop rewrite, Alias
   resolution. Then it hands a parsed, resolved request to the executor.
2. **`GatewayExecutor`** — frozen request-entry snapshot, candidate
   selection, same-account retry, account fallback. One logical client
   request uses one immutable pricing revision, one `ForwardRouteSet`, one
   contract set, and one Alias resolution from start to finish. Each
   fallback iteration **re-reads** accounts, eligible Custom runtimes, and
   Zen Free cooldown.
3. **`provider_adapter.rs`** — exhaustive match on sealed
   `ProviderAdapterKind`. Returns a data-only `AttemptSpec` (URL, path,
   upstream protocol, auth scheme, redirect policy, opaque
   `CredentialHandle`, `ProxyRoutingModel`). Adapters take an account,
   config, and request plan. They do **not** decrypt keys, open databases,
   or build HTTP clients.
4. **`forwarder.rs` / `forward_once`** — exactly one upstream `.send()`
   per call. Owns transport selection and timeouts only. No policy, no
   retry, no fallback inside `forward_once`.
5. **Host `CredentialResolver`** — decrypts the handle after the outer
   loop has already selected the account.

Auth collects Bearer / `x-api-key` / `x-goog-api-key` candidates. Any hit
on `CoreStateInner.credential_snapshot` (primary + enabled sub keys)
passes; the first match in header order is the attribution. The snapshot
is also the forward-log name source. Client credentials are stripped
before upstream; only the selected account's configured scheme is
injected. Never pass Gemini or Anthropic client credentials through to an
upstream offering. Never alias Command Code / GOAT onto OpenCode or send a
GOAT key to an OpenCode endpoint.

Standard entries: `/v1/chat/completions`, `/v1/responses`, `/v1/messages`,
`/v1/models`. Claude Desktop: `/claude-desktop/v1/messages` and
`/claude-desktop/v1/models`. Gemini accepts `/v1beta/models/{model}:*` and
`/v1/models/{model}:*`; `generateContent` / `streamGenerateContent` enter
the conversion chain; `countTokens` / `embedContent` return `501`; unknown
actions return `404`. Authenticated `GET /v1/models` is a **local** reader
of currently routeable published aliases (OpenCode Go and the last
successful Zen Free snapshot) plus eligible Custom declared IDs — **zero
upstream discovery**. Protected
`GET /dashboard/api/v3/application-models` is a different local list: Go
routeable aliases ∩ the active Go pricing snapshot (highspeed variants
inherit the base row; empty intersection is `[]`). It never includes
Custom IDs. Claude Desktop `/claude-desktop/v1/models` still advertises
only the three role aliases.

The Alias registry lives in `ocg-gateway::alias` (facade `ocg_core::alias`).
Preferred aliases are stable lowercase kebab-case (existing OpenCode Go
IDs). Case-folded kebab spellings are accepted; names containing `/`, `_`,
or whitespace are raw IDs and must never fold onto a kebab alias. A raw ID
with exactly one registry mapping pins to that mapping; routability is
checked afterward, so an unroutable mapping is recognized but cannot
produce a production route. Overlapping raw IDs return `400` with
`ambiguous_model_id` and must not call upstream. Unknown names return
`400` on Chat Completions, Responses, Messages, and Gemini generate /
streamGenerate. Eligible Custom IDs overlay resolution and `/v1/models`
but must not steal published Go/Zen aliases. The published kebab
`deepseek-v4-flash` stays Go-owned; raw `deepseek/deepseek-v4-flash` pins
to unroutable GOAT. Forward logs persist `requested_model`,
`resolved_alias`, `upstream_model`, `provider_id`, and `offering_id`.
There is no `requested_alias` field.

JSON conversion lives in `ocg-gateway::protocol`; the host
`gateway/protocol.rs` keeps parse, usage, stream, and route-identity
types. Gemini is client-only and never an upstream protocol. Known models
use hardcoded `MODEL_PROTOCOLS` in `ocg-domain` (`preferred` +
`supported`): client protocol in `supported` passthroughs, otherwise
converts to `preferred`. Unknown models are `400` on every supported
client format — never trial protocols on the request path. Non-empty
`safetySettings` must be `400`; an empty array is acceptable. `topK` and
`thinkingConfig` are compatibility hints — never claim Gemini-equivalent
behavior.

`materialize.rs` parses the client protocol once, resolves the Alias, then
materializes model / protocol / endpoint / auth per candidate. Adapters
must not probe a billable inference path to discover protocol support. The
OpenCode `MODEL_PROTOCOLS` table stays Go-specific. Dynamic Zen `-free`
IDs unknown to the table default to Chat. Custom rematerializes per
account to that card's declared protocol, isolated origin, and auth
scheme.

`zen_models.rs` owns the only Zen Free model-discovery path. A protected,
explicit Providers-page refresh calls the fixed keyless
`https://opencode.ai/zen/v1/models` endpoint through the global proxy,
follows no redirects, keeps only valid IDs ending in `-free`, and
persists the complete successful snapshot before swapping runtime state.
Each model publishes both its raw ID and an Alias with the suffix
removed. Failed or empty refreshes preserve the previous snapshot;
`/v1/models` only reads it. Go-owned `ox-alpha-free` is a reserved
exclusion.

Selector policy: host `gateway/selector.rs` filters cards by capability,
enabled/ready state, credential validity, cooldown, and request-local
failures, then the secret-free `ocg-gateway::selector` state machine walks
that order (`StrictPriority` / `StickyGlobal` / `RoundRobin`). Do not
introduce a model-routing page or per-model quota pool. Zen Free quota is
shared per egress IP: any active `cooldown_free_until` exhausts the whole
free channel (no key rotation).

Pricing snapshots are immutable and provider-scoped. Refresh is manual
only. For OpenCode Go, an allowance derives the account quota-debit
multiplier (`monthly limit / Usage`) only; it is not a routable quota
pool. Official Go rows whose Input/Output/Usage cells are all dashes
(currently Ox Alpha Free / `ox-alpha-free`) are skipped as unpriced Go
promos. A refresh whose official multipliers differ from the active values
first returns a non-activating preview; a follow-up is bound to both the
active revision and the previewed official content hash. The fetcher is
restricted to the OpenCode Go HTTPS host, same-host redirects, a 20-second
deadline, and a 2 MiB body. MiniMax context / priority / high-speed
adjustments are local policy.

Fallback / retry (executor + classify, **not** `forward_once`):

- Only a pre-send DNS/TCP/TLS connection failure can retry **once** on the
  same account, and only before any downstream bytes.
- Partial SSE never falls back. Ambiguous stream results log
  `outcome_unknown`. `StreamOutcomeGuard` finalizes on drop.
- Inference `401` on OpenCode (Go/Zen) is returned as-is: no rotate, no
  `auth_error` (Go uses 401 for `ModelError` as well as invalid keys).
  Ordinary Custom `401` rotates and persists `auth_error`. Dashboard ping
  / key verification still record `auth_error` on 401.
- `403` and Go-channel `429` can select another account. Free-channel
  `429` cools the IP-shared free pool and does not rotate keys; routing
  continues with later compatible cards. Generic Custom/GOAT `429` does
  not parse Go windows.
- `408`, `5xx`, post-connect failures, body timeouts, and stream
  interruptions are never replayed.
- Shared reqwest client: 30-second connect timeout; non-stream 900-second
  total deadline; streams 300-second idle per chunk.

`ProxyRoutingModel` on `AttemptSpec`:

- `RequestEntrySnapshot` — frozen dual-leg `ForwardRouteSet` (Go / Zen).
  Follows redirects. Restricted URL (https or loopback http).
- `ProcessWideNoRedirect` — GOAT loopback tests only. Production GOAT
  fail-closes without a loopback guard.
- `IsolatedTrustedAdmin` — Custom: process-wide proxy, no redirects, no
  client-header forwarding, administrator-trusted URL.

Global outbound proxy is process-wide (`AppConfig`): Auto / Manual HTTP /
Direct / List. List mode uses `proxy_list_direction` plus
`proxy_list_models`. Listed models take the direction exception leg
(whitelist → proxy, blacklist → direct); unlisted models and non-model
outbound (verify, Zen refresh, usage, pricing, updater download) take the
direction default. Membership is validated only on dashboard
`update_settings` (non-empty, exact known id, de-duplicated); load
tolerates old values. Construction lives in `ocg-infra::http`;
`ocg-core::http_client` folds catalog aliases before exact match. A
request holds one `ForwardRouteSet` from entry; a concurrent settings
switch only affects later requests.

### Plan catalog

`BUILTIN_PLANS` and `ProviderAdapterKind` live in `ocg-domain::provider`
(facade `ocg_core::provider`). Five families:

| Family | IDs | Routable | Notes |
| --- | --- | --- | --- |
| OpenCode Go | `opencode` / `go` | yes | Official keys only |
| Zen Free | `opencode-zen-free` / `anonymous-free` | yes | Credentialless singleton, DB-owned |
| Command Code GOAT | `command-code` / `goat` | no | Disabled `pending` draft; verify `501` |
| SCNet Token Plans | `scnet` / `token-plan-basic\|standard\|premium` | no | `sk-tp-` prefix; verify `501` |
| Custom API | `custom` / `api` | yes | Trusted-admin destination |

Every persistent mutation path rejects `enabled=true` for a catalogued
`routable=false` offering before mutating the row, revision, or
timestamps. On every `Database::open`, leftover enabled GOAT and all three
SCNet tiers are disabled without changing `updated_at`. Custom enabled
state is preserved. Unverified GOAT is reset to `pending`. Go, Zen Free,
and unknown pairs are untouched.

Custom API (`custom.rs` + `custom_http.rs`): any syntactically valid HTTP
or HTTPS origin (LAN, loopback, self-selected) is accepted; URL-embedded
credentials, query, and fragment are rejected. Never follow redirects;
never forward dashboard/client auth; construct only the configured Bearer
or `x-api-key`. Joined endpoints must preserve scheme, host, port, and
base-path containment. Timeouts clamp `connect_timeout_secs` to 5–60
seconds. Create/update leave Custom disabled `pending`. Verification sends
one protocol-correct minimal non-stream request to the first declared
model; only a `2xx` JSON object succeeds; it does not discover or mutate
capabilities and never auto-enables. Explicit enable after verification is
required. Key, base URL, or capability changes re-pend verification and
disable the account; protocol and auth scheme are fixed at create. Custom
costs/usage are unpriced/unknown with no provider quota debit.

SCNet official usable-model snapshot `2026-08-21` (exact case and order,
adapter input only, never `model_aliases`): `GLM-5.2`, `GLM-5`,
`GLM-5.1`, `Kimi-K3`, `Kimi-K2.7-Code`, `Kimi-K2.6`, `Kimi-K2.5`,
`DeepSeek-V4-Flash`, `DeepSeek-V3.2`, `MiniMax-M3`, `MiniMax-M2.7`,
`MiniMax-M2.5`, `MiMo-V2.5-Pro`. Pricing-table / FAQ extras that are
**not** in that table: `DeepSeek-V4-Pro`, `DeepSeek-V4-Flash-0731`,
`Qwen3.8-max`, `Qwen3-235B-A22B`. Documented bases:
`https://api.scnet.cn/api/llm/v1` and
`https://api.scnet.cn/api/llm/anthropic`. Risk acknowledgement id
`scnet-token-plan-restrictions`, version `2026-08-21`. This crate must not
issue live Token Plan requests.

### Dashboard V3

Live dashboard JSON is `/dashboard/api/v3`, mounted beside the retired V2
REST tombstone. Wire DTOs are camelCase, `deny_unknown_fields` on mutation
bodies, and always serialize nullable response fields as `T | null`.

Control-plane identity:

- `settings_revision` — in-memory `AtomicU64` on `CoreState`, bumped after
  a successful persist. Not stored in SQLite as the CAS token.
- `process_generation` — assigned once per `CoreState`, never persisted.
  A CAS token from a previous process cannot be reused after restart.
- `pricingRevision` — immutable snapshot id. Pricing mutations also send
  `expectedPricingRevision`.

Mutations require top-level `expectedRevision` **and** `processGeneration`
(including `/auth/register`, `/auth/login`, `/auth/logout`, and
`POST /accounts/{id}/usage/refresh`). Missing `expectedRevision` is a
dedicated `400` `missingExpectedRevision`. Mismatch is `409`
`revisionConflict` with `currentRevision` / `processGeneration` in the
error envelope. The Vue `controlPlane` store records tokens from every V3
payload. The intended 409 recovery is to refresh tokens from `GET /contract`
without replaying the mutation, but the current client checks the obsolete
snake-case code `revision_conflict`; see Known Debt. Revision and generation
tokens are process-local and do not coordinate separate processes sharing a
data directory.

Not mutations (no CAS, no revision bump): operational diagnostics such as
`POST /settings/test-proxy` and `POST /custom/models/discover`.
`GET /settings/check-update` and `GET /settings/update-status` capture
revision/generation and never bump them. `POST /settings/install-update`
requires CAS, starts atomically, does not bump, and holds no network/DB
lock.

Secret boundary: plaintext keys must not appear on `Settings` or
provider/Zen/contract DTOs. `ConnectionInfo` (`GET /connection`) is the
**only** secret-bearing V3 response DTO (primary key + every non-deleted
sub-key value, including disabled sub-keys, under dashboard session
protection). Only enabled keys enter the authentication snapshot.
`CustomModelDiscoveryRequest.apiKey` is write-only. Account list/get
payloads stay secret-free. Logs and error envelopes redact known secrets.

The frozen contract is `schema/dashboard-api-v3.schema.json`, generated
from `dashboard_v3::contract_schema_pretty()` via
`crates/ocg-core/examples/export_dashboard_v3_schema.rs`. Generated
TypeScript (`src/api/generated/dashboard-v3.ts`) is types only — no HTTP
wrappers. `CATALOG_TYPE_NAMES` in `dashboard_v3/types.rs` is the ordered
`$defs` catalog; existing definition objects must stay byte-identical when
you append.

Frontend: Pinia stores call `dashboardV3` directly. Existing pages that
still speak older field names go through `src/api/dashboard.ts`
presenters. Do not add a V2 import, route fallback, or recursive case
conversion.

`dashboard.rs` still contains the historical V2 REST handlers and serves
dashboard HTML/assets. Those protected REST handlers are **not** the live
API: `host_router` intercepts retired `/dashboard/api/...` paths first.

### Retired V2 REST

Protected Dashboard V2 REST is retired.

- Anonymous retired REST: empty-body **401** (auth runs before the
  tombstone).
- Authenticated retired REST (including loopback local mode): **410** with
  `{ "code": "dashboardV2Removed", "message": "Dashboard API V2 has been removed; refresh the page and retry." }`.
- Unknown `/dashboard/api/...` paths that are not V3 and not a preserved
  family are also 410 once authenticated.

Preserved `/dashboard/api` families (exact path, no trailing slash, no
extra segments):

- `auth/status`, `auth/register`, `auth/login`, `auth/logout`
- `browser/sessions/{token}/ws` (non-empty token)

V3 has its own auth and browser WebSocket under `/dashboard/api/v3/...`.
The current Vue shell uses V3. Inference routes, dashboard HTML, and
`/dashboard/assets/...` are outside the tombstone.

### State, credentials, and settings

`CoreStateInner` (`state.rs`) is shared by gateway, dashboard, and CLI.

Lock order: (1) `settings_update`, (2) `db`, (3) `config`, (4)
`http_client`, (5) `gateway`, (6) `pricing`, (7) `zen_free_models`,
(8) `provider_contracts`, (9) `routing`, (10) `credential_snapshot`.
Never acquire in reverse. Do not hold the routing lock across DB or
network I/O. Async gates: `settings_host_effects` (persist → listener
rebind → compensation) is acquired before `gateway_lifecycle` when a
settings write also rebinds. Never hold a `parking_lot` lock across those
awaits.

Two credential tiers share one `access_keys` table (schema v27) and one
auth snapshot:

- Primary key: fixed id `00000000-0000-0000-0000-000000000001`, display
  name `"Primary"`. Always enabled, never deleted. Public `AppConfig` and
  dashboard APIs still expose `gateway_key`; sanitized config JSON is
  **not** the database authority after v27.
- Sub keys: non-primary rows, active ceiling 64, soft-delete keeps
  identity/name and clears the value. Lifecycle only through
  `/dashboard/api/v3/keys*`. CLI has no sub-key commands.

Primary/sub values are mutually exclusive
(`gateway_keys::ensure_primary_value_allowed`) on dashboard, settings, and
sub-key enable paths.

`AppConfig` uses serde defaults for backward-compatible loading. A pre-1.3
config without `claude_desktop_models` receives default Sonnet
`minimax-m3` and is rewritten. Ordinary settings saves preserve the
dedicated Claude Desktop mapping. Downstream client root URL priority:
non-empty `OCG_CLIENT_ROOT_URL` (read-only, never written back) > SQLite
manual value > frontend derivation from production origin / dev Gateway
port.

Dashboard authentication is skipped for **direct** requests when the
gateway binds loopback. Requests carrying standard reverse-proxy
forwarding headers still require login. Non-loopback binds use a single
administrator (Argon2 hash in SQLite, HttpOnly session cookie). Docker may
bootstrap the first administrator with **both** `OCG_ADMIN_USERNAME` and
`OCG_ADMIN_PASSWORD`; setting only one fails startup; otherwise the first
registration wins.

Settings uses `GET /dashboard/api/v3/settings/check-update` for GitHub
Release metadata. Updater-enabled installed desktop runtimes can continue
through a signed download and install; development builds, CLI, and Docker
retain the metadata/release-link path. The outbound request runs only when
the user clicks the button.

### Account lifecycle and browser runtime

Schema v16 added `account_type` (`key | managed`) and `setup_step`
(`google_account → opencode_registration → payment → key_verification → ready`).
Existing rows migrate to `key + ready`. A managed draft is persisted
immediately with an empty key and `enabled=false`; selector, enable, and
the request path all require both `ready` and a non-empty key.
`google_account` is labeled **sign-in identity** in the UI and is
skippable.

`AppConfig::default()` seeds `opencode_invite_url` with
`DEFAULT_OPENCODE_INVITE_URL` (demo). Normalized values must be a
credential-free HTTPS URL up to 2,048 characters whose host is exactly
`opencode.ai` or `console.opencode.ai`. Creating a managed draft can edit
the invite URL and write it back to Settings when it differs. Signup,
registration, and payment remain manual in the isolated browser; the user
copies the key back. Never add CDP autofill or automated payment clicks.

Managed setup may move **forward exactly one step** or **rewind to any
earlier unfinished step**. Skipping forward is rejected; the setup API
must not enter `ready` directly. A real key probe returning `2xx`
transitions to `ready + enabled`; `429` also proves validity and records
cooldown. `401`/`403`, network errors, and `5xx` remain at
`key_verification`.

Official Go usage (`go_usage.rs`, `https://opencode.ai/zen/go/v1/usage`) is
a calibration baseline coordinated by `usage_sync.rs`. Manual
`POST /dashboard/api/v3/accounts/{id}/usage/refresh` and the background
reconciler share one fetch + key-CAS + three-window calibration path.
Ready+enabled accounts with local activity in the last 24h reconcile about
hourly; inactive ones about daily. Disabled / non-ready / empty-key
accounts are excluded. Startup must not stampede: global concurrency 1,
pacing, bounded jitter, injectable clock/jitter/fetch seams. Manual
refresh has a 15s per-account throttle after any attempt, in-flight
dedupe, and Retry-After / `nextAllowedAt`. Local max Go usage ≥80% may
expedite at most once per 15 minutes. Real inference `429` keeps existing
cooldown/selector writes and additionally schedules an official sync ~1–2
minutes later (never inline). Official failures or `status=rate-limited`
must never write inference cooldown. After success, schedule around the
earliest `resetsAt` (bounded jitter) while respecting active/inactive
cadence. Failure backoff: 5m → 15m → 1h → 6h; never erase last success or
the previous baseline. Sync metadata lives in `provider_usage_sync_state`
(the five leftover `accounts.usage_sync_*` columns are dropped in v27).
The public Go docs have not listed this path yet.

`console_usage.rs` is **frozen** deprecated compatibility code — do not
call or extend. Remove only after ≥2 minor releases plus stable
real-account evidence. Manual slider/PATCH calibration stays available.

Zen Free is database-owned: it can be enabled, disabled, and reordered,
but cannot be created or deleted through generic account APIs. GOAT /
SCNet drafts stay disabled and unroutable. Custom is catalog-routable
after verify-then-enable.

Browser: `GET /dashboard/api/v3/browser/capabilities`,
`POST /accounts/{id}/browser`, `DELETE /accounts/{id}/browser-profile`,
and `/browser/sessions/{token}/ws`. Targets include Google signup/login,
GitHub signup/login, the configured invite, and the OpenCode console
(`https://opencode.ai/auth`). The worker host allowlist includes
`accounts.google.com`, `github.com`, `opencode.ai`,
`console.opencode.ai`, and `auth.opencode.ai`. Remote tokens are
memory-only, administrator-session-bound, and Origin-checked; they expire
after 30 minutes idle or four hours total.

Desktop native browser hooks are registered by `src-tauri/src/host/` into
`CoreState`. Vue still calls HTTP. Windows discovers Edge then Chrome;
macOS checks Chrome, Edge, and Chromium; Linux searches `PATH` for
Chrome/Chromium/Edge. The external browser uses
`browser-profiles/<account_id>`, `--no-first-run`,
`--no-default-browser-check`, and a new window. Never add CDP,
automation, `--no-sandbox`, or disabled web security.

`crates/ocg-browser-worker` keeps one Chromium per node. An account switch
sends SIGTERM to the current process group and waits for profile flush,
forcing termination only after the bounded timeout. The sidecar runs as
UID/GID 10001 with a read-only root and no capabilities; a shared runtime
volume holds a random control token. Chromium must create its own
user/PID/network namespaces and renderer seccomp sandbox, so the browser
service uses `seccomp=unconfined` and cannot use `no-new-privileges`. It
still does not mount SQLite or publish a host port. The project-scoped
browser bridge is not Docker `internal`, because Chromium needs outbound
HTTPS to Google/OpenCode.

Profile deletion must stop the browser, validate account IDs against path
traversal, and atomically rename both new and legacy profiles into
staging. Purge only after the database operation commits; restore staging
on failure. Reset keeps a completed account's key, while a pending managed
account also returns to `google_account`. Delete confirmations must state
that cookies/profile are removed.

### Persistence

`crates/ocg-core/src/db.rs` defines the SQLite schema, migrations, and
queries. Current schema is **v27**. `provider_contracts.rs` owns provider
contract scopes and model-protocol evidence. `models.rs` defines shared
serde types and `AppConfig`. Key obfuscation is `ocg-infra::crypto`
(facade `ocg_core::crypto`): this is lightweight obfuscation, not a KMS.
Windows desktop uses `MachineBoundCipher`; CLI/Docker use
`StaticKeyCipher` from `OCG_MANAGER_ENCRYPTION_KEY` or
`<data-dir>/.encryption-key`. Production hosts must call
`Database::open_with_cipher` so v27 ciphertext probes use the already
resolved cipher. Account `key_cipher` / `password_cipher` are validated in
place and **never re-encrypted**. A schema newer than this build supports
fails closed.

Historical versions still matter on upgrade:

- v16: managed setup columns.
- v21: usage-sync metadata (later moved off `accounts` in v27).
- v22: immutable provider/offering bindings, provider pricing/usage,
  quota windows, provider-aware forward logs. First migration of a
  pre-v22 file writes `data.sqlite.pre-v22.<UTC>.bak`.
- v23: Plan verification, Alias / upstream log identity, optional native
  cost, Custom config tables, SCNet acknowledgements. First migration of a
  pre-v23 file writes `data.sqlite.pre-v23.<UTC>.bak` before any v23 write.
- v24: actual proxy route leg on forward logs (`auto` / `proxy` /
  `direct`; historical empty string = unrecorded).
- v25: `provider_model_catalogs` (last successful Zen Free snapshot).
- v26: `provider_contract_scopes` and `provider_contract_model_protocols`.
  Additive; no separate pre-v24/v25/v26 backups.
- **v27:** copy primary `gateway_key` + `sub_gateway_keys` into
  `access_keys`; drop `sub_gateway_keys`; drop leftover
  `accounts.usage_sync_*`. After the database is at canonical v26, an
  existing (non-empty) library gets a unique sibling
  `data.sqlite.pre-v3.<UTC>.bak` plus a SHA-256 sidecar **before any v27
  write**. A brand-new empty directory creates v27 directly and does not
  write that copy. Operator recovery:
  [MAINTAINER-v3-migration.md](MAINTAINER-v3-migration.md).

GUI data directory: Windows `%USERPROFILE%\.ocg-mgr` or macOS/Linux
`~/.ocg-mgr`. CLI default: `~/.ocg-mgr-cli`. Docker stores SQLite, keys,
and `.encryption-key` in `ocg-data`; long-lived cookies and browser state
live in `ocg-browser-profiles`. Stop and back up those two sensitive
volumes together. `ocg-browser-runtime` contains only the runtime control
token and should not be backed up. OCG Manager does not encrypt browser
profiles.

Forward-log inserts go through `ocg-infra::sqlite_logs` (one explicit
statement per helper). Callers own timestamps, diagnostics, cost policy,
redaction, and transactions.

### Per-node boundaries

Each node owns its account data and is managed through its own dashboard.
There is no cross-node sync and no Admin API. Do not add one.

## Lifecycle Classes

Keep these four classes separate. Do not cancel one from another.

| Class | Start | Stop | Notes |
| --- | --- | --- | --- |
| **Gateway listener** (`GatewayLifecycle`) | `start_gateway` / `bind` | `stop` (signal-only) or `stop_and_wait` (CLI) | TCP bind, dashboard trust, forward-log backfill, HTTP server. Rebind is slot-aware (same-port stop-then-bind, new-port bind-first). Does not start or cancel process-level workers. |
| **Control-plane workers** (`ControlPlaneWorkers`) | `ensure_started` from `start_gateway` (once per `CoreState`) | none — exits when the owning `CoreState` is dropped | Official usage reconciler. No public cancel API. Listener stop must not kill it. |
| **Desktop capabilities** | Tauri setup: auto-start (Windows release/installed only), Dock (macOS), updater starter | process exit | Not WebView commands. CLI/Docker leave hooks unset. `auto_start` and `show_dock_icon` stay capability-gated on the HTTP settings form. |
| **Browser runtime** | Native hooks on desktop; remote worker in Docker | account switch / profile reset / process exit | Native Browser vs sidecar are different hosts of the same `BrowserRuntime` slot. |

Tauri `src/lib.rs`: start uses `start_gateway` (listener + usage workers);
exit uses `host::gateway::stop_listener` (listener only). Settings port
changes rebind through `GatewayLifecycle` / `settings_host_effects` with
config-fingerprint compensation; concurrent failed port writes must not
clobber a successful timeout write.

Updater is configured as a `CoreState` starter, never a WebView `invoke`
command. `src-tauri/capabilities/default.json` has no updater permission.
Updater outbound follows the process-wide **default-leg** proxy policy
(List mode included).

## HTTP Routes

### Inference (unchanged paths)

| Method | Path | Notes |
| --- | --- | --- |
| POST | `/v1/chat/completions` | OpenAI Chat |
| POST | `/v1/responses` | OpenAI Responses (stateless; `store` / `previous_response_id` / `conversation` / `background` → 400) |
| POST | `/v1/messages` | Anthropic Messages |
| GET | `/v1/models` | Local list; auth required |
| POST | `/claude-desktop/v1/messages` | Role alias rewrite then Messages |
| GET | `/claude-desktop/v1/models` | Three role aliases |
| POST | `/v1beta/models/{model}:*` and `/v1/models/{model}:*` | Gemini client format |

### Dashboard V3 (`/dashboard/api/v3`)

Public: `/auth/status`, `/auth/register`, `/auth/login`, `/auth/logout`.

Session-protected (non-exhaustive; see `dashboard_v3/mod.rs`):
`/contract`, `/connection`, `/settings`, `/settings/test-proxy`,
`/claude-desktop/models`, `/settings/check-update`,
`/settings/update-status`, `/settings/install-update`, `/pricing`,
`/pricing/refresh`, `/pricing/multipliers`,
`/providers/{provider_id}/{offering_id}/pricing`, `/keys`,
`/keys/primary/regenerate`, `/keys/{id}`, `/keys/{id}/regenerate`,
`/accounts`, `/accounts/managed`, `/accounts/order`, `/accounts/{id}`,
`/accounts/{id}/toggle`, `/accounts/{id}/browser`,
`/accounts/{id}/browser-profile`, `/accounts/{id}/setup`,
`/accounts/{id}/setup/verify-key`, `/accounts/{id}/reset-cooldown`,
`/accounts/{id}/custom-config`, `/accounts/{id}/model-capabilities`,
`/accounts/{id}/acknowledgements`, `/accounts/{id}/usage`,
`/accounts/{id}/usage/refresh`, `/accounts/{id}/provider-usage`,
`/accounts/{id}/verify`, `/providers`, `/providers/model-capabilities`,
`/providers/zen-free`, `/providers/zen-free/models`,
`/providers/zen-free/models/refresh`, `/provider-contracts`,
`/provider-contracts/provider/{scope_id}/protocols/{protocol}`,
`/providers/{provider_id}/protocol-probes`, `/browser/capabilities`,
`/browser/sessions/{token}/ws`, `/gateway/status`,
`/application-models`, `/dashboard/summary`,
`/dashboard/daily-cost-by-model`, `/logs/gateway`, `/logs/forward`,
`/logs/forward/models`, `/logs/forward/keys`,
`/custom/models/discover`.

Go/Zen protocol probes are `POST /providers/{provider_id}/protocol-probes`.
Custom is rejected there (`protocol probes for Custom API are account-owned`).
The historical V2 `POST /accounts/{id}/protocol-probes` is 410. Custom
connection verify is `POST /accounts/{id}/verify`; model discovery is the
operational `POST /custom/models/discover`.

### Static dashboard

`GET /dashboard`, `GET /dashboard/`, `GET /dashboard/assets/{*path}`.

## Storage And Migration

See [Persistence](#persistence) and
[MAINTAINER-v3-migration.md](MAINTAINER-v3-migration.md). Summary for
operators:

1. Stop every process that has the data directory open. WAL files belong
   with `data.sqlite`.
2. Keep the matching encryption key (Windows machine-bound material, or
   `OCG_MANAGER_ENCRYPTION_KEY` / `.encryption-key`). A different cipher
   fails closed and must not be "fixed" by rewriting ciphertext.
3. Back up the whole data directory **before** opening a newer binary,
   including `.encryption-key` and `browser-profiles/` when present.
4. v27 writes `data.sqlite.pre-v3.<UTC>.bak` + `.sha256` only for an
   existing v26 library. Verify the sidecar before restoring.
5. Restore the pre-v3 file only onto a v26-capable binary, or retry v27
   from that v26 snapshot. Never point a v26 binary at schema 27.
6. A failed v27 transaction rolls back; the source must remain v26. Leave
   any pre-v3 files in place — a later successful open creates another
   unique name rather than overwriting.

Downgrades are not guaranteed. To roll back, restore the matching older
backup; do not open a migrated database with an older binary.

## Extension Procedure

### Add or change a provider (sealed)

1. Add identities and catalog facts in `ocg-domain` (`ids.rs`,
   `provider.rs`). Extend `ProviderAdapterKind` exhaustively (`ALL`,
   `from_offering`, capability composition). Keep Custom as
   `ConfigurableHttp`, not a superclass.
2. If the family needs protocol rows, add them in `ocg-domain::protocol`.
   Do not trial protocols on the request path.
3. If the family needs aliases, add mappings in `ocg-gateway::alias`.
   Unroutable mappings may be recognized without producing a production
   route.
4. In `ocg-core`, implement `resolve_route` for the new kind so it returns
   an `AttemptSpec` only. **Adapters cannot own DB, `CoreState`, or a raw
   reqwest client.** Decrypt and HTTP stay in the Host resolver /
   `forward_once`.
5. Fail closed until routing, verify, usage, and pricing are actually
   implemented. GOAT/SCNet are the template for "catalog present, not
   live".
6. Run `cargo test -p ocg-domain`, `cargo test -p ocg-gateway`, and
   `cargo test -p ocg-core`. Purity/dependency guards will fail a
   forbidden import.

Do not add a plugin loader, dynamic library, or user-supplied adapter
script.

### Add or change a Dashboard V3 endpoint

1. Add or extend DTOs in `dashboard_v3/types.rs` and append new names to
   `CATALOG_TYPE_NAMES`. Do not change existing `$defs` objects.
2. Mount the route in `dashboard_v3/mod.rs`. Mutations go through
   `parse_mutation_json` + `check_expectation`. Keep the secret boundary.
3. Prefer `account_control` / `gateway_keys` / `control::observability`
   over duplicating persist logic. Do not import `gateway` from
   `dashboard_v3`.
4. Add an integration test under `crates/ocg-core/tests/dashboard_v3_*.rs`.
5. Run `pnpm run contract:v3:generate` (or `--check` in CI) and update the
   handwritten client in `src/api/dashboard-v3.ts`. Presenters in
   `dashboard.ts` / `dashboard-presenters.ts` only if an existing page
   needs the older shape.
6. Do not revive `/dashboard/api` REST. New protected JSON belongs on V3.

### Add a host capability

Desktop capabilities live in `src-tauri/src/host/`, registered into
`CoreState`. Do not reintroduce `#[tauri::command]`. Vue must keep calling
HTTP.

## Failure Modes

| Symptom | Likely cause | What to do |
| --- | --- | --- |
| Dashboard JSON `410` `dashboardV2Removed` | Client still calling `/dashboard/api/...` REST | Refresh/upgrade the page; use `/dashboard/api/v3` |
| Dashboard JSON `409` `revisionConflict` | Stale `expectedRevision` / `processGeneration` / `expectedPricingRevision` | Reload the resource; do not auto-replay the mutation |
| Dashboard JSON `400` `missingExpectedRevision` | Mutation body omitted CAS | Send `expectedRevision` + `processGeneration` |
| Empty-body `401` on `/dashboard/api/...` | Anonymous retired REST or missing session | Log in; loopback skips only **direct** requests |
| Gateway `400` `ambiguous_model_id` | Raw ID maps to more than one family (including Custom overlap) | Rename/avoid the colliding Custom ID; do not call upstream |
| Gateway `400` unknown model | Name is neither a published alias nor an eligible Custom ID | Use `/v1/models`; do not probe protocols |
| Inference `401` unchanged, no failover | OpenCode Go/Zen `ModelError` or invalid key | Expected; ping/verify still record `auth_error` |
| Zen `429` cools every Free card | Egress-IP shared pool | Wait for `cooldown_free_until`; later non-Free cards may still run |
| `success_no_usage` | Upstream omitted usage chunks | Chat streams request `include_usage`; without a chunk the row stays missing usage |
| Open fails: schema newer than 27 | Data directory from a newer binary | Restore a matching backup; do not run an older binary on v27 |
| Open fails: cipher / ciphertext | Wrong `.encryption-key` or machine-bound context | Restore the matching key; never rewrite ciphertext |
| Interrupted v27 open | Transaction rolled back; pre-v3 backup may already exist | See [MAINTAINER-v3-migration.md](MAINTAINER-v3-migration.md) |
| Settings port change bound the old port | Rebind failed; compensation restored config | Check gateway logs; concurrent writes are serialized by `settings_host_effects` |
| Usage loop still running after `stop_gateway` | Listener stop does not cancel `ControlPlaneWorkers` | Drop `CoreState` (process exit) |

## Upgrades And Database Migrations

SQLite migrations run in place when the GUI or CLI starts. Back up the
complete data directory before upgrading, including the database,
`.encryption-key` when present, and `browser-profiles/`; for Docker, back
up both `ocg-data` and `ocg-browser-profiles`. Stop the process first for
a direct/manual upgrade. The signed desktop updater manages its own stop
and restart. Downgrades are not guaranteed; restore the data backup made
by the matching older version instead of opening a migrated database with
an older binary.

Schema v23 writes `data.sqlite.pre-v23.<timestamp>.bak` before any v23
write. Schema v27 writes `data.sqlite.pre-v3.<UTC>.bak` plus a SHA-256
sidecar after canonical v26 and before any v27 write (existing libraries
only). Keep those files with the normal backup until the upgraded
installation is verified. They are rollback points, not a general backup
or a license to run an older binary on the migrated database.

Version 1.4.1 has neither the updater runtime nor its embedded
verification key. For the one-time Windows transition, instruct users to
quit the tray app, run the first updater-enabled setup, and choose the
second upgrade-method option, **Install without uninstalling**
(不要卸载，直接安装). Tauri merely selects the first option by default;
that option is not required. Users must not uninstall 1.4.1 first. The
optional equivalent for advanced users is:

```powershell
Start-Process -FilePath .\ocg-manager_<version>_windows-x64-setup.exe -ArgumentList '/UPDATE','/P','/R' -Wait
```

macOS/Linux use their normal direct replacement once. Later desktop
releases can use the signed Settings update path. CLI and Docker upgrades
remain manual.

## Release Artifacts

The supported matrix is intentionally small:

| Runner | GUI | CLI |
| --- | --- | --- |
| Windows 10/11 x64 | NSIS current-user setup | x64 ZIP |
| macOS 11+ | Universal DMG (x64 + ARM64) | Universal tar.gz |
| Linux x64 | AppImage + deb | x64 tar.gz |

Stable delivery names are:

```text
ocg-manager_<version>_windows-x64-setup.exe
ocg-manager_<version>_windows-x64-setup.exe.sig
ocg-manager-cli_<version>_windows-x64.zip
ocg-manager_<version>_macos-universal.dmg
ocg-manager_<version>_macos-universal.app.tar.gz
ocg-manager_<version>_macos-universal.app.tar.gz.sig
ocg-manager-cli_<version>_macos-universal.tar.gz
ocg-manager_<version>_linux-x64.AppImage
ocg-manager_<version>_linux-x64.AppImage.sig
ocg-manager_<version>_linux-x64.deb
ocg-manager_<version>_linux-x64.deb.sig
ocg-manager-cli_<version>_linux-x64.tar.gz
compose.example.yaml
latest.json
SHA256SUMS
```

Each CLI archive contains its executable, `dist/`, and `LICENSE`. Do not
ship the CLI executable alone: `serve` needs the sibling dashboard assets.
Windows has no portable GUI artifact.

The `linux/amd64` and `linux/arm64` containers are published separately as
`ghcr.io/klarkxy/opencode-go-mgr`; the GitHub Release contains the seven
ordinary platform payloads, the extra macOS updater archive, four updater
signatures, the pull-only Compose example, `latest.json`, and `SHA256SUMS`
(currently 15 attachments). The local verifier pins that current 15-file
contract, while the workflow also requires the GitHub asset names and count
to match the assembled `release/` set exactly. The runtime image includes
`LICENSE` at `/usr/share/licenses/ocg-manager/LICENSE`.

### scripts/release.mjs

`scripts/release.mjs` does the heavy lifting:

1. Validates that `package.json`, `src-tauri/tauri.conf.json`, the workspace
   `Cargo.toml`, `src-tauri/Cargo.toml`, and all three versioned fields in
   `compose.example.yaml` all agree. It also checks the Git tag, if any,
   against that version.
2. Resolves the updater signing mode before creating the staging tree. With
   `OCG_REQUIRE_UPDATER_ARTIFACTS=1`, either a missing private key or missing
   `TAURI_UPDATER_PUBLIC_KEY` fails before `release/` can be replaced. A
   configured public key must also match the committed SHA-256 continuity
   baseline in `src-tauri/updater-public-key.sha256`.
3. When a signing key is configured, merges `src-tauri/tauri.updater.conf.json`
   plus an ephemeral public-key config and enables Tauri updater artifacts.
   `TAURI_SIGNING_PRIVATE_KEY` accepts either the private-key content or its
   secure path outside the repository; there is no separate path variable.
   With no signing key, the script preserves the ordinary local build and
   prints that the result is for smoke testing, not an updater-enabled
   published release.
4. Rejects unsupported host/architecture pairs
   (`process.platform`/`process.arch`).
5. Invokes `@tauri-apps/cli` with the exact bundle path for the platform
   (`nsis` on Windows and `appimage,deb` on Linux). macOS uses `dmg` with
   `--target universal-apple-darwin` for unsigned local builds and `app,dmg`
   when updater signing is enabled, because Tauri only emits the updater
   archive for the `app` target.
6. Cryptographically verifies every payload/signature pair against the actual
   `TAURI_UPDATER_PUBLIC_KEY` before staging it, then collects the NSIS and
   AppImage signatures plus the macOS `.app.tar.gz`/signature. It explicitly
   signs the deb with `tauri signer sign` because deb is not a native Tauri
   updater artifact. A nonempty but mismatched key therefore fails closed.
7. Builds the CLI binary, packages it with `dist/` and `LICENSE` into the
   per-platform archive, and on macOS uses `lipo` + `codesign -` to create
   the universal CLI.
8. Writes `SHA256SUMS` over every payload and signature in the staged
   `release/` directory.
9. Atomically replaces `release/`. On any error, the previous `release/` is
   preserved and the staged tree is removed.

`scripts/release.mjs` does **not** erase Cargo's incremental build caches —
repeated release builds reuse the same `target/` tree.

`pnpm run release:check` validates versions, Compose, and any configured
signing key without building a native bundle. The keyless preflight
exercises the unsigned contract. For a production tag push, each runner
signs a temporary payload with the repository signing secret and verifies
it against the continuity-checked `TAURI_UPDATER_PUBLIC_KEY` before
starting the expensive native build.

## CI Workflow

### quality.yml — the reusable quality gate

`.github/workflows/quality.yml` runs on pull requests and pushes to `main`,
and `release.yml` calls it once for a release. The gate is three parallel
jobs so frontend failures surface without waiting for Rust, and Windows
does not rebuild the dashboard:

- **Web** — `pnpm run contract:v3:check`, Node tests (`scripts/*.test.mjs`
  and `src/**/*.test.ts`), TypeScript checking, a Vite production bundle,
  `DESIGN.md` lint, and Compose validation.
- **Rust** — `cargo fmt`, locked workspace tests, and Clippy against a
  stub `dist/index.html` so tauri-build can compile the Linux desktop
  crate. WebKit headers are installed only on this job.
- **Windows Tauri** — `cargo test -p ocg-manager --lib` / `clippy` against a
  stub `dist/index.html`, covering Windows-only auto-start without pnpm
  or Vite.

Node/pnpm and Rust build caches are shared across compatible runs; pull
requests restore but do not write the Rust cache. Failed non-PR runs still
write the Rust cache so a follow-up fix can reuse the compile.

### release.yml — candidates and tag releases

`.github/workflows/release.yml` runs on `workflow_dispatch` and on `v*` tags.

- A manual candidate can select Windows x64, macOS Universal, Linux x64, or
  all three platforms and intentionally produces unsigned smoke artifacts,
  even when a manual dispatch selects a tag as its ref.
- Only a `push` event for a `v*` tag forces the complete three-platform
  matrix and supplies the repository signing secrets. For this
  single-maintainer repository, pushing that tag is the explicit publication
  authorization.
- The quality job runs in parallel with a keyless Ubuntu preflight that
  parses the extracted installer smoke under `pwsh`, runs the
  release-helper tests, and validates all version manifests.

After preflight, each selected native runner restores its platform Rust cache
and installs dependencies. The workflow injects signing secrets only when its
plan proves the event is an actual `v*` tag push, then proves the signing pair
and committed public-key fingerprint before running the signed build. Manual
jobs receive empty signing values and run the ordinary unsigned build. Both
paths execute CLI/GUI smokes and upload `release-<platform>` with seven-day
retention. The expensive generic test/type/lint suite is not repeated on all
three native runners.

### Per-runner smoke flows

- **Windows CLI** — verifies `SHA256SUMS`, expands the ZIP, runs
  `key add` / `key list` / `key disable` / `key enable` / `status` /
  `key remove` against a temp data dir, then starts `serve --port=19042` and
  waits for `id="app"` to appear in the dashboard HTML.
- **macOS / Linux CLI** — the same `key` and `serve` flow plus a
  `lipo -archs` check that the macOS CLI is a universal binary.
- **Windows GUI** — downloads the current published installer, silently
  installs and launches it, writes a data sentinel, and enables `auto_start`.
  It then runs the candidate NSIS package through `/UPDATE /P /R /ARGS
  --startup` without uninstalling, verifies the old PID exits, the candidate
  version returns through `/settings/update-status`, and both the sentinel
  and `HKCU\Software\Microsoft\Windows\CurrentVersion\Run\OCG Manager`
  survive. Installer processes have an explicit timeout and are waited
  independently from the `/R`-launched GUI process so a successful restart
  cannot hang CI; uninstall completion is bounded and checked through removal
  postconditions. It then runs the existing off/on cleanup checks, silently
  uninstalls, and confirms user data remains. The PowerShell implementation
  lives in `scripts/smoke-windows-release.ps1` instead of an inline YAML
  block. A manual dispatch whose candidate is already the latest release may
  use the candidate-only install path.
- **macOS GUI** — mount the DMG, `codesign --verify --deep --strict`, check
  the binary is universal with `lipo -archs`, launch with `--startup`, wait
  for the dashboard.
- **Linux GUI** — `dpkg-deb --info` / `dpkg-deb --contents` on the deb,
  `file` on the AppImage, then launch under `dbus-run-session -- xvfb-run -a
  env APPIMAGE_EXTRACT_AND_RUN=1 WEBKIT_DISABLE_COMPOSITING_MODE=1` and wait
  for the dashboard.

`scripts/smoke-windows-release.ps1` currently probes the legacy V2 URLs
`http://127.0.0.1:9042/dashboard/api/settings/update-status` and
`/dashboard/api/settings`. On this architecture those authenticated paths
return 410; a V3 candidate must be smoked against
`/dashboard/api/v3/settings/update-status` (and the matching V3 settings
read). The script must be updated to V3 before it can be relied on for release
smoke validation.

### draft-release and verify-release

When a `v*` tag is pushed, the downstream `draft-release` job downloads the
three per-runner Actions artifacts, assembles their payloads/signatures and
`compose.example.yaml` in `release/`, generates `latest.json` with immutable
tag URLs and bundle-aware platform keys, regenerates `SHA256SUMS` over the
manifest, signatures, and every other attachment, and creates or updates a
**draft** GitHub Release. `verify-release` then requires the GitHub asset
names to match the assembled `release/` set exactly. The local verifier also
pins the current 15-file contract, then re-derives `latest.json`, recomputes
every checksum, verifies all four updater signatures, and compares every
downloaded artifact with the digest reported by GitHub Release storage. The
draft job passes its numeric Release ID downstream; verification and
publication re-check that exact ID, tag, and draft state instead of using
the tag lookup endpoint, which does not expose draft Releases.

SemVer prerelease tags such as `v1.5.8-beta.1` use this same real signed tag
path and the same exact assembled immutable attachments. Their updater
manifest keeps the full prerelease identifier in payload names and download
URLs, and the Windows packaged smoke accepts that same prerelease
`CandidateVersion`. Generated notes begin with a prominent Beta warning for
managed account registration and isolated browser profiles. The warning
names the still unverified real Google/OpenCode signup/payment, noVNC
keyboard/clipboard, and live GHCR first-publication paths, states that
gateway/redaction/release changes are also included, and says the preview is
not production-ready. Automatic notes for a later stable tag skip
same-version prerelease tags as their baseline, preserving the complete
feature scope since the prior stable release.

### publish-release — publish only the verified tag build

The `v*` tag push is the single maintainer's explicit release authorization.
`publish-release` therefore runs automatically after `verify-release`
succeeds. It compares the current asset/digest-set fingerprint with the
verified fingerprint and refuses any draft that changed after verification.
Manual candidates cannot reach the draft, verification, or publication jobs.
A missing signing key, failed smoke, or failed verification leaves the
Release unpublished.

The publication job is serialized in the repository-wide
`release-moving-channels` queue. Immediately before publishing it compares
the candidate with the current GitHub latest release and advances `latest`
only for a strictly newer stable SemVer. A delayed older run can therefore
publish its immutable release without rolling the moving latest channel back.
For a prerelease tag, the workflow marks both draft and public Release as
`prerelease=true`, forces `make_latest=false`, and never calls the
stable-only latest-channel comparison. Stable tag behavior is unchanged.

### Updater signing key

Generate the production updater key once on a trusted workstation, writing it
to a secure path outside the checkout (do not run this with a repository
path):

```powershell
node node_modules/@tauri-apps/cli/tauri.js signer generate -w <secure-path-outside-repository>/ocg-updater.key
```

- Store the private-key content and password as repository Actions secrets
  named `TAURI_SIGNING_PRIVATE_KEY` and
  `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`. The release workflow references them
  only when the event-derived plan identifies an actual `v*` tag push; manual
  candidates receive empty values and remain unsigned.
- Repository secrets are not isolated by an Environment. If another
  write-capable maintainer is added, reassess a protected signing Environment
  or tag ruleset before the next release.
- Keep at least two independently stored encrypted backups of both the
  private key and its password. If they are lost, already-installed clients
  that trust the matching public key cannot receive another in-app update and
  will need a new direct-install bootstrap.
- The public key is safe to share; this project injects its content through
  the `TAURI_UPDATER_PUBLIC_KEY` repository Actions variable instead of
  committing it. Store the generated key contents, not local filesystem
  paths, in GitHub.
- Updater signatures prove that a payload was issued by this project, but are
  separate from operating-system code signing.

### Key continuity and rotation

The committed `src-tauri/updater-public-key.sha256` is the production trust
continuity anchor. Normal CI has no override: a mismatched repository
variable fails both signing preflight and release verification. Key rotation
is a break-glass recovery, not a routine secret update. Generate and back up
the new pair, prepare a direct-install bootstrap for every existing client,
and update the committed fingerprint in an explicitly reviewed security
change. Do not change the variable or fingerprint alone; old installed
clients cannot trust a release signed only by the replacement key.

### container.yml — the image pipeline

`.github/workflows/container.yml` accepts a published-Release event, but a
Release published by `release.yml` with `github.token` does not recursively
start another workflow. After the signed tag pipeline publishes the Release,
dispatch `container.yml` explicitly for that tag with `publish_latest=true`
for a stable release. It checks out the release tag and builds each
architecture natively (amd64 on `ubuntu-24.04`, arm64 on `ubuntu-24.04-arm` —
no QEMU emulation for release artifacts). Via `docker-bake.hcl`, each leg
builds its own `linux/<arch>` smoke images in parallel: the main
`ghcr.io/klarkxy/opencode-go-mgr` service and the
`ghcr.io/klarkxy/opencode-go-mgr-browser` sidecar. The main smoke covers the
dashboard, authentication, and license. The browser smoke starts Xvfb/noVNC
under a read-only root, zero capabilities, Chromium-compatible seccomp
configuration, and no host-published port, then uses the token-protected
control API to launch a real ordinary Chromium process with a persistent
profile.

All verified results — two images per architecture — are pushed by digest
without assigning a mutable name, then enter the repository-wide serialized
tag queue. `resolve` is the only job that interprets the requested tag or
optional `source_ref`; both native build legs check out that resolved full
commit SHA and fail if `HEAD` differs. The publishing job uses the immutable
`github.workflow_sha`, so the privileged registry helper always matches the
reviewed workflow definition rather than executable files from a hotfix ref.

Before writing a user-visible tag, the publishing job uses `docker buildx
imagetools create --dry-run` to assemble each candidate OCI index locally. It
hashes the exact returned JSON and validates both architecture children plus
the index version/revision annotations. The main and browser `X.Y.Z` and
`sha-<12-character-commit>` tags are all preflighted against those locally
known digests before the browser tags, then the main tags, are created and
verified. Existing immutable tags are accepted only at the exact candidate
digest.

Next, an empty Docker credential directory must anonymously pull both exact
version tags, and GitHub must successfully publish signed provenance for both
final index digests. Only then does the same serialized job re-read every
remote moving channel and preflight the pair again. Stable `X.Y` and opted-in
`latest` either converge both images at the candidate or retain an already
aligned newer pair; the browser moves before the main image, and a split pair
fails closed. Each architecture image also records an SPDX SBOM and BuildKit
SLSA provenance. `X.Y.Z` and `sha-*` are release-specific immutable tags;
`X.Y` and `latest` are monotonic moving channels. The browser image is a GHCR
package, not a GitHub Release asset, so the native release keeps only the
assembled GitHub attachments (the workflow compares that exact set, and the
local verifier pins the current 15-file contract).

Package visibility is managed separately from the linked repository, so the
workflow cannot rely on its repository token to make a package public. A new
browser package does not exist until its first digest is pushed. Consequently,
the first `container.yml` run that creates that package is expected to stop at
the anonymous-pull gate while the package still has GitHub's default private
visibility. This is the only bootstrap exception: set the new browser package
to **Public** (and confirm the main package is also Public), then manually
rerun `container.yml` for the same tag. Immutable-tag replay is accepted only
at the same digests, so the rerun completes the original publication without
replacing artifacts. Do not treat the container distribution as complete until
that rerun is green. Every later release must pass the anonymous gate on its
first run.

Before the first stable release on this dual-architecture path, publish a
temporary SemVer prerelease and dispatch `container.yml` with
`publish_latest=false`. Use that rehearsal to prove both native runners,
package visibility, anonymous pulls, exact index children, and both signed
provenance records. Do not use a stable tag as the rehearsal and do not
advance `X.Y` or `latest` until the prerelease run is fully green.

After tag publication the gate uses an empty Docker credential directory to
pull both exact-version tags. A private or inaccessible package therefore
fails `container.yml` instead of appearing as a successful public Compose
dependency.

A manual dispatch can backfill an existing release tag and must opt in before
updating `latest`. `resolve` checks out the exact `refs/tags/<tag>` ref (or
the explicit hotfix `source_ref`), verifies the release tag and repository
version, and emits one full SHA; no downstream job re-resolves the symbolic
input. Rebuilding different bytes for an existing full-version or `sha-*` tag
fails instead of overwriting it; only an exact-digest replay is accepted. Its
GitHub signing certificate identifies the workflow ref that triggered the
dispatch, even though the build checks out the resolved release commit. Do
not describe a historical manual backfill as tag-triggered provenance; normal
`release.published` runs use the release tag context.

After publication, record the digest and verify both the OCI index and the
GitHub attestation. Constrain verification to this signer workflow:

```bash
docker buildx imagetools inspect ghcr.io/klarkxy/opencode-go-mgr:X.Y.Z
docker buildx imagetools inspect ghcr.io/klarkxy/opencode-go-mgr-browser:X.Y.Z
docker buildx imagetools inspect --raw \
  ghcr.io/klarkxy/opencode-go-mgr@sha256:<digest>
docker buildx imagetools inspect --format '{{json .SBOM}}' \
  ghcr.io/klarkxy/opencode-go-mgr@sha256:<digest> > sbom.json
gh attestation verify \
  oci://ghcr.io/klarkxy/opencode-go-mgr@sha256:<digest> \
  --repo klarkxy/opencode-go-mgr \
  --signer-workflow klarkxy/opencode-go-mgr/.github/workflows/container.yml
gh attestation verify \
  oci://ghcr.io/klarkxy/opencode-go-mgr-browser@sha256:<browser-digest> \
  --repo klarkxy/opencode-go-mgr \
  --signer-workflow klarkxy/opencode-go-mgr/.github/workflows/container.yml
```

SBOM and provenance are supply-chain metadata, not vulnerability scanning.
The GitHub attestation signs the provenance statement; this project does not
currently add a separate Cosign image signature.

Current Windows installers are unsigned and macOS uses ad-hoc signing (`-`),
not Developer ID notarization. Review native candidate smoke results and these
platform warnings before pushing the release tag, because a successful tag
workflow publishes automatically. Windows/Linux ARM64, 32-bit x86, RPM, Snap,
and app stores remain unsupported. Signed in-app update is limited to
updater-enabled installed desktop builds; 1.4.1, development builds, CLI, and
Docker retain the direct/manual path.

### CI Coverage Boundaries

Pull requests automatically receive the three-job quality gate: frontend
checks (including the Dashboard V3 contract), Linux workspace Rust
tests/Clippy (including the Tauri crate), and the Windows job that covers
compilation and unit tests for Windows-only Tauri behavior. Native
installer/package smokes remain manual release candidates or tag runs. The
container workflow covers `linux/amd64` and `linux/arm64` (each built and
smoke-tested on its native runner) and runs after a release is published or
manually dispatched.

CI does not drive real desktop UI interactions or launch real Claude Desktop
or Gemini CLI clients, and it does not test backup/restore, database
downgrade, migration rollback, an upstream account, or a real gateway
request. Rust tests cover Gemini/Claude Desktop routing, authentication,
alias rewriting, non-stream conversion, SSE event shapes, Dashboard V3 CAS,
the V2 410 tombstone, v27 open/backup, and host lifecycle source contracts,
but they cannot prove that new versions of third-party clients still accept
the generated configuration. The main container smoke checks TCP health,
dashboard HTML, auth status, the bundled license, and a protected settings
request returning `401`. The browser smoke launches real Chromium and
verifies its profile and absence of public ports, but it does not log in to
Google/OpenCode, operate noVNC keyboard/clipboard, or make a real payment.
Google data-center-IP risk, desktop browser discovery, cookie persistence
across restarts, and remote account switching remain manual checks.

## Release Procedure

1. Choose `X.Y.Z` (or an immutable SemVer prerelease such as
   `X.Y.Z-beta.N`) and set it in `package.json`, `src-tauri/tauri.conf.json`,
   the workspace `Cargo.toml`, `src-tauri/Cargo.toml`, and the header plus
   default main and browser images in `compose.example.yaml`.
2. Run `cargo check --workspace --all-targets` to refresh `Cargo.lock`, then
   run `pnpm install --frozen-lockfile`, `cargo fmt --all -- --check`,
   `pnpm run test`, `pnpm run design:lint`, `pnpm run contract:v3:check`,
   `pnpm run release:check`, and `pnpm run build`. Commit the intended
   lockfile changes; never hand-edit them.
3. Compare against the previous public tag, review the diff and
   current-platform `release/` payloads, then commit the version, lockfile,
   documentation, and release-note changes.
4. Merge the reviewed change first. On the final commit already on `main`,
   create an annotated tag with `git tag -a vX.Y.Z -m "OCG Manager vX.Y.Z"`
   (preserving the prerelease suffix when applicable), then push the tag.
   Never tag a branch commit that will later be squash-merged.
5. Wait for `quality`, `preflight`, every native matrix job, `draft-release`,
   `verify-release`, and `publish-release` to pass. Confirm that publication
   converted the same verified draft, then review the exact assembled
   attachments, smoke logs, platform warnings, and notes generated from the
   previous-tag diff.
6. Explicitly dispatch `container.yml` for the published tag (for example,
   `gh workflow run container.yml --ref main -f tag=vX.Y.Z -f publish_latest=true`;
   omit `source_ref`), wait for it to pass, verify both GHCR packages are
   public, inspect each version and digest, and anonymously pull both
   full-version tags.

Treat published assets and tags as immutable. If a published payload is
wrong, ship a new patch version; do not replace the asset or retarget the
tag.

## Release Validation Checklist

Run these checks **before** publishing a `v*` tag. The CI smoke flow covers
most of them; the manual parts need a real desktop.

- [ ] All three jobs in the reusable quality gate are green (including
      `contract:v3:check`); the tag-only signed `release:check` passed; every
      selected `pnpm run build` and platform smoke is green.
- [ ] `git diff --check` is clean, the previous-tag diff contains only the
      intended release scope, and all four code version manifests,
      `compose.example.yaml`, plus the four local Cargo lock entries agree.
- [ ] Each runner's `release/SHA256SUMS` matches every payload in that
      directory; `verify-release` accepted the exact assembled asset set,
      updater manifest, four signatures, checksums, and GitHub server digests.
- [ ] Run `cargo test -p ocg-core gemini` and
      `cargo test -p ocg-core claude_desktop`. Exercise Gemini
      `generateContent` and `streamGenerateContent` with Bearer, `x-api-key`,
      and `x-goog-api-key` against both a Chat-native and a Messages-native
      model; confirm Google JSON/SSE error and usage envelopes, HTTP status,
      and SSE termination match the client protocol. Confirm `countTokens`
      and `embedContent` return the documented `501` response and an unknown
      action returns `404`.
- [ ] Confirm a non-empty Gemini `safetySettings` request returns `400`,
      while `null` and `[]` remain accepted. Exercise representative
      unsupported `cachedContent`, `fileData`, Google Search, and `urlContext`
      requests so they fail before any upstream request is billed. Treat
      `topK` and `thinkingConfig` as compatibility hints only; do not assert
      native Gemini-equivalent semantics in smoke tests.
- [ ] Exercise authenticated Claude Desktop model discovery and Messages
      alias rewriting. Save all three mappings through
      `PUT /dashboard/api/v3/claude-desktop/models` (with CAS tokens), restart
      with the same data directory, and verify the mappings survive. On a
      non-loopback dashboard, verify the mapping API returns `401` without a
      valid session. Confirm the retired V2
      `PUT /dashboard/api/claude-desktop/models` is authenticated `410`.
- [ ] Open the **Applications** view and confirm all 16 guides are present
      and selectable. Spot-check that copied results contain no masked key,
      and actually launch Claude Desktop and Gemini CLI once each for a text
      and a tool call.
- [ ] Cover schema v16 migration, schema v27 (`access_keys`, pre-v3 backup +
      SHA-256 sidecar, dropped `sub_gateway_keys` and `accounts.usage_sync_*`,
      ciphertext validated not rewritten), older pre-v22/pre-v23 rollback
      copies when present, Alias / upstream log identity, optional native
      cost, unverified GOAT rows stay disabled `pending`, Zen Free catalog
      persistence, provider contract scopes / model-protocol tables, legacy
      `key + ready`, managed transitions (forward one step / rewind earlier
      steps / no skip-forward), pending-route isolation, the invite URL
      allowlist and demo-default write-back, and the
      `2xx`/`429`/`401`/`403`/network/`5xx` key-verification branches. Confirm
      that no DTO or log contains a plaintext key except the session-protected
      `GET /dashboard/api/v3/connection` payload.
- [ ] Confirm authenticated `GET /v1/models` and protected
      `GET /dashboard/api/v3/application-models` are local reads and make no
      upstream request. `/v1/models` is currently routeable published aliases
      plus eligible Custom IDs; `application-models` is Go routeable aliases ∩
      the active pricing snapshot (highspeed inherits the base row) and must
      not include Custom. Unknown models return `400` on Chat / Responses /
      Messages / Gemini unless they match that `/v1/models` list. Command
      Code GOAT / SCNet Token Plan drafts stay disabled, unroutable
      (`routable=false`), and `501` on verify. Do not smoke GOAT or SCNet as
      live routing, usage, pricing, or provider guides. These local-list and
      fail-closed checks do not require live provider keys.
- [ ] Bounded fake-upstream Custom API smoke (no live provider key): URL
      credentials are rejected; a `2xx` JSON object verifies; the card stays
      disabled; explicit enable is required; declared model/protocol
      forwarding succeeds; redirects are denied; dashboard/client auth is not
      forwarded and only the configured Bearer or `x-api-key` is sent;
      successful logs are unpriced/`cost_state=unknown` with no quota debit;
      editing the URL, key, or capabilities re-pends verification and disables
      the account. Confirm Direct/Manual/Auto inherit the process-wide proxy.
- [ ] Verify Edge/Chrome priority on Windows and browser discovery on
      macOS/Linux. With two accounts, prove profile isolation and cookie
      persistence across restart. Reset must sign out of the console but keep a
      completed key; delete must clean new and legacy profiles; legacy WebView
      profiles must not be imported.
- [ ] Manually complete (optional) sign-in identity → invite URL → OpenCode
      login → payment review → key paste. A tester performs real payment only
      when explicitly intended. Console opens `opencode.ai/auth`. Log in once
      for a legacy key account and verify later access to authoritative quota
      and referral use. For a ready Key account and a ready managed account,
      exercise **Refresh quota** against official `/zen/go/v1/usage` (invalid
      key, 409 after a key change, and network/schema failures must error
      clearly and leave the previous local calibration). Cover desktop
      and Docker sidecar paths.
- [ ] On Windows, run the installer once, confirm SmartScreen warning text,
      open the dashboard, add an account, send one request.
- [ ] On macOS, mount the DMG, confirm the **Open Anyway** flow works, open
      the dashboard, add an account, send one request.
- [ ] On Linux, install the `.deb`, launch the AppImage, confirm the
      dashboard opens under Xvfb on CI and under a real Wayland or X11
      session locally.
- [ ] On Windows, verify `auto_start` toggles the `HKCU\...\Run\OCG Manager`
      value and that the value is removed on uninstall.
- [ ] Confirm `scripts/release.mjs` reported a successful atomic replacement
      of `release/` and that the previous `release/` is gone.
- [ ] Build both containers locally and confirm UID/GID `10001`, bundled
      `LICENSE`, read-only/capability hardening, dashboard authentication,
      and backup/restore ownership on isolated volumes. Run
      `docker compose --profile browser up -d` and verify one Chromium,
      noVNC keyboard/clipboard, account switching, sidecar restart, 1 GiB
      shm, no public port, and two-volume backup/restore.
- [ ] Review the intended GitHub Release notes and the unsigned/ad-hoc
      warnings before pushing the tag; after publication, confirm the same
      notes and exact verified asset set are public.
- [ ] After publishing, confirm `container.yml` passed and anonymously pull
      the main image and `ghcr.io/klarkxy/opencode-go-mgr-browser:<version>`
      by their expected digests; verify each signer workflow, SBOM, and SLSA
      provenance, while the GitHub Release remains the exact assembled asset
      set.

## Known Debt

- The server emits 409 code `revisionConflict`, but
  `src/api/dashboard-v3.ts` still checks `revision_conflict`. The matching
  frontend test also mocks the obsolete spelling, so real conflicts do not
  trigger the intended token/resource refresh. Until fixed, users must refresh
  and re-apply the mutation manually.
- `dashboard.rs` still contains the retired V2 REST handlers behind the 410
  tombstone, plus live V2 auth and the V2 browser WebSocket. Do not treat
  those handlers as the live dashboard contract. New JSON belongs on
  `dashboard_v3`.
- Some frontend unit tests still import historical types from
  `src/api/tauri.ts`. Production pages use `dashboard-v3.ts` /
  `dashboard.ts`. Do not add new `invoke()` usage; do not document
  `tauri.ts` as the live client.
- Auto-start is capability-gated: only Windows release/installed Tauri
  processes inject the registry sync hook. Development builds, the CLI,
  Docker, macOS, and Linux dashboards do not expose the switch. Dock
  visibility is macOS Tauri only.
- Existing generated Tauri schema files are noisy in diffs; avoid touching
  them unless the Tauri config actually changed.
- Streaming cost is exact only when upstream emits usage chunks. Chat streams
  request `stream_options.include_usage`. Without a chunk, Go rows end as
  `success_no_usage`; Zen success without usage stays `success` / `free`.
- Legacy `profiles/<account_id>` WebView profiles are not migrated to
  external Chromium, so users sign in again after upgrading. The old path is
  retained only for safe reset/delete cleanup; never attempt cross-engine
  reuse.
- The Responses endpoint is stateless. `previous_response_id`, `conversation`,
  `store: true`, and `background: true` return `400` rather than being
  silently ignored. This is intentional — see `protocol.rs` and the User
  guide.
- Gemini is a compatibility input, not a native upstream. Only
  `generateContent` and `streamGenerateContent` forward requests;
  `countTokens` and `embedContent` return `501`. Non-empty safety policy,
  cached content, file-backed media, Google-hosted tools, and other semantics
  that cannot survive conversion are rejected with `400`. `topK` and
  `thinkingConfig` may be accepted for client compatibility but are not a
  promise of equivalent behavior on Chat Completions or Messages upstreams.
  Every other non-null `generationConfig` field must be mapped or rejected;
  never add a silent pass-through exception.
- Claude Desktop only advertises three fixed Claude aliases, mapped to the
  supported actual models; it does not mean OCG Manager provides native
  Claude 4.6 models or the full Anthropic Models API.
- Command Code GOAT and SCNet Token Plans are schema/UI drafts only. They
  create disabled `pending` accounts; verification is `501`; they are not
  selected for Alias routing. SCNet official usable-model and endpoint
  snapshots are adapter input only and must not be published as client
  aliases. Do not document or ship those families as live support. Custom API
  is live under the trusted-administrator boundary (`custom.rs` +
  `custom_http.rs`); keep that path out of GOAT/SCNet anti-abuse wording.
- Custom provider-scope protocol probes are not on V3; the V2 account-owned
  probe path is 410. Custom verify and model discovery are the live Custom
  operational paths.
- `console_usage.rs` remains frozen. Do not call, extend, or delete it in the
  current V3 implementation.
- Databases that once ran the unreleased multi-Key development build (PR
  #43 config-embedded shape) may show two "Primary" rows in
  `/logs/forward/keys` (old random UUID plus `PRIMARY_KEY_ID`). Acceptable
  leftover; rebuild `data.sqlite` if you need a clean attribution set.
  First start still backfills NULL historical rows to `PRIMARY_KEY_ID`.

## Deliberate Non-Goals

- Dynamic / plugin provider extension, user-defined adapters, or adapters
  that own SQLite, `CoreState`, or a raw `reqwest::Client`.
- Remote node sync, an Admin API, or a multi-tenant control plane.
- Tauri `invoke` as a dashboard data path; WebView commands stay removed.
- Request-time upstream discovery on `GET /v1/models` or
  `GET /dashboard/api/v3/application-models`.
- Live GOAT or SCNet routing, usage, pricing, verification, or provider
  guides.
- `/embeddings`, Gemini `embedContent` (501), or Gemini `countTokens` as a
  real upstream count (501 so Gemini CLI can fall back locally).
- Gemini as an upstream protocol.
- Automatic pricing or Zen catalog polling.
- Cross-engine reuse of legacy WebView profiles.
- Database downgrade support or letting an older binary open a newer schema.
- Windows/Linux ARM64, 32-bit x86, RPM, Snap, app-store packages, Windows
  Authenticode, or Apple notarization.
- A second Cosign image signature on top of GitHub provenance.

## Coding Conventions

- **Ponytail principle.** Prefer deleting code over adding code; reuse
  existing helpers before adding new abstractions. The codebase favors flat
  call sites over speculative indirection — but do not omit required CAS,
  tombstones, or fail-closed checks.
- **Keep the crate DAG.** Domain and gateway stay I/O-free. Facades reexport
  item-by-item. Adapters return `AttemptSpec`. `forward_once` is one
  upstream call. Dashboard V3 does not import `gateway`.
- **No Tauri `invoke()` paths.** The Vue data path is HTTP
  `/dashboard/api/v3`. Do not register `generate_handler`.
- **Do not revive protected V2 REST.** New JSON is V3. The 410 tombstone
  stays in front of retired `/dashboard/api/...` paths.
- **Do not weaken security boundaries.** Gateway authentication, key
  obfuscation, URL validation, cooldown writes, SSE pass-through, and the
  ConnectionInfo secret boundary are not simplification candidates.
- **Do not add remote sync.** Each node is managed through its own dashboard.
- **Capability-gate `auto_start` and `show_dock_icon`.** Only the Windows
  release/installed Tauri process injects the registry sync hook; Dock is
  macOS Tauri only.
- **Local Alias lists stay local.** Authenticated `GET /v1/models` and
  dashboard `application-models` must not grow request-time upstream
  discovery. The explicit Zen Free refresh on Providers is the only
  directory-fetch exception and is restricted to the fixed official
  endpoint. Do not equate the two lists; do not invent a `requested_alias`
  log field.
- **Don't re-invent `cargo test` ergonomics.** The CLI and core use
  `parking_lot::Mutex`, which is not re-entrant. When a function needs to
  call another lock holder, `drop` the guard first.
- **Match the surrounding style.** When you change code in a file, the new
  code should look like the old code: same comment density, naming, and
  idiom.

---

[中文维护者指南](MAINTAINER.zh-CN.md) · [User guide](USER.md) ·
[用户指南](USER.zh-CN.md) · [Docs index](README.md) ·
[v27 recovery](MAINTAINER-v3-migration.md) ·
[Back to README](../README.md)
