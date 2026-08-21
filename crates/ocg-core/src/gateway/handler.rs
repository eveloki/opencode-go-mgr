use crate::alias;
use crate::gateway::diagnostics::{
    ErrorDiagnostic, REQUEST_ID_HEADER, RequestTrace, emit_failure, serialize_diagnostic,
};
use crate::gateway::forwarder::{
    ForwardAction, UpstreamPayloadTooLargeResponse, forward_get, forward_request,
    rate_limited_response,
};
use crate::gateway::materialize::{materialize_account_routes, protocol_error_from_resolve};
use crate::gateway::protocol::{
    ApiFormat, MaterializeSpec, ProtocolError, RequestPlan, format_error, format_protocol_error,
    materialize_parsed_request, parse_client_request, parse_gemini_request,
};
use crate::gateway::routing::resolve_conversation_key;
use crate::gateway::selector::AccountSelector;
use crate::models::UpstreamChannel;
use crate::models::{
    AppConfig, CLAUDE_DESKTOP_HAIKU_ALIAS, CLAUDE_DESKTOP_OPUS_ALIAS, CLAUDE_DESKTOP_SONNET_ALIAS,
};
use crate::state::CoreState;
use axum::body::{Body, Bytes, to_bytes};
use axum::extract::{Extension, Path, State};
use axum::http::{HeaderMap, HeaderValue, Request, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

pub async fn request_trace_middleware(
    State(state): State<CoreState>,
    mut request: Request<Body>,
    next: Next,
) -> Response {
    let trace = RequestTrace::new();
    let path = request.uri().path().to_string();
    let client_body_bytes = request
        .headers()
        .get(axum::http::header::CONTENT_LENGTH)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<usize>().ok());
    let authenticated = check_auth(request.headers(), &state);
    request.extensions_mut().insert(trace.clone());
    let mut response = next.run(request).await;

    if response.status() == StatusCode::PAYLOAD_TOO_LARGE
        && authenticated
        && response
            .extensions()
            .get::<UpstreamPayloadTooLargeResponse>()
            .is_none()
    {
        let mut diagnostic = ErrorDiagnostic::new(
            &trace,
            1,
            "client",
            "body_limit",
            client_format_for_path(&path),
        );
        diagnostic.client_body_bytes = client_body_bytes;
        diagnostic.downstream_status = Some(StatusCode::PAYLOAD_TOO_LARGE.as_u16());
        let duration_ms = diagnostic.duration_ms.min(i64::MAX as u64) as i64;
        let encoded = serialize_diagnostic(diagnostic);
        let _ = state.db.lock().log_gateway_diagnostic(
            "warn",
            "gateway_request",
            "gateway request body exceeded the configured limit",
            Some(&trace.request_id),
            Some(1),
            Some("client"),
            Some("body_limit"),
            Some(duration_ms),
            Some(&encoded),
        );
        emit_failure(&encoded);
    }

    response.headers_mut().insert(
        REQUEST_ID_HEADER,
        HeaderValue::from_str(&trace.request_id)
            .expect("generated request id must be a valid header value"),
    );
    response
}

fn client_format_for_path(path: &str) -> ApiFormat {
    if path.ends_with("/responses") {
        ApiFormat::Responses
    } else if path.ends_with("/messages") {
        ApiFormat::Messages
    } else if path.starts_with("/v1beta/models/")
        || (path.starts_with("/v1/models/") && path.contains(':'))
    {
        ApiFormat::Gemini
    } else {
        ApiFormat::ChatCompletions
    }
}

pub async fn chat_completions(
    State(state): State<CoreState>,
    Extension(trace): Extension<RequestTrace>,
    headers: HeaderMap,
    body: Bytes,
) -> axum::response::Response {
    proxy_handler(state, trace, headers, body, ApiFormat::ChatCompletions).await
}

pub async fn responses(
    State(state): State<CoreState>,
    Extension(trace): Extension<RequestTrace>,
    headers: HeaderMap,
    body: Bytes,
) -> axum::response::Response {
    proxy_handler(state, trace, headers, body, ApiFormat::Responses).await
}

pub async fn messages(
    State(state): State<CoreState>,
    Extension(trace): Extension<RequestTrace>,
    headers: HeaderMap,
    body: Bytes,
) -> axum::response::Response {
    proxy_handler(state, trace, headers, body, ApiFormat::Messages).await
}

pub async fn claude_desktop_messages(
    State(state): State<CoreState>,
    Extension(trace): Extension<RequestTrace>,
    headers: HeaderMap,
    body: Bytes,
) -> axum::response::Response {
    proxy_handler_inner(state, trace, headers, body, ApiFormat::Messages, true).await
}

