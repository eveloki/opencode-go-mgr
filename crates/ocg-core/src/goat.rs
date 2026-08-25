//! Command Code GOAT runtime: verified GET `/models` catalogs and eligibility.
//!
//! Production inference uses saved catalog facts plus hard-coded family rules.
//! Verification is the only outbound catalog call.

use crate::http_client;
use crate::models::AppConfig;
use crate::provider::{
    COMMAND_CODE_GOAT_BASE_URL, COMMAND_CODE_GOAT_MODELS_PATH, COMMAND_CODE_PROVIDER_ID,
    ConnectionVerificationStatus, GOAT_OFFERING_ID, GoatModelAccess, is_command_code_goat,
    parse_command_code_models_catalog,
};
use ocg_domain::ids::custom_model_id_matches;
use reqwest::StatusCode;
use std::collections::HashMap;
use std::fmt;
#[cfg(debug_assertions)]
use std::sync::{LazyLock, RwLock};
use std::time::Duration;

/// Snapshot used to reject stale GOAT verification commits after network I/O.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GoatVerificationContract {
    pub account_id: String,
    pub account_updated_at: String,
    pub key_cipher: String,
}

/// Data-only GOAT routing state loaded from persistence for one account.
#[derive(Debug, Clone)]
pub struct GoatAccountRuntime {
    pub account_id: String,
    pub enabled: bool,
    pub verification_status: ConnectionVerificationStatus,
    pub setup_ready: bool,
    pub has_key: bool,
    pub model_access: GoatModelAccess,
    pub models: Vec<String>,
}

pub const MAX_GOAT_VERIFICATION_BODY_BYTES: usize = 256 * 1024;
pub const GOAT_VERIFICATION_CONFLICT_MESSAGE: &str =
    "the Command Code GOAT account changed while it was being verified; retry verification";

#[cfg(debug_assertions)]
static GOAT_VERIFY_ORIGINS: LazyLock<RwLock<HashMap<u64, String>>> =
    LazyLock::new(|| RwLock::new(HashMap::new()));

/// RAII guard for the debug-only GOAT verification origin substitute.
#[cfg(debug_assertions)]
#[doc(hidden)]
pub struct GoatVerifyOriginGuard {
    process_generation: u64,
    origin: String,
}

#[cfg(debug_assertions)]
impl Drop for GoatVerifyOriginGuard {
    fn drop(&mut self) {
        if let Ok(mut origins) = GOAT_VERIFY_ORIGINS.write()
            && origins
                .get(&self.process_generation)
                .is_some_and(|origin| origin == &self.origin)
        {
            origins.remove(&self.process_generation);
        }
    }
}

/// Installs a loopback-only origin used by GOAT GET `/models` tests.
#[cfg(debug_assertions)]
#[doc(hidden)]
pub fn install_goat_verify_origin_for_test(
    process_generation: u64,
    origin: impl Into<String>,
) -> Result<GoatVerifyOriginGuard, String> {
    let origin = origin.into();
    ensure_loopback_origin(&origin)?;
    let origin = origin.trim_end_matches('/').to_string();
    let guard = GoatVerifyOriginGuard {
        process_generation,
        origin: origin.clone(),
    };
    GOAT_VERIFY_ORIGINS
        .write()
        .map_err(|_| "GOAT verify origin lock is poisoned".to_string())?
        .insert(process_generation, origin);
    Ok(guard)
}

#[cfg(debug_assertions)]
pub fn goat_verify_base_url(process_generation: Option<u64>) -> String {
    if let Some(generation) = process_generation
        && let Ok(origins) = GOAT_VERIFY_ORIGINS.read()
        && let Some(origin) = origins.get(&generation)
    {
        return format!("{}/provider/v1", origin.trim_end_matches('/'));
    }
    COMMAND_CODE_GOAT_BASE_URL.to_string()
}

#[cfg(debug_assertions)]
fn ensure_loopback_origin(origin: &str) -> Result<(), String> {
    let url = reqwest::Url::parse(origin).map_err(|error| error.to_string())?;
    if url.scheme() != "http"
        || !matches!(
            url.host_str(),
            Some("localhost") | Some("127.0.0.1") | Some("::1") | Some("[::1]")
        )
    {
        return Err("GOAT verify test origin must be an HTTP loopback URL".to_string());
    }
    Ok(())
}

impl GoatAccountRuntime {
    pub fn eligible(&self) -> bool {
        self.enabled
            && self.verification_status == ConnectionVerificationStatus::Verified
            && self.setup_ready
            && self.has_key
            && self
                .models
                .iter()
                .any(|model| self.model_access.allows(model))
            && is_command_code_goat(COMMAND_CODE_PROVIDER_ID, GOAT_OFFERING_ID)
    }

    pub fn serves(&self, requested: &str) -> bool {
        self.eligible()
            && self.models.iter().any(|model| {
                self.model_access.allows(model) && custom_model_id_matches(model, requested)
            })
    }
}

