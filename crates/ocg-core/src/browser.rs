use anyhow::{Context, Result, anyhow, bail};
use parking_lot::Mutex;
use reqwest::Url;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::io::Write;
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

const WORKER_URL_ENV: &str = "OCG_BROWSER_WORKER_URL";
const WORKER_TOKEN_FILE_ENV: &str = "OCG_BROWSER_CONTROL_TOKEN_FILE";
const PROFILES_DIR_ENV: &str = "OCG_BROWSER_PROFILES_DIR";
const PROFILE_TOMBSTONE_PREFIX: &str = ".ocg-profile-delete-";
const PROFILE_OPERATION_JOURNAL_DIR: &str = "browser-profile-operations";
const PROFILE_OPERATION_JOURNAL_VERSION: u32 = 1;
const DEFAULT_WORKER_TOKEN_FILE: &str = "/run/ocg-browser/control-token";
const SESSION_IDLE_TIMEOUT: Duration = Duration::from_secs(30 * 60);
const SESSION_MAX_LIFETIME: Duration = Duration::from_secs(4 * 60 * 60);

pub type NativeBrowserLauncher = Arc<dyn Fn(&str, &str) -> Result<()> + Send + Sync + 'static>;
pub type NativeBrowserStopper = Arc<dyn Fn(&str) -> Result<()> + Send + Sync + 'static>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum BrowserMode {
    Native,
    Remote,
    Unsupported,
}

#[derive(Debug, Clone, Serialize)]
pub struct BrowserCapabilities {
    pub mode: BrowserMode,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct BrowserOpenResult {
    pub mode: BrowserMode,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_token: Option<String>,
}

#[derive(Debug, Clone)]
struct RemoteWorkerConfig {
    base_url: Url,
    token_file: PathBuf,
}

#[derive(Debug, Deserialize)]
struct WorkerSessionResponse {
    vnc_ws_url: String,
}

#[derive(Debug, Deserialize)]
struct WorkerSessionStatus {
    active: bool,
    account_id: Option<String>,
}

#[derive(Debug, Clone)]
struct RemoteSession {
    account_id: String,
    binding: String,
    worker_ws_url: String,
    created_at: Instant,
    last_active: Instant,
    cancellation: tokio::sync::watch::Sender<bool>,
}

pub struct RemoteWebsocketSession {
    pub worker_ws_url: String,
    pub cancellation: tokio::sync::watch::Receiver<bool>,
}

pub struct BrowserRuntime {
    native_launcher: OnceLock<NativeBrowserLauncher>,
    native_stopper: OnceLock<NativeBrowserStopper>,
    native_unavailable_reason: OnceLock<String>,
    remote: std::result::Result<Option<RemoteWorkerConfig>, String>,
    sessions: Mutex<HashMap<String, RemoteSession>>,
    operations: tokio::sync::Mutex<()>,
    client: reqwest::Client,
}

pub struct BrowserOperation<'a> {
    runtime: &'a BrowserRuntime,
    _guard: tokio::sync::MutexGuard<'a, ()>,
}

impl BrowserRuntime {
    pub fn new() -> Self {
        let remote = remote_worker_from_env();
        let client = reqwest::Client::builder()
            .no_proxy()
            .connect_timeout(Duration::from_secs(5))
            // The worker may spend up to 60 seconds on a configured graceful
            // Chromium shutdown before forcing it. Leave room for that flush,
            // process startup, and the control response itself.
            .timeout(Duration::from_secs(75))
            .build()
            .expect("browser worker HTTP client should build");
        Self {
            native_launcher: OnceLock::new(),
            native_stopper: OnceLock::new(),
            native_unavailable_reason: OnceLock::new(),
            remote,
            sessions: Mutex::new(HashMap::new()),
            operations: tokio::sync::Mutex::new(()),
            client,
        }
    }

    pub fn register_native_hooks(
        &self,
        launcher: NativeBrowserLauncher,
        stopper: NativeBrowserStopper,
    ) -> Result<()> {
        self.native_launcher
            .set(launcher)
            .map_err(|_| anyhow!("native browser launcher is already registered"))?;
        self.native_stopper
            .set(stopper)
            .map_err(|_| anyhow!("native browser stopper is already registered"))?;
        Ok(())
    }

    pub fn register_native_unavailable_reason(&self, reason: String) -> Result<()> {
        self.native_unavailable_reason
            .set(reason)
            .map_err(|_| anyhow!("native browser unavailable reason is already registered"))
    }

    pub async fn capabilities(&self) -> BrowserCapabilities {
        if self.native_launcher.get().is_some() {
            return BrowserCapabilities {
                mode: BrowserMode::Native,
                reason: None,
            };
        }
        let remote = match &self.remote {
            Ok(Some(remote)) => remote,
            Ok(None) => {
                return BrowserCapabilities {
                    mode: BrowserMode::Unsupported,
                    reason: Some(
                        self.native_unavailable_reason
                            .get()
                            .cloned()
                            .unwrap_or_else(|| {
                                "no desktop Chromium launcher or remote browser worker is configured"
                                    .to_string()
                            }),
                    ),
                };
            }
            Err(error) => {
                return BrowserCapabilities {
                    mode: BrowserMode::Unsupported,
                    reason: Some(error.clone()),
                };
            }
        };
        match self
            .worker_request(remote, reqwest::Method::GET, "health", None)
            .await
        {
            Ok(_) => BrowserCapabilities {
                mode: BrowserMode::Remote,
                reason: None,
            },
            Err(error) => BrowserCapabilities {
                mode: BrowserMode::Unsupported,
                reason: Some(format!("remote browser worker is unavailable: {error}")),
            },
        }
    }

    pub async fn open(
        &self,
        account_id: &str,
        url: &str,
        binding: &str,
    ) -> Result<BrowserOpenResult> {
        self.operation().await.open(account_id, url, binding).await
    }

    pub async fn operation(&self) -> BrowserOperation<'_> {
        BrowserOperation {
            runtime: self,
            _guard: self.operations.lock().await,
        }
    }

    async fn open_inner(
        &self,
        account_id: &str,
        url: &str,
        binding: &str,
    ) -> Result<BrowserOpenResult> {
        validate_account_id(account_id)?;
        validate_browser_url(url)?;
        if let Some(launcher) = self.native_launcher.get() {
            launcher(account_id, url)?;
            return Ok(BrowserOpenResult {
                mode: BrowserMode::Native,
                session_token: None,
            });
        }
        let remote = self
            .remote
            .as_ref()
            .map_err(|error| anyhow!(error.clone()))?
            .as_ref()
            .ok_or_else(|| anyhow!("browser support is unavailable in this runtime"))?;
        // Invalidate the old view before asking the single-worker sidecar to
        // switch accounts. Even a malformed/lost worker response must never
        // leave the previous account's token able to view the new profile.
        self.invalidate_remote_sessions();
        let response = self
            .worker_request(
                remote,
                reqwest::Method::POST,
                "session",
                Some(serde_json::json!({ "account_id": account_id, "url": url })),
            )
            .await?;
        let worker: WorkerSessionResponse = response
            .json()
            .await
            .context("browser worker returned invalid session JSON")?;
        validate_worker_websocket_url(remote, &worker.vnc_ws_url)?;

        let token = uuid::Uuid::new_v4().simple().to_string();
        let now = Instant::now();
        let (cancellation, _) = tokio::sync::watch::channel(false);
        let mut sessions = self.sessions.lock();
        sessions.insert(
            token.clone(),
            RemoteSession {
                account_id: account_id.to_string(),
                binding: binding.to_string(),
                worker_ws_url: worker.vnc_ws_url,
                created_at: now,
                last_active: now,
                cancellation,
            },
        );
        Ok(BrowserOpenResult {
            mode: BrowserMode::Remote,
            session_token: Some(token),
        })
    }

    pub fn remote_websocket_session(
        &self,
        token: &str,
        binding: &str,
    ) -> Result<RemoteWebsocketSession> {
        let now = Instant::now();
        let mut sessions = self.sessions.lock();
        revoke_expired_sessions(&mut sessions, now);
        let session = sessions
            .get_mut(token)
            .ok_or_else(|| anyhow!("browser session is missing or expired"))?;
        if session.binding != binding {
            bail!("browser session belongs to a different administrator session");
        }
        session.last_active = now;
        Ok(RemoteWebsocketSession {
            worker_ws_url: session.worker_ws_url.clone(),
            cancellation: session.cancellation.subscribe(),
        })
    }

    pub fn touch_remote_session(&self, token: &str) -> bool {
        let now = Instant::now();
        let mut sessions = self.sessions.lock();
        revoke_expired_sessions(&mut sessions, now);
        if let Some(session) = sessions.get_mut(token) {
            session.last_active = now;
            true
        } else {
            false
        }
    }

    pub fn remote_session_active(&self, token: &str) -> bool {
        let now = Instant::now();
        let mut sessions = self.sessions.lock();
        revoke_expired_sessions(&mut sessions, now);
        sessions.contains_key(token)
    }

    pub fn invalidate_remote_sessions(&self) {
        let mut sessions = self.sessions.lock();
        for (_, session) in sessions.drain() {
            let _ = session.cancellation.send(true);
        }
    }

    pub async fn stop_account(&self, account_id: &str) -> Result<()> {
        self.operation().await.stop_account(account_id).await
    }

    async fn stop_account_inner(&self, account_id: &str) -> Result<()> {
        validate_account_id(account_id)?;
        if let Some(stopper) = self.native_stopper.get() {
            stopper(account_id)?;
        }
        if let Ok(Some(remote)) = &self.remote {
            // The compose file keeps the worker URL configured even when its
            // optional profile is stopped. In that state no Chromium process can
            // survive the container, and the profile lock check performed before
            // staging remains the final corruption guard.
            if let Ok(response) = self
                .worker_request(remote, reqwest::Method::GET, "session", None)
                .await
            {
                let status: WorkerSessionStatus = response
                    .json()
                    .await
                    .context("browser worker returned invalid session status JSON")?;
                if status.active && status.account_id.as_deref() == Some(account_id) {
                    self.worker_request(
                        remote,
                        reqwest::Method::DELETE,
                        "session",
                        Some(serde_json::json!({ "account_id": account_id })),
                    )
                    .await?;
                }
            }
        }
        self.sessions.lock().retain(|_, session| {
            let keep = session.account_id != account_id;
            if !keep {
                let _ = session.cancellation.send(true);
            }
            keep
        });
        Ok(())
    }

    async fn worker_request(
        &self,
        remote: &RemoteWorkerConfig,
        method: reqwest::Method,
        endpoint: &str,
        body: Option<serde_json::Value>,
    ) -> Result<reqwest::Response> {
        let token = tokio::fs::read_to_string(&remote.token_file)
            .await
            .with_context(|| {
                format!(
                    "failed to read browser worker control token {}",
                    remote.token_file.display()
                )
            })?;
        let token = token.trim();
        if token.len() < 32 || token.len() > 512 {
            bail!("browser worker control token has an invalid length");
        }
        let url = remote
            .base_url
            .join(endpoint)
            .context("failed to construct browser worker URL")?;
        let mut request = self.client.request(method, url).bearer_auth(token);
        if let Some(body) = body {
            request = request.json(&body);
        }
        let response = request
            .send()
            .await
            .context("browser worker request failed")?;
        if !response.status().is_success() {
            bail!("browser worker returned HTTP {}", response.status());
        }
        Ok(response)
    }
}

