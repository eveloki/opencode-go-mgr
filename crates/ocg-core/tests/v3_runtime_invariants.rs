//! V3 characterization: request-entry snapshots, per-fallback re-reads,
//! stream finalization, Go `success_no_usage`, Custom 401 rotation, and
//! deferred post-429 usage sync.
//!
//! Existing suites already freeze retry/SSE/redaction/model-list/OpenCode 401
//! passthrough and the proxy `ForwardRouteSet` snapshot. This file covers the
//! gaps a GatewayExecutor refactor can otherwise redefine.

use axum::http::StatusCode;
use chrono::{Duration as ChronoDuration, Utc};
use ocg_core::gateway;
use ocg_core::models::UsageWindowKind;
use ocg_core::provider::{
    CUSTOM_API_OFFERING_ID, CUSTOM_PROVIDER_ID, OPENCODE_PROVIDER_ID, UpstreamProtocolKind,
    ZEN_FREE_ACCOUNT_ID,
};
use ocg_core::provider_contracts::ContractScope;
use ocg_core::usage_sync::INFERENCE_429_DELAY_MIN;
use ocg_core::zen_models::{ZEN_MODELS_SOURCE_URL, ZenFreeModelCatalog};
use serde_json::json;
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

#[path = "fixtures/v3/harness.rs"]
mod harness;

use harness::*;

const GO_MODEL: &str = "deepseek-v4-flash";
const ZEN_ONLY_MODEL: &str = "deepseek-v4-flash-free";
const SHARED_ALIAS: &str = "mimo-v2.5";
const CUSTOM_MODEL: &str = "custom-v3-model";
const CUSTOM_KEY_A: &str = "v3-custom-key-a";
const CUSTOM_KEY_B: &str = "v3-custom-key-b";
const CUSTOM_KEY_C: &str = "v3-custom-key-c";

fn disable_all_go_protocols(state: &Arc<ocg_core::state::CoreStateInner>) {
    let now = Utc::now();
    let scope = ContractScope::provider(OPENCODE_PROVIDER_ID);
    {
        let db = state.db.lock();
        for protocol in [
            UpstreamProtocolKind::ChatCompletions,
            UpstreamProtocolKind::Responses,
            UpstreamProtocolKind::Messages,
        ] {
            db.set_protocol_switch(&scope, protocol, false, now)
                .unwrap();
        }
    }
    state.reload_provider_contracts().unwrap();
}

fn inflate_active_pricing(state: &Arc<ocg_core::state::CoreStateInner>, model_id: &str) -> String {
    let mut snapshot = (*state.pricing_snapshot()).clone();
    let mut found = false;
    for model in &mut snapshot.models {
        if model.model_id == model_id {
            model.quota_multiplier *= 100.0;
            found = true;
        }
    }
    assert!(
        found,
        "priced model {model_id} must exist in the seed snapshot"
    );
    snapshot.revision = format!("v3-inflated-{}", uuid::Uuid::new_v4());
    snapshot.activated_at = Utc::now().to_rfc3339();
    let revision = snapshot.revision.clone();
    state.activate_pricing_snapshot(snapshot).unwrap();
    revision
}

fn go_state_with_keys(keys: &[&str]) -> (Arc<ocg_core::state::CoreStateInner>, std::path::PathBuf) {
    build_go_state("http://127.0.0.1:1".into(), keys)
}

