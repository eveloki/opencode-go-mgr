use crate::native_browser::{
    close_all_browser_processes, stop_external_browser, validate_account_id,
};
use crate::state::AppState;
use ocg_core::browser::{BrowserProfileOperationKind, StagedBrowserProfiles};
use ocg_core::models::AccountType;
use ocg_core::state::CoreState;
use tauri::State;

const OCG_CONSOLE_URL: &str = "https://opencode.ai/auth";

/// Backward-compatible legacy command: open the OpenCode console for an account.
#[tauri::command]
pub async fn open_browser(
    state: State<'_, AppState>,
    account_id: String,
) -> Result<String, String> {
    open_browser_inner(&state.core, &account_id).await
}

pub(crate) async fn open_browser_inner(
    core: &CoreState,
    account_id: &str,
) -> Result<String, String> {
    validate_account_id(account_id)?;
    let operation = core.browser.operation().await;
    core.recover_browser_profiles_for_account(account_id)
        .map_err(|error| error.to_string())?;
    core.db
        .lock()
        .get_account(account_id)
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "account not found".to_string())?;
    operation
        .open(account_id, OCG_CONSOLE_URL, "legacy-tauri-command")
        .await
        .map_err(|error| error.to_string())?;
    Ok(OCG_CONSOLE_URL.to_string())
}

#[tauri::command]
pub fn close_browser(state: State<'_, AppState>) -> Result<(), String> {
    close_all_browser_processes(&state.browser_processes, Some(&state.core.data_dir()))
}

#[tauri::command]
pub fn close_account_browser(state: State<'_, AppState>, account_id: String) -> Result<(), String> {
    stop_external_browser(
        &state.browser_processes,
        &account_id,
        Some(&state.core.data_dir()),
    )
}

/// Reset browser identity. Ready accounts keep their Key; pending managed
/// accounts return to the first setup step. Both current Chromium and legacy
/// WebView profiles are removed atomically.
#[tauri::command]
pub async fn reset_browser_profile(
    state: State<'_, AppState>,
    account_id: String,
) -> Result<(), String> {
    validate_account_id(&account_id)?;
    let operation = state.core.browser.operation().await;
    state
        .core
        .recover_browser_profiles_for_account(&account_id)
        .map_err(|error| error.to_string())?;
    let account = state
        .core
        .db
        .lock()
        .get_account(&account_id)
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "account not found".to_string())?;
    operation
        .stop_account(&account_id)
        .await
        .map_err(|error| error.to_string())?;

    let data_dir = state.core.data_dir();
    let staged = StagedBrowserProfiles::stage(
        &data_dir,
        &account_id,
        BrowserProfileOperationKind::ResetProfile,
    )
    .map_err(|error| error.to_string())?;
    if account.account_type == AccountType::Managed && !account.setup_step.is_ready() {
        if let Err(error) = state
            .core
            .db
            .lock()
            .reset_pending_managed_setup(&account_id)
        {
            let purge_error = staged.purge().err();
            return Err(match purge_error {
                Some(purge) => format!(
                    "failed to reset managed setup: {error}; failed to finish browser profile reset: {purge}"
                ),
                None => format!("failed to reset managed setup: {error}"),
            });
        }
    }
    staged.purge().map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[tokio::test]
    async fn legacy_open_recovers_staged_profile_before_native_launch() {
        use ocg_core::crypto::{KeyCipher, StaticKeyCipher};
        use ocg_core::db::Database;
        use ocg_core::models::{Account, AccountSetupStep, AccountType};
        use ocg_core::state::CoreStateInner;
        use std::sync::atomic::{AtomicBool, Ordering};

        let data_dir = std::env::temp_dir().join(format!(
            "ocg-native-browser-open-recovery-test-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&data_dir).unwrap();
        let cipher: Arc<dyn KeyCipher + Send + Sync> =
            Arc::new(StaticKeyCipher::new("browser-open-test"));
        let core = Arc::new(
            CoreStateInner::new(
                Database::open(data_dir.clone()).unwrap(),
                data_dir.clone(),
                cipher,
            )
            .unwrap(),
        );
        let now = chrono::Utc::now();
        let account = Account {
            id: "account-1".into(),
            provider_id: ocg_core::provider::default_provider_id(),
            offering_id: ocg_core::provider::default_offering_id(),
            credential_kind: ocg_core::provider::default_credential_kind(),
            quota_scope: ocg_core::provider::default_quota_scope(),
            name: "account-1".into(),
            username: None,
            password_cipher: None,
            key_cipher: "cipher".into(),
            enabled: true,
            account_type: AccountType::Key,
            setup_step: AccountSetupStep::Ready,
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
        core.db.lock().create_account(&account).unwrap();
        let profile = data_dir.join("browser-profiles").join(&account.id);
        std::fs::create_dir_all(&profile).unwrap();
        std::fs::write(profile.join("Cookies"), b"recover-me").unwrap();
        let staged = StagedBrowserProfiles::stage(
            &data_dir,
            &account.id,
            BrowserProfileOperationKind::DeleteAccount,
        )
        .unwrap();
        assert!(!profile.exists());
        drop(staged);

        let launched = Arc::new(AtomicBool::new(false));
        let launched_flag = launched.clone();
        let expected_profile = profile.clone();
        core.browser
            .register_native_hooks(
                Arc::new(move |_, _| {
                    if !expected_profile.join("Cookies").is_file() {
                        anyhow::bail!("profile was not recovered before launch");
                    }
                    launched_flag.store(true, Ordering::SeqCst);
                    Ok(())
                }),
                Arc::new(|_| Ok(())),
            )
            .unwrap();

        let opened = open_browser_inner(&core, &account.id).await.unwrap();

        assert_eq!(opened, OCG_CONSOLE_URL);
        assert!(launched.load(Ordering::SeqCst));
        assert!(profile.join("Cookies").is_file());
        drop(core);
        std::fs::remove_dir_all(data_dir).unwrap();
    }

    #[tokio::test]
    async fn legacy_open_rejects_missing_account_without_launching() {
        use ocg_core::crypto::{KeyCipher, StaticKeyCipher};
        use ocg_core::db::Database;
        use ocg_core::state::CoreStateInner;
        use std::sync::atomic::{AtomicBool, Ordering};

        let data_dir = std::env::temp_dir().join(format!(
            "ocg-native-browser-missing-account-test-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&data_dir).unwrap();
        let cipher: Arc<dyn KeyCipher + Send + Sync> =
            Arc::new(StaticKeyCipher::new("browser-missing-test"));
        let core = Arc::new(
            CoreStateInner::new(
                Database::open(data_dir.clone()).unwrap(),
                data_dir.clone(),
                cipher,
            )
            .unwrap(),
        );
        let launched = Arc::new(AtomicBool::new(false));
        let launched_flag = launched.clone();
        core.browser
            .register_native_hooks(
                Arc::new(move |_, _| {
                    launched_flag.store(true, Ordering::SeqCst);
                    Ok(())
                }),
                Arc::new(|_| Ok(())),
            )
            .unwrap();

        let error = open_browser_inner(&core, "missing-account")
            .await
            .expect_err("missing account must not launch");
        assert_eq!(error, "account not found");
        assert!(!launched.load(Ordering::SeqCst));

        drop(core);
        std::fs::remove_dir_all(data_dir).unwrap();
    }
}
