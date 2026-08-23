//! HTTP-neutral account control-plane mutations.
//!
//! Dashboard V3 adapters wrap these functions with CAS. The CLI calls the
//! same functions without a Dashboard CAS token; both paths bump
//! `settings_revision` after a successful persist. This module does not
//! serialize HTTP envelopes or import `dashboard` / `dashboard_v3` /
//! `gateway`.

use crate::browser::{BrowserProfileOperationKind, StagedBrowserProfiles};
use crate::models::{
    Account, AccountSetupStep, AccountType, AccountUpdate, normalize_account_notes,
};
use crate::provider::{
    GO_OFFERING_ID, OPENCODE_PROVIDER_ID, VerificationPolicy, ZEN_FREE_ACCOUNT_ID,
};
use crate::state::CoreState;
use chrono::Utc;
use std::fmt;

const ZEN_FREE_MUTATION_MESSAGE: &str =
    "Zen Free settings must use the dedicated provider-settings endpoint";
const ZEN_FREE_DELETE_MESSAGE: &str = "Zen Free is a built-in singleton and cannot be deleted";
const SETUP_INCOMPLETE_MESSAGE: &str = "account setup is not complete and cannot be enabled";
const VERIFY_BEFORE_ENABLE_MESSAGE: &str = "verify the account connection before enabling it";

#[derive(Debug)]
pub enum AccountControlError {
    NotFound,
    RevisionConflict,
    Invalid(String),
    Conflict(String),
    Unavailable(String),
    Internal(anyhow::Error),
}

impl fmt::Display for AccountControlError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFound => f.write_str("account not found"),
            Self::RevisionConflict => f.write_str("control-plane revision conflict"),
            Self::Invalid(message) | Self::Conflict(message) | Self::Unavailable(message) => {
                f.write_str(message)
            }
            Self::Internal(error) => write!(f, "{error}"),
        }
    }
}

impl std::error::Error for AccountControlError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Internal(error) => Some(error.as_ref()),
            Self::NotFound
            | Self::RevisionConflict
            | Self::Invalid(_)
            | Self::Conflict(_)
            | Self::Unavailable(_) => None,
        }
    }
}

/// Create an enabled ready OpenCode Go API-key account.
///
/// Holds `settings_update` and bumps `settings_revision` on success. Custom
/// and other catalog plans are not accepted here; the CLI surface stays
/// Go-only.
pub fn create_go_api_key(
    state: &CoreState,
    name: String,
    key: String,
    username: Option<String>,
    password: Option<String>,
) -> Result<Account, AccountControlError> {
    let _settings_update = state.settings_update.lock();
    let name = name.trim().to_string();
    if name.is_empty() {
        return Err(AccountControlError::Invalid("name is required".into()));
    }
    if name.chars().count() > 200 {
        return Err(AccountControlError::Invalid(
            "name must be at most 200 characters".into(),
        ));
    }
    let plan = crate::provider::builtin_plan(OPENCODE_PROVIDER_ID, GO_OFFERING_ID)
        .ok_or_else(|| AccountControlError::Invalid("unknown provider offering".into()))?;
    crate::provider::validate_plan_key(plan, &key)
        .map_err(|error| AccountControlError::Invalid(error.to_string()))?;
    let now = Utc::now();
    let id = uuid::Uuid::new_v4().to_string();
    let account = Account {
        id: id.clone(),
        provider_id: OPENCODE_PROVIDER_ID.to_string(),
        offering_id: GO_OFFERING_ID.to_string(),
        credential_kind: crate::provider::CredentialKind::ApiKey,
        quota_scope: crate::provider::QuotaScope::Key,
        name,
        username: clean_optional(username),
        password_cipher: encrypted_optional(state, password)?,
        key_cipher: state
            .encrypt_key(key.trim())
            .map_err(AccountControlError::Internal)?,
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
        notes: normalize_account_notes("")
            .map_err(|error| AccountControlError::Invalid(error.to_string()))?,
        created_at: now,
        updated_at: now,
    };
    crate::provider::ensure_enabled_offering_is_routable(
        &account.provider_id,
        &account.offering_id,
        account.enabled,
    )
    .map_err(|error| AccountControlError::Conflict(error.to_string()))?;
    {
        let db = state.db.lock();
        db.create_account_with_contract(&account, None, &[], None)
            .map_err(map_write_error)?;
        let _ = db.log_gateway(
            "info",
            "account",
            &format!("created account {}", account.name),
        );
    }
    commit_account(state, &id, true)
}

