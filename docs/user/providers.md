[简体中文](providers.zh-CN.md)

# Providers

**Providers** is the supplier control plane — the page you land on when an old
bookmark still ends in `?view=pricing`.

Under the hood it is a static Provider Registry plus a handful of
capability-specific adapters. Custom API is just one of those adapters, not a
base class everyone inherits from. Scopes are split like this:

- `Provider(provider_id)` for built-ins. SCNet's three token-plan offerings
  share one SCNet scope.
- `CustomEndpoint(account_id)` for each Custom destination. Custom endpoints
  stay isolated from each other and from the built-in families.

The left rail lists those scopes. The main pane is **Overview**, **Model
catalog**, **Upstream protocol policy**, **Model contracts**, **Protocol
probe**, and scoped **Pricing**.

**Overview** shows the selected provider, scope revision, production-inference
state, catalog-routable state, disabled reasons, and each offering with its
bound accounts (enabled/disabled and verification). Command Code GOAT routes
only through verified, explicitly enabled accounts. SCNet remains archived
and cannot be promoted to production routing.

**Model catalog** is local. Sources are labeled Static catalog, Official Zen
catalog, Custom discovery, or Account-declared; the URL and last refresh time
are shown when available. Refresh never happens on its own:

- OpenCode Go **Refresh model catalog** uses the selected Go account Key to
  call the official `GET /zen/go/v1/models` endpoint. The saved provider
  catalog replaces the static fallback after success; a failed or empty
  refresh keeps the previous snapshot.
- Zen Free **Refresh model catalog** (choose the Zen Free account) calls the
  official keyless directory `https://opencode.ai/zen/v1/models`. A failed or
  empty refresh keeps the previous snapshot.
- Custom **Refresh model catalog** (choose that Custom account) discovers
  models from the configured base URL without changing declared capabilities.
  Truncated discovery is reported; a failed refresh keeps the previous
  snapshot. The account form **Fetch models** action remains a separate
  explicit edit that only merges IDs into the unsaved capability list.
- Command Code GOAT **Refresh model catalog** uses a selected verified GOAT
  account to call the official `GET /provider/v1/models` endpoint. Success
  updates both that account's allowed catalog and the shared Provider catalog;
  failure keeps the last good snapshot. SCNet does not refresh.

Saved Go/GOAT catalogs feed local Alias resolution without another upstream
call. A model is advertised only when its saved contract has an enabled,
known protocol; an unfamiliar Go ID therefore remains visible in the Provider
catalog but fails closed for client routing. Zen Free still derives one extra
alias by stripping `-free` from each saved ID, as described under
[Zen Free models](routing.md#zen-free-models).

**Upstream protocol policy** gives you three switches: Chat Completions,
Responses, and Messages. Flipping one immediately applies to every account in
this scope and changes production routing. Switches beat probe evidence and
static support. Disabling an account keeps the saved contract intact;
re-enabling restores the saved catalog, evidence, and switches.

**Model contracts** list every local model with its preferred protocol and
per-protocol status: Globally closed, Unavailable, Unsupported, Static,
Preset, Probe confirmed, or Latest probe failed (with a sanitized error and
timestamp). A probe can only confirm or add support inside the adapter's
structural ceiling. A failed probe is recorded; it does not erase static
capability.

**Protocol probe** is an explicit action: pick a test account, send a real
minimal request, and accept that it may spend quota. Client requests never
probe. GOAT and SCNet show that probes are not available for this plan.

**Pricing** is scoped to the selected provider. **Refresh price table** only
hits the official source owned by that Provider. OpenCode and Command Code
keep separate revisions and last-good snapshots; one failing does not touch
the other. If a Provider later owns several priced Plans, the same action
refreshes those Plans only. Refresh stays manual:

- OpenCode Go shows revision, documentation timestamp, window limits, token
  rates, `Usage`, and the quota-debit multiplier, and can fetch
  `https://opencode.ai/docs/go/` after you press refresh. A failed fetch or
  validation keeps the last successful snapshot. The allowance is not a quota
  pool and does not route requests: it only derives that debit multiplier
  (`monthly limit / Usage`). Saving a temporary override creates a new
  persistent revision for later estimates.
- Command Code GOAT shows its saved official subscription/rate snapshot from
  `https://commandcode.ai/docs/plans/goat`. It is display/reference data and
  does not enter OpenCode Go quota debit or invent a GOAT usage API. SCNet is
  archived and has no price table.
- Zen Free has no price (egress-IP-shared free quota).
- Custom API is unpriced: successful forwards log `cost_state=unknown` with
  no quota debit and no official usage refresh.

There is no model-level quota pool.

At request time the gateway never discovers or probes. Flow: Alias → account
eligibility → adapter ceiling → saved contract → protocol switch →
passthrough or conversion. Authenticated `GET /v1/models` and protected
`GET /dashboard/api/v3/application-models` publish only currently routable
models that have an effective enabled protocol. The Applications picker stays
Go aliases ∩ active pricing and does not include Custom.

---

[User guide index](../USER.md) · [简体中文](providers.zh-CN.md) · [Docs index](../README.md)
