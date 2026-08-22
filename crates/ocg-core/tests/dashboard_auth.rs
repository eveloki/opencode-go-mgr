use chrono::Utc;
use ocg_core::browser::browser_profile_paths;
use ocg_core::crypto::{KeyCipher, StaticKeyCipher};
use ocg_core::db::Database;
use ocg_core::gateway;
use ocg_core::models::{AppConfig, ForwardLog, RoutingMode};
use ocg_core::provider::{
    COMMAND_CODE_PROVIDER_ID, GOAT_OFFERING_ID, OPENCODE_PROVIDER_ID, ZEN_FREE_ACCOUNT_ID,
};
use ocg_core::state::CoreStateInner;
use reqwest::StatusCode;
use serde_json::json;
use std::fs;
use std::net::SocketAddr;
use std::sync::{
    Arc, Mutex as StdMutex,
    atomic::{AtomicBool, Ordering},
};

static AUTO_START_SYNCED: AtomicBool = AtomicBool::new(false);
static AUTO_START_FAIL: AtomicBool = AtomicBool::new(false);

fn test_auto_start_sync(enabled: bool) -> anyhow::Result<()> {
    if enabled && AUTO_START_FAIL.load(Ordering::Relaxed) {
        anyhow::bail!("test auto-start failure");
    }
    AUTO_START_SYNCED.store(enabled, Ordering::Relaxed);
    Ok(())
}

fn state(label: &str) -> Arc<CoreStateInner> {
    let mut dir = std::env::temp_dir();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    dir.push(format!("ocg-auth-test-{label}-{nanos}"));
    fs::create_dir_all(&dir).unwrap();
    let db = Database::open(dir.clone()).unwrap();
    let cipher: Arc<dyn KeyCipher + Send + Sync> = Arc::new(StaticKeyCipher::new("test"));
    Arc::new(CoreStateInner::new(db, dir, cipher).unwrap())
}

fn settings_payload(state: &CoreStateInner, config: &AppConfig) -> serde_json::Value {
    settings_payload_at(config, state.settings_revision())
}

fn settings_payload_at(config: &AppConfig, expected_revision: u64) -> serde_json::Value {
    let mut payload = serde_json::to_value(config).expect("settings should serialize");
    payload["expected_revision"] = json!(expected_revision);
    payload
}

/// Every request in this suite targets loopback listeners; never route them
/// through an ambient system/environment proxy (which aborts such
/// connections on some machines).
fn loopback_client() -> reqwest::Client {
    reqwest::Client::builder()
        .no_proxy()
        .build()
        .expect("test client should build")
}

