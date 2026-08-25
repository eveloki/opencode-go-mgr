//! Custom API runtime helpers: capability matching, verification probe, and
//! per-account route identity.
//!
//! Account model capabilities are the client-facing IDs and the exact upstream
//! IDs. Verification sends one protocol-correct non-stream request against the
//! first declared model. Discovery never mutates the declared list.
//! The adapter identity is Configurable HTTP, not a base class other providers
//! inherit from. Custom keeps configurable URL/auth and
//! verified-then-explicit-enable.

use crate::custom_http::{
    self, CustomHttpClient, CustomHttpError, HttpInferenceTransport, InferenceHttpError,
    join_custom_endpoint, json_content_headers,
};
use crate::kernel::ids::{CUSTOM_API_OFFERING_ID, CUSTOM_PROVIDER_ID};
use crate::kernel::protocol::ApiFormat;
use crate::models::{
    AccountCustomConfig, AccountCustomConfigInput, AccountModelCapability, AppConfig,
    CustomModelDiscoveryResult,
};
use crate::provider::ConnectionVerificationStatus;
use crate::provider::{
    UpstreamAuthScheme, UpstreamProtocolKind, custom_endpoint_relative_path, is_custom_api,
};
use reqwest::StatusCode;
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::time::Duration;

/// Canonical definition: [`crate::kernel::ids::custom_model_id_matches`].
pub use crate::kernel::ids::custom_model_id_matches;

/// Custom destination URL trust policy lives with the outbound boundary in
/// [`crate::custom_http`]; re-exported here for the Custom runtime surface.
pub use crate::custom_http::{
    CustomUrlHost, CustomUrlTarget, inspect_custom_url, validate_custom_base_url,
};

/// Upper bound for a Custom verification response. The probe only needs a 2xx
/// JSON object; anything larger is rejected without certifying the account.
pub const MAX_CUSTOM_VERIFICATION_BODY_BYTES: usize = 64 * 1024;

/// Discovery is an interactive dashboard aid, not an unbounded upstream
/// directory mirror. These caps keep a malicious or accidental endpoint from
/// consuming arbitrary memory or issuing an unbounded cursor chain.
pub const MAX_CUSTOM_MODEL_DISCOVERY_BODY_BYTES: usize = 256 * 1024;
pub const MAX_CUSTOM_MODEL_DISCOVERY_MODELS: usize = 1_000;
pub const MAX_CUSTOM_MODEL_DISCOVERY_PAGES: usize = 10;
pub const CUSTOM_MODEL_DISCOVERY_TIMEOUT_SECS: u64 = 30;

/// Dashboard conflict when a stale Custom probe no longer matches the account.
pub const CUSTOM_VERIFICATION_CONFLICT_MESSAGE: &str =
    "the Custom account changed while it was being verified; retry verification";

/// One Custom account's persisted config + declared capabilities, in account order.
#[derive(Debug, Clone)]
pub struct CustomAccountRuntime {
    pub account_id: String,
    pub enabled: bool,
    pub verification_status: ConnectionVerificationStatus,
    pub setup_ready: bool,
    pub has_key: bool,
    pub config: AccountCustomConfig,
    pub capabilities: Vec<AccountModelCapability>,
}

impl CustomAccountRuntime {
    pub fn eligible(&self) -> bool {
        self.enabled
            && self.verification_status == ConnectionVerificationStatus::Verified
            && self.setup_ready
            && self.has_key
            && is_custom_api(CUSTOM_PROVIDER_ID, CUSTOM_API_OFFERING_ID)
    }

    pub fn capability_matching(&self, requested: &str) -> Option<&AccountModelCapability> {
        self.capabilities
            .iter()
            .find(|capability| custom_model_id_matches(&capability.model_id, requested))
    }
}

pub fn custom_runtimes_by_account(
    runtimes: &[CustomAccountRuntime],
) -> HashMap<String, CustomAccountRuntime> {
    runtimes
        .iter()
        .cloned()
        .map(|runtime| (runtime.account_id.clone(), runtime))
        .collect()
}

/// Case-preserving declared IDs from eligible enabled+verified Custom accounts,
/// de-duplicated in account then capability order.
pub fn eligible_custom_model_ids(runtimes: &[CustomAccountRuntime]) -> Vec<String> {
    let mut ids = Vec::new();
    for runtime in runtimes.iter().filter(|runtime| runtime.eligible()) {
        for capability in &runtime.capabilities {
            if ids
                .iter()
                .any(|existing: &String| custom_model_id_matches(existing, &capability.model_id))
            {
                continue;
            }
            ids.push(capability.model_id.clone());
        }
    }
    ids
}

pub fn any_eligible_custom_model(runtimes: &[CustomAccountRuntime], requested: &str) -> bool {
    runtimes
        .iter()
        .any(|runtime| runtime.eligible() && runtime.capability_matching(requested).is_some())
}

