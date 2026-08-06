use crate::state::AppState;
use chrono::Utc;
use ocg_core::browser::{BrowserProfileOperationKind, StagedBrowserProfiles};
use ocg_core::models::{Account, AccountInput, AccountSetupStep, AccountType, AccountUpdate};
use ocg_core::state::CoreState;
use tauri::State;

#[tauri::command]
pub fn get_accounts(state: State<'_, AppState>) -> Result<Vec<Account>, String> {
    get_accounts_inner(&state.core)
}

pub(crate) fn get_accounts_inner(core: &CoreState) -> Result<Vec<Account>, String> {
    core.db.lock().list_accounts().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn create_account(state: State<'_, AppState>, input: AccountInput) -> Result<Account, String> {
    create_account_inner(&state.core, input)
}

pub(crate) fn create_account_inner(
    core: &CoreState,
    input: AccountInput,
) -> Result<Account, String> {
    let id = uuid::Uuid::new_v4().to_string();
    let now = Utc::now();
    let account = Account {
        id: id.clone(),
        name: input.name,
        username: input.username,
        password_cipher: match input.password.as_deref().map(str::trim) {
            Some("") | None => None,
            Some(password) => Some(core.encrypt_key(password).map_err(|e| e.to_string())?),
        },
        key_cipher: core.encrypt_key(&input.key).map_err(|e| e.to_string())?,
        enabled: true,
        account_type: AccountType::Key,
        setup_step: AccountSetupStep::Ready,
        referral_code: input.referral_code,
        purchase_date: input.purchase_date.unwrap_or_default(),
        expires_on: String::new(),
        cooldown_until: None,
        cooldown_generic_until: None,
        cooldown_5h_until: None,
        cooldown_week_until: None,
        cooldown_month_until: None,
        cooldown_free_until: None,
        last_error: None,
        auth_error: None,
        created_at: now,
        updated_at: now,
    };
    let db = core.db.lock();
    db.create_account(&account).map_err(|e| e.to_string())?;
    let account = db
        .get_account(&id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "created account not found".to_string())?;
    let _ = db.log_gateway(
        "info",
        "account",
        &format!("created account {}", account.name),
    );
    Ok(account)
}

#[tauri::command]
pub fn update_account(
    state: State<'_, AppState>,
    id: String,
    update: AccountUpdate,
) -> Result<Account, String> {
    update_account_inner(&state.core, id, update)
}

pub(crate) fn update_account_inner(
    core: &CoreState,
    id: String,
    update: AccountUpdate,
) -> Result<Account, String> {
    let key_cipher = update
        .key
        .as_ref()
        .filter(|k| !k.is_empty())
        .map(|k| core.encrypt_key(k))
        .transpose()
        .map_err(|e| e.to_string())?;
    let password_cipher = match update.password.as_deref().map(str::trim) {
        Some("") => Some(String::new()),
        None => None,
        Some(password) => Some(core.encrypt_key(password).map_err(|e| e.to_string())?),
    };
    {
        let db = core.db.lock();
        if update.enabled == Some(true) {
            let account = db
                .get_account(&id)
                .map_err(|e| e.to_string())?
                .ok_or_else(|| "account not found".to_string())?;
            let resulting_key = key_cipher.as_deref().unwrap_or(&account.key_cipher);
            if !account.setup_step.is_ready() || resulting_key.is_empty() {
                return Err("account setup is not complete and cannot be enabled".to_string());
            }
        }
        db.update_account(
            &id,
            &update,
            key_cipher.as_deref(),
            password_cipher.as_deref(),
        )
        .map_err(|e| e.to_string())?;
    }
    let db = core.db.lock();
    let account = db
        .get_account(&id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "account not found".to_string())?;
    let _ = db.log_gateway(
        "info",
        "account",
        &format!("updated account {}", account.name),
    );
    Ok(account)
}

#[tauri::command]
pub async fn delete_account(state: State<'_, AppState>, id: String) -> Result<(), String> {
    delete_account_inner(&state.core, id).await
}

pub(crate) async fn delete_account_inner(core: &CoreState, id: String) -> Result<(), String> {
    let browser_operation = core.browser.operation().await;
    core.recover_browser_profiles_for_account(&id)
        .map_err(|e| e.to_string())?;
    let account = {
        let db = core.db.lock();
        db.get_account(&id).map_err(|e| e.to_string())?
    };
    let Some(account) = account else {
        return Ok(());
    };
    browser_operation
        .stop_account(&id)
        .await
        .map_err(|e| e.to_string())?;
    let staged = StagedBrowserProfiles::stage(
        &core.data_dir(),
        &id,
        BrowserProfileOperationKind::DeleteAccount,
    )
    .map_err(|e| e.to_string())?;
    let delete_result = {
        let mut db = core.db.lock();
        let result = db.delete_account(&id);
        if result.is_ok() {
            let _ = db.log_gateway(
                "info",
                "account",
                &format!("deleted account {}", account.name),
            );
        }
        result
    };
    if let Err(error) = delete_result {
        let restore_error = staged.restore().err();
        return Err(match restore_error {
            Some(restore) => format!(
                "failed to delete account: {error}; failed to restore browser profile: {restore}"
            ),
            None => format!("failed to delete account: {error}"),
        });
    }
    staged.purge().map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn toggle_account(state: State<'_, AppState>, id: String) -> Result<Account, String> {
    toggle_account_inner(&state.core, id)
}

pub(crate) fn toggle_account_inner(core: &CoreState, id: String) -> Result<Account, String> {
    let db = core.db.lock();
    let account = db
        .get_account(&id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "account not found".to_string())?;
    let next_enabled = !account.enabled;
    if next_enabled && (!account.setup_step.is_ready() || account.key_cipher.is_empty()) {
        return Err("account setup is not complete and cannot be enabled".to_string());
    }
    let update = AccountUpdate {
        name: None,
        username: None,
        password: None,
        key: None,
        enabled: Some(next_enabled),
        referral_code: None,
        purchase_date: None,
    };
    db.update_account(&id, &update, None, None)
        .map_err(|e| e.to_string())?;
    let account = db
        .get_account(&id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "account not found after toggle".to_string())?;
    Ok(account)
}

#[tauri::command]
pub fn test_account(state: State<'_, AppState>, id: String) -> Result<String, String> {
    test_account_inner(&state.core, id)
}

pub(crate) fn test_account_inner(core: &CoreState, id: String) -> Result<String, String> {
    let db = core.db.lock();
    let account = db
        .get_account(&id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "account not found".to_string())?;
    if !account.setup_step.is_ready() || account.key_cipher.is_empty() {
        return Err("account setup is not complete and cannot be tested".to_string());
    }
    let key = core
        .decrypt_key(&account.key_cipher)
        .map_err(|e| e.to_string())?;
    let masked = if key.len() > 8 && key.is_char_boundary(4) && key.is_char_boundary(key.len() - 4)
    {
        format!("{}...{}", &key[..4], &key[key.len() - 4..])
    } else {
        "***".to_string()
    };
    Ok(format!(
        "account {} key looks valid ({})",
        account.name, masked
    ))
}

#[tauri::command]
pub fn get_account_usage(
    state: State<'_, AppState>,
    id: String,
) -> Result<ocg_core::models::UsageWindow, String> {
    get_account_usage_inner(&state.core, id)
}

pub(crate) fn get_account_usage_inner(
    core: &CoreState,
    id: String,
) -> Result<ocg_core::models::UsageWindow, String> {
    core.db.lock().account_usage(&id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn reset_account_cooldown(state: State<'_, AppState>, id: String) -> Result<Account, String> {
    reset_account_cooldown_inner(&state.core, id)
}

pub(crate) fn reset_account_cooldown_inner(
    core: &CoreState,
    id: String,
) -> Result<Account, String> {
    {
        let db = core.db.lock();
        db.clear_account_cooldown(&id).map_err(|e| e.to_string())?;
    }
    let db = core.db.lock();
    let account = db
        .get_account(&id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "account not found".to_string())?;
    let _ = db.log_gateway(
        "info",
        "account",
        &format!("reset cooldown for {}", account.name),
    );
    Ok(account)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, Utc};
    use ocg_core::browser::browser_profile_paths;
    use ocg_core::crypto::{KeyCipher, StaticKeyCipher};
    use ocg_core::db::Database;
    use ocg_core::state::CoreStateInner;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::Arc;

    fn temp_core() -> (PathBuf, CoreState) {
        let dir = std::env::temp_dir().join(format!("ocg-tauri-acct-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        let cipher: Arc<dyn KeyCipher + Send + Sync> = Arc::new(StaticKeyCipher::new("test"));
        let db = Database::open(dir.clone()).unwrap();
        (
            dir.clone(),
            Arc::new(CoreStateInner::new(db, dir, cipher).unwrap()),
        )
    }

    #[tokio::test]
    async fn account_command_inners_cover_lifecycle() {
        let (dir, core) = temp_core();

        assert!(get_accounts_inner(&core).unwrap().is_empty());

        let created = create_account_inner(
            &core,
            AccountInput {
                name: "main".into(),
                username: Some("alice".into()),
                password: Some("  secret  ".into()),
                key: "sk-long-enough-key".into(),
                referral_code: None,
                purchase_date: None,
            },
        )
        .unwrap();
        assert_eq!(created.name, "main");
        assert!(created.password_cipher.is_some());

        let blank = create_account_inner(
            &core,
            AccountInput {
                name: "blank".into(),
                username: None,
                password: Some("".into()),
                key: "short".into(),
                referral_code: Some("ref".into()),
                purchase_date: Some("2026-01-15".into()),
            },
        )
        .unwrap();
        assert!(blank.password_cipher.is_none());

        let listed = get_accounts_inner(&core).unwrap();
        assert_eq!(listed.len(), 2);

        let updated = update_account_inner(
            &core,
            created.id.clone(),
            AccountUpdate {
                name: Some("renamed".into()),
                username: None,
                password: Some("".into()),
                key: Some("sk-replacement-key".into()),
                enabled: None,
                referral_code: None,
                purchase_date: None,
            },
        )
        .unwrap();
        assert_eq!(updated.name, "renamed");
        assert!(updated.password_cipher.is_none());

        let toggled = toggle_account_inner(&core, created.id.clone()).unwrap();
        assert!(!toggled.enabled);
        let restored = toggle_account_inner(&core, created.id.clone()).unwrap();
        assert!(restored.enabled);

        let msg = test_account_inner(&core, created.id.clone()).unwrap();
        assert!(msg.contains("renamed"));
        assert!(msg.contains("..."));

        let short_msg = test_account_inner(&core, blank.id.clone()).unwrap();
        assert!(short_msg.contains("***"));

        let usage = get_account_usage_inner(&core, created.id.clone()).unwrap();
        assert_eq!(usage.account_id, created.id);

        {
            let db = core.db.lock();
            db.set_account_cooldown(
                &created.id,
                Some(Utc::now() + Duration::hours(1)),
                Some("limited"),
            )
            .unwrap();
        }
        let cleared = reset_account_cooldown_inner(&core, created.id.clone()).unwrap();
        assert!(cleared.cooldown_until.is_none());

        let mut pending = blank.clone();
        pending.id = uuid::Uuid::new_v4().to_string();
        pending.name = "pending".into();
        pending.key_cipher = String::new();
        pending.enabled = false;
        pending.account_type = AccountType::Managed;
        pending.setup_step = AccountSetupStep::GoogleAccount;
        core.db.lock().create_account(&pending).unwrap();

        assert!(toggle_account_inner(&core, pending.id.clone()).is_err());
        assert!(test_account_inner(&core, pending.id.clone()).is_err());
        assert!(
            update_account_inner(
                &core,
                pending.id.clone(),
                AccountUpdate {
                    name: None,
                    username: None,
                    password: None,
                    key: Some("sk-cannot-bypass-setup".into()),
                    enabled: Some(true),
                    referral_code: None,
                    purchase_date: None,
                },
            )
            .is_err()
        );

        let blank_profiles = browser_profile_paths(&dir, &blank.id).unwrap();
        assert!(blank_profiles.iter().all(|path| path.starts_with(&dir)));
        for profile in &blank_profiles {
            fs::create_dir_all(profile).unwrap();
            fs::write(profile.join("Cookies"), b"session").unwrap();
        }
        delete_account_inner(&core, blank.id.clone()).await.unwrap();
        assert!(blank_profiles.iter().all(|path| !path.exists()));

        let pending_profile = browser_profile_paths(&dir, &pending.id).unwrap()[0].clone();
        fs::create_dir_all(&pending_profile).unwrap();
        fs::write(pending_profile.join("SingletonLock"), b"active").unwrap();
        assert!(
            delete_account_inner(&core, pending.id.clone())
                .await
                .is_err()
        );
        assert!(core.db.lock().get_account(&pending.id).unwrap().is_some());
        assert!(pending_profile.exists());
        fs::remove_file(pending_profile.join("SingletonLock")).unwrap();
        delete_account_inner(&core, pending.id.clone())
            .await
            .unwrap();

        delete_account_inner(&core, "missing".into()).await.unwrap();
        assert_eq!(get_accounts_inner(&core).unwrap().len(), 1);

        assert!(toggle_account_inner(&core, "missing".into()).is_err());
        assert!(test_account_inner(&core, "missing".into()).is_err());
        assert!(reset_account_cooldown_inner(&core, "missing".into()).is_err());
        assert!(
            update_account_inner(
                &core,
                "missing".into(),
                AccountUpdate {
                    name: Some("x".into()),
                    username: None,
                    password: None,
                    key: None,
                    enabled: None,
                    referral_code: None,
                    purchase_date: None,
                },
            )
            .is_err()
        );

        let _ = fs::remove_dir_all(dir);
    }
}