/// Enable or disable an account using Dashboard enablement policy.
///
/// Holds `settings_update` and bumps `settings_revision` on success. Pending
/// Custom accounts cannot be enabled; Zen Free is rejected.
pub fn set_account_enabled(
    state: &CoreState,
    id: &str,
    enabled: bool,
) -> Result<Account, AccountControlError> {
    let _settings_update = state.settings_update.lock();
    set_account_enabled_locked(state, id, enabled)
}

/// Same persist + revision bump as [`set_account_enabled`], for callers that
/// already hold `settings_update` (Dashboard CAS).
pub(crate) fn set_account_enabled_locked(
    state: &CoreState,
    id: &str,
    enabled: bool,
) -> Result<Account, AccountControlError> {
    let account = load_account(state, id)?;
    if account.is_zen_free() {
        return Err(AccountControlError::Invalid(
            ZEN_FREE_MUTATION_MESSAGE.into(),
        ));
    }
    if enabled && (!account.setup_step.is_ready() || account.key_cipher.is_empty()) {
        return Err(AccountControlError::Conflict(
            SETUP_INCOMPLETE_MESSAGE.into(),
        ));
    }
    if enabled {
        ensure_account_can_enable(state, &account)?;
    }
    let update = AccountUpdate {
        name: None,
        username: None,
        password: None,
        key: None,
        enabled: Some(enabled),
        referral_code: None,
        purchase_date: None,
        notes: None,
    };
    {
        let db = state.db.lock();
        db.update_account(id, &update, None, None)
            .map_err(map_write_error)?;
        let _ = db.log_gateway(
            "info",
            "account",
            &format!(
                "{} account {}",
                if enabled { "enabled" } else { "disabled" },
                account.name
            ),
        );
    }
    commit_account(state, id, false)
}

/// Delete an account, staging and purging its browser profiles.
///
/// Stops the native/remote browser without holding `settings_update`, then
/// re-locks for the persist + revision bump. `cas` is `(settings_revision,
/// process_generation)` rechecked after the await so Dashboard can keep
/// strong CAS; the CLI passes `None`. Does not cancel process-level workers.
pub async fn delete_account(
    state: &CoreState,
    id: &str,
    cas: Option<(u64, u64)>,
) -> Result<u64, AccountControlError> {
    {
        let _settings_update = state.settings_update.lock();
        check_cas(state, cas)?;
        reject_zen_free_delete(id)?;
        state
            .recover_browser_profiles_for_account(id)
            .map_err(AccountControlError::Internal)?;
        load_account(state, id)?;
    }

    let browser_operation = state.browser.operation().await;
    browser_operation
        .stop_account(id)
        .await
        .map_err(|error| AccountControlError::Unavailable(error.to_string()))?;

    let _settings_update = state.settings_update.lock();
    check_cas(state, cas)?;
    reject_zen_free_delete(id)?;
    let account = load_account(state, id)?;
    let staged = StagedBrowserProfiles::stage(
        &state.data_dir(),
        id,
        BrowserProfileOperationKind::DeleteAccount,
    )
    .map_err(AccountControlError::Internal)?;
    let delete_result = {
        let mut db = state.db.lock();
        let result = db.delete_account(id);
        if result.is_ok() {
            let _ = db.log_gateway(
                "info",
                "account",
                &format!("deleted account {} ({})", id, account.name),
            );
        }
        result
    };
    if let Err(error) = delete_result {
        let restore_error = staged.restore().err();
        return Err(AccountControlError::Internal(match restore_error {
            Some(restore) => anyhow::anyhow!(
                "failed to delete account: {error}; failed to restore browser profile: {restore}"
            ),
            None => anyhow::anyhow!("failed to delete account: {error}"),
        }));
    }
    let revision = state.bump_settings_revision();
    staged.purge().map_err(AccountControlError::Internal)?;
    state
        .reload_provider_contracts()
        .map_err(AccountControlError::Internal)?;
    Ok(revision)
}

