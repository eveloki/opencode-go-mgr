//! Official SCNet Token Plan contract hardening.
//!
//! Pins the 2026-08-21 usable-model snapshot and fail-closed lifecycle.
//! Live routing, GET /models, inference, and a Token Plan HTTP client stay
//! out of scope.

use chrono::Utc;
use ocg_core::alias::{self, ResolvedModel};
use ocg_core::crypto::{KeyCipher, StaticKeyCipher};
use ocg_core::gateway::selector::AccountSelector;
use ocg_core::models::{Account, AccountSetupStep, AccountType};
use ocg_core::provider::{
    SCNET_PROVIDER_ID, SCNET_RISK_ACKNOWLEDGEMENT_BODY, SCNET_RISK_ACKNOWLEDGEMENT_CONTENT_HASH,
    SCNET_RISK_ACKNOWLEDGEMENT_ID, SCNET_RISK_ACKNOWLEDGEMENT_SOURCE_URL,
    SCNET_RISK_ACKNOWLEDGEMENT_VERSION, SCNET_TOKEN_PLAN_ANTHROPIC_BASE_URL,
    SCNET_TOKEN_PLAN_CHAT_COMPLETIONS_PATH, SCNET_TOKEN_PLAN_DOCUMENTED_ENDPOINTS,
    SCNET_TOKEN_PLAN_EXCLUDED_PRICING_TABLE_OR_FAQ_MODELS, SCNET_TOKEN_PLAN_KEY_PREFIX,
    SCNET_TOKEN_PLAN_MESSAGES_PATH, SCNET_TOKEN_PLAN_MODEL_SNAPSHOT, SCNET_TOKEN_PLAN_MODEL_SOURCE,
    SCNET_TOKEN_PLAN_OFFERING_IDS, SCNET_TOKEN_PLAN_OPENAI_BASE_URL,
    SCNET_TOKEN_PLAN_USABLE_MODELS, UpstreamAuthScheme, acknowledgement_content_hash,
    scnet_token_plan_official_offering_name,
};
use reqwest::StatusCode;
use serde_json::{Value, json};
use std::sync::Arc;

#[path = "fixtures/v2/harness.rs"]
mod harness;

use harness::*;

const SNAPSHOT_FIXTURE: &str =
    include_str!("fixtures/scnet/token_plan_usable_models_2026-08-21.json");

fn snapshot_fixture() -> Value {
    serde_json::from_str(SNAPSHOT_FIXTURE).expect("SCNet snapshot fixture must parse")
}

fn fixture_models(snapshot: &Value) -> Vec<&str> {
    snapshot["upstream_models"]
        .as_array()
        .expect("fixture upstream_models")
        .iter()
        .map(|item| item.as_str().expect("model id string"))
        .collect()
}

fn fixture_extras(snapshot: &Value) -> Vec<&str> {
    snapshot["excluded_pricing_table_or_faq_extras"]
        .as_array()
        .expect("fixture extras")
        .iter()
        .map(|item| item.as_str().expect("extra model id string"))
        .collect()
}

fn resolved_mappings(requested: &str) -> Vec<ocg_core::alias::ProviderMapping> {
    match alias::resolve(requested) {
        Ok(ResolvedModel::Alias { mappings, .. }) => mappings,
        Ok(ResolvedModel::PinnedRaw { mapping, .. }) => vec![mapping],
        Err(_) => Vec::new(),
    }
}

fn scnet_account(id: &str, offering_id: &str, enabled: bool) -> Account {
    let cipher: Arc<dyn KeyCipher + Send + Sync> = Arc::new(StaticKeyCipher::new("scnet-contract"));
    Account {
        id: id.into(),
        provider_id: SCNET_PROVIDER_ID.into(),
        offering_id: offering_id.into(),
        credential_kind: ocg_core::provider::CredentialKind::ApiKey,
        quota_scope: ocg_core::provider::QuotaScope::Key,
        name: id.into(),
        username: None,
        password_cipher: None,
        key_cipher: cipher.encrypt("sk-tp-forced-enabled").unwrap(),
        enabled,
        account_type: AccountType::Key,
        setup_step: AccountSetupStep::Ready,
        referral_code: None,
        purchase_date: String::new(),
        expires_on: String::new(),
        cooldown_until: None,
        cooldown_generic_until: None,
        cooldown_5h_until: None,
        cooldown_week_until: None,
        cooldown_month_until: None,
        cooldown_free_until: None,
        last_error: None,
        auth_error: None,
        notes: None,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    }
}

