//! Password-encrypted account migration for Dashboard V3.
//!
//! Plaintext upstream Keys are decrypted and re-encrypted only inside the Host.
//! The dashboard receives a versioned Argon2id + AES-256-GCM envelope, plus
//! secret-free previews/results. Browser profiles, cookies, usage, cooldowns,
//! verification evidence, saved passwords, and referral codes are not portable.

use std::collections::HashSet;
use std::sync::{Arc, OnceLock};

use aes_gcm::aead::{Aead, KeyInit, Payload};
use aes_gcm::{Aes256Gcm, Nonce};
use argon2::{Algorithm, Argon2, Params, Version};
use axum::Json;
use axum::body::Bytes;
use axum::extract::State;
use axum::http::{HeaderMap, HeaderValue, header};
use axum::response::{IntoResponse, Response};
use base64::{Engine, engine::general_purpose::STANDARD};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use zeroize::{Zeroize, Zeroizing};

use crate::dashboard_session;
use crate::db::AccountImportRecord;
use crate::models::{
    Account as ModelAccount, AccountCustomConfigInput, AccountModelCapabilityInput,
    AccountSetupStep as ModelSetupStep, AccountType as ModelAccountType, normalize_account_notes,
    normalize_purchase_date,
};
use crate::provider::{
    CreationAvailability, UpstreamProtocolKind, VerificationPolicy, builtin_plan,
    offering_allows_enablement,
};
use crate::state::CoreState;

use super::types::{
    AccountExport, AccountExportRequest, AccountImportDisposition, AccountImportPreview,
    AccountImportPreviewItem, AccountImportPreviewRequest, AccountImportRequest,
    AccountImportResult,
};
use super::{V3ApiError, check_expectation, parse_json, parse_mutation_json};

const ENVELOPE_FORMAT: &str = "ocg-manager-account-backup";
const ENVELOPE_VERSION: u32 = 1;
const PAYLOAD_VERSION: u32 = 1;
const AAD: &[u8] = b"ocg-manager-account-backup:v1:argon2id-m65536-t3-p1:aes-256-gcm";
const ARGON_MEMORY_KIB: u32 = 64 * 1024;
const ARGON_ITERATIONS: u32 = 3;
const ARGON_LANES: u32 = 1;
const SALT_LEN: usize = 16;
const NONCE_LEN: usize = 12;
const MIN_BUNDLE_PASSWORD_CHARS: usize = 12;
const MAX_PASSWORD_CHARS: usize = 256;
const MAX_ADMIN_USERNAME_CHARS: usize = 64;
pub(super) const MAX_REQUEST_BYTES: usize = 4 * 1024 * 1024;
const MAX_BUNDLE_BYTES: usize = 3 * 1024 * 1024;
const MAX_PLAINTEXT_BYTES: usize = 2 * 1024 * 1024;
const MAX_ACCOUNTS: usize = 200;
const MAX_NAME_CHARS: usize = 200;
const MAX_USERNAME_CHARS: usize = 320;
const MAX_KEY_CHARS: usize = 16 * 1024;
const MAX_NOTES_CHARS: usize = 4000;
const MAX_ENDPOINT_CHARS: usize = 2048;
const MAX_CAPABILITIES: usize = 200;

