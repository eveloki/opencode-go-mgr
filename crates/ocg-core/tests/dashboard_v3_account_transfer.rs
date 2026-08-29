//! Dashboard V3 encrypted account migration: step-up auth, secrecy, preview,
//! atomic import, duplicate handling, and lifecycle normalization.

use ocg_core::provider::{
    CUSTOM_API_OFFERING_ID, CUSTOM_PROVIDER_ID, OPENCODE_PROVIDER_ID, OPENCODE_ZEN_FREE_PROVIDER_ID,
};
use reqwest::header::CACHE_CONTROL;
use reqwest::{Method, StatusCode};
use serde_json::{Map, Value, json};
use std::time::Duration;

#[path = "fixtures/dashboard_v3/harness.rs"]
mod harness;

use harness::{V3Harness, start_loopback, start_public};

const ADMIN_PASSWORD: &str = "admin-password-123";
const BUNDLE_PASSWORD: &str = "migration-password-123";
const GO_KEY: &str = "sk-transfer-go";
const CUSTOM_KEY: &str = "custom-transfer-key";

fn cas(harness: &V3Harness, patch: Value) -> Value {
    let mut body = match patch {
        Value::Object(map) => map,
        _ => Map::new(),
    };
    body.insert(
        "expectedRevision".into(),
        json!(harness.state.settings_revision()),
    );
    body.insert(
        "processGeneration".into(),
        json!(harness.state.process_generation()),
    );
    Value::Object(body)
}

async fn send_json(
    harness: &V3Harness,
    method: Method,
    path: &str,
    body: &Value,
) -> (StatusCode, reqwest::header::HeaderMap, Value) {
    let response = harness
        .client
        .request(method, format!("{}{path}", harness.v3_base))
        .json(body)
        .send()
        .await
        .unwrap();
    let status = response.status();
    let headers = response.headers().clone();
    let body = response.json().await.unwrap_or(Value::Null);
    (status, headers, body)
}

fn assert_no_store(headers: &reqwest::header::HeaderMap) {
    assert_eq!(
        headers
            .get(CACHE_CONTROL)
            .and_then(|value| value.to_str().ok()),
        Some("no-store")
    );
}