impl BrowserOperation<'_> {
    pub async fn open(
        &self,
        account_id: &str,
        url: &str,
        binding: &str,
    ) -> Result<BrowserOpenResult> {
        self.runtime.open_inner(account_id, url, binding).await
    }

    pub async fn stop_account(&self, account_id: &str) -> Result<()> {
        self.runtime.stop_account_inner(account_id).await
    }
}

impl Default for BrowserRuntime {
    fn default() -> Self {
        Self::new()
    }
}

fn session_expired(session: &RemoteSession, now: Instant) -> bool {
    now.duration_since(session.last_active) >= SESSION_IDLE_TIMEOUT
        || now.duration_since(session.created_at) >= SESSION_MAX_LIFETIME
}

fn revoke_expired_sessions(sessions: &mut HashMap<String, RemoteSession>, now: Instant) {
    sessions.retain(|_, session| {
        let keep = !session_expired(session, now);
        if !keep {
            let _ = session.cancellation.send(true);
        }
        keep
    });
}

fn remote_worker_from_env() -> std::result::Result<Option<RemoteWorkerConfig>, String> {
    let value = match std::env::var(WORKER_URL_ENV) {
        Ok(value) if !value.trim().is_empty() => value,
        Ok(_) | Err(std::env::VarError::NotPresent) => return Ok(None),
        Err(std::env::VarError::NotUnicode(_)) => {
            return Err(format!("{WORKER_URL_ENV} must contain valid Unicode"));
        }
    };
    let mut base_url =
        Url::parse(value.trim()).map_err(|error| format!("invalid {WORKER_URL_ENV}: {error}"))?;
    if !matches!(base_url.scheme(), "http" | "https")
        || base_url.host_str().is_none()
        || !base_url.username().is_empty()
        || base_url.password().is_some()
        || base_url.query().is_some()
        || base_url.fragment().is_some()
    {
        return Err(format!(
            "{WORKER_URL_ENV} must be an absolute http(s) URL without credentials, query, or fragment"
        ));
    }
    if base_url.path() != "/" && !base_url.path().is_empty() {
        return Err(format!("{WORKER_URL_ENV} must not include a path"));
    }
    base_url.set_path("/");
    let token_file = match std::env::var(WORKER_TOKEN_FILE_ENV) {
        Ok(value) if !value.trim().is_empty() => PathBuf::from(value),
        Ok(_) | Err(std::env::VarError::NotPresent) => PathBuf::from(DEFAULT_WORKER_TOKEN_FILE),
        Err(std::env::VarError::NotUnicode(_)) => {
            return Err(format!(
                "{WORKER_TOKEN_FILE_ENV} must contain valid Unicode"
            ));
        }
    };
    if !token_file.is_absolute() {
        return Err(format!("{WORKER_TOKEN_FILE_ENV} must be an absolute path"));
    }
    Ok(Some(RemoteWorkerConfig {
        base_url,
        token_file,
    }))
}

fn validate_worker_websocket_url(remote: &RemoteWorkerConfig, value: &str) -> Result<()> {
    let url = Url::parse(value).context("browser worker returned an invalid WebSocket URL")?;
    if !matches!(url.scheme(), "ws" | "wss")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
    {
        bail!("browser worker returned an unsafe WebSocket URL");
    }
    if url.host_str() != remote.base_url.host_str() {
        bail!("browser worker WebSocket host does not match its control host");
    }
    Ok(())
}

pub fn validate_browser_url(value: &str) -> Result<()> {
    let url = Url::parse(value).context("invalid browser URL")?;
    if url.scheme() != "https"
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        bail!("browser URL must be an absolute HTTPS URL without credentials");
    }
    Ok(())
}

pub fn validate_account_id(account_id: &str) -> Result<()> {
    if account_id.is_empty()
        || account_id.len() > 128
        || !account_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        bail!("invalid account id for browser profile");
    }
    let mut components = Path::new(account_id).components();
    if !matches!(components.next(), Some(Component::Normal(_))) || components.next().is_some() {
        bail!("invalid account id for browser profile");
    }
    Ok(())
}

pub fn browser_profile_roots(data_dir: &Path) -> Result<Vec<PathBuf>> {
    match std::env::var(PROFILES_DIR_ENV) {
        Ok(value) => browser_profile_roots_with_override(data_dir, Some(&value)),
        Err(std::env::VarError::NotPresent) => browser_profile_roots_with_override(data_dir, None),
        Err(std::env::VarError::NotUnicode(_)) => {
            bail!("{PROFILES_DIR_ENV} must contain valid Unicode")
        }
    }
}

fn browser_profile_roots_with_override(
    data_dir: &Path,
    profiles_dir_override: Option<&str>,
) -> Result<Vec<PathBuf>> {
    let new_root = match profiles_dir_override {
        Some(value) if !value.trim().is_empty() => {
            let path = PathBuf::from(value);
            if !path.is_absolute() {
                bail!("{PROFILES_DIR_ENV} must be an absolute path");
            }
            path
        }
        Some(_) | None => data_dir.join("browser-profiles"),
    };
    let mut roots = vec![new_root];
    let legacy = data_dir.join("profiles");
    if legacy != roots[0] {
        roots.push(legacy);
    }
    Ok(roots)
}

pub fn browser_profile_paths(data_dir: &Path, account_id: &str) -> Result<Vec<PathBuf>> {
    validate_account_id(account_id)?;
    Ok(browser_profile_roots(data_dir)?
        .into_iter()
        .map(|root| root.join(account_id))
        .collect())
}