pub(crate) fn ensure_account_can_enable(
    state: &CoreState,
    account: &Account,
) -> Result<(), AccountControlError> {
    crate::provider::ensure_offering_can_enable(&account.provider_id, &account.offering_id)
        .map_err(|error| AccountControlError::Conflict(error.to_string()))?;
    let plan = crate::provider::builtin_plan(&account.provider_id, &account.offering_id)
        .ok_or_else(|| AccountControlError::Invalid("unknown provider offering".into()))?;
    if plan.verification_policy == VerificationPolicy::Required {
        let status = state
            .db
            .lock()
            .account_verification_state(&account.id)
            .map_err(AccountControlError::Internal)?
            .map(|state| state.status)
            .unwrap_or(crate::provider::ConnectionVerificationStatus::Pending);
        if !status.allows_enablement() {
            return Err(AccountControlError::Conflict(
                VERIFY_BEFORE_ENABLE_MESSAGE.into(),
            ));
        }
    }
    Ok(())
}

fn commit_account(
    state: &CoreState,
    id: &str,
    reload_contracts: bool,
) -> Result<Account, AccountControlError> {
    let _revision = state.bump_settings_revision();
    if reload_contracts {
        state
            .reload_provider_contracts()
            .map_err(AccountControlError::Internal)?;
    }
    load_account(state, id)
}

fn load_account(state: &CoreState, id: &str) -> Result<Account, AccountControlError> {
    state
        .db
        .lock()
        .get_account(id)
        .map_err(AccountControlError::Internal)?
        .ok_or(AccountControlError::NotFound)
}

fn check_cas(state: &CoreState, cas: Option<(u64, u64)>) -> Result<(), AccountControlError> {
    let Some((revision, generation)) = cas else {
        return Ok(());
    };
    if revision != state.settings_revision() || generation != state.process_generation() {
        Err(AccountControlError::RevisionConflict)
    } else {
        Ok(())
    }
}

fn reject_zen_free_delete(id: &str) -> Result<(), AccountControlError> {
    if id == ZEN_FREE_ACCOUNT_ID {
        Err(AccountControlError::Invalid(ZEN_FREE_DELETE_MESSAGE.into()))
    } else {
        Ok(())
    }
}

fn clean_optional(value: Option<String>) -> Option<String> {
    value.and_then(|s| {
        let trimmed = s.trim().to_string();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        }
    })
}

fn encrypted_optional(
    state: &CoreState,
    value: Option<String>,
) -> Result<Option<String>, AccountControlError> {
    match value.as_deref().map(str::trim) {
        Some("") | None => Ok(None),
        Some(v) => state
            .encrypt_key(v)
            .map(Some)
            .map_err(AccountControlError::Internal),
    }
}

