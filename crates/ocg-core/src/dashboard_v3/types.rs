//! Shared Dashboard V3 wire types and the JSON Schema catalog.
//!
//! Response objects always serialize nullable fields as `T | null` (never omitted).
//! Request optional fields may be omitted; `expectedRevision` is required on every
//! control-plane mutation. Plaintext keys must not appear on `Settings` —
//! `ConnectionInfo` is the only secret-bearing V3 DTO.

use schemars::JsonSchema;
use schemars::generate::{SchemaGenerator, SchemaSettings};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

use crate::models::{AccountSetupStep as ModelAccountSetupStep, AccountType as ModelAccountType};
use crate::provider::{
    ConnectionVerificationStatus as ProviderVerificationStatus, CredentialKind, QuotaScope,
    UpstreamAuthScheme, UpstreamProtocolKind,
};
use crate::state::CoreState;

/// JSON Schema `$defs` names for the kernel catalog.
///
/// Later leases append new names here and register the matching DTO. Existing
/// definition objects must stay byte-identical.
pub const CATALOG_TYPE_NAMES: &[&str] = &[
    "ControlRevision",
    "MutationAck",
    "MutationExpectation",
    "PricingRevision",
    "V3Error",
    "ConnectionInfo",
    "ConnectionSubKey",
    "Settings",
    "SettingsUpdate",
    "ProxySupportedModel",
    "KeyCreate",
    "KeyUpdate",
    "Account",
    "AccountList",
    "AccountMutation",
    "AccountCustomConfig",
    "AccountModelCapability",
    "AccountAcknowledgement",
    "AccountCreate",
    "AccountManagedCreate",
    "AccountUpdate",
    "AccountOrder",
    "AccountSetupUpdate",
    "AccountCustomConfigUpdate",
    "AccountCustomConfigWrite",
    "AccountModelCapabilitiesUpdate",
    "AccountModelCapabilityWrite",
    "AccountAcknowledgementCreate",
    "AccountAcknowledgementWrite",
];

pub const ERROR_UNAUTHORIZED: &str = "unauthorized";
pub const ERROR_INVALID_JSON: &str = "invalidJson";
pub const ERROR_MISSING_EXPECTED_REVISION: &str = "missingExpectedRevision";
pub const ERROR_REVISION_CONFLICT: &str = "revisionConflict";
pub const ERROR_INVALID_REQUEST: &str = "invalidRequest";
pub const ERROR_INTERNAL: &str = "internal";
pub const ERROR_NOT_FOUND: &str = "notFound";
pub const ERROR_CONFLICT: &str = "conflict";
pub const ERROR_PRECONDITION_FAILED: &str = "preconditionFailed";
pub const ERROR_SERVICE_UNAVAILABLE: &str = "serviceUnavailable";

/// Live CAS token, process generation, and pricing snapshot id.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[schemars(rename_all = "camelCase", deny_unknown_fields)]
pub struct ControlRevision {
    pub revision: u64,
    pub process_generation: u64,
    pub pricing_revision: String,
}

impl ControlRevision {
    pub fn from_state(state: &CoreState) -> Self {
        Self {
            revision: state.settings_revision(),
            process_generation: state.process_generation(),
            pricing_revision: state.pricing_snapshot().revision.clone(),
        }
    }
}

/// Required process-scoped mutation precondition.
///
/// Both fields travel at the top level of every mutation request. The random
/// process generation prevents a revision captured before restart from being
/// accepted by a fresh process whose in-memory counter reused the same value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[schemars(rename_all = "camelCase", deny_unknown_fields)]
pub struct MutationExpectation {
    pub expected_revision: u64,
    pub process_generation: u64,
}

/// Successful control-plane mutation acknowledgement.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[schemars(rename_all = "camelCase", deny_unknown_fields)]
pub struct MutationAck {
    pub revision: u64,
    pub process_generation: u64,
}

/// Pricing snapshot identity. Distinct from the u64 settings CAS token.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[schemars(rename_all = "camelCase", deny_unknown_fields)]
pub struct PricingRevision {
    pub pricing_revision: String,
}

/// Stable non-2xx JSON envelope for every Dashboard V3 error.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[schemars(rename_all = "camelCase", deny_unknown_fields)]
pub struct V3Error {
    pub code: String,
    pub message: String,
    pub current_revision: Option<u64>,
    pub process_generation: Option<u64>,
}

impl V3Error {
    pub fn unauthorized() -> Self {
        Self {
            code: ERROR_UNAUTHORIZED.to_string(),
            message: "dashboard session is required".to_string(),
            current_revision: None,
            process_generation: None,
        }
    }

    pub fn invalid_json() -> Self {
        Self {
            code: ERROR_INVALID_JSON.to_string(),
            message: "request body must be valid JSON".to_string(),
            current_revision: None,
            process_generation: None,
        }
    }

    pub fn missing_expected_revision() -> Self {
        Self {
            code: ERROR_MISSING_EXPECTED_REVISION.to_string(),
            message: "expectedRevision is required".to_string(),
            current_revision: None,
            process_generation: None,
        }
    }