#[test]
fn official_snapshot_matches_fixture_and_is_shared_across_tiers() {
    let snapshot = snapshot_fixture();
    let models = fixture_models(&snapshot);
    let extras = fixture_extras(&snapshot);

    assert_eq!(snapshot["source"], SCNET_TOKEN_PLAN_MODEL_SOURCE);
    assert_eq!(snapshot["version"], SCNET_TOKEN_PLAN_MODEL_SNAPSHOT.version);
    assert_eq!(
        snapshot["source_url"],
        SCNET_TOKEN_PLAN_MODEL_SNAPSHOT.source_url
    );
    assert_eq!(models, SCNET_TOKEN_PLAN_USABLE_MODELS);
    assert_eq!(
        extras,
        SCNET_TOKEN_PLAN_EXCLUDED_PRICING_TABLE_OR_FAQ_MODELS.to_vec()
    );
    assert_eq!(snapshot["key_prefix"], SCNET_TOKEN_PLAN_KEY_PREFIX);
    assert_eq!(snapshot["auth_scheme"], "bearer");
    assert_eq!(
        snapshot["openai_base_url"],
        SCNET_TOKEN_PLAN_OPENAI_BASE_URL
    );
    assert_eq!(
        snapshot["anthropic_base_url"],
        SCNET_TOKEN_PLAN_ANTHROPIC_BASE_URL
    );
    assert_eq!(
        snapshot["openai_chat_completions_path"],
        SCNET_TOKEN_PLAN_CHAT_COMPLETIONS_PATH
    );
    assert_eq!(
        snapshot["anthropic_messages_path"],
        SCNET_TOKEN_PLAN_MESSAGES_PATH
    );
    assert_eq!(
        SCNET_TOKEN_PLAN_DOCUMENTED_ENDPOINTS.auth_scheme,
        UpstreamAuthScheme::Bearer
    );

    let offerings = snapshot["offerings"].as_array().expect("offerings");
    assert_eq!(offerings.len(), SCNET_TOKEN_PLAN_OFFERING_IDS.len());
    for (entry, offering_id) in offerings.iter().zip(SCNET_TOKEN_PLAN_OFFERING_IDS) {
        assert_eq!(entry["offering_id"], offering_id);
        assert_eq!(
            entry["official_name"],
            scnet_token_plan_official_offering_name(offering_id).unwrap()
        );
    }
    assert!(std::ptr::eq(
        SCNET_TOKEN_PLAN_MODEL_SNAPSHOT.upstream_models,
        SCNET_TOKEN_PLAN_USABLE_MODELS
    ));
}

#[test]
fn risk_notice_id_version_body_source_and_hash_stay_stable() {
    let snapshot = snapshot_fixture();
    let notice = &snapshot["risk_notice"];
    assert_eq!(notice["acknowledgement_id"], SCNET_RISK_ACKNOWLEDGEMENT_ID);
    assert_eq!(notice["version"], SCNET_RISK_ACKNOWLEDGEMENT_VERSION);
    assert_eq!(notice["source_url"], SCNET_RISK_ACKNOWLEDGEMENT_SOURCE_URL);
    assert_eq!(notice["body"], SCNET_RISK_ACKNOWLEDGEMENT_BODY);
    assert_eq!(
        notice["content_hash"],
        SCNET_RISK_ACKNOWLEDGEMENT_CONTENT_HASH
    );
    assert_eq!(
        acknowledgement_content_hash(SCNET_RISK_ACKNOWLEDGEMENT_BODY),
        SCNET_RISK_ACKNOWLEDGEMENT_CONTENT_HASH
    );
}

