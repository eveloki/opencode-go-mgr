//! Black-box v2.0 Alias and multi-Plan contract tests.
//!
//! These tests drive public Gateway and dashboard HTTP/JSON. They are the
//! independent acceptance slice for the accepted unified-alias / multi-Plan
//! contracts and may fail on commit e3dea932 solely because that behavior is
//! not implemented yet.
//!
//! Requirement map: `fixtures/v2/requirement_map.md`.
//!
//! Out of scope: live GOAT / SCNet / Custom network calls.

use reqwest::StatusCode;
use serde_json::{Value, json};
use std::collections::{HashMap, VecDeque};

#[path = "fixtures/v2/harness.rs"]
mod harness;

use harness::*;

fn go_success_replies(keys: &[&str]) -> HashMap<String, VecDeque<FakeReply>> {
    let mut replies = HashMap::new();
    for key in keys {
        replies.insert(
            (*key).to_string(),
            VecDeque::from([FakeReply {
                status: 200,
                body: SUCCESS_CHAT_BODY,
            }]),
        );
    }
    replies.insert(
        String::new(),
        VecDeque::from([FakeReply {
            status: 200,
            body: SUCCESS_CHAT_BODY,
        }]),
    );
    replies
}

/// Catalog is the one Plan source. `/providers` must not diverge.
#[tokio::test]
async fn providers_catalog_is_the_only_plan_source() {
    let harness = V2Harness::start().await;
    let (catalog_status, catalog) = harness.get_json("/providers/catalog").await;
    let (compat_status, compat) = harness.get_json("/providers").await;
    assert_eq!(catalog_status, StatusCode::OK, "{catalog}");
    assert_eq!(compat_status, StatusCode::OK, "{compat}");
    assert_eq!(
        catalog, compat,
        "GET /providers must be the same Plan catalog as /providers/catalog"
    );
    let entries = catalog
        .as_array()
        .expect("catalog must be a JSON array of Plan entries");
    assert!(
        !entries.is_empty(),
        "catalog must list hardcoded Plans, got {catalog}"
    );

    let required = required_catalog_fields();
    let expected_plans = catalog_contract()["plans"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    for plan in &expected_plans {
        let provider_id = plan["provider_id"].as_str().unwrap();
        let offering_id = plan["offering_id"].as_str().unwrap();
        let entry = catalog_entry(&catalog, provider_id, offering_id).unwrap_or_else(|| {
            panic!(
                "catalog is the only Plan source and must include {provider_id}/{offering_id}: {catalog}"
            )
        });
        let missing = missing_fields(entry, &required);
        assert!(
            missing.is_empty(),
            "v2-contract: {provider_id}/{offering_id} missing catalog fields {missing:?}: {entry}"
        );
        if let Some(required_flag) = plan["verification_required"].as_bool() {
            assert_eq!(
                entry["verification_required"], required_flag,
                "{provider_id}/{offering_id} verification_required"
            );
        }
        if plan["requires_risk_acknowledgement"] == true {
            let ack = &entry["risk_acknowledgement"];
            assert!(
                ack.is_object(),
                "{provider_id}/{offering_id} must publish risk_acknowledgement: {entry}"
            );
            for field in ["id", "version", "content_hash"] {
                assert!(
                    ack[field].as_str().is_some_and(|value| !value.is_empty()),
                    "risk_acknowledgement.{field} is required: {ack}"
                );
            }
        }
        if let Some(aliases) = plan["required_aliases"].as_array() {
            let published = alias_names(entry);
            for alias in aliases {
                let alias = alias.as_str().unwrap();
                assert!(
                    published.contains(alias),
                    "{provider_id}/{offering_id} must publish alias {alias}, got {published:?}"
                );
            }
        }
    }

    harness.shutdown();
}

/// Unknown offerings fail closed at the dashboard create gate.
#[tokio::test]
async fn unknown_offering_create_fails_closed() {
    let harness = V2Harness::start().await;
    let before = harness.accounts().await;
    let (status, body) = harness
        .create_account(json!({
            "provider_id": "not-a-provider",
            "offering_id": "not-an-offering",
            "name": "should-not-exist",
            "key": GO_ACCOUNT_KEY,
            "expected_revision": harness.settings_revision().await
        }))
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert!(
        !json_contains_secret(&body, GO_ACCOUNT_KEY),
        "unknown-offering error leaked the Key: {body}"
    );
    let after = harness.accounts().await;
    assert_eq!(
        after.as_array().map(Vec::len),
        before.as_array().map(Vec::len),
        "unknown offering must not persist an account: {after}"
    );
    harness.shutdown();
}

/// `/v1/models` exposes aliases, not raw upstream IDs, even if a fake
/// upstream still lists vendor-prefixed names.
#[tokio::test]
async fn client_models_list_exposes_aliases_not_raw_upstream_ids() {
    let mut replies = go_success_replies(&[GO_ACCOUNT_KEY]);
    replies.insert(
        GO_ACCOUNT_KEY.to_string(),
        VecDeque::from([FakeReply {
            status: 200,
            body: MIXED_UPSTREAM_MODELS_BODY,
        }]),
    );
    let harness = V2Harness::start_with_upstream(Some(replies)).await;
    let _go = harness.create_go_account("go-main", GO_ACCOUNT_KEY).await;

    let (status, body) = harness.list_client_models().await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let ids = client_model_ids(&body);
    assert!(
        ids.iter().any(|id| id == GO_ALIAS),
        "client model list must include preferred Go alias {GO_ALIAS}: {ids:?}"
    );
    assert!(
        !ids.iter().any(|id| id == GOAT_UNIQUE_RAW_ID),
        "v2-contract: client model list must not advertise the GOAT raw id {GOAT_UNIQUE_RAW_ID}: {ids:?}"
    );
    assert!(
        !ids.iter().any(|id| id == "vendor-raw-not-an-alias"),
        "client model list must not proxy unknown raw upstream ids: {ids:?}"
    );
    assert!(
        ids.iter().all(|id| !id.contains('/')),
        "aliases are kebab-case and must not include provider-prefixed raw ids: {ids:?}"
    );

    let (app_status, app_models) = harness.get_json("/application-models").await;
    assert_eq!(app_status, StatusCode::OK, "{app_models}");
    let app_ids: Vec<String> = match &app_models {
        Value::Array(items) => items
            .iter()
            .filter_map(|item| {
                item.as_str()
                    .or_else(|| item["id"].as_str())
                    .map(str::to_string)
            })
            .collect(),
        other => panic!("application-models must list aliases: {other}"),
    };
    assert!(
        app_ids.iter().any(|id| id == GO_ALIAS),
        "Applications must copy aliases, not raw upstream ids: {app_ids:?}"
    );
    assert!(
        !app_ids.iter().any(|id| id == GOAT_UNIQUE_RAW_ID),
        "Applications must not expose the GOAT raw id: {app_ids:?}"
    );

    harness.shutdown();
}

/// Claude Desktop keeps the three role aliases.
#[tokio::test]
async fn claude_desktop_models_remain_role_aliases() {
    let harness = V2Harness::start().await;
    let (status, body) = harness.claude_desktop_models().await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let ids = client_model_ids(&body);
    assert_eq!(
        ids.len(),
        3,
        "Claude Desktop must keep exactly three role aliases: {body}"
    );
    assert_eq!(
        ids,
        vec![
            ocg_core::models::CLAUDE_DESKTOP_SONNET_ALIAS.to_string(),
            ocg_core::models::CLAUDE_DESKTOP_OPUS_ALIAS.to_string(),
            ocg_core::models::CLAUDE_DESKTOP_HAIKU_ALIAS.to_string(),
        ],
        "Claude Desktop must keep the advertised three-role aliases: {body}"
    );
    harness.shutdown();
}

/// Alias chat responses rewrite `model` back to the client-requested name.
#[tokio::test]
async fn alias_request_rewrites_response_model_to_client_name() {
    let harness = V2Harness::start_with_chat_success(&[GO_ACCOUNT_KEY]).await;
    let _go = harness.create_go_account("go-main", GO_ACCOUNT_KEY).await;
    let (status, body) = harness.chat(GO_ALIAS).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        body["model"].as_str(),
        Some(GO_ALIAS),
        "v2-contract: response.model must be the client-requested alias, not the upstream id: {body}"
    );
    assert_ne!(
        body["model"].as_str(),
        Some("upstream-should-not-leak"),
        "upstream model id leaked into the client response: {body}"
    );
    harness.shutdown();
}