/// Resolves browser profile paths with an explicit current-root override.
/// This is primarily useful for deterministic callers/tests that must exercise
/// the same external-root semantics without mutating process-global env state.
pub fn browser_profile_paths_with_override(
    data_dir: &Path,
    account_id: &str,
    profiles_dir_override: Option<&str>,
) -> Result<Vec<PathBuf>> {
    validate_account_id(account_id)?;
    Ok(
        browser_profile_roots_with_override(data_dir, profiles_dir_override)?
            .into_iter()
            .map(|root| root.join(account_id))
            .collect(),
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BrowserProfileOperationKind {
    DeleteAccount,
    ResetProfile,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct BrowserProfileOperationPath {
    original: PathBuf,
    tombstone: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct BrowserProfileOperationJournal {
    version: u32,
    operation_id: String,
    account_id: String,
    kind: BrowserProfileOperationKind,
    paths: Vec<BrowserProfileOperationPath>,
}

#[cfg(unix)]
fn sync_directory(path: &Path, label: &str) -> Result<()> {
    std::fs::File::open(path)
        .with_context(|| format!("failed to open {label} {} for sync", path.display()))?
        .sync_all()
        .with_context(|| format!("failed to sync {label} {}", path.display()))
}

#[cfg(windows)]
fn sync_directory(path: &Path, label: &str) -> Result<()> {
    use std::os::windows::fs::OpenOptionsExt;

    // Opening a directory handle requires FILE_FLAG_BACKUP_SEMANTICS. Some
    // Windows filesystems/providers still reject FlushFileBuffers for directory
    // handles; those documented capability failures are safe to treat as a
    // best-effort boundary because every journal file itself is sync_all'd.
    const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
    let file = match std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
        .open(path)
    {
        Ok(file) => file,
        Err(error) if windows_directory_sync_unsupported(&error) => return Ok(()),
        Err(error) => {
            return Err(error)
                .with_context(|| format!("failed to open {label} {} for sync", path.display()));
        }
    };
    match file.sync_all() {
        Ok(()) => Ok(()),
        Err(error) if windows_directory_sync_unsupported(&error) => Ok(()),
        Err(error) => {
            Err(error).with_context(|| format!("failed to sync {label} {}", path.display()))
        }
    }
}

#[cfg(windows)]
fn windows_directory_sync_unsupported(error: &std::io::Error) -> bool {
    // ERROR_INVALID_FUNCTION, ERROR_ACCESS_DENIED, ERROR_INVALID_HANDLE,
    // ERROR_NOT_SUPPORTED, and ERROR_INVALID_PARAMETER respectively.
    matches!(error.raw_os_error(), Some(1 | 5 | 6 | 50 | 87))
}

#[cfg(not(any(unix, windows)))]
fn sync_directory(_path: &Path, _label: &str) -> Result<()> {
    Ok(())
}

fn sync_parent_directory(path: &Path, label: &str) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| anyhow!("{label} has no parent directory"))?;
    sync_directory(parent, label)
}

fn ensure_real_directory_chain(path: &Path, label: &str) -> Result<()> {
    if !path.is_absolute() {
        bail!("{label} {} must be absolute", path.display());
    }
    for ancestor in path.ancestors().collect::<Vec<_>>().into_iter().rev() {
        if ancestor.as_os_str().is_empty() {
            continue;
        }
        let metadata = std::fs::symlink_metadata(ancestor)
            .with_context(|| format!("failed to inspect {label} {}", ancestor.display()))?;
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            bail!("{label} {} must be a real directory", ancestor.display());
        }
    }
    Ok(())
}

fn profile_operation_journal_dir(data_dir: &Path) -> Result<PathBuf> {
    let absolute_data_dir = if data_dir.is_absolute() {
        data_dir.to_path_buf()
    } else {
        std::env::current_dir()
            .context("failed to resolve the current directory")?
            .join(data_dir)
    };
    ensure_real_directory_chain(&absolute_data_dir, "data directory")?;
    let data_dir = std::fs::canonicalize(&absolute_data_dir).with_context(|| {
        format!(
            "failed to canonicalize data directory {}",
            absolute_data_dir.display()
        )
    })?;
    let journal_dir = data_dir.join(PROFILE_OPERATION_JOURNAL_DIR);
    match std::fs::symlink_metadata(&journal_dir) {
        Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => {}
        Ok(_) => bail!(
            "browser profile operation journal {} must be a real directory",
            journal_dir.display()
        ),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            std::fs::create_dir(&journal_dir).with_context(|| {
                format!(
                    "failed to create browser profile operation journal {}",
                    journal_dir.display()
                )
            })?;
            sync_directory(&data_dir, "data directory")?;
        }
        Err(error) => {
            return Err(error).with_context(|| {
                format!(
                    "failed to inspect browser profile operation journal {}",
                    journal_dir.display()
                )
            });
        }
    }
    ensure_real_directory_chain(&journal_dir, "browser profile operation journal")?;
    Ok(journal_dir)
}

fn journal_file_name(operation_id: &str) -> String {
    format!("operation-{operation_id}.json")
}

fn persist_profile_operation_journal(
    data_dir: &Path,
    journal: &BrowserProfileOperationJournal,
) -> Result<PathBuf> {
    let journal_dir = profile_operation_journal_dir(data_dir)?;
    let final_path = journal_dir.join(journal_file_name(&journal.operation_id));
    let temporary_path = journal_dir.join(format!(".operation-{}.tmp", journal.operation_id));
    let encoded = serde_json::to_vec_pretty(journal)
        .context("failed to serialize browser profile operation journal")?;
    let write_result = (|| -> Result<()> {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary_path)
            .with_context(|| {
                format!(
                    "failed to create browser profile operation journal {}",
                    temporary_path.display()
                )
            })?;
        file.write_all(&encoded).with_context(|| {
            format!(
                "failed to write browser profile operation journal {}",
                temporary_path.display()
            )
        })?;
        file.sync_all().with_context(|| {
            format!(
                "failed to sync browser profile operation journal {}",
                temporary_path.display()
            )
        })?;
        std::fs::rename(&temporary_path, &final_path).with_context(|| {
            format!(
                "failed to commit browser profile operation journal {}",
                final_path.display()
            )
        })?;
        sync_directory(&journal_dir, "browser profile operation journal")?;
        Ok(())
    })();
    if write_result.is_err() {
        let _ = std::fs::remove_file(&temporary_path);
    }
    write_result?;
    Ok(final_path)
}

fn remove_profile_operation_journal(journal_path: &Path) -> Result<()> {
    let metadata = match std::fs::symlink_metadata(journal_path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => {
            return Err(error).with_context(|| {
                format!(
                    "failed to inspect browser profile operation journal {}",
                    journal_path.display()
                )
            });
        }
    };
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        bail!(
            "browser profile operation journal {} must be a real file",
            journal_path.display()
        );
    }
    std::fs::remove_file(journal_path).with_context(|| {
        format!(
            "failed to remove browser profile operation journal {}",
            journal_path.display()
        )
    })?;
    sync_parent_directory(journal_path, "browser profile operation journal")
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct BrowserProfileRecoveryReport {
    pub restored: usize,
    pub purged: usize,
    pub issues: Vec<String>,
}

impl BrowserProfileRecoveryReport {
    pub fn has_activity(&self) -> bool {
        self.restored > 0 || self.purged > 0 || !self.issues.is_empty()
    }

    pub fn summary(&self) -> String {
        let mut summary = format!(
            "restored {}, purged {} browser profile tombstone(s)",
            self.restored, self.purged
        );
        if !self.issues.is_empty() {
            summary.push_str(&format!(", {} issue(s)", self.issues.len()));
        }
        summary
    }

    fn require_complete(self) -> Result<Self> {
        if self.issues.is_empty() {
            Ok(self)
        } else {
            bail!(
                "browser profile recovery incomplete: {}",
                self.issues.join("; ")
            )
        }
    }
}

#[derive(Debug, Default)]
struct DiscoveredTombstones {
    directories: Vec<PathBuf>,
    unsafe_entry: bool,
}

fn tombstone_account_id(file_name: &std::ffi::OsStr) -> Option<String> {
    let name = file_name.to_str()?;
    let remainder = name.strip_prefix(PROFILE_TOMBSTONE_PREFIX)?;
    let (account_id, nonce) = remainder.rsplit_once('-')?;
    if nonce.len() != 32 || !nonce.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    validate_account_id(account_id).ok()?;
    Some(account_id.to_string())
}

fn purge_tombstone(path: &Path) -> Result<bool> {
    let metadata = match std::fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => {
            return Err(error)
                .with_context(|| format!("failed to inspect tombstone {}", path.display()));
        }
    };
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        bail!("tombstone {} must be a real directory", path.display());
    }
    std::fs::remove_dir_all(path)
        .with_context(|| format!("failed to remove browser profile {}", path.display()))?;
    sync_parent_directory(path, "browser profile root")?;
    Ok(true)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ProfilePathState {
    Missing,
    RealDirectory,
}

fn inspect_profile_path(path: &Path, label: &str) -> Result<ProfilePathState> {
    let parent = path
        .parent()
        .ok_or_else(|| anyhow!("{label} has no parent directory"))?;
    ensure_real_directory_chain(parent, "browser profile root")?;
    match std::fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => {
            Ok(ProfilePathState::RealDirectory)
        }
        Ok(_) => bail!("{label} {} must be a real directory", path.display()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(ProfilePathState::Missing),
        Err(error) => {
            Err(error).with_context(|| format!("failed to inspect {label} {}", path.display()))
        }
    }
}

fn purge_profile_directory(path: &Path, label: &str) -> Result<bool> {
    match inspect_profile_path(path, label)? {
        ProfilePathState::Missing => Ok(false),
        ProfilePathState::RealDirectory => {
            std::fs::remove_dir_all(path)
                .with_context(|| format!("failed to remove {label} {}", path.display()))?;
            sync_parent_directory(path, "browser profile root")?;
            Ok(true)
        }
    }
}

