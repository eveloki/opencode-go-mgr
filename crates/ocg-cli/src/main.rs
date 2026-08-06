use anyhow::Result;
use chrono::Utc;
use clap::{Parser, Subcommand};
use ocg_core::browser::{BrowserProfileOperationKind, StagedBrowserProfiles};
use ocg_core::crypto::{KeyCipher, StaticKeyCipher, load_or_create_static_cipher};
use ocg_core::db::Database;
use ocg_core::gateway;
use ocg_core::models::{Account, AppConfig};
use ocg_core::state::CoreStateInner;
use std::net::{IpAddr, SocketAddr};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

#[derive(Parser)]
#[command(name = "ocg-manager-cli")]
#[command(about = "Headless CLI for OCG Manager gateway")]
#[command(version)]
struct Cli {
    /// Data directory for the CLI (default: ~/.ocg-mgr-cli)
    #[arg(long, global = true)]
    data_dir: Option<PathBuf>,

    /// Encryption key for API key storage.
    /// If omitted, uses OCG_MANAGER_ENCRYPTION_KEY env var or generates one in <data-dir>/.encryption-key.
    #[arg(long, global = true)]
    encryption_key: Option<String>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Start the gateway server
    Serve {
        /// Address to listen on
        #[arg(long, default_value = "127.0.0.1")]
        host: IpAddr,
        /// Gateway port (overrides config)
        #[arg(short, long)]
        port: Option<u16>,
        /// Directory containing the built web dashboard (dist)
        #[arg(long)]
        dashboard_dir: Option<PathBuf>,
    },
    /// Manage API keys
    Key {
        #[command(subcommand)]
        action: KeyAction,
    },
    /// Show gateway status
    Status,
}