static CRYPTO_GATE: OnceLock<Arc<Semaphore>> = OnceLock::new();

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct EncryptedEnvelope {
    format: String,
    version: u32,
    salt: String,
    nonce: String,
    ciphertext: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PortablePayload {
    version: u32,
    exported_at: String,
    accounts: Vec<PortableAccount>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PortableAccount {
    provider_id: String,
    offering_id: String,
    name: String,
    username: Option<String>,
    key: String,
    enabled: bool,
    account_type: String,
    setup_step: String,
    purchase_date: String,
    notes: Option<String>,
    custom_config: Option<PortableCustomConfig>,
    model_capabilities: Vec<PortableModelCapability>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PortableCustomConfig {
    endpoint_url: String,
    upstream_protocol: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PortableModelCapability {
    model_id: String,
    protocol: String,
}

impl Zeroize for PortablePayload {
    fn zeroize(&mut self) {
        self.version.zeroize();
        self.exported_at.zeroize();
        self.accounts.zeroize();
    }
}

impl Zeroize for PortableAccount {
    fn zeroize(&mut self) {
        self.provider_id.zeroize();
        self.offering_id.zeroize();
        self.name.zeroize();
        self.username.zeroize();
        self.key.zeroize();
        self.enabled.zeroize();
        self.account_type.zeroize();
        self.setup_step.zeroize();
        self.purchase_date.zeroize();
        self.notes.zeroize();
        self.custom_config.zeroize();
        self.model_capabilities.zeroize();
    }
}

impl Zeroize for PortableCustomConfig {
    fn zeroize(&mut self) {
        self.endpoint_url.zeroize();
        self.upstream_protocol.zeroize();
    }
}

impl Zeroize for PortableModelCapability {
    fn zeroize(&mut self) {
        self.model_id.zeroize();
        self.protocol.zeroize();
    }
}

#[derive(Debug)]
struct ValidatedAccount {
    portable_index: usize,
    provider_id: String,
    offering_id: String,
    name: String,
    username: Option<String>,
    key: Zeroizing<String>,
    enabled: bool,
    account_type: ModelAccountType,
    setup_step: ModelSetupStep,
    purchase_date: String,
    notes: Option<String>,
    custom_config: Option<AccountCustomConfigInput>,
    capabilities: Vec<AccountModelCapabilityInput>,
}

#[derive(Debug)]
enum TransferError {
    Invalid(String),
    InvalidBundle,
    AdminMissing,
    Unauthorized,
    Busy,
    InsecureTransport,
    Internal,
}

pub(super) async fn export_accounts(
    State(state): State<CoreState>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    no_store(
        export_accounts_inner(state, headers, body)
            .await
            .into_response(),
    )
}

pub(super) async fn preview_import(
    State(state): State<CoreState>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    no_store(
        preview_import_inner(state, headers, body)
            .await
            .into_response(),
    )
}

pub(super) async fn import_accounts(
    State(state): State<CoreState>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    no_store(
        import_accounts_inner(state, headers, body)
            .await
            .into_response(),
    )
}

async fn export_accounts_inner(
    state: CoreState,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<AccountExport>, V3ApiError> {
    ensure_transport(&state, &headers)?;
    ensure_body_bound(&state, &body)?;
    let input = parse_json::<AccountExportRequest>(&body)?;
    validate_admin_credentials(&input.admin_username, &input.admin_password)
        .map_err(|error| map_transfer_error(&state, error))?;
    validate_bundle_password(&input.bundle_password)
        .map_err(|error| map_transfer_error(&state, error))?;
    if !dashboard_session::is_initialized(&state.db)
        .map_err(|_| V3ApiError::internal("failed to inspect administrator state"))?
    {
        return Err(map_transfer_error(&state, TransferError::AdminMissing));
    }
    let permit = crypto_permit().map_err(|error| map_transfer_error(&state, error))?;
    let blocking_state = state.clone();
    let admin_username = Zeroizing::new(input.admin_username);
    let admin_password = Zeroizing::new(input.admin_password);
    let bundle_password = Zeroizing::new(input.bundle_password);
    let exported = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        if !dashboard_session::credentials_match(
            &blocking_state.db,
            admin_username.as_str(),
            admin_password.as_str(),
        )
        .map_err(|_| TransferError::Internal)?
        {
            return Err(TransferError::Unauthorized);
        }
        let (payload, skipped_accounts, revision) = export_payload(&blocking_state)?;
        let payload = Zeroizing::new(payload);
        let exported_accounts = payload.accounts.len() as u64;
        let bundle = encrypt_payload(&payload, bundle_password.as_str())?;
        Ok::<_, TransferError>((bundle, exported_accounts, skipped_accounts, revision))
    })
    .await
    .map_err(|_| V3ApiError::internal("account export worker failed"))?
    .map_err(|error| map_transfer_error(&state, error))?;
    let (bundle, exported_accounts, skipped_accounts, revision) = exported;
    Ok(Json(AccountExport {
        filename: format!(
            "ocg-manager-accounts-{}.ocgbackup",
            Utc::now().format("%Y%m%d-%H%M%S")
        ),
        bundle,
        exported_accounts,
        skipped_accounts,
        revision,
        process_generation: state.process_generation(),
    }))
}

async fn preview_import_inner(
    state: CoreState,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<AccountImportPreview>, V3ApiError> {
    ensure_transport(&state, &headers)?;
    ensure_body_bound(&state, &body)?;
    let input = parse_json::<AccountImportPreviewRequest>(&body)?;
    validate_bundle_password(&input.password).map_err(|error| map_transfer_error(&state, error))?;
    ensure_bundle_bound(&input.bundle).map_err(|error| map_transfer_error(&state, error))?;
    let permit = crypto_permit().map_err(|error| map_transfer_error(&state, error))?;
    let password = Zeroizing::new(input.password);
    let bundle = Zeroizing::new(input.bundle);
    let (exported_at, validated) = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        decrypt_and_validate(bundle.as_str(), password.as_str())
    })
    .await
    .map_err(|_| V3ApiError::internal("account import preview worker failed"))?
    .map_err(|error| map_transfer_error(&state, error))?;
    let (items, importable_accounts, duplicate_accounts, revision) =
        preview_against_current(&state, &validated)?;
    Ok(Json(AccountImportPreview {
        exported_at,
        items,
        importable_accounts,
        duplicate_accounts,
        revision,
        process_generation: state.process_generation(),
    }))
}

async fn import_accounts_inner(
    state: CoreState,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<AccountImportResult>, V3ApiError> {
    ensure_transport(&state, &headers)?;
    ensure_body_bound(&state, &body)?;
    let input = parse_mutation_json::<AccountImportRequest>(&body)?;
    validate_bundle_password(&input.password).map_err(|error| map_transfer_error(&state, error))?;
    ensure_bundle_bound(&input.bundle).map_err(|error| map_transfer_error(&state, error))?;
    let expectation = input.expectation;
    let permit = crypto_permit().map_err(|error| map_transfer_error(&state, error))?;
    let password = Zeroizing::new(input.password);
    let bundle = Zeroizing::new(input.bundle);
    let (_, validated) = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        decrypt_and_validate(bundle.as_str(), password.as_str())
    })
    .await
    .map_err(|_| V3ApiError::internal("account import worker failed"))?
    .map_err(|error| map_transfer_error(&state, error))?;

    let _settings_update = state.settings_update.lock();
    check_expectation(&state, &expectation)?;
    let existing = current_logical_accounts(&state)?;
    let mut records = Vec::new();
    let mut items = Vec::with_capacity(validated.len());
    let mut duplicate_accounts = 0_u64;
    for account in validated {
        let logical = logical_key(&account.provider_id, &account.offering_id, &account.name);
        if existing.contains(&logical) {
            duplicate_accounts += 1;
            items.push(preview_item(
                &account,
                AccountImportDisposition::Duplicate,
                Some("an account with the same Plan and name already exists".to_string()),
            ));
            continue;
        }
        let now = Utc::now();
        let id = uuid::Uuid::new_v4().to_string();
        let key_cipher = if account.key.is_empty() {
            String::new()
        } else {
            state
                .encrypt_key(account.key.as_str())
                .map_err(|_| V3ApiError::internal("failed to protect an imported credential"))?
        };
        let model = ModelAccount {
            id,
            provider_id: account.provider_id.clone(),
            offering_id: account.offering_id.clone(),
            credential_kind: builtin_plan(&account.provider_id, &account.offering_id)
                .expect("validated Plan must remain sealed")
                .offering
                .credential_kind,
            quota_scope: builtin_plan(&account.provider_id, &account.offering_id)
                .expect("validated Plan must remain sealed")
                .offering
                .quota_scope,
            name: account.name.clone(),
            username: account.username.clone(),
            password_cipher: None,
            key_cipher,
            enabled: account.enabled,
            account_type: account.account_type,
            setup_step: account.setup_step,
            referral_code: None,
            purchase_date: account.purchase_date.clone(),
            expires_on: String::new(),
            cooldown_until: None,
            cooldown_generic_until: None,
            cooldown_5h_until: None,
            cooldown_week_until: None,
            cooldown_month_until: None,
            cooldown_free_until: None,
            last_error: None,
            auth_error: None,
            notes: account.notes.clone(),
            created_at: now,
            updated_at: now,
        };
        records.push(AccountImportRecord {
            account: model,
            custom_config: account.custom_config.clone(),
            capabilities: account.capabilities.clone(),
        });
        items.push(preview_item(
            &account,
            AccountImportDisposition::Imported,
            None,
        ));
    }

    let imported_accounts = records.len() as u64;
    let revision = if records.is_empty() {
        state.settings_revision()
    } else {
        state
            .db
            .lock()
            .import_accounts_with_contracts(&records)
            .map_err(|_| V3ApiError::internal("failed to import account migration package"))?;
        let revision = state.bump_settings_revision();
        state.reload_provider_contracts().map_err(|_| {
            V3ApiError::internal("accounts imported but runtime contracts could not reload")
        })?;
        revision
    };

    Ok(Json(AccountImportResult {
        items,
        imported_accounts,
        duplicate_accounts,
        revision,
        process_generation: state.process_generation(),
    }))
}

fn export_payload(state: &CoreState) -> Result<(PortablePayload, u64, u64), TransferError> {
    let _settings_update = state.settings_update.lock();
    let revision = state.settings_revision();
    let snapshots = {
        let db = state.db.lock();
        let accounts = db.list_accounts().map_err(|_| TransferError::Internal)?;
        accounts
            .into_iter()
            .map(|account| {
                let contract = db
                    .load_account_contract(&account.id)
                    .map_err(|_| TransferError::Internal)?;
                Ok((account, contract))
            })
            .collect::<Result<Vec<_>, TransferError>>()?
    };
    let mut accounts = Zeroizing::new(Vec::new());
    let mut skipped = 0_u64;
    for (account, contract) in snapshots {
        if account.is_zen_free() {
            skipped += 1;
            continue;
        }
        let portable_key_required = migration_exports_key(account.account_type, account.setup_step);
        let mut key = Zeroizing::new(if !portable_key_required || account.key_cipher.is_empty() {
            String::new()
        } else {
            state
                .decrypt_key(&account.key_cipher)
                .map_err(|_| TransferError::Internal)?
        });
        if portable_key_required && key.trim().is_empty() {
            return Err(TransferError::Internal);
        }
        accounts.push(PortableAccount {
            provider_id: account.provider_id,
            offering_id: account.offering_id,
            name: account.name,
            username: account.username,
            key: std::mem::take(&mut *key),
            enabled: account.enabled,
            account_type: account.account_type.as_str().to_string(),
            setup_step: account.setup_step.as_str().to_string(),
            purchase_date: account.purchase_date,
            notes: account.notes,
            custom_config: contract.custom_config.map(|config| PortableCustomConfig {
                endpoint_url: config.endpoint_url,
                upstream_protocol: config.upstream_protocol.as_str().to_string(),
            }),
            model_capabilities: contract
                .model_capabilities
                .into_iter()
                .map(|capability| PortableModelCapability {
                    model_id: capability.model_id,
                    protocol: capability.protocol.as_str().to_string(),
                })
                .collect(),
        });
    }
    if accounts.len() > MAX_ACCOUNTS {
        return Err(TransferError::Invalid(format!(
            "at most {MAX_ACCOUNTS} accounts can be exported at once"
        )));
    }
    Ok((
        PortablePayload {
            version: PAYLOAD_VERSION,
            exported_at: Utc::now().to_rfc3339(),
            accounts: std::mem::take(&mut *accounts),
        },
        skipped,
        revision,
    ))
}

fn migration_exports_key(account_type: ModelAccountType, setup_step: ModelSetupStep) -> bool {
    account_type == ModelAccountType::Key
        || (account_type == ModelAccountType::Managed && setup_step == ModelSetupStep::Ready)
}

fn encrypt_payload(payload: &PortablePayload, password: &str) -> Result<String, TransferError> {
    let mut salt = [0_u8; SALT_LEN];
    let mut nonce = [0_u8; NONCE_LEN];
    getrandom::fill(&mut salt).map_err(|_| TransferError::Internal)?;
    getrandom::fill(&mut nonce).map_err(|_| TransferError::Internal)?;
    encrypt_payload_with_material(payload, password, salt, nonce)
}

fn encrypt_payload_with_material(
    payload: &PortablePayload,
    password: &str,
    salt: [u8; SALT_LEN],
    nonce: [u8; NONCE_LEN],
) -> Result<String, TransferError> {
    let plaintext =
        Zeroizing::new(serde_json::to_vec(payload).map_err(|_| TransferError::Internal)?);
    if plaintext.len() > MAX_PLAINTEXT_BYTES {
        return Err(TransferError::Invalid(
            "account backup is too large".to_string(),
        ));
    }
    let key = derive_key(password, &salt)?;
    let cipher = Aes256Gcm::new_from_slice(key.as_ref()).map_err(|_| TransferError::Internal)?;
    let ciphertext = cipher
        .encrypt(
            Nonce::from_slice(&nonce),
            Payload {
                msg: plaintext.as_slice(),
                aad: AAD,
            },
        )
        .map_err(|_| TransferError::Internal)?;
    let envelope = EncryptedEnvelope {
        format: ENVELOPE_FORMAT.to_string(),
        version: ENVELOPE_VERSION,
        salt: STANDARD.encode(salt),
        nonce: STANDARD.encode(nonce),
        ciphertext: STANDARD.encode(ciphertext),
    };
    serde_json::to_string_pretty(&envelope).map_err(|_| TransferError::Internal)
}

fn decrypt_and_validate(
    bundle: &str,
    password: &str,
) -> Result<(String, Vec<ValidatedAccount>), TransferError> {
    let envelope: EncryptedEnvelope =
        serde_json::from_str(bundle).map_err(|_| TransferError::InvalidBundle)?;
    if envelope.format != ENVELOPE_FORMAT || envelope.version != ENVELOPE_VERSION {
        return Err(TransferError::InvalidBundle);
    }
    let salt = STANDARD
        .decode(envelope.salt)
        .map_err(|_| TransferError::InvalidBundle)?;
    let nonce = STANDARD
        .decode(envelope.nonce)
        .map_err(|_| TransferError::InvalidBundle)?;
    let ciphertext = STANDARD
        .decode(envelope.ciphertext)
        .map_err(|_| TransferError::InvalidBundle)?;
    if salt.len() != SALT_LEN
        || nonce.len() != NONCE_LEN
        || ciphertext.len() > MAX_PLAINTEXT_BYTES + 32
    {
        return Err(TransferError::InvalidBundle);
    }
    let key = derive_key(password, &salt)?;
    let cipher = Aes256Gcm::new_from_slice(key.as_ref()).map_err(|_| TransferError::Internal)?;
    let plaintext = Zeroizing::new(
        cipher
            .decrypt(
                Nonce::from_slice(&nonce),
                Payload {
                    msg: &ciphertext,
                    aad: AAD,
                },
            )
            .map_err(|_| TransferError::InvalidBundle)?,
    );
    if plaintext.len() > MAX_PLAINTEXT_BYTES {
        return Err(TransferError::InvalidBundle);
    }
    let payload: PortablePayload =
        serde_json::from_slice(plaintext.as_slice()).map_err(|_| TransferError::InvalidBundle)?;
    validate_payload(payload)
}

fn derive_key(password: &str, salt: &[u8]) -> Result<Zeroizing<[u8; 32]>, TransferError> {
    let params = Params::new(ARGON_MEMORY_KIB, ARGON_ITERATIONS, ARGON_LANES, Some(32))
        .map_err(|_| TransferError::Internal)?;
    let argon = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
    let mut key = Zeroizing::new([0_u8; 32]);
    argon
        .hash_password_into(password.as_bytes(), salt, key.as_mut())
        .map_err(|_| TransferError::Internal)?;
    Ok(key)
}

fn validate_payload(
    payload: PortablePayload,
) -> Result<(String, Vec<ValidatedAccount>), TransferError> {
    let mut payload = Zeroizing::new(payload);
    if payload.version != PAYLOAD_VERSION || payload.accounts.len() > MAX_ACCOUNTS {
        return Err(TransferError::InvalidBundle);
    }
    if payload.exported_at.chars().count() > 64
        || DateTime::parse_from_rfc3339(&payload.exported_at).is_err()
    {
        return Err(TransferError::InvalidBundle);
    }
    let exported_at = payload.exported_at.clone();
    let mut logical = HashSet::new();
    let mut validated = Vec::with_capacity(payload.accounts.len());
    for (index, account) in payload.accounts.iter_mut().enumerate() {
        let prefix = || format!("account {}", index + 1);
        account.provider_id = account.provider_id.trim().to_string();
        account.offering_id = account.offering_id.trim().to_string();
        account.name = account.name.trim().to_string();
        account.key = account.key.trim().to_string();
        if account.key.chars().count() > MAX_KEY_CHARS {
            return Err(TransferError::Invalid(format!(
                "{} has an account Key that is too long",
                prefix()
            )));
        }
        if account
            .username
            .as_deref()
            .is_some_and(|value| value.chars().count() > MAX_USERNAME_CHARS)
        {
            return Err(TransferError::Invalid(format!(
                "{} has a username that is too long",
                prefix()
            )));
        }
        if account.name.is_empty() || account.name.chars().count() > MAX_NAME_CHARS {
            return Err(TransferError::Invalid(format!(
                "{} has an invalid name",
                prefix()
            )));
        }
        let plan = builtin_plan(&account.provider_id, &account.offering_id)
            .ok_or_else(|| TransferError::Invalid(format!("{} uses an unknown Plan", prefix())))?;
        if plan.offering.singleton_account_id.is_some()
            || plan.creation_availability == CreationAvailability::Unavailable
        {
            return Err(TransferError::Invalid(format!(
                "{} uses a Plan that cannot be imported",
                prefix()
            )));
        }
        let account_type =
            ModelAccountType::try_from(account.account_type.as_str()).map_err(|_| {
                TransferError::Invalid(format!("{} has an invalid account type", prefix()))
            })?;
        let source_setup = ModelSetupStep::try_from(account.setup_step.as_str()).map_err(|_| {
            TransferError::Invalid(format!("{} has an invalid setup step", prefix()))
        })?;
        let (setup_step, key, enabled) = match account_type {
            ModelAccountType::Key => {
                if source_setup != ModelSetupStep::Ready || account.key.is_empty() {
                    return Err(TransferError::Invalid(format!(
                        "{} is missing its account Key",
                        prefix()
                    )));
                }
                crate::provider::validate_plan_key(plan, &account.key).map_err(|_| {
                    TransferError::Invalid(format!("{} has an invalid account Key", prefix()))
                })?;
                (
                    ModelSetupStep::Ready,
                    Zeroizing::new(std::mem::take(&mut account.key)),
                    account.enabled
                        && offering_allows_enablement(&account.provider_id, &account.offering_id)
                        && plan.verification_policy == VerificationPolicy::NotRequired,
                )
            }
            ModelAccountType::Managed => {
                if !plan.managed_registration {
                    return Err(TransferError::Invalid(format!(
                        "{} is not a supported managed account",
                        prefix()
                    )));
                }
                if source_setup == ModelSetupStep::Ready {
                    if account.key.is_empty() {
                        return Err(TransferError::Invalid(format!(
                            "{} is missing its managed account Key",
                            prefix()
                        )));
                    }
                    crate::provider::validate_plan_key(plan, &account.key).map_err(|_| {
                        TransferError::Invalid(format!("{} has an invalid account Key", prefix()))
                    })?;
                    (
                        ModelSetupStep::Ready,
                        Zeroizing::new(std::mem::take(&mut account.key)),
                        account.enabled
                            && offering_allows_enablement(
                                &account.provider_id,
                                &account.offering_id,
                            ),
                    )
                } else {
                    (
                        ModelSetupStep::GoogleAccount,
                        Zeroizing::new(String::new()),
                        false,
                    )
                }
            }
        };
        let purchase_date = if account.purchase_date.trim().is_empty() {
            String::new()
        } else {
            normalize_purchase_date(&account.purchase_date).map_err(|_| {
                TransferError::Invalid(format!("{} has an invalid purchase date", prefix()))
            })?
        };
        let notes = match account.notes.as_deref() {
            Some(value) if value.chars().count() > MAX_NOTES_CHARS => {
                return Err(TransferError::Invalid(format!(
                    "{} has notes that are too long",
                    prefix()
                )));
            }
            Some(value) => normalize_account_notes(value)
                .map_err(|_| TransferError::Invalid(format!("{} has invalid notes", prefix())))?,
            None => None,
        };
        let requires_custom = crate::provider::plan_requires_custom_config(plan);
        let (custom_config, capabilities) = if requires_custom {
            let config = account.custom_config.as_ref().ok_or_else(|| {
                TransferError::Invalid(format!("{} is missing its Custom Endpoint", prefix()))
            })?;
            if config.endpoint_url.chars().count() > MAX_ENDPOINT_CHARS {
                return Err(TransferError::Invalid(format!(
                    "{} has a Custom Endpoint that is too long",
                    prefix()
                )));
            }
            let endpoint_url = crate::custom::validate_custom_endpoint_url(&config.endpoint_url)
                .map_err(|_| {
                    TransferError::Invalid(format!("{} has an invalid Custom Endpoint", prefix()))
                })?;
            let protocol = UpstreamProtocolKind::try_from(config.upstream_protocol.as_str())
                .map_err(|_| {
                    TransferError::Invalid(format!("{} has an invalid upstream protocol", prefix()))
                })?;
            if account.model_capabilities.is_empty()
                || account.model_capabilities.len() > MAX_CAPABILITIES
            {
                return Err(TransferError::Invalid(format!(
                    "{} has an invalid model capability list",
                    prefix()
                )));
            }
            let mut seen_models = HashSet::new();
            let capabilities = account
                .model_capabilities
                .iter()
                .map(|capability| {
                    let model_id = capability.model_id.trim().to_string();
                    crate::provider::validate_custom_model_id(&model_id).map_err(|_| {
                        TransferError::Invalid(format!("{} has an invalid model ID", prefix()))
                    })?;
                    if !seen_models.insert(model_id.clone()) {
                        return Err(TransferError::Invalid(format!(
                            "{} contains duplicate model IDs",
                            prefix()
                        )));
                    }
                    let capability_protocol = UpstreamProtocolKind::try_from(
                        capability.protocol.as_str(),
                    )
                    .map_err(|_| {
                        TransferError::Invalid(format!(
                            "{} has an invalid model protocol",
                            prefix()
                        ))
                    })?;
                    if capability_protocol != protocol {
                        return Err(TransferError::Invalid(format!(
                            "{} has a model protocol mismatch",
                            prefix()
                        )));
                    }
                    Ok(AccountModelCapabilityInput {
                        model_id,
                        protocol: capability_protocol,
                        source: Some("import".to_string()),
                    })
                })
                .collect::<Result<Vec<_>, TransferError>>()?;
            crate::custom::validate_custom_capability_expansion(protocol, &capabilities).map_err(
                |_| TransferError::Invalid(format!("{} has invalid Custom capabilities", prefix())),
            )?;
            (
                Some(AccountCustomConfigInput {
                    endpoint_url,
                    upstream_protocol: protocol,
                }),
                capabilities,
            )
        } else {
            if account.custom_config.is_some() || !account.model_capabilities.is_empty() {
                return Err(TransferError::Invalid(format!(
                    "{} contains Custom-only fields",
                    prefix()
                )));
            }
            (None, Vec::new())
        };
        let logical_key = logical_key(&account.provider_id, &account.offering_id, &account.name);
        if !logical.insert(logical_key) {
            return Err(TransferError::Invalid(format!(
                "{} duplicates an earlier account in the package",
                prefix()
            )));
        }
        validated.push(ValidatedAccount {
            portable_index: index,
            provider_id: account.provider_id.clone(),
            offering_id: account.offering_id.clone(),
            name: account.name.clone(),
            username: account.username.clone().and_then(trim_optional),
            key,
            enabled: if requires_custom { false } else { enabled },
            account_type,
            setup_step,
            purchase_date,
            notes,
            custom_config,
            capabilities,
        });
    }
    Ok((exported_at, validated))
}

fn preview_against_current(
    state: &CoreState,
    validated: &[ValidatedAccount],
) -> Result<(Vec<AccountImportPreviewItem>, u64, u64, u64), V3ApiError> {
    let _settings_update = state.settings_update.lock();
    let revision = state.settings_revision();
    let existing = current_logical_accounts(state)?;
    let mut importable = 0_u64;
    let mut duplicates = 0_u64;
    let items = validated
        .iter()
        .map(|account| {
            let duplicate = existing.contains(&logical_key(
                &account.provider_id,
                &account.offering_id,
                &account.name,
            ));
            if duplicate {
                duplicates += 1;
                preview_item(
                    account,
                    AccountImportDisposition::Duplicate,
                    Some("an account with the same Plan and name already exists".to_string()),
                )
            } else {
                importable += 1;
                preview_item(account, AccountImportDisposition::Import, None)
            }
        })
        .collect();
    Ok((items, importable, duplicates, revision))
}

fn current_logical_accounts(
    state: &CoreState,
) -> Result<HashSet<(String, String, String)>, V3ApiError> {
    Ok(state
        .db
        .lock()
        .list_accounts()
        .map_err(|_| V3ApiError::internal("failed to inspect existing accounts"))?
        .into_iter()
        .filter(|account| !account.is_zen_free())
        .map(|account| logical_key(&account.provider_id, &account.offering_id, &account.name))
        .collect())
}

fn preview_item(
    account: &ValidatedAccount,
    disposition: AccountImportDisposition,
    reason: Option<String>,
) -> AccountImportPreviewItem {
    AccountImportPreviewItem {
        index: account.portable_index as u64,
        name: account.name.clone(),
        provider_id: account.provider_id.clone(),
        offering_id: account.offering_id.clone(),
        account_type: account.account_type.into(),
        disposition,
        reason,
    }
}

fn logical_key(provider_id: &str, offering_id: &str, name: &str) -> (String, String, String) {
    (
        provider_id.trim().to_string(),
        offering_id.trim().to_string(),
        name.trim().to_string(),
    )
}

fn trim_optional(value: String) -> Option<String> {
    let value = value.trim().to_string();
    (!value.is_empty()).then_some(value)
}

fn validate_bundle_password(password: &str) -> Result<(), TransferError> {
    let length = password.chars().count();
    if !(MIN_BUNDLE_PASSWORD_CHARS..=MAX_PASSWORD_CHARS).contains(&length) {
        return Err(TransferError::Invalid(format!(
            "migration password must contain {MIN_BUNDLE_PASSWORD_CHARS} to {MAX_PASSWORD_CHARS} characters"
        )));
    }
    Ok(())
}

fn validate_admin_credentials(username: &str, password: &str) -> Result<(), TransferError> {
    if !(1..=MAX_ADMIN_USERNAME_CHARS).contains(&username.trim().chars().count())
        || !(8..=MAX_PASSWORD_CHARS).contains(&password.chars().count())
    {
        return Err(TransferError::Invalid(
            "administrator credentials have an invalid length".to_string(),
        ));
    }
    Ok(())
}

fn ensure_body_bound(state: &CoreState, body: &Bytes) -> Result<(), V3ApiError> {
    if body.len() > MAX_REQUEST_BYTES {
        return Err(V3ApiError::invalid_request_at(
            state,
            "account migration request is too large",
        ));
    }
    Ok(())
}

fn ensure_bundle_bound(bundle: &str) -> Result<(), TransferError> {
    if bundle.len() > MAX_BUNDLE_BYTES {
        return Err(TransferError::Invalid(
            "account backup is too large".to_string(),
        ));
    }
    Ok(())
}

fn ensure_transport(state: &CoreState, headers: &HeaderMap) -> Result<(), V3ApiError> {
    let local =
        dashboard_session::is_local_dashboard_request(state.dashboard_local_mode(), headers);
    if !local {
        return Err(map_transfer_error(state, TransferError::InsecureTransport));
    }
    Ok(())
}

fn crypto_permit() -> Result<OwnedSemaphorePermit, TransferError> {
    Arc::clone(CRYPTO_GATE.get_or_init(|| Arc::new(Semaphore::new(1))))
        .try_acquire_owned()
        .map_err(|_| TransferError::Busy)
}

fn map_transfer_error(state: &CoreState, error: TransferError) -> V3ApiError {
    match error {
        TransferError::Invalid(message) => V3ApiError::invalid_request_at(state, message),
        TransferError::InvalidBundle => V3ApiError::invalid_request_at(
            state,
            "migration password is incorrect or the backup file is damaged",
        ),
        TransferError::AdminMissing => V3ApiError::precondition_failed_at(
            state,
            "configure an administrator account before exporting account Keys",
        ),
        TransferError::Unauthorized => V3ApiError::unauthorized_credentials(),
        TransferError::Busy => V3ApiError::service_unavailable(
            state,
            "another account migration cryptographic operation is in progress",
        ),
        TransferError::InsecureTransport => {
            V3ApiError::forbidden_at(state, "account migration is limited to the local dashboard")
        }
        TransferError::Internal => V3ApiError::internal("account migration failed"),
    }
}

fn no_store(mut response: Response) -> Response {
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

pub(super) async fn add_no_store(response: Response) -> Response {
    no_store(response)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_account(name: impl Into<String>) -> PortableAccount {
        PortableAccount {
            provider_id: "opencode".to_string(),
            offering_id: "go".to_string(),
            name: name.into(),
            username: Some("user@example.com".to_string()),
            key: "sk-ocg-test-secret".to_string(),
            enabled: true,
            account_type: "key".to_string(),
            setup_step: "ready".to_string(),
            purchase_date: "2026-08-01".to_string(),
            notes: Some("portable".to_string()),
            custom_config: None,
            model_capabilities: Vec::new(),
        }
    }

    fn sample_payload() -> PortablePayload {
        PortablePayload {
            version: PAYLOAD_VERSION,
            exported_at: "2026-08-29T00:00:00Z".to_string(),
            accounts: vec![sample_account("Primary")],
        }
    }

    #[test]
    fn encrypted_bundle_round_trips_without_plaintext_secret() {
        let payload = sample_payload();
        let bundle = encrypt_payload(&payload, "correct horse battery").unwrap();
        let second = encrypt_payload(&payload, "correct horse battery").unwrap();
        assert_ne!(
            bundle, second,
            "OS randomness must produce a fresh envelope"
        );
        assert!(!bundle.contains("sk-ocg-test-secret"));
        let (_, accounts) = decrypt_and_validate(&bundle, "correct horse battery").unwrap();
        assert_eq!(accounts.len(), 1);
        assert_eq!(accounts[0].key.as_str(), "sk-ocg-test-secret");
    }

    #[test]
    fn wrong_password_and_tampering_share_invalid_bundle_result() {
        let bundle = encrypt_payload(&sample_payload(), "correct horse battery").unwrap();
        assert!(matches!(
            decrypt_and_validate(&bundle, "wrong password value"),
            Err(TransferError::InvalidBundle)
        ));
        let mut envelope: EncryptedEnvelope = serde_json::from_str(&bundle).unwrap();
        let mut ciphertext = STANDARD.decode(&envelope.ciphertext).unwrap();
        ciphertext[0] ^= 1;
        envelope.ciphertext = STANDARD.encode(ciphertext);
        assert!(matches!(
            decrypt_and_validate(
                &serde_json::to_string(&envelope).unwrap(),
                "correct horse battery"
            ),
            Err(TransferError::InvalidBundle)
        ));

        let mut wrong_version: EncryptedEnvelope = serde_json::from_str(&bundle).unwrap();
        wrong_version.version += 1;
        assert!(matches!(
            decrypt_and_validate(
                &serde_json::to_string(&wrong_version).unwrap(),
                "correct horse battery"
            ),
            Err(TransferError::InvalidBundle)
        ));

        let mut wrong_nonce: EncryptedEnvelope = serde_json::from_str(&bundle).unwrap();
        wrong_nonce.nonce = STANDARD.encode([0_u8; NONCE_LEN - 1]);
        assert!(matches!(
            decrypt_and_validate(
                &serde_json::to_string(&wrong_nonce).unwrap(),
                "correct horse battery"
            ),
            Err(TransferError::InvalidBundle)
        ));
    }

    #[test]
    fn duplicate_rows_inside_bundle_fail_closed() {
        let mut payload = sample_payload();
        payload.accounts.push(PortableAccount {
            provider_id: "opencode".to_string(),
            offering_id: "go".to_string(),
            name: "Primary".to_string(),
            username: None,
            key: "sk-ocg-another-secret".to_string(),
            enabled: false,
            account_type: "key".to_string(),
            setup_step: "ready".to_string(),
            purchase_date: String::new(),
            notes: None,
            custom_config: None,
            model_capabilities: Vec::new(),
        });
        let bundle = encrypt_payload(&payload, "correct horse battery").unwrap();
        assert!(matches!(
            decrypt_and_validate(&bundle, "correct horse battery"),
            Err(TransferError::Invalid(_))
        ));
    }

    #[test]
    fn managed_lifecycle_is_normalized_without_browser_identity() {
        assert!(!migration_exports_key(
            ModelAccountType::Managed,
            ModelSetupStep::Payment
        ));
        assert!(migration_exports_key(
            ModelAccountType::Managed,
            ModelSetupStep::Ready
        ));
        let mut draft = sample_payload();
        draft.accounts[0].account_type = "managed".to_string();
        draft.accounts[0].setup_step = "payment".to_string();
        draft.accounts[0].enabled = true;
        let (_, draft) = validate_payload(draft).unwrap();
        assert_eq!(draft[0].setup_step, ModelSetupStep::GoogleAccount);
        assert!(!draft[0].enabled);
        assert!(draft[0].key.is_empty());

        let mut ready = sample_payload();
        ready.accounts[0].account_type = "managed".to_string();
        let (_, ready) = validate_payload(ready).unwrap();
        assert_eq!(ready[0].setup_step, ModelSetupStep::Ready);
        assert!(ready[0].enabled);
        assert_eq!(ready[0].key.as_str(), "sk-ocg-test-secret");
    }

    #[test]
    fn account_count_and_decoded_ciphertext_limits_fail_closed() {
        let mut payload = sample_payload();
        for index in 1..MAX_ACCOUNTS {
            payload
                .accounts
                .push(sample_account(format!("Account {index}")));
        }
        assert_eq!(validate_payload(payload).unwrap().1.len(), MAX_ACCOUNTS);

        let mut oversized = sample_payload();
        for index in 1..=MAX_ACCOUNTS {
            oversized
                .accounts
                .push(sample_account(format!("Account {index}")));
        }
        assert!(matches!(
            validate_payload(oversized),
            Err(TransferError::InvalidBundle)
        ));

        let envelope = EncryptedEnvelope {
            format: ENVELOPE_FORMAT.to_string(),
            version: ENVELOPE_VERSION,
            salt: STANDARD.encode([0_u8; SALT_LEN]),
            nonce: STANDARD.encode([0_u8; NONCE_LEN]),
            ciphertext: STANDARD.encode(vec![0_u8; MAX_PLAINTEXT_BYTES + 33]),
        };
        assert!(matches!(
            decrypt_and_validate(
                &serde_json::to_string(&envelope).unwrap(),
                "correct horse battery"
            ),
            Err(TransferError::InvalidBundle)
        ));
    }

    #[test]
    fn v1_encryption_vector_is_stable() {
        use sha2::{Digest, Sha256};

        let bundle = encrypt_payload_with_material(
            &sample_payload(),
            "correct horse battery",
            [7_u8; SALT_LEN],
            [9_u8; NONCE_LEN],
        )
        .unwrap();
        assert_eq!(
            format!("{:x}", Sha256::digest(bundle.as_bytes())),
            "c67afea9b4f3fdd9b66d79882ccd36bc8bf3a23c73d7d2943c120cca3a036550"
        );
    }
}