/// A unique raw upstream ID is pinned to one provider. With only Go
/// routeable, the GOAT-shaped raw id must not fall through to OpenCode Go.
#[tokio::test]
async fn unique_raw_upstream_id_pins_to_one_provider_and_skips_go() {
    let harness = V2Harness::start_with_chat_success(&[GO_ACCOUNT_KEY]).await;
    let go = harness.create_go_account("go-main", GO_ACCOUNT_KEY).await;
    let (status, body) = harness.chat(GOAT_UNIQUE_RAW_ID).await;
    assert_ne!(
        status,
        StatusCode::OK,
        "raw id {GOAT_UNIQUE_RAW_ID} is uniquely GOAT and must not succeed on Go: {body}"
    );
    assert_eq!(
        harness.fake_call_keys(),
        Vec::<String>::new(),
        "unique raw id must pin to command-code/goat and must not call the Go upstream"
    );
    let logs = harness.forward_logs().await;
    for item in logs["items"].as_array().unwrap_or(&Vec::new()) {
        assert_ne!(
            item["provider_id"].as_str(),
            Some(OPENCODE_PROVIDER_ID),
            "raw GOAT id was attributed to Go: {item}"
        );
        assert_ne!(
            item["account_id"], go["id"],
            "raw GOAT id was routed to the Go account: {item}"
        );
    }
    harness.shutdown();
}