#[derive(Subcommand)]
enum KeyAction {
    /// List all keys and their status
    List,
    /// Add a new key
    Add {
        /// Display name for the key
        name: String,
        /// The OpenCode-Go API key
        key: String,
        /// OpenCode-Go login account
        #[arg(long)]
        username: Option<String>,
        /// OpenCode-Go login password
        #[arg(long)]
        password: Option<String>,
    },
    /// Remove a key
    Remove {
        /// Account ID
        id: String,
    },
    /// Enable a key
    Enable {
        /// Account ID
        id: String,
    },
    /// Disable a key
    Disable {
        /// Account ID
        id: String,
    },
    /// Ping upstream with one or all enabled keys — shows real status code / body
    Ping {
        /// Account ID; omit to ping every enabled key
        id: Option<String>,
        /// Model to send (default: deepseek-v4-flash)
        #[arg(long, default_value = "deepseek-v4-flash")]
        model: String,
        /// User message (default: "ping")
        #[arg(long, default_value = "ping")]
        message: String,
        /// max_tokens for the ping (default: 3)
        #[arg(long, default_value_t = 3)]
        max_tokens: u32,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let data_dir = resolve_data_dir(cli.data_dir);
    let cipher = resolve_cipher(&data_dir, cli.encryption_key)?;

    match cli.command {
        Commands::Serve {
            host,
            port,
            dashboard_dir,
        } => serve(data_dir, cipher, host, port, dashboard_dir).await,
        Commands::Key { action } => key_command(data_dir, cipher, action).await,
        Commands::Status => status_command(data_dir, cipher).await,
    }
}

fn resolve_data_dir(data_dir: Option<PathBuf>) -> PathBuf {
    data_dir.unwrap_or_else(|| {
        let home = std::env::var("HOME")
            .or_else(|_| std::env::var("USERPROFILE"))
            .map(PathBuf::from)
            .unwrap_or_else(|_| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
        home.join(".ocg-mgr-cli")
    })
}

fn resolve_cipher(
    data_dir: &Path,
    encryption_key: Option<String>,
) -> Result<Arc<dyn KeyCipher + Send + Sync>> {
    let env_key = std::env::var("OCG_MANAGER_ENCRYPTION_KEY").ok();
    resolve_cipher_with(data_dir, encryption_key, env_key)
}

/// Priority: explicit encryption_key > env_key > on-disk key file.
fn resolve_cipher_with(
    data_dir: &Path,
    encryption_key: Option<String>,
    env_key: Option<String>,
) -> Result<Arc<dyn KeyCipher + Send + Sync>> {
    let cipher = match encryption_key {
        Some(secret) => StaticKeyCipher::new(&secret),
        None => match env_key {
            Some(secret) => StaticKeyCipher::new(&secret),
            None => load_or_create_static_cipher(data_dir)?,
        },
    };
    Ok(Arc::new(cipher))
}

fn build_state(
    data_dir: PathBuf,
    cipher: Arc<dyn KeyCipher + Send + Sync>,
) -> Result<Arc<CoreStateInner>> {
    let db = Database::open(data_dir.clone())?;
    Ok(Arc::new(CoreStateInner::new(db, data_dir, cipher)?))
}

async fn serve(
    data_dir: PathBuf,
    cipher: Arc<dyn KeyCipher + Send + Sync>,
    host: IpAddr,
    port: Option<u16>,
    dashboard_dir: Option<PathBuf>,
) -> Result<()> {
    let state = start_serve(data_dir, cipher, host, port, dashboard_dir).await?;
    println!("press Ctrl+C to stop");
    tokio::signal::ctrl_c().await?;
    println!("shutting down...");
    stop_serve(&state).await;
    Ok(())
}

async fn start_serve(
    data_dir: PathBuf,
    cipher: Arc<dyn KeyCipher + Send + Sync>,
    host: IpAddr,
    port: Option<u16>,
    dashboard_dir: Option<PathBuf>,
) -> Result<Arc<CoreStateInner>> {
    let state = build_state(data_dir, cipher)?;
    let executable = if dashboard_dir.is_none() {
        std::env::current_exe().ok()
    } else {
        None
    };
    state.set_dashboard_dir(resolve_dashboard_dir(dashboard_dir, executable.as_deref()));

    let mut config = state.config();
    if let Some(port) = port {
        config.gateway_port = port;
        state.set_config(config.clone())?;
    }

    let handle =
        gateway::start_gateway_on(state.clone(), SocketAddr::new(host, config.gateway_port))
            .await?;
    println!("gateway started on http://{}:{}", host, handle.port);
    println!("gateway key: {}", config.gateway_key);
    println!("dashboard: http://{}:{}/dashboard/", host, handle.port);
    println!("upstream: {}", config.upstream_base_url);

    {
        let mut gateway_lock = state.gateway.lock();
        *gateway_lock = Some(handle);
    }

    let _ = state.db.lock().log_gateway(
        "info",
        "gateway",
        &format!("cli gateway started on port {}", config.gateway_port),
    );
    Ok(state)
}

async fn stop_serve(state: &CoreStateInner) {
    let handle = state.gateway.lock().take();
    if let Some(handle) = handle {
        let _ = handle.shutdown.send(());
        let _ = tokio::time::timeout(Duration::from_secs(5), handle.task).await;
    }
    let _ = state
        .db
        .lock()
        .log_gateway("info", "gateway", "cli gateway stopped");
}

fn resolve_dashboard_dir(explicit: Option<PathBuf>, executable: Option<&Path>) -> Option<PathBuf> {
    explicit.or_else(|| {
        let dist = executable?.parent()?.join("dist");
        dist.is_dir().then_some(dist)
    })
}

async fn key_command(
    data_dir: PathBuf,
    cipher: Arc<dyn KeyCipher + Send + Sync>,
    action: KeyAction,
) -> Result<()> {
    let state = build_state(data_dir, cipher)?;
    let db = state.db.lock();

    match action {
        KeyAction::List => {
            let accounts = db.list_accounts()?;
            if accounts.is_empty() {
                println!("no keys configured");
                return Ok(());
            }
            println!("{:<36} {:<20} {:<8}", "id", "name", "enabled");
            for account in accounts {
                println!(
                    "{:<36} {:<20} {:<8}",
                    account.id,
                    account.name,
                    if account.enabled { "yes" } else { "no" },
                );
            }
        }
        KeyAction::Add {
            name,
            key,
            username,
            password,
        } => {
            let id = uuid::Uuid::new_v4().to_string();
            let now = Utc::now();
            let key_cipher = state.encrypt_key(&key)?;
            let password_cipher = match password {
                Some(p) if !p.trim().is_empty() => Some(state.encrypt_key(p.trim())?),
                _ => None,
            };
            let account = Account {
                id: id.clone(),
                name,
                username: username.and_then(|s| {
                    let trimmed = s.trim().to_string();
                    if trimmed.is_empty() {
                        None
                    } else {
                        Some(trimmed)
                    }
                }),
                password_cipher,
                key_cipher,
                enabled: true,
                account_type: ocg_core::models::AccountType::Key,
                setup_step: ocg_core::models::AccountSetupStep::Ready,
                referral_code: None,
                purchase_date: String::new(),
                expires_on: String::new(),
                cooldown_until: None,
                cooldown_generic_until: None,
                cooldown_5h_until: None,
                cooldown_week_until: None,
                cooldown_month_until: None,
                cooldown_free_until: None,
                last_error: None,
                auth_error: None,
                created_at: now,
                updated_at: now,
            };
            db.create_account(&account)?;
            let account = db
                .get_account(&id)?
                .ok_or_else(|| anyhow::anyhow!("created key not found: {}", id))?;
            db.log_gateway(
                "info",
                "account",
                &format!("cli added account {}", account.name),
            )?;
            println!("added key {} ({})", id, account.name);
        }
        KeyAction::Remove { id } => {
            // ponytail: drop the outer guard from line 197 before re-locking —
            // parking_lot::Mutex is not re-entrant, so the second lock() would deadlock.
            drop(db);
            let browser_operation = state.browser.operation().await;
            state.recover_browser_profiles_for_account(&id)?;
            let account = state
                .db
                .lock()
                .get_account(&id)?
                .ok_or_else(|| anyhow::anyhow!("key not found: {}", id))?;
            browser_operation.stop_account(&id).await?;
            let staged = StagedBrowserProfiles::stage(
                &state.data_dir(),
                &id,
                BrowserProfileOperationKind::DeleteAccount,
            )?;
            let delete_result = {
                let mut db = state.db.lock();
                let result = db.delete_account(&id);
                if result.is_ok() {
                    let _ = db.log_gateway(
                        "info",
                        "account",
                        &format!("cli removed account {}", account.name),
                    );
                }
                result
            };
            if let Err(error) = delete_result {
                let restore_error = staged.restore().err();
                match restore_error {
                    Some(restore) => anyhow::bail!(
                        "failed to remove account: {error}; failed to restore browser profile: {restore}"
                    ),
                    None => anyhow::bail!("failed to remove account: {error}"),
                }
            }
            staged.purge()?;
            println!("removed key {} ({})", id, account.name);
        }
        KeyAction::Enable { id } => {
            drop(db);
            toggle_account(&state, &id, true)?;
        }
        KeyAction::Disable { id } => {
            drop(db);
            toggle_account(&state, &id, false)?;
        }
        KeyAction::Ping {
            id,
            model,
            message,
            max_tokens,
        } => {
            drop(db);
            ping_keys(&state, id.as_deref(), &model, &message, max_tokens).await?;
        }
    }
    Ok(())
}

fn toggle_account(state: &Arc<CoreStateInner>, id: &str, enabled: bool) -> Result<()> {
    let db = state.db.lock();
    let account = db
        .get_account(id)?
        .ok_or_else(|| anyhow::anyhow!("key not found: {}", id))?;
    if enabled && (!account.setup_step.is_ready() || account.key_cipher.is_empty()) {
        anyhow::bail!("account setup is not complete and cannot be enabled");
    }
    let update = ocg_core::models::AccountUpdate {
        name: None,
        username: None,
        password: None,
        key: None,
        enabled: Some(enabled),
        referral_code: None,
        purchase_date: None,
    };
    db.update_account(id, &update, None, None)?;
    db.log_gateway(
        "info",
        "account",
        &format!(
            "cli {} account {}",
            if enabled { "enabled" } else { "disabled" },
            account.name
        ),
    )?;
    println!(
        "{} key {} ({})",
        if enabled { "enabled" } else { "disabled" },
        id,
        account.name
    );
    Ok(())
}

async fn status_command(data_dir: PathBuf, cipher: Arc<dyn KeyCipher + Send + Sync>) -> Result<()> {
    let state = build_state(data_dir, cipher)?;
    let config: AppConfig = state.config();
    let db = state.db.lock();
    let accounts = db.list_accounts()?;
    let enabled = accounts.iter().filter(|a| a.enabled).count();

    println!("data dir: {:?}", state.data_dir());
    println!("gateway port: {}", config.gateway_port);
    println!("gateway key: {}", config.gateway_key);
    println!("upstream: {}", config.upstream_base_url);
    println!("accounts: {} total, {} enabled", accounts.len(), enabled);
    Ok(())
}

/// One-shot ping: decrypts the key, sends a tiny chat completion, prints real upstream status.
/// Used to surface real 401/403/429/200 — what each key actually does upstream, no inference.
async fn ping_one(
    state: &Arc<CoreStateInner>,
    account: &Account,
    model: &str,
    message: &str,
    max_tokens: u32,
) -> (u16, String) {
    let key = match state.decrypt_key(&account.key_cipher) {
        Ok(k) => k,
        Err(e) => return (0, format!("decrypt failed: {}", e)),
    };
    let (config, client) = state.upstream_context();
    let url = format!(
        "{}/v1/chat/completions",
        config.upstream_base_url.trim_end_matches('/')
    );
    let body = serde_json::json!({
        "model": model,
        "messages": [{"role": "user", "content": message}],
        "max_tokens": max_tokens,
        "stream": false,
    });
    let started = std::time::Instant::now();
    let resp = client
        .post(&url)
        .header("Authorization", format!("Bearer {}", key))
        .header("Content-Type", "application/json")
        .json(&body)
        .timeout(std::time::Duration::from_secs(
            config.non_stream_timeout_secs,
        ))
        .send()
        .await;
    let elapsed = started.elapsed();
    match resp {
        Ok(r) => {
            let status = r.status().as_u16();
            match r.text().await {
                Ok(text) => {
                    let trimmed = text.chars().take(200).collect::<String>();
                    (status, format!("{}ms {}", elapsed.as_millis(), trimmed))
                }
                Err(error) => {
                    let error = if error.is_timeout() {
                        "response body timed out".to_string()
                    } else {
                        format!("response body failed: {error}")
                    };
                    (
                        0,
                        format!("{}ms {} after HTTP {}", elapsed.as_millis(), error, status),
                    )
                }
            }
        }
        Err(e) => (
            0,
            format!("{}ms request failed: {}", elapsed.as_millis(), e),
        ),
    }
}

async fn ping_keys(
    state: &Arc<CoreStateInner>,
    id: Option<&str>,
    model: &str,
    message: &str,
    max_tokens: u32,
) -> Result<()> {
    let targets: Vec<Account> = {
        let db = state.db.lock();
        match id {
            Some(i) => match db.get_account(i)? {
                Some(a) if a.setup_step.is_ready() && !a.key_cipher.is_empty() => vec![a],
                Some(_) => anyhow::bail!("account setup is not complete and cannot be pinged"),
                None => anyhow::bail!("key not found: {}", i),
            },
            None => db
                .list_accounts()?
                .into_iter()
                .filter(|a| a.enabled && a.setup_step.is_ready() && !a.key_cipher.is_empty())
                .collect(),
        }
    };
    if targets.is_empty() {
        println!("no enabled keys to ping");
        return Ok(());
    }
    println!(
        "pinging {} key(s) with model={} message={:?}",
        targets.len(),
        model,
        message
    );
    for account in targets {
        let (status, body) = ping_one(state, &account, model, message, max_tokens).await;
        let verdict = if status == 200 { "OK" } else { "FAIL" };
        println!(
            "[{}] {} ({}) status={} {}",
            verdict, account.id, account.name, status, body
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        Cli, Commands, KeyAction, build_state, key_command, ping_keys, resolve_cipher_with,
        resolve_dashboard_dir, resolve_data_dir, start_serve, status_command, stop_serve,
        toggle_account,
    };
    use clap::{CommandFactory, Parser};
    use ocg_core::browser::browser_profile_paths;
    use ocg_core::crypto::{KeyCipher, StaticKeyCipher};
    use ocg_core::models::{AccountSetupStep, AccountType};
    use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpListener as StdTcpListener};
    use std::path::PathBuf;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    fn temp_dir(label: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("ocg-cli-test-{}-{}", label, uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn free_port() -> u16 {
        StdTcpListener::bind(("127.0.0.1", 0))
            .unwrap()
            .local_addr()
            .unwrap()
            .port()
    }

    fn test_cipher() -> Arc<dyn KeyCipher + Send + Sync> {
        Arc::new(StaticKeyCipher::new("cli-test-secret"))
    }

    #[test]
    fn exposes_package_version() {
        assert_eq!(
            Cli::command().get_version(),
            Some(env!("CARGO_PKG_VERSION"))
        );
    }

    #[test]
    fn serve_accepts_container_bind_address() {
        let cli = Cli::try_parse_from(["ocg-manager-cli", "serve", "--host", "0.0.0.0"]).unwrap();
        let Commands::Serve { host, .. } = cli.command else {
            panic!("expected serve command");
        };
        assert!(host.is_unspecified());
    }

    #[test]
    fn cli_parses_key_and_status_subcommands() {
        let list = Cli::try_parse_from(["ocg-manager-cli", "key", "list"]).unwrap();
        assert!(matches!(
            list.command,
            Commands::Key {
                action: KeyAction::List
            }
        ));

        let add = Cli::try_parse_from([
            "ocg-manager-cli",
            "key",
            "add",
            "main",
            "sk-test",
            "--username",
            "user",
            "--password",
            "pass",
        ])
        .unwrap();
        let Commands::Key {
            action:
                KeyAction::Add {
                    name,
                    key,
                    username,
                    password,
                },
        } = add.command
        else {
            panic!("expected key add");
        };
        assert_eq!((name.as_str(), key.as_str()), ("main", "sk-test"));
        assert_eq!(username.as_deref(), Some("user"));
        assert_eq!(password.as_deref(), Some("pass"));

        assert!(matches!(
            Cli::try_parse_from(["ocg-manager-cli", "status"])
                .unwrap()
                .command,
            Commands::Status
        ));
    }

    #[test]
    fn resolve_data_dir_prefers_explicit_path() {
        let explicit = PathBuf::from("/tmp/custom-ocg-data");
        assert_eq!(resolve_data_dir(Some(explicit.clone())), explicit);
        let fallback = resolve_data_dir(None);
        assert!(fallback.ends_with(".ocg-mgr-cli"));
    }

    fn assert_cipher_matches_static(
        cipher: &Arc<dyn KeyCipher + Send + Sync>,
        secret: &str,
        plaintext: &str,
    ) {
        let expected = StaticKeyCipher::new(secret);
        let ciphertext = cipher.encrypt(plaintext).unwrap();
        assert_eq!(expected.decrypt(&ciphertext).unwrap(), plaintext);
        let ciphertext = expected.encrypt(plaintext).unwrap();
        assert_eq!(cipher.decrypt(&ciphertext).unwrap(), plaintext);
    }

    #[test]
    fn resolve_cipher_uses_explicit_env_then_file() {
        let dir = temp_dir("cipher");
        let explicit = resolve_cipher_with(
            &dir,
            Some("explicit-secret".into()),
            Some("env-secret".into()),
        )
        .unwrap();
        assert_cipher_matches_static(&explicit, "explicit-secret", "plain-explicit");

        let from_env = resolve_cipher_with(&dir, None, Some("env-secret".into())).unwrap();
        assert_cipher_matches_static(&from_env, "env-secret", "plain-env");

        let file_dir = temp_dir("cipher-file");
        let first = resolve_cipher_with(&file_dir, None, None).unwrap();
        let second = resolve_cipher_with(&file_dir, None, None).unwrap();
        let ciphertext = first.encrypt("roundtrip").unwrap();
        assert_eq!(second.decrypt(&ciphertext).unwrap(), "roundtrip");
        assert!(file_dir.join(".encryption-key").is_file());

        let _ = std::fs::remove_dir_all(dir);
        let _ = std::fs::remove_dir_all(file_dir);
    }

    #[test]
    fn dashboard_dir_prefers_explicit_then_existing_packaged_dist() {
        let root = std::env::temp_dir().join(format!("ocg-cli-dashboard-{}", uuid::Uuid::new_v4()));
        let dist = root.join("dist");
        std::fs::create_dir_all(&dist).unwrap();
        let executable = root.join("ocg-manager-cli");
        let explicit = root.join("custom");

        assert_eq!(
            resolve_dashboard_dir(Some(explicit.clone()), Some(&executable)),
            Some(explicit)
        );
        assert_eq!(
            resolve_dashboard_dir(None, Some(&executable)),
            Some(dist.clone())
        );
        std::fs::remove_dir_all(&dist).unwrap();
        assert_eq!(resolve_dashboard_dir(None, Some(&executable)), None);

        std::fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn key_lifecycle_and_status_cover_cli_account_commands() {
        let dir = temp_dir("keys");
        let cipher = test_cipher();

        key_command(dir.clone(), cipher.clone(), KeyAction::List)
            .await
            .unwrap();

        key_command(
            dir.clone(),
            cipher.clone(),
            KeyAction::Add {
                name: "main".into(),
                key: "sk-main".into(),
                username: Some("  alice  ".into()),
                password: Some("  secret  ".into()),
            },
        )
        .await
        .unwrap();

        key_command(
            dir.clone(),
            cipher.clone(),
            KeyAction::Add {
                name: "blank-creds".into(),
                key: "sk-blank".into(),
                username: Some("   ".into()),
                password: Some("".into()),
            },
        )
        .await
        .unwrap();

        let state = build_state(dir.clone(), cipher.clone()).unwrap();
        let accounts = state.db.lock().list_accounts().unwrap();
        assert_eq!(accounts.len(), 2);
        let main = accounts
            .iter()
            .find(|account| account.name == "main")
            .unwrap()
            .clone();
        assert_eq!(main.username.as_deref(), Some("alice"));
        assert!(main.password_cipher.is_some());
        let blank = accounts
            .iter()
            .find(|account| account.name == "blank-creds")
            .unwrap()
            .clone();
        assert!(blank.username.is_none());
        assert!(blank.password_cipher.is_none());

        let mut pending = blank.clone();
        pending.id = uuid::Uuid::new_v4().to_string();
        pending.name = "pending".into();
        pending.key_cipher = String::new();
        pending.enabled = true;
        pending.account_type = AccountType::Managed;
        pending.setup_step = AccountSetupStep::GoogleAccount;
        state.db.lock().create_account(&pending).unwrap();

        key_command(dir.clone(), cipher.clone(), KeyAction::List)
            .await
            .unwrap();
        status_command(dir.clone(), cipher.clone()).await.unwrap();

        key_command(
            dir.clone(),
            cipher.clone(),
            KeyAction::Disable {
                id: main.id.clone(),
            },
        )
        .await
        .unwrap();
        let disabled = state.db.lock().get_account(&main.id).unwrap().unwrap();
        assert!(!disabled.enabled);

        key_command(
            dir.clone(),
            cipher.clone(),
            KeyAction::Enable {
                id: main.id.clone(),
            },
        )
        .await
        .unwrap();
        let enabled = state.db.lock().get_account(&main.id).unwrap().unwrap();
        assert!(enabled.enabled);

        assert!(toggle_account(&state, &pending.id, true).is_err());
        assert!(
            ping_keys(
                &state,
                Some(pending.id.as_str()),
                "deepseek-v4-flash",
                "ping",
                3,
            )
            .await
            .is_err()
        );

        let blank_profiles = browser_profile_paths(&dir, &blank.id).unwrap();
        assert!(blank_profiles.iter().all(|path| path.starts_with(&dir)));
        for profile in &blank_profiles {
            std::fs::create_dir_all(profile).unwrap();
            std::fs::write(profile.join("Cookies"), b"session").unwrap();
        }

        key_command(
            dir.clone(),
            cipher.clone(),
            KeyAction::Remove {
                id: blank.id.clone(),
            },
        )
        .await
        .unwrap();
        assert!(state.db.lock().get_account(&blank.id).unwrap().is_none());
        assert!(blank_profiles.iter().all(|path| !path.exists()));

        let pending_profile = browser_profile_paths(&dir, &pending.id).unwrap()[0].clone();
        std::fs::create_dir_all(&pending_profile).unwrap();
        std::fs::write(pending_profile.join("SingletonLock"), b"active").unwrap();
        let active_profile = key_command(
            dir.clone(),
            cipher.clone(),
            KeyAction::Remove {
                id: pending.id.clone(),
            },
        )
        .await;
        assert!(active_profile.is_err());
        assert!(state.db.lock().get_account(&pending.id).unwrap().is_some());
        assert!(pending_profile.exists());
        std::fs::remove_file(pending_profile.join("SingletonLock")).unwrap();
        key_command(
            dir.clone(),
            cipher.clone(),
            KeyAction::Remove {
                id: pending.id.clone(),
            },
        )
        .await
        .unwrap();

        let missing = key_command(
            dir.clone(),
            cipher.clone(),
            KeyAction::Remove {
                id: "missing-id".into(),
            },
        )
        .await;
        assert!(missing.is_err());

        let missing_toggle = toggle_account(&state, "missing-id", true);
        assert!(missing_toggle.is_err());

        let _ = std::fs::remove_dir_all(dir);
    }

    async fn spawn_json_upstream(
        hits: Arc<AtomicUsize>,
    ) -> (SocketAddr, tokio::task::JoinHandle<()>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            loop {
                let Ok((mut stream, _)) = listener.accept().await else {
                    break;
                };
                hits.fetch_add(1, Ordering::SeqCst);
                let mut buf = vec![0_u8; 4096];
                let _ = stream.read(&mut buf).await;
                let body = br#"{"id":"ping","object":"chat.completion","choices":[{"index":0,"message":{"role":"assistant","content":"pong"},"finish_reason":"stop"}],"usage":{"prompt_tokens":1,"completion_tokens":1}}"#;
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                let _ = stream.write_all(response.as_bytes()).await;
                let _ = stream.write_all(body).await;
            }
        });
        (addr, server)
    }

    #[tokio::test]
    async fn ping_keys_hits_configured_upstream_and_handles_empty_targets() {
        let hits = Arc::new(AtomicUsize::new(0));
        let (addr, server) = spawn_json_upstream(hits.clone()).await;

        let dir = temp_dir("ping");
        let cipher = test_cipher();
        let state = build_state(dir.clone(), cipher.clone()).unwrap();
        let mut config = state.config();
        config.upstream_base_url = format!("http://{addr}");
        config.non_stream_timeout_secs = 5;
        state.set_config(config).unwrap();

        key_command(
            dir.clone(),
            cipher.clone(),
            KeyAction::Add {
                name: "pingable".into(),
                key: "sk-ping".into(),
                username: None,
                password: None,
            },
        )
        .await
        .unwrap();
        let account_id = state.db.lock().list_accounts().unwrap()[0].id.clone();

        ping_keys(&state, None, "deepseek-v4-flash", "ping", 3)
            .await
            .unwrap();
        ping_keys(
            &state,
            Some(account_id.as_str()),
            "deepseek-v4-flash",
            "ping",
            3,
        )
        .await
        .unwrap();
        assert!(hits.load(Ordering::SeqCst) >= 2);

        toggle_account(&state, &account_id, false).unwrap();
        ping_keys(&state, None, "deepseek-v4-flash", "ping", 3)
            .await
            .unwrap();

        let missing = ping_keys(&state, Some("nope"), "deepseek-v4-flash", "ping", 3).await;
        assert!(missing.is_err());

        // Reopen with a different cipher so decrypt fails while the account still exists.
        let wrong_cipher: Arc<dyn KeyCipher + Send + Sync> =
            Arc::new(StaticKeyCipher::new("other-secret"));
        let wrong_state = build_state(dir.clone(), wrong_cipher).unwrap();
        let wrong_id = wrong_state.db.lock().list_accounts().unwrap()[0].id.clone();
        ping_keys(
            &wrong_state,
            Some(wrong_id.as_str()),
            "deepseek-v4-flash",
            "ping",
            3,
        )
        .await
        .unwrap();

        server.abort();
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn start_serve_binds_port_persists_override_and_stops_cleanly() {
        let dir = temp_dir("serve");
        let dash = dir.join("custom-dist");
        std::fs::create_dir_all(&dash).unwrap();
        let port = free_port();
        let cipher = test_cipher();

        let state = start_serve(
            dir.clone(),
            cipher.clone(),
            IpAddr::V4(Ipv4Addr::LOCALHOST),
            Some(port),
            Some(dash.clone()),
        )
        .await
        .unwrap();

        assert_eq!(state.active_gateway_port(), port);
        assert_eq!(state.config().gateway_port, port);
        assert_eq!(state.dashboard_dir(), Some(dash.clone()));
        assert!(std::net::TcpStream::connect(SocketAddr::from(([127, 0, 0, 1], port))).is_ok());

        stop_serve(&state).await;
        assert!(state.gateway.lock().is_none());
        assert!(
            std::net::TcpStream::connect(SocketAddr::from(([127, 0, 0, 1], port))).is_err(),
            "gateway port should reject connections after graceful stop"
        );

        // Reopen and ensure the port override was persisted for the next start.
        let reopened = build_state(dir.clone(), cipher).unwrap();
        assert_eq!(reopened.config().gateway_port, port);

        let _ = std::fs::remove_dir_all(dir);
    }
}
