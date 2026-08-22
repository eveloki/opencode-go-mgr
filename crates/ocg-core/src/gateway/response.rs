//! Shared Gateway protocol error responses.
//!
//! Handler and executor both emit these client-facing shapes. Keeping the
//! helpers here avoids a handler <-> executor import cycle.

use crate::gateway::diagnostics::{
    ErrorDiagnostic, RequestTrace, emit_failure, serialize_diagnostic,
};
use crate::gateway::protocol::{ProtocolError, format_error, format_protocol_error};
use crate::kernel::protocol::ApiFormat;
use crate::state::CoreState;
use axum::http::StatusCode;
use axum::response::IntoResponse;

pub(crate) fn local_protocol_failure(
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

pub(crate) fn protocol_error_response(
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

pub(crate) fn protocol_error_from(
    format: ApiFormat,
    error: ProtocolError,
) -> axum::response::Response {
    (
        error.status,
        axum::Json(format_protocol_error(format, &error, None)),
    )
        .into_response()
}