pub fn api_format_for_custom_protocol(protocol: UpstreamProtocolKind) -> ApiFormat {
    match protocol {
        UpstreamProtocolKind::ChatCompletions => ApiFormat::ChatCompletions,
        UpstreamProtocolKind::Responses => ApiFormat::Responses,
        UpstreamProtocolKind::Messages => ApiFormat::Messages,
    }
}

pub fn join_custom_protocol_url(
    base_url: &str,
    protocol: UpstreamProtocolKind,
) -> Result<reqwest::Url, CustomHttpError> {
    join_custom_endpoint(base_url, custom_endpoint_relative_path(protocol))
}

/// Immutable identity of the Custom account a verification probe was issued
/// against. Commit is allowed only when this exact contract still exists and
/// the account is still unverified (`pending` or `failed`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CustomVerificationContract {
    pub account_id: String,
    /// Raw `accounts.updated_at` text; the per-account revision token.
    pub account_updated_at: String,
    /// Encrypted key ciphertext, not the plaintext secret.
    pub key_cipher: String,
    pub base_url: String,
    pub upstream_protocol: UpstreamProtocolKind,
    pub auth_scheme: UpstreamAuthScheme,
    /// Declared capability IDs in persistence order.
    pub capabilities: Vec<(String, UpstreamProtocolKind)>,
}

