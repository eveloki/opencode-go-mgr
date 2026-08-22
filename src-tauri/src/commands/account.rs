use crate::state::AppState;
use chrono::Utc;
use ocg_core::browser::{BrowserProfileOperationKind, StagedBrowserProfiles};
use ocg_core::models::{
    Account, AccountInput, AccountSetupStep, AccountType, AccountUpdate, normalize_account_notes,
};
use ocg_core::provider::builtin_offering;
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
    let provider_id = input.provider_id.trim();
    let offering_id = input.offering_id.trim();
    let offering = builtin_offering(provider_id, offering_id)
        .ok_or_else(|| format!("unknown provider offering `{provider_id}/{offering_id}`"))?;
    if offering.singleton_account_id.is_some() {
        return Err(
            "singleton provider offering cannot be created through the account command".into(),
        );
    }
    let account = Account {
        id: id.clone(),
        provider_id: offering.provider_id.to_string(),
        offering_id: offering.offering_id.to_string(),
        credential_kind: offering.credential_kind,
        quota_scope: offering.quota_scope,
        name: input.name,
        username: input.username,
        password_cipher: match input.password.as_deref().map(str::trim) {
            Some("") | None => None,
            Some(password) => Some(core.encrypt_key(password).map_err(|e| e.to_string())?),
        },
        key_cipher: core.encrypt_key(&input.key).map_err(|e| e.to_string())?,
        enabled: ocg_core::provider::offering_allows_enablement(
            offering.provider_id,
            offering.offering_id,
        ),
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
        notes: normalize_account_notes(input.notes.as_deref().unwrap_or(""))
            .map_err(|error| error.to_string())?,
        created_at: now,
        updated_at: now,
    };
    ocg_core::provider::ensure_enabled_offering_is_routable(
        &account.provider_id,
        &account.offering_id,
        account.enabled,
    )
    .map_err(|error| error.to_string())?;
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
    mut update: AccountUpdate,
) -> Result<Account, String> {
    if let Some(value) = update.notes.take() {
        update.notes = Some(
            normalize_account_notes(&value)
                .map_err(|error| error.to_string())?
                .unwrap_or_default(),
        );
    }
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
            ocg_core::provider::ensure_offering_can_enable(
                &account.provider_id,
                &account.offering_id,
            )
            .map_err(|error| error.to_string())?;
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
    if next_enabled {
        ocg_core::provider::ensure_offering_can_enable(&account.provider_id, &account.offering_id)
            .map_err(|error| error.to_string())?;
    }
    let update = AccountUpdate {
        name: None,
        username: None,
        password: None,
        key: None,
        enabled: Some(next_enabled),
        referral_code: None,
        purchase_date: None,
        notes: None,
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
    let db = core.db.lock();
    let account = db
        .get_account(&id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "account not found".to_string())?;
    if account.provider_id != ocg_core::provider::OPENCODE_PROVIDER_ID
        || account.offering_id != ocg_core::provider::GO_OFFERING_ID
    {
        return Err("legacy usage windows are only available for OpenCode Go accounts".into());
    }
    db.account_usage(&id).map_err(|e| e.to_string())
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

        assert_eq!(get_accounts_inner(&core).unwrap().len(), 1);

        let created = create_account_inner(
            &core,
            AccountInput {
                provider_id: ocg_core::provider::default_provider_id(),
                offering_id: ocg_core::provider::default_offering_id(),
                name: "main".into(),
                username: Some("alice".into()),
                password: Some("  secret  ".into()),
                key: "sk-long-enough-key".into(),
                referral_code: None,
                purchase_date: None,
                notes: None,
            },
        )
        .unwrap();
        assert_eq!(created.name, "main");
        assert!(created.password_cipher.is_some());

        let goat = create_account_inner(
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
        assert_eq!(
            goat.provider_id,
            ocg_core::provider::COMMAND_CODE_PROVIDER_ID
        );
        assert_eq!(goat.offering_id, ocg_core::provider::GOAT_OFFERING_ID);
        assert!(!goat.enabled, "GOAT must persist as a disabled draft");
        let goat_before = core.db.lock().get_account(&goat.id).unwrap().unwrap();
        let goat_enable = toggle_account_inner(&core, goat.id.clone())
            .expect_err("GOAT toggle enable must fail closed");
        assert!(goat_enable.contains("not routable"), "{goat_enable}");
        let goat_after = core.db.lock().get_account(&goat.id).unwrap().unwrap();
        assert!(!goat_after.enabled);
        assert_eq!(goat_after.updated_at, goat_before.updated_at);

        assert!(
            create_account_inner(
                &core,
                AccountInput {
                    provider_id: ocg_core::provider::OPENCODE_ZEN_FREE_PROVIDER_ID.into(),
                    offering_id: ocg_core::provider::ANONYMOUS_FREE_OFFERING_ID.into(),
                    name: "zen".into(),
                    username: None,
                    password: None,
                    key: String::new(),
                    referral_code: None,
                    purchase_date: None,
                    notes: None,
                },
            )
            .is_err()
        );

        let blank = create_account_inner(
            &core,
            AccountInput {
                provider_id: ocg_core::provider::default_provider_id(),
                offering_id: ocg_core::provider::default_offering_id(),
                name: "blank".into(),
                username: None,
                password: Some("".into()),
                key: "short".into(),
                referral_code: Some("ref".into()),
                purchase_date: Some("2026-01-15".into()),
                notes: None,
            },
        )
        .unwrap();
        assert!(blank.password_cipher.is_none());

        let listed = get_accounts_inner(&core).unwrap();
        assert_eq!(listed.len(), 4);

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
                notes: None,
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
        assert!(get_account_usage_inner(&core, goat.id).is_err());
        assert!(
            get_account_usage_inner(&core, ocg_core::provider::ZEN_FREE_ACCOUNT_ID.into()).is_err()
        );

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
                    notes: None,
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
        assert_eq!(get_accounts_inner(&core).unwrap().len(), 3);

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
                    notes: None,
                },
            )
            .is_err()
        );

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn unroutable_offerings_cannot_be_enabled_through_account_commands() {
        let (dir, core) = temp_core();
        for plan in ocg_core::provider::BUILTIN_PLANS
            .iter()
            .copied()
            .filter(|plan| !plan.routable && plan.offering.singleton_account_id.is_none())
        {
            let created = create_account_inner(
                &core,
                AccountInput {
                    provider_id: plan.offering.provider_id.into(),
                    offering_id: plan.offering.offering_id.into(),
                    name: format!("{}-tauri", plan.offering.offering_id),
                    username: None,
                    password: None,
                    key: if plan.key_prefix == Some(ocg_core::provider::SCNET_TOKEN_PLAN_KEY_PREFIX)
                    {
                        "sk-tp-tauri".into()
                    } else {
                        "draft-key".into()
                    },
                    referral_code: None,
                    purchase_date: None,
                    notes: None,
                },
            )
            .unwrap();
            assert!(!created.enabled, "{} must save disabled", plan.display_name);
            let before = core.db.lock().get_account(&created.id).unwrap().unwrap();
            assert!(
                toggle_account_inner(&core, created.id.clone())
                    .is_err_and(|error| error.contains("not routable"))
            );
            assert!(
                update_account_inner(
                    &core,
                    created.id.clone(),
                    AccountUpdate {
                        enabled: Some(true),
                        ..AccountUpdate::default()
                    },
                )
                .is_err_and(|error| error.contains("not routable"))
            );
            let after = core.db.lock().get_account(&created.id).unwrap().unwrap();
            assert!(!after.enabled);
            assert_eq!(after.updated_at, before.updated_at);
            let renamed = update_account_inner(
                &core,
                created.id.clone(),
                AccountUpdate {
                    name: Some(format!("{}-edited", plan.offering.offering_id)),
                    enabled: Some(false),
                    ..AccountUpdate::default()
                },
            )
            .unwrap();
            assert!(!renamed.enabled);
            assert_eq!(
                renamed.name,
                format!("{}-edited", plan.offering.offering_id)
            );
        }

        let go = create_account_inner(
            &core,
            AccountInput {
                provider_id: ocg_core::provider::OPENCODE_PROVIDER_ID.into(),
                offering_id: ocg_core::provider::GO_OFFERING_ID.into(),
                name: "go-tauri".into(),
                username: None,
                password: None,
                key: "sk-go-tauri".into(),
                referral_code: None,
                purchase_date: None,
                notes: None,
            },
        )
        .unwrap();
        assert!(go.enabled);
        let disabled = toggle_account_inner(&core, go.id.clone()).unwrap();
        assert!(!disabled.enabled);
        let enabled = toggle_account_inner(&core, go.id).unwrap();
        assert!(enabled.enabled);

        let _ = fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn tauri_account_mutations_skip_cas_and_enable_pending_custom() {
        let (dir, core) = temp_core();
        let revision = core.settings_revision();

        let go = create_account_inner(
            &core,
            AccountInput {
                provider_id: ocg_core::provider::OPENCODE_PROVIDER_ID.into(),
                offering_id: ocg_core::provider::GO_OFFERING_ID.into(),
                name: "go-cas".into(),
                username: None,
                password: None,
                key: "sk-go-cas".into(),
                referral_code: None,
                purchase_date: None,
                notes: None,
            },
        )
        .unwrap();
        assert!(go.enabled);
        assert_eq!(core.settings_revision(), revision);

        let custom = create_account_inner(
            &core,
            AccountInput {
                provider_id: ocg_core::provider::CUSTOM_PROVIDER_ID.into(),
                offering_id: ocg_core::provider::CUSTOM_API_OFFERING_ID.into(),
                name: "custom-cas".into(),
                username: None,
                password: None,
                key: "custom-key".into(),
                referral_code: None,
                purchase_date: None,
                notes: None,
            },
        )
        .unwrap();
        assert!(
            custom.enabled,
            "Tauri Custom create uses offering_allows_enablement and skips dashboard verification"
        );
        let contract = core.db.lock().load_account_contract(&custom.id).unwrap();
        assert_eq!(
            contract.verification.status,
            ocg_core::provider::ConnectionVerificationStatus::Pending
        );
        assert!(contract.custom_config.is_none());
        assert!(contract.model_capabilities.is_empty());
        assert_eq!(core.settings_revision(), revision);

        let disabled = toggle_account_inner(&core, custom.id.clone()).unwrap();
        assert!(!disabled.enabled);
        let reenabled = toggle_account_inner(&core, custom.id.clone()).unwrap();
        assert!(
            reenabled.enabled,
            "Tauri toggle does not consult Custom verification status"
        );
        assert_eq!(
            core.db
                .lock()
                .account_verification_state(&custom.id)
                .unwrap()
                .unwrap()
                .status,
            ocg_core::provider::ConnectionVerificationStatus::Pending
        );

        let renamed = update_account_inner(
            &core,
            custom.id.clone(),
            AccountUpdate {
                name: Some("custom-renamed".into()),
                enabled: Some(false),
                ..AccountUpdate::default()
            },
        )
        .unwrap();
        assert_eq!(renamed.name, "custom-renamed");
        assert!(!renamed.enabled);
        let forced = update_account_inner(
            &core,
            custom.id.clone(),
            AccountUpdate {
                enabled: Some(true),
                ..AccountUpdate::default()
            },
        )
        .unwrap();
        assert!(forced.enabled);

        let masked = test_account_inner(&core, custom.id.clone()).unwrap();
        assert!(masked.contains("custom-renamed"));
        assert!(core.gateway.lock().is_none());

        reset_account_cooldown_inner(&core, go.id.clone()).unwrap();
        delete_account_inner(&core, go.id.clone()).await.unwrap();
        assert_eq!(
            core.settings_revision(),
            revision,
            "Tauri account commands must not bump the dashboard CAS token"
        );

        let _ = fs::remove_dir_all(dir);
    }
}