#[tokio::test]
async fn catalog_identifies_snapshot_with_empty_aliases() {
    let harness = V2Harness::start().await;
    let catalog = harness.catalog().await;
    for offering_id in SCNET_TOKEN_PLAN_OFFERING_IDS {
        let entry = catalog_entry(&catalog, SCNET_PROVIDER_ID, offering_id)
            .unwrap_or_else(|| panic!("catalog must include scnet/{offering_id}"));
        assert_eq!(entry["model_source"], SCNET_TOKEN_PLAN_MODEL_SOURCE);
        assert_eq!(entry["routable"], false);
        assert_eq!(entry["verification_runtime_availability"], "unavailable");
        assert_eq!(entry["pricing_availability"], "unavailable");
        assert_eq!(entry["usage_availability"], "unavailable");
        assert_eq!(entry["key_prefix"], SCNET_TOKEN_PLAN_KEY_PREFIX);
        assert_eq!(entry["auth_schemes"], json!(["bearer"]));
        assert!(
            alias_names(entry).is_empty(),
            "unroutable Token Plans must not publish aliases: {entry}"
        );
        let notice = &entry["risk_notice"];
        assert_eq!(notice["acknowledgement_id"], SCNET_RISK_ACKNOWLEDGEMENT_ID);
        assert_eq!(notice["version"], SCNET_RISK_ACKNOWLEDGEMENT_VERSION);
        assert_eq!(notice["source_url"], SCNET_RISK_ACKNOWLEDGEMENT_SOURCE_URL);
        assert_eq!(notice["body"], SCNET_RISK_ACKNOWLEDGEMENT_BODY);
        assert_eq!(
            notice["content_hash"],
            SCNET_RISK_ACKNOWLEDGEMENT_CONTENT_HASH
        );
    }
    harness.shutdown();
}

#[tokio::test]
async fn create_stays_disabled_pending_and_lifecycle_fail_closed() {
    let harness = V2Harness::start_with_chat_success(&[SCNET_ACCOUNT_KEY]).await;
    let catalog = harness.catalog().await;

    for offering_id in SCNET_TOKEN_PLAN_OFFERING_IDS {
        let entry = catalog_entry(&catalog, SCNET_PROVIDER_ID, offering_id).unwrap();
        let notice = &entry["risk_notice"];
        let (status, body) = harness
            .create_account(json!({
                "provider_id": SCNET_PROVIDER_ID,
                "offering_id": offering_id,
                "name": format!("scnet-{offering_id}"),
                "key": "sk-not-token-plan",
                "expected_revision": harness.settings_revision().await,
                "acknowledgements": matching_acknowledgements(notice)
            }))
            .await;
        assert_eq!(
            status,
            StatusCode::BAD_REQUEST,
            "wrong prefix must fail closed: {body}"
        );

        let (status, draft) = harness
            .create_account(json!({
                "provider_id": SCNET_PROVIDER_ID,
                "offering_id": offering_id,
                "name": format!("scnet-{offering_id}"),
                "key": SCNET_ACCOUNT_KEY,
                "expected_revision": harness.settings_revision().await,
                "acknowledgements": matching_acknowledgements(notice)
            }))
            .await;
        assert_eq!(status, StatusCode::OK, "{draft}");
        assert_eq!(draft["enabled"], false, "{draft}");
        assert_eq!(draft["verification_status"], "pending", "{draft}");
        assert_eq!(draft["plan_routable"], false, "{draft}");
        let id = draft["id"].as_str().expect("draft id").to_string();

        let (status, body) = harness
            .post_json(
                &format!("/accounts/{id}/toggle"),
                &json!({ "expected_revision": harness.settings_revision().await }),
            )
            .await;
        assert_eq!(
            status,
            StatusCode::CONFLICT,
            "enable must fail closed while unroutable: {body}"
        );

        let (status, body) = harness
            .post_json(
                &format!("/accounts/{id}/verify"),
                &json!({ "expected_revision": harness.settings_revision().await }),
            )
            .await;
        assert_eq!(
            status,
            StatusCode::NOT_IMPLEMENTED,
            "verify must stay 501: {body}"
        );

        let (status, body) = harness
            .post_json(&format!("/accounts/{id}/test"), &json!({}))
            .await;
        assert_eq!(
            status,
            StatusCode::CONFLICT,
            "test must fail closed without a live Token Plan client: {body}"
        );

        let (status, usage) = harness
            .get_json(&format!("/providers/accounts/{id}/usage"))
            .await;
        assert_eq!(status, StatusCode::OK, "{usage}");
        assert_eq!(usage["availability"], "unavailable", "{usage}");
        assert!(
            usage["quota_windows"]
                .as_array()
                .is_some_and(|windows| windows.is_empty()),
            "Token Plan must not invent 5h/week windows: {usage}"
        );

        let (status, pricing) = harness
            .get_json(&format!(
                "/providers/{SCNET_PROVIDER_ID}/{offering_id}/pricing"
            ))
            .await;
        assert_eq!(status, StatusCode::OK, "{pricing}");
        assert_eq!(pricing["availability"], "unavailable", "{pricing}");

        let stored = harness.account_by_id(&id).await;
        assert_eq!(stored["enabled"], false, "{stored}");
        assert_eq!(stored["verification_status"], "pending", "{stored}");
        assert!(
            !json_contains_secret(&stored, SCNET_ACCOUNT_KEY),
            "lifecycle JSON leaked the Key: {stored}"
        );
    }

    assert!(
        harness.fake_call_keys().is_empty(),
        "Token Plan lifecycle must not call upstream: {:?}",
        harness.fake_call_keys()
    );
    harness.shutdown();
}