impl CustomVerificationContract {
    pub fn from_parts(
        account_id: impl Into<String>,
        account_updated_at: impl Into<String>,
        key_cipher: impl Into<String>,
        config: &AccountCustomConfig,
        capabilities: &[AccountModelCapability],
    ) -> Self {
        Self {
            account_id: account_id.into(),
            account_updated_at: account_updated_at.into(),
            key_cipher: key_cipher.into(),
            base_url: config.base_url.clone(),
            upstream_protocol: config.upstream_protocol,
            auth_scheme: config.auth_scheme,
            capabilities: capabilities
                .iter()
                .map(|capability| (capability.model_id.clone(), capability.protocol))
                .collect(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CustomVerifyFailure {
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CustomModelDiscoveryFailure {
    pub message: String,
}

impl fmt::Display for CustomModelDiscoveryFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for CustomModelDiscoveryFailure {}

/// Fetch declared model IDs from the one safe endpoint formed by the Custom
/// base URL. This never probes completion endpoints and never writes account
/// state. OpenAI- and Anthropic-compatible list envelopes both use `data`.
pub async fn discover_custom_models(
    config: &AppConfig,
    input: &AccountCustomConfigInput,
    api_key: &str,
) -> Result<CustomModelDiscoveryResult, CustomModelDiscoveryFailure> {
    tokio::time::timeout(
        Duration::from_secs(CUSTOM_MODEL_DISCOVERY_TIMEOUT_SECS),
        discover_custom_models_inner(config, input, api_key),
    )
    .await
    .map_err(|_| CustomModelDiscoveryFailure {
        message: format!(
            "Custom model discovery timed out after {CUSTOM_MODEL_DISCOVERY_TIMEOUT_SECS} seconds"
        ),
    })?
}

async fn discover_custom_models_inner(
    config: &AppConfig,
    input: &AccountCustomConfigInput,
    api_key: &str,
) -> Result<CustomModelDiscoveryResult, CustomModelDiscoveryFailure> {
    if api_key.trim().is_empty() {
        return Err(CustomModelDiscoveryFailure {
            message: "Custom model discovery requires an API key".to_string(),
        });
    }
    let mut url = join_custom_endpoint(&input.base_url, "models").map_err(|error| {
        CustomModelDiscoveryFailure {
            message: format!("invalid Custom model discovery endpoint: {error}"),
        }
    })?;
    let client = custom_http::build_custom_http_client(config).map_err(|error| {
        CustomModelDiscoveryFailure {
            message: format!("failed to build Custom HTTP client: {error}"),
        }
    })?;
    let headers = model_discovery_headers(input.upstream_protocol);
    let timeout = Some(model_discovery_request_timeout(config));
    let mut models = Vec::new();
    let mut seen_models = HashSet::new();
    let mut seen_cursors = HashSet::new();

    for page in 0..MAX_CUSTOM_MODEL_DISCOVERY_PAGES {
        let response = client
            .send_isolated(
                reqwest::Method::GET,
                url.clone(),
                input.auth_scheme,
                api_key,
                headers.clone(),
                None,
                timeout,
            )
            .await
            .map_err(|error| CustomModelDiscoveryFailure {
                message: format!("Custom model discovery network or timeout error: {error}"),
            })?;
        let status = response.status();
        let body = read_custom_model_discovery_body(response).await?;
        if !status.is_success() {
            return Err(CustomModelDiscoveryFailure {
                message: discovery_status_message(status),
            });
        }
        let page_result = parse_model_discovery_page(&body)?;
        for model in page_result.models {
            if seen_models.insert(model.to_ascii_lowercase()) {
                models.push(model);
                if models.len() >= MAX_CUSTOM_MODEL_DISCOVERY_MODELS {
                    return Ok(CustomModelDiscoveryResult {
                        models,
                        truncated: true,
                    });
                }
            }
        }
        if !page_result.has_more {
            return Ok(CustomModelDiscoveryResult {
                models,
                truncated: false,
            });
        }
        if page + 1 >= MAX_CUSTOM_MODEL_DISCOVERY_PAGES {
            return Ok(CustomModelDiscoveryResult {
                models,
                truncated: true,
            });
        }
        let cursor = page_result.cursor.or(page_result.last_valid_id).ok_or_else(|| CustomModelDiscoveryFailure {
            message: "Custom model discovery response has_more=true but contains no valid model ID for after_id".to_string(),
        })?;
        advance_model_discovery_cursor(&mut url, &mut seen_cursors, &cursor)?;
    }
    unreachable!("bounded discovery loop always returns")
}

fn model_discovery_request_timeout(config: &AppConfig) -> Duration {
    Duration::from_secs(
        config
            .non_stream_timeout_secs
            .min(CUSTOM_MODEL_DISCOVERY_TIMEOUT_SECS),
    )
}

fn model_discovery_headers(protocol: UpstreamProtocolKind) -> reqwest::header::HeaderMap {
    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert(
        reqwest::header::ACCEPT,
        reqwest::header::HeaderValue::from_static("application/json"),
    );
    if protocol == UpstreamProtocolKind::Messages {
        headers.insert(
            reqwest::header::HeaderName::from_static("anthropic-version"),
            reqwest::header::HeaderValue::from_static("2023-06-01"),
        );
    }
    headers
}

struct CustomModelDiscoveryPage {
    models: Vec<String>,
    has_more: bool,
    last_valid_id: Option<String>,
    cursor: Option<String>,
}

fn parse_model_discovery_page(
    body: &[u8],
) -> Result<CustomModelDiscoveryPage, CustomModelDiscoveryFailure> {
    let value: Value = serde_json::from_slice(body).map_err(|_| CustomModelDiscoveryFailure {
        message: "Custom model discovery did not return JSON with a data array".to_string(),
    })?;
    let object = value
        .as_object()
        .ok_or_else(|| CustomModelDiscoveryFailure {
            message: "Custom model discovery did not return a JSON object with a data array"
                .to_string(),
        })?;
    let data = object
        .get("data")
        .and_then(Value::as_array)
        .ok_or_else(|| CustomModelDiscoveryFailure {
            message: "Custom model discovery response is missing a data array".to_string(),
        })?;
    let has_more = match object.get("has_more") {
        None => false,
        Some(Value::Bool(value)) => *value,
        Some(_) => {
            return Err(CustomModelDiscoveryFailure {
                message: "Custom model discovery response has an invalid has_more value"
                    .to_string(),
            });
        }
    };
    let cursor = match object.get("last_id") {
        None | Some(Value::Null) => None,
        Some(Value::String(value)) if !value.chars().any(char::is_control) => Some(
            crate::provider::validate_custom_model_id(value).map_err(|_| {
                CustomModelDiscoveryFailure {
                    message: "Custom model discovery response has an invalid last_id cursor"
                        .to_string(),
                }
            })?,
        ),
        Some(_) => {
            return Err(CustomModelDiscoveryFailure {
                message: "Custom model discovery response has an invalid last_id cursor"
                    .to_string(),
            });
        }
    };
    let mut models = Vec::new();
    let mut last_valid_id = None;
    for item in data {
        let Some(id) = item
            .as_object()
            .and_then(|item| item.get("id"))
            .and_then(Value::as_str)
        else {
            continue;
        };
        if id.chars().any(char::is_control) {
            continue;
        }
        let Ok(id) = crate::provider::validate_custom_model_id(id) else {
            continue;
        };
        last_valid_id = Some(id.clone());
        models.push(id);
    }
    Ok(CustomModelDiscoveryPage {
        models,
        has_more,
        last_valid_id,
        cursor,
    })
}

fn advance_model_discovery_cursor(
    url: &mut reqwest::Url,
    seen_cursors: &mut HashSet<String>,
    cursor: &str,
) -> Result<(), CustomModelDiscoveryFailure> {
    if !seen_cursors.insert(cursor.to_ascii_lowercase()) {
        return Err(CustomModelDiscoveryFailure {
            message: "Custom model discovery cursor loop detected".to_string(),
        });
    }
    // The base endpoint was validated above and has no query. Only this
    // encoded cursor is added; no upstream-provided URL is ever followed.
    // Replacing rather than appending avoids a multi-page after_id chain.
    url.set_query(None);
    url.query_pairs_mut().append_pair("after_id", cursor);
    Ok(())
}

async fn read_custom_model_discovery_body(
    response: reqwest::Response,
) -> Result<Vec<u8>, CustomModelDiscoveryFailure> {
    HttpInferenceTransport::read_body_limited(response, MAX_CUSTOM_MODEL_DISCOVERY_BODY_BYTES)
        .await
        .map_err(|error| match error {
            InferenceHttpError::Oversize { .. } => oversized_model_discovery_body(),
            other => CustomModelDiscoveryFailure {
                message: format!("Custom model discovery response body failed: {other}"),
            },
        })
}

#[cfg(test)]
fn model_discovery_body_size_allowed(size: usize) -> Result<(), CustomModelDiscoveryFailure> {
    if size > MAX_CUSTOM_MODEL_DISCOVERY_BODY_BYTES {
        Err(oversized_model_discovery_body())
    } else {
        Ok(())
    }
}

fn oversized_model_discovery_body() -> CustomModelDiscoveryFailure {
    CustomModelDiscoveryFailure {
        message: format!(
            "Custom model discovery response exceeded the {MAX_CUSTOM_MODEL_DISCOVERY_BODY_BYTES}-byte limit"
        ),
    }
}

fn discovery_status_message(status: StatusCode) -> String {
    match status {
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => format!(
            "Custom model discovery authentication failed (upstream returned {})",
            status.as_u16()
        ),
        StatusCode::NOT_FOUND | StatusCode::METHOD_NOT_ALLOWED => format!(
            "Custom model discovery is unsupported at this endpoint (upstream returned {})",
            status.as_u16()
        ),
        StatusCode::TOO_MANY_REQUESTS => {
            "Custom model discovery is rate limited by the upstream (429)".to_string()
        }
        status if status.is_server_error() => format!(
            "Custom model discovery upstream server error ({})",
            status.as_u16()
        ),
        _ => format!(
            "Custom model discovery upstream returned {}",
            status.as_u16()
        ),
    }
}

impl fmt::Display for CustomVerifyFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for CustomVerifyFailure {}

pub fn first_declared_capability(
    capabilities: &[AccountModelCapability],
) -> Option<&AccountModelCapability> {
    capabilities.first()
}

pub fn minimal_verification_body(
    protocol: UpstreamProtocolKind,
    model_id: &str,
) -> Result<Vec<u8>, CustomVerifyFailure> {
    let body = match protocol {
        UpstreamProtocolKind::ChatCompletions => json!({
            "model": model_id,
            "messages": [{"role": "user", "content": "ping"}],
            "max_tokens": 1,
            "stream": false
        }),
        UpstreamProtocolKind::Responses => json!({
            "model": model_id,
            "input": "ping",
            "max_output_tokens": 1,
            "store": false,
            "stream": false
        }),
        UpstreamProtocolKind::Messages => json!({
            "model": model_id,
            "max_tokens": 1,
            "stream": false,
            "messages": [{"role": "user", "content": "ping"}]
        }),
    };
    serde_json::to_vec(&body).map_err(|error| CustomVerifyFailure {
        message: format!("failed to encode Custom verification request: {error}"),
    })
}

/// POST one protocol-correct non-stream request. Only a 2xx JSON object proves
/// verified. Never uses GET /models and never mutates capabilities.
pub async fn probe_custom_connection(
    config: &AppConfig,
    custom_config: &AccountCustomConfig,
    first_capability: &AccountModelCapability,
    api_key: &str,
) -> Result<(), CustomVerifyFailure> {
    if first_capability.protocol != custom_config.upstream_protocol {
        return Err(CustomVerifyFailure {
            message: "model capability protocol must match account custom_config.upstream_protocol"
                .to_string(),
        });
    }
    let url = join_custom_protocol_url(&custom_config.base_url, custom_config.upstream_protocol)
        .map_err(|error| CustomVerifyFailure {
            message: format!("invalid Custom verification endpoint: {error}"),
        })?;
    let body =
        minimal_verification_body(custom_config.upstream_protocol, &first_capability.model_id)?;
    let client = CustomHttpClient::from_config(config)?;
    let extra =
        json_content_headers(custom_config.upstream_protocol == UpstreamProtocolKind::Messages)
            .map_err(|error| CustomVerifyFailure {
                message: error.to_string(),
            })?;
    let response = client
        .send_isolated(
            reqwest::Method::POST,
            url,
            custom_config.auth_scheme,
            api_key,
            extra,
            Some(body),
            Some(Duration::from_secs(config.non_stream_timeout_secs)),
        )
        .await
        .map_err(|error| CustomVerifyFailure {
            message: format!("Custom verification request failed: {error}"),
        })?;
    let status = response.status();
    let bytes = read_custom_verification_body(response).await?;
    prove_verified_json_object(status, &bytes)
}

async fn read_custom_verification_body(
    response: reqwest::Response,
) -> Result<Vec<u8>, CustomVerifyFailure> {
    HttpInferenceTransport::read_body_limited(response, MAX_CUSTOM_VERIFICATION_BODY_BYTES)
        .await
        .map_err(|error| match error {
            InferenceHttpError::Oversize { .. } => oversized_verification_body(),
            other => CustomVerifyFailure {
                message: format!("Custom verification response body failed: {other}"),
            },
        })
}

fn oversized_verification_body() -> CustomVerifyFailure {
    CustomVerifyFailure {
        message: format!(
            "Custom verification response exceeded the {MAX_CUSTOM_VERIFICATION_BODY_BYTES}-byte limit"
        ),
    }
}

fn prove_verified_json_object(status: StatusCode, body: &[u8]) -> Result<(), CustomVerifyFailure> {
    if !status.is_success() {
        return Err(CustomVerifyFailure {
            message: format!("Custom verification upstream returned {}", status.as_u16()),
        });
    }
    let parsed: Value = serde_json::from_slice(body).map_err(|_| CustomVerifyFailure {
        message: "Custom verification did not return a JSON object".to_string(),
    })?;
    if !parsed.is_object() {
        return Err(CustomVerifyFailure {
            message: "Custom verification did not return a JSON object".to_string(),
        });
    }
    Ok(())
}

impl CustomHttpClient {
    fn from_config(config: &AppConfig) -> Result<Self, CustomVerifyFailure> {
        custom_http::build_custom_http_client(config).map_err(|error| CustomVerifyFailure {
            message: format!("failed to build Custom HTTP client: {error}"),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::ProviderBindingError;
    use std::net::IpAddr;

    #[test]
    fn custom_base_url_trusts_administrator_http_origins_and_rejects_credentials() {
        use crate::provider::validate_custom_model_id;

        assert!(validate_custom_base_url("https://api.example.com/v1").is_ok());
        assert!(validate_custom_base_url("http://127.0.0.1:8080/v1").is_ok());
        assert!(validate_custom_base_url("http://localhost:3000").is_ok());
        assert!(validate_custom_base_url("http://app.localhost/v1").is_ok());
        assert!(validate_custom_base_url("http://api.example.com/v1").is_ok());
        assert!(validate_custom_base_url("https://192.168.1.8/v1").is_ok());
        assert!(validate_custom_base_url("http://10.0.0.1:9000/v1").is_ok());
        assert!(validate_custom_base_url("https://169.254.169.254/latest").is_ok());
        assert!(validate_custom_base_url("http://metadata.google.internal/").is_ok());
        assert!(validate_custom_base_url("https://[::ffff:169.254.169.254]/").is_ok());
        assert!(validate_custom_base_url("https://[2001:db8::1]/v1").is_ok());
        assert!(validate_custom_base_url("https://user:pass@api.example.com").is_err());
        assert!(validate_custom_base_url("https://api.example.com/v1?x=1").is_err());
        assert!(validate_custom_base_url("https://api.example.com/v1#frag").is_err());
        assert!(validate_custom_base_url("javascript:alert(1)").is_err());
        assert!(validate_custom_base_url("ftp://api.example.com/v1").is_err());
        assert_eq!(
            validate_custom_model_id("deepseek/deepseek-v4-flash").unwrap(),
            "deepseek/deepseek-v4-flash"
        );
        assert!(validate_custom_model_id("").is_err());
        assert_eq!(
            custom_endpoint_relative_path(UpstreamProtocolKind::ChatCompletions),
            "chat/completions"
        );
        assert_eq!(
            custom_endpoint_relative_path(UpstreamProtocolKind::Responses),
            "responses"
        );
        assert_eq!(
            custom_endpoint_relative_path(UpstreamProtocolKind::Messages),
            "messages"
        );
    }

    #[test]
    fn custom_url_host_uses_url_host_not_bracketed_host_str() {
        assert!(validate_custom_base_url("http://[::ffff:127.0.0.1]/v1").is_ok());
        assert!(validate_custom_base_url("http://[::1]/v1").is_ok());
        let mapped_loopback = validate_custom_base_url("http://[::ffff:127.0.0.1]/v1").unwrap();
        let parsed = reqwest::Url::parse(&mapped_loopback).unwrap();
        match inspect_custom_url(&parsed).unwrap().host {
            CustomUrlHost::Ip(ip) => {
                assert_eq!(ip, "::ffff:127.0.0.1".parse::<IpAddr>().unwrap());
            }
            CustomUrlHost::Domain(domain) => {
                panic!("mapped loopback must stay an IP host, got {domain}")
            }
        }
        let metadata = validate_custom_base_url("https://[::ffff:169.254.169.254]/latest").unwrap();
        let parsed = reqwest::Url::parse(&metadata).unwrap();
        match inspect_custom_url(&parsed).unwrap().host {
            CustomUrlHost::Ip(_) => {}
            CustomUrlHost::Domain(domain) => {
                panic!("mapped metadata IP must stay an IP host, got {domain}")
            }
        }
    }

    #[test]
    fn custom_base_url_normalizes_decimal_loopback_literals() {
        assert_eq!(
            validate_custom_base_url("http://127.1:8080/v1").unwrap(),
            "http://127.0.0.1:8080/v1"
        );
        assert_eq!(
            validate_custom_base_url("http://127.0.1/v1").unwrap(),
            "http://127.0.0.1/v1"
        );
        let parsed = reqwest::Url::parse("http://127.1/v1").unwrap();
        match inspect_custom_url(&parsed).unwrap().host {
            CustomUrlHost::Ip(ip) => assert_eq!(ip, "127.0.0.1".parse::<IpAddr>().unwrap()),
            CustomUrlHost::Domain(domain) => panic!("127.1 must not stay a domain: {domain}"),
        }
    }

    #[test]
    fn custom_base_url_errors_keep_existing_variants_and_messages() {
        assert_eq!(
            validate_custom_base_url("").unwrap_err(),
            ProviderBindingError::InvalidCustomBaseUrl("base URL is required".to_string())
        );
        assert_eq!(
            validate_custom_base_url("   ").unwrap_err(),
            ProviderBindingError::InvalidCustomBaseUrl("base URL is required".to_string())
        );
        let too_long = format!("https://api.example.com/{}", "a".repeat(2048));
        assert_eq!(
            validate_custom_base_url(&too_long).unwrap_err(),
            ProviderBindingError::InvalidCustomBaseUrl("base URL is too long".to_string())
        );
        let parsed_err = validate_custom_base_url("not a url").unwrap_err();
        match parsed_err {
            ProviderBindingError::InvalidCustomBaseUrl(message) => {
                assert!(message.starts_with("invalid base URL: "), "{message}");
            }
            other => panic!("expected InvalidCustomBaseUrl, got {other:?}"),
        }
        assert_eq!(
            validate_custom_base_url("https://api.example.com/v1?x=1").unwrap_err(),
            ProviderBindingError::InvalidCustomBaseUrl(
                "base URL must not include a query or fragment".to_string()
            )
        );
        assert_eq!(
            validate_custom_base_url("https://api.example.com/v1#frag").unwrap_err(),
            ProviderBindingError::InvalidCustomBaseUrl(
                "base URL must not include a query or fragment".to_string()
            )
        );
        assert_eq!(
            validate_custom_base_url("ftp://api.example.com/v1").unwrap_err(),
            ProviderBindingError::InvalidCustomBaseUrl(
                "base URL must use http or https".to_string()
            )
        );
        assert_eq!(
            validate_custom_base_url("javascript:alert(1)").unwrap_err(),
            ProviderBindingError::InvalidCustomBaseUrl(
                "base URL must use http or https".to_string()
            )
        );
        assert_eq!(
            validate_custom_base_url("https://user:pass@api.example.com").unwrap_err(),
            ProviderBindingError::InvalidCustomBaseUrl(
                "base URL must not include credentials".to_string()
            )
        );
        let hostless = reqwest::Url::parse("file:///tmp").unwrap();
        assert_eq!(
            inspect_custom_url(&hostless).unwrap_err(),
            ProviderBindingError::InvalidCustomBaseUrl(
                "base URL must use http or https".to_string()
            )
        );
    }

    #[test]
    fn custom_runtime_identity_is_configurable_http_not_a_base_class() {
        use crate::provider::{
            ANONYMOUS_FREE_OFFERING_ID, COMMAND_CODE_PROVIDER_ID, ConfigurableHttpAdapter,
            GO_OFFERING_ID, GOAT_OFFERING_ID, OPENCODE_PROVIDER_ID, OPENCODE_ZEN_FREE_PROVIDER_ID,
            ProviderAdapterKind, SCNET_PROVIDER_ID, SCNET_TOKEN_PLAN_BASIC_OFFERING_ID,
            VerificationAdapter, builtin_plan,
        };
        assert_eq!(
            ProviderAdapterKind::from_offering(CUSTOM_PROVIDER_ID, CUSTOM_API_OFFERING_ID),
            Some(ProviderAdapterKind::ConfigurableHttp)
        );
        for (provider_id, offering_id) in [
            (OPENCODE_PROVIDER_ID, GO_OFFERING_ID),
            (OPENCODE_ZEN_FREE_PROVIDER_ID, ANONYMOUS_FREE_OFFERING_ID),
            (COMMAND_CODE_PROVIDER_ID, GOAT_OFFERING_ID),
            (SCNET_PROVIDER_ID, SCNET_TOKEN_PLAN_BASIC_OFFERING_ID),
        ] {
            assert_ne!(
                ProviderAdapterKind::from_offering(provider_id, offering_id),
                Some(ProviderAdapterKind::ConfigurableHttp)
            );
        }
        let runtime = CustomAccountRuntime {
            account_id: "acc".into(),
            enabled: true,
            verification_status: ConnectionVerificationStatus::Verified,
            setup_ready: true,
            has_key: true,
            config: AccountCustomConfig {
                account_id: "acc".into(),
                base_url: "http://127.0.0.1:9".into(),
                upstream_protocol: UpstreamProtocolKind::ChatCompletions,
                auth_scheme: crate::provider::UpstreamAuthScheme::Bearer,
                created_at: chrono::Utc::now(),
                updated_at: chrono::Utc::now(),
            },
            capabilities: Vec::new(),
        };
        assert!(runtime.eligible());
        let plan = builtin_plan(CUSTOM_PROVIDER_ID, CUSTOM_API_OFFERING_ID).unwrap();
        let verification = ConfigurableHttpAdapter::verification(plan);
        assert!(verification.never_auto_enable);
        assert!(verification.probe_first_declared_model);
        assert!(!verification.uses_get_models);
        assert_eq!(
            verification.runtime_availability,
            plan.verification_runtime_availability
        );
    }

    #[test]
    fn custom_model_id_matching_is_exact_or_case_folded_without_separator_folding() {
        assert!(custom_model_id_matches("glm-5.2", "GLM-5.2"));
        assert!(custom_model_id_matches("my-local", "my-local"));
        assert!(!custom_model_id_matches("glm-5.2", "glm/5.2"));
        assert!(custom_model_id_matches(
            "deepseek/deepseek-v4-flash",
            "DeepSeek/deepseek-v4-flash"
        ));
        assert!(!custom_model_id_matches(
            "deepseek/deepseek-v4-flash",
            "deepseek-v4-flash"
        ));
    }

    #[test]
    fn verification_bodies_are_non_stream_and_token_bounded() {
        let chat = serde_json::from_slice::<Value>(
            &minimal_verification_body(UpstreamProtocolKind::ChatCompletions, "local-model")
                .unwrap(),
        )
        .unwrap();
        assert_eq!(chat["stream"], false);
        assert_eq!(chat["max_tokens"], 1);
        assert_eq!(chat["model"], "local-model");

        let responses = serde_json::from_slice::<Value>(
            &minimal_verification_body(UpstreamProtocolKind::Responses, "local-model").unwrap(),
        )
        .unwrap();
        assert_eq!(responses["stream"], false);
        assert_eq!(responses["max_output_tokens"], 1);

        let messages = serde_json::from_slice::<Value>(
            &minimal_verification_body(UpstreamProtocolKind::Messages, "local-model").unwrap(),
        )
        .unwrap();
        assert_eq!(messages["stream"], false);
        assert_eq!(messages["max_tokens"], 1);
    }

    #[test]
    fn model_discovery_page_uses_last_id_and_ignores_unsafe_data_ids() {
        let page = parse_model_discovery_page(
            br#"{"data":[{"id":"Model-A"},{"id":"  "},{"id":"model-b\n"}],"has_more":true,"last_id":"Model-A"}"#,
        )
        .unwrap();
        assert_eq!(page.models, vec!["Model-A"]);
        assert_eq!(page.cursor.as_deref(), Some("Model-A"));
        assert!(page.has_more);
    }

    #[test]
    fn model_discovery_cursor_replaces_query_and_rejects_loops() {
        let mut url = join_custom_endpoint("https://api.example.com/v1", "models").unwrap();
        let mut cursors = HashSet::new();
        advance_model_discovery_cursor(&mut url, &mut cursors, "first").unwrap();
        assert_eq!(
            url.as_str(),
            "https://api.example.com/v1/models?after_id=first"
        );
        advance_model_discovery_cursor(&mut url, &mut cursors, "second").unwrap();
        assert_eq!(
            url.as_str(),
            "https://api.example.com/v1/models?after_id=second"
        );
        assert!(advance_model_discovery_cursor(&mut url, &mut cursors, "SECOND").is_err());
    }

    #[test]
    fn malformed_model_discovery_shapes_are_actionable() {
        assert!(parse_model_discovery_page(br#"[]"#).is_err());
        assert!(parse_model_discovery_page(br#"{"data":{}}"#).is_err());
        assert!(parse_model_discovery_page(br#"{"data":[],"has_more":"yes"}"#).is_err());
        assert!(parse_model_discovery_page(br#"{"data":[],"last_id":42}"#).is_err());
    }

    #[test]
    fn model_discovery_headers_are_protocol_specific() {
        let chat = model_discovery_headers(UpstreamProtocolKind::ChatCompletions);
        assert_eq!(
            chat.get(reqwest::header::ACCEPT).unwrap(),
            "application/json"
        );
        assert!(chat.get("anthropic-version").is_none());

        let messages = model_discovery_headers(UpstreamProtocolKind::Messages);
        assert_eq!(messages.get("anthropic-version").unwrap(), "2023-06-01");
        assert!(messages.get(reqwest::header::AUTHORIZATION).is_none());
        assert!(messages.get("x-api-key").is_none());
    }

    #[test]
    fn model_discovery_response_body_limit_is_enforced() {
        assert!(model_discovery_body_size_allowed(MAX_CUSTOM_MODEL_DISCOVERY_BODY_BYTES).is_ok());
        assert!(
            model_discovery_body_size_allowed(MAX_CUSTOM_MODEL_DISCOVERY_BODY_BYTES + 1).is_err()
        );
    }

    #[test]
    fn model_discovery_timeout_is_shorter_than_the_general_request_timeout() {
        let mut config = AppConfig {
            non_stream_timeout_secs: 900,
            ..AppConfig::default()
        };
        assert_eq!(
            model_discovery_request_timeout(&config),
            Duration::from_secs(CUSTOM_MODEL_DISCOVERY_TIMEOUT_SECS)
        );

        config.non_stream_timeout_secs = 7;
        assert_eq!(
            model_discovery_request_timeout(&config),
            Duration::from_secs(7)
        );
    }

    #[test]
    fn only_2xx_json_object_proves_verified() {
        assert!(prove_verified_json_object(StatusCode::OK, br#"{"id":"ok"}"#).is_ok());
        assert!(prove_verified_json_object(StatusCode::CREATED, br#"{"ok":true}"#).is_ok());
        assert!(prove_verified_json_object(StatusCode::OK, b"[1]").is_err());
        assert!(prove_verified_json_object(StatusCode::OK, b"\"ok\"").is_err());
        assert!(prove_verified_json_object(StatusCode::OK, b"not-json").is_err());
        assert!(prove_verified_json_object(StatusCode::BAD_REQUEST, br#"{"error":"no"}"#).is_err());
        assert!(prove_verified_json_object(StatusCode::FOUND, br#"{"id":"ok"}"#).is_err());
    }

    #[test]
    fn verification_contract_identity_covers_revision_key_config_and_order() {
        let config = AccountCustomConfig {
            account_id: "acc".into(),
            base_url: "http://127.0.0.1:9".into(),
            upstream_protocol: UpstreamProtocolKind::Responses,
            auth_scheme: crate::provider::UpstreamAuthScheme::Bearer,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };
        let caps = vec![
            AccountModelCapability {
                account_id: "acc".into(),
                model_id: "one".into(),
                protocol: UpstreamProtocolKind::Responses,
                verified_at: None,
                source: "manual".into(),
            },
            AccountModelCapability {
                account_id: "acc".into(),
                model_id: "two".into(),
                protocol: UpstreamProtocolKind::Responses,
                verified_at: None,
                source: "manual".into(),
            },
        ];
        let contract =
            CustomVerificationContract::from_parts("acc", "rev-1", "cipher-a", &config, &caps);
        assert_eq!(contract.account_updated_at, "rev-1");
        assert_eq!(contract.key_cipher, "cipher-a");
        assert_eq!(
            contract.capabilities,
            vec![
                ("one".into(), UpstreamProtocolKind::Responses),
                ("two".into(), UpstreamProtocolKind::Responses)
            ]
        );
        let reordered = CustomVerificationContract::from_parts(
            "acc",
            "rev-1",
            "cipher-a",
            &config,
            &[caps[1].clone(), caps[0].clone()],
        );
        assert_ne!(contract, reordered);
        let rotated_key =
            CustomVerificationContract::from_parts("acc", "rev-1", "cipher-b", &config, &caps);
        assert_ne!(contract, rotated_key);
    }

    #[tokio::test]
    async fn oversized_verification_body_is_rejected_without_certifying() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let pad = "x".repeat(MAX_CUSTOM_VERIFICATION_BODY_BYTES);
        let body = format!(r#"{{"pad":"{pad}"}}"#);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("loopback listener");
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let Ok((mut stream, _)) = listener.accept().await else {
                return;
            };
            let mut buf = vec![0_u8; 8192];
            let _ = stream.read(&mut buf).await;
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = stream.write_all(response.as_bytes()).await;
        });

        let app_config = AppConfig {
            proxy_mode: crate::models::ProxyMode::Direct,
            connect_timeout_secs: 5,
            non_stream_timeout_secs: 5,
            ..AppConfig::default()
        };
        let custom_config = AccountCustomConfig {
            account_id: "acc".into(),
            base_url: format!("http://127.0.0.1:{}", addr.port()),
            upstream_protocol: UpstreamProtocolKind::ChatCompletions,
            auth_scheme: crate::provider::UpstreamAuthScheme::Bearer,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };
        let capability = AccountModelCapability {
            account_id: "acc".into(),
            model_id: "local".into(),
            protocol: UpstreamProtocolKind::ChatCompletions,
            verified_at: None,
            source: "manual".into(),
        };
        let error = probe_custom_connection(&app_config, &custom_config, &capability, "sk")
            .await
            .expect_err("oversized verification bodies must not prove verified");
        assert!(error.message.contains("exceeded"), "{}", error.message);
    }
}
