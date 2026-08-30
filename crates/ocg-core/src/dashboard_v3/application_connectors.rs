//! Local Desktop application connector control plane.
//!
//! The browser selects only a static client id, an existing Key id and model
//! aliases. Paths, Gateway URLs, config text and plaintext Keys never cross
//! this V3 response surface.

use axum::Json;
use axum::body::Bytes;
use axum::extract::{Path, State};
use std::str::FromStr;

use crate::application_connectors as core;
use crate::state::CoreState;

use super::types::{
    ApplicationConnectorAction, ApplicationConnectorChange, ApplicationConnectorCommitRequest,
    ApplicationConnectorCommitResult, ApplicationConnectorItem, ApplicationConnectorPreview,
    ApplicationConnectorPreviewRequest, ApplicationConnectorStatus, ApplicationConnectors,
};
use super::{V3ApiError, check_expectation, parse_json, parse_mutation_json};

pub(super) async fn list_connectors(
    State(state): State<CoreState>,
) -> Result<Json<ApplicationConnectors>, V3ApiError> {
    let items = if state.dashboard_local_mode() {
        match state.application_connectors() {
            Ok(items) => items.into_iter().map(connector_item).collect(),
            Err(error)
                if error.kind() == core::ApplicationConnectorErrorKind::UnsupportedRuntime =>
            {
                unsupported_items()
            }
            Err(error) => return Err(map_error(&state, error)),
        }
    } else {
        unsupported_items()
    };
    Ok(Json(ApplicationConnectors {
        items,
        revision: state.settings_revision(),
        process_generation: state.process_generation(),
    }))
}

pub(super) async fn preview_connector(
    State(state): State<CoreState>,
    Path(id): Path<String>,
    body: Bytes,
) -> Result<Json<ApplicationConnectorPreview>, V3ApiError> {
    ensure_local_desktop(&state)?;
    let input = parse_json::<ApplicationConnectorPreviewRequest>(&body)?;
    let id =
        core::ApplicationConnectorId::from_str(&id).map_err(|error| map_error(&state, error))?;
    let action = core_action(input.action);
    let preview = state
        .preview_application_connector(id, action, input.key_id.as_deref(), input.model_values)
        .map_err(|error| map_error(&state, error))?;
    Ok(Json(connector_preview(&state, preview)))
}

pub(super) async fn commit_connector(
    State(state): State<CoreState>,
    Path(id): Path<String>,
    body: Bytes,
) -> Result<Json<ApplicationConnectorCommitResult>, V3ApiError> {
    ensure_local_desktop(&state)?;
    let input = parse_mutation_json::<ApplicationConnectorCommitRequest>(&body)?;
    let id =
        core::ApplicationConnectorId::from_str(&id).map_err(|error| map_error(&state, error))?;

    let _settings_update = state.settings_update.lock();
    check_expectation(&state, &input.expectation)?;
    let result = state
        .commit_application_connector(core::ApplicationConnectorCommit {
            id,
            action: core_action(input.action),
            key_id: input.key_id,
            model_values: input.model_values,
            preview_fingerprint: input.preview_fingerprint,
        })
        .map_err(|error| map_error(&state, error))?;
    if result.changed {
        state.bump_settings_revision();
    }
    Ok(Json(ApplicationConnectorCommitResult {
        connector: connector_item(result.inspection),
        changed: result.changed,
        revision: state.settings_revision(),
        process_generation: state.process_generation(),
    }))
}

fn ensure_local_desktop(state: &CoreState) -> Result<(), V3ApiError> {
    if !state.dashboard_local_mode() {
        return Err(V3ApiError::forbidden_at(
            state,
            "application connectors are available only from the local Desktop dashboard",
        ));
    }
    if !state.application_connector_supported() {
        return Err(V3ApiError::not_implemented(
            state,
            "application connectors are unavailable in this runtime",
        ));
    }
    Ok(())
}

fn connector_item(value: core::ApplicationConnectorInspection) -> ApplicationConnectorItem {
    ApplicationConnectorItem {
        id: connector_id(value.id).to_string(),
        status: connector_status(value.status),
        detected: value.detected,
        automatic: value.automatic,
        detail: value.detail,
        target_paths: value.target_paths,
    }
}