    pub fn revision_conflict(current_revision: u64, process_generation: u64) -> Self {
        Self {
            code: ERROR_REVISION_CONFLICT.to_string(),
            message: "settings changed since they were loaded; reload and try again".to_string(),
            current_revision: Some(current_revision),
            process_generation: Some(process_generation),
        }
    }

    pub fn invalid_request(message: impl Into<String>) -> Self {
        Self {
            code: ERROR_INVALID_REQUEST.to_string(),
            message: message.into(),
            current_revision: None,
            process_generation: None,
        }
    }

    pub fn invalid_request_at(
        message: impl Into<String>,
        current_revision: u64,
        process_generation: u64,
    ) -> Self {
        Self {
            code: ERROR_INVALID_REQUEST.to_string(),
            message: message.into(),
            current_revision: Some(current_revision),
            process_generation: Some(process_generation),
        }
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self {
            code: ERROR_INTERNAL.to_string(),
            message: message.into(),
            current_revision: None,
            process_generation: None,
        }
    }

    pub fn not_found(
        message: impl Into<String>,
        current_revision: u64,
        process_generation: u64,
    ) -> Self {
        Self {
            code: ERROR_NOT_FOUND.to_string(),
            message: message.into(),
            current_revision: Some(current_revision),
            process_generation: Some(process_generation),
        }
    }

    pub fn conflict(
        message: impl Into<String>,
        current_revision: u64,
        process_generation: u64,
    ) -> Self {
        Self {
            code: ERROR_CONFLICT.to_string(),
            message: message.into(),
            current_revision: Some(current_revision),
            process_generation: Some(process_generation),
        }
    }

    pub fn precondition_failed(
        message: impl Into<String>,
        current_revision: u64,
        process_generation: u64,
    ) -> Self {
        Self {
            code: ERROR_PRECONDITION_FAILED.to_string(),
            message: message.into(),
            current_revision: Some(current_revision),
            process_generation: Some(process_generation),
        }
    }

    pub fn service_unavailable(
        message: impl Into<String>,
        current_revision: u64,
        process_generation: u64,
    ) -> Self {
        Self {
            code: ERROR_SERVICE_UNAVAILABLE.to_string(),
            message: message.into(),
            current_revision: Some(current_revision),
            process_generation: Some(process_generation),
        }
    }
}

/// Lightweight connection-center payload. The only V3 DTO allowed to carry
/// plaintext primary and sub Key values.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[schemars(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConnectionInfo {
    pub gateway_port: u16,
    pub client_root_url: String,
    pub upstream_base_url: String,
    pub primary_key: String,
    pub sub_keys: Vec<ConnectionSubKey>,
    pub revision: u64,
    pub process_generation: u64,
}

/// One non-deleted sub Key as exposed by [`ConnectionInfo`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[schemars(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConnectionSubKey {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    pub value: String,
}

/// Application settings contract. Never contains primary/sub Key plaintext
/// or a field named `gatewayKey` / `key`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[schemars(rename_all = "camelCase", deny_unknown_fields)]
pub struct Settings {
    pub revision: u64,
    pub process_generation: u64,
    pub gateway_port: u16,
    pub upstream_base_url: String,
    pub proxy_mode: ProxyMode,
    pub proxy_url: String,
    pub proxy_list_direction: ProxyListDirection,
    pub proxy_list_models: Vec<String>,
    pub proxy_supported_models: Vec<ProxySupportedModel>,
    pub opencode_invite_url: String,
    pub client_root_url: String,
    pub client_root_url_from_env: bool,
    pub auto_start: Option<bool>,
    pub auto_start_supported: bool,
    pub show_dock_icon: Option<bool>,
    pub dock_visibility_supported: bool,
    pub connect_timeout_secs: u64,
    pub non_stream_timeout_secs: u64,
    pub stream_idle_timeout_secs: u64,
    pub routing_mode: RoutingMode,
    pub conversation_sticky: bool,
}

/// PATCH-style settings write. `expectedRevision` and `processGeneration`
/// are required; every other field may be omitted. Unknown fields, including
/// any Key material, are rejected.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[schemars(rename_all = "camelCase", deny_unknown_fields)]
pub struct SettingsUpdate {
    #[serde(flatten)]
    pub expectation: MutationExpectation,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gateway_port: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub upstream_base_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proxy_mode: Option<ProxyMode>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proxy_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proxy_list_direction: Option<ProxyListDirection>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proxy_list_models: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub opencode_invite_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_root_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auto_start: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub show_dock_icon: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connect_timeout_secs: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub non_stream_timeout_secs: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stream_idle_timeout_secs: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub routing_mode: Option<RoutingMode>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub conversation_sticky: Option<bool>,
}

/// POST `/keys` body. CAS tokens are required; `name` is required. Unknown
/// fields, including any Key material, are rejected.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[schemars(rename_all = "camelCase", deny_unknown_fields)]
pub struct KeyCreate {
    #[serde(flatten)]
    pub expectation: MutationExpectation,
    pub name: String,
}

/// PATCH `/keys/{id}` body. CAS tokens are required; `name` and `enabled`
/// may be omitted. Unknown fields, including any Key material, are rejected.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[schemars(rename_all = "camelCase", deny_unknown_fields)]
pub struct KeyUpdate {
    #[serde(flatten)]
    pub expectation: MutationExpectation,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
}