#[tokio::test]
async fn public_dashboard_uses_first_registration_and_session_cookie() {
    let state = state("public");
    let handle = gateway::start_gateway_on(state, SocketAddr::from(([0, 0, 0, 0], 0)))
        .await
        .unwrap();
    let base = format!("http://127.0.0.1:{}/dashboard/api", handle.port);
    let client = loopback_client();

    let status = client
        .get(format!("{base}/auth/status"))
        .send()
        .await
        .unwrap()
        .json::<serde_json::Value>()
        .await
        .unwrap();
    assert_eq!(status["local"], false);
    assert_eq!(status["initialized"], false);
    assert_eq!(status["authenticated"], false);

    let response = client
        .post(format!("{base}/auth/register"))
        .json(&json!({ "username": "admin", "password": "password123" }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);
    let cookie = response
        .headers()
        .get(reqwest::header::SET_COOKIE)
        .unwrap()
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_string();

    assert_eq!(
        client
            .get(format!("{base}/settings"))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        client
            .put(format!("{base}/accounts/order"))
            .json(&json!({ "account_ids": [ZEN_FREE_ACCOUNT_ID] }))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        client
            .get(format!("{base}/settings/check-update"))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        client
            .get(format!("{base}/settings/update-status"))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        client
            .post(format!("{base}/settings/install-update"))
            .json(&json!({ "expected_version": "999.0.0" }))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        client
            .get(format!("{base}/application-models"))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        client
            .get(format!("{base}/settings"))
            .header(reqwest::header::COOKIE, &cookie)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );
    let reordered = client
        .put(format!("{base}/accounts/order"))
        .header(reqwest::header::COOKIE, &cookie)
        .json(&json!({ "account_ids": [ZEN_FREE_ACCOUNT_ID] }))
        .send()
        .await
        .unwrap();
    assert_eq!(reordered.status(), StatusCode::OK);
    let reordered = reordered.json::<serde_json::Value>().await.unwrap();
    let reordered_ids = reordered
        .as_array()
        .expect("reorder response should be an account list")
        .iter()
        .map(|account| {
            account["id"]
                .as_str()
                .expect("account id should be a string")
        })
        .collect::<Vec<_>>();
    assert_eq!(reordered_ids, [ZEN_FREE_ACCOUNT_ID]);
    let application_models = client
        .get(format!("{base}/application-models"))
        .header(reqwest::header::COOKIE, &cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(application_models.status(), StatusCode::OK);
    assert!(
        application_models
            .json::<serde_json::Value>()
            .await
            .unwrap()
            .as_array()
            .is_some(),
        "authenticated application-models must return a local JSON array"
    );

    assert_eq!(
        client
            .post(format!("{base}/auth/login"))
            .json(&json!({ "username": "admin", "password": "wrong-password" }))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        client
            .post(format!("{base}/auth/register"))
            .json(&json!({ "username": "other", "password": "password456" }))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::CONFLICT
    );

    assert_eq!(
        client
            .post(format!("{base}/auth/logout"))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        client
            .post(format!("{base}/auth/logout"))
            .header(reqwest::header::COOKIE, &cookie)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        client
            .get(format!("{base}/settings"))
            .header(reqwest::header::COOKIE, &cookie)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::UNAUTHORIZED
    );

    let login = client
        .post(format!("{base}/auth/login"))
        .json(&json!({ "username": "admin", "password": "password123" }))
        .send()
        .await
        .unwrap();
    assert_eq!(login.status(), StatusCode::OK);
    let replacement_cookie = login
        .headers()
        .get(reqwest::header::SET_COOKIE)
        .unwrap()
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_string();
    assert_ne!(replacement_cookie, cookie);
    assert_eq!(
        client
            .get(format!("{base}/settings"))
            .header(reqwest::header::COOKIE, &replacement_cookie)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );

    gateway::stop_gateway(handle);
}

#[tokio::test]
async fn loopback_dashboard_skips_login() {
    let state = state("local");
    let handle = gateway::start_gateway_on(state, SocketAddr::from(([127, 0, 0, 1], 0)))
        .await
        .unwrap();
    let base = format!("http://127.0.0.1:{}/dashboard/api", handle.port);
    let client = loopback_client();

    let status = client
        .get(format!("{base}/auth/status"))
        .send()
        .await
        .unwrap()
        .json::<serde_json::Value>()
        .await
        .unwrap();
    assert_eq!(status["local"], true);
    assert_eq!(status["authenticated"], true);
    assert_eq!(
        client
            .get(format!("{base}/settings"))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );

    let forwarded_status = client
        .get(format!("{base}/auth/status"))
        .header("x-forwarded-for", "203.0.113.10")
        .send()
        .await
        .unwrap()
        .json::<serde_json::Value>()
        .await
        .unwrap();
    assert_eq!(forwarded_status["local"], false);
    assert_eq!(forwarded_status["authenticated"], false);
    assert_eq!(
        client
            .get(format!("{base}/settings"))
            .header("x-forwarded-for", "203.0.113.10")
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::UNAUTHORIZED
    );

    gateway::stop_gateway(handle);
}

#[tokio::test]
async fn loopback_desktop_update_api_is_safe_atomic_and_pollable() {
    let current_version = env!("CARGO_PKG_VERSION");
    let current_major = current_version
        .split('.')
        .next()
        .unwrap()
        .parse::<u64>()
        .unwrap();
    let newer_version = format!("{}.0.0", current_major + 1);
    let client = loopback_client();

    let unsupported_state = state("desktop-update-unsupported");
    let unsupported_handle =
        gateway::start_gateway_on(unsupported_state, SocketAddr::from(([127, 0, 0, 1], 0)))
            .await
            .unwrap();
    let unsupported_url = format!(
        "http://127.0.0.1:{}/dashboard/api/settings/install-update",
        unsupported_handle.port
    );
    assert_eq!(
        client.post(&unsupported_url).send().await.unwrap().status(),
        StatusCode::UNSUPPORTED_MEDIA_TYPE
    );
    assert_eq!(
        client
            .post(&unsupported_url)
            .form(&[("expected_version", newer_version.as_str())])
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::UNSUPPORTED_MEDIA_TYPE
    );
    assert_eq!(
        client
            .post(&unsupported_url)
            .json(&json!({ "expected_version": newer_version }))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::BAD_REQUEST
    );
    gateway::stop_gateway(unsupported_handle);

    let supported_state = state("desktop-update-supported");
    let started_versions = Arc::new(StdMutex::new(Vec::new()));
    let captured_versions = started_versions.clone();
    supported_state.set_desktop_update_starter(Arc::new(move |expected_version| {
        captured_versions.lock().unwrap().push(expected_version);
        Ok(())
    }));
    let supported_handle = gateway::start_gateway_on(
        supported_state.clone(),
        SocketAddr::from(([127, 0, 0, 1], 0)),
    )
    .await
    .unwrap();
    let base = format!(
        "http://127.0.0.1:{}/dashboard/api/settings",
        supported_handle.port
    );
    let initial = client
        .get(format!("{base}/update-status"))
        .send()
        .await
        .unwrap()
        .json::<serde_json::Value>()
        .await
        .unwrap();
    assert_eq!(initial["phase"], "idle");
    assert_eq!(initial["current_version"], current_version);
    assert_eq!(initial["install_supported"], true);

    for rejected in [current_version.to_string(), "0.0.1".to_string()] {
        assert_eq!(
            client
                .post(format!("{base}/install-update"))
                .json(&json!({ "expected_version": rejected }))
                .send()
                .await
                .unwrap()
                .status(),
            StatusCode::BAD_REQUEST,
            "{rejected}"
        );
    }
    assert!(started_versions.lock().unwrap().is_empty());

    let accepted = client
        .post(format!("{base}/install-update"))
        .json(&json!({ "expected_version": format!("v{newer_version}-beta.1") }))
        .send()
        .await
        .unwrap();
    assert_eq!(accepted.status(), StatusCode::ACCEPTED);
    let accepted = accepted.json::<serde_json::Value>().await.unwrap();
    assert_eq!(accepted["phase"], "checking");
    assert_eq!(
        started_versions.lock().unwrap().as_slice(),
        [format!("{newer_version}-beta.1")]
    );
    assert_eq!(
        client
            .post(format!("{base}/install-update"))
            .json(&json!({ "expected_version": newer_version }))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::CONFLICT
    );
    assert_eq!(started_versions.lock().unwrap().len(), 1);

    assert!(supported_state.set_desktop_update_progress(64, Some(128)));
    let downloading = client
        .get(format!("{base}/update-status"))
        .send()
        .await
        .unwrap()
        .json::<serde_json::Value>()
        .await
        .unwrap();
    assert_eq!(downloading["phase"], "downloading");
    assert_eq!(downloading["downloaded"], 64);
    assert_eq!(downloading["total"], 128);

    assert!(supported_state.set_desktop_update_installing());
    supported_state.set_desktop_update_failed("signature verification failed");
    let failed = client
        .get(format!("{base}/update-status"))
        .send()
        .await
        .unwrap()
        .json::<serde_json::Value>()
        .await
        .unwrap();
    assert_eq!(failed["phase"], "failed");
    assert_eq!(failed["error"], "signature verification failed");

    let retried = client
        .post(format!("{base}/install-update"))
        .json(&json!({ "expected_version": newer_version }))
        .send()
        .await
        .unwrap();
    assert_eq!(retried.status(), StatusCode::ACCEPTED);
    let retried = retried.json::<serde_json::Value>().await.unwrap();
    assert_eq!(retried["phase"], "checking");
    assert_eq!(retried["downloaded"], 0);
    assert!(retried["total"].is_null());
    assert!(retried["error"].is_null());
    assert_eq!(started_versions.lock().unwrap().len(), 2);

    gateway::stop_gateway(supported_handle);
}

#[tokio::test]
async fn loopback_settings_trim_and_require_gateway_key() {
    let state = state("settings-key");
    let handle = gateway::start_gateway_on(state.clone(), SocketAddr::from(([127, 0, 0, 1], 0)))
        .await
        .unwrap();
    let url = format!("http://127.0.0.1:{}/dashboard/api/settings", handle.port);
    let client = loopback_client();

    // v1.6.1 semantics: a settings update may customize the primary key
    // (trimmed, non-blank, unique across credentials).
    let mut config = state.config();
    config.gateway_key = "  trimmed-key  ".into();
    config.client_root_url = "  http://192.168.1.20:9042/proxy/v1/  ".into();
    config.connect_timeout_secs = 12;
    config.non_stream_timeout_secs = 345;
    config.stream_idle_timeout_secs = 678;
    assert_eq!(
        client
            .post(&url)
            .json(&settings_payload(&state, &config))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );
    let saved = state.config();
    assert_eq!(saved.gateway_key, "trimmed-key");
    assert_eq!(saved.client_root_url, "http://192.168.1.20:9042/proxy");
    assert_eq!(saved.connect_timeout_secs, 12);
    assert_eq!(saved.non_stream_timeout_secs, 345);
    assert_eq!(saved.stream_idle_timeout_secs, 678);
    let roundtrip = client
        .get(&url)
        .send()
        .await
        .unwrap()
        .json::<serde_json::Value>()
        .await
        .unwrap();
    assert_eq!(roundtrip["connect_timeout_secs"], 12);
    assert_eq!(roundtrip["non_stream_timeout_secs"], 345);
    assert_eq!(roundtrip["stream_idle_timeout_secs"], 678);
    assert_eq!(
        roundtrip["client_root_url"],
        "http://192.168.1.20:9042/proxy"
    );
    assert_eq!(roundtrip["auto_start_supported"], false);
    assert_eq!(roundtrip["client_root_url_from_env"], false);
    assert_eq!(roundtrip["gateway_key"], json!("trimmed-key"));

    // A blank key is rejected and keeps the previous value.
    config.gateway_key = "   ".into();
    assert_eq!(
        client
            .post(&url)
            .json(&settings_payload(&state, &config))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::BAD_REQUEST
    );
    assert_eq!(state.config().gateway_key, "trimmed-key");

    // A key colliding with any non-deleted sub key — enabled or disabled —
    // is rejected by the unified gate.
    let sub = ocg_core::gateway_keys::create_sub_key(&state, "Laptop").unwrap();
    let mut colliding = state.config();
    colliding.gateway_key = sub.key.clone();
    assert_eq!(
        client
            .post(&url)
            .json(&settings_payload(&state, &colliding))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::BAD_REQUEST
    );
    assert_eq!(state.config().gateway_key, "trimmed-key");
    ocg_core::gateway_keys::set_sub_key_enabled(&state, &sub.id, false).unwrap();
    assert_eq!(
        client
            .post(&url)
            .json(&settings_payload(&state, &colliding))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::BAD_REQUEST,
        "disabled sub keys still hold their value and block the primary"
    );
    assert_eq!(state.config().gateway_key, "trimmed-key");

    // Sub key material submitted through the generic settings save never
    // touches the sub key table.
    let before = state.db.lock().list_sub_gateway_keys().unwrap();
    let mut with_keys = state.config();
    with_keys.gateway_key = "final-key".into();
    let payload = {
        let mut value = settings_payload(&state, &with_keys);
        value["gateway_keys"] = json!([
            {"id": "forged", "name": "Forged", "key": "ocg-forged", "enabled": true,
             "created_at": "2026-08-16T00:00:00Z"}
        ]);
        value
    };
    assert_eq!(
        client
            .post(&url)
            .json(&payload)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );
    assert_eq!(state.config().gateway_key, "final-key");
    assert_eq!(
        state.db.lock().list_sub_gateway_keys().unwrap(),
        before,
        "settings updates cannot create, modify, or remove sub keys"
    );

    for client_root_url in [
        "ocg.example.com",
        "ftp://ocg.example.com",
        "https://user:secret@ocg.example.com",
        "https://ocg.example.com?node=one",
        "https://ocg.example.com#settings",
        "https://ocg.example.com/v1/chat/completions",
    ] {
        let mut invalid = state.config();
        invalid.client_root_url = client_root_url.into();
        assert_eq!(
            client
                .post(&url)
                .json(&settings_payload(&state, &invalid))
                .send()
                .await
                .unwrap()
                .status(),
            StatusCode::BAD_REQUEST,
            "{client_root_url}"
        );
        assert_eq!(
            state.config().client_root_url,
            "http://192.168.1.20:9042/proxy"
        );
    }

    for (field, value) in [
        ("connect_timeout_secs", 0),
        ("connect_timeout_secs", 301),
        ("non_stream_timeout_secs", 0),
        ("non_stream_timeout_secs", 3_601),
        ("stream_idle_timeout_secs", 0),
        ("stream_idle_timeout_secs", 3_601),
    ] {
        let mut invalid = state.config();
        match field {
            "connect_timeout_secs" => invalid.connect_timeout_secs = value,
            "non_stream_timeout_secs" => invalid.non_stream_timeout_secs = value,
            "stream_idle_timeout_secs" => invalid.stream_idle_timeout_secs = value,
            _ => unreachable!(),
        }
        assert_eq!(
            client
                .post(&url)
                .json(&settings_payload(&state, &invalid))
                .send()
                .await
                .unwrap()
                .status(),
            StatusCode::BAD_REQUEST,
            "{field}={value}"
        );
        let unchanged = state.config();
        assert_eq!(unchanged.connect_timeout_secs, 12);
        assert_eq!(unchanged.non_stream_timeout_secs, 345);
        assert_eq!(unchanged.stream_idle_timeout_secs, 678);
    }

    gateway::stop_gateway(handle);
}

#[tokio::test]
async fn loopback_settings_accept_legacy_payload_without_revision() {
    let state = state("settings-legacy-payload");
    let handle = gateway::start_gateway_on(state.clone(), SocketAddr::from(([127, 0, 0, 1], 0)))
        .await
        .unwrap();
    let url = format!("http://127.0.0.1:{}/dashboard/api/settings", handle.port);

    let mut config = state.config();
    config.connect_timeout_secs = 17;
    let payload = serde_json::to_value(&config).unwrap();
    assert!(payload.get("expected_revision").is_none());

    let response = loopback_client()
        .post(&url)
        .json(&payload)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(state.config().connect_timeout_secs, 17);

    gateway::stop_gateway(handle);
}

#[tokio::test]
async fn loopback_settings_round_trip_routing_modes_and_reject_unknown_values() {
    let state = state("settings-routing");
    let handle = gateway::start_gateway_on(state.clone(), SocketAddr::from(([127, 0, 0, 1], 0)))
        .await
        .unwrap();
    let url = format!("http://127.0.0.1:{}/dashboard/api/settings", handle.port);
    let client = loopback_client();

    for mode in [
        RoutingMode::StrictPriority,
        RoutingMode::StickyGlobal,
        RoutingMode::RoundRobin,
    ] {
        let mut config = state.config();
        config.routing_mode = mode;
        config.conversation_sticky = mode != RoutingMode::StrictPriority;
        let response = client
            .post(&url)
            .json(&settings_payload(&state, &config))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let loaded = client
            .get(&url)
            .send()
            .await
            .unwrap()
            .json::<serde_json::Value>()
            .await
            .unwrap();
        assert_eq!(loaded["routing_mode"], serde_json::to_value(mode).unwrap());
        assert_eq!(
            loaded["conversation_sticky"],
            mode != RoutingMode::StrictPriority
        );
    }

    let before = state.config();
    let before_revision = state.settings_revision();
    let mut invalid = settings_payload(&state, &before);
    invalid["routing_mode"] = json!("weighted-random");
    let response = client.post(&url).json(&invalid).send().await.unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(state.config().routing_mode, before.routing_mode);
    assert_eq!(
        state.config().conversation_sticky,
        before.conversation_sticky
    );
    assert_eq!(state.settings_revision(), before_revision);

    gateway::stop_gateway(handle);
}

#[tokio::test]
async fn loopback_settings_reject_stale_revision_after_key_regeneration() {
    let state = state("settings-stale-revision");
    let handle = gateway::start_gateway_on(state.clone(), SocketAddr::from(([127, 0, 0, 1], 0)))
        .await
        .unwrap();
    let url = format!("http://127.0.0.1:{}/dashboard/api/settings", handle.port);
    let client = loopback_client();
    let loaded = client
        .get(&url)
        .send()
        .await
        .unwrap()
        .json::<serde_json::Value>()
        .await
        .unwrap();
    let stale_revision = loaded["revision"].as_u64().unwrap();
    let mut stale_config: AppConfig = serde_json::from_value(loaded).unwrap();

    let regenerated = client
        .post(format!("{url}/regenerate-gateway-key"))
        .send()
        .await
        .unwrap();
    assert_eq!(regenerated.status(), StatusCode::OK);
    let regenerated = regenerated.json::<serde_json::Value>().await.unwrap();
    let regenerated_key = regenerated["key"].as_str().unwrap();
    assert_ne!(regenerated["revision"].as_u64().unwrap(), stale_revision);

    stale_config.connect_timeout_secs += 1;
    let stale_update = client
        .post(&url)
        .json(&settings_payload_at(&stale_config, stale_revision))
        .send()
        .await
        .unwrap();
    assert_eq!(stale_update.status(), StatusCode::CONFLICT);
    assert_eq!(state.config().gateway_key, regenerated_key);
    assert_ne!(
        state.config().connect_timeout_secs,
        stale_config.connect_timeout_secs
    );

    gateway::stop_gateway(handle);
}

#[tokio::test]
async fn loopback_settings_gate_and_sync_auto_start() {
    AUTO_START_SYNCED.store(false, Ordering::Relaxed);
    AUTO_START_FAIL.store(false, Ordering::Relaxed);
    let unsupported_state = state("settings-auto-start-unsupported");
    let unsupported_handle = gateway::start_gateway_on(
        unsupported_state.clone(),
        SocketAddr::from(([127, 0, 0, 1], 0)),
    )
    .await
    .unwrap();
    let unsupported_url = format!(
        "http://127.0.0.1:{}/dashboard/api/settings",
        unsupported_handle.port
    );
    let client = loopback_client();
    let mut unsupported_config = unsupported_state.config();
    unsupported_config.auto_start = true;
    assert_eq!(
        client
            .post(&unsupported_url)
            .json(&settings_payload(&unsupported_state, &unsupported_config))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::BAD_REQUEST
    );
    assert!(!unsupported_state.config().auto_start);

    let mut preserved_config = unsupported_state.config();
    preserved_config.auto_start = true;
    unsupported_state
        .set_config(preserved_config.clone())
        .unwrap();
    let roundtrip = client
        .get(&unsupported_url)
        .send()
        .await
        .unwrap()
        .json::<serde_json::Value>()
        .await
        .unwrap();
    assert_eq!(roundtrip["auto_start_supported"], false);
    assert_eq!(roundtrip["auto_start"], true);
    preserved_config.connect_timeout_secs = 31;
    assert_eq!(
        client
            .post(&unsupported_url)
            .json(&settings_payload(&unsupported_state, &preserved_config))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );
    assert!(unsupported_state.config().auto_start);
    preserved_config.auto_start = false;
    assert_eq!(
        client
            .post(&unsupported_url)
            .json(&settings_payload(&unsupported_state, &preserved_config))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::BAD_REQUEST
    );
    assert!(unsupported_state.config().auto_start);
    gateway::stop_gateway(unsupported_handle);

    let supported_state = state("settings-auto-start-supported");
    supported_state.set_auto_start_sync(test_auto_start_sync);
    let supported_handle = gateway::start_gateway_on(
        supported_state.clone(),
        SocketAddr::from(([127, 0, 0, 1], 0)),
    )
    .await
    .unwrap();
    let supported_url = format!(
        "http://127.0.0.1:{}/dashboard/api/settings",
        supported_handle.port
    );
    let mut supported_config = supported_state.config();
    supported_config.auto_start = true;
    let pre_update_revision = supported_state.settings_revision();
    let pre_update_payload = settings_payload_at(&supported_config, pre_update_revision);
    AUTO_START_FAIL.store(true, Ordering::Relaxed);
    assert_eq!(
        client
            .post(&supported_url)
            .json(&pre_update_payload)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::INTERNAL_SERVER_ERROR
    );
    assert!(!supported_state.config().auto_start);
    assert_ne!(supported_state.settings_revision(), pre_update_revision);
    let persisted = supported_state
        .db
        .lock()
        .get_setting("config")
        .unwrap()
        .unwrap();
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&persisted).unwrap()["auto_start"],
        false
    );

    AUTO_START_FAIL.store(false, Ordering::Relaxed);
    assert_eq!(
        client
            .post(&supported_url)
            .json(&pre_update_payload)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::CONFLICT
    );
    assert_eq!(
        client
            .post(&supported_url)
            .json(&settings_payload(&supported_state, &supported_config))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );
    assert!(supported_state.config().auto_start);
    assert!(AUTO_START_SYNCED.load(Ordering::Relaxed));
    let roundtrip = client
        .get(&supported_url)
        .send()
        .await
        .unwrap()
        .json::<serde_json::Value>()
        .await
        .unwrap();
    assert_eq!(roundtrip["auto_start_supported"], true);
    assert_eq!(roundtrip["auto_start"], true);

    gateway::stop_gateway(supported_handle);
}