/// A raw upstream ID mapped to more than one Plan is rejected as
/// `ambiguous_model_id` and never reaches an upstream.
#[tokio::test]
async fn ambiguous_raw_upstream_id_is_rejected() {
    let harness = V2Harness::start_with_chat_success(&[GO_ACCOUNT_KEY, CUSTOM_ACCOUNT_KEY]).await;
    let _go = harness.create_go_account("go-main", GO_ACCOUNT_KEY).await;
    let catalog = harness.catalog().await;
    let overlaps = overlapping_raw_ids(&catalog);

    let mut requested = Vec::new();
    if !overlaps.is_empty() {
        requested.extend(overlaps.into_iter().map(|(raw, _)| raw));
    } else if catalog_entry(&catalog, CUSTOM_PROVIDER_ID, CUSTOM_OFFERING_ID).is_some() {
        for (name, alias) in [("custom-a", "custom-one"), ("custom-b", "custom-two")] {
            let revision = harness.settings_revision().await;
            let (status, body) = harness
                .create_account(json!({
                    "provider_id": CUSTOM_PROVIDER_ID,
                    "offering_id": CUSTOM_OFFERING_ID,
                    "name": name,
                    "key": CUSTOM_ACCOUNT_KEY,
                    "expected_revision": revision,
                    "custom": {
                        "base_url": harness.upstream_base_url.clone(),
                        "protocol": "chat_completions",
                        "auth": "bearer",
                        "models": [{
                            "alias": alias,
                            "upstream_model_id": CUSTOM_OVERLAP_RAW_ID
                        }]
                    }
                }))
                .await;
            assert!(
                status.is_success(),
                "Custom create is required to exercise overlapping raw ids when the catalog has none: {status} {body}"
            );
        }
        requested.push(CUSTOM_OVERLAP_RAW_ID.to_string());
    } else {
        panic!(
            "v2-contract: ambiguous_model_id is untestable until the catalog publishes overlapping raw ids or Custom create exists. catalog={catalog}"
        );
    }

    for raw in requested {
        let (status, body) = harness.chat(&raw).await;
        assert_eq!(
            status,
            StatusCode::BAD_REQUEST,
            "overlapping raw id {raw} must fail closed: {body}"
        );
        assert_eq!(
            error_type(&body),
            Some(AMBIGUOUS_ERROR_TYPE),
            "overlapping raw id {raw} must return {AMBIGUOUS_ERROR_TYPE}: {body}"
        );
        assert!(
            error_message(&body).to_ascii_lowercase().contains("alias"),
            "ambiguous error should point the client at an alias: {body}"
        );
    }
    assert!(
        harness
            .fake_call_keys()
            .into_iter()
            .all(|key| key != GO_ACCOUNT_KEY && key != CUSTOM_ACCOUNT_KEY),
        "ambiguous raw ids must not call any upstream: {:?}",
        harness.fake_calls()
    );
    harness.shutdown();
}