pub fn goat_runtimes_by_account(
    runtimes: &[GoatAccountRuntime],
) -> HashMap<String, GoatAccountRuntime> {
    runtimes
        .iter()
        .cloned()
        .map(|runtime| (runtime.account_id.clone(), runtime))
        .collect()
}

pub fn eligible_goat_model_ids(runtimes: &[GoatAccountRuntime]) -> Vec<String> {
    let mut ids = Vec::new();
    for runtime in runtimes.iter().filter(|runtime| runtime.eligible()) {
        for model in &runtime.models {
            if !runtime.model_access.allows(model) {
                continue;
            }
            if ids
                .iter()
                .any(|existing: &String| custom_model_id_matches(existing, model))
            {
                continue;
            }
            ids.push(model.clone());
        }
    }
    ids
}

#[derive(Debug, Clone)]
pub struct GoatVerifyFailure {
    pub message: String,
}

impl fmt::Display for GoatVerifyFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for GoatVerifyFailure {}

pub fn official_goat_models_url() -> String {
    format!(
        "{}{}",
        COMMAND_CODE_GOAT_BASE_URL.trim_end_matches('/'),
        COMMAND_CODE_GOAT_MODELS_PATH
    )
}

pub fn goat_models_url_for_base(base: &str) -> String {
    format!(
        "{}{}",
        base.trim_end_matches('/'),
        COMMAND_CODE_GOAT_MODELS_PATH
    )
}

pub async fn probe_goat_models(
    config: &AppConfig,
    api_key: &str,
    base_url: &str,
) -> Result<Vec<String>, GoatVerifyFailure> {
    probe_provider_models(config, api_key, base_url, "Command Code GOAT").await
}

pub async fn probe_provider_models(
    config: &AppConfig,
    api_key: &str,
    base_url: &str,
    provider_label: &str,
) -> Result<Vec<String>, GoatVerifyFailure> {
    let key = api_key.trim();
    if key.is_empty() {
        return Err(GoatVerifyFailure {
            message: format!("{provider_label} model refresh requires a stored Key"),
        });
    }
    let url = goat_models_url_for_base(base_url);
    let client = http_client::configured_builder(config)
        .and_then(|builder| {
            builder
                .connect_timeout(Duration::from_secs(config.connect_timeout_secs))
                .redirect(http_client::no_redirect_policy())
                .build()
                .map_err(Into::into)
        })
        .map_err(|error| GoatVerifyFailure {
            message: format!("failed to build {provider_label} model client: {error}"),
        })?;
    let response = client
        .get(&url)
        .bearer_auth(key)
        .header(reqwest::header::ACCEPT, "application/json")
        .timeout(Duration::from_secs(config.non_stream_timeout_secs))
        .send()
        .await
        .map_err(|error| GoatVerifyFailure {
            message: format!("{provider_label} GET /models failed: {error}"),
        })?;
    let status = response.status();
    let bytes = read_limited_body(response).await?;
    if status == StatusCode::UNAUTHORIZED || status == StatusCode::FORBIDDEN {
        return Err(GoatVerifyFailure {
            message: format!("{provider_label} GET /models returned {}", status.as_u16()),
        });
    }
    if !status.is_success() {
        return Err(GoatVerifyFailure {
            message: format!("{provider_label} GET /models returned {}", status.as_u16()),
        });
    }
    parse_command_code_models_catalog(&bytes).map_err(|message| GoatVerifyFailure { message })
}

async fn read_limited_body(response: reqwest::Response) -> Result<Vec<u8>, GoatVerifyFailure> {
    let mut bytes = Vec::new();
    let mut stream = response.bytes_stream();
    use futures_util::StreamExt;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|error| GoatVerifyFailure {
            message: format!("Command Code GOAT GET /models body failed: {error}"),
        })?;
        if bytes.len() + chunk.len() > MAX_GOAT_VERIFICATION_BODY_BYTES {
            return Err(GoatVerifyFailure {
                message: format!(
                    "Command Code GOAT GET /models exceeded the {MAX_GOAT_VERIFICATION_BODY_BYTES}-byte limit"
                ),
            });
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::GoatModelAccess;

    fn runtime(model_access: GoatModelAccess) -> GoatAccountRuntime {
        GoatAccountRuntime {
            account_id: "goat-1".into(),
            enabled: true,
            verification_status: ConnectionVerificationStatus::Verified,
            setup_ready: true,
            has_key: true,
            model_access,
            models: vec!["gpt-5.6-sol".into(), "anthropic/claude-opus-4.1".into()],
        }
    }

    #[test]
    fn goat_mode_filters_saved_catalog_while_all_mode_keeps_it() {
        let goat = runtime(GoatModelAccess::Goat);
        assert!(goat.serves("gpt-5.6-sol"));
        assert!(!goat.serves("anthropic/claude-opus-4.1"));
        assert_eq!(eligible_goat_model_ids(&[goat]), vec!["gpt-5.6-sol"]);

        let all = runtime(GoatModelAccess::All);
        assert!(all.serves("anthropic/claude-opus-4.1"));
        assert_eq!(eligible_goat_model_ids(&[all]).len(), 2);
    }
}
