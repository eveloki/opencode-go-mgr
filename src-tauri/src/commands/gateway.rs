use crate::state::AppState;
use ocg_core::gateway;
use ocg_core::models::{AppConfig, GatewayStatus};
use ocg_core::state::CoreState;
use std::net::SocketAddr;
use std::sync::{Mutex, MutexGuard, PoisonError};
use tauri::State;

/// Process-local Tauri adapter gate covering a restart through status, log,
/// and return. Core `GatewayLifecycle::rebind` serializes only until it
/// returns, which would otherwise let another wrapper stop this listener
/// before the caller observes success.
static GATEWAY_RESTART_GATE: Mutex<()> = Mutex::new(());

fn lock_gateway_restart_gate() -> MutexGuard<'static, ()> {
    // A panicked restart must not pin the adapter; recover the guard so a
    // later call can fail on bind/slot state instead of hanging.
    GATEWAY_RESTART_GATE
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
}

#[tauri::command]
pub fn get_gateway_status(state: State<'_, AppState>) -> Result<GatewayStatus, String> {
    let config = state.core.config();
    let running = state.core.gateway.lock().is_some();
    Ok(status_from_config(&state.core, running, &config))
}

pub(super) fn status_from_config(
    core: &CoreState,
    running: bool,
    config: &AppConfig,
) -> GatewayStatus {
    let last_error = if running {
        None
    } else {
        core.db.lock().latest_gateway_error().ok().flatten()
    };
    GatewayStatus {
        running,
        port: core.active_gateway_port(),
        key: config.gateway_key.clone(),
        upstream_base_url: config.upstream_base_url.clone(),
        last_error,
    }
}

pub(super) fn restart_inner(core: &CoreState, config: &AppConfig) -> Result<GatewayStatus, String> {
    #[cfg(test)]
    observe_restart_entry(core, config.gateway_port);
    let _restart_gate = lock_gateway_restart_gate();
    #[cfg(test)]
    observe_restart_gate_acquired(core, config.gateway_port);
    let status = restart_inner_locked(core, config)?;
    #[cfg(test)]
    pause_restart_return_if_requested(core, status.port);
    Ok(status)
}

fn restart_inner_locked(core: &CoreState, config: &AppConfig) -> Result<GatewayStatus, String> {
    let addr = SocketAddr::from(([127, 0, 0, 1], config.gateway_port));
    let port = match tauri::async_runtime::block_on(gateway::GatewayLifecycle::rebind(
        std::sync::Arc::clone(core),
        addr,
    )) {
        Ok(port) => port,
        Err(e) => {
            let message = format!(
                "failed to restart gateway on port {}: {}",
                config.gateway_port, e
            );
            let _ = core.db.lock().log_gateway("error", "gateway", &message);
            return Err(message);
        }
    };

    // Report this transition's installed port rather than re-reading the slot.
    // The adapter gate above keeps another wrapper from replacing this
    // listener before status and the success log are written.
    let mut status = status_from_config(core, true, config);
    status.port = port;

    let _ = core.db.lock().log_gateway(
        "info",
        "gateway",
        &format!("gateway restarted on port {}", port),
    );
    Ok(status)
}

#[tauri::command]
pub fn restart_gateway(state: State<'_, AppState>) -> Result<GatewayStatus, String> {
    let config = state.core.config();
    restart_inner(&state.core, &config)
}

#[cfg(test)]
pub(crate) fn stop_and_wait(handle: ocg_core::state::GatewayHandle) {
    let _ = tauri::async_runtime::block_on(gateway::GatewayLifecycle::stop_and_wait(handle));
}

#[cfg(test)]
struct RestartPortObserver {
    core_id: usize,
    port: u16,
    observed: std::sync::mpsc::SyncSender<()>,
}

#[cfg(test)]
struct RestartReturnPause {
    core_id: usize,
    port: u16,
    entered: std::sync::mpsc::SyncSender<()>,
    release: std::sync::mpsc::Receiver<()>,
}

#[cfg(test)]
static RESTART_ENTRY_OBSERVER: Mutex<Option<RestartPortObserver>> = Mutex::new(None);
#[cfg(test)]
static RESTART_GATE_ACQUIRED_OBSERVER: Mutex<Option<RestartPortObserver>> = Mutex::new(None);
#[cfg(test)]
static RESTART_RETURN_PAUSE: Mutex<Option<RestartReturnPause>> = Mutex::new(None);

#[cfg(test)]
fn core_id(core: &CoreState) -> usize {
    std::sync::Arc::as_ptr(core) as usize
}

#[cfg(test)]
fn take_matching_observer(
    slot: &Mutex<Option<RestartPortObserver>>,
    core: &CoreState,
    port: u16,
) -> Option<RestartPortObserver> {
    let mut slot = slot.lock().unwrap_or_else(PoisonError::into_inner);
    if slot
        .as_ref()
        .is_some_and(|observer| observer.core_id == core_id(core) && observer.port == port)
    {
        slot.take()
    } else {
        None
    }
}