#[tokio::test]
async fn loopback_forward_logs_apply_filters_before_pagination() {
    let state = state("forward-logs");
    for (account_id, prompt_tokens) in [("selected", 10), ("other", 100)] {
        state
            .db
            .lock()
            .log_forward(&ForwardLog {
                id: 0,
                timestamp: Utc::now(),
                model: "glm-5.2".into(),
                account_id: account_id.into(),
                account_name: account_id.into(),
                route_account_id: None,
                provider_id: None,
                offering_id: None,
                credential_account_id: None,
                client_key_id: None,
                client_key_name: None,
                status: "success".into(),
                http_status: Some(200),
                route: String::new(),
                prompt_tokens,
                completion_tokens: prompt_tokens * 2,
                cached_tokens: 0,
                cache_creation_tokens: 0,
                cost: Some(prompt_tokens as f64 / 100.0),
                raw_cost_usd: None,
                quota_debit: None,
                effective_paid_cost_usd: None,
                pricing_revision_id: None,
                quota_multiplier: None,
                local_adjustment_multiplier: None,
                service_tier: None,
                cost_state: "legacy_estimate".into(),
                error_message: None,
                request_id: None,
                attempt: None,
                error_source: None,
                error_stage: None,
                duration_ms: None,
                diagnostic: None,
            })
            .unwrap();
    }

    let handle = gateway::start_gateway_on(state, SocketAddr::from(([127, 0, 0, 1], 0)))
        .await
        .unwrap();
    let response = loopback_client()
        .get(format!(
            "http://127.0.0.1:{}/dashboard/api/logs/forward?limit=1&offset=0&status=success&account_id=selected",
            handle.port
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body["items"].as_array().unwrap().len(), 1);
    assert_eq!(body["items"][0]["account_id"], "selected");
    assert_eq!(body["summary"]["total_requests"], 1);
    assert_eq!(body["summary"]["prompt_tokens"], 10);
    assert_eq!(body["summary"]["completion_tokens"], 20);
    assert_eq!(body["summary"]["cost"], 0.1);

    gateway::stop_gateway(handle);
}

#[tokio::test]
async fn loopback_forward_logs_filter_by_provider_attribution() {
    let state = state("forward-provider-logs");
    let insert = |model: &str,
                  provider_id: &str,
                  offering_id: &str,
                  route_account_id: &str,
                  credential_account_id: &str| {
        state
            .db
            .lock()
            .log_forward(&ForwardLog {
                id: 0,
                timestamp: Utc::now(),
                model: model.into(),
                account_id: credential_account_id.into(),
                account_name: credential_account_id.into(),
                route_account_id: Some(route_account_id.into()),
                provider_id: Some(provider_id.into()),
                offering_id: Some(offering_id.into()),
                credential_account_id: Some(credential_account_id.into()),
                client_key_id: None,
                client_key_name: None,
                status: "success".into(),
                http_status: Some(200),
                route: String::new(),
                prompt_tokens: 1,
                completion_tokens: 1,
                cached_tokens: 0,
                cache_creation_tokens: 0,
                cost: Some(0.1),
                raw_cost_usd: None,
                quota_debit: None,
                effective_paid_cost_usd: None,
                pricing_revision_id: None,
                quota_multiplier: None,
                local_adjustment_multiplier: None,
                service_tier: None,
                cost_state: "legacy_estimate".into(),
                error_message: None,
                request_id: None,
                attempt: None,
                error_source: None,
                error_stage: None,
                duration_ms: None,
                diagnostic: None,
            })
            .unwrap();
    };
    insert("go-a", "opencode", "go", "go-a", "go-a");
    insert("go-b", "opencode", "go", "go-b", "go-b");
    insert("zen", "opencode", "zen-free", "zen-free", "go-a");
    // Inserted last so a global LIMIT before filtering would conceal Go rows.
    insert("goat", "goat", "goat", "goat-a", "goat-a");

    let handle = gateway::start_gateway_on(state, SocketAddr::from(([127, 0, 0, 1], 0)))
        .await
        .unwrap();
    let base = format!(
        "http://127.0.0.1:{}/dashboard/api/logs/forward",
        handle.port
    );
    let client = loopback_client();

    let second_go: serde_json::Value = client
        .get(format!(
            "{base}?provider_id=opencode&offering_id=go&limit=1&offset=1"
        ))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(second_go["summary"]["total_requests"], 2);
    assert_eq!(second_go["items"][0]["model"], "go-a");

    let routed_zen: serde_json::Value = client
        .get(format!("{base}?route_account_id=zen-free"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(routed_zen["summary"]["total_requests"], 1);
    assert_eq!(routed_zen["items"][0]["credential_account_id"], "go-a");

    let credential_go_a: serde_json::Value = client
        .get(format!("{base}?credential_account_id=go-a"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(credential_go_a["summary"]["total_requests"], 2);
    assert_eq!(credential_go_a["items"][0]["model"], "zen");

    let empty_provider: serde_json::Value = client
        .get(format!("{base}?provider_id=&offering_id=go"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(empty_provider["summary"]["total_requests"], 2);

    gateway::stop_gateway(handle);
}

#[tokio::test]
async fn account_crud_exposes_and_enforces_shared_revision_without_breaking_legacy_calls() {
    let state = state("account-revision-cas");
    let handle = gateway::start_gateway_on(state.clone(), SocketAddr::from(([127, 0, 0, 1], 0)))
        .await
        .unwrap();
    let base = format!("http://127.0.0.1:{}/dashboard/api", handle.port);
    let client = loopback_client();

    let initial_revision = state.settings_revision();
    let listed: serde_json::Value = client
        .get(format!("{base}/accounts"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(
        listed
            .as_array()
            .unwrap()
            .iter()
            .all(|account| { account["revision"].as_u64() == Some(initial_revision) })
    );

    let created: serde_json::Value = client
        .post(format!("{base}/accounts"))
        .json(&json!({
            "name": "CAS account",
            "key": "cas-key",
            "expected_revision": initial_revision
        }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let account_id = created["id"].as_str().unwrap().to_string();
    let mut revision = created["revision"].as_u64().unwrap();
    assert_eq!(revision, initial_revision + 1);

    client
        .patch(format!("{base}/accounts/{account_id}/usage"))
        .json(&json!({
            "window": "window_5h",
            "percent": 50.0,
            "resets_in_minutes": 180
        }))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap();
    let provider_usage: serde_json::Value = client
        .get(format!("{base}/accounts/{account_id}/provider-usage"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(provider_usage["availability"], "available");
    assert_eq!(provider_usage["quota_windows"].as_array().unwrap().len(), 3);
    let rolling = provider_usage["quota_windows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|window| window["window_kind"] == "five_hours")
        .unwrap();
    assert_eq!(rolling["source"], "opencode-go-live");
    assert_eq!(
        rolling["used"].as_f64().unwrap(),
        rolling["limit_value"].as_f64().unwrap() * 0.5
    );

    let stale = client
        .patch(format!("{base}/accounts/{account_id}"))
        .json(&json!({
            "name": "stale",
            "expected_revision": initial_revision
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(stale.status(), StatusCode::CONFLICT);
    assert_eq!(state.settings_revision(), revision);

    let updated: serde_json::Value = client
        .patch(format!("{base}/accounts/{account_id}"))
        .json(&json!({
            "name": "updated",
            "expected_revision": revision
        }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    revision += 1;
    assert_eq!(updated["revision"].as_u64(), Some(revision));

    // Empty-body legacy toggle remains accepted and still advances the shared
    // revision returned on the account DTO.
    let toggled: serde_json::Value = client
        .post(format!("{base}/accounts/{account_id}/toggle"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    revision += 1;
    assert_eq!(toggled["revision"].as_u64(), Some(revision));

    let stale_toggle = client
        .post(format!("{base}/accounts/{account_id}/toggle"))
        .json(&json!({ "expected_revision": revision - 1 }))
        .send()
        .await
        .unwrap();
    assert_eq!(stale_toggle.status(), StatusCode::CONFLICT);

    let accounts: serde_json::Value = client
        .get(format!("{base}/accounts"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let mut account_ids = accounts
        .as_array()
        .unwrap()
        .iter()
        .map(|account| account["id"].as_str().unwrap().to_string())
        .collect::<Vec<_>>();
    account_ids.reverse();
    let reordered: serde_json::Value = client
        .put(format!("{base}/accounts/order"))
        .json(&json!({
            "account_ids": account_ids.clone(),
            "expected_revision": revision
        }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    revision += 1;
    assert!(
        reordered
            .as_array()
            .unwrap()
            .iter()
            .all(|account| { account["revision"].as_u64() == Some(revision) })
    );

    let legacy_reordered: serde_json::Value = client
        .put(format!("{base}/accounts/order"))
        .json(&json!({ "account_ids": account_ids }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    revision += 1;
    assert!(
        legacy_reordered
            .as_array()
            .unwrap()
            .iter()
            .all(|account| { account["revision"].as_u64() == Some(revision) })
    );

    let stale_cooldown_reset = client
        .post(format!("{base}/accounts/{account_id}/reset-cooldown"))
        .json(&json!({ "expected_revision": revision - 1 }))
        .send()
        .await
        .unwrap();
    assert_eq!(stale_cooldown_reset.status(), StatusCode::CONFLICT);
    assert_eq!(state.settings_revision(), revision);

    let cooldown_reset: serde_json::Value = client
        .post(format!("{base}/accounts/{account_id}/reset-cooldown"))
        .json(&json!({ "expected_revision": revision }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    revision += 1;
    assert_eq!(cooldown_reset["revision"].as_u64(), Some(revision));

    let legacy_cooldown_reset: serde_json::Value = client
        .post(format!("{base}/accounts/{account_id}/reset-cooldown"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    revision += 1;
    assert_eq!(legacy_cooldown_reset["revision"].as_u64(), Some(revision));

    let profile = browser_profile_paths(&state.data_dir(), &account_id).unwrap()[0].clone();
    fs::create_dir_all(&profile).unwrap();
    fs::write(profile.join("Cookies"), b"stale request must preserve this").unwrap();
    let stale_profile_reset = client
        .delete(format!("{base}/accounts/{account_id}/browser-profile"))
        .json(&json!({ "expected_revision": revision - 1 }))
        .send()
        .await
        .unwrap();
    assert_eq!(stale_profile_reset.status(), StatusCode::CONFLICT);
    assert!(profile.join("Cookies").is_file());

    let profile_reset: serde_json::Value = client
        .delete(format!("{base}/accounts/{account_id}/browser-profile"))
        .json(&json!({ "expected_revision": revision }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    revision += 1;
    assert_eq!(profile_reset["revision"].as_u64(), Some(revision));
    assert!(!profile.exists());

    fs::create_dir_all(&profile).unwrap();
    fs::write(profile.join("Cookies"), b"legacy request").unwrap();
    let legacy_profile_reset: serde_json::Value = client
        .delete(format!("{base}/accounts/{account_id}/browser-profile"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    revision += 1;
    assert_eq!(legacy_profile_reset["revision"].as_u64(), Some(revision));
    assert!(!profile.exists());

    let stale_delete = client
        .delete(format!("{base}/accounts/{account_id}"))
        .json(&json!({ "expected_revision": revision - 1 }))
        .send()
        .await
        .unwrap();
    assert_eq!(stale_delete.status(), StatusCode::CONFLICT);
    assert!(state.db.lock().get_account(&account_id).unwrap().is_some());

    let deleted = client
        .delete(format!("{base}/accounts/{account_id}"))
        .json(&json!({ "expected_revision": revision }))
        .send()
        .await
        .unwrap();
    assert_eq!(deleted.status(), StatusCode::NO_CONTENT);
    revision += 1;
    assert_eq!(
        deleted
            .headers()
            .get("x-ocg-settings-revision")
            .unwrap()
            .to_str()
            .unwrap(),
        revision.to_string()
    );
    assert_eq!(state.settings_revision(), revision);

    let legacy_created: serde_json::Value = client
        .post(format!("{base}/accounts"))
        .json(&json!({ "name": "legacy delete", "key": "legacy-key" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    revision += 1;
    assert_eq!(legacy_created["revision"].as_u64(), Some(revision));
    let legacy_id = legacy_created["id"].as_str().unwrap();
    let legacy_deleted = client
        .delete(format!("{base}/accounts/{legacy_id}"))
        .send()
        .await
        .unwrap();
    assert_eq!(legacy_deleted.status(), StatusCode::NO_CONTENT);
    revision += 1;
    assert_eq!(
        legacy_deleted
            .headers()
            .get("x-ocg-settings-revision")
            .unwrap()
            .to_str()
            .unwrap(),
        revision.to_string()
    );
    assert_eq!(state.settings_revision(), revision);

    gateway::stop_gateway(handle);
}

#[tokio::test]
async fn multi_provider_dashboard_api_is_guarded_and_keeps_legacy_free_mode_consistent() {
    let state = state("multi-provider-dashboard");
    let handle = gateway::start_gateway_on(state.clone(), SocketAddr::from(([127, 0, 0, 1], 0)))
        .await
        .unwrap();
    let base = format!("http://127.0.0.1:{}/dashboard/api", handle.port);
    let client = loopback_client();

    let catalog: serde_json::Value = client
        .get(format!("{base}/providers/catalog"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let goat = catalog
        .as_array()
        .unwrap()
        .iter()
        .find(|item| {
            item["provider_id"] == COMMAND_CODE_PROVIDER_ID
                && item["offering_id"] == GOAT_OFFERING_ID
        })
        .unwrap();
    assert_eq!(goat["pricing_availability"], "unavailable");
    assert_eq!(goat["usage_availability"], "unavailable");
    let zen_catalog = catalog
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["provider_id"] == "opencode-zen-free")
        .unwrap();
    assert_eq!(zen_catalog["pricing_availability"], "not_applicable");
    assert_eq!(zen_catalog["usage_availability"], "local_state");

    let models: serde_json::Value = client
        .get(format!("{base}/providers/model-capabilities"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(
        models
            .as_array()
            .unwrap()
            .iter()
            .all(|item| item["provider_id"] == OPENCODE_PROVIDER_ID)
    );

    let created = client
        .post(format!("{base}/accounts"))
        .json(&json!({
            "provider_id": COMMAND_CODE_PROVIDER_ID,
            "offering_id": GOAT_OFFERING_ID,
            "name": "GOAT test",
            "key": "goat-key"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(created.status(), StatusCode::OK);
    let created: serde_json::Value = created.json().await.unwrap();
    assert_eq!(created["provider_id"], COMMAND_CODE_PROVIDER_ID);
    assert_eq!(created["offering_id"], GOAT_OFFERING_ID);
    assert_eq!(created["credential_kind"], "api_key");
    assert_eq!(created["quota_scope"], "key");
    assert_eq!(
        created["revision"].as_u64(),
        Some(state.settings_revision())
    );
    let goat_id = created["id"].as_str().unwrap();
    let summary: serde_json::Value = client
        .get(format!("{base}/dashboard/summary"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(
        summary["available_accounts"], 1,
        "Zen Free is available without a key while unconfigured GOAT is not"
    );
    let goat_usage: serde_json::Value = client
        .get(format!("{base}/accounts/{goat_id}/provider-usage"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(goat_usage["availability"], "unavailable");
    assert_eq!(goat_usage["experimental"], true);
    assert_eq!(
        client
            .post(format!("{base}/accounts/{goat_id}/test"))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::CONFLICT
    );
    assert_eq!(
        client
            .get(format!("{base}/accounts/{goat_id}/usage"))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::BAD_REQUEST
    );

    let invalid = client
        .post(format!("{base}/accounts"))
        .json(&json!({
            "provider_id": COMMAND_CODE_PROVIDER_ID,
            "offering_id": "go",
            "name": "bad",
            "key": "bad"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(invalid.status(), StatusCode::BAD_REQUEST);

    let binding_change = client
        .patch(format!("{base}/accounts/{goat_id}"))
        .json(&json!({ "provider_id": OPENCODE_PROVIDER_ID }))
        .send()
        .await
        .unwrap();
    assert_eq!(binding_change.status(), StatusCode::BAD_REQUEST);

    for request in [
        client.delete(format!("{base}/accounts/{ZEN_FREE_ACCOUNT_ID}")),
        client.post(format!("{base}/accounts/{ZEN_FREE_ACCOUNT_ID}/test")),
        client
            .patch(format!("{base}/accounts/{ZEN_FREE_ACCOUNT_ID}/usage"))
            .json(&json!({
                "window": "window_5h", "percent": 50.0
            })),
        client.post(format!(
            "{base}/accounts/{ZEN_FREE_ACCOUNT_ID}/reset-cooldown"
        )),
    ] {
        assert_eq!(
            request.send().await.unwrap().status(),
            StatusCode::BAD_REQUEST
        );
    }

    let before = state.settings_revision();
    let zen = client
        .patch(format!("{base}/providers/zen-free"))
        .json(&json!({
            "enabled": true,
            "expected_revision": before
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(zen.status(), StatusCode::OK);
    let zen: serde_json::Value = zen.json().await.unwrap();
    let revision = zen["revision"].as_u64().unwrap();
    assert_eq!(revision, before + 1);
    assert!(zen["account"].get("free_alias_enabled").is_none());
    assert!(
        serde_json::to_value(state.config())
            .unwrap()
            .get("free_model_routing")
            .is_none()
    );

    let free_until = Utc::now() + chrono::Duration::minutes(5);
    state
        .db
        .lock()
        .set_account_rate_limit(
            ZEN_FREE_ACCOUNT_ID,
            free_until,
            "test free cooldown",
            Some(ocg_core::models::UsageWindowKind::Free),
        )
        .unwrap();
    let zen_usage: serde_json::Value = client
        .get(format!(
            "{base}/accounts/{ZEN_FREE_ACCOUNT_ID}/provider-usage"
        ))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(zen_usage["availability"], "local_state");
    assert_eq!(zen_usage["experimental"], false);
    assert!(zen_usage["free_cooldown_until"].is_string());
    assert_eq!(zen_usage["quota_windows"][0]["window_kind"], "free");
    assert_eq!(zen_usage["quota_windows"][0]["used"], 1.0);
    let cooled_summary: serde_json::Value = client
        .get(format!("{base}/dashboard/summary"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(
        cooled_summary["available_accounts"], 0,
        "the authoritative egress cooldown blocks Zen while GOAT remains unavailable"
    );

    let stale = client
        .patch(format!("{base}/providers/zen-free"))
        .json(&json!({
            "enabled": false,
            "expected_revision": before
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(stale.status(), StatusCode::CONFLICT);

    let settings_save = client
        .post(format!("{base}/settings"))
        .json(&settings_payload_at(&state.config(), revision))
        .send()
        .await
        .unwrap();
    assert_eq!(settings_save.status(), StatusCode::OK);
    let listed: serde_json::Value = client
        .get(format!("{base}/accounts"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let zen_card = listed
        .as_array()
        .unwrap()
        .iter()
        .find(|account| account["id"] == ZEN_FREE_ACCOUNT_ID)
        .unwrap();
    assert_eq!(zen_card["enabled"], true);
    assert!(zen_card.get("free_alias_enabled").is_none());

    gateway::stop_gateway(handle);
}

#[tokio::test]
async fn gateway_key_lifecycle_api_manages_sub_keys() {
    let state = state("keys-lifecycle");
    let handle = gateway::start_gateway_on(state.clone(), SocketAddr::from(([127, 0, 0, 1], 0)))
        .await
        .unwrap();
    let base = format!("http://127.0.0.1:{}/dashboard/api", handle.port);
    let client = loopback_client();

    // The primary key id never addresses a sub key row: lifecycle operations
    // on it fail cleanly.
    let primary_id = ocg_core::gateway_keys::PRIMARY_KEY_ID;
    for operation in ["patch", "delete"] {
        let request = match operation {
            "patch" => client
                .patch(format!("{base}/settings/keys/{primary_id}"))
                .json(&json!({ "name": "Nope" })),
            _ => client.delete(format!("{base}/settings/keys/{primary_id}")),
        };
        assert_eq!(
            request.send().await.unwrap().status(),
            StatusCode::BAD_REQUEST,
            "the primary id must not address a sub key ({operation})"
        );
    }
    assert!(
        state
            .db
            .lock()
            .get_sub_gateway_key(primary_id)
            .unwrap()
            .is_none()
    );

    // Every successful key mutation advances the shared settings revision,
    // keeping the optimistic lock meaningful between key-API writers.
    let revision_before = state.settings_revision();

    // Create a sub key; the full value comes back exactly once.
    let created = client
        .post(format!("{base}/settings/keys"))
        .json(&json!({ "name": "Laptop" }))
        .send()
        .await
        .unwrap();
    assert_eq!(created.status(), StatusCode::OK);
    let created: serde_json::Value = created.json().await.unwrap();
    let secondary_id = created["id"].as_str().unwrap().to_string();
    let secondary_value = created["key"].as_str().unwrap().to_string();
    assert_eq!(created["name"], "Laptop");
    assert_eq!(created["enabled"], true);
    assert!(created["deleted_at"].is_null());
    assert!(!secondary_value.is_empty());
    assert!(
        created["revision"].as_u64().unwrap() > revision_before,
        "creating a key must advance the settings revision"
    );

    // A stale revision (captured before the create) is rejected with 409.
    let stale_create = client
        .post(format!("{base}/settings/keys"))
        .json(&json!({ "name": "Deck", "expected_revision": revision_before }))
        .send()
        .await
        .unwrap();
    assert_eq!(stale_create.status(), StatusCode::CONFLICT);

    // Rename via PATCH audits old and new names.
    let renamed = client
        .patch(format!("{base}/settings/keys/{secondary_id}"))
        .json(&json!({ "name": "Deck" }))
        .send()
        .await
        .unwrap();
    assert_eq!(renamed.status(), StatusCode::OK);

    // Regenerate and delete honor the optional settings-revision check: a
    // stale revision is rejected with 409 before any mutation applies.
    let stale = state.settings_revision().wrapping_sub(1);
    let stale_regenerate = client
        .post(format!("{base}/settings/keys/{secondary_id}/regenerate"))
        .json(&json!({ "expected_revision": stale }))
        .send()
        .await
        .unwrap();
    assert_eq!(stale_regenerate.status(), StatusCode::CONFLICT);
    let stale_delete = client
        .delete(format!("{base}/settings/keys/{secondary_id}"))
        .json(&json!({ "expected_revision": stale }))
        .send()
        .await
        .unwrap();
    assert_eq!(stale_delete.status(), StatusCode::CONFLICT);
    assert_eq!(
        state
            .db
            .lock()
            .get_sub_gateway_key(&secondary_id)
            .unwrap()
            .unwrap()
            .key,
        secondary_value,
        "conflicting requests must not mutate the key"
    );

    // Disable, then verify the value no longer authenticates.
    let disabled = client
        .patch(format!("{base}/settings/keys/{secondary_id}"))
        .json(&json!({ "enabled": false }))
        .send()
        .await
        .unwrap();
    assert_eq!(disabled.status(), StatusCode::OK);
    let unauthorized = client
        .post(format!(
            "http://127.0.0.1:{}/v1/chat/completions",
            handle.port
        ))
        .header("authorization", format!("Bearer {secondary_value}"))
        .json(&json!({"model":"m","messages":[],"max_tokens":1}))
        .send()
        .await
        .unwrap();
    assert_eq!(unauthorized.status(), StatusCode::UNAUTHORIZED);

    // A wrong x-api-key alongside a correct x-goog-api-key must pass (the
    // OR semantics regression from the config-list form); the request then
    // fails downstream on the unknown model instead of with 401.
    let current_primary = state.config().gateway_key;
    let or_semantics = client
        .post(format!(
            "http://127.0.0.1:{}/v1/chat/completions",
            handle.port
        ))
        .header("x-api-key", "wrong-key")
        .header("x-goog-api-key", &current_primary)
        .json(&json!({"model":"m","messages":[],"max_tokens":1}))
        .send()
        .await
        .unwrap();
    assert_ne!(
        or_semantics.status(),
        StatusCode::UNAUTHORIZED,
        "a correct x-goog-api-key must win over a wrong x-api-key"
    );

    // Regenerate returns the new value; the old one is invalid immediately.
    let re_enabled = client
        .patch(format!("{base}/settings/keys/{secondary_id}"))
        .json(&json!({ "enabled": true }))
        .send()
        .await
        .unwrap();
    assert_eq!(re_enabled.status(), StatusCode::OK);
    let regenerated = client
        .post(format!("{base}/settings/keys/{secondary_id}/regenerate"))
        .send()
        .await
        .unwrap();
    assert_eq!(regenerated.status(), StatusCode::OK);
    let regenerated: serde_json::Value = regenerated.json().await.unwrap();
    let new_value = regenerated["key"].as_str().unwrap();
    assert_ne!(new_value, secondary_value);

    // The connection endpoint aggregates the primary value and sub keys
    // with values, behind the same session layer.
    let connection = client
        .get(format!("{base}/connection"))
        .send()
        .await
        .unwrap();
    assert_eq!(connection.status(), StatusCode::OK);
    let connection: serde_json::Value = connection.json().await.unwrap();
    assert_eq!(connection["primary_key"], json!(current_primary));
    assert_eq!(
        connection["sub_keys"][0]["value"],
        json!(new_value),
        "sub key values ride along for the switcher's copy action"
    );
    assert!(connection["revision"].as_u64().is_some());

    // The legacy regenerate endpoint rotates the primary key: the old value
    // stops authenticating and the new one passes.
    let legacy = client
        .post(format!("{base}/settings/regenerate-gateway-key"))
        .send()
        .await
        .unwrap();
    assert_eq!(legacy.status(), StatusCode::OK);
    let legacy: serde_json::Value = legacy.json().await.unwrap();
    let rotated = legacy["key"].as_str().unwrap().to_string();
    assert_ne!(rotated, current_primary);
    assert_eq!(state.config().gateway_key, rotated);
    let old_rejected = client
        .post(format!(
            "http://127.0.0.1:{}/v1/chat/completions",
            handle.port
        ))
        .header("authorization", format!("Bearer {current_primary}"))
        .json(&json!({"model":"m","messages":[],"max_tokens":1}))
        .send()
        .await
        .unwrap();
    assert_eq!(old_rejected.status(), StatusCode::UNAUTHORIZED);
    let new_accepted = client
        .post(format!(
            "http://127.0.0.1:{}/v1/chat/completions",
            handle.port
        ))
        .header("authorization", format!("Bearer {rotated}"))
        .json(&json!({"model":"m","messages":[],"max_tokens":1}))
        .send()
        .await
        .unwrap();
    assert_ne!(
        new_accepted.status(),
        StatusCode::UNAUTHORIZED,
        "the rotated primary value authenticates"
    );

    // Deleting the sub key keeps attribution data with no plaintext.
    let deleted = client
        .delete(format!("{base}/settings/keys/{secondary_id}"))
        .send()
        .await
        .unwrap();
    assert_eq!(deleted.status(), StatusCode::OK);
    let tombstone = state
        .db
        .lock()
        .get_sub_gateway_key(&secondary_id)
        .unwrap()
        .unwrap();
    assert!(tombstone.deleted_at.is_some());
    assert!(tombstone.key.is_empty());
    assert_eq!(tombstone.name, "Deck");

    // Unknown keys are reported, not silently ignored.
    let missing = client
        .delete(format!("{base}/settings/keys/does-not-exist"))
        .send()
        .await
        .unwrap();
    assert_eq!(missing.status(), StatusCode::BAD_REQUEST);

    // Every mutation wrote an audit entry, without "gateway key" wording.
    let audits = state.db.lock().list_gateway_logs(100).unwrap();
    for expected in ["created key `Laptop`", "renamed key `Laptop` to `Deck`"] {
        assert!(
            audits
                .iter()
                .any(|log| log.category == "keys" && log.message.contains(expected)),
            "missing audit containing {expected:?}"
        );
    }
    assert!(
        !audits
            .iter()
            .any(|log| log.category == "keys" && log.message.contains("gateway key")),
        "audit wording must say \"key\" only"
    );

    gateway::stop_gateway(handle);
}
