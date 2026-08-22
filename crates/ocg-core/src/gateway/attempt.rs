//! Transport-boundary description of one upstream inference attempt.
//!
//! [`AttemptSpec`] is data only: endpoint/base URL, path, upstream protocol,
//! auth scheme, redirect policy, an opaque credential handle, and the
//! proxy-routing model. Provider adapters produce this value. They never
//! receive Host state, a database handle, or an HTTP client, and they never
//! see plaintext credentials.
//!
//! [`CredentialResolver`] is the Host-side seam. The single-attempt executor
//! resolves the handle, constructs the authorization header, and selects the
//! outbound client from [`ProxyRoutingModel`]. This slice does not rewrite
//! the outer fallback loop.
//!
//! [`AttemptTimeouts`] and [`AttemptTransportError`] describe the single POST
//! boundary. `forward_once` in `forwarder` performs exactly one `.send()` and
//! owns only transport selection and those timeouts.
//!
//! The temporary process-host resolver and `DbAttemptSink` live in `forwarder`
//! because `state.rs` / `db.rs` are outside this lease. A later host slice
//! should move the concrete resolver next to `KeyHost`.

use crate::kernel::protocol::ApiFormat;
use std::time::Duration;

/// Authentication belongs to the provider/offering adapter, not to the wire
/// protocol. In particular, a Messages endpoint does not imply `x-api-key`
/// for every future provider.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum UpstreamAuth {
    OpenCodeProtocolDefault,
    Bearer,
    XApiKey,
    None,
}

/// How the single-attempt executor selects an outbound client and URL/header
/// policy. This replaces `provider_id` / `custom_route` branches in the
/// forwarder send path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProxyRoutingModel {
    /// Frozen request-entry `ForwardRouteSet` snapshot (OpenCode Go / Zen Free).
    /// Follows redirects. Restricted URL (https or loopback http). Forwards
    /// harmless client headers.
    RequestEntrySnapshot,
    /// Process-wide default-leg client with redirects disabled (GOAT loopback).
    /// Restricted URL. Forwards harmless client headers.
    ProcessWideNoRedirect,
    /// Custom trusted-admin isolated client: process-wide proxy, no redirects,
    /// no client-header forwarding, administrator-trusted URL.
    IsolatedTrustedAdmin,
}

/// Opaque credential identity. Adapters store only an account id (or none);
/// plaintext is resolved by [`CredentialResolver`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum CredentialHandle {
    /// Anonymous / keyless route (Zen Free). Host must not decrypt.
    None,
    /// Host decrypts this account's stored key. Never contains plaintext.
    Account { id: String },
}

impl CredentialHandle {
    pub(crate) fn account_id(&self) -> Option<&str> {
        match self {
            Self::None => None,
            Self::Account { id } => Some(id.as_str()),
        }
    }
}

/// Data-only description of one upstream inference attempt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AttemptSpec {
    pub base_url: String,
    pub path: String,
    pub upstream: ApiFormat,
    pub auth: UpstreamAuth,
    /// Redirect policy: OpenCode Go / Zen follow redirects; GOAT and Custom
    /// trusted-admin do not. The selected routing model must enforce the same
    /// policy; this field keeps that transport contract explicit and testable.
    pub follow_redirects: bool,
    pub credential: CredentialHandle,
    pub proxy_routing: ProxyRoutingModel,
}

impl AttemptSpec {
    pub(crate) fn credential_account_id(&self) -> Option<&str> {
        self.credential.account_id()
    }

    /// Restricted OpenCode/GOAT URL check: https or loopback http. Custom
    /// trusted-admin destinations skip this.
    pub(crate) fn restricted_upstream_url(&self) -> bool {
        !matches!(self.proxy_routing, ProxyRoutingModel::IsolatedTrustedAdmin)
    }

    pub(crate) fn isolates_client_headers(&self) -> bool {
        matches!(self.proxy_routing, ProxyRoutingModel::IsolatedTrustedAdmin)
    }

    /// Wire auth after OpenCode protocol-default mapping. Messages uses
    /// `x-api-key`; other OpenCode defaults use Bearer.
    pub(crate) fn wire_auth(&self) -> UpstreamAuth {
        match self.auth {
            UpstreamAuth::OpenCodeProtocolDefault if self.upstream == ApiFormat::Messages => {
                UpstreamAuth::XApiKey
            }
            UpstreamAuth::OpenCodeProtocolDefault => UpstreamAuth::Bearer,
            auth => auth,
        }
    }

