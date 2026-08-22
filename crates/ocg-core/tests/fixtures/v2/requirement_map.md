# v2.0 Alias / multi-Plan black-box requirement map

Tests live in `crates/ocg-core/tests/v2_alias_plan_contracts.rs`. They speak public HTTP and JSON only.

| Test | Accepted requirement |
| --- | --- |
| `providers_catalog_is_the_only_plan_source` | Public catalog is the one Plan source (`GET /dashboard/api/providers/catalog`, same body as `/providers`). Entries publish `display_name`/`display_family`, `creation_availability`, `verification_policy`, `verification_runtime_availability`, `routable`, `form_fields`, `risk_notice` (when required), `model_aliases`, `pricing_availability`, `usage_availability`, and `manual_usage_calibration`. `model_aliases` is the Alias registry's currently routeable mappings for that offering (deterministic order, no raw IDs). GOAT/SCNet are `verification_policy=required` and unroutable with empty aliases; GOAT alone exposes manual usage calibration because it has no machine-readable usage endpoint. Custom is `verification_policy=required`, `verification_runtime_availability=available`, and catalog-routable with empty static aliases (client IDs come from account capabilities). SCNet offerings are `token-plan-basic`/`token-plan-standard`/`token-plan-premium`. |
| `unknown_offering_create_fails_closed` | Unknown provider/offering is rejected; no account is persisted. |
| `client_models_list_exposes_aliases_not_raw_upstream_ids` | Outbound `/v1/models` is a local routeable Alias registry list (deterministic order, `owned_by` = routeable `provider_id`). Zero Go accounts is enough; it never calls, filters, or restores upstream `/v1/models`, and never advertises raw IDs. |
| `claude_desktop_models_remain_role_aliases` | Claude Desktop still advertises only the three role aliases (not the Plan model union). |
| `alias_request_rewrites_response_model_to_client_name` | Non-stream responses rewrite `model` to the client-requested name. |
| `unique_raw_upstream_id_pins_to_one_provider_and_skips_go` | A unique raw upstream ID is pinned to that provider and must not fall back to Go. |
| `ambiguous_raw_upstream_id_is_rejected` | A raw ID mapped to more than one Plan returns `ambiguous_model_id` and does not call upstream. Live catalog/registry has no overlap unless an eligible Custom capability collides with a distinct provider mapping. Structured coverage is `v2_alias_runtime::ambiguous_model_id_is_structured_across_client_formats`. |
| `go_alias_request_still_routes_and_logs_opencode_go` | OpenCode Go alias routing remains compatible. |
| `zen_free_explicit_free_model_stays_anonymous` | Zen Free stays anonymous (no account Key) and compatible. |
| `go_import_remains_immediately_routable_without_verification` | Go import stays `not_required` and enabled after create. |
| `goat_and_scnet_create_disabled_pending_drafts` | GOAT / SCNet / Custom create as disabled pending drafts. SCNet create sends `acknowledgements:[{acknowledgement_id,version}]` matching catalog `risk_notice`. Custom create sends `custom_config:{base_url,upstream_protocol,auth_scheme}` plus non-empty `model_capabilities`. Custom is catalog-routable but create still does not auto-enable. |
| `disabled_draft_is_not_selected_for_alias_routing` | Disabled drafts are not selected for alias routing. |
| `verify_runtime_unavailable_leaves_draft_unchanged` | `POST /accounts/{id}/verify` is 501 for GOAT/SCNet; the draft stays disabled/`pending`. Custom verification runtime is available and is covered by the Custom trusted-admin black-box tests. |
| `scnet_create_requires_versioned_acknowledgement` | SCNet create requires a catalog-versioned risk acknowledgement. |
| `scnet_acknowledgement_persists_and_does_not_runtime_block` | Acknowledgement is persisted as `acknowledgement_id`/`version`/`content_hash`/`accepted_at` and does not itself runtime-block after confirmation. |
| `account_secrets_absent_from_json_errors_and_logs` | Account Keys never appear in dashboard JSON, errors, or logs. |
| `forward_logs_distinguish_requested_alias_and_upstream_model` | Logs distinguish `requested_model`, `resolved_alias`, `upstream_model`, `provider_id`, `offering_id`. |
| `alias_stream_does_not_cross_account_retry_after_output` | After downstream output, the gateway must not retry across accounts. |

Out of scope for this slice: live GOAT / SCNet network behavior. Custom trusted-admin runtime coverage lives in `custom_trusted_admin.rs`.
