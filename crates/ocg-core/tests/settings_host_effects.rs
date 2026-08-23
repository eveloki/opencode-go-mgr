//! Host settings-effects extraction: shared CoreState path, hook rollback,
//! V2/V3 adapter mapping, and unchanged production host SCC.

use ocg_core::crypto::{KeyCipher, StaticKeyCipher};
use ocg_core::dashboard_v3::{
    ERROR_INTERNAL, ERROR_INVALID_REQUEST, ERROR_MISSING_EXPECTED_REVISION, ERROR_REVISION_CONFLICT,
};
use ocg_core::db::Database;
use ocg_core::gateway::{self, GatewayLifecycle};
use ocg_core::state::{
    CoreState, CoreStateInner, DockVisibilitySync, GatewayHandle, HostSettingsError,
};
use reqwest::StatusCode;
use serde_json::{Value, json};
use std::cell::{Cell, RefCell};
use std::fs;
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::{Barrier, oneshot};

#[path = "fixtures/dashboard_v3/harness.rs"]
mod harness;

use harness::{V3Harness, loopback_client, start_loopback};

fn production_prefix(source: &str) -> &str {
    source.split("#[cfg(test)]").next().unwrap_or(source)
}

fn temp_data_dir(label: &str) -> PathBuf {
    let mut dir = std::env::temp_dir();
    dir.push(format!(
        "ocg-host-effects-{}-{}",
        label,
        uuid::Uuid::new_v4()
    ));
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn new_state(label: &str) -> (Arc<CoreStateInner>, PathBuf) {
    let dir = temp_data_dir(label);
    let db = Database::open(dir.clone()).unwrap();
    let cipher: Arc<dyn KeyCipher + Send + Sync> = Arc::new(StaticKeyCipher::new("host-effects"));
    (
        Arc::new(CoreStateInner::new(db, dir.clone(), cipher).unwrap()),
        dir,
    )
}

/// Installs a loopback listener whose graceful-shutdown future pauses after
/// observing the signal. A normal lifecycle rebind can then hold the async
/// lifecycle gate while HTTP settings persistence and unrelated writers run.
async fn install_held_listener(state: &CoreState) -> (u16, oneshot::Receiver<()>, Arc<Barrier>) {
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .expect("held listener should bind");
    let local_addr = listener.local_addr().expect("held listener local address");
    state.set_dashboard_local_mode(true);
    let app = gateway::build_router(state.clone());
    let (shutdown_tx, shutdown_rx) = oneshot::channel();
    let (shutdown_seen_tx, shutdown_seen_rx) = oneshot::channel();
    let release = Arc::new(Barrier::new(2));
    let server_release = release.clone();
    let task = tokio::spawn(async move {
        let server = axum::serve(listener, app).with_graceful_shutdown(async move {
            let _ = shutdown_rx.await;
            let _ = shutdown_seen_tx.send(());
            server_release.wait().await;
        });
        if let Err(error) = server.await {
            panic!("held Gateway server failed: {error}");
        }
    });
    *state.gateway.lock() = Some(GatewayHandle {
        port: local_addr.port(),
        listen_addr: local_addr,
        dashboard_is_local: true,
        shutdown: shutdown_tx,
        task,
    });
    (local_addr.port(), shutdown_seen_rx, release)
}

fn persisted_auto_start(state: &CoreStateInner) -> bool {
    let stored = state.db.lock().get_setting("config").unwrap().unwrap();
    serde_json::from_str::<Value>(&stored).unwrap()["auto_start"]
        .as_bool()
        .unwrap()
}

fn persisted_show_dock_icon(state: &CoreStateInner) -> bool {
    let stored = state.db.lock().get_setting("config").unwrap().unwrap();
    serde_json::from_str::<Value>(&stored).unwrap()["show_dock_icon"]
        .as_bool()
        .unwrap()
}

thread_local! {
    static AUTO_START_CALLS: RefCell<Vec<bool>> = const { RefCell::new(Vec::new()) };
    static AUTO_START_FAIL_AT: Cell<Option<usize>> = const { Cell::new(None) };
}

fn reset_auto_start_hook() {
    AUTO_START_CALLS.with(|calls| calls.borrow_mut().clear());
    AUTO_START_FAIL_AT.with(|fail| fail.set(None));
}

fn auto_start_calls() -> Vec<bool> {
    AUTO_START_CALLS.with(|calls| calls.borrow().clone())
}

fn recording_auto_start(enabled: bool) -> ocg_core::Result<()> {
    let index = AUTO_START_CALLS.with(|calls| {
        let mut calls = calls.borrow_mut();
        calls.push(enabled);
        calls.len() - 1
    });
    if AUTO_START_FAIL_AT.with(|fail| fail.get() == Some(index)) {
        anyhow::bail!("auto-start hook failed");
    }
    Ok(())
}

fn always_fail_auto_start(_enabled: bool) -> ocg_core::Result<()> {
    anyhow::bail!("auto-start hook failed")
}

fn fail_enable_auto_start(enabled: bool) -> ocg_core::Result<()> {
    if enabled {
        anyhow::bail!("auto-start hook failed");
    }
    Ok(())
}

fn ok_auto_start(_enabled: bool) -> ocg_core::Result<()> {
    Ok(())
}

struct DockHook {
    calls: Mutex<Vec<bool>>,
    fail_at: Mutex<Option<usize>>,
}

impl DockHook {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            calls: Mutex::new(Vec::new()),
            fail_at: Mutex::new(None),
        })
    }

    fn fail_at(self: &Arc<Self>, index: usize) {
        *self.fail_at.lock().unwrap() = Some(index);
    }

    fn calls(&self) -> Vec<bool> {
        self.calls.lock().unwrap().clone()
    }

    fn sync(self: &Arc<Self>) -> DockVisibilitySync {
        let hook = Arc::clone(self);
        Arc::new(move |visible| {
            let index = {
                let mut calls = hook.calls.lock().unwrap();
                calls.push(visible);
                calls.len() - 1
            };
            if *hook.fail_at.lock().unwrap() == Some(index) {
                anyhow::bail!("dock hook failed");
            }
            Ok(())
        })
    }
}