#[cfg(test)]
fn observe_restart_entry(core: &CoreState, port: u16) {
    if let Some(observer) = take_matching_observer(&RESTART_ENTRY_OBSERVER, core, port) {
        let _ = observer.observed.send(());
    }
}

#[cfg(test)]
fn observe_restart_gate_acquired(core: &CoreState, port: u16) {
    if let Some(observer) = take_matching_observer(&RESTART_GATE_ACQUIRED_OBSERVER, core, port) {
        let _ = observer.observed.send(());
    }
}

#[cfg(test)]
fn pause_restart_return_if_requested(core: &CoreState, port: u16) {
    let pause = {
        let mut slot = RESTART_RETURN_PAUSE
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if slot
            .as_ref()
            .is_some_and(|pause| pause.core_id == core_id(core) && pause.port == port)
        {
            slot.take()
        } else {
            None
        }
    };
    if let Some(pause) = pause {
        let _ = pause.entered.send(());
        let _ = pause.release.recv();
    }
}

#[cfg(test)]
mod tests {
    use super::{restart_inner, stop_and_wait};
    use ocg_core::crypto::{KeyCipher, StaticKeyCipher};
    use ocg_core::db::Database;
    use ocg_core::gateway;
    use ocg_core::state::{CoreState, CoreStateInner};
    use std::fs;
    use std::net::{TcpListener, TcpStream};
    use std::path::PathBuf;
    use std::sync::Arc;

    fn temp_data_dir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ocg-restart-test-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn free_port() -> u16 {
        TcpListener::bind(("127.0.0.1", 0))
            .unwrap()
            .local_addr()
            .unwrap()
            .port()
    }