/// One known model backing the list-mode checkbox grid.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[schemars(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProxySupportedModel {
    pub id: String,
    pub preferred_protocol: String,
    pub zen_free: bool,
}

/// Global outbound proxy mode. Wire values stay kebab-case, matching V2.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
#[schemars(rename_all = "kebab-case")]
pub enum ProxyMode {
    Auto,
    Manual,
    Direct,
    List,
}

/// Which listed models take the list-mode exception leg.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
#[schemars(rename_all = "kebab-case")]
pub enum ProxyListDirection {
    Whitelist,
    Blacklist,
}

/// Account selection mode. Wire values stay kebab-case, matching V2.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
#[schemars(rename_all = "kebab-case")]
pub enum RoutingMode {
    StrictPriority,
    StickyGlobal,
    RoundRobin,
}

/// Secret-free account resource. Distinct from `models::Account`.
///
/// Responses emit `T | null` for every optional field. Plaintext upstream
/// keys, passwords, ciphers, gateway Keys, and referral codes never appear.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[schemars(rename_all = "camelCase", deny_unknown_fields)]
pub struct Account {
    pub id: String,
    pub provider_id: String,
    pub offering_id: String,
    pub credential_kind: AccountCredentialKind,
    pub quota_scope: AccountQuotaScope,
    pub name: String,
    pub username: Option<String>,
    pub enabled: bool,
    pub account_type: AccountType,
    pub setup_step: AccountSetupStep,
    pub purchase_date: String,
    pub expires_on: String,
    pub cooldown_until: Option<String>,
    pub cooldown_generic_until: Option<String>,
    pub cooldown_5h_until: Option<String>,
    pub cooldown_week_until: Option<String>,
    pub cooldown_month_until: Option<String>,
    pub cooldown_free_until: Option<String>,
    pub last_error: Option<String>,
    pub auth_error: Option<String>,
    pub notes: Option<String>,
    pub usage_sync_last_success_at: Option<String>,
    pub usage_sync_next_allowed_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub revision: u64,
    pub process_generation: u64,
    pub verification_status: AccountVerificationStatus,
    pub connection_verified_at: Option<String>,
    pub verification_error: Option<String>,
    pub plan_routable: bool,
    pub custom_config: Option<AccountCustomConfig>,
    pub model_capabilities: Vec<AccountModelCapability>,
    pub acknowledgements: Vec<AccountAcknowledgement>,
}

/// GET `/accounts` and PUT `/accounts/order` envelope.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[schemars(rename_all = "camelCase", deny_unknown_fields)]
pub struct AccountList {
    pub accounts: Vec<Account>,
    pub revision: u64,
    pub process_generation: u64,
}

/// Successful single-account mutation. `account` is `null` after delete.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[schemars(rename_all = "camelCase", deny_unknown_fields)]
pub struct AccountMutation {
    pub account: Option<Account>,
    pub revision: u64,
    pub process_generation: u64,
}

/// Nested Custom HTTP destination as returned on an account.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[schemars(rename_all = "camelCase", deny_unknown_fields)]
pub struct AccountCustomConfig {
    pub account_id: String,
    pub base_url: String,
    pub upstream_protocol: AccountUpstreamProtocol,
    pub auth_scheme: AccountAuthScheme,
    pub created_at: String,
    pub updated_at: String,
}

/// One declared Custom model capability as returned on an account.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[schemars(rename_all = "camelCase", deny_unknown_fields)]
pub struct AccountModelCapability {
    pub account_id: String,
    pub model_id: String,
    pub protocol: AccountUpstreamProtocol,
    pub verified_at: Option<String>,
    pub source: String,
}

/// One persisted Plan risk acknowledgement as returned on an account.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[schemars(rename_all = "camelCase", deny_unknown_fields)]
pub struct AccountAcknowledgement {
    pub account_id: String,
    pub acknowledgement_id: String,
    pub version: String,
    pub content_hash: String,
    pub accepted_at: String,
}

/// POST `/accounts` body. CAS tokens and `name` are required. `key`,
/// `password`, and `referralCode` are write-only and never echoed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[schemars(rename_all = "camelCase", deny_unknown_fields)]
pub struct AccountCreate {
    #[serde(flatten)]
    pub expectation: MutationExpectation,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub offering_id: Option<String>,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,
    pub key: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub referral_code: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub purchase_date: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub custom_config: Option<AccountCustomConfigWrite>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub acknowledgements: Vec<AccountAcknowledgementWrite>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub model_capabilities: Vec<AccountModelCapabilityWrite>,
}

/// POST `/accounts/managed` body. CAS tokens and `name` are required.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[schemars(rename_all = "camelCase", deny_unknown_fields)]
pub struct AccountManagedCreate {
    #[serde(flatten)]
    pub expectation: MutationExpectation,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
}

/// PATCH `/accounts/{id}` body. CAS tokens are required; other fields may be
/// omitted. Write-only `key` / `password` / `referralCode` are accepted and
/// never echoed. Unknown fields, including provider binding, are rejected.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[schemars(rename_all = "camelCase", deny_unknown_fields)]
pub struct AccountUpdate {
    #[serde(flatten)]
    pub expectation: MutationExpectation,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub referral_code: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub purchase_date: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
}