#[tokio::test]
async fn alias_adapter_and_selector_do_not_expose_token_plans() {
    for model in SCNET_TOKEN_PLAN_USABLE_MODELS
        .iter()
        .chain(SCNET_TOKEN_PLAN_EXCLUDED_PRICING_TABLE_OR_FAQ_MODELS)
    {
        for mapping in resolved_mappings(model) {
            assert_ne!(
                mapping.provider_id, SCNET_PROVIDER_ID,
                "{model} must not resolve to an SCNet mapping: {mapping:?}"
            );
        }
        assert!(
            !alias::published_aliases()
                .iter()
                .any(|alias| alias == model),
            "official Token Plan spelling `{model}` must not be a published alias"
        );
    }
    assert!(
        !alias::published_aliases()
            .iter()
            .any(|alias| alias.contains("scnet"))
    );

    for offering_id in SCNET_TOKEN_PLAN_OFFERING_IDS {
        let forced = scnet_account("scnet-forced", offering_id, true);
        assert!(
            !AccountSelector::is_available(&forced, &[]),
            "selector must ignore SCNet even if enabled is forced: {offering_id}"
        );
    }

    let harness = V2Harness::start_with_chat_success(&[SCNET_ACCOUNT_KEY]).await;
    let catalog = harness.catalog().await;
    let standard = catalog_entry(
        &catalog,
        SCNET_PROVIDER_ID,
        ocg_core::provider::SCNET_TOKEN_PLAN_STANDARD_OFFERING_ID,
    )
    .unwrap();
    let (status, draft) = harness
        .create_account(json!({
            "provider_id": SCNET_PROVIDER_ID,
            "offering_id": ocg_core::provider::SCNET_TOKEN_PLAN_STANDARD_OFFERING_ID,
            "name": "scnet-unroutable",
            "key": SCNET_ACCOUNT_KEY,
            "expected_revision": harness.settings_revision().await,
            "acknowledgements": matching_acknowledgements(&standard["risk_notice"])
        }))
        .await;
    assert_eq!(status, StatusCode::OK, "{draft}");

    let (status, body) = harness.chat("GLM-5.2").await;
    assert_ne!(
        status,
        StatusCode::OK,
        "SCNet draft must not serve GLM-5.2: {body}"
    );
    let (status, body) = harness.chat("Qwen3-235B-A22B").await;
    assert_ne!(
        status,
        StatusCode::OK,
        "excluded FAQ extra must not route: {body}"
    );
    assert!(
        !harness
            .fake_call_keys()
            .iter()
            .any(|key| key == SCNET_ACCOUNT_KEY),
        "adapter must not send the Token Plan key: {:?}",
        harness.fake_call_keys()
    );

    let (status, models) = harness.list_client_models().await;
    if status == StatusCode::OK {
        let ids = client_model_ids(&models);
        for model in SCNET_TOKEN_PLAN_USABLE_MODELS {
            assert!(
                !ids.iter().any(|id| id == model),
                "client /v1/models must not advertise official Token Plan spelling `{model}`"
            );
        }
        assert!(
            !ids.iter().any(|id| id.contains("scnet")),
            "client /v1/models leaked an SCNet id: {ids:?}"
        );
    }
    harness.shutdown();
}