fn validate_profile_operation_journal(
    journal_path: &Path,
    journal: &BrowserProfileOperationJournal,
) -> Result<Vec<(PathBuf, String)>> {
    if journal.version != PROFILE_OPERATION_JOURNAL_VERSION {
        bail!(
            "unsupported browser profile operation journal version {}",
            journal.version
        );
    }
    validate_account_id(&journal.account_id)?;
    let parsed_operation_id = uuid::Uuid::parse_str(&journal.operation_id)
        .context("invalid browser profile operation id")?;
    if parsed_operation_id.simple().to_string() != journal.operation_id
        || journal_path.file_name().and_then(|name| name.to_str())
            != Some(journal_file_name(&journal.operation_id).as_str())
    {
        bail!(
            "browser profile operation journal {} has a mismatched operation id",
            journal_path.display()
        );
    }
    if journal.paths.is_empty() {
        bail!("browser profile operation journal has no paths");
    }

    let mut seen = HashSet::new();
    let mut groups = Vec::new();
    for entry in &journal.paths {
        if !entry.original.is_absolute() || !entry.tombstone.is_absolute() {
            bail!("browser profile operation paths must be absolute");
        }
        if entry
            .original
            .components()
            .any(|component| matches!(component, Component::CurDir | Component::ParentDir))
            || entry
                .tombstone
                .components()
                .any(|component| matches!(component, Component::CurDir | Component::ParentDir))
        {
            bail!("browser profile operation paths must be normalized");
        }
        if entry.original.file_name().and_then(|name| name.to_str())
            != Some(journal.account_id.as_str())
            || entry
                .tombstone
                .file_name()
                .and_then(tombstone_account_id)
                .as_deref()
                != Some(journal.account_id.as_str())
        {
            bail!("browser profile operation path does not match its account id");
        }
        let original_parent = entry
            .original
            .parent()
            .ok_or_else(|| anyhow!("browser profile path has no parent directory"))?;
        let tombstone_parent = entry
            .tombstone
            .parent()
            .ok_or_else(|| anyhow!("browser profile tombstone has no parent directory"))?;
        if original_parent != tombstone_parent {
            bail!("browser profile and tombstone must share a parent directory");
        }
        ensure_real_directory_chain(original_parent, "browser profile root")?;
        let canonical_parent = std::fs::canonicalize(original_parent).with_context(|| {
            format!(
                "failed to canonicalize browser profile root {}",
                original_parent.display()
            )
        })?;
        if canonical_parent != original_parent {
            bail!(
                "browser profile root {} must be canonical",
                original_parent.display()
            );
        }
        if !seen.insert(entry.original.clone()) || !seen.insert(entry.tombstone.clone()) {
            bail!("browser profile operation journal contains duplicate paths");
        }
        // Validate all leaves before any recovery action. This prevents one
        // unsafe entry from causing a partial destructive recovery.
        inspect_profile_path(&entry.original, "browser profile")?;
        inspect_profile_path(&entry.tombstone, "browser profile tombstone")?;
        groups.push((canonical_parent, journal.account_id.clone()));
    }
    Ok(groups)
}

fn profile_operation_reached_goal(
    kind: BrowserProfileOperationKind,
    account_exists: bool,
    entry: &BrowserProfileOperationPath,
) -> Result<bool> {
    let original = inspect_profile_path(&entry.original, "browser profile")?;
    let tombstone = inspect_profile_path(&entry.tombstone, "browser profile tombstone")?;
    if kind == BrowserProfileOperationKind::DeleteAccount && account_exists {
        Ok(original == ProfilePathState::RealDirectory && tombstone == ProfilePathState::Missing)
    } else {
        Ok(original == ProfilePathState::Missing && tombstone == ProfilePathState::Missing)
    }
}

fn recover_profile_operation(
    journal: &BrowserProfileOperationJournal,
    account_exists: bool,
    report: &mut BrowserProfileRecoveryReport,
) -> Result<()> {
    let restore = journal.kind == BrowserProfileOperationKind::DeleteAccount && account_exists;
    if restore {
        for entry in journal.paths.iter().rev() {
            match (
                inspect_profile_path(&entry.original, "browser profile")?,
                inspect_profile_path(&entry.tombstone, "browser profile tombstone")?,
            ) {
                (ProfilePathState::RealDirectory, ProfilePathState::Missing) => {}
                (ProfilePathState::Missing, ProfilePathState::RealDirectory) => {
                    std::fs::rename(&entry.tombstone, &entry.original).with_context(|| {
                        format!(
                            "failed to restore browser profile {}",
                            entry.original.display()
                        )
                    })?;
                    sync_parent_directory(&entry.original, "browser profile root")?;
                    report.restored += 1;
                }
                (ProfilePathState::Missing, ProfilePathState::Missing) => bail!(
                    "cannot restore browser profile {}; both profile and tombstone are missing",
                    entry.original.display()
                ),
                (ProfilePathState::RealDirectory, ProfilePathState::RealDirectory) => bail!(
                    "cannot restore browser profile {}; profile and tombstone both exist",
                    entry.original.display()
                ),
            }
        }
    } else {
        for entry in &journal.paths {
            if purge_profile_directory(&entry.tombstone, "browser profile tombstone")? {
                report.purged += 1;
            }
            if purge_profile_directory(&entry.original, "browser profile")? {
                report.purged += 1;
            }
        }
    }

    if journal
        .paths
        .iter()
        .map(|entry| profile_operation_reached_goal(journal.kind, account_exists, entry))
        .collect::<Result<Vec<_>>>()?
        .into_iter()
        .all(|complete| complete)
    {
        Ok(())
    } else {
        bail!("browser profile operation did not reach its recovery goal")
    }
}

#[derive(Debug, Default)]
struct JournalRecoveryOutcome {
    report: BrowserProfileRecoveryReport,
    protected_groups: HashSet<(PathBuf, String)>,
    block_legacy_recovery: bool,
}

fn recover_profile_operation_journals<F>(
    data_dir: &Path,
    only_account: Option<&str>,
    account_exists: &mut F,
) -> JournalRecoveryOutcome
where
    F: FnMut(&str) -> Result<bool>,
{
    let mut outcome = JournalRecoveryOutcome::default();
    let journal_dir = match profile_operation_journal_dir(data_dir) {
        Ok(path) => path,
        Err(error) => {
            outcome.report.issues.push(error.to_string());
            outcome.block_legacy_recovery = true;
            return outcome;
        }
    };
    let journal_entries = match std::fs::read_dir(&journal_dir) {
        Ok(entries) => entries,
        Err(error) => {
            outcome.report.issues.push(format!(
                "failed to scan browser profile operation journal {}: {error}",
                journal_dir.display()
            ));
            outcome.block_legacy_recovery = true;
            return outcome;
        }
    };
    let mut entries = Vec::new();
    for entry in journal_entries {
        match entry {
            Ok(entry) => entries.push(entry),
            Err(error) => {
                outcome.report.issues.push(format!(
                    "failed to read an entry in browser profile operation journal {}: {error}",
                    journal_dir.display()
                ));
                outcome.block_legacy_recovery = true;
            }
        }
    }
    entries.sort_by_key(|entry| entry.file_name());
    let mut account_states = HashMap::new();

    for entry in entries {
        let file_name = entry.file_name();
        let Some(file_name) = file_name.to_str() else {
            outcome.report.issues.push(format!(
                "browser profile operation journal {} has a non-Unicode name",
                entry.path().display()
            ));
            outcome.block_legacy_recovery = true;
            continue;
        };
        if !file_name.starts_with("operation-") || !file_name.ends_with(".json") {
            continue;
        }
        let journal_path = entry.path();
        match entry.file_type() {
            Ok(file_type) if file_type.is_file() && !file_type.is_symlink() => {}
            Ok(_) => {
                outcome.report.issues.push(format!(
                    "browser profile operation journal {} must be a real file",
                    journal_path.display()
                ));
                outcome.block_legacy_recovery = true;
                continue;
            }
            Err(error) => {
                outcome.report.issues.push(format!(
                    "failed to inspect browser profile operation journal {}: {error}",
                    journal_path.display()
                ));
                outcome.block_legacy_recovery = true;
                continue;
            }
        }
        let encoded = match std::fs::read(&journal_path) {
            Ok(encoded) => encoded,
            Err(error) => {
                outcome.report.issues.push(format!(
                    "failed to read browser profile operation journal {}: {error}",
                    journal_path.display()
                ));
                outcome.block_legacy_recovery = true;
                continue;
            }
        };
        let journal: BrowserProfileOperationJournal = match serde_json::from_slice(&encoded) {
            Ok(journal) => journal,
            Err(error) => {
                outcome.report.issues.push(format!(
                    "failed to parse browser profile operation journal {}: {error}",
                    journal_path.display()
                ));
                outcome.block_legacy_recovery = true;
                continue;
            }
        };
        let groups = match validate_profile_operation_journal(&journal_path, &journal) {
            Ok(groups) => groups,
            Err(error) => {
                outcome.report.issues.push(format!(
                    "invalid browser profile operation journal {}: {error}",
                    journal_path.display()
                ));
                outcome.block_legacy_recovery = true;
                continue;
            }
        };
        if only_account.is_some_and(|account_id| account_id != journal.account_id) {
            continue;
        }
        let exists = if journal.kind == BrowserProfileOperationKind::ResetProfile {
            false
        } else {
            match account_states.get(&journal.account_id) {
                Some(exists) => *exists,
                None => match account_exists(&journal.account_id) {
                    Ok(exists) => {
                        account_states.insert(journal.account_id.clone(), exists);
                        exists
                    }
                    Err(error) => {
                        outcome.report.issues.push(format!(
                            "failed to read account {} while recovering browser profiles: {error}",
                            journal.account_id
                        ));
                        outcome.protected_groups.extend(groups);
                        continue;
                    }
                },
            }
        };
        match recover_profile_operation(&journal, exists, &mut outcome.report) {
            Ok(()) => {
                if let Err(error) = remove_profile_operation_journal(&journal_path) {
                    outcome.report.issues.push(error.to_string());
                    outcome.protected_groups.extend(groups);
                }
            }
            Err(error) => {
                outcome.report.issues.push(format!(
                    "failed to recover browser profile operation {}: {error}",
                    journal.operation_id
                ));
                outcome.protected_groups.extend(groups);
            }
        }
    }
    outcome
}

