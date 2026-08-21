# v2.0 Alias / multi-Plan black-box requirement map

Tests live in `crates/ocg-core/tests/v2_alias_plan_contracts.rs`. They speak public HTTP and JSON only.

| Test | Accepted requirement |
| --- | --- |
| `providers_catalog_is_the_only_plan_source` | Public catalog is the one Plan source (`GET /dashboard/api/providers/catalog`, same body as `/providers`). |
| `unknown_offering_create_fails_closed` | Unknown provider/offering is rejected; no account is persisted. |
| `client_models_list_exposes_aliases_not_raw_upstream_ids` | Outbound `/v1/models` exposes aliases only, never raw upstream IDs. |
| `claude_desktop_models_remain_role_aliases` | Claude Desktop still advertises only the three role aliases (not the Plan model union). |
| `alias_request_rewrites_response_model_to_client_name` | Non-stream responses rewrite `model` to the client-requested name. |
| `unique_raw_upstream_id_pins_to_one_provider_and_skips_go` | A unique raw upstream ID is pinned to that provider and must not fall back to Go. |
| `ambiguous_raw_upstream_id_is_rejected` | A raw ID mapped to more than one Plan returns `ambiguous_model_id` and does not call upstream. |
| `go_alias_request_still_routes_and_logs_opencode_go` | OpenCode Go alias routing remains compatible. |
| `zen_free_explicit_free_model_stays_anonymous` | Zen Free stays anonymous (no account Key) and compatible. |
| `go_import_remains_immediately_routable_without_verification` | Go import stays `not_required` and enabled after create. |
| `goat_and_scnet_create_disabled_pending_drafts` | GOAT / SCNet create as disabled pending drafts. |
| `disabled_draft_is_not_selected_for_alias_routing` | Disabled drafts are not selected for alias routing. |
| `verify_success_atomically_enables_draft` | `POST /accounts/{id}/verify` success atomically sets verified + ready + enabled. |
| `scnet_create_requires_versioned_acknowledgement` | SCNet create requires a catalog-versioned risk acknowledgement. |
| `scnet_acknowledgement_persists_and_does_not_runtime_block` | Acknowledgement is persisted/versioned and does not itself runtime-block after confirmation. |
| `account_secrets_absent_from_json_errors_and_logs` | Account Keys never appear in dashboard JSON, errors, or logs. |
| `forward_logs_distinguish_requested_alias_and_upstream_model` | Logs distinguish `requested_model`, `resolved_alias`, `upstream_model`, `provider_id`, `offering_id`. |
| `alias_stream_does_not_cross_account_retry_after_output` | After downstream output, the gateway must not retry across accounts. |

Out of scope for this slice: live GOAT / SCNet / Custom network behavior.
