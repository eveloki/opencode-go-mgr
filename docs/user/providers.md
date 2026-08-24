[简体中文](providers.zh-CN.md)

# Providers

**Providers** is the supplier control plane. Older bookmarks that use
`?view=pricing` open this view.

The public base is a static Provider Registry plus capability-specific
adapters. Custom API is one Configurable HTTP adapter, not a base class.
Contract scopes are:

- `Provider(provider_id)` for built-ins. SCNet's three token-plan offerings
  share one SCNet scope.
- `CustomEndpoint(account_id)` for each Custom destination. Custom endpoints
  stay isolated from each other and from the built-in families.

The left rail lists those scopes. The main pane is **Overview**, **Model
catalog**, **Upstream protocol policy**, **Model contracts**, **Protocol
probe**, and scoped **Pricing**.

**Overview** shows the selected provider, scope revision, production-inference
state, catalog-routable state, disabled reasons, and each offering with its
bound accounts (enabled/disabled and verification). Command Code GOAT and
SCNet remain non-routable drafts: probes cannot promote them to production
routing.

**Model catalog** is local. Source labels are Static catalog, Official Zen
catalog, Custom discovery, or Account-declared. When a source URL is present
it is shown, as is the last successful refresh time (or Not yet refreshed).
Refresh is never automatic:

- OpenCode Go uses the static protocol catalog and does not refresh.
- Zen Free **Refresh model catalog** (choose the Zen Free account) calls the
  official keyless directory `https://opencode.ai/zen/v1/models`. A failed or
  empty refresh keeps the previous snapshot.
- Custom **Refresh model catalog** (choose that Custom account) discovers
  models from the configured base URL without changing declared capabilities.
  Truncated discovery is reported; a failed refresh keeps the previous
  snapshot. The account form **Fetch models** action remains a separate
  explicit edit that only merges IDs into the unsaved capability list.
- Command Code GOAT and SCNet do not refresh; their catalogs are adapter
  input only and are never published as client aliases.

Refreshing a catalog does not automatically publish a new stable alias. Zen
Free still derives one extra alias by stripping `-free` from each saved ID,
as described under [Zen Free models](routing.md#zen-free-models). Probe-confirmed extras
stay on the contract until they also match a published alias or an eligible
Custom declared ID.

**Upstream protocol policy** has three switches: Chat Completions, Responses,
and Messages. Turning a protocol on or off immediately applies to every
account in this scope and affects production routing. Switches take
precedence over probe evidence and static support. Disabling an account does
not delete the saved contract; re-enabling restores the saved catalog,
evidence, and switches.

**Model contracts** list each local model with its preferred protocol and
per-protocol status: Globally closed (the switch is off), Unavailable,
Unsupported, Static, Preset, Probe confirmed, or Latest probe failed (with a
sanitized error and last-probe time). Probe success may confirm or add
support only inside the adapter's structural ceiling. Probe failure is
recorded and does not delete static capability.

**Protocol probe** is an explicit action. It uses the selected test account
and sends a real minimal request that may consume quota — confirm that
warning before sending. Client requests never probe. GOAT and SCNet show that
probes are not available for this plan.

**Pricing** is scoped to the selected provider. Refresh remains manual only:

- OpenCode Go shows revision, documentation timestamp, window limits, token
  rates, `Usage`, and the quota-debit multiplier, and can fetch
  `https://opencode.ai/docs/go/` after you press refresh. A failed fetch or
  validation keeps the last successful snapshot. The allowance is not a quota
  pool and does not route requests: it only derives that debit multiplier
  (`monthly limit / Usage`). Saving a temporary override creates a new
  persistent revision for later estimates.
- Command Code GOAT and SCNet show dated official package references checked
  on `2026-08-22`; they still have no live pricing or usage path and do not
  become routable.
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
