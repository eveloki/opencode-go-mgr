[简体中文](extending.zh-CN.md)

# Extending OCG Manager

## Add or change a provider (sealed)

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

## Add or change a Dashboard V3 endpoint

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

## Add a host capability

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
| Interrupted v27 open | Transaction rolled back; pre-v3 backup may already exist | See [storage-migration.md](storage-migration.md) |
| Settings port change bound the old port | Rebind failed; compensation restored config | Check gateway logs; concurrent writes are serialized by `settings_host_effects` |
| Usage loop still running after `stop_gateway` | Listener stop does not cancel `ControlPlaneWorkers` | Drop `CoreState` (process exit) |
---

[Maintainer guide index](../MAINTAINER.md) · [简体中文](extending.zh-CN.md) · [Docs index](../README.md)