pub async fn claude_desktop_models(
    State(state): State<CoreState>,
    headers: HeaderMap,
) -> axum::response::Response {
    if !check_auth(&headers, &state) {
        return protocol_error_response(
            ApiFormat::Messages,
            StatusCode::UNAUTHORIZED,
            "invalid gateway key",
            None,
        );
    }

    axum::Json(serde_json::json!({
        "data": [
            {
                "type": "model",
                "id": CLAUDE_DESKTOP_SONNET_ALIAS,
                "display_name": "Claude Sonnet 4.6",
                "created_at": "2026-02-17T00:00:00Z"
            },
            {
                "type": "model",
                "id": CLAUDE_DESKTOP_OPUS_ALIAS,
                "display_name": "Claude Opus 4.6",
                "created_at": "2026-02-05T00:00:00Z"
            },
            {
                "type": "model",
                "id": CLAUDE_DESKTOP_HAIKU_ALIAS,
                "display_name": "Claude Haiku 4.5",
                "created_at": "2025-10-01T00:00:00Z"
            }
        ],
        "has_more": false,
        "first_id": CLAUDE_DESKTOP_SONNET_ALIAS,
        "last_id": CLAUDE_DESKTOP_HAIKU_ALIAS
    }))
    .into_response()
}

pub async fn gemini_model_action(
    State(state): State<CoreState>,
    Extension(trace): Extension<RequestTrace>,
    Path(model_action): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> axum::response::Response {
    let client_body_bytes = body.len();
    let Some((model, action)) = model_action.rsplit_once(':') else {
        return gemini_error(
            &state,
            &trace,
            &headers,
            StatusCode::NOT_FOUND,
            "Gemini model action is required",
            Some(client_body_bytes),
        );
    };
    if model.is_empty() {
        return gemini_error(
            &state,
            &trace,
            &headers,
            StatusCode::BAD_REQUEST,
            "Gemini model is required",
            Some(client_body_bytes),
        );
    }
    match action {
        "generateContent" => {
            gemini_proxy_handler(state, trace, headers, body, model.to_string(), false).await
        }
        "streamGenerateContent" => {
            gemini_proxy_handler(state, trace, headers, body, model.to_string(), true).await
        }
        "countTokens" => gemini_expected_fallback(
            &state,
            &headers,
            StatusCode::NOT_IMPLEMENTED,
            "Gemini countTokens is not available; Gemini CLI falls back to local estimation",
        ),
        "embedContent" => gemini_error(
            &state,
            &trace,
            &headers,
            StatusCode::NOT_IMPLEMENTED,
            "Gemini embeddings are not supported by this gateway",
            Some(client_body_bytes),
        ),
        _ => gemini_error(
            &state,
            &trace,
            &headers,
            StatusCode::NOT_FOUND,
            "unknown Gemini model action",
            Some(client_body_bytes),
        ),
    }
}

/// GET /v1/models — authenticated discovery that never advertises raw IDs
/// inference would reject.
///
/// The request still uses an enabled OpenCode Go account so availability,
/// cooldown, redaction, and upstream error behavior stay aligned with the
/// rest of the gateway. On a successful catalog, `data[].id` is restricted to
/// hardcoded Alias registry names.
pub async fn models(
    State(state): State<CoreState>,
    Extension(trace): Extension<RequestTrace>,
    headers: HeaderMap,
) -> axum::response::Response {
    if !check_auth(&headers, &state) {
        return protocol_error_response(
            ApiFormat::ChatCompletions,
            StatusCode::UNAUTHORIZED,
            "invalid gateway key",
            None,
        );
    }

    let (config, client) = state.upstream_context();
    match forward_get(&client, &state, &config, "/v1/models").await {
        Ok(resp) if resp.status().is_success() => {
            restrict_models_list_to_published_aliases(resp).await
        }
        Ok(resp) => resp,
        Err(e) => local_failure_response(
            &state,
            &trace,
            ApiFormat::ChatCompletions,
            StatusCode::BAD_GATEWAY,
            &format!("models error: {}", e),
            "transport",
            "connect",
            None,
            None,
        ),
    }
}

async fn restrict_models_list_to_published_aliases(
    response: axum::response::Response,
) -> axum::response::Response {
    let status = response.status();
    let headers = response.headers().clone();
    let bytes = match to_bytes(response.into_body(), 1024 * 1024).await {
        Ok(bytes) => bytes,
        Err(_) => return published_alias_models_response(),
    };
    let Ok(mut payload) = serde_json::from_slice::<serde_json::Value>(&bytes) else {
        return restore_models_response(status, headers, bytes);
    };
    let Some(data) = payload.get("data").and_then(serde_json::Value::as_array) else {
        return restore_models_response(status, headers, bytes);
    };
    let filtered: Vec<serde_json::Value> = data
        .iter()
        .filter(|item| {
            item.get("id")
                .and_then(serde_json::Value::as_str)
                .is_some_and(alias::is_published_alias)
        })
        .cloned()
        .collect();
    if filtered.is_empty() {
        return restore_models_response(status, headers, bytes);
    }
    payload["data"] = serde_json::Value::Array(filtered);
    axum::Json(payload).into_response()
}

fn restore_models_response(
    status: StatusCode,
    headers: HeaderMap,
    bytes: Bytes,
) -> axum::response::Response {
    let mut response = (status, bytes).into_response();
    *response.headers_mut() = headers;
    response
}

fn published_alias_models_response() -> axum::response::Response {
    let data: Vec<serde_json::Value> = alias::published_aliases()
        .into_iter()
        .map(|id| {
            serde_json::json!({
                "id": id,
                "object": "model",
                "created": 0,
                "owned_by": "opencode"
            })
        })
        .collect();
    axum::Json(serde_json::json!({
        "object": "list",
        "data": data
    }))
    .into_response()
}

async fn proxy_handler(
    state: CoreState,
    trace: RequestTrace,
    headers: HeaderMap,
    body: Bytes,
    client_format: ApiFormat,
) -> axum::response::Response {
    proxy_handler_inner(state, trace, headers, body, client_format, false).await
}

async fn proxy_handler_inner(
    state: CoreState,
    trace: RequestTrace,
    headers: HeaderMap,
    body: Bytes,
    client_format: ApiFormat,
    claude_desktop: bool,
) -> axum::response::Response {
    let (config, client) = state.upstream_context();
    let client_body_bytes = body.len();

    let Some(client_key_id) = extract_client_key_id(&headers, &state) else {
        return protocol_error_response(
            client_format,
            StatusCode::UNAUTHORIZED,
            "invalid gateway key",
            None,
        );
    };

    let client_body = body.clone();
    let parsed = match parse_client_request(client_format, body) {
        Ok(parsed) => parsed,
        Err(error) => {
            return local_protocol_failure(
                &state,
                &trace,
                client_format,
                error,
                Some(client_body_bytes),
                Some(&client_body),
            );
        }
    };
    let client_model = parsed.requested_model.clone();
    let routing_model = if claude_desktop {
        match config
            .claude_desktop_models
            .model_for_alias(&parsed.requested_model)
        {
            Some(model) => model.to_string(),
            None => {
                return local_protocol_failure(
                    &state,
                    &trace,
                    ApiFormat::Messages,
                    ProtocolError::new(format!(
                        "unsupported Claude Desktop model alias `{}`",
                        parsed.requested_model
                    )),
                    Some(client_body_bytes),
                    Some(&client_body),
                );
            }
        }
    } else {
        parsed.requested_model.clone()
    };
    let resolved = match alias::resolve(&routing_model) {
        Ok(resolved) => resolved,
        Err(error) => {
            return local_protocol_failure(
                &state,
                &trace,
                client_format,
                protocol_error_from_resolve(error),
                Some(client_body_bytes),
                Some(&client_body),
            );
        }
    };

    execute_plan(
        state,
        trace,
        client_body,
        headers,
        client_format,
        parsed,
        resolved,
        client_model,
        routing_model,
        config,
        client,
        Some(client_key_id),
    )
    .await
}

#[cfg(test)]
fn rewrite_claude_desktop_model(
    body: &Bytes,
    models: &crate::models::ClaudeDesktopModels,
) -> Result<Bytes, ProtocolError> {
    let mut request: serde_json::Value = serde_json::from_slice(body)
        .map_err(|error| ProtocolError::new(format!("invalid JSON request: {error}")))?;
    let object = request
        .as_object_mut()
        .ok_or_else(|| ProtocolError::new("request must be a JSON object"))?;
    let alias = object
        .get("model")
        .and_then(serde_json::Value::as_str)
        .filter(|model| !model.is_empty())
        .ok_or_else(|| ProtocolError::new("request model is required"))?;
    let model = models
        .model_for_alias(alias)
        .ok_or_else(|| {
            ProtocolError::new(format!("unsupported Claude Desktop model alias `{alias}`"))
        })?
        .to_string();
    object.insert("model".to_string(), serde_json::Value::String(model));
    serde_json::to_vec(&request)
        .map(Bytes::from)
        .map_err(|error| ProtocolError::new(format!("failed to encode request: {error}")))
}

async fn gemini_proxy_handler(
    state: CoreState,
    trace: RequestTrace,
    headers: HeaderMap,
    body: Bytes,
    model: String,
    stream: bool,
) -> axum::response::Response {
    let (config, client) = state.upstream_context();
    let client_body_bytes = body.len();
    let Some(client_key_id) = extract_client_key_id(&headers, &state) else {
        return protocol_error_response(
            ApiFormat::Gemini,
            StatusCode::UNAUTHORIZED,
            "invalid gateway key",
            None,
        );
    };
    let parsed = match parse_gemini_request(model, stream, body.clone()) {
        Ok(parsed) => parsed,
        Err(error) => {
            return local_protocol_failure(
                &state,
                &trace,
                ApiFormat::Gemini,
                error,
                Some(client_body_bytes),
                Some(&body),
            );
        }
    };
    let client_model = parsed.requested_model.clone();
    let routing_model = parsed.requested_model.clone();
    let resolved = match alias::resolve(&routing_model) {
        Ok(resolved) => resolved,
        Err(error) => {
            return local_protocol_failure(
                &state,
                &trace,
                ApiFormat::Gemini,
                protocol_error_from_resolve(error),
                Some(client_body_bytes),
                Some(&body),
            );
        }
    };
    execute_plan(
        state,
        trace,
        body,
        headers,
        ApiFormat::Gemini,
        parsed,
        resolved,
        client_model,
        routing_model,
        config,
        client,
        Some(client_key_id),
    )
    .await
}

#[allow(clippy::too_many_arguments)]
async fn execute_plan(
    state: CoreState,
    trace: RequestTrace,
    client_body: Bytes,
    headers: HeaderMap,
    client_format: ApiFormat,
    parsed: crate::gateway::protocol::ParsedClientRequest,
    resolved: alias::ResolvedModel,
    client_model: String,
    routing_model: String,
    config: AppConfig,
    client: reqwest::Client,
    client_key_id: Option<String>,
) -> axum::response::Response {
    // One logical client request, including safe retries and account fallback,
    // must use one immutable pricing revision from start to finish.
    let pricing_snapshot = state.pricing_snapshot();
    let conversation_key = if config.conversation_sticky {
        resolve_conversation_key(client_format, &routing_model, &headers, &client_body)
    } else {
        None
    };
    let (diagnostic_model, diagnostic_channel) = match &resolved {
        alias::ResolvedModel::Alias {
            alias, mappings, ..
        } => {
            let zen_only = mappings
                .iter()
                .filter(|mapping| mapping.routeable)
                .all(|mapping| mapping.is_zen_free());
            (
                (*alias).to_string(),
                if zen_only {
                    UpstreamChannel::Free
                } else {
                    UpstreamChannel::Go
                },
            )
        }
        alias::ResolvedModel::PinnedRaw { mapping, .. } => (
            mapping.upstream_model.to_string(),
            if mapping.is_zen_free() {
                UpstreamChannel::Free
            } else {
                UpstreamChannel::Go
            },
        ),
    };
    let requested_plan = match materialize_parsed_request(
        &parsed,
        &MaterializeSpec {
            client_model: client_model.clone(),
            upstream_model: diagnostic_model,
            channel: diagnostic_channel,
            upstream_base_override: None,
            original_model: None,
            allow_go_fallback: false,
        },
    ) {
        Ok(plan) => plan,
        Err(error) => {
            return local_protocol_failure(
                &state,
                &trace,
                client_format,
                error,
                Some(client_body.len()),
                Some(&client_body),
            );
        }
    };

    let mut last_error: Option<String> = None;
    let mut failed_ids: Vec<String> = Vec::new();
    let mut attempt = 0u32;

    loop {
        let (accounts, free_cooldown) = {
            let db = state.db.lock();
            let accounts = match db.list_accounts() {
                Ok(accounts) => accounts,
                Err(error) => {
                    let message = format!("failed to select account: {error}");
                    record_plan_failure(
                        &state,
                        &trace,
                        &client_body,
                        attempt.max(1),
                        client_format,
                        &requested_plan,
                        "gateway",
                        "account_selection",
                        StatusCode::INTERNAL_SERVER_ERROR,
                        &message,
                    );
                    return protocol_error_response(
                        client_format,
                        StatusCode::INTERNAL_SERVER_ERROR,
                        &message,
                        None,
                    );
                }
            };
            let free_cooldown = match db.free_channel_cooldown_until() {
                Ok(cooldown) => cooldown,
                Err(error) => {
                    let message = format!("failed to read free-channel cooldown: {error}");
                    return protocol_error_response(
                        client_format,
                        StatusCode::INTERNAL_SERVER_ERROR,
                        &message,
                        None,
                    );
                }
            };
            (accounts, free_cooldown)
        };
        let free_available =
            free_cooldown.is_none() && !AccountSelector::free_channel_exhausted(&accounts);
        let route_set = match materialize_account_routes(
            &accounts,
            &config,
            &parsed,
            &resolved,
            &client_model,
            &routing_model,
            &client_body,
            free_available,
        ) {
            Ok(route_set) => route_set,
            Err(error) => {
                return local_protocol_failure(
                    &state,
                    &trace,
                    client_format,
                    error,
                    Some(client_body.len()),
                    Some(&client_body),
                );
            }
        };
        let excluded = failed_ids.iter().map(String::as_str).collect::<Vec<_>>();
        let routing_candidates = route_set
            .routes
            .iter()
            .map(|route| route.routing.clone())
            .collect::<Vec<_>>();
        let selected = state.routing.select_candidate(
            &routing_candidates,
            config.routing_mode,
            config.conversation_sticky,
            conversation_key.as_deref(),
            &excluded,
        );
        let Some(selected) = selected else {
            if route_set.free_only
                && let Some(until) = free_cooldown
            {
                record_plan_failure(
                    &state,
                    &trace,
                    &client_body,
                    attempt.max(1),
                    client_format,
                    &requested_plan,
                    "gateway",
                    "account_selection",
                    StatusCode::TOO_MANY_REQUESTS,
                    "free channel is rate-limited",
                );
                return rate_limited_response(client_format, until);
            }
            let now = chrono::Utc::now();
            let soonest = route_set
                .routes
                .iter()
                .filter_map(|route| {
                    route
                        .routing
                        .account
                        .cooldown_ends_at_for(route.routing.channel, now)
                })
                .min();
            return match soonest {
                Some(until) => {
                    record_plan_failure(
                        &state,
                        &trace,
                        &client_body,
                        attempt.max(1),
                        client_format,
                        &requested_plan,
                        "gateway",
                        "account_selection",
                        StatusCode::TOO_MANY_REQUESTS,
                        "all compatible accounts are rate-limited",
                    );
                    rate_limited_response(client_format, until)
                }
                None => {
                    let msg = last_error.clone().unwrap_or_else(|| {
                        route_set.incompatibility.unwrap_or_else(|| {
                            "no compatible provider accounts are available".to_string()
                        })
                    });
                    record_plan_failure(
                        &state,
                        &trace,
                        &client_body,
                        attempt.max(1),
                        client_format,
                        &requested_plan,
                        "gateway",
                        "account_selection",
                        StatusCode::SERVICE_UNAVAILABLE,
                        &msg,
                    );
                    protocol_error_response(
                        client_format,
                        StatusCode::SERVICE_UNAVAILABLE,
                        &msg,
                        None,
                    )
                }
            };
        };
        let route = route_set
            .routes
            .into_iter()
            .find(|route| {
                route.routing.account.id == selected.account.id
                    && route.routing.channel == selected.channel
                    && route.routing.resolved_model == selected.resolved_model
            })
            .expect("selected routing candidate must retain its request plan");
        let account = route.routing.account;
        let active_plan = route.plan;

        let mut retried_same_account = false;
        loop {
            attempt = attempt.saturating_add(1);
            match forward_request(
                &client,
                &state,
                &account,
                &config,
                &active_plan,
                &trace,
                &client_body,
                attempt,
                !retried_same_account,
                headers.clone(),
                pricing_snapshot.clone(),
                client_key_id.as_deref(),
            )
            .await
            {
                Ok(result) => match result.action {
                    ForwardAction::Return => return result.response,
                    ForwardAction::RetrySameAccount if !retried_same_account => {
                        retried_same_account = true;
                        let _ = state.db.lock().log_gateway_diagnostic(
                                "warn",
                                "gateway",
                                &format!(
                                    "account {} attempt ended before any downstream response data was emitted; retrying once: {:?}",
                                    account.name, result.error_message
                                ),
                                Some(&trace.request_id),
                                Some(attempt as i64),
                                Some("gateway"),
                                Some("retry"),
                                Some(trace.elapsed_ms() as i64),
                                None,
                            );
                        continue;
                    }
                    ForwardAction::RetrySameAccount => return result.response,
                    ForwardAction::ExhaustFreeChannel => {
                        last_error = result.error_message.clone();
                        failed_ids.push(account.id.clone());
                        let _ = state.db.lock().log_gateway_diagnostic(
                            "warn",
                            "gateway",
                            &format!(
                                "Zen Free route {} was exhausted before output; continuing through the global account order: {:?}",
                                account.name, result.error_message
                            ),
                            Some(&trace.request_id),
                            Some(attempt as i64),
                            Some("upstream"),
                            Some("free_fallback"),
                            Some(trace.elapsed_ms() as i64),
                            None,
                        );
                        break;
                    }
                    ForwardAction::TryNextAccount => {
                        last_error = result.error_message.clone();
                        failed_ids.push(account.id.clone());
                        let _ = state.db.lock().log_gateway_diagnostic(
                            "warn",
                            "gateway",
                            &format!(
                                "account {} was rejected, switching to next: {:?}",
                                account.name, result.error_message
                            ),
                            Some(&trace.request_id),
                            Some(attempt as i64),
                            Some("upstream"),
                            Some("upstream_http"),
                            Some(trace.elapsed_ms() as i64),
                            None,
                        );
                        break;
                    }
                },
                Err(e) => {
                    let message = format!("forward error: {e}");
                    record_plan_failure(
                        &state,
                        &trace,
                        &client_body,
                        attempt,
                        client_format,
                        &active_plan,
                        "gateway",
                        "internal",
                        StatusCode::INTERNAL_SERVER_ERROR,
                        &format!("account {} forward failed locally: {e}", account.name),
                    );
                    return protocol_error_response(
                        client_format,
                        StatusCode::INTERNAL_SERVER_ERROR,
                        &message,
                        None,
                    );
                }
            }
        }
    }
}

/// Candidate credential values a client may present, in fixed priority
/// order: the Bearer token, then `x-api-key`, then `x-goog-api-key`. Every
/// non-empty candidate is an independent credential claim; a wrong value
/// alongside a correct one never downgrades the request.
fn candidate_key_values(headers: &HeaderMap) -> Vec<&str> {
    let mut candidates = Vec::with_capacity(3);
    let bearer = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|auth| auth.trim().strip_prefix("Bearer "))
        .map(str::trim)
        .filter(|value| !value.is_empty());
    if let Some(value) = bearer {
        candidates.push(value);
    }
    for name in ["x-api-key", "x-goog-api-key"] {
        let value = headers
            .get(name)
            .and_then(|value| value.to_str().ok())
            .map(str::trim)
            .filter(|value| !value.is_empty());
        if let Some(value) = value {
            candidates.push(value);
        }
    }
    candidates
}