/// PUT `/accounts/order` body. CAS tokens and the complete id set are required.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[schemars(rename_all = "camelCase", deny_unknown_fields)]
pub struct AccountOrder {
    #[serde(flatten)]
    pub expectation: MutationExpectation,
    pub account_ids: Vec<String>,
}

/// PATCH `/accounts/{id}/setup` body. CAS tokens and `setupStep` are required.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[schemars(rename_all = "camelCase", deny_unknown_fields)]
pub struct AccountSetupUpdate {
    #[serde(flatten)]
    pub expectation: MutationExpectation,
    pub setup_step: AccountSetupStep,
}

/// PUT `/accounts/{id}/custom-config` body. Protocol and auth scheme are
/// immutable after create; the handler enforces that.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[schemars(rename_all = "camelCase", deny_unknown_fields)]
pub struct AccountCustomConfigUpdate {
    #[serde(flatten)]
    pub expectation: MutationExpectation,
    pub base_url: String,
    pub upstream_protocol: AccountUpstreamProtocol,
    pub auth_scheme: AccountAuthScheme,
}

/// Create-time Custom destination (no timestamps). Nested under `AccountCreate`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[schemars(rename_all = "camelCase", deny_unknown_fields)]
pub struct AccountCustomConfigWrite {
    pub base_url: String,
    pub upstream_protocol: AccountUpstreamProtocol,
    pub auth_scheme: AccountAuthScheme,
}

/// PUT `/accounts/{id}/model-capabilities` body.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[schemars(rename_all = "camelCase", deny_unknown_fields)]
pub struct AccountModelCapabilitiesUpdate {
    #[serde(flatten)]
    pub expectation: MutationExpectation,
    pub capabilities: Vec<AccountModelCapabilityWrite>,
}

/// One declared Custom model capability on create or replace.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[schemars(rename_all = "camelCase", deny_unknown_fields)]
pub struct AccountModelCapabilityWrite {
    pub model_id: String,
    pub protocol: AccountUpstreamProtocol,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
}

/// POST `/accounts/{id}/acknowledgements` body.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[schemars(rename_all = "camelCase", deny_unknown_fields)]
pub struct AccountAcknowledgementCreate {
    #[serde(flatten)]
    pub expectation: MutationExpectation,
    pub acknowledgement_id: String,
    pub version: String,
}

/// Create-time Plan risk acknowledgement. Nested under `AccountCreate`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[schemars(rename_all = "camelCase", deny_unknown_fields)]
pub struct AccountAcknowledgementWrite {
    pub acknowledgement_id: String,
    pub version: String,
}

/// Wire identity matching V2 `api_key` / `none`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
#[schemars(rename_all = "snake_case")]
pub enum AccountCredentialKind {
    ApiKey,
    None,
}

impl From<CredentialKind> for AccountCredentialKind {
    fn from(value: CredentialKind) -> Self {
        match value {
            CredentialKind::ApiKey => Self::ApiKey,
            CredentialKind::None => Self::None,
        }
    }
}

/// Wire identity matching V2 `key` / `egress-ip`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
#[schemars(rename_all = "kebab-case")]
pub enum AccountQuotaScope {
    Key,
    EgressIp,
}

impl From<QuotaScope> for AccountQuotaScope {
    fn from(value: QuotaScope) -> Self {
        match value {
            QuotaScope::Key => Self::Key,
            QuotaScope::EgressIp => Self::EgressIp,
        }
    }
}

/// Wire identity matching V2 `key` / `managed`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
#[schemars(rename_all = "snake_case")]
pub enum AccountType {
    Key,
    Managed,
}

impl From<ModelAccountType> for AccountType {
    fn from(value: ModelAccountType) -> Self {
        match value {
            ModelAccountType::Key => Self::Key,
            ModelAccountType::Managed => Self::Managed,
        }
    }
}

impl From<AccountType> for ModelAccountType {
    fn from(value: AccountType) -> Self {
        match value {
            AccountType::Key => Self::Key,
            AccountType::Managed => Self::Managed,
        }
    }
}

/// Managed-setup wizard step. Wire values match V2 snake_case.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
#[schemars(rename_all = "snake_case")]
pub enum AccountSetupStep {
    GoogleAccount,
    OpencodeRegistration,
    Payment,
    KeyVerification,
    Ready,
}

impl From<ModelAccountSetupStep> for AccountSetupStep {
    fn from(value: ModelAccountSetupStep) -> Self {
        match value {
            ModelAccountSetupStep::GoogleAccount => Self::GoogleAccount,
            ModelAccountSetupStep::OpencodeRegistration => Self::OpencodeRegistration,
            ModelAccountSetupStep::Payment => Self::Payment,
            ModelAccountSetupStep::KeyVerification => Self::KeyVerification,
            ModelAccountSetupStep::Ready => Self::Ready,
        }
    }
}

