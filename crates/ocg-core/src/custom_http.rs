//! Dedicated Custom API HTTP client and reusable inference HTTP primitives.
//!
//! Custom destinations are administrator-trusted. Direct, Manual, and Auto all
//! inherit the process-wide proxy policy from [`crate::http_client`]. The
//! client never follows redirects, never forwards dashboard/client auth, and
//! always composes isolated Bearer / `x-api-key` headers.
//!
//! Catalog-free transport mechanics live in [`ocg_infra::inference_http`]. This
//! module maps [`AppConfig`] through [`crate::http_client::outbound_proxy_spec`]
//! and [`UpstreamAuthScheme`] onto that transport. Custom product policy stays
//! here: URL trust, 5-60s connect clamp, isolated send gating, and JSON /
//! forbidden-header helpers.

use crate::models::{AppConfig, ProxyMode};
use crate::provider::UpstreamAuthScheme;
use crate::provider::validate_custom_base_url;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use std::fmt;
use std::time::Duration;

pub use ocg_infra::inference_http::{
    InferenceHttpError, InferenceRedirectPolicy, apply_inference_request_timeout,
    join_inference_endpoint,
};

/// Build isolated upstream auth headers. Callers supply the configured scheme
/// and key; this never copies dashboard or client credentials.
pub fn isolated_inference_headers(
    scheme: UpstreamAuthScheme,
    api_key: &str,
) -> Result<HeaderMap, InferenceHttpError> {
    ocg_infra::inference_http::isolated_inference_headers(inference_auth_scheme(scheme), api_key)
}

/// Connect timeout for the provider-neutral inference HTTP adapter.
pub fn inference_connect_timeout(config: &AppConfig) -> Duration {
    Duration::from_secs(config.connect_timeout_secs)
}

/// Custom verification and forwarding bound connection setup independently of
/// the provider-neutral transport's process-wide timeout setting.
fn custom_connect_timeout(config: &AppConfig) -> Duration {
    Duration::from_secs(config.connect_timeout_secs.clamp(5, 60))
}

fn inference_auth_scheme(
    scheme: UpstreamAuthScheme,
) -> ocg_infra::inference_http::InferenceAuthScheme {
    match scheme {
        UpstreamAuthScheme::Bearer => ocg_infra::inference_http::InferenceAuthScheme::Bearer,
        UpstreamAuthScheme::XApiKey => ocg_infra::inference_http::InferenceAuthScheme::XApiKey,
    }
}

fn core_proxy_mode(mode: ocg_infra::http::ProxyMode) -> ProxyMode {
    match mode {
        ocg_infra::http::ProxyMode::Auto => ProxyMode::Auto,
        ocg_infra::http::ProxyMode::Manual => ProxyMode::Manual,
        ocg_infra::http::ProxyMode::Direct => ProxyMode::Direct,
        ocg_infra::http::ProxyMode::List => ProxyMode::List,
    }
}

/// Construction spec for a provider-neutral inference HTTP client.
/// Redirect policy is chosen by the owning adapter; proxy/default routing and
/// connect timeout come from process-wide [`AppConfig`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HttpInferenceTransportSpec {
    pub redirect: InferenceRedirectPolicy,
}

impl HttpInferenceTransportSpec {
    pub const fn follow_redirects() -> Self {
        Self {
            redirect: InferenceRedirectPolicy::Follow,
        }
    }

    pub const fn no_redirects() -> Self {
        Self {
            redirect: InferenceRedirectPolicy::None,
        }
    }

    fn to_infra(self) -> ocg_infra::inference_http::HttpInferenceTransportSpec {
        ocg_infra::inference_http::HttpInferenceTransportSpec::new(self.redirect)
    }

    fn from_infra(spec: ocg_infra::inference_http::HttpInferenceTransportSpec) -> Self {
        Self {
            redirect: spec.redirect(),
        }
    }
}

/// One outbound inference attempt. Auth is optional so keyless adapters can
/// reuse the same send path; callers that need isolated Bearer / `x-api-key`
/// supply the scheme and key here.
#[derive(Debug)]
pub struct InferenceHttpRequest<'a> {
    pub method: reqwest::Method,
    pub url: reqwest::Url,
    pub auth: Option<(UpstreamAuthScheme, &'a str)>,
    pub extra_headers: HeaderMap,
    pub body: Option<Vec<u8>>,
    pub request_timeout: Option<Duration>,
}