/// Extracts the id of the credential that authenticates this request.
/// Authentication succeeds when ANY non-empty candidate header matches ANY
/// currently valid credential (the primary key value or an enabled,
/// non-deleted sub key) in the credential snapshot; the first candidate hit,
/// in header order, attributes the request (the primary key resolves to the
/// fixed `PRIMARY_KEY_ID`).
pub(crate) fn extract_client_key_id(headers: &HeaderMap, state: &CoreState) -> Option<String> {
    candidate_key_values(headers)
        .into_iter()
        .find_map(|value| state.credential_entry_for_value(value))
        .map(|entry| entry.id)
}

fn check_auth(headers: &HeaderMap, state: &CoreState) -> bool {
    extract_client_key_id(headers, state).is_some()
}

fn gemini_error(
    state: &CoreState,
    trace: &RequestTrace,
    headers: &HeaderMap,
    status: StatusCode,
    message: &str,
    client_body_bytes: Option<usize>,
) -> axum::response::Response {
    if !check_auth(headers, state) {
        return protocol_error_response(
            ApiFormat::Gemini,
            StatusCode::UNAUTHORIZED,
            "invalid gateway key",
            None,
        );
    }
    local_failure_response(
        state,
        trace,
        ApiFormat::Gemini,
        status,
        message,
        "client",
        "validation",
        client_body_bytes,
        None,
    )
}