/// OpenCode Go alias routing remains the compatible paid path.
#[tokio::test]
async fn go_alias_request_still_routes_and_logs_opencode_go() {
    let harness = V2Harness::start_with_chat_success(&[GO_ACCOUNT_KEY]).await;
    let go = harness.create_go_account("go-main", GO_ACCOUNT_KEY).await;
    let (status, body) = harness.chat(GO_ALIAS).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(harness.fake_call_keys(), vec![GO_ACCOUNT_KEY.to_string()]);
    let logs = harness.forward_logs().await;
    let item = &logs["items"]
        .as_array()
        .and_then(|items| items.first())
        .unwrap_or_else(|| panic!("expected a forward log: {logs}"));
    assert_eq!(item["provider_id"].as_str(), Some(OPENCODE_PROVIDER_ID));
    assert_eq!(item["offering_id"].as_str(), Some(GO_OFFERING_ID));
    assert_eq!(item["account_id"], go["id"]);
    harness.shutdown();
}

/// Zen Free stays anonymous and does not send an account Key.
#[tokio::test]
async fn zen_free_explicit_free_model_stays_anonymous() {
    let harness = V2Harness::start_with_chat_success(&[GO_ACCOUNT_KEY]).await;
    let _go = harness.create_go_account("go-main", GO_ACCOUNT_KEY).await;
    let revision = harness.settings_revision().await;
    let (status, body) = harness
        .patch_json(
            "/providers/zen-free",
            &json!({
                "enabled": true,
                "free_alias_enabled": false,
                "expected_revision": revision
            }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");

    let (status, body) = harness.chat(FREE_MODEL).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let keys = harness.fake_call_keys();
    assert_eq!(
        keys,
        vec![String::new()],
        "Zen Free must remain anonymous and must not rotate a Go Key: {keys:?}"
    );
    let calls = harness.fake_calls();
    assert!(
        calls.iter().all(|call| {
            call.authorization.is_none()
                && call.x_api_key.is_none()
                && call.x_goog_api_key.is_none()
        }),
        "Zen Free leaked an auth header: {calls:?}"
    );
    harness.shutdown();
}

/// Go import stays immediately routable; verification is not required.
#[tokio::test]
async fn go_import_remains_immediately_routable_without_verification() {
    let harness = V2Harness::start_with_chat_success(&[GO_ACCOUNT_KEY]).await;
    let account = harness.create_go_account("go-main", GO_ACCOUNT_KEY).await;
    assert_eq!(account["enabled"], true, "{account}");
    assert_eq!(account["setup_step"], "ready", "{account}");
    let status = account["verification_status"]
        .as_str()
        .unwrap_or("not_required");
    assert_eq!(
        status, "not_required",
        "Go import must not require connection verification: {account}"
    );
    let (chat_status, body) = harness.chat(GO_ALIAS).await;
    assert_eq!(chat_status, StatusCode::OK, "{body}");
    harness.shutdown();
}

/// GOAT and SCNet create as disabled pending drafts.
#[tokio::test]
async fn goat_and_scnet_create_disabled_pending_drafts() {
    let harness = V2Harness::start().await;
    let catalog = harness.catalog().await;

    let goat = catalog_entry(&catalog, COMMAND_CODE_PROVIDER_ID, GOAT_OFFERING_ID)
        .expect("catalog must include command-code/goat");
    assert_eq!(
        goat["verification_required"], true,
        "v2-contract: command-code/goat must require verification: {goat}"
    );
    let (status, body) = harness
        .create_account(json!({
            "provider_id": COMMAND_CODE_PROVIDER_ID,
            "offering_id": GOAT_OFFERING_ID,
            "name": "goat-draft",
            "key": GOAT_ACCOUNT_KEY,
            "expected_revision": harness.settings_revision().await
        }))
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        body["enabled"], false,
        "GOAT must save as a disabled draft: {body}"
    );
    assert_eq!(
        body["verification_status"].as_str(),
        Some("pending"),
        "v2-contract: GOAT draft verification_status: {body}"
    );
    assert_eq!(
        body["key"], "",
        "draft JSON must not return the Key: {body}"
    );

    let scnet = scnet_entries(&catalog)
        .into_iter()
        .find(|entry| entry["offering_id"] == "standard")
        .or_else(|| scnet_entries(&catalog).into_iter().next())
        .expect("v2-contract: catalog must include an SCNet Token Plan");
    let ack = &scnet["risk_acknowledgement"];
    let (status, body) = harness
        .create_account(json!({
            "provider_id": scnet["provider_id"],
            "offering_id": scnet["offering_id"],
            "name": "scnet-draft",
            "key": SCNET_ACCOUNT_KEY,
            "expected_revision": harness.settings_revision().await,
            "acknowledgement": {
                "id": ack["id"],
                "version": ack["version"],
                "accepted": true
            }
        }))
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        body["enabled"], false,
        "SCNet must save as a disabled draft: {body}"
    );
    assert_eq!(
        body["verification_status"].as_str(),
        Some("pending"),
        "SCNet draft verification_status: {body}"
    );
    harness.shutdown();
}