    #[test]
    fn failed_port_change_keeps_old_gateway_running() {
        let dir = temp_data_dir();
        let cipher: Arc<dyn KeyCipher + Send + Sync> = Arc::new(StaticKeyCipher::new("test"));
        let db = Database::open(dir.clone()).unwrap();
        let state = Arc::new(CoreStateInner::new(db, dir.clone(), cipher).unwrap());
        let old_port = free_port();
        let old_handle =
            tauri::async_runtime::block_on(gateway::start_gateway(state.clone(), old_port))
                .unwrap();
        *state.gateway.lock() = Some(old_handle);

        let occupied = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let mut config = state.config();
        config.gateway_port = occupied.local_addr().unwrap().port();

        let err = restart_inner(&state, &config).unwrap_err();
        assert!(
            err.starts_with(&format!(
                "failed to restart gateway on port {}",
                config.gateway_port
            )),
            "{err}"
        );
        assert_eq!(state.active_gateway_port(), old_port);
        assert!(TcpStream::connect(("127.0.0.1", old_port)).is_ok());

        stop_and_wait(state.gateway.lock().take().unwrap());
        drop(occupied);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn status_from_config_reports_running_and_stopped_error() {
        use super::status_from_config;

        let dir = temp_data_dir();
        let cipher: Arc<dyn KeyCipher + Send + Sync> = Arc::new(StaticKeyCipher::new("test"));
        let db = Database::open(dir.clone()).unwrap();
        let state = Arc::new(CoreStateInner::new(db, dir.clone(), cipher).unwrap());
        state
            .db
            .lock()
            .log_gateway("error", "gateway", "boom")
            .unwrap();
        let config = state.config();

        let stopped = status_from_config(&state, false, &config);
        assert!(!stopped.running);
        assert_eq!(stopped.last_error.as_deref(), Some("boom"));
        assert_eq!(stopped.key, config.gateway_key);

        let port = free_port();
        let handle =
            tauri::async_runtime::block_on(gateway::start_gateway(state.clone(), port)).unwrap();
        *state.gateway.lock() = Some(handle);
        let running = status_from_config(&state, true, &config);
        assert!(running.running);
        assert!(running.last_error.is_none());
        assert_eq!(running.port, port);

        stop_and_wait(state.gateway.lock().take().unwrap());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn restart_inner_same_port_and_new_port_succeed() {
        let dir = temp_data_dir();
        let cipher: Arc<dyn KeyCipher + Send + Sync> = Arc::new(StaticKeyCipher::new("test"));
        let db = Database::open(dir.clone()).unwrap();
        let state = Arc::new(CoreStateInner::new(db, dir.clone(), cipher).unwrap());
        let first_port = free_port();
        let mut config = state.config();
        config.gateway_port = first_port;

        let first = restart_inner(&state, &config).unwrap();
        assert!(first.running);
        assert_eq!(first.port, first_port);
        assert!(TcpStream::connect(("127.0.0.1", first_port)).is_ok());

        let same = restart_inner(&state, &config).unwrap();
        assert!(same.running);
        assert_eq!(same.port, first_port);

        let second_port = free_port();
        config.gateway_port = second_port;
        let moved = restart_inner(&state, &config).unwrap();
        assert!(moved.running);
        assert_eq!(moved.port, second_port);
        assert!(TcpStream::connect(("127.0.0.1", second_port)).is_ok());

        stop_and_wait(state.gateway.lock().take().unwrap());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn restart_inner_is_a_listener_lifecycle_and_does_not_bump_revision() {
        let dir = temp_data_dir();
        let cipher: Arc<dyn KeyCipher + Send + Sync> = Arc::new(StaticKeyCipher::new("test"));
        let db = Database::open(dir.clone()).unwrap();
        let state = Arc::new(CoreStateInner::new(db, dir.clone(), cipher).unwrap());
        let mut config = state.config();
        config.gateway_port = free_port();
        let revision = state.settings_revision();

        let started = restart_inner(&state, &config).unwrap();
        assert!(started.running);
        assert_eq!(state.settings_revision(), revision);
        assert!(TcpStream::connect(("127.0.0.1", config.gateway_port)).is_ok());

        let restarted = restart_inner(&state, &config).unwrap();
        assert!(restarted.running);
        assert_eq!(restarted.port, config.gateway_port);
        assert_eq!(state.settings_revision(), revision);

        stop_and_wait(state.gateway.lock().take().unwrap());
        assert!(state.gateway.lock().is_none());
        assert_eq!(state.settings_revision(), revision);
        let _ = state.db.lock().list_accounts().unwrap();

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn restart_callers_delegate_whole_transition_to_gateway_lifecycle() {
        let gateway_src = include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/commands/gateway.rs"
        ));
        let production = gateway_src
            .split("pub(crate) fn stop_and_wait")
            .next()
            .expect("production gateway commands");
        assert!(
            production.contains("GatewayLifecycle::rebind"),
            "restart_inner must delegate the whole transition to GatewayLifecycle::rebind"
        );
        assert!(
            production.contains("static GATEWAY_RESTART_GATE: Mutex<()>")
                && production.contains("lock_gateway_restart_gate")
                && production.contains("PoisonError::into_inner"),
            "Tauri adapter must own a process-local non-async restart gate with deterministic poison recovery"
        );
        assert_eq!(
            production.matches("lock_gateway_restart_gate()").count(),
            2,
            "adapter gate definition plus one acquire in restart_inner; do not lock it recursively"
        );
        assert!(
            production.contains("let _restart_gate = lock_gateway_restart_gate();")
                && production.contains("restart_inner_locked("),
            "restart_inner must hold the adapter gate through the locked transition"
        );
        assert!(
            !production.contains("tokio::sync::Mutex"),
            "adapter gate must not be an async mutex"
        );
        assert!(
            production.contains("restart_inner(&state.core, &config)"),
            "restart_gateway must keep calling restart_inner"
        );
        assert!(
            !production.contains("gateway::start_gateway")
                && !production.contains("::start_gateway"),
            "duplicate start_gateway bind path must be gone from restart"
        );
        assert!(
            !production.contains("tokio::time::timeout")
                && !production.contains("Duration::from_secs(5)"),
            "duplicate Tauri stop timeout must be gone from restart"
        );
        assert!(
            !production.contains(".replace("),
            "duplicate slot replace must be gone from restart"
        );

        let settings_src = include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/commands/setting.rs"
        ));
        let settings_production = settings_src
            .split("#[cfg(test)]")
            .next()
            .expect("production settings commands");
        assert!(
            settings_production.contains("commands::gateway::restart_inner"),
            "settings port-change restart must go through restart_inner"
        );
        assert!(
            !settings_production.contains("gateway::start_gateway")
                && !settings_production.contains("GatewayLifecycle::"),
            "settings must not bind or rebind the listener itself"
        );
    }

    fn install_restart_observer(
        slot: &'static std::sync::Mutex<Option<super::RestartPortObserver>>,
        core: &CoreState,
        port: u16,
    ) -> std::sync::mpsc::Receiver<()> {
        let (observed, receiver) = std::sync::mpsc::sync_channel(0);
        let mut slot = slot
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        assert!(
            slot.is_none(),
            "only one observer may be installed per stage"
        );
        *slot = Some(super::RestartPortObserver {
            core_id: super::core_id(core),
            port,
            observed,
        });
        receiver
    }

