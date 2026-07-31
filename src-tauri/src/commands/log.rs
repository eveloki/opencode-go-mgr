use crate::state::AppState;
use ocg_core::models::{ForwardLog, GatewayLog};
use ocg_core::state::CoreState;
use tauri::State;

#[tauri::command]
pub fn get_gateway_logs(
    state: State<'_, AppState>,
    limit: Option<i64>,
) -> Result<Vec<GatewayLog>, String> {
    get_gateway_logs_inner(&state.core, limit)
}

pub(crate) fn get_gateway_logs_inner(
    core: &CoreState,
    limit: Option<i64>,
) -> Result<Vec<GatewayLog>, String> {
    core.db
        .lock()
        .list_gateway_logs(limit.unwrap_or(200))
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_forward_logs(
    state: State<'_, AppState>,
    limit: Option<i64>,
) -> Result<Vec<ForwardLog>, String> {
    get_forward_logs_inner(&state.core, limit)
}

pub(crate) fn get_forward_logs_inner(
    core: &CoreState,
    limit: Option<i64>,
) -> Result<Vec<ForwardLog>, String> {
    core.db
        .lock()
        .list_forward_logs(limit.unwrap_or(200))
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ocg_core::crypto::{KeyCipher, StaticKeyCipher};
    use ocg_core::db::Database;
    use ocg_core::state::CoreStateInner;
    use std::fs;
    use std::sync::Arc;

    #[test]
    fn log_command_inners_read_empty_and_written_rows() {
        let dir = std::env::temp_dir().join(format!("ocg-tauri-log-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        let cipher: Arc<dyn KeyCipher + Send + Sync> = Arc::new(StaticKeyCipher::new("test"));
        let db = Database::open(dir.clone()).unwrap();
        let core = Arc::new(CoreStateInner::new(db, dir.clone(), cipher).unwrap());

        assert!(get_gateway_logs_inner(&core, Some(10)).unwrap().is_empty());
        assert!(get_forward_logs_inner(&core, None).unwrap().is_empty());

        core.db.lock().log_gateway("info", "test", "hello").unwrap();
        let gateway = get_gateway_logs_inner(&core, Some(5)).unwrap();
        assert_eq!(gateway.len(), 1);
        assert_eq!(gateway[0].message, "hello");

        let _ = fs::remove_dir_all(dir);
    }
}