/// Disabled drafts must not be selected when an alias is requested.
#[tokio::test]
async fn disabled_draft_is_not_selected_for_alias_routing() {
    let harness = V2Harness::start_with_chat_success(&[GO_ACCOUNT_KEY, GOAT_ACCOUNT_KEY]).await;
    let go = harness.create_go_account("go-main", GO_ACCOUNT_KEY).await;
    let (status, goat) = harness
        .create_account(json!({
            "provider_id": COMMAND_CODE_PROVIDER_ID,
            "offering_id": GOAT_OFFERING_ID,
            "name": "goat-draft",
            "key": GOAT_ACCOUNT_KEY,
            "expected_revision": harness.settings_revision().await
        }))
        .await;
    assert_eq!(status, StatusCode::OK, "{goat}");
    assert_eq!(
        goat["enabled"], false,
        "v2-contract: GOAT must create as a disabled draft: {goat}"
    );

    let (status, body) = harness.chat(GO_ALIAS).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(harness.fake_call_keys(), vec![GO_ACCOUNT_KEY.to_string()]);
    let logs = harness.forward_logs().await;
    let item = &logs["items"]
        .as_array()
        .and_then(|items| items.first())
        .unwrap_or_else(|| panic!("expected a forward log: {logs}"));
    assert_eq!(item["account_id"], go["id"]);
    assert_ne!(item["account_id"], goat["id"]);
    harness.shutdown();
}

/// Successful verify atomically enables the draft.
#[tokio::test]
async fn verify_success_atomically_enables_draft() {
    let mut replies = go_success_replies(&[GOAT_ACCOUNT_KEY]);
    replies.insert(
        GOAT_ACCOUNT_KEY.to_string(),
        VecDeque::from([
            FakeReply {
                status: 200,
                body: MIXED_UPSTREAM_MODELS_BODY,
            },
            FakeReply {
                status: 200,
                body: SUCCESS_CHAT_BODY,
            },
        ]),
    );
    let harness = V2Harness::start_with_upstream(Some(replies)).await;
    let (status, draft) = harness
        .create_account(json!({
            "provider_id": COMMAND_CODE_PROVIDER_ID,
            "offering_id": GOAT_OFFERING_ID,
            "name": "goat-verify",
            "key": GOAT_ACCOUNT_KEY,
            "expected_revision": harness.settings_revision().await
        }))
        .await;
    assert_eq!(status, StatusCode::OK, "{draft}");
    let id = draft["id"].as_str().expect("draft id");
    let (status, body) = harness
        .post_json(
            &format!("/accounts/{id}/verify"),
            &json!({ "expected_revision": harness.settings_revision().await }),
        )
        .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "v2-contract: POST /accounts/{{id}}/verify must exist and succeed against the fake upstream: {body}"
    );
    assert_eq!(body["enabled"], true, "{body}");
    assert_eq!(
        body["verification_status"].as_str(),
        Some("verified"),
        "{body}"
    );
    assert_eq!(body["setup_step"], "ready", "{body}");
    assert!(
        body["connection_verified_at"]
            .as_str()
            .is_some_and(|value| !value.is_empty()),
        "connection_verified_at must be stamped: {body}"
    );
    assert_eq!(body["key"], "", "{body}");
    harness.shutdown();
}