/// Neutral inference HTTP wrapper around [`ocg_infra::inference_http`].
/// Provider policy (Custom URL trust, permitted auth, redirect prohibition,
/// endpoint prefix isolation, verify lifecycle) stays in the owning adapter.
#[derive(Clone)]
pub struct HttpInferenceTransport {
    inner: ocg_infra::inference_http::HttpInferenceTransport,
}

impl fmt::Debug for HttpInferenceTransport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HttpInferenceTransport")
            .field("proxy_mode", &self.proxy_mode())
            .field("redirect", &self.spec().redirect)
            .finish_non_exhaustive()
    }
}

impl HttpInferenceTransport {
    pub fn build(
        config: &AppConfig,
        spec: HttpInferenceTransportSpec,
    ) -> Result<Self, InferenceHttpError> {
        Self::build_with_connect_timeout(config, spec, Self::connect_timeout(config))
    }

    fn build_with_connect_timeout(
        config: &AppConfig,
        spec: HttpInferenceTransportSpec,
        connect_timeout: Duration,
    ) -> Result<Self, InferenceHttpError> {
        let mut proxy = crate::http_client::outbound_proxy_spec(config);
        proxy.connect_timeout = connect_timeout;
        Ok(Self {
            inner: ocg_infra::inference_http::HttpInferenceTransport::build(
                &proxy,
                spec.to_infra(),
            )?,
        })
    }

    pub fn spec(&self) -> HttpInferenceTransportSpec {
        HttpInferenceTransportSpec::from_infra(self.inner.spec())
    }

    pub fn proxy_mode(&self) -> ProxyMode {
        core_proxy_mode(self.inner.proxy_mode())
    }

    pub fn redirect_policy(&self) -> InferenceRedirectPolicy {
        self.inner.redirect_policy()
    }

    pub fn connect_timeout(config: &AppConfig) -> Duration {
        inference_connect_timeout(config)
    }

    pub fn join_endpoint(base_url: &str, path: &str) -> Result<reqwest::Url, InferenceHttpError> {
        ocg_infra::inference_http::HttpInferenceTransport::join_endpoint(base_url, path)
    }

    pub fn isolated_headers(
        scheme: UpstreamAuthScheme,
        api_key: &str,
    ) -> Result<HeaderMap, InferenceHttpError> {
        isolated_inference_headers(scheme, api_key)
    }

    pub(crate) fn request(
        &self,
        method: reqwest::Method,
        url: reqwest::Url,
    ) -> reqwest::RequestBuilder {
        self.inner.request(method, url)
    }

    pub async fn send(
        &self,
        request: InferenceHttpRequest<'_>,
    ) -> Result<reqwest::Response, InferenceHttpError> {
        self.inner
            .send(ocg_infra::inference_http::InferenceHttpRequest::new(
                request.method,
                request.url,
                request
                    .auth
                    .map(|(scheme, key)| (inference_auth_scheme(scheme), key)),
                request.extra_headers,
                request.body,
                request.request_timeout,
            ))
            .await
    }

    pub async fn read_body_limited(
        response: reqwest::Response,
        max_bytes: usize,
    ) -> Result<Vec<u8>, InferenceHttpError> {
        ocg_infra::inference_http::HttpInferenceTransport::read_body_limited(response, max_bytes)
            .await
    }
}

#[derive(Clone)]
pub struct CustomHttpClient {
    transport: HttpInferenceTransport,
}

impl fmt::Debug for CustomHttpClient {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CustomHttpClient")
            .field("proxy_mode", &self.transport.proxy_mode())
            .finish_non_exhaustive()
    }
}

impl CustomHttpClient {
    pub fn proxy_mode(&self) -> ProxyMode {
        self.transport.proxy_mode()
    }

