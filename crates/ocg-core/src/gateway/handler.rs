use crate::gateway::diagnostics::{
    ErrorDiagnostic, REQUEST_ID_HEADER, RequestTrace, emit_failure, serialize_diagnostic,
};
use crate::gateway::forwarder::{
    ForwardAction, UpstreamPayloadTooLargeResponse, forward_get, forward_request,
    rate_limited_response,
};
use crate::gateway::free_models::{
    apply_shared_free_exhaustion, decide_route, is_free_model, resolve_upstream_base,
    rewrite_body_model,
};
use crate::gateway::protocol::{
    ApiFormat, ProtocolError, RequestPlan, format_error, prepare_gemini_request, prepare_request,
};
use crate::gateway::routing::resolve_conversation_key;
use crate::gateway::selector::AccountSelector;
use crate::models::UpstreamChannel;
use crate::models::{
    AppConfig, CLAUDE_DESKTOP_HAIKU_ALIAS, CLAUDE_DESKTOP_OPUS_ALIAS, CLAUDE_DESKTOP_SONNET_ALIAS,
    ClaudeDesktopModels,
};
use crate::state::CoreState;
use axum::body::{Body, Bytes};
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
    let authenticated = check_auth(request.headers(), &state.config());
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
    if !check_auth(&headers, &state.config()) {
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

/// GET /v1/models — passthrough, any enabled account's key works.
pub async fn models(
    State(state): State<CoreState>,
    Extension(trace): Extension<RequestTrace>,
    headers: HeaderMap,
) -> axum::response::Response {
    if !check_auth(&headers, &state.config()) {
        return protocol_error_response(
            ApiFormat::ChatCompletions,
            StatusCode::UNAUTHORIZED,
            "invalid gateway key",
            None,
        );
    }

    let (config, client) = state.upstream_context();
    match forward_get(&client, &state, &config, "/v1/models").await {
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

    if !check_auth(&headers, &config) {
        return protocol_error_response(
            client_format,
            StatusCode::UNAUTHORIZED,
            "invalid gateway key",
            None,
        );
    }

    let body = if claude_desktop {
        match rewrite_claude_desktop_model(&body, &config.claude_desktop_models) {
            Ok(body) => body,
            Err(error) => {
                return local_failure_response(
                    &state,
                    &trace,
                    ApiFormat::Messages,
                    error.status,
                    &error.message,
                    "client",
                    "validation",
                    Some(client_body_bytes),
                    Some(&body),
                );
            }
        }
    } else {
        body
    };
    // Keep the post-rewrite body so its model always matches the initial plan's
    // model; that lets route finalization reuse the parsed plan instead of
    // re-parsing the body on every request.
    let client_body = body.clone();
    let plan = match prepare_request(client_format, body) {
        Ok(plan) => plan,
        Err(error) => {
            let stage = if error.message.starts_with("invalid JSON request") {
                "parse"
            } else {
                "validation"
            };
            return local_failure_response(
                &state,
                &trace,
                client_format,
                error.status,
                &error.message,
                "client",
                stage,
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
        plan,
        config,
        client,
    )
    .await
}

fn rewrite_claude_desktop_model(
    body: &Bytes,
    models: &ClaudeDesktopModels,
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
    if !check_auth(&headers, &config) {
        return protocol_error_response(
            ApiFormat::Gemini,
            StatusCode::UNAUTHORIZED,
            "invalid gateway key",
            None,
        );
    }
    let plan = match prepare_gemini_request(model, stream, body.clone()) {
        Ok(plan) => plan,
        Err(error) => {
            let stage = if error.message.starts_with("invalid JSON request") {
                "parse"
            } else {
                "validation"
            };
            return local_failure_response(
                &state,
                &trace,
                ApiFormat::Gemini,
                error.status,
                &error.message,
                "client",
                stage,
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
        plan,
        config,
        client,
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
    plan: RequestPlan,
    config: AppConfig,
    client: reqwest::Client,
) -> axum::response::Response {
    // One logical client request, including safe retries and account fallback,
    // must use one immutable pricing revision from start to finish.
    let pricing_snapshot = state.pricing_snapshot();
    let conversation_key = if config.conversation_sticky {
        resolve_conversation_key(client_format, &plan.model, &headers, &client_body)
    } else {
        None
    };
    // The client body's model, already extracted by the initial prepare_request.
    let body_model = plan.model.clone();

    let plan = match apply_free_routing_policy(
        &config,
        client_format,
        plan,
        &client_body,
        &body_model,
        &state,
        conversation_key.as_deref(),
    ) {
        Ok(plan) => plan,
        Err((status, message)) => {
            return local_failure_response(
                &state,
                &trace,
                client_format,
                status,
                &message,
                "client",
                "validation",
                Some(client_body.len()),
                Some(&client_body),
            );
        }
    };

    let mut last_error: Option<String> = None;
    let mut failed_ids: Vec<String> = Vec::new();
    let mut attempt = 0u32;
    let mut active_plan = plan;
    let mut free_fallback_used = false;

    loop {
        if active_plan.channel == UpstreamChannel::Free {
            let free_cooldown = match state.db.lock().free_channel_cooldown_until() {
                Ok(cooldown) => cooldown,
                Err(error) => {
                    let message = format!("failed to read free-channel cooldown: {error}");
                    record_plan_failure(
                        &state,
                        &trace,
                        &client_body,
                        attempt.max(1),
                        client_format,
                        &active_plan,
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
            if let Some(until) = free_cooldown {
                if let Some(outcome) = begin_go_fallback(
                    &state,
                    &trace,
                    client_format,
                    &client_body,
                    &active_plan,
                    &config,
                    attempt.max(1),
                    &mut free_fallback_used,
                ) {
                    match outcome {
                        Ok(go_plan) => {
                            failed_ids.clear();
                            active_plan = go_plan;
                            continue;
                        }
                        Err(message) => {
                            record_plan_failure(
                                &state,
                                &trace,
                                &client_body,
                                attempt.max(1),
                                client_format,
                                &active_plan,
                                "gateway",
                                "free_fallback",
                                StatusCode::BAD_REQUEST,
                                &message,
                            );
                            return protocol_error_response(
                                client_format,
                                StatusCode::BAD_REQUEST,
                                &message,
                                None,
                            );
                        }
                    }
                }
                record_plan_failure(
                    &state,
                    &trace,
                    &client_body,
                    attempt.max(1),
                    client_format,
                    &active_plan,
                    "gateway",
                    "account_selection",
                    StatusCode::TOO_MANY_REQUESTS,
                    "free channel is rate-limited",
                );
                return rate_limited_response(client_format, until);
            }
        }

        let account = {
            let accounts = match state.db.lock().list_accounts() {
                Ok(accounts) => accounts,
                Err(e) => {
                    let message = format!("failed to select account: {e}");
                    record_plan_failure(
                        &state,
                        &trace,
                        &client_body,
                        attempt.max(1),
                        client_format,
                        &active_plan,
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
            let excluded = failed_ids.iter().map(String::as_str).collect::<Vec<_>>();
            match state.routing.select_account_for(
                &accounts,
                config.routing_mode,
                config.conversation_sticky,
                conversation_key.as_deref(),
                active_plan.channel,
                &active_plan.model,
                &excluded,
            ) {
                Some(a) => a,
                None => {
                    // Prefer mode: free pool exhausted -> fall back to original Go model once.
                    if let Some(outcome) = begin_go_fallback(
                        &state,
                        &trace,
                        client_format,
                        &client_body,
                        &active_plan,
                        &config,
                        attempt.max(1),
                        &mut free_fallback_used,
                    ) {
                        match outcome {
                            Ok(go_plan) => {
                                failed_ids.clear();
                                active_plan = go_plan;
                                continue;
                            }
                            Err(message) => {
                                record_plan_failure(
                                    &state,
                                    &trace,
                                    &client_body,
                                    attempt.max(1),
                                    client_format,
                                    &active_plan,
                                    "gateway",
                                    "free_fallback",
                                    StatusCode::BAD_REQUEST,
                                    &message,
                                );
                                return protocol_error_response(
                                    client_format,
                                    StatusCode::BAD_REQUEST,
                                    &message,
                                    None,
                                );
                            }
                        }
                    }

                    let soonest = state.db.lock().soonest_cooldown_reset().ok().flatten();
                    return match soonest {
                        Some(until) => {
                            record_plan_failure(
                                &state,
                                &trace,
                                &client_body,
                                attempt.max(1),
                                client_format,
                                &active_plan,
                                "gateway",
                                "account_selection",
                                StatusCode::TOO_MANY_REQUESTS,
                                "all accounts are rate-limited",
                            );
                            rate_limited_response(client_format, until)
                        }
                        None => {
                            let msg =
                                last_error.unwrap_or_else(|| "no available accounts".to_string());
                            record_plan_failure(
                                &state,
                                &trace,
                                &client_body,
                                attempt.max(1),
                                client_format,
                                &active_plan,
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
                }
            }
        };

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
                        if let Some(outcome) = begin_go_fallback(
                            &state,
                            &trace,
                            client_format,
                            &client_body,
                            &active_plan,
                            &config,
                            attempt,
                            &mut free_fallback_used,
                        ) {
                            match outcome {
                                Ok(go_plan) => {
                                    failed_ids.clear();
                                    active_plan = go_plan;
                                    break;
                                }
                                Err(_) => return result.response,
                            }
                        }
                        return result.response;
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

fn apply_free_routing_policy(
    config: &AppConfig,
    client_format: ApiFormat,
    plan: RequestPlan,
    client_body: &Bytes,
    body_model: &str,
    state: &CoreState,
    conversation_key: Option<&str>,
) -> Result<RequestPlan, (StatusCode, String)> {
    let db = state.db.lock();
    let accounts = db.list_accounts().map_err(|error| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("failed to list accounts for free-channel routing: {error}"),
        )
    })?;
    let global_free_cooldown = db.free_channel_cooldown_until().map_err(|error| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("failed to read free-channel cooldown: {error}"),
        )
    })?;
    drop(db);
    let free_available =
        global_free_cooldown.is_none() && !AccountSelector::free_channel_exhausted(&accounts);

    // Sticky conversation locks channel + resolved model after the first request.
    // Prefer-mode sticky onto free is dropped once the IP-shared free pool is cooling.
    if let Some(key) = conversation_key {
        if let Some((_account_id, channel, resolved_model)) = state.routing.sticky_binding(key) {
            let drop_exhausted_prefer_sticky =
                channel == UpstreamChannel::Free && !free_available && !is_free_model(&plan.model);
            if !drop_exhausted_prefer_sticky {
                let original_model = plan.model.clone();
                return finalize_route_plan(
                    config,
                    client_format,
                    plan,
                    client_body,
                    body_model,
                    &resolved_model,
                    channel,
                    original_model,
                    None,
                    false,
                );
            }
        }
    }

    let decision = apply_shared_free_exhaustion(
        decide_route(
            config.free_model_routing,
            &plan.model,
            plan.client,
            plan.upstream,
            client_body,
        )
        .map_err(|message| (StatusCode::BAD_REQUEST, message))?,
        free_available,
    );

    finalize_route_plan(
        config,
        client_format,
        plan,
        client_body,
        body_model,
        &decision.model,
        decision.channel,
        decision.original_model,
        decision.mapped_from,
        decision.allow_go_fallback,
    )
}

#[allow(clippy::too_many_arguments)]
fn finalize_route_plan(
    config: &AppConfig,
    client_format: ApiFormat,
    initial_plan: RequestPlan,
    client_body: &Bytes,
    body_model: &str,
    model: &str,
    channel: UpstreamChannel,
    original_model: String,
    _mapped_from: Option<String>,
    allow_go_fallback: bool,
) -> Result<RequestPlan, (StatusCode, String)> {
    let base = resolve_upstream_base(channel, &config.upstream_base_url)
        .map_err(|message| (StatusCode::BAD_REQUEST, message))?;

    // When the routed model equals the body's model, the initial plan already
    // holds the parsed and converted request for this exact body; reusing it
    // avoids a full JSON parse + conversion per request. Only remapped routes
    // (free-tier aliases, sticky rebinds, Claude Desktop aliases) re-encode.
    let mut plan = if body_model != model {
        let body =
            rewrite_body_model(client_body, model).map_err(|m| (StatusCode::BAD_REQUEST, m))?;
        prepare_request(client_format, body).map_err(|error| (error.status, error.message))?
    } else {
        initial_plan
    };
    plan.model = model.to_string();
    plan.channel = channel;
    plan.upstream_base_override = match channel {
        UpstreamChannel::Free => Some(base),
        UpstreamChannel::Go => None,
    };
    plan.original_model = (original_model != model).then_some(original_model);
    plan.allow_go_fallback = allow_go_fallback;
    Ok(plan)
}

#[allow(clippy::too_many_arguments)]
fn begin_go_fallback(
    state: &CoreState,
    trace: &RequestTrace,
    client_format: ApiFormat,
    client_body: &Bytes,
    active_plan: &RequestPlan,
    config: &AppConfig,
    attempt: u32,
    free_fallback_used: &mut bool,
) -> Option<Result<RequestPlan, String>> {
    if active_plan.channel != UpstreamChannel::Free
        || !active_plan.allow_go_fallback
        || *free_fallback_used
    {
        return None;
    }
    *free_fallback_used = true;
    Some(
        rebuild_go_fallback_plan(client_format, client_body, active_plan, config).inspect(
            |go_plan| {
                let _ = state.db.lock().log_gateway_diagnostic(
                    "info",
                    "gateway",
                    &format!(
                        "free channel exhausted; falling back to Go model {} without rotating keys",
                        go_plan.model
                    ),
                    Some(&trace.request_id),
                    Some(attempt as i64),
                    Some("gateway"),
                    Some("free_fallback"),
                    Some(trace.elapsed_ms() as i64),
                    None,
                );
            },
        ),
    )
}

fn rebuild_go_fallback_plan(
    client_format: ApiFormat,
    client_body: &Bytes,
    free_plan: &RequestPlan,
    config: &AppConfig,
) -> Result<RequestPlan, String> {
    let original = free_plan
        .original_model
        .clone()
        .ok_or_else(|| "missing original model for free-to-Go fallback".to_string())?;
    let body = rewrite_body_model(client_body, &original)?;
    let mut plan = prepare_request(client_format, body).map_err(|error| error.message)?;
    plan.model = original;
    plan.channel = UpstreamChannel::Go;
    plan.upstream_base_override = None;
    plan.original_model = None;
    plan.allow_go_fallback = false;
    let _ = config;
    Ok(plan)
}

fn check_auth(headers: &HeaderMap, config: &AppConfig) -> bool {
    // Accept the standard bearer token plus Anthropic and Gemini SDK headers.
    let bearer_matches = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .map(|auth| {
            let expected = format!("Bearer {}", config.gateway_key);
            auth.trim() == expected
        })
        .unwrap_or(false);
    bearer_matches
        || ["x-api-key", "x-goog-api-key"].iter().any(|name| {
            headers
                .get(*name)
                .and_then(|v| v.to_str().ok())
                .is_some_and(|value| value.trim() == config.gateway_key)
        })
}

fn gemini_error(
    state: &CoreState,
    trace: &RequestTrace,
    headers: &HeaderMap,
    status: StatusCode,
    message: &str,
    client_body_bytes: Option<usize>,
) -> axum::response::Response {
    if !check_auth(headers, &state.config()) {
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
    if !check_auth(headers, &state.config()) {
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
    protocol_error_response(format, status, message, None)
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

#[cfg(test)]
mod tests {
    use super::{check_auth, rewrite_claude_desktop_model};
    use crate::gateway::protocol::{ApiFormat, prepare_request};
    use crate::models::{AppConfig, CLAUDE_DESKTOP_OPUS_ALIAS, ClaudeDesktopModels};
    use axum::body::Bytes;
    use axum::http::{HeaderMap, HeaderValue};
    use serde_json::json;

    #[test]
    fn gemini_api_key_header_is_accepted_and_wrong_key_is_rejected() {
        let config = AppConfig {
            gateway_key: "gateway-test-key".to_string(),
            ..AppConfig::default()
        };
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-goog-api-key",
            HeaderValue::from_static("gateway-test-key"),
        );
        assert!(check_auth(&headers, &config));

        headers.insert("x-goog-api-key", HeaderValue::from_static("wrong-key"));
        assert!(!check_auth(&headers, &config));
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
    }
}