/// SCNet create requires the catalog's current acknowledgement version.
#[tokio::test]
async fn scnet_create_requires_versioned_acknowledgement() {
    let harness = V2Harness::start().await;
    let catalog = harness.catalog().await;
    let scnet = scnet_entries(&catalog)
        .into_iter()
        .next()
        .expect("v2-contract: catalog must include SCNet");
    let revision = harness.settings_revision().await;
    let (status, body) = harness
        .create_account(json!({
            "provider_id": scnet["provider_id"],
            "offering_id": scnet["offering_id"],
            "name": "scnet-no-ack",
            "key": SCNET_ACCOUNT_KEY,
            "expected_revision": revision
        }))
        .await;
    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "SCNet create without acknowledgement must fail: {body}"
    );

    let (status, body) = harness
        .create_account(json!({
            "provider_id": scnet["provider_id"],
            "offering_id": scnet["offering_id"],
            "name": "scnet-stale-ack",
            "key": SCNET_ACCOUNT_KEY,
            "expected_revision": harness.settings_revision().await,
            "acknowledgement": {
                "id": "not-the-catalog-id",
                "version": "0",
                "accepted": true
            }
        }))
        .await;
    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "stale acknowledgement version must fail: {body}"
    );
    harness.shutdown();
}

/// Acknowledgement is persisted and versioned. After confirmation it is a
/// warning, not a runtime block.
#[tokio::test]
async fn scnet_acknowledgement_persists_and_does_not_runtime_block() {
    let harness = V2Harness::start().await;
    let catalog = harness.catalog().await;
    let scnet = scnet_entries(&catalog)
        .into_iter()
        .next()
        .expect("v2-contract: catalog must include SCNet");
    let ack = &scnet["risk_acknowledgement"];
    let (status, body) = harness
        .create_account(json!({
            "provider_id": scnet["provider_id"],
            "offering_id": scnet["offering_id"],
            "name": "scnet-acked",
            "key": SCNET_ACCOUNT_KEY,
            "expected_revision": harness.settings_revision().await,
            "acknowledgement": {
                "id": ack["id"],
                "version": ack["version"],
                "accepted": true
            }
        }))
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");

    let stored = body
        .get("acknowledgements")
        .cloned()
        .or_else(|| body.get("acknowledgement").cloned())
        .unwrap_or_else(|| panic!("SCNet account must persist acknowledgement: {body}"));
    let record = if stored.is_array() {
        stored
            .as_array()
            .and_then(|items| items.first())
            .cloned()
            .unwrap_or(stored)
    } else {
        stored
    };
    assert_eq!(record["id"], ack["id"], "{record}");
    assert_eq!(record["version"], ack["version"], "{record}");
    assert_eq!(record["content_hash"], ack["content_hash"], "{record}");
    assert!(
        record["confirmed_at"]
            .as_str()
            .is_some_and(|value| !value.is_empty()),
        "acknowledgement must record confirmed_at: {record}"
    );
    assert_ne!(body["runtime_blocked"], true, "{body}");
    assert_ne!(record["blocks_runtime"], true, "{record}");
    if let Some(flag) = body.get("acknowledgement_blocks_runtime") {
        assert_eq!(flag, false, "{body}");
    }

    // Confirmation is not a runtime gate: a later verify/enable path may make
    // the card routeable. The acknowledgement itself must not add a block.
    assert_eq!(
        body["enabled"], false,
        "draft stays disabled until verify: {body}"
    );
    harness.shutdown();
}

