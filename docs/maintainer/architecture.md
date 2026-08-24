[简体中文](architecture.zh-CN.md)

# Architecture

## Four-layer crates

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

## ocg-core as composition / control plane

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

## Gateway execution

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

## Plan catalog

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
---

[Maintainer guide index](../MAINTAINER.md) · [简体中文](architecture.zh-CN.md) · [Docs index](../README.md)
