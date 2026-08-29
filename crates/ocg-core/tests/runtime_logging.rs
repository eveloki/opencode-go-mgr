//! Focused coverage for low-frequency runtime/control-plane observability.

use ocg_core::crypto::{KeyCipher, StaticKeyCipher};
use ocg_core::db::Database;
use ocg_core::gateway::{self, GatewayLifecycle};
use ocg_core::state::CoreStateInner;
use reqwest::StatusCode;
use serde_json::json;
use std::net::SocketAddr;
use std::sync::Arc;

#[path = "fixtures/dashboard_v3/harness.rs"]
mod harness;

#[tokio::test]
async fn listener_bind_and_rebind_are_persisted_without_the_gateway_key() {
    let dir = harness::temp_data_dir("runtime-logging-listener");
    let db = Database::open(dir.clone()).unwrap();
    let cipher: Arc<dyn KeyCipher + Send + Sync> =
        Arc::new(StaticKeyCipher::new("runtime-logging"));
    let state = Arc::new(CoreStateInner::new(db, dir.clone(), cipher).unwrap());
    let gateway_key = state.config().gateway_key;

    let handle = gateway::start_gateway_on(state.clone(), SocketAddr::from(([127, 0, 0, 1], 0)))
        .await
        .unwrap();
    let first_port = handle.port;
    *state.gateway.lock() = Some(handle);

    let second_port =
        GatewayLifecycle::rebind(state.clone(), SocketAddr::from(([127, 0, 0, 1], 0)))
            .await
            .unwrap();
    assert_ne!(first_port, second_port);

    let active = state.gateway.lock().take().unwrap();
    gateway::stop_gateway_and_wait(active).await;

    let logs = state.db.lock().list_gateway_logs(20).unwrap();
    assert!(
        logs.iter().any(|log| {
            log.category == "gateway" && log.message.starts_with("event=listener_bound address=")
        }),
        "{logs:?}"
    );
    assert!(
        logs.iter().any(|log| {
            log.message.contains("event=listener_rebound")
                && log.message.contains(&format!("previous_port={first_port}"))
                && log.message.contains(&format!("new_port={second_port}"))
        }),
        "{logs:?}"
    );
    assert!(
        logs.iter()
            .any(|log| log.message == "event=official_usage_worker_started"),
        "{logs:?}"
    );
    assert!(logs.iter().all(|log| !log.message.contains(&gateway_key)));

    drop(state);
    std::fs::remove_dir_all(dir).unwrap();
}

#[tokio::test]
async fn settings_log_names_changed_fields_without_logging_values() {
    let harness = harness::start_loopback("runtime-logging-settings").await;
    let canary_url = "https://runtime-log-canary.invalid";
    let primary_key = harness.state.config().gateway_key;
    let response = harness
        .client
        .put(format!("{}/settings", harness.v3_base))
        .json(&json!({
            "expectedRevision": harness.state.settings_revision(),
            "processGeneration": harness.state.process_generation(),
            "clientRootUrl": canary_url,
            "connectTimeoutSecs": 17
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let logs = harness.state.db.lock().list_gateway_logs(20).unwrap();
    let settings = logs
        .iter()
        .find(|log| log.message.starts_with("event=settings_updated"))
        .expect("settings update should be visible in runtime logs");
    assert_eq!(settings.category, "settings");
    assert!(settings.message.contains("client_root_url"));
    assert!(settings.message.contains("connect_timeout_secs"));
    assert!(!settings.message.contains(canary_url));
    assert!(logs.iter().all(|log| !log.message.contains(&primary_key)));

    harness.stop();
}