fn connector_preview(
    state: &CoreState,
    value: core::ApplicationConnectorPreview,
) -> ApplicationConnectorPreview {
    ApplicationConnectorPreview {
        id: connector_id(value.id).to_string(),
        action: wire_action(value.action),
        status: connector_status(value.status),
        fingerprint: value.fingerprint,
        detail: value.detail,
        target_paths: value.target_paths,
        changes: value
            .changes
            .into_iter()
            .map(|change| ApplicationConnectorChange {
                field: change.field,
                before: change.before,
                after: change.after,
                sensitive: change.sensitive,
            })
            .collect(),
        revision: state.settings_revision(),
        process_generation: state.process_generation(),
    }
}

fn unsupported_items() -> Vec<ApplicationConnectorItem> {
    core::ApplicationConnectorId::ALL
        .into_iter()
        .map(|id| ApplicationConnectorItem {
            id: connector_id(id).to_string(),
            status: ApplicationConnectorStatus::UnsupportedRuntime,
            detected: false,
            automatic: false,
            detail: Some("available only in the installed local Desktop app".to_string()),
            target_paths: Vec::new(),
        })
        .collect()
}

fn connector_id(id: core::ApplicationConnectorId) -> &'static str {
    match id {
        core::ApplicationConnectorId::ClaudeCode => "claude-code",
        core::ApplicationConnectorId::Codex => "codex",
        core::ApplicationConnectorId::Dsh => "dsh",
        core::ApplicationConnectorId::GeminiCli => "gemini-cli",
        core::ApplicationConnectorId::OpenCode => "opencode",
        core::ApplicationConnectorId::OpenClaw => "openclaw",
        core::ApplicationConnectorId::Pi => "pi",
        core::ApplicationConnectorId::Hermes => "hermes",
    }
}

fn core_action(action: ApplicationConnectorAction) -> core::ApplicationConnectorAction {
    match action {
        ApplicationConnectorAction::Connect => core::ApplicationConnectorAction::Connect,
        ApplicationConnectorAction::Restore => core::ApplicationConnectorAction::Restore,
    }
}

fn wire_action(action: core::ApplicationConnectorAction) -> ApplicationConnectorAction {
    match action {
        core::ApplicationConnectorAction::Connect => ApplicationConnectorAction::Connect,
        core::ApplicationConnectorAction::Restore => ApplicationConnectorAction::Restore,
    }
}

fn connector_status(status: core::ApplicationConnectorStatus) -> ApplicationConnectorStatus {
    match status {
        core::ApplicationConnectorStatus::UnsupportedRuntime => {
            ApplicationConnectorStatus::UnsupportedRuntime
        }
        core::ApplicationConnectorStatus::NotDetected => ApplicationConnectorStatus::NotDetected,
        core::ApplicationConnectorStatus::ManualOnly => ApplicationConnectorStatus::ManualOnly,
        core::ApplicationConnectorStatus::Ready => ApplicationConnectorStatus::Ready,
        core::ApplicationConnectorStatus::Connected => ApplicationConnectorStatus::Connected,
        core::ApplicationConnectorStatus::Conflict => ApplicationConnectorStatus::Conflict,
        core::ApplicationConnectorStatus::Partial => ApplicationConnectorStatus::Partial,
    }
}