async fn register_admin(harness: &V3Harness) {
    let (status, _, body) = send_json(
        harness,
        Method::POST,
        "/auth/register",
        &cas(
            harness,
            json!({ "username": "admin", "password": ADMIN_PASSWORD }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
}

async fn create_source_accounts(harness: &V3Harness) {
    let (status, _, body) = send_json(
        harness,
        Method::POST,
        "/accounts",
        &cas(harness, json!({ "name": "Migrated Go", "key": GO_KEY })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");

    let (status, _, body) = send_json(
        harness,
        Method::POST,
        "/accounts",
        &cas(
            harness,
            json!({
                "name": "Migrated Custom",
                "key": CUSTOM_KEY,
                "providerId": CUSTOM_PROVIDER_ID,
                "offeringId": CUSTOM_API_OFFERING_ID,
                "customConfig": {
                    "endpointUrl": "https://api.example.com/v1/messages",
                    "upstreamProtocol": "messages"
                },
                "modelCapabilities": [{
                    "modelId": "org/migrated-model",
                    "protocol": "messages"
                }]
            }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
}

#[tokio::test]
async fn encrypted_account_migration_moves_keys_without_exposing_them() {
    let source = start_loopback("account-transfer-source").await;

    let (status, headers, body) = send_json(
        &source,
        Method::POST,
        "/accounts/transfer/export",
        &json!({
            "adminUsername": "admin",
            "adminPassword": ADMIN_PASSWORD,
            "bundlePassword": BUNDLE_PASSWORD
        }),
    )
    .await;
    assert_eq!(status, StatusCode::PRECONDITION_FAILED, "{body}");
    assert_no_store(&headers);

    let oversized = source
        .client
        .post(format!("{}/accounts/transfer/preview", source.v3_base))
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .body("x".repeat(4 * 1024 * 1024 + 1))
        .send()
        .await
        .unwrap();
    assert_eq!(oversized.status(), StatusCode::PAYLOAD_TOO_LARGE);
    assert_no_store(oversized.headers());

    register_admin(&source).await;
    create_source_accounts(&source).await;

    let (status, headers, body) = send_json(
        &source,
        Method::POST,
        "/accounts/transfer/export",
        &json!({
            "adminUsername": "admin",
            "adminPassword": ADMIN_PASSWORD,
            "bundlePassword": BUNDLE_PASSWORD
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_no_store(&headers);
    assert_eq!(body["exportedAccounts"], 2);
    assert_eq!(body["skippedAccounts"], 1);
    let encoded = body.to_string();
    assert!(!encoded.contains(GO_KEY));
    assert!(!encoded.contains(CUSTOM_KEY));
    let bundle = body["bundle"].as_str().unwrap().to_string();

    let (status, headers, body) = send_json(
        &source,
        Method::POST,
        "/accounts/transfer/export",
        &json!({
            "adminUsername": "admin",
            "adminPassword": "wrong-admin-password",
            "bundlePassword": BUNDLE_PASSWORD
        }),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "{body}");
    assert_no_store(&headers);

    let target = start_loopback("account-transfer-target").await;
    let (status, headers, preview) = send_json(
        &target,
        Method::POST,
        "/accounts/transfer/preview",
        &json!({ "password": BUNDLE_PASSWORD, "bundle": bundle }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{preview}");
    assert_no_store(&headers);
    assert_eq!(preview["importableAccounts"], 2);
    assert_eq!(preview["duplicateAccounts"], 0);
    assert_eq!(preview["items"].as_array().unwrap().len(), 2);
    assert!(!preview.to_string().contains(GO_KEY));
    assert!(!preview.to_string().contains(CUSTOM_KEY));

    let preview_url = format!("{}/accounts/transfer/preview", target.v3_base);
    let first = target
        .client
        .post(&preview_url)
        .json(&json!({ "password": BUNDLE_PASSWORD, "bundle": bundle }));
    let second = target
        .client
        .post(&preview_url)
        .json(&json!({ "password": BUNDLE_PASSWORD, "bundle": bundle }));
    let (first, second) = tokio::join!(first.send(), second.send());
    let first = first.unwrap();
    let second = second.unwrap();
    assert_no_store(first.headers());
    assert_no_store(second.headers());
    let mut statuses = [first.status(), second.status()];
    statuses.sort();
    assert_eq!(statuses, [StatusCode::OK, StatusCode::SERVICE_UNAVAILABLE]);

    let stale_request = cas(
        &target,
        json!({ "password": BUNDLE_PASSWORD, "bundle": bundle }),
    );
    let client = target.client.clone();
    let import_url = format!("{}/accounts/transfer/import", target.v3_base);
    let stale_import = tokio::spawn(async move {
        client
            .post(import_url)
            .json(&stale_request)
            .send()
            .await
            .unwrap()
    });
    tokio::time::sleep(Duration::from_millis(100)).await;
    target.state.bump_settings_revision();
    let stale_import = stale_import.await.unwrap();
    assert_eq!(stale_import.status(), StatusCode::CONFLICT);
    assert_no_store(stale_import.headers());
    assert_eq!(target.state.db.lock().list_accounts().unwrap().len(), 1);

    let before = target.state.settings_revision();
    let (status, headers, imported) = send_json(
        &target,
        Method::POST,
        "/accounts/transfer/import",
        &cas(
            &target,
            json!({ "password": BUNDLE_PASSWORD, "bundle": bundle }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{imported}");
    assert_no_store(&headers);
    assert_eq!(imported["importedAccounts"], 2);
    assert_eq!(imported["duplicateAccounts"], 0);
    assert_eq!(target.state.settings_revision(), before + 1);
    assert!(!imported.to_string().contains(GO_KEY));
    assert!(!imported.to_string().contains(CUSTOM_KEY));

    let accounts = target.state.db.lock().list_accounts().unwrap();
    let migrated: Vec<_> = accounts
        .iter()
        .filter(|account| account.provider_id != OPENCODE_ZEN_FREE_PROVIDER_ID)
        .collect();
    assert_eq!(migrated.len(), 2);
    let go = migrated
        .iter()
        .find(|account| account.provider_id == OPENCODE_PROVIDER_ID)
        .unwrap();
    assert_eq!(target.state.decrypt_key(&go.key_cipher).unwrap(), GO_KEY);
    assert!(go.enabled);
    let custom = migrated
        .iter()
        .find(|account| account.provider_id == CUSTOM_PROVIDER_ID)
        .unwrap();
    assert_eq!(
        target.state.decrypt_key(&custom.key_cipher).unwrap(),
        CUSTOM_KEY
    );
    assert!(
        !custom.enabled,
        "Custom accounts must be re-verified after import"
    );
    let custom_contract = target
        .state
        .db
        .lock()
        .load_account_contract(&custom.id)
        .unwrap();
    assert_eq!(custom_contract.model_capabilities[0].source, "import");
    let (_, listed) = target
        .get_json(&format!("{}/accounts", target.v3_base))
        .await;
    let custom_view = listed["accounts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|account| account["providerId"] == CUSTOM_PROVIDER_ID)
        .unwrap();
    assert_eq!(custom_view["verificationStatus"], "pending");

    let revision = target.state.settings_revision();
    let (status, _, duplicate) = send_json(
        &target,
        Method::POST,
        "/accounts/transfer/import",
        &cas(
            &target,
            json!({ "password": BUNDLE_PASSWORD, "bundle": bundle }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{duplicate}");
    assert_eq!(duplicate["importedAccounts"], 0);
    assert_eq!(duplicate["duplicateAccounts"], 2);
    assert_eq!(target.state.settings_revision(), revision);

    let (status, headers, body) = send_json(
        &target,
        Method::POST,
        "/accounts/transfer/preview",
        &json!({ "password": "wrong-bundle-password", "bundle": bundle }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_no_store(&headers);
    assert!(!body.to_string().contains(GO_KEY));
    assert!(!body.to_string().contains(CUSTOM_KEY));

    let public = start_public("account-transfer-public").await;
    let unauthorized = public
        .client
        .post(format!("{}/accounts/transfer/preview", public.v3_base))
        .json(&json!({ "password": BUNDLE_PASSWORD, "bundle": bundle }))
        .send()
        .await
        .unwrap();
    assert_eq!(unauthorized.status(), StatusCode::UNAUTHORIZED);
    assert_no_store(unauthorized.headers());
    let registered = public
        .client
        .post(format!("{}/auth/register", public.v2_base))
        .json(&json!({ "username": "admin", "password": ADMIN_PASSWORD }))
        .send()
        .await
        .unwrap();
    assert_eq!(registered.status(), StatusCode::CREATED);
    let cookie = registered
        .headers()
        .get(reqwest::header::SET_COOKIE)
        .unwrap()
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_string();
    let export_body = json!({
        "adminUsername": "admin",
        "adminPassword": ADMIN_PASSWORD,
        "bundlePassword": BUNDLE_PASSWORD
    });
    let insecure = public
        .client
        .post(format!("{}/accounts/transfer/export", public.v3_base))
        .header(reqwest::header::COOKIE, &cookie)
        .json(&export_body)
        .send()
        .await
        .unwrap();
    assert_eq!(insecure.status(), StatusCode::FORBIDDEN);
    assert_no_store(insecure.headers());
    let spoofed_https = public
        .client
        .post(format!("{}/accounts/transfer/export", public.v3_base))
        .header(reqwest::header::COOKIE, cookie)
        .header("x-forwarded-proto", "https")
        .json(&export_body)
        .send()
        .await
        .unwrap();
    assert_eq!(spoofed_https.status(), StatusCode::FORBIDDEN);
    assert_no_store(spoofed_https.headers());

    source.stop();
    target.stop();
    public.stop();
}
