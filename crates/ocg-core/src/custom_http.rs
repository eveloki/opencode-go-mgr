//! Dedicated Custom API HTTP client.
//!
//! Custom destinations are administrator-trusted. Direct, Manual, and Auto all
//! inherit the process-wide proxy policy from [`crate::http_client`]. The
//! client never follows redirects, never forwards dashboard/client auth, and
//! always composes isolated Bearer / `x-api-key` headers.

use crate::models::{AppConfig, ProxyMode};
use crate::provider::UpstreamAuthScheme;
use crate::provider::validate_custom_base_url;
use reqwest::header::{AUTHORIZATION, HeaderMap, HeaderName, HeaderValue};
use std::error::Error;
use std::fmt;
use std::time::Duration;

#[derive(Clone)]
pub struct CustomHttpClient {
    client: reqwest::Client,
    proxy_mode: ProxyMode,
}

impl fmt::Debug for CustomHttpClient {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CustomHttpClient")
            .field("proxy_mode", &self.proxy_mode)
            .finish_non_exhaustive()
    }
}

impl CustomHttpClient {
    pub fn proxy_mode(&self) -> ProxyMode {
        self.proxy_mode
    }

    pub(crate) fn request(
        &self,
        method: reqwest::Method,
        url: reqwest::Url,
    ) -> reqwest::RequestBuilder {
        self.client.request(method, url)
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
        let headers = isolated_custom_headers(scheme, api_key)?;
        let mut builder = self.client.request(method, url);
        for (name, value) in &headers {
            builder = builder.header(name, value);
        }
        for (name, value) in &extra_headers {
            builder = builder.header(name, value);
        }
        if let Some(body) = body {
            builder = builder.body(body);
        }
        if let Some(request_timeout) = request_timeout {
            builder = builder.timeout(request_timeout);
        }
        builder.send().await.map_err(map_reqwest_send_error)
    }
}

fn map_reqwest_send_error(error: reqwest::Error) -> CustomHttpError {
    let mut current: Option<&dyn Error> = Some(&error);
    while let Some(err) = current {
        if let Some(custom) = err.downcast_ref::<CustomHttpError>() {
            return custom.clone();
        }
        current = err.source();
    }
    if error.is_timeout() {
        CustomHttpError::Network(format!("upstream request timed out: {error}"))
    } else {
        CustomHttpError::Network(error.to_string())
    }
}

pub fn build_custom_http_client(config: &AppConfig) -> Result<CustomHttpClient, CustomHttpError> {
    // Connect timeout only. Non-stream callers apply `non_stream_timeout_secs`
    // per request; streaming must be able to outlive that total duration.
    let client = crate::http_client::configured_builder(config)
        .map_err(|error| CustomHttpError::Build(error.to_string()))?
        .redirect(crate::http_client::no_redirect_policy())
        .connect_timeout(Duration::from_secs(config.connect_timeout_secs))
        .build()
        .map_err(|error| CustomHttpError::Build(error.to_string()))?;
    Ok(CustomHttpClient {
        client,
        proxy_mode: config.proxy_mode,
    })
}

/// Join `path` onto a persisted Custom base URL while keeping the origin and
/// path prefix. Absolute URLs, protocol-relative targets, decoded dot-segments,
/// encoded slash/backslash, and nested percent-encoding are rejected as
/// endpoint override.
pub fn join_custom_endpoint(base_url: &str, path: &str) -> Result<reqwest::Url, CustomHttpError> {
    let canonical = validate_custom_base_url(base_url).map_err(CustomHttpError::from)?;
    let base = reqwest::Url::parse(&canonical)
        .map_err(|error| CustomHttpError::InvalidUrl(error.to_string()))?;
    let relative = path.trim();
    if relative.is_empty() {
        return Ok(base);
    }
    if is_endpoint_override(relative) {
        return Err(CustomHttpError::EndpointOverride(relative.to_string()));
    }
    let stripped = relative.trim_start_matches('/');
    let joined = format!("{canonical}/{stripped}");
    let parsed = reqwest::Url::parse(&joined)
        .map_err(|error| CustomHttpError::InvalidUrl(error.to_string()))?;
    if parsed.scheme() != base.scheme()
        || parsed.host() != base.host()
        || parsed.port_or_known_default() != base.port_or_known_default()
    {
        return Err(CustomHttpError::EndpointOverride(relative.to_string()));
    }
    if parsed.query().is_some() || parsed.fragment().is_some() {
        return Err(CustomHttpError::EndpointOverride(
            "joined endpoint must not include a query or fragment".to_string(),
        ));
    }
    if !path_has_prefix(parsed.path(), base.path()) {
        return Err(CustomHttpError::EndpointOverride(
            "joined path escaped the Custom base prefix".to_string(),
        ));
    }
    if path_has_unsafe_segments(parsed.path()) {
        return Err(CustomHttpError::EndpointOverride(
            "joined path must not contain unsafe or recursively encoded segments".to_string(),
        ));
    }
    Ok(parsed)
}