    pub(crate) fn request_url(&self) -> Result<String, String> {
        let path = if self.path.is_empty() {
            self.upstream
                .upstream_path()
                .ok_or_else(|| "Gemini is a client-only protocol".to_string())?
                .to_string()
        } else {
            self.path.clone()
        };
        Ok(format!("{}{}", self.base_url.trim_end_matches('/'), path))
    }
}

/// Timeouts applied at the single-POST boundary. Non-stream uses reqwest's
/// per-request timeout; stream wraps `.send()` with a header-wait timeout.
/// Body idle timeouts, SSE conversion, and retry stay outside `forward_once`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct AttemptTimeouts {
    pub non_stream: Duration,
    pub stream_header: Duration,
}

impl AttemptTimeouts {
    pub(crate) fn from_secs(non_stream: u64, stream_header: u64) -> Self {
        Self {
            non_stream: Duration::from_secs(non_stream),
            stream_header: Duration::from_secs(stream_header),
        }
    }
}

/// Errors from the single upstream POST. Classification, logging, cooldown,
/// CAS, usage scheduling, and retry stay in the caller.
#[derive(Debug)]
pub(crate) enum AttemptTransportError {
    HeaderTimeout { timeout: Duration },
    Send(reqwest::Error),
}

impl std::fmt::Display for AttemptTransportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::HeaderTimeout { timeout } => write!(
                f,
                "upstream did not return response headers within {}s",
                timeout.as_secs()
            ),
            Self::Send(error) if error.is_timeout() => {
                write!(f, "upstream request timed out: {error}")
            }
            Self::Send(error) => write!(f, "upstream request failed: {error}"),
        }
    }
}

impl std::error::Error for AttemptTransportError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::HeaderTimeout { .. } => None,
            Self::Send(error) => Some(error),
        }
    }
}

/// Host-side seam: decrypts an opaque [`CredentialHandle`]. Provider adapters
/// never receive this trait or the resulting plaintext. The single-attempt
/// executor constructs the authorization header from the resolved secret and
/// [`AttemptSpec::wire_auth`].
pub(crate) trait CredentialResolver {
    fn resolve_credential(
        &self,
        handle: &CredentialHandle,
    ) -> Result<Option<String>, CredentialResolveError>;
}

#[derive(Debug)]
pub(crate) enum CredentialResolveError {
    Decrypt(anyhow::Error),
    HandleMismatch { expected: String, actual: String },
}

impl std::fmt::Display for CredentialResolveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Decrypt(error) => write!(f, "{error}"),
            Self::HandleMismatch { expected, actual } => write!(
                f,
                "credential handle `{actual}` does not match selected account `{expected}`"
            ),
        }
    }
}

impl std::error::Error for CredentialResolveError {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    struct MapResolver(HashMap<String, String>);

    impl CredentialResolver for MapResolver {
        fn resolve_credential(
            &self,
            handle: &CredentialHandle,
        ) -> Result<Option<String>, CredentialResolveError> {
            match handle.account_id() {
                None => Ok(None),
                Some(id) => Ok(self.0.get(id).cloned()),
            }
        }
    }

    fn spec(
        auth: UpstreamAuth,
        upstream: ApiFormat,
        follow_redirects: bool,
        credential: CredentialHandle,
        proxy_routing: ProxyRoutingModel,
    ) -> AttemptSpec {
        AttemptSpec {
            base_url: "https://opencode.ai/zen/go".into(),
            path: "/v1/chat/completions".into(),
            upstream,
            auth,
            follow_redirects,
            credential,
            proxy_routing,
        }
    }

    #[test]
    fn attempt_spec_is_data_only_and_describes_the_transport_boundary() {
        let spec = spec(
            UpstreamAuth::OpenCodeProtocolDefault,
            ApiFormat::ChatCompletions,
            true,
            CredentialHandle::Account { id: "go-1".into() },
            ProxyRoutingModel::RequestEntrySnapshot,
        );
        assert_eq!(spec.base_url, "https://opencode.ai/zen/go");
        assert_eq!(spec.path, "/v1/chat/completions");
        assert_eq!(spec.upstream, ApiFormat::ChatCompletions);
        assert_eq!(spec.auth, UpstreamAuth::OpenCodeProtocolDefault);
        assert!(spec.follow_redirects);
        assert_eq!(spec.credential_account_id(), Some("go-1"));
        assert_eq!(spec.proxy_routing, ProxyRoutingModel::RequestEntrySnapshot);
        assert!(spec.restricted_upstream_url());
        assert!(!spec.isolates_client_headers());
        assert_eq!(
            spec.request_url().unwrap(),
            "https://opencode.ai/zen/go/v1/chat/completions"
        );
        let debug = format!("{spec:?}");
        assert!(debug.contains("go-1"));
        assert!(!debug.contains("sk-"));
    }