fn recover_staged_browser_profiles_in_roots_with_exclusions<F>(
    roots: Vec<PathBuf>,
    only_account: Option<&str>,
    protected_groups: &HashSet<(PathBuf, String)>,
    mut account_exists: F,
) -> BrowserProfileRecoveryReport
where
    F: FnMut(&str) -> Result<bool>,
{
    let mut report = BrowserProfileRecoveryReport::default();
    let mut groups: BTreeMap<(PathBuf, String), DiscoveredTombstones> = BTreeMap::new();

    for root in roots {
        let root = if root.is_absolute() {
            root
        } else {
            match std::env::current_dir() {
                Ok(current_dir) => current_dir.join(root),
                Err(error) => {
                    report.issues.push(format!(
                        "failed to resolve browser profile root against the current directory: {error}"
                    ));
                    continue;
                }
            }
        };
        let root_metadata = match std::fs::symlink_metadata(&root) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => {
                report.issues.push(format!(
                    "failed to inspect browser profile root {}: {error}",
                    root.display()
                ));
                continue;
            }
        };
        if root_metadata.file_type().is_symlink() || !root_metadata.is_dir() {
            report.issues.push(format!(
                "browser profile root {} must be a real directory",
                root.display()
            ));
            continue;
        }
        if let Err(error) = ensure_real_directory_chain(&root, "browser profile root") {
            report.issues.push(error.to_string());
            continue;
        }
        let root = match std::fs::canonicalize(&root) {
            Ok(root) => root,
            Err(error) => {
                report.issues.push(format!(
                    "failed to canonicalize browser profile root {}: {error}",
                    root.display()
                ));
                continue;
            }
        };
        let entries = match std::fs::read_dir(&root) {
            Ok(entries) => entries,
            Err(error) => {
                report.issues.push(format!(
                    "failed to scan browser profile root {}: {error}",
                    root.display()
                ));
                continue;
            }
        };
        for entry in entries {
            let entry = match entry {
                Ok(entry) => entry,
                Err(error) => {
                    report.issues.push(format!(
                        "failed to read an entry in browser profile root {}: {error}",
                        root.display()
                    ));
                    continue;
                }
            };
            let Some(account_id) = tombstone_account_id(&entry.file_name()) else {
                continue;
            };
            if only_account.is_some_and(|expected| expected != account_id) {
                continue;
            }
            let path = entry.path();
            let group = groups.entry((root.clone(), account_id)).or_default();
            match entry.file_type() {
                Ok(file_type) if file_type.is_dir() && !file_type.is_symlink() => {
                    group.directories.push(path);
                }
                Ok(_) => {
                    group.unsafe_entry = true;
                    report.issues.push(format!(
                        "browser profile tombstone {} must be a real directory",
                        path.display()
                    ));
                }
                Err(error) => {
                    group.unsafe_entry = true;
                    report.issues.push(format!(
                        "failed to inspect browser profile tombstone {}: {error}",
                        path.display()
                    ));
                }
            }
        }
    }

    let mut account_states = HashMap::new();
    for ((root, account_id), mut group) in groups {
        if protected_groups.contains(&(root.clone(), account_id.clone())) {
            continue;
        }
        if group.unsafe_entry {
            continue;
        }
        group.directories.sort();
        let exists = match account_states.get(&account_id) {
            Some(exists) => *exists,
            None => match account_exists(&account_id) {
                Ok(exists) => {
                    account_states.insert(account_id.clone(), exists);
                    exists
                }
                Err(error) => {
                    report.issues.push(format!(
                        "failed to read account {account_id} while recovering browser profiles: {error}"
                    ));
                    continue;
                }
            },
        };
        let original = root.join(&account_id);

        if !exists {
            for tombstone in group.directories {
                match purge_tombstone(&tombstone) {
                    Ok(true) => report.purged += 1,
                    Ok(false) => {}
                    Err(error) => report.issues.push(error.to_string()),
                }
            }
            continue;
        }

        match std::fs::symlink_metadata(&original) {
            Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => {
                for tombstone in group.directories {
                    match purge_tombstone(&tombstone) {
                        Ok(true) => report.purged += 1,
                        Ok(false) => {}
                        Err(error) => report.issues.push(error.to_string()),
                    }
                }
            }
            Ok(_) => report.issues.push(format!(
                "browser profile {} must be a real directory",
                original.display()
            )),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                if group.directories.len() != 1 {
                    report.issues.push(format!(
                        "cannot restore browser profile {} from {} ambiguous tombstones",
                        original.display(),
                        group.directories.len()
                    ));
                    continue;
                }
                let tombstone = &group.directories[0];
                match std::fs::symlink_metadata(tombstone) {
                    Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => {
                        match std::fs::rename(tombstone, &original) {
                            Ok(()) => {
                                match sync_parent_directory(&original, "browser profile root") {
                                    Ok(()) => report.restored += 1,
                                    Err(error) => report.issues.push(error.to_string()),
                                }
                            }
                            Err(error) => report.issues.push(format!(
                                "failed to restore browser profile {}: {error}",
                                original.display()
                            )),
                        }
                    }
                    Ok(_) => report.issues.push(format!(
                        "browser profile tombstone {} must be a real directory",
                        tombstone.display()
                    )),
                    Err(error) => report.issues.push(format!(
                        "failed to inspect browser profile tombstone {}: {error}",
                        tombstone.display()
                    )),
                }
            }
            Err(error) => report.issues.push(format!(
                "failed to inspect browser profile {}: {error}",
                original.display()
            )),
        }
    }

    report
}

#[cfg(test)]
fn recover_staged_browser_profiles_in_roots<F>(
    roots: Vec<PathBuf>,
    only_account: Option<&str>,
    account_exists: F,
) -> BrowserProfileRecoveryReport
where
    F: FnMut(&str) -> Result<bool>,
{
    recover_staged_browser_profiles_in_roots_with_exclusions(
        roots,
        only_account,
        &HashSet::new(),
        account_exists,
    )
}

pub fn recover_staged_browser_profiles<F>(
    data_dir: &Path,
    mut account_exists: F,
) -> BrowserProfileRecoveryReport
where
    F: FnMut(&str) -> Result<bool>,
{
    let roots = browser_profile_roots(data_dir);
    recover_browser_profiles_with_roots(data_dir, roots, None, &mut account_exists)
}

pub fn recover_staged_browser_profiles_for_account(
    data_dir: &Path,
    account_id: &str,
    account_exists: bool,
) -> Result<BrowserProfileRecoveryReport> {
    validate_account_id(account_id)?;
    let roots = browser_profile_roots(data_dir);
    recover_browser_profiles_with_roots(data_dir, roots, Some(account_id), &mut |_| {
        Ok(account_exists)
    })
    .require_complete()
}

fn recover_browser_profiles_with_roots<F>(
    data_dir: &Path,
    roots: Result<Vec<PathBuf>>,
    only_account: Option<&str>,
    account_exists: &mut F,
) -> BrowserProfileRecoveryReport
where
    F: FnMut(&str) -> Result<bool>,
{
    let mut journal_outcome =
        recover_profile_operation_journals(data_dir, only_account, account_exists);
    if journal_outcome.block_legacy_recovery {
        return journal_outcome.report;
    }
    let roots = match roots {
        Ok(roots) => roots,
        Err(error) => {
            journal_outcome.report.issues.push(error.to_string());
            return journal_outcome.report;
        }
    };
    let legacy = recover_staged_browser_profiles_in_roots_with_exclusions(
        roots,
        only_account,
        &journal_outcome.protected_groups,
        account_exists,
    );
    journal_outcome.report.restored += legacy.restored;
    journal_outcome.report.purged += legacy.purged;
    journal_outcome.report.issues.extend(legacy.issues);
    journal_outcome.report
}

#[derive(Debug)]
pub struct StagedBrowserProfiles {
    journal: Option<BrowserProfileOperationJournal>,
    journal_path: Option<PathBuf>,
}

fn rollback_profile_renames(paths: &[BrowserProfileOperationPath]) -> Vec<String> {
    let mut issues = Vec::new();
    for completed in paths.iter().rev() {
        match std::fs::rename(&completed.tombstone, &completed.original) {
            Ok(()) => {
                if let Err(error) =
                    sync_parent_directory(&completed.original, "browser profile root")
                {
                    issues.push(error.to_string());
                }
            }
            Err(error) => issues.push(format!(
                "failed to restore browser profile {}: {error}",
                completed.original.display()
            )),
        }
    }
    issues
}

fn staged_profile_error(
    journal_path: &Path,
    renamed_paths: &[BrowserProfileOperationPath],
    cause: String,
) -> anyhow::Error {
    let mut rollback_issues = rollback_profile_renames(renamed_paths);
    if rollback_issues.is_empty()
        && let Err(error) = remove_profile_operation_journal(journal_path)
    {
        rollback_issues.push(error.to_string());
    }
    if rollback_issues.is_empty() {
        anyhow!(cause)
    } else {
        anyhow!("{cause}; {}", rollback_issues.join("; "))
    }
}

