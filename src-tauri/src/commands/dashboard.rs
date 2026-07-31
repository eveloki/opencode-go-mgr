use crate::state::AppState;
use chrono::Utc;
use ocg_core::models::{DailyModelCost, DashboardSummary};
use ocg_core::state::CoreState;
use tauri::State;

#[tauri::command]
pub fn get_dashboard_summary(state: State<'_, AppState>) -> Result<DashboardSummary, String> {
    get_dashboard_summary_inner(&state.core)
}

pub(crate) fn get_dashboard_summary_inner(core: &CoreState) -> Result<DashboardSummary, String> {
    let db = core.db.lock();
    let accounts = db.list_accounts().map_err(|e| e.to_string())?;
    let total_accounts = accounts.len();
    let now = Utc::now();
    let available_accounts = accounts
        .iter()
        .filter(|a| a.enabled && a.auth_error.is_none() && !a.is_cooling_at(now))
        .count();

    let gateway_running = core.gateway.lock().is_some();

    let (today_cost, week_cost, month_cost) = db.total_usage().map_err(|e| e.to_string())?;

    Ok(DashboardSummary {
        total_accounts,
        available_accounts,
        gateway_running,
        today_cost,
        week_cost,
        month_cost,
    })
}

/// Return per-day, per-model cost buckets for the last `days` days, for the
/// dashboard stacked-bar chart. Defaults to 30 days.
#[tauri::command]
pub fn get_daily_cost_by_model(
    state: State<'_, AppState>,
    days: Option<i64>,
) -> Result<Vec<DailyModelCost>, String> {
    get_daily_cost_by_model_inner(&state.core, days)
}

pub(crate) fn get_daily_cost_by_model_inner(
    core: &CoreState,
    days: Option<i64>,
) -> Result<Vec<DailyModelCost>, String> {
    core.db
        .lock()
        .daily_cost_by_model(days.unwrap_or(30))
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ocg_core::crypto::{KeyCipher, StaticKeyCipher};
    use ocg_core::db::Database;
    use ocg_core::models::{Account, AccountInput};
    use ocg_core::state::CoreStateInner;
    use std::fs;
    use std::sync::Arc;

    #[test]
    fn dashboard_inners_summarize_accounts_and_costs() {
        let dir = std::env::temp_dir().join(format!("ocg-tauri-dash-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        let cipher: Arc<dyn KeyCipher + Send + Sync> = Arc::new(StaticKeyCipher::new("test"));
        let db = Database::open(dir.clone()).unwrap();
        let core = Arc::new(CoreStateInner::new(db, dir.clone(), cipher).unwrap());

        let summary = get_dashboard_summary_inner(&core).unwrap();
        assert_eq!(summary.total_accounts, 0);
        assert!(!summary.gateway_running);

        crate::commands::account::create_account_inner(
            &core,
            AccountInput {
                name: "a".into(),
                username: None,
                password: None,
                key: "sk-a".into(),
                referral_code: None,
                purchase_date: None,
            },
        )
        .unwrap();

        // Mark one account cooling so available count differs from total.
        let id = get_dashboard_summary_inner(&core).unwrap();
        assert_eq!(id.total_accounts, 1);
        assert_eq!(id.available_accounts, 1);

        {
            let accounts = core.db.lock().list_accounts().unwrap();
            let account: &Account = &accounts[0];
            core.db
                .lock()
                .set_account_cooldown(
                    &account.id,
                    Some(Utc::now() + chrono::Duration::hours(2)),
                    Some("limited"),
                )
                .unwrap();
        }

        let cooled = get_dashboard_summary_inner(&core).unwrap();
        assert_eq!(cooled.available_accounts, 0);
        assert!(
            get_daily_cost_by_model_inner(&core, Some(7))
                .unwrap()
                .is_empty()
        );
        assert!(
            get_daily_cost_by_model_inner(&core, None)
                .unwrap()
                .is_empty()
        );

        let _ = fs::remove_dir_all(dir);
    }
}
