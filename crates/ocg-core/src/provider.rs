use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;

pub const OPENCODE_PROVIDER_ID: &str = "opencode";
pub const COMMAND_CODE_PROVIDER_ID: &str = "command-code";
pub const OPENCODE_ZEN_FREE_PROVIDER_ID: &str = "opencode-zen-free";

pub const GO_OFFERING_ID: &str = "go";
pub const GOAT_OFFERING_ID: &str = "goat";
pub const ANONYMOUS_FREE_OFFERING_ID: &str = "anonymous-free";

pub const QUOTA_WINDOW_FIVE_HOURS: &str = "five_hours";
pub const QUOTA_WINDOW_WEEK: &str = "week";
pub const QUOTA_WINDOW_MONTH: &str = "month";
pub const QUOTA_WINDOW_FREE: &str = "free";

/// Reserved account row representing the egress-IP-scoped OpenCode Zen free
/// route. It is created by schema migration, never by the generic account API.
pub const ZEN_FREE_ACCOUNT_ID: &str = "00000000-0000-0000-0000-000000000002";
pub const ZEN_FREE_ACCOUNT_NAME: &str = "OpenCode Zen Free";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CredentialKind {
    ApiKey,
    None,
}

impl CredentialKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ApiKey => "api_key",
            Self::None => "none",
        }
    }
}

impl TryFrom<&str> for CredentialKind {
    type Error = ProviderBindingError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "api_key" => Ok(Self::ApiKey),
            "none" => Ok(Self::None),
            _ => Err(ProviderBindingError::UnknownCredentialKind(
                value.to_string(),
            )),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum QuotaScope {
    Key,
    EgressIp,
}

impl QuotaScope {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Key => "key",
            Self::EgressIp => "egress-ip",
        }
    }
}