    fn install_restart_return_pause(
        core: &CoreState,
        port: u16,
    ) -> (
        std::sync::mpsc::Receiver<()>,
        std::sync::mpsc::SyncSender<()>,
    ) {
        let (entered, entered_receiver) = std::sync::mpsc::sync_channel(0);
        let (release, release_receiver) = std::sync::mpsc::sync_channel(0);
        let mut slot = super::RESTART_RETURN_PAUSE
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        assert!(slot.is_none(), "only one return pause may be installed");
        *slot = Some(super::RestartReturnPause {
            core_id: super::core_id(core),
            port,
            entered,
            release: release_receiver,
        });
        (entered_receiver, release)
    }

    #[test]
    fn concurrent_restart_inner_serializes_through_return_and_log() {
        let dir = temp_data_dir();
        let cipher: Arc<dyn KeyCipher + Send + Sync> = Arc::new(StaticKeyCipher::new("test"));
        let db = Database::open(dir.clone()).unwrap();
        let state = Arc::new(CoreStateInner::new(db, dir.clone(), cipher).unwrap());
        let first_port = free_port();
        let mut config = state.config();
        config.gateway_port = first_port;
        let started = restart_inner(&state, &config).unwrap();
        assert_eq!(started.port, first_port);

        let port_a = distinct_free_port(&[first_port]);
        let port_b = distinct_free_port(&[first_port, port_a]);
        let state_a = state.clone();
        let state_b = state.clone();
        let mut config_a = config.clone();
        config_a.gateway_port = port_a;
        let mut config_b = config.clone();
        config_b.gateway_port = port_b;

        // These hooks are in the real `restart_inner` path: A pauses only
        // after rebind/status/log, while B reports before and after acquiring
        // the production adapter gate.
        let (a_paused, release_a) = install_restart_return_pause(&state, port_a);
        let b_entered = install_restart_observer(&super::RESTART_ENTRY_OBSERVER, &state, port_b);
        let b_acquired_gate =
            install_restart_observer(&super::RESTART_GATE_ACQUIRED_OBSERVER, &state, port_b);
        let thread_a = std::thread::spawn(move || restart_inner(&state_a, &config_a));
        a_paused
            .recv_timeout(std::time::Duration::from_secs(5))
            .expect("A must pause after its real restart transition");
        assert_eq!(state.active_gateway_port(), port_a);
        assert!(TcpStream::connect(("127.0.0.1", port_a)).is_ok());

        let (b_finished, b_result) = std::sync::mpsc::channel();
        let thread_b = std::thread::spawn(move || {
            let result = restart_inner(&state_b, &config_b);
            let _ = b_finished.send(result);
        });
        b_entered
            .recv_timeout(std::time::Duration::from_secs(5))
            .expect("B must enter the real restart_inner before trying the gate");
        match b_acquired_gate.recv_timeout(std::time::Duration::from_millis(250)) {
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
            Ok(()) => panic!("B acquired the adapter gate before A returned"),
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                panic!("B gate observer disconnected before A returned")
            }
        }
        assert!(
            matches!(
                b_result.try_recv(),
                Err(std::sync::mpsc::TryRecvError::Empty)
            ),
            "B finished before A released the adapter gate"
        );

        release_a.send(()).expect("release A return pause");
        b_acquired_gate
            .recv_timeout(std::time::Duration::from_secs(5))
            .expect("B must acquire the adapter gate after A returns");
        let status_a = thread_a.join().expect("restart thread a").unwrap();
        let status_b = b_result
            .recv_timeout(std::time::Duration::from_secs(5))
            .expect("restart thread b result")
            .unwrap();
        thread_b.join().expect("restart thread b");

        assert!(status_a.running);
        assert!(status_b.running);
        assert_eq!(status_a.port, port_a);
        assert_eq!(status_b.port, port_b);

        let logs = state.db.lock().list_gateway_logs(32).unwrap();
        let restart_ports: Vec<u16> = logs
            .iter()
            .filter(|log| log.category == "gateway" && log.level == "info")
            .filter_map(|log| {
                log.message
                    .strip_prefix("gateway restarted on port ")
                    .and_then(|port| port.parse().ok())
            })
            .collect();
        assert!(
            restart_ports.contains(&port_a) && restart_ports.contains(&port_b),
            "each wrapper must write its success log, got {restart_ports:?}"
        );
        let last_logged = restart_ports[0];
        let installed = state.active_gateway_port();
        assert_eq!(
            last_logged, installed,
            "last success log must match the still-installed port"
        );
        assert_eq!(installed, port_b, "B must be the final installed restart");
        assert!(TcpStream::connect(("127.0.0.1", installed)).is_ok());
        assert!(TcpStream::connect(("127.0.0.1", port_a)).is_err());

        stop_and_wait(state.gateway.lock().take().unwrap());
        let _ = fs::remove_dir_all(dir);
    }

    fn distinct_free_port(exclude: &[u16]) -> u16 {
        for _ in 0..32 {
            let port = free_port();
            if !exclude.contains(&port) {
                return port;
            }
        }
        panic!("could not allocate a distinct free port");
    }
}