fn cas_patch(harness: &V3Harness, patch: Value) -> Value {
    let mut body = match patch {
        Value::Object(map) => map,
        _ => serde_json::Map::new(),
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

fn v2_payload(config: &ocg_core::models::AppConfig, expected_revision: Option<u64>) -> Value {
    let mut payload = serde_json::to_value(config).unwrap();
    if let Some(revision) = expected_revision {
        payload["expected_revision"] = json!(revision);
    }
    payload
}

#[test]
fn v2_and_v3_settings_updates_share_the_core_state_host_path() {
    let state_src = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/state.rs"));
    let v2_src = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/dashboard.rs"));
    let v3_src = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/src/dashboard_v3/settings.rs"
    ));
    let lib_src = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/lib.rs"));
    let state_production = production_prefix(state_src);
    // dashboard.rs has #[cfg(test)] helpers before update_settings; inspect
    // the full file so the shared call site is not skipped.
    let v2_production = v2_src;
    let v3_production = production_prefix(v3_src);
    let lib_production = production_prefix(lib_src);

    assert!(state_production.contains("pub fn apply_host_settings("));
    assert!(state_production.contains("pub enum HostSettingsError"));
    assert!(state_production.contains("failed to synchronize desktop settings:"));
    assert_eq!(
        state_production.matches("self.set_config(next)").count(),
        1,
        "success path must persist next exactly once"
    );
    assert!(
        state_production.contains(
            "if auto_start_supported {\n                self.sync_auto_start(next_auto_start)?;"
        ),
        "supported auto-start must be reasserted after every successful save"
    );
    assert!(
        state_production.contains("if dock_visibility_supported {\n                self.sync_dock_visibility(next_show_dock_icon)?;"),
        "supported Dock visibility must be reasserted after every successful save"
    );
    assert!(
        !state_production.contains("auto_start_changed"),
        "supported auto-start invoke must not be gated on a changed flag"
    );
    assert!(
        !state_production.contains("dock_changed"),
        "supported Dock invoke must not be gated on a changed flag"
    );

    assert!(state_production.contains("rebind_gateway_listener_if_port_changed"));
    assert!(state_production.contains("GatewayLifecycle::rebind"));
    assert!(state_production.contains("rebind_from_serving_request"));
    assert!(state_production.contains("settings_host_effects"));
    assert!(state_production.contains("lock_settings_host_effects"));
    assert!(state_production.contains("rebind_listener_after_settings_commit"));
    assert!(state_production.contains("compensate_failed_listener_rebind"));
    assert!(state_production.contains("live.gateway_port != committed.gateway_port"));
    let listener_src = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/src/gateway/listener.rs"
    ));
    let listener_production = production_prefix(listener_src);
    assert!(listener_production.contains("observe_displaced_listener"));
    assert!(listener_production.contains("schedule_dashboard_trust_recompute"));
    assert!(listener_production.contains("recompute_dashboard_trust"));
    assert!(listener_production.contains("stop_and_wait(handle)"));
    assert!(v2_production.contains("apply_host_settings"));
    assert!(v3_production.contains("apply_host_settings"));
    assert!(v2_production.contains("lock_settings_host_effects"));
    assert!(v3_production.contains("lock_settings_host_effects"));
    assert!(v2_production.contains("rebind_listener_after_settings_commit"));
    assert!(v3_production.contains("rebind_listener_after_settings_commit"));
    assert!(v2_production.contains("map_host_settings_error"));
    assert!(v3_production.contains("map_host_settings_error"));

    for adapter in [v2_production, v3_production] {
        assert!(
            !adapter.contains("failed to synchronize desktop settings"),
            "adapters must not keep the host rollback policy"
        );
        assert!(
            !adapter.contains("failed to restore auto-start state"),
            "adapters must not keep auto-start restore policy"
        );
        assert!(
            !adapter.contains("failed to restore Dock visibility"),
            "adapters must not keep Dock restore policy"
        );
        assert!(
            !adapter.contains("sync_auto_start("),
            "adapters must not call auto-start hooks directly"
        );
        assert!(
            !adapter.contains("sync_dock_visibility("),
            "adapters must not call Dock hooks directly"
        );
        assert!(
            !adapter.contains("auto-start is unavailable in this runtime"),
            "unsupported messages must live on HostSettingsError"
        );
        assert!(
            !adapter.contains("Dock visibility is unavailable in this runtime"),
            "unsupported messages must live on HostSettingsError"
        );
        assert!(
            !adapter.contains("GatewayLifecycle::"),
            "HTTP adapters must rebind through CoreState, not GatewayLifecycle directly"
        );
        assert!(
            !adapter.contains("apply_host_settings(&config, previous_config)"),
            "adapters must not unconditionally restore previous_config after a failed rebind"
        );
        assert!(
            !adapter.contains("rebind_gateway_listener_if_port_changed"),
            "HTTP adapters must rebind through the serialized host-effect commit helper"
        );
    }

    assert!(!lib_production.contains("mod host_settings"));
    assert!(!lib_production.contains("mod settings_host"));
    assert!(!lib_production.contains("mod settings_effects"));
}

#[test]
fn production_host_scc_membership_is_unchanged() {
    let kernel = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/kernel/mod.rs"));
    let start = kernel
        .find("const EXPECTED_HOST_SCC:")
        .expect("host SCC whitelist should remain in kernel/mod.rs");
    let block = &kernel[start..];
    let end = block.find(';').expect("EXPECTED_HOST_SCC should end");
    let members: Vec<&str> = block[..end]
        .lines()
        .filter_map(|line| {
            let trimmed = line.trim().trim_end_matches(',');
            trimmed.strip_prefix('"')?.strip_suffix('"')
        })
        .collect();
    assert_eq!(
        members,
        [
            "dashboard",
            "dashboard_v3",
            "gateway",
            "protocol_probe",
            "state"
        ]
    );
}