#[tokio::test]
async fn entry_pricing_snapshot_survives_midflight_activation() {
    let (state, dir) = go_state_with_keys(&["key-1", "key-2"]);
    let captured_revision = state.pricing_snapshot().revision.clone();
    let expected_cost = state.estimate_cost(GO_MODEL, 10, 2, 0, 0, None).quota_debit;

    let state_for_cb = state.clone();
    let captured_for_cb = captured_revision.clone();
    let (base_url, calls, stop) = start_scripted_upstream(
        vec![
            ScriptedReply {
                status: 403,
                body: FORBIDDEN_BODY,
            },
            ScriptedReply {
                status: 200,
                body: SUCCESS_BODY,
            },
        ],
        Arc::new(move |index| {
            if index == 0 {
                let inflated = inflate_active_pricing(&state_for_cb, GO_MODEL);
                assert_ne!(inflated, captured_for_cb);
            }
        }),
    )
    .await;
    let mut config = state.config();
    config.upstream_base_url = base_url;
    state.set_config(config).unwrap();

    let (port, gateway_handle) = start_gateway(state.clone()).await;
    let (status, body) = chat(port, GO_MODEL).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(calls.load(Ordering::SeqCst), 2);

    let logs = state.db.lock().list_forward_logs(10).unwrap();
    let success = logs
        .iter()
        .find(|log| log.status.starts_with("success"))
        .expect("fallback success row");
    assert_eq!(
        success.pricing_revision_id.as_deref(),
        Some(captured_revision.as_str()),
        "in-flight fallback must keep the entry pricing revision"
    );
    assert_eq!(success.quota_debit, expected_cost);
    assert_ne!(
        state.pricing_snapshot().revision,
        captured_revision,
        "live pricing must have flipped after the first attempt"
    );

    gateway::stop_gateway(gateway_handle);
    let _ = stop.send(());
    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn entry_contracts_snapshot_survives_midflight_protocol_disable() {
    let (state, dir) = go_state_with_keys(&["key-1", "key-2"]);
    let state_for_cb = state.clone();
    let (base_url, calls, stop) = start_scripted_upstream(
        vec![
            ScriptedReply {
                status: 403,
                body: FORBIDDEN_BODY,
            },
            ScriptedReply {
                status: 200,
                body: SUCCESS_BODY,
            },
        ],
        Arc::new(move |index| {
            if index == 0 {
                disable_all_go_protocols(&state_for_cb);
            }
        }),
    )
    .await;
    let mut config = state.config();
    config.upstream_base_url = base_url;
    state.set_config(config).unwrap();

    let (port, gateway_handle) = start_gateway(state.clone()).await;
    let (status, body) = chat(port, GO_MODEL).await;
    assert_eq!(
        status,
        StatusCode::OK,
        "entry contract snapshot must still allow Chat: {body}"
    );
    assert_eq!(calls.load(Ordering::SeqCst), 2);

    let (status, body) = chat(port, GO_MODEL).await;
    assert_ne!(
        status,
        StatusCode::OK,
        "a later request must observe the disabled protocols: {body}"
    );
    assert_eq!(
        calls.load(Ordering::SeqCst),
        2,
        "the follow-up request must fail locally without another upstream call"
    );

    gateway::stop_gateway(gateway_handle);
    let _ = stop.send(());
    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn entry_alias_snapshot_survives_midflight_zen_catalog_replace() {
    let (state, dir) = go_state_with_keys(&["key-1"]);
    state
        .db
        .lock()
        .reorder_accounts(&[ZEN_FREE_ACCOUNT_ID.into(), "acct-1".into()])
        .unwrap();

    let state_for_cb = state.clone();
    let (base_url, calls, stop) = start_scripted_upstream(
        vec![ScriptedReply {
            status: 429,
            body: LIMITED_BODY,
        }],
        Arc::new(move |index| {
            if index == 0 {
                state_for_cb
                    .activate_zen_free_model_catalog(ZenFreeModelCatalog {
                        models: vec!["hy3-free".into()],
                        refreshed_at: Some(Utc::now()),
                        source_url: ZEN_MODELS_SOURCE_URL.into(),
                    })
                    .unwrap();
            }
        }),
    )
    .await;
    let mut config = state.config();
    config.upstream_base_url = format!("{base_url}/zen/go");
    state.set_config(config).unwrap();

    let (port, gateway_handle) = start_gateway(state.clone()).await;
    let (status, body) = chat(port, ZEN_ONLY_MODEL).await;
    assert_eq!(
        status,
        StatusCode::TOO_MANY_REQUESTS,
        "in-flight zen-only resolve must not become unknown_model: {body}"
    );
    assert_eq!(calls.load(Ordering::SeqCst), 1);

    let (status, body) = chat(port, ZEN_ONLY_MODEL).await;
    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "a later request must re-resolve against the replaced catalog: {body}"
    );
    assert!(
        body.contains("unknown model"),
        "replaced catalog must drop the zen-only alias: {body}"
    );
    assert_eq!(calls.load(Ordering::SeqCst), 1);

    gateway::stop_gateway(gateway_handle);
    let _ = stop.send(());
    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn fallback_rereads_accounts_and_skips_a_card_disabled_mid_request() {
    let (state, dir) = go_state_with_keys(&["key-1", "key-2", "key-3"]);
    let state_for_cb = state.clone();
    let (base_url, calls, stop) = start_scripted_upstream(
        vec![
            ScriptedReply {
                status: 403,
                body: FORBIDDEN_BODY,
            },
            ScriptedReply {
                status: 200,
                body: SUCCESS_BODY,
            },
        ],
        Arc::new(move |index| {
            if index == 0 {
                set_account_enabled(&state_for_cb, "acct-2", false);
            }
        }),
    )
    .await;
    let mut config = state.config();
    config.upstream_base_url = base_url;
    state.set_config(config).unwrap();

    let (port, gateway_handle) = start_gateway(state.clone()).await;
    let (status, body) = chat(port, GO_MODEL).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(calls.load(Ordering::SeqCst), 2);

    let mut logs = state.db.lock().list_forward_logs(10).unwrap();
    logs.sort_by_key(|log| log.attempt);
    assert_eq!(logs.len(), 2, "{logs:?}");
    assert_eq!(logs[0].account_id, "acct-1");
    assert_eq!(logs[0].attempt, Some(1));
    assert_eq!(logs[1].account_id, "acct-3");
    assert_eq!(logs[1].attempt, Some(2));
    assert!(
        logs.iter().all(|log| log.account_id != "acct-2"),
        "disabled card must be skipped after the per-fallback account re-read: {logs:?}"
    );

    gateway::stop_gateway(gateway_handle);
    let _ = stop.send(());
    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn fallback_rereads_free_cooldown_and_skips_zen() {
    let (state, dir) = go_state_with_keys(&["key-1"]);
    state
        .db
        .lock()
        .reorder_accounts(&["acct-1".into(), ZEN_FREE_ACCOUNT_ID.into()])
        .unwrap();

    let state_for_cb = state.clone();
    let (base_url, calls, stop) = start_scripted_upstream(
        vec![
            ScriptedReply {
                status: 403,
                body: FORBIDDEN_BODY,
            },
            ScriptedReply {
                status: 200,
                body: SUCCESS_BODY,
            },
        ],
        Arc::new(move |index| {
            if index == 0 {
                let until = Utc::now() + ChronoDuration::minutes(30);
                state_for_cb
                    .db
                    .lock()
                    .set_account_rate_limit(
                        ZEN_FREE_ACCOUNT_ID,
                        until,
                        "free cooldown written mid-request",
                        Some(UsageWindowKind::Free),
                    )
                    .unwrap();
            }
        }),
    )
    .await;
    let mut config = state.config();
    config.upstream_base_url = format!("{base_url}/zen/go");
    state.set_config(config).unwrap();

    let (port, gateway_handle) = start_gateway(state.clone()).await;
    let (status, body) = chat(port, SHARED_ALIAS).await;
    assert_ne!(
        status,
        StatusCode::OK,
        "Zen must not be selected after a mid-request free cooldown re-read: {body}"
    );
    assert_eq!(
        calls.load(Ordering::SeqCst),
        1,
        "only the rejected Go attempt should have reached upstream"
    );

    gateway::stop_gateway(gateway_handle);
    let _ = stop.send(());
    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn go_success_without_usage_is_success_no_usage_for_non_stream_and_stream() {
    let replies = HashMap::from([(
        "key-1".to_string(),
        VecDeque::from([
            FakeReply {
                status: 200,
                body: SUCCESS_BODY_WITHOUT_USAGE,
            },
            FakeReply {
                status: 200,
                body: CHAT_STREAM_WITHOUT_USAGE,
            },
        ]),
    )]);
    let (base_url, _calls, stop) = start_fake_upstream(replies).await;
    let (state, dir) = build_go_state(base_url, &["key-1"]);
    let (port, gateway_handle) = start_gateway(state.clone()).await;

    let (status, body) = chat(port, GO_MODEL).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let (status, body) = chat_stream(port, GO_MODEL).await;
    assert_eq!(status, StatusCode::OK, "{body}");

    let mut logs = state.db.lock().list_forward_logs(10).unwrap();
    logs.sort_by_key(|log| log.id);
    assert_eq!(logs.len(), 2, "{logs:?}");
    for log in &logs {
        assert_eq!(log.status, "success_no_usage", "{log:?}");
        assert_eq!(log.cost_state, "usage_missing", "{log:?}");
        assert!(log.cost.is_none(), "{log:?}");
        assert!(log.quota_debit.is_none(), "{log:?}");
        assert_eq!((log.prompt_tokens, log.completion_tokens), (0, 0));
        assert_eq!(log.account_id, "acct-1");
        assert_eq!(log.attempt, Some(1));
    }

    gateway::stop_gateway(gateway_handle);
    let _ = stop.send(());
    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn go_inference_429_schedules_deferred_usage_sync_without_an_inline_fetch() {
    let replies = HashMap::from([(
        "key-1".to_string(),
        VecDeque::from([FakeReply {
            status: 429,
            body: LIMITED_BODY,
        }]),
    )]);
    let (base_url, _calls, stop) = start_fake_upstream(replies).await;
    let (state, dir) = build_go_state(base_url, &["key-1"]);
    let now = chrono::DateTime::parse_from_rfc3339("2026-08-18T12:00:00Z")
        .unwrap()
        .with_timezone(&Utc);
    state.usage_sync.set_clock_for_test(move || now);
    state.usage_sync.set_jitter_for_test(|| 0.0);
    let fetches = Arc::new(AtomicUsize::new(0));
    let fetches_cb = fetches.clone();
    state.usage_sync.set_fetch_for_test(move |_cfg, _key| {
        fetches_cb.fetch_add(1, Ordering::SeqCst);
        Box::pin(async { Err(ocg_core::go_usage::GoUsageError::Network) })
    });
    state
        .db
        .lock()
        .record_account_usage_sync_success(
            "acct-1",
            now - ChronoDuration::hours(1),
            now + ChronoDuration::hours(20),
            false,
        )
        .unwrap();

    let (port, gateway_handle) = start_gateway(state.clone()).await;
    let (status, body) = chat(port, GO_MODEL).await;
    assert_eq!(
        status,
        StatusCode::TOO_MANY_REQUESTS,
        "a lone Go 429 still rotates then returns the soonest reset: {body}"
    );
    assert_eq!(fetches.load(Ordering::SeqCst), 0);

    let sync = state
        .db
        .lock()
        .account_usage_sync_state("acct-1")
        .unwrap()
        .unwrap();
    assert_eq!(sync.next_eligible_at, Some(now + INFERENCE_429_DELAY_MIN));

    state.usage_sync.clear_test_seams();
    gateway::stop_gateway(gateway_handle);
    let _ = stop.send(());
    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn custom_401_rotates_persists_auth_error_and_skips_a_runtime_disabled_mid_request() {
    let (state, dir) = go_state_with_keys(&[]);
    let disable_id: Arc<std::sync::Mutex<Option<String>>> = Arc::new(std::sync::Mutex::new(None));
    let disable_id_cb = disable_id.clone();
    let state_for_cb = state.clone();
    let (origin, calls, stop) = start_scripted_upstream(
        vec![
            ScriptedReply {
                status: 200,
                body: SUCCESS_BODY,
            },
            ScriptedReply {
                status: 200,
                body: SUCCESS_BODY,
            },
            ScriptedReply {
                status: 200,
                body: SUCCESS_BODY,
            },
            ScriptedReply {
                status: 401,
                body: r#"{"error":{"message":"expired custom key"}}"#,
            },
            ScriptedReply {
                status: 200,
                body: SUCCESS_BODY,
            },
        ],
        Arc::new(move |index| {
            if index == 3
                && let Some(id) = disable_id_cb.lock().unwrap().clone()
            {
                set_account_enabled(&state_for_cb, &id, false);
            }
        }),
    )
    .await;
    let (port, gateway_handle) = start_gateway(state.clone()).await;

    let first = create_verified_custom(port, &state, "custom-a", CUSTOM_KEY_A, &origin).await;
    let second = create_verified_custom(port, &state, "custom-b", CUSTOM_KEY_B, &origin).await;
    let third = create_verified_custom(port, &state, "custom-c", CUSTOM_KEY_C, &origin).await;
    *disable_id.lock().unwrap() = Some(second.clone());

    let (status, body) = chat(port, CUSTOM_MODEL).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(
        calls.load(Ordering::SeqCst) >= 5,
        "verify (3) + 401 + fallback must have happened: {}",
        calls.load(Ordering::SeqCst)
    );

    let after_first = state.db.lock().get_account(&first).unwrap().unwrap();
    assert!(
        after_first.auth_error.is_some(),
        "ordinary Custom 401 must persist auth_error: {after_first:?}"
    );
    let after_second = state.db.lock().get_account(&second).unwrap().unwrap();
    assert!(!after_second.enabled, "{after_second:?}");
    assert!(after_second.auth_error.is_none(), "{after_second:?}");

    let logs = state.db.lock().list_forward_logs(20).unwrap();
    assert!(
        logs.iter()
            .any(|log| log.account_id == first && log.http_status == Some(401)),
        "{logs:?}"
    );
    assert!(
        logs.iter()
            .any(|log| log.account_id == third && log.status.starts_with("success")),
        "third Custom runtime must be selected after the mid-request disable: {logs:?}"
    );
    assert!(
        logs.iter().all(|log| log.account_id != second),
        "disabled Custom runtime must not be attempted: {logs:?}"
    );

    gateway::stop_gateway(gateway_handle);
    let _ = stop.send(());
    let _ = std::fs::remove_dir_all(dir);
}

async fn dashboard_json(
    port: u16,
    method: reqwest::Method,
    path: &str,
    body: serde_json::Value,
) -> (StatusCode, serde_json::Value) {
    let response = loopback_client()
        .request(
            method,
            format!("http://127.0.0.1:{port}/dashboard/api{path}"),
        )
        .json(&body)
        .send()
        .await
        .unwrap();
    let status = response.status();
    let body = response.json().await.unwrap();
    (status, body)
}

async fn create_verified_custom(
    port: u16,
    state: &Arc<ocg_core::state::CoreStateInner>,
    name: &str,
    key: &str,
    origin: &str,
) -> String {
    let (status, draft) = dashboard_json(
        port,
        reqwest::Method::POST,
        "/accounts",
        json!({
            "provider_id": CUSTOM_PROVIDER_ID,
            "offering_id": CUSTOM_API_OFFERING_ID,
            "name": name,
            "key": key,
            "expected_revision": state.settings_revision(),
            "custom_config": {
                "base_url": origin,
                "upstream_protocol": "chat_completions",
                "auth_scheme": "bearer"
            },
            "model_capabilities": [{
                "model_id": CUSTOM_MODEL,
                "protocol": "chat_completions"
            }]
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{draft}");
    let id = draft["id"].as_str().unwrap().to_string();
    let (status, verified) = dashboard_json(
        port,
        reqwest::Method::POST,
        &format!("/accounts/{id}/verify"),
        json!({ "expected_revision": state.settings_revision() }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{verified}");
    assert_eq!(verified["verification_status"].as_str(), Some("verified"));
    let (status, enabled) = dashboard_json(
        port,
        reqwest::Method::POST,
        &format!("/accounts/{id}/toggle"),
        json!({ "expected_revision": state.settings_revision() }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{enabled}");
    assert_eq!(enabled["enabled"], true);
    id
}
