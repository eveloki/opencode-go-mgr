use crate::state::AppState;
use ocg_core::models::{
    AppConfig, GatewayStatus, PRIMARY_KEY_REQUIRED_MESSAGE, normalize_client_root_url,
};
use ocg_core::state::CoreState;
use tauri::State;

#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> Result<AppConfig, String> {
    get_settings_inner(&state.core)
}

pub(crate) fn get_settings_inner(core: &CoreState) -> Result<AppConfig, String> {
    Ok(core.settings_config())
}

#[tauri::command]
pub fn update_settings(
    state: State<'_, AppState>,
    mut config: AppConfig,
) -> Result<GatewayStatus, String> {
    update_settings_inner(&state.core, &mut config, true)
}

pub(crate) fn update_settings_inner(
    core: &CoreState,
    config: &mut AppConfig,
    sync_auto_start: bool,
) -> Result<GatewayStatus, String> {
    let _settings_update = core.settings_update.lock();
    config.gateway_key = config.gateway_key.trim().to_string();
    if config.gateway_key.is_empty() {
        return Err(PRIMARY_KEY_REQUIRED_MESSAGE.to_string());
    }
    // Same unified cross-tier gate as the dashboard settings update: the
    // primary value must differ from every non-deleted sub key's value.
    {
        let db = core.db.lock();
        ocg_core::gateway_keys::ensure_primary_value_allowed(&db, &config.gateway_key)
            .map_err(|error| error.to_string())?;
    }
    config.validate_timeouts()?;
    validate_upstream_url(&config.upstream_base_url)?;
    config.client_root_url = normalize_client_root_url(&config.client_root_url)?;
    // ponytail: only restart if the port actually changed. Gateway key and
    // upstream URL are already live
    // — handler.rs reads state.config() per request. Restarting on every save
    // would drop in-flight requests for no reason.
    // ponytail: if the gateway is not running, do not start it here; the next
    // manual start will pick up the new port from config.
    // ponytail: probe-bind the new port BEFORE we touch the DB or in-memory
    // config. If the bind fails, the old config stays put and the gateway keeps
    // serving on the old port — no "save failed with gateway down" regression.
    let old_port = core.config().gateway_port;
    let port_changed = old_port != config.gateway_port;
    let was_running = {
        let gw = core.gateway.lock();
        gw.is_some()
    };

    if port_changed {
        // ponytail: skip the TOCTOU pre-bind. The previous code opened a probe
        // TcpListener, dropped it, then called start_gateway — a classic
        // race-stop window where another process could grab the port between
        // probe and bind. Instead, write the new config only AFTER a
        // successful restart, so a failed bind leaves the in-memory and
        // on-disk configs on the old port.
        if was_running {
            match crate::commands::gateway::restart_inner(core, config) {
                Ok(status) => {
                    if sync_auto_start {
                        crate::autostart::sync(config.auto_start).map_err(|e| e.to_string())?;
                    }
                    core.set_config(config.clone()).map_err(|e| e.to_string())?;
                    return Ok(status);
                }
                Err(e) => {
                    let _ = core.db.lock().log_gateway(
                        "warn",
                        "settings",
                        &format!("port change to {} failed: {}", config.gateway_port, e),
                    );
                    return Err(e);
                }
            }
        }
    }

    if sync_auto_start {
        crate::autostart::sync(config.auto_start).map_err(|e| e.to_string())?;
    }
    core.set_config(config.clone()).map_err(|e| e.to_string())?;
    let _ = core
        .db
        .lock()
        .log_gateway("info", "settings", "settings updated");

    let snapshot = core.config();
    Ok(crate::commands::gateway::status_from_config(
        core,
        was_running,
        &snapshot,
    ))
}

#[tauri::command]
pub fn regenerate_gateway_key(state: State<'_, AppState>) -> Result<String, String> {
    regenerate_gateway_key_inner(&state.core)
}

pub(crate) fn regenerate_gateway_key_inner(core: &CoreState) -> Result<String, String> {
    let _settings_update = core.settings_update.lock();
    // Converged on the primary key: the dashboard endpoint and this command
    // share one rotation path (set_config refreshes the credential snapshot).
    let new_value = {
        let db = core.db.lock();
        ocg_core::gateway_keys::generate_primary_value(&db, &core.config().gateway_key)
            .map_err(|error| error.to_string())?
    };
    let mut config = core.config();
    config.gateway_key = new_value;
    core.set_config(config).map_err(|e| e.to_string())?;
    let _ = core.db.lock().log_gateway(
        "info",
        "keys",
        &format!(
            "regenerated primary key `{}`",
            ocg_core::gateway_keys::PRIMARY_KEY_NAME
        ),
    );
    Ok(core.config().gateway_key)
}