impl From<AccountSetupStep> for ModelAccountSetupStep {
    fn from(value: AccountSetupStep) -> Self {
        match value {
            AccountSetupStep::GoogleAccount => Self::GoogleAccount,
            AccountSetupStep::OpencodeRegistration => Self::OpencodeRegistration,
            AccountSetupStep::Payment => Self::Payment,
            AccountSetupStep::KeyVerification => Self::KeyVerification,
            AccountSetupStep::Ready => Self::Ready,
        }
    }
}

/// Connection-verification status. Wire values match V2 snake_case.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
#[schemars(rename_all = "snake_case")]
pub enum AccountVerificationStatus {
    NotRequired,
    Pending,
    Verified,
    Failed,
}

impl From<ProviderVerificationStatus> for AccountVerificationStatus {
    fn from(value: ProviderVerificationStatus) -> Self {
        match value {
            ProviderVerificationStatus::NotRequired => Self::NotRequired,
            ProviderVerificationStatus::Pending => Self::Pending,
            ProviderVerificationStatus::Verified => Self::Verified,
            ProviderVerificationStatus::Failed => Self::Failed,
        }
    }
}

/// Custom/upstream protocol. Wire values match V2 snake_case.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
#[schemars(rename_all = "snake_case")]
pub enum AccountUpstreamProtocol {
    ChatCompletions,
    Responses,
    Messages,
}

impl From<UpstreamProtocolKind> for AccountUpstreamProtocol {
    fn from(value: UpstreamProtocolKind) -> Self {
        match value {
            UpstreamProtocolKind::ChatCompletions => Self::ChatCompletions,
            UpstreamProtocolKind::Responses => Self::Responses,
            UpstreamProtocolKind::Messages => Self::Messages,
        }
    }
}

impl From<AccountUpstreamProtocol> for UpstreamProtocolKind {
    fn from(value: AccountUpstreamProtocol) -> Self {
        match value {
            AccountUpstreamProtocol::ChatCompletions => Self::ChatCompletions,
            AccountUpstreamProtocol::Responses => Self::Responses,
            AccountUpstreamProtocol::Messages => Self::Messages,
        }
    }
}

/// Custom auth scheme. Wire values match V2 kebab-case.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
#[schemars(rename_all = "kebab-case")]
pub enum AccountAuthScheme {
    Bearer,
    XApiKey,
}

impl From<UpstreamAuthScheme> for AccountAuthScheme {
    fn from(value: UpstreamAuthScheme) -> Self {
        match value {
            UpstreamAuthScheme::Bearer => Self::Bearer,
            UpstreamAuthScheme::XApiKey => Self::XApiKey,
        }
    }
}

impl From<AccountAuthScheme> for UpstreamAuthScheme {
    fn from(value: AccountAuthScheme) -> Self {
        match value {
            AccountAuthScheme::Bearer => Self::Bearer,
            AccountAuthScheme::XApiKey => Self::XApiKey,
        }
    }
}

/// Deterministic JSON Schema catalog for the checked-in V3 contract.
///
/// Response types are generated with the serialize contract so `Option` fields
/// stay required `T | null`. Request types use the deserialize contract so
/// optional fields may be omitted. Adding a DTO later must append a `$defs`
/// entry without renaming existing definitions.
pub fn contract_schema() -> Value {
    let mut serialize = SchemaSettings::draft2020_12()
        .for_serialize()
        .into_generator();
    include_type::<ControlRevision>(&mut serialize);
    include_type::<MutationAck>(&mut serialize);
    include_type::<PricingRevision>(&mut serialize);
    include_type::<V3Error>(&mut serialize);
    include_type::<ConnectionInfo>(&mut serialize);
    include_type::<ConnectionSubKey>(&mut serialize);
    include_type::<Settings>(&mut serialize);
    include_type::<ProxySupportedModel>(&mut serialize);
    include_type::<Account>(&mut serialize);
    include_type::<AccountList>(&mut serialize);
    include_type::<AccountMutation>(&mut serialize);
    include_type::<AccountCustomConfig>(&mut serialize);
    include_type::<AccountModelCapability>(&mut serialize);
    include_type::<AccountAcknowledgement>(&mut serialize);
    let mut defs = serialize.take_definitions(true);

    let mut deserialize = SchemaSettings::draft2020_12().into_generator();
    include_type::<MutationExpectation>(&mut deserialize);
    include_type::<SettingsUpdate>(&mut deserialize);
    include_type::<KeyCreate>(&mut deserialize);
    include_type::<KeyUpdate>(&mut deserialize);
    include_type::<AccountCreate>(&mut deserialize);
    include_type::<AccountManagedCreate>(&mut deserialize);
    include_type::<AccountUpdate>(&mut deserialize);
    include_type::<AccountOrder>(&mut deserialize);
    include_type::<AccountSetupUpdate>(&mut deserialize);
    include_type::<AccountCustomConfigUpdate>(&mut deserialize);
    include_type::<AccountCustomConfigWrite>(&mut deserialize);
    include_type::<AccountModelCapabilitiesUpdate>(&mut deserialize);
    include_type::<AccountModelCapabilityWrite>(&mut deserialize);
    include_type::<AccountAcknowledgementCreate>(&mut deserialize);
    include_type::<AccountAcknowledgementWrite>(&mut deserialize);
    for (name, schema) in deserialize.take_definitions(true) {
        defs.entry(name).or_insert(schema);
    }

    for name in CATALOG_TYPE_NAMES {
        if !defs.contains_key(*name) {
            panic!("dashboard v3 schema catalog is missing $defs/{name}");
        }
    }

    json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "title": "DashboardApiV3",
        "$comment": "Extensible Dashboard V3 contract catalog. Add new $defs for later DTOs; do not rename or reshape existing definitions. ConnectionInfo is the only plaintext Key DTO.",
        "anyOf": catalog_refs(&defs),
        "$defs": defs,
    })
}