fn is_endpoint_override(path: &str) -> bool {
    let trimmed = path.trim();
    if trimmed.contains("://")
        || trimmed.starts_with("//")
        || trimmed.starts_with('\\')
        || trimmed.contains('\\')
    {
        return true;
    }
    if trimmed.contains('\0') || trimmed.chars().any(char::is_control) {
        return true;
    }
    if path_has_unsafe_segments(trimmed) {
        return true;
    }
    matches!(
        reqwest::Url::parse(trimmed)
            .ok()
            .map(|url| url.scheme().to_string()),
        Some(scheme) if matches!(
            scheme.as_str(),
            "http" | "https" | "ftp" | "file" | "ws" | "wss" | "javascript" | "data"
        )
    )
}

fn path_has_unsafe_segments(path: &str) -> bool {
    for segment in path.split(['/', '\\']) {
        if segment.is_empty() {
            continue;
        }
        if segment == "."
            || segment == ".."
            || segment.contains('\0')
            || segment.chars().any(char::is_control)
        {
            return true;
        }
        match percent_decode_utf8(segment) {
            Some(decoded)
                if decoded == "."
                    || decoded == ".."
                    || decoded.contains('/')
                    || decoded.contains('\\')
                    || decoded.contains('\0')
                    || decoded.chars().any(char::is_control)
                    || contains_percent_escape(&decoded) =>
            {
                return true;
            }
            None => return true,
            Some(_) => {}
        }
    }
    false
}

fn contains_percent_escape(input: &str) -> bool {
    input.as_bytes().windows(3).any(|window| {
        window[0] == b'%' && hex_val(window[1]).is_some() && hex_val(window[2]).is_some()
    })
}

fn percent_decode_utf8(input: &str) -> Option<String> {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            if index + 2 >= bytes.len() {
                return None;
            }
            let value = hex_pair(bytes[index + 1], bytes[index + 2])?;
            out.push(value);
            index += 3;
        } else {
            out.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(out).ok()
}

fn hex_pair(high: u8, low: u8) -> Option<u8> {
    Some((hex_val(high)? << 4) | hex_val(low)?)
}

fn hex_val(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn path_has_prefix(path: &str, prefix: &str) -> bool {
    match prefix {
        "" | "/" => path.starts_with('/'),
        other => {
            let prefix = other.trim_end_matches('/');
            path == prefix || path.starts_with(&format!("{prefix}/"))
        }
    }
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
    let mut headers = HeaderMap::new();
    match scheme {
        UpstreamAuthScheme::Bearer => {
            let value = HeaderValue::from_str(&format!("Bearer {api_key}"))
                .map_err(|error| CustomHttpError::InvalidUrl(error.to_string()))?;
            headers.insert(AUTHORIZATION, value);
        }
        UpstreamAuthScheme::XApiKey => {
            let value = HeaderValue::from_str(api_key)
                .map_err(|error| CustomHttpError::InvalidUrl(error.to_string()))?;
            headers.insert(HeaderName::from_static("x-api-key"), value);
        }
    }
    Ok(headers)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::AppConfig;
    use reqwest::StatusCode;
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