    #[test]
    fn attempt_spec_production_source_does_not_name_host_state() {
        let source = include_str!("attempt.rs");
        let production = source
            .split("#[cfg(test)]")
            .next()
            .expect("production source precedes tests");
        assert!(
            !production.contains("CoreState"),
            "AttemptSpec must not name CoreState"
        );
        assert!(
            !production.contains("Database"),
            "AttemptSpec must not name Database"
        );
        assert!(
            !production.contains("reqwest::Client"),
            "AttemptSpec must not name reqwest::Client"
        );
    }

    #[test]
    fn wire_auth_maps_opencode_protocol_default_without_provider_id() {
        let chat = spec(
            UpstreamAuth::OpenCodeProtocolDefault,
            ApiFormat::ChatCompletions,
            true,
            CredentialHandle::Account { id: "go-1".into() },
            ProxyRoutingModel::RequestEntrySnapshot,
        );
        assert_eq!(chat.wire_auth(), UpstreamAuth::Bearer);
        let messages = AttemptSpec {
            path: "/v1/messages".into(),
            upstream: ApiFormat::Messages,
            ..chat.clone()
        };
        assert_eq!(messages.wire_auth(), UpstreamAuth::XApiKey);
        let custom = spec(
            UpstreamAuth::XApiKey,
            ApiFormat::ChatCompletions,
            false,
            CredentialHandle::Account {
                id: "custom-1".into(),
            },
            ProxyRoutingModel::IsolatedTrustedAdmin,
        );
        assert_eq!(custom.wire_auth(), UpstreamAuth::XApiKey);
        assert!(!custom.restricted_upstream_url());
        assert!(custom.isolates_client_headers());
        assert!(!custom.follow_redirects);
        let goat = spec(
            UpstreamAuth::Bearer,
            ApiFormat::ChatCompletions,
            false,
            CredentialHandle::Account {
                id: "goat-1".into(),
            },
            ProxyRoutingModel::ProcessWideNoRedirect,
        );
        assert!(goat.restricted_upstream_url());
        assert!(!goat.isolates_client_headers());
        let zen = spec(
            UpstreamAuth::None,
            ApiFormat::ChatCompletions,
            true,
            CredentialHandle::None,
            ProxyRoutingModel::RequestEntrySnapshot,
        );
        assert_eq!(zen.wire_auth(), UpstreamAuth::None);
        assert!(zen.credential_account_id().is_none());
    }

    #[test]
    fn empty_path_uses_upstream_protocol_path() {
        let spec = AttemptSpec {
            path: String::new(),
            ..spec(
                UpstreamAuth::Bearer,
                ApiFormat::Responses,
                true,
                CredentialHandle::None,
                ProxyRoutingModel::RequestEntrySnapshot,
            )
        };
        assert_eq!(
            spec.request_url().unwrap(),
            "https://opencode.ai/zen/go/v1/responses"
        );
        let gemini = AttemptSpec {
            path: String::new(),
            upstream: ApiFormat::Gemini,
            ..spec
        };
        assert!(
            gemini
                .request_url()
                .unwrap_err()
                .contains("Gemini is a client-only protocol")
        );
    }

    #[test]
    fn credential_resolver_seam_decrypts_handles_not_adapter_secrets() {
        let mut secrets = HashMap::new();
        secrets.insert("go-1".into(), "sk-live-secret".into());
        let resolver = MapResolver(secrets);
        assert_eq!(
            resolver
                .resolve_credential(&CredentialHandle::Account { id: "go-1".into() })
                .unwrap()
                .as_deref(),
            Some("sk-live-secret")
        );
        assert_eq!(
            resolver
                .resolve_credential(&CredentialHandle::None)
                .unwrap(),
            None
        );
        let handle = CredentialHandle::Account { id: "go-1".into() };
        assert!(!format!("{handle:?}").contains("sk-live-secret"));
    }

    #[test]
    fn attempt_timeouts_are_transport_durations_only() {
        let timeouts = AttemptTimeouts::from_secs(900, 300);
        assert_eq!(timeouts.non_stream, Duration::from_secs(900));
        assert_eq!(timeouts.stream_header, Duration::from_secs(300));
        assert_ne!(timeouts.non_stream, timeouts.stream_header);
    }

    #[test]
    fn attempt_transport_error_messages_match_stage0_send_text() {
        let header = AttemptTransportError::HeaderTimeout {
            timeout: Duration::from_secs(300),
        };
        assert_eq!(
            header.to_string(),
            "upstream did not return response headers within 300s"
        );
        assert!(matches!(
            header,
            AttemptTransportError::HeaderTimeout { .. }
        ));
    }
}
