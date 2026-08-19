use crate::state::AppState;
use chrono::Utc;
use ocg_core::models::{DailyModelCost, DashboardSummary, UpstreamChannel};
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
    let free_channel_cooling = db
        .free_channel_cooldown_until()
        .map_err(|e| e.to_string())?
        .is_some();
    let available_accounts = accounts
        .iter()
        .filter(|account| dashboard_account_is_available(account, now, free_channel_cooling))
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

fn dashboard_account_is_available(
    account: &ocg_core::models::Account,
    now: chrono::DateTime<Utc>,
    free_channel_cooling: bool,
) -> bool {
    if !account.enabled
        || !account.setup_step.is_ready()
        || account.auth_error.is_some()
        || account.validate_provider_binding().is_err()
    {
        return false;
    }
    match (account.provider_id.as_str(), account.offering_id.as_str()) {
        (ocg_core::provider::OPENCODE_PROVIDER_ID, ocg_core::provider::GO_OFFERING_ID) => {
            !account.key_cipher.is_empty() && !account.is_cooling_for(UpstreamChannel::Go, now)
        }
        (
            ocg_core::provider::OPENCODE_ZEN_FREE_PROVIDER_ID,
            ocg_core::provider::ANONYMOUS_FREE_OFFERING_ID,
        ) => !free_channel_cooling && !account.is_cooling_for(UpstreamChannel::Free, now),
        _ => false,
    }
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
    use ocg_core::models::{Account, AccountInput, AccountSetupStep, AccountType};
    use ocg_core::state::CoreStateInner;
    use std::fs;
    use std::sync::Arc;

    #[test]
    fn dashboard_summary_excludes_draft_and_keyless_accounts() {
        let dir = std::env::temp_dir().join(format!("ocg-tauri-dash-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        let cipher: Arc<dyn KeyCipher + Send + Sync> = Arc::new(StaticKeyCipher::new("test"));
        let db = Database::open(dir.clone()).unwrap();
        let core = Arc::new(CoreStateInner::new(db, dir.clone(), cipher).unwrap());

        let now = Utc::now();
        let base =
            |id: &str, name: &str, setup_step: AccountSetupStep, key_cipher: String| Account {
                id: id.into(),
                provider_id: ocg_core::provider::default_provider_id(),
                offering_id: ocg_core::provider::default_offering_id(),
                credential_kind: ocg_core::provider::default_credential_kind(),
                quota_scope: ocg_core::provider::default_quota_scope(),
                free_alias_enabled: false,
                name: name.into(),
                username: None,
                password_cipher: None,
                key_cipher,
                enabled: true,
                account_type: AccountType::Managed,
                setup_step,
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
                created_at: now,
                updated_at: now,
            };

        {
            let db = core.db.lock();
            // Draft managed account: setup unfinished, no key yet.
            db.create_account(&base(
                "draft",
                "draft",
                AccountSetupStep::GoogleAccount,
                String::new(),
            ))
            .unwrap();
            // Finished setup but never stored a key: not routable.
            db.create_account(&base(
                "keyless",
                "keyless",
                AccountSetupStep::Ready,
                String::new(),
            ))
            .unwrap();
        }

        let summary = get_dashboard_summary_inner(&core).unwrap();
        assert_eq!(summary.total_accounts, 3);
        assert_eq!(summary.available_accounts, 1);

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn dashboard_inners_summarize_accounts_and_costs() {
        let dir = std::env::temp_dir().join(format!("ocg-tauri-dash-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        let cipher: Arc<dyn KeyCipher + Send + Sync> = Arc::new(StaticKeyCipher::new("test"));
        let db = Database::open(dir.clone()).unwrap();
        let core = Arc::new(CoreStateInner::new(db, dir.clone(), cipher).unwrap());

        let summary = get_dashboard_summary_inner(&core).unwrap();
        assert_eq!(summary.total_accounts, 1);
        assert!(!summary.gateway_running);

        crate::commands::account::create_account_inner(
            &core,
            AccountInput {
                provider_id: ocg_core::provider::default_provider_id(),
                offering_id: ocg_core::provider::default_offering_id(),
                name: "a".into(),
                username: None,
                password: None,
                key: "sk-a".into(),
                referral_code: None,
                purchase_date: None,
                notes: None,
            },
        )
        .unwrap();

        // Mark one account cooling so available count differs from total.
        let id = get_dashboard_summary_inner(&core).unwrap();
        assert_eq!(id.total_accounts, 2);
        assert_eq!(id.available_accounts, 2);

        {
            let accounts = core.db.lock().list_accounts().unwrap();
            let account = accounts.iter().find(|account| account.name == "a").unwrap();
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
        assert_eq!(cooled.available_accounts, 1);
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

    #[test]
    fn dashboard_summary_excludes_goat_and_honors_zen_free_cooldown() {
        let dir = std::env::temp_dir().join(format!("ocg-tauri-dash-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        let cipher: Arc<dyn KeyCipher + Send + Sync> = Arc::new(StaticKeyCipher::new("test"));
        let db = Database::open(dir.clone()).unwrap();
        let core = Arc::new(CoreStateInner::new(db, dir.clone(), cipher).unwrap());

        crate::commands::account::create_account_inner(
            &core,
            AccountInput {
                provider_id: ocg_core::provider::COMMAND_CODE_PROVIDER_ID.into(),
                offering_id: ocg_core::provider::GOAT_OFFERING_ID.into(),
                name: "goat".into(),
                username: None,
                password: None,
                key: "goat-key".into(),
                referral_code: None,
                purchase_date: None,
                notes: None,
            },
        )
        .unwrap();
        let summary = get_dashboard_summary_inner(&core).unwrap();
        assert_eq!(summary.total_accounts, 2);
        assert_eq!(summary.available_accounts, 1);

        core.db
            .lock()
            .set_account_rate_limit(
                ocg_core::provider::ZEN_FREE_ACCOUNT_ID,
                Utc::now() + chrono::Duration::hours(1),
                "free limited",
                Some(ocg_core::models::UsageWindowKind::Free),
            )
            .unwrap();

        assert_eq!(
            get_dashboard_summary_inner(&core)
                .unwrap()
                .available_accounts,
            0
        );

        let _ = fs::remove_dir_all(dir);
    }
}
