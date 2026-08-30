//! Typed Core/desktop seam for managed local application connectors.
use serde::Serialize;
use std::{
    collections::BTreeMap,
    fmt,
    path::PathBuf,
    str::FromStr,
    sync::{Arc, OnceLock},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ApplicationConnectorId {
    ClaudeCode,
    Codex,
    Dsh,
    GeminiCli,
    #[serde(rename = "opencode")]
    OpenCode,
    #[serde(rename = "openclaw")]
    OpenClaw,
    Pi,
    Hermes,
}
impl ApplicationConnectorId {
    pub const ALL: [Self; 8] = [
        Self::ClaudeCode,
        Self::Codex,
        Self::Dsh,
        Self::GeminiCli,
        Self::OpenCode,
        Self::OpenClaw,
        Self::Pi,
        Self::Hermes,
    ];

    pub fn uses_native_credentials(self) -> bool {
        matches!(self, Self::Pi)
    }
}
impl FromStr for ApplicationConnectorId {
    type Err = ApplicationConnectorError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "claude-code" => Ok(Self::ClaudeCode),
            "codex" => Ok(Self::Codex),
            "dsh" => Ok(Self::Dsh),
            "gemini-cli" => Ok(Self::GeminiCli),
            "opencode" => Ok(Self::OpenCode),
            "openclaw" => Ok(Self::OpenClaw),
            "pi" => Ok(Self::Pi),
            "hermes" => Ok(Self::Hermes),
            _ => Err(ApplicationConnectorError::new(
                ApplicationConnectorErrorKind::InvalidRequest,
                "unknown application connector",
            )),
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ApplicationConnectorAction {
    Connect,
    Restore,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ApplicationConnectorStatus {
    UnsupportedRuntime,
    NotDetected,
    ManualOnly,
    Ready,
    Connected,
    Conflict,
    Partial,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplicationConnectorInspection {
    pub id: ApplicationConnectorId,
    pub status: ApplicationConnectorStatus,
    pub automatic: bool,
    pub detected: bool,
    pub detail: Option<String>,
    pub target_paths: Vec<String>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplicationConnectorChange {
    pub field: String,
    pub before: Option<String>,
    pub after: Option<String>,
    pub sensitive: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplicationConnectorPreview {
    pub id: ApplicationConnectorId,
    pub action: ApplicationConnectorAction,
    pub status: ApplicationConnectorStatus,
    pub fingerprint: String,
    pub detail: Option<String>,
    pub target_paths: Vec<String>,
    pub changes: Vec<ApplicationConnectorChange>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApplicationConnectorCommit {
    pub id: ApplicationConnectorId,
    pub action: ApplicationConnectorAction,
    pub key_id: Option<String>,
    pub model_values: BTreeMap<String, String>,
    pub preview_fingerprint: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplicationConnectorCommitResult {
    pub inspection: ApplicationConnectorInspection,
    pub changed: bool,
}
#[derive(Clone)]
pub struct ApplicationConnectorSecret(String);
impl ApplicationConnectorSecret {
    pub(crate) fn new(v: String) -> Self {
        Self(v)
    }
    pub fn expose_to_host(&self) -> &str {
        &self.0
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApplicationConnectorHostOperation {
    List,
    Preview,
    Commit,
}
pub struct ApplicationConnectorHostRequest {
    pub operation: ApplicationConnectorHostOperation,
    pub id: ApplicationConnectorId,
    pub action: ApplicationConnectorAction,
    pub key_id: Option<String>,
    pub secret: Option<ApplicationConnectorSecret>,
    pub model_values: BTreeMap<String, String>,
    pub gateway_url: String,
    pub data_dir: PathBuf,
    pub desktop_executable: Option<PathBuf>,
    pub preview_fingerprint: Option<String>,
}
pub enum ApplicationConnectorHostResult {
    Inspections(Vec<ApplicationConnectorInspection>),
    Preview(ApplicationConnectorPreview),
    Committed(ApplicationConnectorCommitResult),
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApplicationConnectorErrorKind {
    UnsupportedRuntime,
    InvalidRequest,
    NotFound,
    Conflict,
    Precondition,
    Internal,
}
#[derive(Debug)]
pub struct ApplicationConnectorError {
    kind: ApplicationConnectorErrorKind,
    message: String,
}
impl ApplicationConnectorError {
    pub fn new(kind: ApplicationConnectorErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }
    pub fn kind(&self) -> ApplicationConnectorErrorKind {
        self.kind
    }
}
impl fmt::Display for ApplicationConnectorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}
impl std::error::Error for ApplicationConnectorError {}
pub type ApplicationConnectorResult<T> = Result<T, ApplicationConnectorError>;
pub type ApplicationConnectorHost = Arc<
    dyn Fn(
            ApplicationConnectorHostRequest,
        ) -> ApplicationConnectorResult<ApplicationConnectorHostResult>
        + Send
        + Sync
        + 'static,
>;

/// Process-level Host hook storage for local application connectors.
///
/// The Core process owns one immutable registration. Headless runtimes leave
/// it unset and fail closed without importing desktop-specific code here.
pub struct ApplicationConnectorCapabilities {
    host: OnceLock<ApplicationConnectorHost>,
    executable: OnceLock<PathBuf>,
}

impl ApplicationConnectorCapabilities {
    pub fn new() -> Self {
        Self {
            host: OnceLock::new(),
            executable: OnceLock::new(),
        }
    }

    pub fn set_host(&self, host: ApplicationConnectorHost, executable: PathBuf) {
        assert!(
            self.host.set(host).is_ok(),
            "application connector Host is already configured"
        );
        assert!(
            self.executable.set(executable).is_ok(),
            "application connector executable is already configured"
        );
    }

    pub fn supported(&self) -> bool {
        self.host.get().is_some()
    }

    pub fn executable(&self) -> Option<PathBuf> {
        self.executable.get().cloned()
    }

    pub fn call(
        &self,
        request: ApplicationConnectorHostRequest,
    ) -> ApplicationConnectorResult<ApplicationConnectorHostResult> {
        let host = self.host.get().ok_or_else(|| {
            ApplicationConnectorError::new(
                ApplicationConnectorErrorKind::UnsupportedRuntime,
                "application connectors require the desktop Host",
            )
        })?;
        host(request)
    }
}

impl Default for ApplicationConnectorCapabilities {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn connector_registry_is_the_fixed_eight_client_surface() {
        let ids = ApplicationConnectorId::ALL
            .into_iter()
            .map(|id| serde_json::to_value(id).unwrap())
            .collect::<Vec<_>>();
        assert_eq!(
            ids,
            vec![
                "claude-code",
                "codex",
                "dsh",
                "gemini-cli",
                "opencode",
                "openclaw",
                "pi",
                "hermes",
            ]
            .into_iter()
            .map(serde_json::Value::from)
            .collect::<Vec<_>>()
        );
        assert!(ApplicationConnectorId::from_str("claude-desktop").is_err());
        assert!(ApplicationConnectorId::Pi.uses_native_credentials());
        assert!(!ApplicationConnectorId::Dsh.uses_native_credentials());
    }
}