fn map_write_error(error: anyhow::Error) -> AccountControlError {
    if let Some(binding) = error.downcast_ref::<crate::provider::ProviderBindingError>() {
        return AccountControlError::Conflict(binding.to_string());
    }
    let message = error.to_string();
    if message.contains("not routable") {
        AccountControlError::Conflict(message)
    } else {
        AccountControlError::Internal(error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::{KeyCipher, StaticKeyCipher};
    use crate::db::Database;
    use crate::models::{AccountSetupStep, AccountType};
    use crate::provider::{CUSTOM_API_OFFERING_ID, CUSTOM_PROVIDER_ID};
    use crate::state::CoreStateInner;
    use std::sync::Arc;

    fn temp_state(label: &str) -> (Arc<CoreStateInner>, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!(
            "ocg-account-control-{label}-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let db = Database::open(dir.clone()).unwrap();
        let cipher: Arc<dyn KeyCipher + Send + Sync> =
            Arc::new(StaticKeyCipher::new("account-control"));
        (
            Arc::new(CoreStateInner::new(db, dir.clone(), cipher).unwrap()),
            dir,
        )
    }

    fn custom_pending(state: &CoreStateInner, id: &str) -> Account {
        let now = Utc::now();
        Account {
            id: id.to_string(),
            provider_id: CUSTOM_PROVIDER_ID.to_string(),
            offering_id: CUSTOM_API_OFFERING_ID.to_string(),
            credential_kind: crate::provider::CredentialKind::ApiKey,
            quota_scope: crate::provider::QuotaScope::Key,
            name: id.to_string(),
            username: None,
            password_cipher: None,
            key_cipher: state.encrypt_key("custom-key").unwrap(),
            enabled: false,
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
        }
    }

    #[test]
    fn go_create_and_toggle_bump_revision_and_reject_pending_custom() {
        let (state, dir) = temp_state("go-toggle");
        let before = state.settings_revision();
        let created = create_go_api_key(
            &state,
            "go-main".into(),
            "sk-go".into(),
            Some("  alice  ".into()),
            Some("  secret  ".into()),
        )
        .unwrap();
        assert!(created.enabled);
        assert_eq!(created.provider_id, OPENCODE_PROVIDER_ID);
        assert_eq!(created.offering_id, GO_OFFERING_ID);
        assert_eq!(created.username.as_deref(), Some("alice"));
        assert_eq!(state.settings_revision(), before + 1);

        let disabled = set_account_enabled(&state, &created.id, false).unwrap();
        assert!(!disabled.enabled);
        assert_eq!(state.settings_revision(), before + 2);

        let enabled = set_account_enabled(&state, &created.id, true).unwrap();
        assert!(enabled.enabled);
        assert_eq!(state.settings_revision(), before + 3);

        state
            .db
            .lock()
            .create_account(&custom_pending(&state, "cli-custom"))
            .unwrap();
        let error = set_account_enabled(&state, "cli-custom", true).unwrap_err();
        match error {
            AccountControlError::Conflict(message) => {
                assert_eq!(message, VERIFY_BEFORE_ENABLE_MESSAGE);
            }
            other => panic!("expected verify-first conflict, got {other}"),
        }
        assert!(
            !state
                .db
                .lock()
                .get_account("cli-custom")
                .unwrap()
                .unwrap()
                .enabled
        );
        assert_eq!(state.settings_revision(), before + 3);

        let zen = set_account_enabled(&state, ZEN_FREE_ACCOUNT_ID, false).unwrap_err();
        assert!(
            matches!(zen, AccountControlError::Invalid(message) if message == ZEN_FREE_MUTATION_MESSAGE)
        );

        drop(state);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn delete_account_bumps_revision_and_rejects_zen() {
        let (state, dir) = temp_state("delete");
        let created =
            create_go_api_key(&state, "gone".into(), "sk-gone".into(), None, None).unwrap();
        let before = state.settings_revision();
        delete_account(&state, &created.id, None).await.unwrap();
        assert!(state.db.lock().get_account(&created.id).unwrap().is_none());
        assert_eq!(state.settings_revision(), before + 1);

        let zen = delete_account(&state, ZEN_FREE_ACCOUNT_ID, None)
            .await
            .unwrap_err();
        assert!(
            matches!(zen, AccountControlError::Invalid(message) if message == ZEN_FREE_DELETE_MESSAGE)
        );
        assert!(
            state
                .db
                .lock()
                .get_account(ZEN_FREE_ACCOUNT_ID)
                .unwrap()
                .is_some()
        );

        drop(state);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn production_source_is_http_and_gateway_neutral() {
        let production = include_str!("account_control.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("production source precedes tests");
        for needle in [
            "crate::dashboard",
            "crate::dashboard_v3",
            "crate::gateway",
            "expected_revision",
            "expectedRevision",
        ] {
            assert!(
                !production.contains(needle),
                "account_control must stay HTTP-neutral, missing-CAS for CLI, found {needle}"
            );
        }
        assert!(production.contains("bump_settings_revision"));
        assert!(production.contains("VERIFY_BEFORE_ENABLE_MESSAGE"));
    }
}