/// Pretty-printed catalog JSON with a trailing newline.
pub fn contract_schema_pretty() -> String {
    let mut encoded = serde_json::to_string_pretty(&contract_schema())
        .expect("dashboard v3 schema should serialize");
    if !encoded.ends_with('\n') {
        encoded.push('\n');
    }
    encoded
}

fn include_type<T: JsonSchema>(generator: &mut SchemaGenerator) {
    generator.subschema_for::<T>();
}

fn catalog_refs(defs: &Map<String, Value>) -> Vec<Value> {
    CATALOG_TYPE_NAMES
        .iter()
        .filter(|name| defs.contains_key(**name))
        .map(|name| json!({ "$ref": format!("#/$defs/{name}") }))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn wire_fields_are_camel_case() {
        let revision = ControlRevision {
            revision: 7,
            process_generation: 9,
            pricing_revision: "seed".into(),
        };
        assert_eq!(
            serde_json::to_value(&revision).unwrap(),
            json!({
                "revision": 7,
                "processGeneration": 9,
                "pricingRevision": "seed",
            })
        );

        let parsed: MutationExpectation = serde_json::from_value(json!({
            "expectedRevision": 3,
            "processGeneration": 9,
        }))
        .unwrap();
        assert_eq!(parsed.expected_revision, 3);
        assert_eq!(parsed.process_generation, 9);
        assert!(
            serde_json::from_value::<MutationExpectation>(json!({ "expected_revision": 3 }))
                .is_err()
        );
        assert!(
            serde_json::from_value::<MutationExpectation>(json!({
                "expectedRevision": 3,
                "processGeneration": 7,
                "value": "must-not-be-accepted"
            }))
            .is_err()
        );
    }

    #[test]
    fn error_envelope_always_emits_nullable_fields() {
        let error = V3Error::missing_expected_revision();
        let value = serde_json::to_value(&error).unwrap();
        assert_eq!(value["code"], "missingExpectedRevision");
        assert_eq!(value["currentRevision"], Value::Null);
        assert_eq!(value["processGeneration"], Value::Null);
        assert!(!value.as_object().unwrap().contains_key("current_revision"));
    }

    #[test]
    fn schema_catalog_is_extensible_and_names_kernel_types() {
        let schema = contract_schema();
        let defs = schema["$defs"].as_object().expect("catalog $defs");
        for name in CATALOG_TYPE_NAMES {
            assert!(defs.contains_key(*name), "missing {name}");
        }
        let required_error = defs["V3Error"]["required"]
            .as_array()
            .expect("V3Error.required");
        for field in ["code", "message", "currentRevision", "processGeneration"] {
            assert!(
                required_error.iter().any(|value| value == field),
                "{field} must stay required so responses emit T|null"
            );
        }
        let expectation_required = defs["MutationExpectation"]["required"]
            .as_array()
            .expect("MutationExpectation.required");
        assert_eq!(
            expectation_required,
            &vec![json!("expectedRevision"), json!("processGeneration")]
        );
        assert_eq!(schema["title"], "DashboardApiV3");
    }

    #[test]
    fn connection_info_is_the_only_secret_bearing_dto() {
        let connection = ConnectionInfo {
            gateway_port: 9042,
            client_root_url: String::new(),
            upstream_base_url: "https://opencode.ai/zen/go".into(),
            primary_key: "ocg-secret".into(),
            sub_keys: vec![ConnectionSubKey {
                id: "sub".into(),
                name: "Laptop".into(),
                enabled: true,
                value: "ocg-sub-secret".into(),
            }],
            revision: 3,
            process_generation: 9,
        };
        let value = serde_json::to_value(&connection).unwrap();
        assert_eq!(value["primaryKey"], "ocg-secret");
        assert_eq!(value["subKeys"][0]["value"], "ocg-sub-secret");
        assert!(value.get("gatewayKey").is_none());
        assert!(value.get("key").is_none());
        assert!(value.get("gateway_key").is_none());
        assert_eq!(value["processGeneration"], 9);
    }

    #[test]
    fn settings_wire_omits_key_fields_and_nulls_unsupported_host_toggles() {
        let settings = Settings {
            revision: 4,
            process_generation: 9,
            gateway_port: 9042,
            upstream_base_url: "https://opencode.ai/zen/go".into(),
            proxy_mode: ProxyMode::Auto,
            proxy_url: String::new(),
            proxy_list_direction: ProxyListDirection::Whitelist,
            proxy_list_models: Vec::new(),
            proxy_supported_models: vec![ProxySupportedModel {
                id: "gpt-5.6-luna".into(),
                preferred_protocol: "responses".into(),
                zen_free: false,
            }],
            opencode_invite_url: String::new(),
            client_root_url: String::new(),
            client_root_url_from_env: false,
            auto_start: None,
            auto_start_supported: false,
            show_dock_icon: None,
            dock_visibility_supported: false,
            connect_timeout_secs: 30,
            non_stream_timeout_secs: 900,
            stream_idle_timeout_secs: 300,
            routing_mode: RoutingMode::StrictPriority,
            conversation_sticky: false,
        };
        let value = serde_json::to_value(&settings).unwrap();
        let object = value.as_object().unwrap();
        for forbidden in [
            "key",
            "gatewayKey",
            "gateway_key",
            "primaryKey",
            "primary_key",
        ] {
            assert!(
                !object.contains_key(forbidden),
                "settings must not expose {forbidden}"
            );
        }
        assert_eq!(value["autoStart"], Value::Null);
        assert_eq!(value["showDockIcon"], Value::Null);
        assert_eq!(value["autoStartSupported"], false);
        assert_eq!(value["proxyMode"], "auto");
        assert_eq!(value["routingMode"], "strict-priority");
        assert_eq!(
            value["proxySupportedModels"][0]["preferredProtocol"],
            "responses"
        );
    }

    #[test]
    fn settings_update_requires_cas_and_allows_omitted_patch_fields() {
        let parsed: SettingsUpdate = serde_json::from_value(json!({
            "expectedRevision": 7,
            "processGeneration": 9,
            "connectTimeoutSecs": 12
        }))
        .unwrap();
        assert_eq!(parsed.expectation.expected_revision, 7);
        assert_eq!(parsed.expectation.process_generation, 9);
        assert_eq!(parsed.connect_timeout_secs, Some(12));
        assert!(parsed.proxy_mode.is_none());
        assert!(
            serde_json::from_value::<SettingsUpdate>(json!({
                "expectedRevision": 7,
                "processGeneration": 9,
                "gatewayKey": "ocg-secret"
            }))
            .is_err()
        );
        assert!(
            serde_json::from_value::<SettingsUpdate>(json!({
                "expected_revision": 7,
                "processGeneration": 9
            }))
            .is_err()
        );
    }

    #[test]
    fn key_mutation_dtos_require_cas_and_reject_secret_fields() {
        let created: KeyCreate = serde_json::from_value(json!({
            "expectedRevision": 4,
            "processGeneration": 9,
            "name": "Laptop"
        }))
        .unwrap();
        assert_eq!(created.expectation.expected_revision, 4);
        assert_eq!(created.expectation.process_generation, 9);
        assert_eq!(created.name, "Laptop");
        assert!(
            serde_json::from_value::<KeyCreate>(json!({
                "expectedRevision": 4,
                "processGeneration": 9,
                "name": "Laptop",
                "value": "ocg-secret"
            }))
            .is_err()
        );
        assert!(
            serde_json::from_value::<KeyCreate>(json!({
                "expectedRevision": 4,
                "processGeneration": 9,
                "name": "Laptop",
                "key": "ocg-secret"
            }))
            .is_err()
        );
        assert!(
            serde_json::from_value::<KeyCreate>(json!({
                "expectedRevision": 4,
                "processGeneration": 9,
                "name": "Laptop",
                "gatewayKey": "ocg-secret"
            }))
            .is_err()
        );
        assert!(
            serde_json::from_value::<KeyCreate>(json!({
                "expectedRevision": 4,
                "processGeneration": 9,
                "name": "Laptop",
                "primaryKey": "ocg-secret"
            }))
            .is_err()
        );

        let patched: KeyUpdate = serde_json::from_value(json!({
            "expectedRevision": 5,
            "processGeneration": 9,
            "enabled": false
        }))
        .unwrap();
        assert_eq!(patched.expectation.expected_revision, 5);
        assert_eq!(patched.enabled, Some(false));
        assert!(patched.name.is_none());
        assert!(
            serde_json::from_value::<KeyUpdate>(json!({
                "expectedRevision": 5,
                "processGeneration": 9,
                "value": "ocg-secret"
            }))
            .is_err()
        );
        assert!(
            serde_json::from_value::<KeyUpdate>(json!({
                "expected_revision": 5,
                "processGeneration": 9
            }))
            .is_err()
        );
    }

    #[test]
    fn mutation_ack_serializes_without_credential_fields() {
        let ack = MutationAck {
            revision: 8,
            process_generation: 9,
        };
        let value = serde_json::to_value(&ack).unwrap();
        let object = value.as_object().unwrap();
        assert_eq!(object.get("revision"), Some(&json!(8)));
        assert_eq!(object.get("processGeneration"), Some(&json!(9)));
        for forbidden in [
            "key",
            "gatewayKey",
            "gateway_key",
            "primaryKey",
            "primary_key",
            "value",
            "name",
            "id",
        ] {
            assert!(
                !object.contains_key(forbidden),
                "MutationAck must not expose {forbidden}"
            );
        }
    }

    #[test]
    fn account_response_emits_nulls_and_never_carries_secrets() {
        let account = Account {
            id: "acct-1".into(),
            provider_id: "opencode".into(),
            offering_id: "go".into(),
            credential_kind: AccountCredentialKind::ApiKey,
            quota_scope: AccountQuotaScope::Key,
            name: "main".into(),
            username: None,
            enabled: true,
            account_type: AccountType::Key,
            setup_step: AccountSetupStep::Ready,
            purchase_date: "2026-01-31".into(),
            expires_on: "2026-02-28".into(),
            cooldown_until: None,
            cooldown_generic_until: None,
            cooldown_5h_until: None,
            cooldown_week_until: None,
            cooldown_month_until: None,
            cooldown_free_until: None,
            last_error: None,
            auth_error: None,
            notes: None,
            usage_sync_last_success_at: None,
            usage_sync_next_allowed_at: None,
            created_at: "2026-01-31T00:00:00Z".into(),
            updated_at: "2026-01-31T00:00:00Z".into(),
            revision: 4,
            process_generation: 9,
            verification_status: AccountVerificationStatus::NotRequired,
            connection_verified_at: None,
            verification_error: None,
            plan_routable: true,
            custom_config: None,
            model_capabilities: Vec::new(),
            acknowledgements: Vec::new(),
        };
        let value = serde_json::to_value(&account).unwrap();
        let object = value.as_object().unwrap();
        for forbidden in [
            "key",
            "password",
            "passwordCipher",
            "keyCipher",
            "gatewayKey",
            "gateway_key",
            "primaryKey",
            "referralCode",
            "referral_code",
        ] {
            assert!(
                !object.contains_key(forbidden),
                "Account must not expose {forbidden}"
            );
        }
        assert_eq!(value["username"], Value::Null);
        assert_eq!(value["notes"], Value::Null);
        assert_eq!(value["customConfig"], Value::Null);
        assert_eq!(value["cooldown5hUntil"], Value::Null);
        assert_eq!(value["verificationStatus"], "not_required");
        assert_eq!(value["quotaScope"], "key");
        assert_eq!(value["processGeneration"], 9);

        let listed = AccountList {
            accounts: vec![account.clone()],
            revision: 4,
            process_generation: 9,
        };
        let listed_value = serde_json::to_value(&listed).unwrap();
        assert_eq!(listed_value["accounts"][0]["id"], "acct-1");
        assert_eq!(listed_value["revision"], 4);

        let deleted = AccountMutation {
            account: None,
            revision: 5,
            process_generation: 9,
        };
        let deleted_value = serde_json::to_value(&deleted).unwrap();
        assert_eq!(deleted_value["account"], Value::Null);
        assert_eq!(deleted_value["revision"], 5);
    }

    #[test]
    fn account_requests_accept_write_only_secrets_and_reject_unknown_fields() {
        let created: AccountCreate = serde_json::from_value(json!({
            "expectedRevision": 3,
            "processGeneration": 9,
            "name": "Go",
            "key": "sk-secret",
            "password": "pw-secret",
            "referralCode": "ref-1"
        }))
        .unwrap();
        assert_eq!(created.expectation.expected_revision, 3);
        assert_eq!(created.key, "sk-secret");
        assert_eq!(created.password.as_deref(), Some("pw-secret"));
        assert_eq!(created.referral_code.as_deref(), Some("ref-1"));
        assert!(created.provider_id.is_none());
        assert!(created.custom_config.is_none());
        assert!(
            serde_json::from_value::<AccountCreate>(json!({
                "expectedRevision": 3,
                "processGeneration": 9,
                "name": "Go",
                "key": "sk-secret",
                "gatewayKey": "ocg-secret"
            }))
            .is_err()
        );
        assert!(
            serde_json::from_value::<AccountUpdate>(json!({
                "expectedRevision": 3,
                "processGeneration": 9,
                "providerId": "opencode"
            }))
            .is_err()
        );
        assert!(
            serde_json::from_value::<AccountManagedCreate>(json!({
                "expectedRevision": 3,
                "processGeneration": 9,
                "name": "draft",
                "key": "sk-secret"
            }))
            .is_err()
        );

        let patched: AccountUpdate = serde_json::from_value(json!({
            "expectedRevision": 4,
            "processGeneration": 9,
            "enabled": false
        }))
        .unwrap();
        assert_eq!(patched.enabled, Some(false));
        assert!(patched.key.is_none());
        assert!(patched.name.is_none());
    }

    #[test]
    fn service_unavailable_error_emits_stable_code_and_cas_tokens() {
        let error = V3Error::service_unavailable("browser stop failed", 11, 9);
        let value = serde_json::to_value(&error).unwrap();
        assert_eq!(value["code"], ERROR_SERVICE_UNAVAILABLE);
        assert_eq!(value["code"], "serviceUnavailable");
        assert_eq!(value["message"], "browser stop failed");
        assert_eq!(value["currentRevision"], 11);
        assert_eq!(value["processGeneration"], 9);

        let schema = contract_schema();
        let defs = schema["$defs"].as_object().expect("catalog $defs");
        assert!(defs.contains_key("V3Error"));
        assert_eq!(
            defs["V3Error"]["properties"]["code"]["type"], "string",
            "new error codes must not reshape the V3Error catalog definition"
        );
    }
}