impl TryFrom<&str> for QuotaScope {
    type Error = ProviderBindingError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "key" => Ok(Self::Key),
            "egress-ip" => Ok(Self::EgressIp),
            _ => Err(ProviderBindingError::UnknownQuotaScope(value.to_string())),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BuiltinOffering {
    pub provider_id: &'static str,
    pub offering_id: &'static str,
    pub credential_kind: CredentialKind,
    pub quota_scope: QuotaScope,
    pub singleton_account_id: Option<&'static str>,
}

pub const BUILTIN_OFFERINGS: [BuiltinOffering; 3] = [
    BuiltinOffering {
        provider_id: OPENCODE_PROVIDER_ID,
        offering_id: GO_OFFERING_ID,
        credential_kind: CredentialKind::ApiKey,
        quota_scope: QuotaScope::Key,
        singleton_account_id: None,
    },
    BuiltinOffering {
        provider_id: COMMAND_CODE_PROVIDER_ID,
        offering_id: GOAT_OFFERING_ID,
        credential_kind: CredentialKind::ApiKey,
        quota_scope: QuotaScope::Key,
        singleton_account_id: None,
    },
    BuiltinOffering {
        provider_id: OPENCODE_ZEN_FREE_PROVIDER_ID,
        offering_id: ANONYMOUS_FREE_OFFERING_ID,
        credential_kind: CredentialKind::None,
        quota_scope: QuotaScope::EgressIp,
        singleton_account_id: Some(ZEN_FREE_ACCOUNT_ID),
    },
];

pub fn default_provider_id() -> String {
    OPENCODE_PROVIDER_ID.to_string()
}

pub fn default_offering_id() -> String {
    GO_OFFERING_ID.to_string()
}

pub fn default_credential_kind() -> CredentialKind {
    CredentialKind::ApiKey
}

pub fn default_quota_scope() -> QuotaScope {
    QuotaScope::Key
}

pub fn builtin_offering(provider_id: &str, offering_id: &str) -> Option<BuiltinOffering> {
    BUILTIN_OFFERINGS
        .iter()
        .copied()
        .find(|offering| offering.provider_id == provider_id && offering.offering_id == offering_id)
}

pub fn validate_account_binding(
    account_id: &str,
    provider_id: &str,
    offering_id: &str,
    credential_kind: CredentialKind,
    quota_scope: QuotaScope,
) -> Result<(), ProviderBindingError> {
    let offering = builtin_offering(provider_id, offering_id).ok_or_else(|| {
        ProviderBindingError::UnknownOffering {
            provider_id: provider_id.to_string(),
            offering_id: offering_id.to_string(),
        }
    })?;
    if offering.credential_kind != credential_kind || offering.quota_scope != quota_scope {
        return Err(ProviderBindingError::BindingMismatch {
            provider_id: provider_id.to_string(),
            offering_id: offering_id.to_string(),
        });
    }
    match offering.singleton_account_id {
        Some(singleton) if account_id != singleton => {
            Err(ProviderBindingError::SingletonAccountRequired(singleton))
        }
        None if account_id == ZEN_FREE_ACCOUNT_ID => {
            Err(ProviderBindingError::ReservedAccountId(ZEN_FREE_ACCOUNT_ID))
        }
        _ => Ok(()),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderBindingError {
    UnknownOffering {
        provider_id: String,
        offering_id: String,
    },
    UnknownCredentialKind(String),
    UnknownQuotaScope(String),
    BindingMismatch {
        provider_id: String,
        offering_id: String,
    },
    SingletonAccountRequired(&'static str),
    ReservedAccountId(&'static str),
}

impl fmt::Display for ProviderBindingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownOffering {
                provider_id,
                offering_id,
            } => write!(f, "unknown provider offering `{provider_id}/{offering_id}`"),
            Self::UnknownCredentialKind(value) => {
                write!(f, "unknown credential kind `{value}`")
            }
            Self::UnknownQuotaScope(value) => write!(f, "unknown quota scope `{value}`"),
            Self::BindingMismatch {
                provider_id,
                offering_id,
            } => write!(
                f,
                "provider binding does not match `{provider_id}/{offering_id}`"
            ),
            Self::SingletonAccountRequired(id) => {
                write!(f, "provider offering requires singleton account `{id}`")
            }
            Self::ReservedAccountId(id) => write!(f, "account id `{id}` is reserved"),
        }
    }
}

impl std::error::Error for ProviderBindingError {}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QuotaWindow {
    pub account_id: String,
    pub window_kind: String,
    pub used: f64,
    pub limit_value: Option<f64>,
    pub started_at: Option<DateTime<Utc>>,
    pub resets_at: Option<DateTime<Utc>>,
    pub calibration_offset: f64,
    pub unit: String,
    pub source: String,
    pub observed_at: Option<DateTime<Utc>>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CreditBalance {
    pub account_id: String,
    pub balance_kind: String,
    pub amount: f64,
    pub unit: String,
    pub source: String,
    pub observed_at: Option<DateTime<Utc>>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderPricingSnapshot {
    pub provider_id: String,
    pub offering_id: String,
    pub revision: String,
    pub activated_at: String,
    pub document_updated_at: Option<String>,
    pub source_url: String,
    pub content_hash: String,
    pub snapshot_json: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderUsageSyncState {
    pub account_id: String,
    pub last_success_at: Option<DateTime<Utc>>,
    pub last_attempt_at: Option<DateTime<Utc>>,
    pub next_eligible_at: Option<DateTime<Utc>>,
    pub failure_streak: i64,
    pub last_expedited_at: Option<DateTime<Utc>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_pairs_derive_credential_and_quota_scope() {
        let goat = builtin_offering(COMMAND_CODE_PROVIDER_ID, GOAT_OFFERING_ID).unwrap();
        assert_eq!(goat.credential_kind, CredentialKind::ApiKey);
        assert_eq!(goat.quota_scope, QuotaScope::Key);

        let free =
            builtin_offering(OPENCODE_ZEN_FREE_PROVIDER_ID, ANONYMOUS_FREE_OFFERING_ID).unwrap();
        assert_eq!(free.credential_kind, CredentialKind::None);
        assert_eq!(free.quota_scope, QuotaScope::EgressIp);
        assert_eq!(free.singleton_account_id, Some(ZEN_FREE_ACCOUNT_ID));
    }

    #[test]
    fn singleton_and_pair_validation_is_fail_closed() {
        assert!(
            validate_account_binding(
                "account-1",
                OPENCODE_PROVIDER_ID,
                GO_OFFERING_ID,
                CredentialKind::ApiKey,
                QuotaScope::Key,
            )
            .is_ok()
        );
        assert!(
            validate_account_binding(
                "account-1",
                OPENCODE_ZEN_FREE_PROVIDER_ID,
                ANONYMOUS_FREE_OFFERING_ID,
                CredentialKind::None,
                QuotaScope::EgressIp,
            )
            .is_err()
        );
        assert!(
            validate_account_binding(
                ZEN_FREE_ACCOUNT_ID,
                OPENCODE_PROVIDER_ID,
                GO_OFFERING_ID,
                CredentialKind::ApiKey,
                QuotaScope::Key,
            )
            .is_err()
        );
    }
}