fn gemini_expected_fallback(
    state: &CoreState,
    headers: &HeaderMap,
    status: StatusCode,
    message: &str,
) -> axum::response::Response {
    if !check_auth(headers, state) {
        return protocol_error_response(
            ApiFormat::Gemini,
            StatusCode::UNAUTHORIZED,
            "invalid gateway key",
            None,
        );
    }
    protocol_error_response(ApiFormat::Gemini, status, message, None)
}

#[allow(clippy::too_many_arguments)]
fn local_failure_response(
    state: &CoreState,
    trace: &RequestTrace,
    format: ApiFormat,
    status: StatusCode,
    message: &str,
    error_source: &str,
    error_stage: &str,
    client_body_bytes: Option<usize>,
    summary_body: Option<&[u8]>,
) -> axum::response::Response {
    let mut diagnostic = ErrorDiagnostic::new(trace, 1, error_source, error_stage, format);
    diagnostic.client_body_bytes = client_body_bytes;
    diagnostic.downstream_status = Some(status.as_u16());
    if let Some(body) = summary_body {
        diagnostic = diagnostic.with_request_summary(body);
    }
    let duration_ms = diagnostic.duration_ms.min(i64::MAX as u64) as i64;
    let encoded = serialize_diagnostic(diagnostic);
    let _ = state.db.lock().log_gateway_diagnostic(
        if status.is_server_error() {
            "error"
        } else {
            "warn"
        },
        "gateway_request",
        message,
        Some(&trace.request_id),
        Some(1),
        Some(error_source),
        Some(error_stage),
        Some(duration_ms),
        Some(&encoded),
    );
    emit_failure(&encoded);
    protocol_error_from(
        format,
        ProtocolError::with_status(status, message.to_string()),
    )
}

