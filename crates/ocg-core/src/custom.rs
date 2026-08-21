//! Custom API runtime helpers: capability matching, verification probe, and
//! per-account route identity.
//!
//! Account model capabilities are the client-facing IDs and the exact upstream
//! IDs. Verification sends one protocol-correct non-stream request against the
//! first declared model. Discovery never mutates the declared list.

use crate::custom_http::{
    self, CustomHttpClient, CustomHttpError, join_custom_endpoint, json_content_headers,
};
use crate::gateway::protocol::ApiFormat;
use crate::models::{AccountCustomConfig, AccountModelCapability, AppConfig};
use crate::provider::ConnectionVerificationStatus;
use crate::provider::{
    CUSTOM_API_OFFERING_ID, CUSTOM_PROVIDER_ID, UpstreamAuthScheme, UpstreamProtocolKind,
    custom_endpoint_relative_path, is_custom_api,
};
use futures_util::StreamExt;
use reqwest::StatusCode;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::fmt;
use std::time::Duration;

/// Upper bound for a Custom verification response. The probe only needs a 2xx
/// JSON object; anything larger is rejected without certifying the account.
pub const MAX_CUSTOM_VERIFICATION_BODY_BYTES: usize = 64 * 1024;

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

/// Match a client-requested name against a declared Custom capability ID.
///
/// Raw-shaped IDs (`/`, `_`, whitespace) never fold separators onto kebab
/// aliases. Otherwise matching is case-insensitive like published aliases.
pub fn custom_model_id_matches(declared: &str, requested: &str) -> bool {
    let declared = declared.trim();
    let requested = requested.trim();
    if declared.is_empty() || requested.is_empty() {
        return false;
    }
    if declared == requested {
        return true;
    }
    if looks_raw_shaped(declared) || looks_raw_shaped(requested) {
        return declared.eq_ignore_ascii_case(requested);
    }
    declared.eq_ignore_ascii_case(requested)
}

fn looks_raw_shaped(name: &str) -> bool {
    name.chars()
        .any(|ch| ch == '/' || ch == '_' || ch.is_whitespace())
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
    if let Some(length) = response.content_length()
        && length > MAX_CUSTOM_VERIFICATION_BODY_BYTES as u64
    {
        return Err(oversized_verification_body());
    }
    let mut body = Vec::new();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|error| CustomVerifyFailure {
            message: format!("Custom verification response body failed: {error}"),
        })?;
        if body.len().saturating_add(chunk.len()) > MAX_CUSTOM_VERIFICATION_BODY_BYTES {
            return Err(oversized_verification_body());
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
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