impl StagedBrowserProfiles {
    pub fn stage(
        data_dir: &Path,
        account_id: &str,
        kind: BrowserProfileOperationKind,
    ) -> Result<Self> {
        let paths = browser_profile_paths(data_dir, account_id)?;
        Self::stage_paths(data_dir, account_id, kind, paths)
    }

    fn stage_paths(
        data_dir: &Path,
        account_id: &str,
        kind: BrowserProfileOperationKind,
        paths: Vec<PathBuf>,
    ) -> Result<Self> {
        validate_account_id(account_id)?;
        let mut planned = Vec::new();
        for original in paths {
            let original = if original.is_absolute() {
                original
            } else {
                std::env::current_dir()
                    .context("failed to resolve the current directory")?
                    .join(original)
            };
            let metadata = match std::fs::symlink_metadata(&original) {
                Ok(metadata) => metadata,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                Err(error) => {
                    return Err(error).with_context(|| {
                        format!("failed to inspect browser profile {}", original.display())
                    });
                }
            };
            let parent = original
                .parent()
                .ok_or_else(|| anyhow!("browser profile has no parent directory"))?;
            ensure_real_directory_chain(parent, "browser profile root")?;
            if metadata.file_type().is_symlink() || !metadata.is_dir() {
                bail!(
                    "browser profile {} must be a real directory",
                    original.display()
                );
            }
            let original = std::fs::canonicalize(&original).with_context(|| {
                format!(
                    "failed to canonicalize browser profile {}",
                    original.display()
                )
            })?;
            if original.file_name().and_then(|name| name.to_str()) != Some(account_id) {
                bail!("browser profile path does not match its account id");
            }
            let parent = original
                .parent()
                .ok_or_else(|| anyhow!("browser profile has no parent directory"))?;
            if ["SingletonLock", "SingletonSocket", "SingletonCookie"]
                .iter()
                .any(|marker| std::fs::symlink_metadata(original.join(marker)).is_ok())
            {
                bail!(
                    "browser profile {} is still in use; close its browser window and retry",
                    original.display()
                );
            }
            let tombstone = parent.join(format!(
                ".ocg-profile-delete-{}-{}",
                account_id,
                uuid::Uuid::new_v4().simple()
            ));
            planned.push(BrowserProfileOperationPath {
                original,
                tombstone,
            });
        }
        if planned.is_empty() {
            return Ok(Self {
                journal: None,
                journal_path: None,
            });
        }

        let journal = BrowserProfileOperationJournal {
            version: PROFILE_OPERATION_JOURNAL_VERSION,
            operation_id: uuid::Uuid::new_v4().simple().to_string(),
            account_id: account_id.to_string(),
            kind,
            paths: planned,
        };
        let journal_path = persist_profile_operation_journal(data_dir, &journal)?;
        for (renamed, entry) in journal.paths.iter().enumerate() {
            if let Err(error) = std::fs::rename(&entry.original, &entry.tombstone) {
                return Err(staged_profile_error(
                    &journal_path,
                    &journal.paths[..renamed],
                    format!(
                        "failed to stage browser profile {}: {error}",
                        entry.original.display()
                    ),
                ));
            }
            if let Err(error) = sync_parent_directory(&entry.tombstone, "browser profile root") {
                return Err(staged_profile_error(
                    &journal_path,
                    &journal.paths[..=renamed],
                    format!(
                        "failed to durably stage browser profile {}: {error}",
                        entry.original.display()
                    ),
                ));
            }
        }
        Ok(Self {
            journal: Some(journal),
            journal_path: Some(journal_path),
        })
    }

    pub fn restore(self) -> Result<()> {
        let (Some(journal), Some(journal_path)) = (self.journal, self.journal_path) else {
            return Ok(());
        };
        if journal.kind != BrowserProfileOperationKind::DeleteAccount {
            bail!("reset browser profile operations cannot be restored");
        }
        let mut report = BrowserProfileRecoveryReport::default();
        recover_profile_operation(&journal, true, &mut report)?;
        remove_profile_operation_journal(&journal_path)
    }

    pub fn purge(self) -> Result<()> {
        let (Some(journal), Some(journal_path)) = (self.journal, self.journal_path) else {
            return Ok(());
        };
        let mut report = BrowserProfileRecoveryReport::default();
        recover_profile_operation(&journal, false, &mut report)?;
        remove_profile_operation_journal(&journal_path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_test_root(label: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "ocg-browser-{label}-{}",
            uuid::Uuid::new_v4().simple()
        ));
        std::fs::create_dir_all(&root).unwrap();
        root
    }

    fn tombstone(root: &Path, account_id: &str, nonce: u128) -> PathBuf {
        root.join(format!(
            "{PROFILE_TOMBSTONE_PREFIX}{account_id}-{nonce:032x}"
        ))
    }

    #[cfg(unix)]
    fn symlink_directory(target: &Path, link: &Path) -> std::io::Result<()> {
        std::os::unix::fs::symlink(target, link)
    }

    #[cfg(windows)]
    fn symlink_directory(target: &Path, link: &Path) -> std::io::Result<()> {
        std::os::windows::fs::symlink_dir(target, link)
    }

    #[cfg(unix)]
    fn remove_directory_symlink(link: &Path) -> std::io::Result<()> {
        std::fs::remove_file(link)
    }

    #[cfg(windows)]
    fn remove_directory_symlink(link: &Path) -> std::io::Result<()> {
        std::fs::remove_dir(link)
    }

    #[test]
    fn account_profile_id_rejects_path_traversal() {
        for invalid in ["", ".", "..", "a/b", "a\\b", "C:evil", "two words"] {
            assert!(
                validate_account_id(invalid).is_err(),
                "accepted {invalid:?}"
            );
        }
        assert!(validate_account_id("98c28790-94fe-48d8_a").is_ok());
    }

    #[test]
    fn profile_roots_honor_external_override_and_keep_legacy_root() {
        let data_dir = temp_test_root("profile-root-override");
        let external = data_dir.join("external-profiles");
        let roots = browser_profile_roots_with_override(
            &data_dir,
            Some(external.to_str().expect("test path should be Unicode")),
        )
        .unwrap();
        assert_eq!(roots, [external, data_dir.join("profiles")]);

        let legacy = data_dir.join("profiles");
        let roots = browser_profile_roots_with_override(
            &data_dir,
            Some(legacy.to_str().expect("test path should be Unicode")),
        )
        .unwrap();
        assert_eq!(roots, [legacy]);
        assert!(browser_profile_roots_with_override(&data_dir, Some("relative/path")).is_err());
        std::fs::remove_dir_all(data_dir).unwrap();
    }

    #[test]
    fn browser_urls_require_safe_absolute_https() {
        assert!(validate_browser_url("https://accounts.google.com/signup").is_ok());
        assert!(validate_browser_url("http://accounts.google.com/signup").is_err());
        assert!(validate_browser_url("https://user:pass@opencode.ai/").is_err());
        assert!(validate_browser_url("javascript:alert(1)").is_err());
    }

    #[test]
    fn remote_sessions_expire_on_idle_or_absolute_deadline() {
        let now = Instant::now();
        let base = RemoteSession {
            account_id: "account-1".into(),
            binding: "admin".into(),
            worker_ws_url: "ws://browser:6080/websockify".into(),
            created_at: now - Duration::from_secs(60),
            last_active: now - Duration::from_secs(60),
            cancellation: tokio::sync::watch::channel(false).0,
        };
        assert!(!session_expired(&base, now));
        let idle = RemoteSession {
            last_active: now - SESSION_IDLE_TIMEOUT,
            ..base.clone()
        };
        assert!(session_expired(&idle, now));
        let absolute = RemoteSession {
            created_at: now - SESSION_MAX_LIFETIME,
            last_active: now,
            ..base
        };
        assert!(session_expired(&absolute, now));
    }

    #[test]
    fn invalidating_remote_sessions_revokes_every_existing_view() {
        let runtime = BrowserRuntime::new();
        let now = Instant::now();
        let (cancellation, receiver) = tokio::sync::watch::channel(false);
        runtime.sessions.lock().insert(
            "old-token".into(),
            RemoteSession {
                account_id: "account-1".into(),
                binding: "admin".into(),
                worker_ws_url: "ws://browser:6080/websockify".into(),
                created_at: now,
                last_active: now,
                cancellation,
            },
        );
        runtime.invalidate_remote_sessions();
        assert!(!runtime.remote_session_active("old-token"));
        assert!(*receiver.borrow());
    }

    #[tokio::test]
    async fn browser_operations_are_globally_serialized() {
        let runtime = BrowserRuntime::new();
        let operation = runtime.operation().await;
        assert!(runtime.operations.try_lock().is_err());
        drop(operation);
        assert!(runtime.operations.try_lock().is_ok());
    }