fn map_error(state: &CoreState, error: core::ApplicationConnectorError) -> V3ApiError {
    match error.kind() {
        core::ApplicationConnectorErrorKind::UnsupportedRuntime => {
            V3ApiError::not_implemented(state, error.to_string())
        }
        core::ApplicationConnectorErrorKind::InvalidRequest => {
            V3ApiError::invalid_request_at(state, error.to_string())
        }
        core::ApplicationConnectorErrorKind::NotFound => {
            V3ApiError::not_found_at(state, error.to_string())
        }
        core::ApplicationConnectorErrorKind::Conflict => {
            V3ApiError::conflict_at(state, error.to_string())
        }
        core::ApplicationConnectorErrorKind::Precondition => {
            V3ApiError::precondition_failed_at(state, error.to_string())
        }
        core::ApplicationConnectorErrorKind::Internal => V3ApiError::internal(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application_connectors::{
        ApplicationConnectorHost, ApplicationConnectorHostResult, ApplicationConnectorInspection,
        ApplicationConnectorStatus as CoreStatus,
    };
    use crate::crypto::{KeyCipher, StaticKeyCipher};
    use crate::db::Database;
    use crate::state::CoreStateInner;
    use serde_json::json;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    fn test_state(label: &str, local: bool) -> (std::path::PathBuf, CoreState) {
        let dir = std::env::temp_dir().join(format!(
            "ocg-v3-application-connectors-{label}-{}",
            uuid::Uuid::new_v4().simple()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let cipher: Arc<dyn KeyCipher + Send + Sync> =
            Arc::new(StaticKeyCipher::new("v3-application-connectors"));
        let state = Arc::new(
            CoreStateInner::new(Database::open(dir.clone()).unwrap(), dir.clone(), cipher).unwrap(),
        );
        state.set_dashboard_local_mode(local);
        (dir, state)
    }

    #[tokio::test]
    async fn headless_and_public_runtimes_list_eight_unsupported_connectors() {
        let (headless_dir, headless) = test_state("headless", true);
        let Json(headless_body) = match list_connectors(State(headless.clone())).await {
            Ok(body) => body,
            Err(_) => panic!("headless connector listing should succeed"),
        };
        assert_eq!(headless_body.items.len(), 8);
        assert!(headless_body.items.iter().all(|item| {
            item.status == ApplicationConnectorStatus::UnsupportedRuntime && !item.automatic
        }));

        let (public_dir, public) = test_state("public", false);
        let called = Arc::new(AtomicBool::new(false));
        let called_by_host = called.clone();
        let host: ApplicationConnectorHost = Arc::new(move |_| {
            called_by_host.store(true, Ordering::SeqCst);
            panic!("public connector listing must not call the Desktop Host")
        });
        public.set_application_connector_host(host, "ocg-manager".into());
        let Json(public_body) = match list_connectors(State(public.clone())).await {
            Ok(body) => body,
            Err(_) => panic!("public connector listing should fail closed as a read"),
        };
        assert_eq!(public_body.items.len(), 8);
        assert!(!called.load(Ordering::SeqCst));

        drop(headless);
        drop(public);
        let _ = std::fs::remove_dir_all(headless_dir);
        let _ = std::fs::remove_dir_all(public_dir);
    }

    #[tokio::test]
    async fn stale_commit_is_rejected_before_the_host_can_write() {
        let (dir, state) = test_state("stale", true);
        let called = Arc::new(AtomicBool::new(false));
        let called_by_host = called.clone();
        let host: ApplicationConnectorHost = Arc::new(move |_| {
            called_by_host.store(true, Ordering::SeqCst);
            Ok(ApplicationConnectorHostResult::Committed(
                crate::application_connectors::ApplicationConnectorCommitResult {
                    inspection: ApplicationConnectorInspection {
                        id: core::ApplicationConnectorId::ClaudeCode,
                        status: CoreStatus::Connected,
                        automatic: true,
                        detected: true,
                        detail: None,
                        target_paths: vec!["~/.claude/settings.json".into()],
                    },
                    changed: true,
                },
            ))
        });
        state.set_application_connector_host(host, "ocg-manager".into());
        let body = Bytes::from(
            serde_json::to_vec(&json!({
                "expectedRevision": state.settings_revision() + 1,
                "processGeneration": state.process_generation(),
                "action": "connect",
                "keyId": crate::gateway_keys::PRIMARY_KEY_ID,
                "modelValues": {"model": "gpt-5.6"},
                "previewFingerprint": "stale"
            }))
            .unwrap(),
        );
        let error = commit_connector(State(state.clone()), Path("claude-code".into()), body)
            .await
            .expect_err("stale CAS must fail");
        assert_eq!(error.status, axum::http::StatusCode::CONFLICT);
        assert!(!called.load(Ordering::SeqCst));

        drop(state);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn public_preview_is_forbidden_before_host_or_key_resolution() {
        let (dir, state) = test_state("public-preview", false);
        let called = Arc::new(AtomicBool::new(false));
        let called_by_host = called.clone();
        let host: ApplicationConnectorHost = Arc::new(move |_| {
            called_by_host.store(true, Ordering::SeqCst);
            Ok(ApplicationConnectorHostResult::Inspections(Vec::new()))
        });
        state.set_application_connector_host(host, "ocg-manager".into());
        let body = Bytes::from_static(
            br#"{"action":"connect","keyId":"missing","modelValues":{"model":"gpt-5.6"}}"#,
        );
        let error = preview_connector(State(state.clone()), Path("claude-code".into()), body)
            .await
            .expect_err("public listener must fail closed");
        assert_eq!(error.status, axum::http::StatusCode::FORBIDDEN);
        assert!(!called.load(Ordering::SeqCst));

        drop(state);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn native_plugin_preview_uses_client_credentials_without_resolving_a_key() {
        let (dir, state) = test_state("native-plugin-credentials", true);
        let called = Arc::new(AtomicBool::new(false));
        let called_by_host = called.clone();
        let host: ApplicationConnectorHost = Arc::new(move |request| {
            assert_eq!(request.id, core::ApplicationConnectorId::Pi);
            assert_eq!(request.action, core::ApplicationConnectorAction::Connect);
            assert!(request.key_id.is_none());
            assert!(request.secret.is_none());
            called_by_host.store(true, Ordering::SeqCst);
            Ok(ApplicationConnectorHostResult::Preview(
                crate::application_connectors::ApplicationConnectorPreview {
                    id: request.id,
                    action: request.action,
                    status: CoreStatus::Ready,
                    fingerprint: "native-preview".into(),
                    detail: None,
                    target_paths: vec!["Pi package manager".into()],
                    changes: Vec::new(),
                },
            ))
        });
        state.set_application_connector_host(host, "ocg-manager".into());
        let body = Bytes::from_static(
            br#"{"action":"connect","keyId":null,"modelValues":{"models":"gpt-5.6"}}"#,
        );
        let Json(preview) =
            match preview_connector(State(state.clone()), Path("pi".into()), body).await {
                Ok(preview) => preview,
                Err(_) => panic!("native plugin preview should not require a Core key"),
            };
        assert_eq!(preview.fingerprint, "native-preview");
        assert!(called.load(Ordering::SeqCst));

        drop(state);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn dsh_plugin_preview_resolves_the_selected_core_key() {
        let (dir, state) = test_state("dsh-managed-credential", true);
        let called = Arc::new(AtomicBool::new(false));
        let called_by_host = called.clone();
        let host: ApplicationConnectorHost = Arc::new(move |request| {
            assert_eq!(request.id, core::ApplicationConnectorId::Dsh);
            assert_eq!(request.action, core::ApplicationConnectorAction::Connect);
            assert_eq!(
                request.key_id.as_deref(),
                Some(crate::gateway_keys::PRIMARY_KEY_ID)
            );
            assert!(request.secret.is_some());
            called_by_host.store(true, Ordering::SeqCst);
            Ok(ApplicationConnectorHostResult::Preview(
                crate::application_connectors::ApplicationConnectorPreview {
                    id: request.id,
                    action: request.action,
                    status: CoreStatus::Ready,
                    fingerprint: "dsh-preview".into(),
                    detail: None,
                    target_paths: vec!["DSH web profile".into(), "DSH .env".into()],
                    changes: Vec::new(),
                },
            ))
        });
        state.set_application_connector_host(host, "ocg-manager".into());
        let body = Bytes::from(
            serde_json::to_vec(&json!({
                "action": "connect",
                "keyId": crate::gateway_keys::PRIMARY_KEY_ID,
                "modelValues": {"models": "gpt-5.6"}
            }))
            .unwrap(),
        );
        let Json(preview) =
            match preview_connector(State(state.clone()), Path("dsh".into()), body).await {
                Ok(preview) => preview,
                Err(_) => panic!("DSH preview should resolve the selected Core key"),
            };
        assert_eq!(preview.fingerprint, "dsh-preview");
        assert!(called.load(Ordering::SeqCst));

        drop(state);
        let _ = std::fs::remove_dir_all(dir);
    }
}