fn local_protocol_failure(
    state: &CoreState,
    trace: &RequestTrace,
    format: ApiFormat,
    error: ProtocolError,
    client_body_bytes: Option<usize>,
    summary_body: Option<&[u8]>,
) -> axum::response::Response {
    let stage = if error.message.starts_with("invalid JSON request") {
        "parse"
    } else {
        "validation"
    };
    let mut diagnostic = ErrorDiagnostic::new(trace, 1, "client", stage, format);
    diagnostic.client_body_bytes = client_body_bytes;
    diagnostic.downstream_status = Some(error.status.as_u16());
    if let Some(body) = summary_body {
        diagnostic = diagnostic.with_request_summary(body);
    }
    let duration_ms = diagnostic.duration_ms.min(i64::MAX as u64) as i64;
    let encoded = serialize_diagnostic(diagnostic);
    let _ = state.db.lock().log_gateway_diagnostic(
        if error.status.is_server_error() {
            "error"
        } else {
            "warn"
        },
        "gateway_request",
        &error.message,
        Some(&trace.request_id),
        Some(1),
        Some("client"),
        Some(stage),
        Some(duration_ms),
        Some(&encoded),
    );
    emit_failure(&encoded);
    protocol_error_from(format, error)
}

#[allow(clippy::too_many_arguments)]
fn record_plan_failure(
    state: &CoreState,
    trace: &RequestTrace,
    client_body: &[u8],
    attempt: u32,
    client_format: ApiFormat,
    plan: &RequestPlan,
    error_source: &str,
    error_stage: &str,
    status: StatusCode,
    message: &str,
) {
    let mut diagnostic =
        ErrorDiagnostic::new(trace, attempt, error_source, error_stage, client_format)
            .with_request_summary(client_body);
    diagnostic.client_body_bytes = Some(client_body.len());
    diagnostic.upstream_body_bytes = Some(plan.body.len());
    diagnostic.upstream_format =
        Some(crate::gateway::diagnostics::api_format_name(plan.upstream).to_string());
    diagnostic.model = Some(plan.model.clone());
    diagnostic.stream = Some(plan.stream);
    diagnostic.downstream_status = Some(status.as_u16());
    let duration_ms = diagnostic.duration_ms.min(i64::MAX as u64) as i64;
    let encoded = serialize_diagnostic(diagnostic);
    let _ = state.db.lock().log_gateway_diagnostic(
        if status.is_server_error() {
            "error"
        } else {
            "warn"
        },
        "gateway_request",
        message,
        Some(&trace.request_id),
        Some(attempt as i64),
        Some(error_source),
        Some(error_stage),
        Some(duration_ms),
        Some(&encoded),
    );
    emit_failure(&encoded);
}