    #[test]
    fn staged_profile_can_be_restored_and_purged() {
        let root = std::env::temp_dir().join(format!(
            "ocg-browser-profile-test-{}",
            uuid::Uuid::new_v4().simple()
        ));
        let profile = root.join("browser-profiles").join("account-1");
        std::fs::create_dir_all(&profile).unwrap();
        std::fs::write(profile.join("Cookies"), b"sensitive").unwrap();
        std::fs::write(profile.join("SingletonLock"), b"running").unwrap();
        assert!(
            StagedBrowserProfiles::stage(
                &root,
                "account-1",
                BrowserProfileOperationKind::DeleteAccount
            )
            .is_err()
        );
        std::fs::remove_file(profile.join("SingletonLock")).unwrap();
        let staged = StagedBrowserProfiles::stage(
            &root,
            "account-1",
            BrowserProfileOperationKind::DeleteAccount,
        )
        .unwrap();
        assert!(!profile.exists());
        staged.restore().unwrap();
        assert!(profile.join("Cookies").is_file());

        let staged = StagedBrowserProfiles::stage(
            &root,
            "account-1",
            BrowserProfileOperationKind::ResetProfile,
        )
        .unwrap();
        staged.purge().unwrap();
        assert!(!profile.exists());
        std::fs::remove_dir_all(root).unwrap();
    }

    /// Production reset path: stage(ResetProfile) then purge must clear both the
    /// current and legacy roots for the target account, without touching siblings.
    #[test]
    fn reset_profile_stages_and_purges_new_and_legacy_roots_only() {
        let data_dir = temp_test_root("reset-new-and-legacy");
        let account_id = "account-1";
        let new_profile = data_dir.join("browser-profiles").join(account_id);
        let legacy_profile = data_dir.join("profiles").join(account_id);
        let other_profile = data_dir.join("browser-profiles").join("account-2");
        std::fs::create_dir_all(&new_profile).unwrap();
        std::fs::create_dir_all(&legacy_profile).unwrap();
        std::fs::create_dir_all(&other_profile).unwrap();
        std::fs::write(new_profile.join("Cookies"), b"new").unwrap();
        std::fs::write(legacy_profile.join("Cookies"), b"legacy").unwrap();
        std::fs::write(other_profile.join("Cookies"), b"sibling").unwrap();

        let staged = StagedBrowserProfiles::stage(
            &data_dir,
            account_id,
            BrowserProfileOperationKind::ResetProfile,
        )
        .expect("production reset stages current and legacy roots");
        assert!(!new_profile.exists());
        assert!(!legacy_profile.exists());
        assert!(other_profile.join("Cookies").is_file());
        staged
            .purge()
            .expect("production reset purges staged profiles");

        assert!(!new_profile.exists());
        assert!(!legacy_profile.exists());
        assert!(other_profile.join("Cookies").is_file());
        assert_eq!(
            std::fs::read(other_profile.join("Cookies")).unwrap(),
            b"sibling"
        );
        std::fs::remove_dir_all(data_dir).unwrap();
    }

