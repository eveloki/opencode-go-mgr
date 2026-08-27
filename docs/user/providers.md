[简体中文](providers.zh-CN.md)

# Providers

**Providers** is the supplier control plane — the page you land on when an old
bookmark still ends in `?view=pricing`.

Under the hood it is a static Provider Registry plus a handful of
capability-specific adapters. Custom API is a Configurable HTTP adapter, not a base class
everyone inherits from. Scopes are split like this:

- `Provider(provider_id)` for built-ins.
- `CustomEndpoint(account_id)` for each Custom destination. Custom endpoints
  stay isolated from each other and from the built-in families.

The left rail lists those scopes. The main pane has two tabs: **Model catalog**
and **Pricing**. The old catalog and model-contract views are merged into one
matrix on the Model catalog tab.

**Model catalog** is local. The matrix has one row per current catalog model and
three columns — Chat Completions, Responses, and Messages. Each cell is a binary
switch for the effective model/protocol state: turning it on writes `force_on`,
and turning it off writes `force_off`. Column menus can turn a whole protocol
column on or off. The switch updates immediately while the CAS-protected save
runs in the background; only affected cells show saving progress.

Underlying static, preset, and probe evidence remains in the contract, but is
not shown as a separate badge in this compact matrix. `auto` remains the stored
default until an explicit switch or a successful probe writes an override. A
successful provider-level probe pins `force_on`. Failed account attempts are
reported and retained as evidence, but never pin the shared protocol
`force_off`; only an explicit switch can do that.

For the built-in **OpenCode Go**, **Zen Free**, and **Command Code GOAT** scopes,
the catalog header offers **Restore static
protocol snapshot**. It makes no upstream request, keeps the current model
catalog, clears manual switches and probe evidence, and restores the static
protocol snapshot dated **2026-08-27**. Any current-catalog protocol pair that
is absent from that static snapshot is explicitly left off, so a newly
discovered model cannot become routable through fallback alone.

The compact source line, refresh action, and matrix share one content panel;
there is no separate catalog-summary card and no refresh-account selector.
Every refreshable scope uses the same action. OpenCode Go refreshes from the
official authenticated model endpoint with a backend-selected eligible Go
account, Zen Free uses the fixed keyless directory
`https://opencode.ai/zen/v1/models`, and Command Code uses its fixed public
official `/models` directory without selecting an account. Refresh is always
explicit.

Before the first successful refresh, the built-in static catalog is the initial
preset. After success, the saved official snapshot is authoritative and
replaces that preset. Models newly added by a refresh are visible in the matrix
with Chat Completions, Responses, and Messages all disabled. They become
enabled only after you turn on a cell or a successful Test confirms it. Existing
overrides and probe results for surviving models are preserved. A failed or
empty refresh keeps the previous snapshot.

Custom API continues to use each account's declared model IDs; discovery never
silently replaces that declaration. The account form **Fetch models** action is
an unsaved-form helper that merges selected IDs into the declaration being
edited. Command Code uses its public official `/models` directory: the GOAT
preset starts enabled, while additional models discovered later start disabled
until you enable their supported protocol in the matrix. It has no separate
Max or account-level GOAT/All mode.

Local catalogs feed Alias resolution without another request-time upstream
call. A model is advertised only when its saved contract has an effective,
known, enabled protocol. Alias names follow the OpenCode Go catalog. Zen Free
publishes only the suffix-stripped Alias while the original `-free` ID remains
available as an exact raw pin, as described under
[Zen Free models](routing.md#zen-free-models).

If every model/protocol cell for a Provider is off, that Provider contributes
no route. If an Alias has no enabled mapping from any Provider, it is removed
from downstream `GET /v1/models` supply.

Each row has a **Test** button. It probes every protocol for that model without
asking for an account. For each protocol the provider automatically tries its
eligible accounts in saved routing order and stops at the first success. A
Popconfirm warns that these real minimal requests may consume quota. Custom
endpoint scopes do not show the Test button because Custom account-level
protocol probing has no V3 counterpart. Models must belong to the current
provider catalog; all three requested protocol endpoints are then tested,
including for newly fetched models not yet in the static table. Each protocol
result is shown above the matrix with its success, failure, or skipped state,
HTTP status, readable upstream message, and a safe upstream help/billing link
when one is supplied. Every actual account attempt is recorded as a
redacted request log; probe traffic never enters Runtime Logs. One account
failure never disables a protocol that another eligible account can serve.

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
  does not enter OpenCode Go quota debit or invent a GOAT usage API.
- Zen Free has no price (egress-IP-shared free quota).
- Custom API is unpriced: successful forwards log `cost_state=unknown` with
  no quota debit and no official usage refresh.

There is no model-level quota pool.

Client requests never probe: at request time the gateway never discovers or
probes. Flow: Alias → account
eligibility → adapter ceiling → saved contract → per-model/per-protocol
effective state → passthrough or conversion. Authenticated `GET /v1/models` and
protected `GET /dashboard/api/v3/application-models` publish only currently
routable models that have an effective enabled protocol. The Applications picker
stays Go aliases ∩ active pricing and does not include Custom.

---

[User guide index](../USER.md) · [简体中文](providers.zh-CN.md) · [Docs index](../README.md)