fn protocol_error_response(
    format: ApiFormat,
    status: StatusCode,
    message: &str,
    upstream: Option<&serde_json::Value>,
) -> axum::response::Response {
    (
        status,
        axum::Json(format_error(format, status, message, upstream)),
    )
        .into_response()
}

fn protocol_error_from(format: ApiFormat, error: ProtocolError) -> axum::response::Response {
    (
        error.status,
        axum::Json(format_protocol_error(format, &error, None)),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::{check_auth, extract_client_key_id, rewrite_claude_desktop_model};
    use crate::gateway::protocol::{
        ApiFormat, MaterializeSpec, materialize_parsed_request, parse_client_request,
        prepare_request,
    };
    use crate::gateway_keys::{CredentialEntry, CredentialSnapshot, PRIMARY_KEY_ID};
    use crate::models::{AppConfig, CLAUDE_DESKTOP_OPUS_ALIAS, ClaudeDesktopModels};
    use crate::state::{CoreState, CoreStateInner};
    use axum::body::Bytes;
    use axum::http::{HeaderMap, HeaderValue};
    use serde_json::json;
    use std::collections::HashMap;

    /// Owns the temp data dir and releases the SQLite connection (and thus
    /// the open database file) before removing the directory on Windows.
    struct StateDir {
        state: Option<CoreState>,
        dir: Option<std::path::PathBuf>,
    }

    impl std::ops::Deref for StateDir {
        type Target = CoreState;
        fn deref(&self) -> &CoreState {
            self.state.as_ref().expect("state present during use")
        }
    }

    impl Drop for StateDir {
        fn drop(&mut self) {
            self.state.take();
            if let Some(dir) = self.dir.take() {
                std::fs::remove_dir_all(dir).ok();
            }
        }
    }

    fn state_with_snapshot() -> StateDir {
        let mut dir = std::env::temp_dir();
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock should be valid")
            .as_nanos();
        dir.push(format!("ocg-auth-matrix-{nanos}"));
        std::fs::create_dir_all(&dir).expect("test data directory should be created");
        let db = crate::db::Database::open(dir.clone()).expect("test database should open");
        let cipher: std::sync::Arc<dyn crate::crypto::KeyCipher + Send + Sync> =
            std::sync::Arc::new(crate::crypto::StaticKeyCipher::new("test"));
        let state = CoreStateInner::new(db, dir.clone(), cipher).expect("state should initialize");
        StateDir {
            state: Some(std::sync::Arc::new(state)),
            dir: Some(dir),
        }
    }

    fn entry(id: &str, name: &str) -> CredentialEntry {
        CredentialEntry {
            id: id.to_string(),
            name: name.to_string(),
        }
    }

    fn snapshot() -> CredentialSnapshot {
        HashMap::from([
            ("ocg-primary".to_string(), entry(PRIMARY_KEY_ID, "Primary")),
            ("ocg-laptop".to_string(), entry("laptop", "Laptop")),
        ])
    }

    #[test]
    fn auth_matrix_across_headers_credentials_and_states() {
        let state = state_with_snapshot();
        *state.credential_snapshot.write() = snapshot();
        let cases = [
            // (header name, presented value, expected key id)
            ("authorization", "Bearer ocg-primary", PRIMARY_KEY_ID),
            ("authorization", "Bearer ocg-laptop", "laptop"),
            ("x-api-key", "ocg-laptop", "laptop"),
            ("x-goog-api-key", "ocg-primary", PRIMARY_KEY_ID),
            ("authorization", "Bearer wrong-key", ""),
            ("authorization", "Bearer ", ""),
            ("x-api-key", "", ""),
            ("x-goog-api-key", "   ", ""),
        ];
        for (header, presented, expected) in cases {
            let mut headers = HeaderMap::new();
            headers.insert(
                axum::http::HeaderName::from_static(header),
                HeaderValue::from_str(presented).expect("test header value should be valid"),
            );
            let matched = extract_client_key_id(&headers, &state);
            if expected.is_empty() {
                assert!(
                    matched.is_none(),
                    "{header}: {presented} should not authenticate"
                );
            } else {
                assert_eq!(
                    matched.as_deref(),
                    Some(expected),
                    "{header}: {presented} should match {expected}"
                );
            }
        }

        let no_headers = HeaderMap::new();
        assert!(extract_client_key_id(&no_headers, &state).is_none());
    }

    #[test]
    fn wrong_x_api_key_alongside_correct_x_goog_api_key_passes() {
        let state = state_with_snapshot();
        *state.credential_snapshot.write() = snapshot();
        let mut headers = HeaderMap::new();
        headers.insert("x-api-key", HeaderValue::from_static("wrong-key"));
        headers.insert("x-goog-api-key", HeaderValue::from_static("ocg-laptop"));
        assert!(check_auth(&headers, &state));
        assert_eq!(
            extract_client_key_id(&headers, &state).as_deref(),
            Some("laptop")
        );

        // Bearer wins attribution when several candidates hit: it comes first
        // in the fixed candidate order.
        headers.insert(
            "authorization",
            HeaderValue::from_static("Bearer ocg-primary"),
        );
        assert_eq!(
            extract_client_key_id(&headers, &state).as_deref(),
            Some(PRIMARY_KEY_ID)
        );

        // Two wrong candidates still fail.
        let mut wrong = HeaderMap::new();
        wrong.insert("x-api-key", HeaderValue::from_static("wrong-key"));
        wrong.insert("x-goog-api-key", HeaderValue::from_static("also-wrong"));
        assert!(!check_auth(&wrong, &state));
    }

    #[test]
    fn bearer_without_prefix_falls_back_to_api_key_headers() {
        let state = state_with_snapshot();
        *state.credential_snapshot.write() = snapshot();
        let mut headers = HeaderMap::new();
        headers.insert("authorization", HeaderValue::from_static("ocg-primary"));
        assert!(extract_client_key_id(&headers, &state).is_none());
        headers.insert("x-api-key", HeaderValue::from_static("ocg-laptop"));
        assert_eq!(
            extract_client_key_id(&headers, &state).as_deref(),
            Some("laptop")
        );
    }

    #[test]
    fn disabled_and_deleted_sub_keys_leave_the_snapshot() {
        // The snapshot only ever contains the primary value and enabled
        // non-deleted sub keys; disabling or soft-deleting removes the entry
        // (covered end to end by the key lifecycle integration tests).
        let state = state_with_snapshot();
        let mut snapshot = snapshot();
        assert!(snapshot.remove("ocg-laptop").is_some());
        *state.credential_snapshot.write() = snapshot;
        let mut headers = HeaderMap::new();
        headers.insert("x-api-key", HeaderValue::from_static("ocg-laptop"));
        assert!(!check_auth(&headers, &state));
    }

    #[test]
    fn claude_desktop_alias_is_rewritten_before_messages_preparation() {
        let models = ClaudeDesktopModels {
            sonnet: "glm-5.2".to_string(),
            opus: String::new(),
            haiku: String::new(),
        };
        let body = Bytes::from(
            serde_json::to_vec(&json!({
                "model": CLAUDE_DESKTOP_OPUS_ALIAS,
                "max_tokens": 1,
                "messages": [{"role":"user","content":"hi"}]
            }))
            .expect("test request should serialize"),
        );

        let rewritten =
            rewrite_claude_desktop_model(&body, &models).expect("known alias should be rewritten");
        let plan = prepare_request(ApiFormat::Messages, rewritten)
            .expect("rewritten request should use the existing preparation path");

        assert_eq!(plan.model, "glm-5.2");
        // glm-5.2 supports Messages natively (live probe); alias rewrite keeps Messages.
        assert_eq!(plan.upstream, ApiFormat::Messages);

        let parsed = parse_client_request(ApiFormat::Messages, body).expect("parse once");
        assert_eq!(parsed.requested_model, CLAUDE_DESKTOP_OPUS_ALIAS);
        let mapped = models
            .model_for_alias(&parsed.requested_model)
            .expect("opus inherits sonnet");
        let resolved = crate::alias::resolve(mapped).expect("mapped Go alias");
        assert!(matches!(
            resolved,
            crate::alias::ResolvedModel::Alias {
                alias: "glm-5.2",
                ..
            }
        ));
        let plan = materialize_parsed_request(
            &parsed,
            &MaterializeSpec {
                client_model: parsed.requested_model.clone(),
                upstream_model: mapped.to_string(),
                channel: crate::models::UpstreamChannel::Go,
                upstream_base_override: None,
                original_model: None,
                allow_go_fallback: false,
            },
        )
        .expect("Claude Desktop keeps the original alias as client_model");
        assert_eq!(plan.model, "glm-5.2");
        assert_eq!(plan.client_model, CLAUDE_DESKTOP_OPUS_ALIAS);
        assert_eq!(plan.upstream, ApiFormat::Messages);
    }

    #[test]
    fn app_config_still_compiles_without_a_key_list() {
        // Compile-time guard: the config shape no longer embeds key entries.
        let config = AppConfig {
            gateway_key: "k".into(),
            ..AppConfig::default()
        };
        config.validate().expect("scalar-key config validates");
    }
}