/// Account Keys stay out of dashboard JSON, errors, and logs.
#[tokio::test]
async fn account_secrets_absent_from_json_errors_and_logs() {
    let harness = V2Harness::start_with_chat_success(&[GO_ACCOUNT_KEY]).await;
    let account = harness.create_go_account("go-secret", GO_ACCOUNT_KEY).await;
    assert_eq!(account["key"], "");
    assert_eq!(account["password"], "");
    assert!(!json_contains_secret(&account, GO_ACCOUNT_KEY));

    let listed = harness.accounts().await;
    assert!(!json_contains_secret(&listed, GO_ACCOUNT_KEY));

    let (status, unknown) = harness.chat("definitely-not-a-model-or-alias").await;
    assert_ne!(status, StatusCode::UNAUTHORIZED, "{unknown}");
    assert!(!json_contains_secret(&unknown, GO_ACCOUNT_KEY));

    let _ = harness.chat(GO_ALIAS).await;
    let logs = harness.forward_logs().await;
    let gateway_logs = harness.gateway_logs().await;
    assert!(!json_contains_secret(&logs, GO_ACCOUNT_KEY), "{logs}");
    assert!(
        !json_contains_secret(&gateway_logs, GO_ACCOUNT_KEY),
        "{gateway_logs}"
    );
    assert!(!json_contains_secret(&logs, GATEWAY_KEY), "{logs}");

    let (conn_status, connection) = harness.get_json("/connection").await;
    assert_eq!(conn_status, StatusCode::OK, "{connection}");
    assert!(
        !json_contains_secret(&connection, GO_ACCOUNT_KEY),
        "connection info must not include the account Key: {connection}"
    );
    harness.shutdown();
}

/// Forward logs distinguish requested alias vs resolved alias vs upstream model.
#[tokio::test]
async fn forward_logs_distinguish_requested_alias_and_upstream_model() {
    let harness = V2Harness::start_with_chat_success(&[GO_ACCOUNT_KEY]).await;
    let go = harness.create_go_account("go-main", GO_ACCOUNT_KEY).await;
    let (status, body) = harness.chat(GO_ALIAS).await;
    assert_eq!(status, StatusCode::OK, "{body}");

    let logs = harness.forward_logs().await;
    let item = logs["items"]
        .as_array()
        .and_then(|items| items.first())
        .unwrap_or_else(|| panic!("expected a forward log row: {logs}"));
    for field in [
        "requested_model",
        "resolved_alias",
        "upstream_model",
        "provider_id",
        "offering_id",
    ] {
        assert!(
            item.get(field).is_some() && !item[field].is_null(),
            "v2-contract: forward log missing {field}: {item}"
        );
    }
    assert_eq!(item["requested_model"].as_str(), Some(GO_ALIAS), "{item}");
    assert_eq!(item["resolved_alias"].as_str(), Some(GO_ALIAS), "{item}");
    assert_eq!(
        item["provider_id"].as_str(),
        Some(OPENCODE_PROVIDER_ID),
        "{item}"
    );
    assert_eq!(item["offering_id"].as_str(), Some(GO_OFFERING_ID), "{item}");
    assert_eq!(item["account_id"], go["id"]);
    assert_ne!(
        item["upstream_model"].as_str(),
        Some(""),
        "upstream_model must be the Plan's raw id: {item}"
    );
    harness.shutdown();
}

/// After the client has seen output, alias routing must not hop accounts.
#[tokio::test]
async fn alias_stream_does_not_cross_account_retry_after_output() {
    let harness = start_v2_with_disconnect_upstream().await;
    let first = harness.create_go_account("go-one", GO_ACCOUNT_KEY).await;
    let _second = harness.create_go_account("go-two", GO_ACCOUNT_KEY_2).await;

    let response = harness
        .client
        .post(harness.gateway("/v1/chat/completions"))
        .header(
            reqwest::header::AUTHORIZATION,
            format!("Bearer {GATEWAY_KEY}"),
        )
        .json(&json!({
            "model": GO_ALIAS,
            "messages": [{"role": "user", "content": "ping"}],
            "max_tokens": 3,
            "stream": true
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.text().await.unwrap();
    assert!(
        body.contains("ok") || body.contains("delta"),
        "client must have seen output before the disconnect: {body}"
    );
    assert_eq!(
        harness.disconnect_call_count(),
        1,
        "output already started; upstream must not be retried on another account"
    );

    let logs = harness.forward_logs().await;
    let items = logs["items"].as_array().cloned().unwrap_or_default();
    let account_ids: Vec<String> = items
        .iter()
        .filter_map(|item| item["account_id"].as_str().map(str::to_string))
        .collect();
    let unique: std::collections::HashSet<_> = account_ids.iter().cloned().collect();
    assert_eq!(
        unique.len(),
        1,
        "output already started; must not retry on another account: {items:?}"
    );
    assert_eq!(
        account_ids.first().map(String::as_str),
        first["id"].as_str()
    );
    harness.shutdown();
}