#[test]
fn successful_host_saves_reassert_every_supported_hook() {
    reset_auto_start_hook();
    let (state, dir) = new_state("host-success");
    state.set_auto_start_sync(recording_auto_start);
    let dock = DockHook::new();
    state.set_dock_visibility_sync(dock.sync());
    let previous = state.config();
    let before = state.settings_revision();

    let mut timeout_only = previous.clone();
    timeout_only.connect_timeout_secs = 12;
    state
        .apply_host_settings(&previous, timeout_only)
        .expect("unrelated settings must persist and reassert host hooks");
    assert_eq!(state.settings_revision(), before + 1);
    assert_eq!(state.config().connect_timeout_secs, 12);
    assert!(!state.config().auto_start);
    assert!(state.config().show_dock_icon);
    assert_eq!(auto_start_calls(), vec![false]);
    assert_eq!(dock.calls(), vec![true]);

    let after_timeout = state.config();
    let mut auto_only = after_timeout.clone();
    auto_only.auto_start = true;
    state
        .apply_host_settings(&after_timeout, auto_only)
        .expect("auto-start change should sync and reassert Dock");
    assert!(state.config().auto_start);
    assert_eq!(auto_start_calls(), vec![false, true]);
    assert_eq!(dock.calls(), vec![true, true]);

    let after_auto = state.config();
    let mut dock_only = after_auto.clone();
    dock_only.show_dock_icon = false;
    state
        .apply_host_settings(&after_auto, dock_only)
        .expect("Dock change should sync and reassert auto-start");
    assert!(!state.config().show_dock_icon);
    assert_eq!(auto_start_calls(), vec![false, true, true]);
    assert_eq!(dock.calls(), vec![true, true, false]);
    assert_eq!(state.settings_revision(), before + 3);

    drop(state);
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn unchanged_supported_hook_failure_rolls_back_config_and_restores_both_hooks() {
    reset_auto_start_hook();
    AUTO_START_FAIL_AT.with(|fail| fail.set(Some(0)));
    let (state, dir) = new_state("host-unchanged-fail");
    state.set_auto_start_sync(recording_auto_start);
    let dock = DockHook::new();
    state.set_dock_visibility_sync(dock.sync());
    let previous = state.config();
    let before = state.settings_revision();
    let mut timeout_only = previous.clone();
    timeout_only.connect_timeout_secs = 12;

    let error = state
        .apply_host_settings(&previous, timeout_only)
        .unwrap_err();
    match error {
        HostSettingsError::Sync(message) => {
            assert_eq!(
                message,
                "failed to synchronize desktop settings: auto-start hook failed"
            );
        }
        other => panic!("expected Sync, got {other:?}"),
    }
    assert_eq!(
        state.config().connect_timeout_secs,
        previous.connect_timeout_secs
    );
    assert!(!state.config().auto_start);
    assert!(state.config().show_dock_icon);
    assert!(!persisted_auto_start(&state));
    assert!(persisted_show_dock_icon(&state));
    assert_eq!(state.settings_revision(), before + 2);
    assert_eq!(auto_start_calls(), vec![false, false]);
    assert_eq!(
        dock.calls(),
        vec![true],
        "unchanged auto-start failure must skip the forward Dock sync and still restore Dock"
    );

    drop(state);
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn unsupported_host_deltas_fail_before_persistence_or_revision_change() {
    let (state, dir) = new_state("host-unsupported");
    let previous = state.config();
    let before = state.settings_revision();
    let primary = previous.gateway_key.clone();

    let mut auto = previous.clone();
    auto.auto_start = true;
    let error = state.apply_host_settings(&previous, auto).unwrap_err();
    assert!(matches!(error, HostSettingsError::AutoStartUnsupported));
    assert_eq!(error.to_string(), HostSettingsError::AUTO_START_UNAVAILABLE);
    assert_eq!(state.settings_revision(), before);
    assert!(!state.config().auto_start);
    assert!(!persisted_auto_start(&state));
    assert_eq!(state.config().gateway_key, primary);

    let mut dock = previous.clone();
    dock.show_dock_icon = false;
    let error = state.apply_host_settings(&previous, dock).unwrap_err();
    assert!(matches!(
        error,
        HostSettingsError::DockVisibilityUnsupported
    ));
    assert_eq!(
        error.to_string(),
        HostSettingsError::DOCK_VISIBILITY_UNAVAILABLE
    );
    assert_eq!(state.settings_revision(), before);
    assert!(state.config().show_dock_icon);
    assert!(persisted_show_dock_icon(&state));

    let mut timeout_only = previous.clone();
    timeout_only.connect_timeout_secs = 12;
    state
        .apply_host_settings(&previous, timeout_only)
        .expect("unsupported runtimes must still persist unchanged host fields");
    assert_eq!(state.settings_revision(), before + 1);
    assert_eq!(state.config().connect_timeout_secs, 12);
    assert!(!state.config().auto_start);
    assert!(state.config().show_dock_icon);

    drop(state);
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn persist_failure_does_not_call_hooks_or_change_revision() {
    reset_auto_start_hook();
    let (state, dir) = new_state("host-persist-fail");
    state.set_auto_start_sync(recording_auto_start);
    let dock = DockHook::new();
    state.set_dock_visibility_sync(dock.sync());
    let previous = state.config();
    let before = state.settings_revision();
    let mut invalid = previous.clone();
    invalid.connect_timeout_secs = 0;
    invalid.auto_start = true;
    invalid.show_dock_icon = false;

    let error = state.apply_host_settings(&previous, invalid).unwrap_err();
    assert!(matches!(error, HostSettingsError::Persist(_)));
    assert_eq!(state.settings_revision(), before);
    assert!(!state.config().auto_start);
    assert!(state.config().show_dock_icon);
    assert!(auto_start_calls().is_empty());
    assert!(dock.calls().is_empty());

    drop(state);
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn first_hook_failure_rolls_back_config_and_restores_both_hooks() {
    reset_auto_start_hook();
    AUTO_START_FAIL_AT.with(|fail| fail.set(Some(0)));
    let (state, dir) = new_state("host-first-hook");
    state.set_auto_start_sync(recording_auto_start);
    let dock = DockHook::new();
    state.set_dock_visibility_sync(dock.sync());
    let previous = state.config();
    let before = state.settings_revision();
    let mut next = previous.clone();
    next.auto_start = true;
    next.show_dock_icon = false;

    let error = state.apply_host_settings(&previous, next).unwrap_err();
    match error {
        HostSettingsError::Sync(message) => {
            assert_eq!(
                message,
                "failed to synchronize desktop settings: auto-start hook failed"
            );
        }
        other => panic!("expected Sync, got {other:?}"),
    }
    assert!(!state.config().auto_start);
    assert!(state.config().show_dock_icon);
    assert!(!persisted_auto_start(&state));
    assert!(persisted_show_dock_icon(&state));
    assert_eq!(state.settings_revision(), before + 2);
    assert_eq!(auto_start_calls(), vec![true, false]);
    assert_eq!(
        dock.calls(),
        vec![true],
        "first-hook failure must skip the forward Dock sync and still restore Dock"
    );

    drop(state);
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn second_hook_failure_rolls_back_config_and_restores_both_hooks() {
    reset_auto_start_hook();
    let (state, dir) = new_state("host-second-hook");
    state.set_auto_start_sync(recording_auto_start);
    let dock = DockHook::new();
    dock.fail_at(0);
    state.set_dock_visibility_sync(dock.sync());
    let previous = state.config();
    let before = state.settings_revision();
    let mut next = previous.clone();
    next.auto_start = true;
    next.show_dock_icon = false;

    let error = state.apply_host_settings(&previous, next).unwrap_err();
    match error {
        HostSettingsError::Sync(message) => {
            assert_eq!(
                message,
                "failed to synchronize desktop settings: dock hook failed"
            );
        }
        other => panic!("expected Sync, got {other:?}"),
    }
    assert!(!state.config().auto_start);
    assert!(state.config().show_dock_icon);
    assert_eq!(state.settings_revision(), before + 2);
    assert_eq!(auto_start_calls(), vec![true, false]);
    assert_eq!(dock.calls(), vec![false, true]);

    drop(state);
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn hook_restore_failures_append_to_the_sync_error() {
    let (state, dir) = new_state("host-restore-fail");
    state.set_auto_start_sync(always_fail_auto_start);
    let dock = DockHook::new();
    dock.fail_at(0);
    state.set_dock_visibility_sync(dock.sync());
    let previous = state.config();
    let mut next = previous.clone();
    next.auto_start = true;

    let error = state.apply_host_settings(&previous, next).unwrap_err();
    match error {
        HostSettingsError::Sync(message) => {
            assert_eq!(
                message,
                "failed to synchronize desktop settings: auto-start hook failed; failed to restore auto-start state: auto-start hook failed; failed to restore Dock visibility: dock hook failed"
            );
        }
        other => panic!("expected Sync, got {other:?}"),
    }
    assert!(!state.config().auto_start);

    drop(state);
    let _ = fs::remove_dir_all(dir);
}

#[tokio::test]
async fn v2_and_v3_map_unsupported_and_sync_errors_without_shape_drift() {
    let harness = start_loopback("host-http-errors").await;
    let before = harness.state.settings_revision();
    let generation = harness.state.process_generation();
    let primary = harness.state.config().gateway_key.clone();

    let mut unsupported = harness.state.config();
    unsupported.auto_start = true;
    let v2 = harness
        .client
        .post(format!("{}/settings", harness.v2_base))
        .json(&v2_payload(
            &unsupported,
            Some(harness.state.settings_revision()),
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(v2.status(), StatusCode::BAD_REQUEST);
    let v2_body: Value = v2.json().await.unwrap();
    assert_eq!(
        v2_body["error"].as_str(),
        Some(HostSettingsError::AUTO_START_UNAVAILABLE)
    );
    assert!(v2_body.get("code").is_none());
    assert!(v2_body.get("currentRevision").is_none());
    assert_eq!(harness.state.settings_revision(), before);
    assert!(!harness.state.config().auto_start);

    let (status, body) =
        put_json(&harness, &cas_patch(&harness, json!({ "autoStart": true }))).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], ERROR_INVALID_REQUEST);
    assert_eq!(
        body["message"].as_str(),
        Some(HostSettingsError::AUTO_START_UNAVAILABLE)
    );
    assert_eq!(body["currentRevision"], before);
    assert_eq!(body["processGeneration"], generation);
    assert!(body.get("current_revision").is_none());
    assert_eq!(harness.state.settings_revision(), before);

    let mut unsupported_dock = harness.state.config();
    unsupported_dock.show_dock_icon = false;
    let v2 = harness
        .client
        .post(format!("{}/settings", harness.v2_base))
        .json(&v2_payload(&unsupported_dock, None))
        .send()
        .await
        .unwrap();
    assert_eq!(v2.status(), StatusCode::BAD_REQUEST);
    let v2_body: Value = v2.json().await.unwrap();
    assert_eq!(
        v2_body["error"].as_str(),
        Some(HostSettingsError::DOCK_VISIBILITY_UNAVAILABLE)
    );
    assert_eq!(harness.state.settings_revision(), before);
    assert!(harness.state.config().show_dock_icon);

    let (status, body) = put_json(
        &harness,
        &cas_patch(&harness, json!({ "showDockIcon": false })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], ERROR_INVALID_REQUEST);
    assert_eq!(
        body["message"].as_str(),
        Some(HostSettingsError::DOCK_VISIBILITY_UNAVAILABLE)
    );
    assert_eq!(harness.state.settings_revision(), before);

    harness.state.set_auto_start_sync(fail_enable_auto_start);
    let mut failing = harness.state.config();
    failing.auto_start = true;
    let v2 = harness
        .client
        .post(format!("{}/settings", harness.v2_base))
        .json(&v2_payload(
            &failing,
            Some(harness.state.settings_revision()),
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(v2.status(), StatusCode::INTERNAL_SERVER_ERROR);
    let v2_body: Value = v2.json().await.unwrap();
    assert_eq!(
        v2_body["error"],
        "failed to synchronize desktop settings: auto-start hook failed"
    );
    assert!(!harness.state.config().auto_start);
    assert_ne!(harness.state.settings_revision(), before);
    assert_eq!(harness.state.config().gateway_key, primary);

    let after_v2_fail = harness.state.settings_revision();
    let (status, body) =
        put_json(&harness, &cas_patch(&harness, json!({ "autoStart": true }))).await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(body["code"], ERROR_INTERNAL);
    assert_eq!(
        body["message"],
        "failed to synchronize desktop settings: auto-start hook failed"
    );
    assert_eq!(body["currentRevision"], Value::Null);
    assert_eq!(body["processGeneration"], Value::Null);
    assert!(!harness.state.config().auto_start);
    assert_ne!(harness.state.settings_revision(), after_v2_fail);
    assert_eq!(harness.state.process_generation(), generation);
    assert_eq!(harness.state.config().gateway_key, primary);

    harness.stop();
}

#[tokio::test]
async fn v2_and_v3_successful_host_writes_preserve_cas_and_primary_key() {
    let harness = start_loopback("host-http-success").await;
    harness.state.set_auto_start_sync(ok_auto_start);
    let dock = DockHook::new();
    harness.state.set_dock_visibility_sync(dock.sync());
    let before = harness.state.settings_revision();
    let generation = harness.state.process_generation();
    let primary = harness.state.config().gateway_key.clone();

    let mut v2_config = harness.state.config();
    v2_config.auto_start = true;
    let v2 = harness
        .client
        .post(format!("{}/settings", harness.v2_base))
        .json(&v2_payload(&v2_config, None))
        .send()
        .await
        .unwrap();
    assert_eq!(v2.status(), StatusCode::OK);
    let v2_body: Value = v2.json().await.unwrap();
    assert_eq!(v2_body["revision"], before + 1);
    assert!(v2_body.get("processGeneration").is_none());
    assert!(harness.state.config().auto_start);
    assert_eq!(harness.state.config().gateway_key, primary);
    assert_eq!(
        dock.calls(),
        vec![true],
        "V2 auto-start save must reassert the unchanged Dock hook"
    );

    let stale = harness
        .client
        .post(format!("{}/settings", harness.v2_base))
        .json(&v2_payload(&v2_config, Some(before)))
        .send()
        .await
        .unwrap();
    assert_eq!(stale.status(), StatusCode::CONFLICT);
    let stale_body: Value = stale.json().await.unwrap();
    assert_eq!(
        stale_body["error"],
        "settings changed since they were loaded; reload and try again"
    );
    assert_eq!(harness.state.settings_revision(), before + 1);

    let (status, body) = put_json(
        &harness,
        &cas_patch(&harness, json!({ "showDockIcon": false })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["revision"], before + 2);
    assert_eq!(body["processGeneration"], generation);
    assert!(!harness.state.config().show_dock_icon);
    assert!(harness.state.config().auto_start);
    assert_eq!(harness.state.config().gateway_key, primary);
    assert_eq!(dock.calls(), vec![true, false]);

    let (status, body) = put_json(
        &harness,
        &json!({
            "processGeneration": generation,
            "autoStart": false
        }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], ERROR_MISSING_EXPECTED_REVISION);
    assert!(harness.state.config().auto_start);

    let (status, body) = put_json(
        &harness,
        &json!({
            "expectedRevision": harness.state.settings_revision(),
            "processGeneration": generation ^ 1,
            "autoStart": false
        }),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["code"], ERROR_REVISION_CONFLICT);
    assert_eq!(body["currentRevision"], before + 2);
    assert_eq!(body["processGeneration"], generation);
    assert!(harness.state.config().auto_start);

    harness.stop();
}

#[tokio::test(flavor = "current_thread")]
async fn v2_and_v3_reassert_unchanged_supported_hooks_and_rollback_on_drift() {
    reset_auto_start_hook();
    let harness = start_loopback("host-http-reassert").await;
    harness.state.set_auto_start_sync(recording_auto_start);
    let dock = DockHook::new();
    harness.state.set_dock_visibility_sync(dock.sync());
    let before = harness.state.settings_revision();
    let generation = harness.state.process_generation();
    let primary = harness.state.config().gateway_key.clone();

    let mut v2_config = harness.state.config();
    v2_config.connect_timeout_secs = 12;
    let v2 = harness
        .client
        .post(format!("{}/settings", harness.v2_base))
        .json(&v2_payload(&v2_config, None))
        .send()
        .await
        .unwrap();
    assert_eq!(v2.status(), StatusCode::OK);
    let v2_body: Value = v2.json().await.unwrap();
    assert_eq!(v2_body["revision"], before + 1);
    assert_eq!(harness.state.config().connect_timeout_secs, 12);
    assert!(!harness.state.config().auto_start);
    assert!(harness.state.config().show_dock_icon);
    assert_eq!(harness.state.config().gateway_key, primary);
    assert_eq!(auto_start_calls(), vec![false]);
    assert_eq!(dock.calls(), vec![true]);

    let (status, body) = put_json(
        &harness,
        &cas_patch(&harness, json!({ "connectTimeoutSecs": 13 })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["revision"], before + 2);
    assert_eq!(body["processGeneration"], generation);
    assert_eq!(harness.state.config().connect_timeout_secs, 13);
    assert_eq!(auto_start_calls(), vec![false, false]);
    assert_eq!(dock.calls(), vec![true, true]);

    AUTO_START_FAIL_AT.with(|fail| fail.set(Some(auto_start_calls().len())));
    let after_success = harness.state.settings_revision();
    let mut failing = harness.state.config();
    failing.connect_timeout_secs = 14;
    let v2 = harness
        .client
        .post(format!("{}/settings", harness.v2_base))
        .json(&v2_payload(
            &failing,
            Some(harness.state.settings_revision()),
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(v2.status(), StatusCode::INTERNAL_SERVER_ERROR);
    let v2_body: Value = v2.json().await.unwrap();
    assert_eq!(
        v2_body["error"],
        "failed to synchronize desktop settings: auto-start hook failed"
    );
    assert_eq!(harness.state.config().connect_timeout_secs, 13);
    assert!(!harness.state.config().auto_start);
    assert_eq!(harness.state.config().gateway_key, primary);
    assert_ne!(harness.state.settings_revision(), after_success);
    assert_eq!(auto_start_calls(), vec![false, false, false, false]);
    assert_eq!(
        dock.calls(),
        vec![true, true, true],
        "unchanged auto-start drift must skip forward Dock sync and still restore Dock"
    );

    AUTO_START_FAIL_AT.with(|fail| fail.set(None));
    dock.fail_at(dock.calls().len());
    let after_v2_fail = harness.state.settings_revision();
    let (status, body) = put_json(
        &harness,
        &cas_patch(&harness, json!({ "connectTimeoutSecs": 14 })),
    )
    .await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(body["code"], ERROR_INTERNAL);
    assert_eq!(
        body["message"],
        "failed to synchronize desktop settings: dock hook failed"
    );
    assert_eq!(body["currentRevision"], Value::Null);
    assert_eq!(body["processGeneration"], Value::Null);
    assert_eq!(harness.state.config().connect_timeout_secs, 13);
    assert_ne!(harness.state.settings_revision(), after_v2_fail);
    assert_eq!(harness.state.process_generation(), generation);
    assert_eq!(harness.state.config().gateway_key, primary);
    assert_eq!(
        auto_start_calls(),
        vec![false, false, false, false, false, false]
    );
    assert_eq!(dock.calls(), vec![true, true, true, true, true]);

    harness.stop();
}

async fn put_json(harness: &V3Harness, body: &Value) -> (StatusCode, Value) {
    let response = harness
        .client
        .put(format!("{}/settings", harness.v3_base))
        .json(body)
        .send()
        .await
        .unwrap();
    let status = response.status();
    let body = response.json().await.unwrap_or(Value::Null);
    (status, body)
}

fn free_port() -> u16 {
    TcpListener::bind(("127.0.0.1", 0))
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

#[tokio::test]
async fn http_v3_port_change_rebinds_running_listener_or_keeps_old_on_failure() {
    let (state, dir) = new_state("http-port-rebind");
    let handle = gateway::start_gateway_on(state.clone(), SocketAddr::from(([127, 0, 0, 1], 0)))
        .await
        .unwrap();
    let old_port = handle.port;
    *state.gateway.lock() = Some(handle);
    assert!(TcpStream::connect(("127.0.0.1", old_port)).is_ok());

    let client = loopback_client();
    let new_port = free_port();
    let response = client
        .put(format!(
            "http://127.0.0.1:{old_port}/dashboard/api/v3/settings"
        ))
        .json(&json!({
            "expectedRevision": state.settings_revision(),
            "processGeneration": state.process_generation(),
            "gatewayPort": new_port,
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK, "{response:?}");
    assert_eq!(state.config().gateway_port, new_port);
    assert_eq!(state.active_gateway_port(), new_port);
    assert!(
        TcpStream::connect(("127.0.0.1", new_port)).is_ok(),
        "settings port change must bind the new listener before returning"
    );

    let occupied = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let occupied_port = occupied.local_addr().unwrap().port();
    let fail = client
        .put(format!(
            "http://127.0.0.1:{new_port}/dashboard/api/v3/settings"
        ))
        .json(&json!({
            "expectedRevision": state.settings_revision(),
            "processGeneration": state.process_generation(),
            "gatewayPort": occupied_port,
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(fail.status(), StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(state.active_gateway_port(), new_port);
    assert_eq!(state.config().gateway_port, new_port);
    assert!(TcpStream::connect(("127.0.0.1", new_port)).is_ok());

    if let Some(handle) = state.gateway.lock().take() {
        gateway::stop_gateway(handle);
    }
    drop(occupied);
    drop(state);
    let _ = fs::remove_dir_all(dir);
}

#[tokio::test]
async fn failed_rebind_compensation_skips_unrelated_successful_commit() {
    let (state, dir) = new_state("compensate-fingerprint");
    let previous = state.config();
    let original_timeout = previous.connect_timeout_secs;
    let occupied = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let occupied_port = occupied.local_addr().unwrap().port();

    let mut failed_port = previous.clone();
    failed_port.gateway_port = occupied_port;
    state
        .apply_host_settings(&previous, failed_port.clone())
        .expect("A persist must succeed before rebind");
    let committed_revision = state.settings_revision();
    let committed = state.config();
    assert_eq!(committed.gateway_port, occupied_port);

    let mut timeout_write = previous.clone();
    timeout_write.connect_timeout_secs = 12;
    state
        .apply_host_settings(&committed, timeout_write)
        .expect("B timeout persist must succeed");
    assert_eq!(state.config().connect_timeout_secs, 12);
    assert_eq!(state.config().gateway_port, previous.gateway_port);

    let restored = state
        .compensate_failed_listener_rebind(&committed, previous.clone(), committed_revision)
        .expect("compensation decision should not fail");
    assert!(
        !restored,
        "compensation must not overwrite B's successful timeout commit"
    );
    assert_eq!(state.config().connect_timeout_secs, 12);
    assert_eq!(state.config().gateway_port, previous.gateway_port);
    assert_ne!(state.config().connect_timeout_secs, original_timeout);

    drop(occupied);
    drop(state);
    let _ = fs::remove_dir_all(dir);
}

#[tokio::test]
async fn failed_rebind_compensation_still_restores_after_revision_only_bump() {
    let (state, dir) = new_state("compensate-revision-only");
    let previous = state.config();
    let occupied = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let occupied_port = occupied.local_addr().unwrap().port();

    let mut failed_port = previous.clone();
    failed_port.gateway_port = occupied_port;
    state
        .apply_host_settings(&previous, failed_port)
        .expect("A persist must succeed before rebind");
    let committed_revision = state.settings_revision();
    let committed = state.config();
    state.bump_settings_revision();
    assert_ne!(state.settings_revision(), committed_revision);

    let restored = state
        .compensate_failed_listener_rebind(&committed, previous.clone(), committed_revision)
        .expect("port restore should persist");
    assert!(
        restored,
        "account/key revision bumps must not skip restore of a failed port write"
    );
    assert_eq!(state.config().gateway_port, previous.gateway_port);

    drop(occupied);
    drop(state);
    let _ = fs::remove_dir_all(dir);
}

#[tokio::test]
async fn failed_http_rebind_releases_sync_gate_and_preserves_later_key_and_claude_writes() {
    let (state, dir) = new_state("http-compensate-live-config");
    let (held_port, shutdown_seen, release) = install_held_listener(&state).await;

    // Hold gateway_lifecycle in a normal wait-for-old transition. The new
    // listener is installed before the held old listener is released, so it
    // can serve the settings and independent writer probes below.
    let lifecycle_state = state.clone();
    let lifecycle = tokio::spawn(async move {
        GatewayLifecycle::rebind(lifecycle_state, SocketAddr::from(([127, 0, 0, 1], 0))).await
    });
    tokio::time::timeout(Duration::from_secs(2), shutdown_seen)
        .await
        .expect("lifecycle rebind should signal the held listener")
        .expect("shutdown observation channel should stay open");
    let serving_port = state.active_gateway_port();
    assert_ne!(serving_port, held_port);

    // Keep configured/listener state aligned before starting the failed write.
    {
        let _settings_update = state.settings_update.lock();
        let mut config = state.config();
        config.gateway_port = serving_port;
        state
            .set_config(config)
            .expect("port alignment should persist");
    }

    let occupied = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let failed_port = occupied.local_addr().unwrap().port();
    let client = loopback_client();
    let settings_state = state.clone();
    let settings_client = client.clone();
    let failed_settings = tokio::spawn(async move {
        settings_client
            .put(format!(
                "http://127.0.0.1:{serving_port}/dashboard/api/v3/settings"
            ))
            .json(&json!({
                "expectedRevision": settings_state.settings_revision(),
                "processGeneration": settings_state.process_generation(),
                "gatewayPort": failed_port,
            }))
            .send()
            .await
            .expect("failed-port settings request should receive a response")
    });

    // The port persist happens before waiting on gateway_lifecycle. Once it is
    // visible, settings_update must already be released.
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            if state.config().gateway_port == failed_port {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("settings request should persist before its listener await");

    let claude = tokio::time::timeout(
        Duration::from_secs(2),
        client
            .put(format!(
                "http://127.0.0.1:{serving_port}/dashboard/api/v3/claude-desktop/models"
            ))
            .json(&json!({
                "expectedRevision": state.settings_revision(),
                "processGeneration": state.process_generation(),
                "sonnet": "glm-5.2",
                "opus": "",
                "haiku": "",
            }))
            .send(),
    )
    .await
    .expect("Claude writer must not block on settings_update across the listener await")
    .expect("Claude writer should receive a response");
    assert_eq!(claude.status(), StatusCode::OK);

    let primary_before = state.config().gateway_key;
    let key = tokio::time::timeout(
        Duration::from_secs(2),
        client
            .post(format!(
                "http://127.0.0.1:{serving_port}/dashboard/api/v3/keys/primary/regenerate"
            ))
            .json(&json!({
                "expectedRevision": state.settings_revision(),
                "processGeneration": state.process_generation(),
            }))
            .send(),
    )
    .await
    .expect("primary-Key writer must not block on settings_update across the listener await")
    .expect("primary-Key writer should receive a response");
    assert_eq!(key.status(), StatusCode::OK);
    let primary_after = state.config().gateway_key;
    assert_ne!(primary_after, primary_before);
    let revision_after_writers = state.settings_revision();

    release.wait().await;
    let installed_port = lifecycle
        .await
        .expect("lifecycle task should finish")
        .expect("lifecycle rebind should succeed");
    assert_eq!(installed_port, serving_port);
    let failed_settings = failed_settings
        .await
        .expect("settings task should finish after the lifecycle gate opens");
    assert_eq!(failed_settings.status(), StatusCode::INTERNAL_SERVER_ERROR);

    let live = state.config();
    assert_eq!(live.gateway_port, serving_port);
    assert_eq!(state.active_gateway_port(), serving_port);
    assert_eq!(live.gateway_key, primary_after);
    assert_eq!(live.claude_desktop_models.sonnet, "glm-5.2");
    assert_eq!(
        state.settings_revision(),
        revision_after_writers + 1,
        "port-only compensation must make one monotonic persisted revision"
    );
    let stored = state.db.lock().get_setting("config").unwrap().unwrap();
    let stored: ocg_core::models::AppConfig = serde_json::from_str(&stored).unwrap();
    assert_eq!(stored.gateway_port, serving_port);
    assert_eq!(stored.gateway_key, primary_after);
    assert_eq!(stored.claude_desktop_models.sonnet, "glm-5.2");

    let handle = state.gateway.lock().take();
    if let Some(handle) = handle {
        let _ = GatewayLifecycle::stop_and_wait(handle).await;
    }
    drop(occupied);
    drop(state);
    let _ = fs::remove_dir_all(dir);
}

#[tokio::test]
async fn concurrent_port_changes_keep_configured_and_active_ports_in_agreement() {
    let (state, dir) = new_state("concurrent-two-ports");
    let handle = gateway::start_gateway_on(state.clone(), SocketAddr::from(([127, 0, 0, 1], 0)))
        .await
        .unwrap();
    *state.gateway.lock() = Some(handle);

    let first = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let second = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let first_port = first.local_addr().unwrap().port();
    let second_port = second.local_addr().unwrap().port();
    assert_ne!(first_port, second_port);
    drop(first);
    drop(second);
    let start = Arc::new(tokio::sync::Barrier::new(3));

    let a_state = state.clone();
    let a_start = start.clone();
    let a = tokio::spawn(async move {
        a_start.wait().await;
        let previous = a_state.config();
        let mut next = previous.clone();
        next.gateway_port = first_port;
        a_state
            .apply_host_settings_and_rebind_listener(previous, next, false)
            .await
            .map(|_| a_state.active_gateway_port())
    });

    let b_state = state.clone();
    let b_start = start.clone();
    let b = tokio::spawn(async move {
        b_start.wait().await;
        let previous = b_state.config();
        let mut next = previous.clone();
        next.gateway_port = second_port;
        b_state
            .apply_host_settings_and_rebind_listener(previous, next, false)
            .await
            .map(|_| b_state.active_gateway_port())
    });

    start.wait().await;
    let (a, b) = tokio::join!(a, b);
    let a = a.expect("first port task should finish");
    let b = b.expect("second port task should finish");
    assert!(
        a.is_ok() && b.is_ok(),
        "both concurrent port changes should succeed: {a:?} {b:?}"
    );
    let configured = state.config().gateway_port;
    let active = state.active_gateway_port();
    assert_eq!(
        configured, active,
        "final configured port must agree with the installed listener"
    );
    assert!(
        configured == first_port || configured == second_port,
        "final port {configured} must be one of the concurrent writes"
    );
    assert!(TcpStream::connect(("127.0.0.1", active)).is_ok());

    if let Some(handle) = state.gateway.lock().take() {
        gateway::stop_gateway(handle);
    }
    drop(state);
    let _ = fs::remove_dir_all(dir);
}

#[tokio::test]
async fn http_v2_and_v3_concurrent_port_changes_agree_on_configured_and_active_port() {
    let (state, dir) = new_state("http-concurrent-ports");
    let handle = gateway::start_gateway_on(state.clone(), SocketAddr::from(([127, 0, 0, 1], 0)))
        .await
        .unwrap();
    let old_port = handle.port;
    *state.gateway.lock() = Some(handle);
    let first = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let second = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let first_port = first.local_addr().unwrap().port();
    let second_port = second.local_addr().unwrap().port();
    assert_ne!(first_port, second_port);
    drop(first);
    drop(second);
    let client = loopback_client();
    let start = Arc::new(tokio::sync::Barrier::new(3));

    let a_state = state.clone();
    let a_client = client.clone();
    let a_start = start.clone();
    let a = tokio::spawn(async move {
        a_start.wait().await;
        let mut body = a_state.config();
        body.gateway_port = first_port;
        a_client
            .post(format!(
                "http://127.0.0.1:{old_port}/dashboard/api/settings"
            ))
            .timeout(Duration::from_secs(5))
            .json(&v2_payload(&body, None))
            .send()
            .await
            .unwrap()
    });

    let b_state = state.clone();
    let b_client = client.clone();
    let b_start = start.clone();
    let b = tokio::spawn(async move {
        b_start.wait().await;
        b_client
            .put(format!(
                "http://127.0.0.1:{old_port}/dashboard/api/v3/settings"
            ))
            .timeout(Duration::from_secs(5))
            .json(&json!({
                "expectedRevision": b_state.settings_revision(),
                "processGeneration": b_state.process_generation(),
                "gatewayPort": second_port,
            }))
            .send()
            .await
            .unwrap()
    });

    start.wait().await;
    let (a, b) = tokio::join!(a, b);
    let a = a.expect("V2 port task should finish");
    let b = b.expect("V3 port task should finish");
    let a_ok = a.status() == StatusCode::OK;
    let b_ok = b.status() == StatusCode::OK;
    assert!(
        a_ok || b_ok,
        "at least one concurrent port write must succeed: {} {}",
        a.status(),
        b.status()
    );
    if a_ok && !b_ok {
        let a_body: Value = a.json().await.unwrap();
        assert_eq!(a_body["revision"], state.settings_revision());
    }
    if b_ok && !a_ok {
        let b_body: Value = b.json().await.unwrap();
        assert_eq!(b_body["revision"], state.settings_revision());
        assert_eq!(b_body["processGeneration"], state.process_generation());
    }
    let configured = state.config().gateway_port;
    let active = state.active_gateway_port();
    assert_eq!(configured, active);
    assert!(
        configured == first_port || configured == second_port,
        "final port {configured} must be one of the concurrent writes"
    );
    assert!(TcpStream::connect(("127.0.0.1", active)).is_ok());

    if let Some(handle) = state.gateway.lock().take() {
        gateway::stop_gateway(handle);
    }
    drop(state);
    let _ = fs::remove_dir_all(dir);
}