    #[test]
    fn staged_profile_rejects_non_directory_targets() {
        let root = std::env::temp_dir().join(format!(
            "ocg-browser-profile-file-test-{}",
            uuid::Uuid::new_v4().simple()
        ));
        let profile = root.join("browser-profiles").join("account-1");
        std::fs::create_dir_all(profile.parent().unwrap()).unwrap();
        std::fs::write(&profile, b"not a profile directory").unwrap();
        assert!(
            StagedBrowserProfiles::stage(
                &root,
                "account-1",
                BrowserProfileOperationKind::DeleteAccount
            )
            .is_err()
        );
        assert!(profile.is_file());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn reset_journal_purges_external_profile_even_when_current_root_changes() {
        let data_dir = temp_test_root("profile-journal-reset");
        let external_root_a = temp_test_root("profile-journal-root-a");
        let external_root_b = temp_test_root("profile-journal-root-b");
        let original = external_root_a.join("account-1");
        std::fs::create_dir_all(&original).unwrap();
        std::fs::write(original.join("Cookies"), b"old-cookie").unwrap();

        let staged = StagedBrowserProfiles::stage_paths(
            &data_dir,
            "account-1",
            BrowserProfileOperationKind::ResetProfile,
            vec![original.clone()],
        )
        .unwrap();
        let journal = staged.journal.as_ref().unwrap().clone();
        let journal_path = staged.journal_path.as_ref().unwrap().clone();
        let tombstone = journal.paths[0].tombstone.clone();
        assert!(journal.paths[0].original.is_absolute());
        assert!(tombstone.is_absolute());
        assert!(journal_path.starts_with(std::fs::canonicalize(&data_dir).unwrap()));
        drop(staged); // simulate a crash after the rename

        let report = recover_browser_profiles_with_roots(
            &data_dir,
            Ok(vec![external_root_b.clone()]),
            None,
            &mut |_| Ok(true),
        );

        assert_eq!(report.restored, 0);
        assert_eq!(report.purged, 1);
        assert!(report.issues.is_empty(), "{:?}", report.issues);
        assert!(!original.exists());
        assert!(!tombstone.exists());
        assert!(!journal_path.exists());
        std::fs::remove_dir_all(data_dir).unwrap();
        std::fs::remove_dir_all(external_root_a).unwrap();
        std::fs::remove_dir_all(external_root_b).unwrap();
    }

    #[test]
    fn delete_journal_restores_before_commit_and_purges_after_commit() {
        for (account_exists, should_restore) in [(true, true), (false, false)] {
            let data_dir = temp_test_root(if account_exists {
                "profile-journal-delete-precommit"
            } else {
                "profile-journal-delete-committed"
            });
            let root = data_dir.join("external");
            let original = root.join("account-1");
            std::fs::create_dir_all(&original).unwrap();
            std::fs::write(original.join("Cookies"), b"cookie").unwrap();
            let staged = StagedBrowserProfiles::stage_paths(
                &data_dir,
                "account-1",
                BrowserProfileOperationKind::DeleteAccount,
                vec![original.clone()],
            )
            .unwrap();
            let journal = staged.journal.as_ref().unwrap().clone();
            let journal_path = staged.journal_path.as_ref().unwrap().clone();
            let tombstone = journal.paths[0].tombstone.clone();
            drop(staged);

            let report =
                recover_browser_profiles_with_roots(&data_dir, Ok(Vec::new()), None, &mut |_| {
                    Ok(account_exists)
                });

            assert!(report.issues.is_empty(), "{:?}", report.issues);
            assert_eq!(report.restored, usize::from(should_restore));
            assert_eq!(report.purged, usize::from(!should_restore));
            assert_eq!(original.is_dir(), should_restore);
            assert!(!tombstone.exists());
            assert!(!journal_path.exists());
            if should_restore {
                assert_eq!(std::fs::read(original.join("Cookies")).unwrap(), b"cookie");
            }
            std::fs::remove_dir_all(data_dir).unwrap();
        }
    }

    #[test]
    fn journal_recovery_preserves_unsafe_profile_files() {
        let data_dir = temp_test_root("profile-journal-unsafe-file");
        let root = data_dir.join("external");
        let original = root.join("account-1");
        std::fs::create_dir_all(&original).unwrap();
        let staged = StagedBrowserProfiles::stage_paths(
            &data_dir,
            "account-1",
            BrowserProfileOperationKind::ResetProfile,
            vec![original.clone()],
        )
        .unwrap();
        let journal_path = staged.journal_path.as_ref().unwrap().clone();
        let tombstone = staged.journal.as_ref().unwrap().paths[0].tombstone.clone();
        drop(staged);
        std::fs::remove_dir_all(&tombstone).unwrap();
        std::fs::write(&tombstone, b"do not delete").unwrap();

        let report =
            recover_browser_profiles_with_roots(&data_dir, Ok(vec![root]), None, &mut |_| Ok(true));

        assert_eq!(report.restored, 0);
        assert_eq!(report.purged, 0);
        assert!(!report.issues.is_empty());
        assert_eq!(std::fs::read(&tombstone).unwrap(), b"do not delete");
        assert!(journal_path.is_file());
        std::fs::remove_dir_all(data_dir).unwrap();
    }

    #[test]
    fn account_scoped_recovery_validates_other_journals_before_legacy_scan() {
        let data_dir = temp_test_root("profile-journal-validation-order");
        let root = data_dir.join("browser-profiles");
        std::fs::create_dir_all(&root).unwrap();
        let legacy_tombstone = tombstone(&root, "target-account", 1);
        std::fs::create_dir(&legacy_tombstone).unwrap();
        std::fs::write(legacy_tombstone.join("Cookies"), b"keep-until-safe").unwrap();

        let invalid_journal = BrowserProfileOperationJournal {
            version: PROFILE_OPERATION_JOURNAL_VERSION + 1,
            operation_id: uuid::Uuid::new_v4().simple().to_string(),
            account_id: "other-account".into(),
            kind: BrowserProfileOperationKind::DeleteAccount,
            paths: vec![BrowserProfileOperationPath {
                original: std::fs::canonicalize(&root).unwrap().join("other-account"),
                tombstone: tombstone(&std::fs::canonicalize(&root).unwrap(), "other-account", 2),
            }],
        };
        let journal_path = persist_profile_operation_journal(&data_dir, &invalid_journal).unwrap();

        let report = recover_browser_profiles_with_roots(
            &data_dir,
            Ok(vec![root.clone()]),
            Some("target-account"),
            &mut |_| Ok(true),
        );

        assert_eq!(report.restored, 0);
        assert_eq!(report.purged, 0);
        assert!(!report.issues.is_empty());
        assert!(journal_path.is_file());
        assert!(legacy_tombstone.is_dir());
        assert!(!root.join("target-account").exists());
        std::fs::remove_dir_all(data_dir).unwrap();
    }

    #[test]
    fn directory_sync_wrapper_accepts_real_profile_directories() {
        let data_dir = temp_test_root("profile-directory-sync");
        let root = data_dir.join("browser-profiles");
        std::fs::create_dir(&root).unwrap();

        sync_directory(&data_dir, "test data directory").unwrap();
        sync_directory(&root, "test browser profile root").unwrap();

        std::fs::remove_dir_all(data_dir).unwrap();
    }

    #[cfg(any(unix, windows))]
    #[test]
    fn journal_recovery_never_follows_profile_symlinks() {
        let data_dir = temp_test_root("profile-journal-unsafe-symlink");
        let outside = temp_test_root("profile-journal-unsafe-symlink-target");
        let sentinel = outside.join("sentinel");
        std::fs::write(&sentinel, b"keep").unwrap();
        let root = data_dir.join("external");
        let original = root.join("account-1");
        std::fs::create_dir_all(&original).unwrap();
        let staged = StagedBrowserProfiles::stage_paths(
            &data_dir,
            "account-1",
            BrowserProfileOperationKind::ResetProfile,
            vec![original],
        )
        .unwrap();
        let journal_path = staged.journal_path.as_ref().unwrap().clone();
        let tombstone = staged.journal.as_ref().unwrap().paths[0].tombstone.clone();
        drop(staged);
        std::fs::remove_dir_all(&tombstone).unwrap();
        if let Err(error) = symlink_directory(&outside, &tombstone) {
            if error.kind() == std::io::ErrorKind::PermissionDenied
                || (cfg!(windows) && error.raw_os_error() == Some(1314))
            {
                std::fs::remove_dir_all(data_dir).unwrap();
                std::fs::remove_dir_all(outside).unwrap();
                return;
            }
            panic!("failed to create directory symlink: {error}");
        }

        let report =
            recover_browser_profiles_with_roots(&data_dir, Ok(vec![root]), None, &mut |_| Ok(true));

        assert_eq!(report.restored, 0);
        assert_eq!(report.purged, 0);
        assert!(!report.issues.is_empty());
        assert_eq!(std::fs::read(&sentinel).unwrap(), b"keep");
        assert!(journal_path.is_file());
        remove_directory_symlink(&tombstone).unwrap();
        std::fs::remove_dir_all(data_dir).unwrap();
        std::fs::remove_dir_all(outside).unwrap();
    }

    #[test]
    fn recovery_restores_one_tombstone_per_profile_root() {
        let data_dir = temp_test_root("profile-recovery-restore");
        let roots = [data_dir.join("browser-profiles"), data_dir.join("profiles")];
        for (index, root) in roots.iter().enumerate() {
            std::fs::create_dir_all(root).unwrap();
            let staged = tombstone(root, "account-1", index as u128 + 1);
            std::fs::create_dir(&staged).unwrap();
            std::fs::write(staged.join("Cookies"), format!("root-{index}")).unwrap();
        }

        let mut account_reads = 0;
        let report = recover_staged_browser_profiles_in_roots(roots.to_vec(), None, |account_id| {
            account_reads += 1;
            assert_eq!(account_id, "account-1");
            Ok(true)
        });

        assert_eq!(report.restored, 2);
        assert_eq!(report.purged, 0);
        assert!(report.issues.is_empty(), "{:?}", report.issues);
        assert_eq!(account_reads, 1);
        for (index, root) in roots.iter().enumerate() {
            assert_eq!(
                std::fs::read_to_string(root.join("account-1").join("Cookies")).unwrap(),
                format!("root-{index}")
            );
        }
        std::fs::remove_dir_all(data_dir).unwrap();
    }

    #[test]
    fn recovery_purges_committed_and_stale_tombstones() {
        let data_dir = temp_test_root("profile-recovery-purge");
        let deleted_root = data_dir.join("browser-profiles");
        let existing_root = data_dir.join("profiles");
        std::fs::create_dir_all(&deleted_root).unwrap();
        std::fs::create_dir_all(existing_root.join("existing-account")).unwrap();

        let deleted = [
            tombstone(&deleted_root, "deleted-account", 1),
            tombstone(&deleted_root, "deleted-account", 2),
        ];
        let stale = [
            tombstone(&existing_root, "existing-account", 3),
            tombstone(&existing_root, "existing-account", 4),
        ];
        for path in deleted.iter().chain(&stale) {
            std::fs::create_dir(path).unwrap();
            std::fs::write(path.join("Cookies"), b"sensitive").unwrap();
        }

        let report = recover_staged_browser_profiles_in_roots(
            vec![deleted_root, existing_root.clone()],
            None,
            |account_id| Ok(account_id == "existing-account"),
        );

        assert_eq!(report.restored, 0);
        assert_eq!(report.purged, 4);
        assert!(report.issues.is_empty(), "{:?}", report.issues);
        assert!(existing_root.join("existing-account").is_dir());
        assert!(deleted.iter().chain(&stale).all(|path| !path.exists()));
        std::fs::remove_dir_all(data_dir).unwrap();
    }

    #[test]
    fn recovery_preserves_ambiguous_or_unsafe_tombstones() {
        let data_dir = temp_test_root("profile-recovery-ambiguous");
        let root = data_dir.join("browser-profiles");
        std::fs::create_dir_all(&root).unwrap();
        let ambiguous = [
            tombstone(&root, "ambiguous-account", 1),
            tombstone(&root, "ambiguous-account", 2),
        ];
        for path in &ambiguous {
            std::fs::create_dir(path).unwrap();
        }
        let unsafe_file = tombstone(&root, "unsafe-account", 3);
        std::fs::write(&unsafe_file, b"not a directory").unwrap();
        let malformed = root.join(".ocg-profile-delete-ignored-account-not-a-uuid");
        std::fs::create_dir(&malformed).unwrap();

        let report = recover_staged_browser_profiles_in_roots(vec![root], None, |_| Ok(true));

        assert_eq!(report.restored, 0);
        assert_eq!(report.purged, 0);
        assert_eq!(report.issues.len(), 2, "{:?}", report.issues);
        assert!(ambiguous.iter().all(|path| path.is_dir()));
        assert!(unsafe_file.is_file());
        assert!(malformed.is_dir());
        std::fs::remove_dir_all(data_dir).unwrap();
    }

    #[test]
    fn target_recovery_makes_a_failed_delete_cleanup_retriable() {
        let data_dir = temp_test_root("profile-recovery-retry");
        let root = data_dir.join("profiles");
        std::fs::create_dir_all(&root).unwrap();
        let staged = tombstone(&root, "deleted-account", 1);
        std::fs::create_dir(&staged).unwrap();
        std::fs::write(staged.join("Cookies"), b"sensitive").unwrap();

        let report =
            recover_staged_browser_profiles_for_account(&data_dir, "deleted-account", false)
                .unwrap();

        assert_eq!(report.purged, 1);
        assert!(!staged.exists());
        std::fs::remove_dir_all(data_dir).unwrap();
    }

    #[cfg(any(unix, windows))]
    #[test]
    fn recovery_and_staging_never_follow_directory_symlinks() {
        let data_dir = temp_test_root("profile-recovery-symlink");
        let outside = temp_test_root("profile-recovery-symlink-target");
        let sentinel = outside.join("sentinel");
        std::fs::write(&sentinel, b"keep").unwrap();

        let root = data_dir.join("browser-profiles");
        std::fs::create_dir(&root).unwrap();
        let tombstone_link = tombstone(&root, "deleted-account", 1);
        if let Err(error) = symlink_directory(&outside, &tombstone_link) {
            if error.kind() == std::io::ErrorKind::PermissionDenied
                || (cfg!(windows) && error.raw_os_error() == Some(1314))
            {
                std::fs::remove_dir_all(data_dir).unwrap();
                std::fs::remove_dir_all(outside).unwrap();
                return;
            }
            panic!("failed to create directory symlink: {error}");
        }

        let report = recover_staged_browser_profiles_in_roots(vec![root], None, |_| Ok(false));
        assert_eq!(report.purged, 0);
        assert_eq!(report.issues.len(), 1, "{:?}", report.issues);
        assert!(
            std::fs::symlink_metadata(&tombstone_link)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert_eq!(std::fs::read(&sentinel).unwrap(), b"keep");
        remove_directory_symlink(&tombstone_link).unwrap();

        let legacy_root = data_dir.join("profiles");
        symlink_directory(&outside, &legacy_root).unwrap();
        std::fs::create_dir(outside.join("account-1")).unwrap();
        assert!(
            StagedBrowserProfiles::stage(
                &data_dir,
                "account-1",
                BrowserProfileOperationKind::DeleteAccount
            )
            .is_err()
        );
        assert!(outside.join("account-1").is_dir());
        remove_directory_symlink(&legacy_root).unwrap();

        std::fs::remove_dir_all(data_dir).unwrap();
        std::fs::remove_dir_all(outside).unwrap();
    }
}