    pub(crate) fn request(
        &self,
        method: reqwest::Method,
        url: reqwest::Url,
    ) -> reqwest::RequestBuilder {
        self.transport.request(method, url)
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn send_isolated(
        &self,
        method: reqwest::Method,
        url: reqwest::Url,
        scheme: UpstreamAuthScheme,
        api_key: &str,
        extra_headers: HeaderMap,
        body: Option<Vec<u8>>,
        request_timeout: Option<Duration>,
    ) -> Result<reqwest::Response, CustomHttpError> {
        if header_map_contains_forbidden_client_credentials(&extra_headers, scheme) {
            return Err(CustomHttpError::InvalidUrl(
                "Custom upstream request must not forward dashboard or client credentials"
                    .to_string(),
            ));
        }
        self.transport
            .send(InferenceHttpRequest {
                method,
                url,
                auth: Some((scheme, api_key)),
                extra_headers,
                body,
                request_timeout,
            })
            .await
            .map_err(CustomHttpError::from)
    }
}

pub fn build_custom_http_client(config: &AppConfig) -> Result<CustomHttpClient, CustomHttpError> {
    // Connect timeout only. Non-stream callers apply `non_stream_timeout_secs`
    // per request; streaming must be able to outlive that total duration.
    // Custom keeps redirect prohibition on this wrapper; the transport can
    // follow redirects when another adapter selects that spec.
    Ok(CustomHttpClient {
        transport: HttpInferenceTransport::build_with_connect_timeout(
            config,
            HttpInferenceTransportSpec::no_redirects(),
            custom_connect_timeout(config),
        )?,
    })
}

/// Join `path` onto a persisted Custom base URL while keeping the origin and
/// path prefix. Absolute URLs, protocol-relative targets, decoded dot-segments,
/// encoded slash/backslash, and nested percent-encoding are rejected as
/// endpoint override.
pub fn join_custom_endpoint(base_url: &str, path: &str) -> Result<reqwest::Url, CustomHttpError> {
    let canonical = validate_custom_base_url(base_url).map_err(CustomHttpError::from)?;
    join_inference_endpoint(&canonical, path).map_err(CustomHttpError::from)
}

const FORBIDDEN_CLIENT_HEADERS: &[&str] = &[
    "cookie",
    "set-cookie",
    "authorization",
    "proxy-authorization",
    "x-api-key",
    "x-goog-api-key",
    "x-ocg-session",
];

/// Build Custom upstream auth headers. Callers cannot supply inbound client or
/// dashboard headers; [`CustomHttpClient::send_isolated`] is the only send
/// path and always composes this map first.
pub fn isolated_custom_headers(
    scheme: UpstreamAuthScheme,
    api_key: &str,
) -> Result<HeaderMap, CustomHttpError> {
    isolated_inference_headers(scheme, api_key).map_err(CustomHttpError::from)
}

pub fn json_content_headers(include_anthropic_version: bool) -> Result<HeaderMap, CustomHttpError> {
    let mut headers = HeaderMap::new();
    headers.insert(
        reqwest::header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    headers.insert(
        reqwest::header::ACCEPT,
        HeaderValue::from_static("application/json"),
    );
    if include_anthropic_version {
        headers.insert(
            HeaderName::from_static("anthropic-version"),
            HeaderValue::from_static("2023-06-01"),
        );
    }
    Ok(headers)
}

pub fn forbidden_forwarded_header_names() -> &'static [&'static str] {
    FORBIDDEN_CLIENT_HEADERS
}

pub fn header_map_contains_forbidden_client_credentials(
    headers: &HeaderMap,
    scheme: UpstreamAuthScheme,
) -> bool {
    headers.keys().any(|name| {
        let lower = name.as_str();
        match scheme {
            UpstreamAuthScheme::Bearer if lower == "authorization" => false,
            UpstreamAuthScheme::XApiKey if lower == "x-api-key" => false,
            _ => FORBIDDEN_CLIENT_HEADERS.contains(&lower),
        }
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CustomHttpError {
    InvalidUrl(String),
    EndpointOverride(String),
    Build(String),
    Network(String),
}

impl fmt::Display for CustomHttpError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidUrl(message)
            | Self::EndpointOverride(message)
            | Self::Build(message)
            | Self::Network(message) => f.write_str(message),
        }
    }
}

impl std::error::Error for CustomHttpError {}

impl From<crate::provider::ProviderBindingError> for CustomHttpError {
    fn from(error: crate::provider::ProviderBindingError) -> Self {
        Self::InvalidUrl(error.to_string())
    }
}

impl From<InferenceHttpError> for CustomHttpError {
    fn from(error: InferenceHttpError) -> Self {
        match error {
            InferenceHttpError::InvalidUrl(message) => Self::InvalidUrl(message),
            InferenceHttpError::EndpointOverride(message) => Self::EndpointOverride(message),
            InferenceHttpError::Build(message) => Self::Build(message),
            InferenceHttpError::Network(message) => Self::Network(message),
            InferenceHttpError::Oversize { limit } => {
                Self::Network(format!("response exceeded the {limit}-byte limit"))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::AppConfig;
    use reqwest::StatusCode;
    use reqwest::header::AUTHORIZATION;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    fn test_config(mode: ProxyMode, proxy_url: &str) -> AppConfig {
        AppConfig {
            proxy_mode: mode,
            proxy_url: proxy_url.to_string(),
            connect_timeout_secs: 5,
            ..AppConfig::default()
        }
    }

    async fn send_get(
        client: &CustomHttpClient,
        url: reqwest::Url,
    ) -> Result<reqwest::Response, CustomHttpError> {
        client
            .send_isolated(
                reqwest::Method::GET,
                url,
                UpstreamAuthScheme::Bearer,
                "test-key",
                HeaderMap::new(),
                None,
                None,
            )
            .await
    }

    async fn serve_http(
        status: u16,
        reason: &str,
        headers: &[(&str, String)],
        body: &str,
        hits: Arc<AtomicUsize>,
    ) -> std::net::SocketAddr {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("loopback listener");
        let addr = listener.local_addr().unwrap();
        let reason = reason.to_string();
        let body = body.to_string();
        let headers = headers
            .iter()
            .map(|(name, value)| (name.to_string(), value.clone()))
            .collect::<Vec<_>>();
        tokio::spawn(async move {
            loop {
                let Ok((mut stream, _)) = listener.accept().await else {
                    break;
                };
                hits.fetch_add(1, Ordering::SeqCst);
                let mut buf = vec![0_u8; 4096];
                let _ = stream.read(&mut buf).await;
                let mut response = format!("HTTP/1.1 {status} {reason}\r\n");
                for (name, value) in &headers {
                    response.push_str(&format!("{name}: {value}\r\n"));
                }
                response.push_str(&format!(
                    "Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                ));
                let _ = stream.write_all(response.as_bytes()).await;
            }
        });
        addr
    }

    async fn serve_counting_proxy(hits: Arc<AtomicUsize>) -> std::net::SocketAddr {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("proxy listener");
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            loop {
                let Ok((mut stream, _)) = listener.accept().await else {
                    break;
                };
                hits.fetch_add(1, Ordering::SeqCst);
                let mut buf = vec![0_u8; 4096];
                let _ = stream.read(&mut buf).await;
                let body = "proxy";
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = stream.write_all(response.as_bytes()).await;
            }
        });
        addr
    }

    #[test]
    fn custom_http_builds_for_direct_manual_and_auto() {
        assert!(build_custom_http_client(&test_config(ProxyMode::Direct, "")).is_ok());
        assert!(
            build_custom_http_client(&test_config(ProxyMode::Manual, "http://127.0.0.1:8080"))
                .is_ok()
        );
        let auto = build_custom_http_client(&test_config(ProxyMode::Auto, "")).unwrap();
        assert_eq!(auto.proxy_mode(), ProxyMode::Auto);
    }

    #[test]
    fn join_custom_endpoint_preserves_prefix_and_rejects_override() {
        let joined =
            join_custom_endpoint("https://api.example.com/v1", "chat/completions").unwrap();
        assert_eq!(
            joined.as_str(),
            "https://api.example.com/v1/chat/completions"
        );
        let absolute_slash =
            join_custom_endpoint("https://api.example.com/v1", "/chat/completions").unwrap();
        assert_eq!(
            absolute_slash.as_str(),
            "https://api.example.com/v1/chat/completions"
        );
        assert_eq!(
            join_custom_endpoint("https://api.example.com", "v1/models")
                .unwrap()
                .as_str(),
            "https://api.example.com/v1/models"
        );
        assert_eq!(
            join_custom_endpoint("http://127.0.0.1:9/v1", "messages")
                .unwrap()
                .as_str(),
            "http://127.0.0.1:9/v1/messages"
        );
        assert_eq!(
            join_custom_endpoint("http://10.0.0.8/prefix", "responses")
                .unwrap()
                .as_str(),
            "http://10.0.0.8/prefix/responses"
        );
        assert!(
            join_custom_endpoint("https://api.example.com/v1", "https://evil.example/x").is_err()
        );
        assert!(join_custom_endpoint("https://api.example.com/v1", "//evil.example/x").is_err());
        assert!(join_custom_endpoint("https://api.example.com/v1", "../admin").is_err());
        assert!(join_custom_endpoint("https://api.example.com/v1", "foo/../admin").is_err());
        assert!(join_custom_endpoint("https://api.example.com/v1", "foo/./bar").is_err());
        assert!(join_custom_endpoint("https://api.example.com/v1", "%2e%2e/admin").is_err());
        assert!(join_custom_endpoint("https://api.example.com/v1", "foo/%2e%2e/admin").is_err());
        assert!(join_custom_endpoint("https://api.example.com/v1", "foo%2fadmin").is_err());
        assert!(join_custom_endpoint("https://api.example.com/v1", "foo%2Fadmin").is_err());
        assert!(join_custom_endpoint("https://api.example.com/v1", "foo%5cadmin").is_err());
        assert!(join_custom_endpoint("https://api.example.com/v1", "foo%5Cadmin").is_err());
        assert!(join_custom_endpoint("https://api.example.com/v1", "%252e%252e/admin").is_err());
        assert!(
            join_custom_endpoint("https://api.example.com/v1", "foo/%252E%252E/admin").is_err()
        );
        assert!(
            join_custom_endpoint("https://api.example.com/v1", "%252f%252fevil.example/x").is_err()
        );
        assert!(join_custom_endpoint("https://api.example.com/v1", "foo%255cadmin").is_err());
        assert!(
            join_custom_endpoint("https://api.example.com/v1", "%25252e%25252e/admin").is_err()
        );
        assert!(join_custom_endpoint("https://api.example.com/v1", "nested%2520space").is_err());
        assert_eq!(
            join_custom_endpoint("https://api.example.com/v1", "hello%20world")
                .unwrap()
                .as_str(),
            "https://api.example.com/v1/hello%20world"
        );
    }

    #[test]
    fn http_inference_transport_is_policy_neutral_and_owns_join_auth_and_timeout() {
        let spec_none = HttpInferenceTransportSpec::no_redirects();
        let spec_follow = HttpInferenceTransportSpec::follow_redirects();
        assert_eq!(spec_none.redirect, InferenceRedirectPolicy::None);
        assert_eq!(spec_follow.redirect, InferenceRedirectPolicy::Follow);
        let direct =
            HttpInferenceTransport::build(&test_config(ProxyMode::Direct, ""), spec_none).unwrap();
        assert_eq!(direct.proxy_mode(), ProxyMode::Direct);
        assert_eq!(direct.redirect_policy(), InferenceRedirectPolicy::None);
        assert_eq!(direct.spec(), spec_none);
        let auto =
            HttpInferenceTransport::build(&test_config(ProxyMode::Auto, ""), spec_follow).unwrap();
        assert_eq!(auto.proxy_mode(), ProxyMode::Auto);
        assert_eq!(auto.redirect_policy(), InferenceRedirectPolicy::Follow);
        assert!(
            HttpInferenceTransport::build(
                &test_config(ProxyMode::Manual, "http://127.0.0.1:8080"),
                spec_none,
            )
            .is_ok()
        );

        let joined = HttpInferenceTransport::join_endpoint(
            crate::provider::COMMAND_CODE_GOAT_BASE_URL,
            crate::provider::COMMAND_CODE_GOAT_CHAT_COMPLETIONS_PATH,
        )
        .unwrap();
        assert_eq!(
            joined.as_str(),
            "https://api.commandcode.ai/provider/v1/chat/completions"
        );
        let with_userinfo = HttpInferenceTransport::join_endpoint(
            "https://user:pass@api.example.com/v1",
            "chat/completions",
        );
        assert!(
            with_userinfo.is_ok(),
            "neutral join must not apply Custom URL trust validation"
        );
        assert!(
            join_custom_endpoint("https://user:pass@api.example.com/v1", "chat/completions")
                .is_err(),
            "Custom join keeps credential rejection"
        );

        let bearer =
            HttpInferenceTransport::isolated_headers(UpstreamAuthScheme::Bearer, "sk-test")
                .unwrap();
        assert_eq!(bearer.get(AUTHORIZATION).unwrap(), "Bearer sk-test");
        assert_eq!(
            HttpInferenceTransport::connect_timeout(&test_config(ProxyMode::Direct, "")),
            Duration::from_secs(5)
        );
        assert_eq!(
            HttpInferenceTransport::connect_timeout(&test_config(ProxyMode::Direct, "")),
            inference_connect_timeout(&test_config(ProxyMode::Direct, ""))
        );
    }

    #[test]
    fn inference_http_primitives_join_auth_timeout_and_redirect_without_custom_policy() {
        let goat = join_inference_endpoint(
            crate::provider::COMMAND_CODE_GOAT_BASE_URL,
            crate::provider::COMMAND_CODE_GOAT_CHAT_COMPLETIONS_PATH,
        )
        .unwrap();
        assert_eq!(
            goat.as_str(),
            "https://api.commandcode.ai/provider/v1/chat/completions"
        );
        let messages = join_inference_endpoint(
            "http://127.0.0.1:9/provider/v1",
            crate::provider::COMMAND_CODE_GOAT_MESSAGES_PATH,
        )
        .unwrap();
        assert_eq!(messages.as_str(), "http://127.0.0.1:9/provider/v1/messages");
        assert!(
            join_inference_endpoint("https://api.commandcode.ai/provider/v1", "../admin").is_err()
        );
        let bearer = isolated_inference_headers(UpstreamAuthScheme::Bearer, "sk-test").unwrap();
        assert_eq!(bearer.get(AUTHORIZATION).unwrap(), "Bearer sk-test");
        let custom = isolated_custom_headers(UpstreamAuthScheme::Bearer, "sk-test").unwrap();
        assert_eq!(bearer, custom);
        assert_eq!(InferenceRedirectPolicy::None, InferenceRedirectPolicy::None);
        assert_ne!(
            InferenceRedirectPolicy::Follow,
            InferenceRedirectPolicy::None
        );
        let _none = InferenceRedirectPolicy::None.reqwest_policy();
        assert_eq!(
            inference_connect_timeout(&test_config(ProxyMode::Direct, "")),
            Duration::from_secs(5)
        );
    }

    #[test]
    fn custom_connect_timeout_clamps_without_changing_neutral_transport_timeout() {
        let mut config = test_config(ProxyMode::Direct, "");
        for secs in [1_u64, 4] {
            config.connect_timeout_secs = secs;
            assert_eq!(
                custom_connect_timeout(&config),
                Duration::from_secs(5),
                "Custom lower bound for {secs}"
            );
            assert_eq!(
                HttpInferenceTransport::connect_timeout(&config),
                Duration::from_secs(secs),
                "neutral transport preserves {secs}"
            );
        }
        for secs in [5_u64, 30, 60] {
            config.connect_timeout_secs = secs;
            assert_eq!(
                custom_connect_timeout(&config),
                Duration::from_secs(secs),
                "Custom in-range {secs}"
            );
            assert_eq!(
                HttpInferenceTransport::connect_timeout(&config),
                Duration::from_secs(secs),
                "neutral transport preserves {secs}"
            );
        }
        for secs in [61_u64, 300] {
            config.connect_timeout_secs = secs;
            assert_eq!(
                custom_connect_timeout(&config),
                Duration::from_secs(60),
                "Custom upper bound for {secs}"
            );
            assert_eq!(
                HttpInferenceTransport::connect_timeout(&config),
                Duration::from_secs(secs),
                "neutral transport preserves {secs}"
            );
        }
    }

    #[test]
    fn isolated_headers_do_not_copy_client_or_dashboard_credentials() {
        let bearer = isolated_custom_headers(UpstreamAuthScheme::Bearer, "sk-custom").unwrap();
        assert_eq!(bearer.get(AUTHORIZATION).unwrap(), "Bearer sk-custom");
        assert!(!header_map_contains_forbidden_client_credentials(
            &bearer,
            UpstreamAuthScheme::Bearer
        ));
        assert!(bearer.get("cookie").is_none());
        assert!(bearer.get("x-api-key").is_none());
        assert!(bearer.get("x-goog-api-key").is_none());
        assert_eq!(bearer.len(), 1);

        let x_api = isolated_custom_headers(UpstreamAuthScheme::XApiKey, "sk-custom").unwrap();
        assert_eq!(x_api.get("x-api-key").unwrap(), "sk-custom");
        assert!(x_api.get(AUTHORIZATION).is_none());
        assert!(!header_map_contains_forbidden_client_credentials(
            &x_api,
            UpstreamAuthScheme::XApiKey
        ));
        assert_eq!(x_api.len(), 1);
        assert!(forbidden_forwarded_header_names().contains(&"cookie"));
        assert!(forbidden_forwarded_header_names().contains(&"authorization"));
    }

    #[tokio::test]
    async fn http_inference_transport_send_and_bounded_read_do_not_discover_models() {
        let hits = Arc::new(AtomicUsize::new(0));
        let addr = serve_http(200, "OK", &[], r#"{"ok":true}"#, hits.clone()).await;
        let transport = HttpInferenceTransport::build(
            &test_config(ProxyMode::Direct, ""),
            HttpInferenceTransportSpec::no_redirects(),
        )
        .unwrap();
        let url =
            reqwest::Url::parse(&format!("http://127.0.0.1:{}/v1/ping", addr.port())).unwrap();
        let response = transport
            .send(InferenceHttpRequest {
                method: reqwest::Method::POST,
                url,
                auth: Some((UpstreamAuthScheme::Bearer, "transport-key")),
                extra_headers: HeaderMap::new(),
                body: Some(br#"{"ping":true}"#.to_vec()),
                request_timeout: Some(Duration::from_secs(5)),
            })
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = HttpInferenceTransport::read_body_limited(response, 64)
            .await
            .unwrap();
        assert_eq!(body, br#"{"ok":true}"#);
        assert_eq!(hits.load(Ordering::SeqCst), 1);

        let oversized = Arc::new(AtomicUsize::new(0));
        let oversized_addr = serve_http(
            200,
            "OK",
            &[("Content-Type", "application/json".to_string())],
            &"x".repeat(32),
            oversized.clone(),
        )
        .await;
        let oversized_url = reqwest::Url::parse(&format!(
            "http://127.0.0.1:{}/v1/ping",
            oversized_addr.port()
        ))
        .unwrap();
        let oversized_response = transport
            .send(InferenceHttpRequest {
                method: reqwest::Method::GET,
                url: oversized_url,
                auth: None,
                extra_headers: HeaderMap::new(),
                body: None,
                request_timeout: Some(Duration::from_secs(5)),
            })
            .await
            .unwrap();
        let error = HttpInferenceTransport::read_body_limited(oversized_response, 8)
            .await
            .expect_err("bounded reader must reject an oversized body");
        assert!(
            matches!(error, InferenceHttpError::Oversize { limit: 8 }),
            "{error:?}"
        );
    }

    #[tokio::test]
    async fn http_inference_transport_redirect_policy_is_owned_by_the_spec() {
        for status in [301_u16, 302, 307, 308] {
            let second_hits = Arc::new(AtomicUsize::new(0));
            let second = serve_http(200, "OK", &[], "second", second_hits.clone()).await;
            let first_hits = Arc::new(AtomicUsize::new(0));
            let location = format!("http://127.0.0.1:{}/next", second.port());
            let first = serve_http(
                status,
                "Redirect",
                &[("Location", location)],
                "",
                first_hits.clone(),
            )
            .await;
            let start =
                reqwest::Url::parse(&format!("http://127.0.0.1:{}/start", first.port())).unwrap();

            let none = HttpInferenceTransport::build(
                &test_config(ProxyMode::Direct, ""),
                HttpInferenceTransportSpec::no_redirects(),
            )
            .unwrap();
            let blocked = none
                .send(InferenceHttpRequest {
                    method: reqwest::Method::GET,
                    url: start.clone(),
                    auth: None,
                    extra_headers: HeaderMap::new(),
                    body: None,
                    request_timeout: None,
                })
                .await
                .unwrap();
            assert_eq!(blocked.status().as_u16(), status, "status {status}");
            tokio::time::sleep(Duration::from_millis(50)).await;
            assert_eq!(second_hits.load(Ordering::SeqCst), 0, "none {status}");

            let follow = HttpInferenceTransport::build(
                &test_config(ProxyMode::Direct, ""),
                HttpInferenceTransportSpec::follow_redirects(),
            )
            .unwrap();
            let followed = follow
                .send(InferenceHttpRequest {
                    method: reqwest::Method::GET,
                    url: start,
                    auth: None,
                    extra_headers: HeaderMap::new(),
                    body: None,
                    request_timeout: None,
                })
                .await
                .unwrap();
            assert_eq!(followed.status(), StatusCode::OK, "follow {status}");
            tokio::time::sleep(Duration::from_millis(50)).await;
            assert_eq!(
                second_hits.load(Ordering::SeqCst),
                1,
                "follow {status} must open the Location target"
            );
        }
    }

    #[tokio::test]
    async fn redirects_are_not_followed_for_301_302_307_308() {
        for status in [301_u16, 302, 307, 308] {
            let second_hits = Arc::new(AtomicUsize::new(0));
            let second = serve_http(200, "OK", &[], "second", second_hits.clone()).await;
            let first_hits = Arc::new(AtomicUsize::new(0));
            let location = format!("http://127.0.0.1:{}/next", second.port());
            let first = serve_http(
                status,
                "Redirect",
                &[("Location", location)],
                "",
                first_hits.clone(),
            )
            .await;
            let client = build_custom_http_client(&test_config(ProxyMode::Direct, "")).unwrap();
            let url =
                reqwest::Url::parse(&format!("http://127.0.0.1:{}/start", first.port())).unwrap();
            let response = send_get(&client, url).await.unwrap();
            assert_eq!(response.status().as_u16(), status, "status {status}");
            tokio::time::sleep(Duration::from_millis(50)).await;
            assert_eq!(first_hits.load(Ordering::SeqCst), 1, "first hop {status}");
            assert_eq!(
                second_hits.load(Ordering::SeqCst),
                0,
                "redirect {status} must not open a second connection"
            );
        }
    }

    #[tokio::test]
    async fn direct_does_not_use_manual_proxy_and_manual_does_not_bypass_it() {
        let upstream_hits = Arc::new(AtomicUsize::new(0));
        let upstream = serve_http(200, "OK", &[], "direct", upstream_hits.clone()).await;
        let proxy_hits = Arc::new(AtomicUsize::new(0));
        let proxy = serve_counting_proxy(proxy_hits.clone()).await;

        let target =
            reqwest::Url::parse(&format!("http://127.0.0.1:{}/v1", upstream.port())).unwrap();
        let direct = build_custom_http_client(&test_config(ProxyMode::Direct, "")).unwrap();
        let response = send_get(&direct, target.clone()).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(upstream_hits.load(Ordering::SeqCst), 1);
        assert_eq!(proxy_hits.load(Ordering::SeqCst), 0);

        let manual = build_custom_http_client(&test_config(
            ProxyMode::Manual,
            &format!("http://127.0.0.1:{}", proxy.port()),
        ))
        .unwrap();
        let proxied = send_get(&manual, target).await.unwrap();
        assert_eq!(proxied.status(), StatusCode::OK);
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert_eq!(proxy_hits.load(Ordering::SeqCst), 1);
        assert_eq!(upstream_hits.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn loopback_and_private_literal_destinations_are_reachable_over_direct() {
        let hits = Arc::new(AtomicUsize::new(0));
        let addr = serve_http(200, "OK", &[], r#"{"ok":true}"#, hits.clone()).await;
        let client = build_custom_http_client(&test_config(ProxyMode::Direct, "")).unwrap();
        let url = reqwest::Url::parse(&format!("http://127.0.0.1:{}/v1", addr.port())).unwrap();
        let response = send_get(&client, url).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(hits.load(Ordering::SeqCst), 1);
    }

    async fn serve_delayed_json(delay: Duration, body: &str) -> std::net::SocketAddr {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("loopback listener");
        let addr = listener.local_addr().unwrap();
        let body = body.to_string();
        tokio::spawn(async move {
            let Ok((mut stream, _)) = listener.accept().await else {
                return;
            };
            let mut buf = vec![0_u8; 4096];
            let _ = stream.read(&mut buf).await;
            tokio::time::sleep(delay).await;
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = stream.write_all(response.as_bytes()).await;
        });
        addr
    }

    #[tokio::test]
    async fn connect_timeout_does_not_bound_post_connect_non_stream_reads() {
        let addr = serve_delayed_json(Duration::from_millis(1500), r#"{"ok":true}"#).await;
        let mut config = test_config(ProxyMode::Direct, "");
        config.connect_timeout_secs = 1;
        let client = build_custom_http_client(&config).unwrap();
        let url = reqwest::Url::parse(&format!("http://127.0.0.1:{}/v1", addr.port())).unwrap();
        let response = client
            .send_isolated(
                reqwest::Method::GET,
                url,
                UpstreamAuthScheme::Bearer,
                "test-key",
                HeaderMap::new(),
                None,
                Some(Duration::from_secs(5)),
            )
            .await
            .expect("post-connect delay must use the per-request timeout, not connect_timeout");
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn non_stream_request_timeout_is_enforced_per_request() {
        let addr = serve_delayed_json(Duration::from_secs(3), r#"{"ok":true}"#).await;
        let mut config = test_config(ProxyMode::Direct, "");
        config.connect_timeout_secs = 5;
        let client = build_custom_http_client(&config).unwrap();
        let url = reqwest::Url::parse(&format!("http://127.0.0.1:{}/v1", addr.port())).unwrap();
        let error = client
            .send_isolated(
                reqwest::Method::GET,
                url,
                UpstreamAuthScheme::Bearer,
                "test-key",
                HeaderMap::new(),
                None,
                Some(Duration::from_secs(1)),
            )
            .await
            .expect_err("non-stream Custom requests must honor the per-request timeout");
        assert!(
            error.to_string().to_ascii_lowercase().contains("timed")
                || error.to_string().to_ascii_lowercase().contains("timeout"),
            "{error}"
        );
    }
}
