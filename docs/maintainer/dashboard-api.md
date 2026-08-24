[简体中文](dashboard-api.zh-CN.md)

# Dashboard API

## Dashboard V3

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
snake-case code `revision_conflict`; see [Known Debt](known-debt.md). Revision and generation
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

## Retired V2 REST

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
---

[Maintainer guide index](../MAINTAINER.md) · [简体中文](dashboard-api.zh-CN.md) · [Docs index](../README.md)