fn validate_upstream_url(url: &str) -> Result<(), String> {
    let parsed = tauri::Url::parse(url).map_err(|e| format!("invalid upstream URL: {}", e))?;
    match parsed.scheme() {
        "https" => Ok(()),
        "http" if is_loopback(&parsed) => Ok(()),
        _ => Err("upstream must use https, except loopback http for local development".to_string()),
    }
}

fn is_loopback(url: &tauri::Url) -> bool {
    matches!(
        url.host_str(),
        Some("localhost") | Some("127.0.0.1") | Some("::1") | Some("[::1]")
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use ocg_core::crypto::{KeyCipher, StaticKeyCipher};
    use ocg_core::db::Database;
    use ocg_core::gateway;
    use ocg_core::state::CoreStateInner;
    use std::fs;
    use std::net::{TcpListener, TcpStream};
    use std::sync::Arc;

    fn temp_core() -> (std::path::PathBuf, CoreState) {
        let dir = std::env::temp_dir().join(format!("ocg-tauri-set-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        let cipher: Arc<dyn KeyCipher + Send + Sync> = Arc::new(StaticKeyCipher::new("test"));
        let db = Database::open(dir.clone()).unwrap();
        (
            dir.clone(),
            Arc::new(CoreStateInner::new(db, dir, cipher).unwrap()),
        )
    }

    fn free_port() -> u16 {
        TcpListener::bind(("127.0.0.1", 0))
            .unwrap()
            .local_addr()
            .unwrap()
            .port()
    }

    #[test]
    fn validate_upstream_url_accepts_https_and_loopback_http() {
        assert!(validate_upstream_url("https://opencode.ai/zen/go").is_ok());
        assert!(validate_upstream_url("http://127.0.0.1:8080").is_ok());
        assert!(validate_upstream_url("http://localhost/v1").is_ok());
        assert!(validate_upstream_url("not a url").is_err());
        assert!(validate_upstream_url("http://example.com").is_err());
        assert!(validate_upstream_url("ftp://127.0.0.1").is_err());
    }

    #[test]
    fn settings_inners_update_and_regenerate_without_autostart_side_effects() {
        let (dir, core) = temp_core();
        let original = get_settings_inner(&core).unwrap();
        let mut next = original.clone();
        next.upstream_base_url = "https://example.com/go".into();
        next.client_root_url = "https://client.example.com/".into();
        next.gateway_port = free_port();

        let status = update_settings_inner(&core, &mut next, false).unwrap();
        assert!(!status.running);
        assert_eq!(core.config().upstream_base_url, "https://example.com/go");
        assert_eq!(core.config().client_root_url, "https://client.example.com");

        let old_key = core.config().gateway_key;
        let new_key = regenerate_gateway_key_inner(&core).unwrap();
        assert_ne!(old_key, new_key);
        assert_eq!(core.config().gateway_key, new_key);

        let mut bad = core.config();
        bad.upstream_base_url = "http://evil.example".into();
        assert!(update_settings_inner(&core, &mut bad, false).is_err());

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn settings_port_change_restarts_running_gateway_or_keeps_old_on_failure() {
        let (dir, core) = temp_core();
        let old_port = free_port();
        let handle =
            tauri::async_runtime::block_on(gateway::start_gateway(core.clone(), old_port)).unwrap();
        *core.gateway.lock() = Some(handle);

        let mut ok = core.config();
        ok.gateway_port = free_port();
        let status = update_settings_inner(&core, &mut ok, false).unwrap();
        assert!(status.running);
        assert_eq!(status.port, ok.gateway_port);
        assert!(TcpStream::connect(("127.0.0.1", ok.gateway_port)).is_ok());

        let occupied = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let mut fail = core.config();
        fail.gateway_port = occupied.local_addr().unwrap().port();
        assert!(update_settings_inner(&core, &mut fail, false).is_err());
        assert_eq!(core.active_gateway_port(), ok.gateway_port);

        crate::commands::gateway::stop_and_wait(core.gateway.lock().take().unwrap());
        drop(occupied);
        let _ = fs::remove_dir_all(dir);
    }
}
