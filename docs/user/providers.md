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

**Model catalog** is local. The matrix has one row per model and three columns —
Chat Completions, Responses, and Messages. Each cell shows the effective state
of that model/protocol pair and carries a three-state control: **Auto** (no
override, follow the underlying evidence), **Force on** (enable the protocol for
that model up to the adapter safety ceiling), or **Force off** (disable it). You
can change cells individually, apply a state to the whole row, or apply a state
to the whole column.

The underlying evidence for a cell is one of: unavailable,
unsupported, static, preset (Custom declared protocols), probe confirmed, or
latest probe failed. The matrix cell shows the effective result after applying
the override: `force_on` enables even when evidence is absent, but never breaks
the adapter ceiling; `force_off` disables even when evidence says supported;
`auto` follows evidence. A failed probe is recorded; it does not erase static
capability.

Refresh is never automatic:

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
  failure keeps the last good snapshot.

Saved Go/GOAT catalogs feed local Alias resolution without another upstream
call. A model is advertised only when its saved contract has an effective,
known, enabled protocol; an unfamiliar Go ID therefore remains visible in the
Provider catalog but fails closed for client routing. Zen Free still derives one
extra alias by stripping `-free` from each saved ID, as described under
[Zen Free models](routing.md#zen-free-models).

Each row has a **Test** button. It auto-selects the first account in the scope
and probes every protocol for that model. A Popconfirm warns that the probe may
consume quota. Custom endpoint scopes do not show the Test button because Custom
account-level protocol probing has no V3 counterpart. Probe results confirm or
add support only inside the adapter safety ceiling.

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
