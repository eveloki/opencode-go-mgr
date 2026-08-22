//! Database-owned sub gateway keys and the in-memory credential snapshot.
//!
//! Two credential tiers share one auth surface:
//! - the primary key is the legacy `AppConfig::gateway_key` scalar: never
//!   disabled or deleted, attributed under the fixed [`PRIMARY_KEY_ID`];
//! - sub keys live in the `sub_gateway_keys` table (schema v20) and change
//!   only through the key lifecycle API.
//!
//! The credential snapshot maps value -> (id, name) and is the single source
//! for both the auth hot path and forward-log name snapshots; readers never
//! take the config or db locks. Concrete hosts (`KeyStore` / [`KeyHost`])
//! live in `state`; this module never names the process-level owner.
//!
//! Invalidation model: the table is written only by this module's mutation
//! entry points (called with the `settings_update` lock held, snapshot
//! updated in the same critical section) and the config scalar only through
//! `set_config` (which refreshes the primary entry). Direct external edits to
//! SQLite or the config store are outside the model and do not take effect
//! until the next restart.

use crate::models::SubGatewayKey;
use chrono::{DateTime, Utc};
use std::collections::HashMap;
use std::ops::Deref;

pub use crate::kernel::ids::{PRIMARY_KEY_ID, PRIMARY_KEY_NAME};

const MAX_NAME_CHARS: usize = 64;
/// Ceiling on active (non-deleted) sub keys. Auth scans, the credential
/// snapshot, and management payloads all scale with the list, and a local
/// node has no realistic need for more devices than this. Tombstones do not
/// count against the ceiling.
const MAX_ACTIVE_SUB_KEYS: usize = 64;

/// Key lifecycle failure: user-correctable rejections vs internal errors.
/// Rollback paths that cannot restore the snapshot consistently return
/// `Internal` so endpoints surface 500 and the next key API entry rebuilds
/// the snapshot from the database.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeyError {
    BadRequest(String),
    Internal(String),
}

impl std::fmt::Display for KeyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BadRequest(message) | Self::Internal(message) => f.write_str(message),
        }
    }
}

impl KeyError {
    pub fn bad_request(message: impl Into<String>) -> Self {
        Self::BadRequest(message.into())
    }

    fn internal(context: &str, error: impl std::fmt::Display) -> Self {
        Self::Internal(format!("{context}: {error}"))
    }
}

/// One authenticating credential as seen by the auth hot path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CredentialEntry {
    pub id: String,
    pub name: String,
}

/// value -> credential; built from the config scalar plus the enabled,
/// non-deleted sub keys.
pub type CredentialSnapshot = HashMap<String, CredentialEntry>;

/// Persistence operations the key lifecycle needs. Implemented for the
/// SQLite database and the process host in `state`.
pub trait KeyStore {
    fn list_active_sub_gateway_keys(&self) -> anyhow::Result<Vec<SubGatewayKey>>;
    fn get_sub_gateway_key(&self, id: &str) -> anyhow::Result<Option<SubGatewayKey>>;
    fn count_active_sub_gateway_keys(&self) -> anyhow::Result<usize>;
    fn insert_sub_gateway_key(&self, key: &SubGatewayKey) -> anyhow::Result<()>;
    fn rename_sub_gateway_key(&self, id: &str, name: &str) -> anyhow::Result<bool>;
    fn set_sub_gateway_key_enabled(&self, id: &str, enabled: bool) -> anyhow::Result<bool>;
    fn update_sub_gateway_key_value(&self, id: &str, new_value: &str) -> anyhow::Result<bool>;
    fn soft_delete_sub_gateway_key(&self, id: &str, now: DateTime<Utc>) -> anyhow::Result<bool>;
    fn active_sub_gateway_key_values(&self) -> anyhow::Result<Vec<String>>;
    fn sub_gateway_key_value_exists(&self, value: &str) -> anyhow::Result<bool>;
    fn random_word(&self) -> String;
}

/// Process-level key host: store plus the config scalar and credential
/// snapshot. Concrete adapters must acquire db before config before the
/// snapshot write lock.
pub trait KeyHost: KeyStore {
    fn primary_gateway_key(&self) -> String;
    fn clone_credential_snapshot(&self) -> CredentialSnapshot;
    fn replace_credential_snapshot(&self, snapshot: CredentialSnapshot);
    fn with_credential_snapshot_mut<R>(&self, f: impl FnOnce(&mut CredentialSnapshot) -> R) -> R;
    /// Stored non-deleted values plus the live snapshot. Hosts must take the
    /// db lock before the snapshot read lock.
    fn load_unique_value_inputs(&self) -> anyhow::Result<(Vec<String>, CredentialSnapshot)>;
    /// Active sub keys plus the primary scalar. Hosts must take the db lock
    /// before the config lock.
    fn load_snapshot_rebuild_inputs(&self) -> anyhow::Result<(Vec<SubGatewayKey>, String)>;
}

impl<T> KeyStore for T
where
    T: Deref,
    T::Target: KeyStore,
{
    fn list_active_sub_gateway_keys(&self) -> anyhow::Result<Vec<SubGatewayKey>> {
        self.deref().list_active_sub_gateway_keys()
    }
    fn get_sub_gateway_key(&self, id: &str) -> anyhow::Result<Option<SubGatewayKey>> {
        self.deref().get_sub_gateway_key(id)
    }
    fn count_active_sub_gateway_keys(&self) -> anyhow::Result<usize> {
        self.deref().count_active_sub_gateway_keys()
    }
    fn insert_sub_gateway_key(&self, key: &SubGatewayKey) -> anyhow::Result<()> {
        self.deref().insert_sub_gateway_key(key)
    }
    fn rename_sub_gateway_key(&self, id: &str, name: &str) -> anyhow::Result<bool> {
        self.deref().rename_sub_gateway_key(id, name)
    }
    fn set_sub_gateway_key_enabled(&self, id: &str, enabled: bool) -> anyhow::Result<bool> {
        self.deref().set_sub_gateway_key_enabled(id, enabled)
    }
    fn update_sub_gateway_key_value(&self, id: &str, new_value: &str) -> anyhow::Result<bool> {
        self.deref().update_sub_gateway_key_value(id, new_value)
    }
    fn soft_delete_sub_gateway_key(&self, id: &str, now: DateTime<Utc>) -> anyhow::Result<bool> {
        self.deref().soft_delete_sub_gateway_key(id, now)
    }
    fn active_sub_gateway_key_values(&self) -> anyhow::Result<Vec<String>> {
        self.deref().active_sub_gateway_key_values()
    }
    fn sub_gateway_key_value_exists(&self, value: &str) -> anyhow::Result<bool> {
        self.deref().sub_gateway_key_value_exists(value)
    }
    fn random_word(&self) -> String {
        self.deref().random_word()
    }
}

impl<T> KeyHost for T
where
    T: Deref,
    T::Target: KeyHost,
{
    fn primary_gateway_key(&self) -> String {
        self.deref().primary_gateway_key()
    }
    fn clone_credential_snapshot(&self) -> CredentialSnapshot {
        self.deref().clone_credential_snapshot()
    }
    fn replace_credential_snapshot(&self, snapshot: CredentialSnapshot) {
        self.deref().replace_credential_snapshot(snapshot)
    }
    fn with_credential_snapshot_mut<R>(&self, f: impl FnOnce(&mut CredentialSnapshot) -> R) -> R {
        self.deref().with_credential_snapshot_mut(f)
    }
    fn load_unique_value_inputs(&self) -> anyhow::Result<(Vec<String>, CredentialSnapshot)> {
        self.deref().load_unique_value_inputs()
    }
    fn load_snapshot_rebuild_inputs(&self) -> anyhow::Result<(Vec<SubGatewayKey>, String)> {
        self.deref().load_snapshot_rebuild_inputs()
    }
}

/// Snapshot ground truth. The primary entry is inserted first; cross-tier
/// value uniqueness (enforced by the API gates) means one value can never
/// resolve to two ids.
pub fn build_credential_snapshot(
    store: &impl KeyStore,
    primary_value: &str,
) -> anyhow::Result<CredentialSnapshot> {
    Ok(assemble_credential_snapshot(
        store.list_active_sub_gateway_keys()?,
        primary_value,
    ))
}

fn assemble_credential_snapshot(
    keys: impl IntoIterator<Item = SubGatewayKey>,
    primary_value: &str,
) -> CredentialSnapshot {
    let mut snapshot = CredentialSnapshot::new();
    if !primary_value.is_empty() {
        snapshot.insert(
            primary_value.to_string(),
            CredentialEntry {
                id: PRIMARY_KEY_ID.to_string(),
                name: PRIMARY_KEY_NAME.to_string(),
            },
        );
    }
    for key in keys {
        if !key.authenticates() {
            continue;
        }
        // Primary attribution wins on a cross-tier value collision (only
        // possible after an out-of-model write): a later revoke of the sub
        // key then cannot evict the primary's live entry.
        if snapshot
            .get(&key.key)
            .is_some_and(|entry| entry.id == PRIMARY_KEY_ID)
        {
            eprintln!(
                "warning: enabled sub key `{}` shares the primary key's value; \
                 attributing the value to the primary key",
                key.id
            );
            continue;
        }
        snapshot.insert(
            key.key.clone(),
            CredentialEntry {
                id: key.id,
                name: key.name,
            },
        );
    }
    snapshot
}

/// Rebuilds the snapshot from the database and the config scalar. Called at
/// every key API entry point (already under `settings_update`) so a snapshot
/// left inconsistent by a failed rollback converges on the next operation;
/// startup loading is the natural self-healing point.
pub fn refresh_snapshot(host: &impl KeyHost) {
    let next = match host.load_snapshot_rebuild_inputs() {
        Ok((keys, primary_value)) => Ok(assemble_credential_snapshot(keys, &primary_value)),
        Err(error) => Err(error),
    };
    match next {
        Ok(next) => host.replace_credential_snapshot(next),
        Err(error) => {
            eprintln!("warning: failed to rebuild the credential snapshot: {error}");
        }
    }
}

/// Unified cross-tier gate (design D2): a candidate primary key value must
/// differ from every non-deleted sub key's value, enabled or disabled, so
/// the same credential can never authenticate under two ids. Shared by the
/// dashboard settings update, the Tauri settings update, and the sub key
/// enable path.
pub fn ensure_primary_value_allowed(store: &impl KeyStore, value: &str) -> Result<(), KeyError> {
    let exists = store
        .sub_gateway_key_value_exists(value)
        .map_err(|error| KeyError::internal("failed to check key values", error))?;
    if exists {
        return Err(KeyError::bad_request(
            "key value is already used by another key",
        ));
    }
    Ok(())
}

fn validate_name(name: &str) -> Result<String, KeyError> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(KeyError::bad_request("key name is required"));
    }
    if trimmed.chars().count() > MAX_NAME_CHARS {
        return Err(KeyError::bad_request(format!(
            "key name must be at most {MAX_NAME_CHARS} characters"
        )));
    }
    Ok(trimmed.to_string())
}

/// Generates a fresh value that collides with no snapshot credential (the
/// primary plus enabled sub keys) and no stored non-deleted sub key value
/// (disabled entries keep their plaintext).
fn generate_unique_value(
    store: &impl KeyStore,
    snapshot: &CredentialSnapshot,
) -> Result<String, KeyError> {
    let stored_values = store
        .active_sub_gateway_key_values()
        .map_err(|error| KeyError::internal("failed to load key values", error))?;
    generate_unique_value_from(&stored_values, snapshot, store)
}

fn generate_unique_value_from(
    stored_values: &[String],
    snapshot: &CredentialSnapshot,
    store: &impl KeyStore,
) -> Result<String, KeyError> {
    loop {
        let candidate = format!("ocg-{}-{}", store.random_word(), store.random_word());
        if snapshot.contains_key(&candidate) {
            continue;
        }
        if stored_values.iter().any(|value| value == &candidate) {
            continue;
        }
        return Ok(candidate);
    }
}

/// Generates a fresh primary key value that collides with no non-deleted sub
/// key value; used by both primary rotation entry points.
pub fn generate_primary_value(store: &impl KeyStore, current: &str) -> Result<String, KeyError> {
    let mut snapshot = CredentialSnapshot::new();
    if !current.is_empty() {
        snapshot.insert(
            current.to_string(),
            CredentialEntry {
                id: PRIMARY_KEY_ID.to_string(),
                name: PRIMARY_KEY_NAME.to_string(),
            },
        );
    }
    generate_unique_value(store, &snapshot)
}

fn active_sub_key(host: &impl KeyHost, id: &str) -> Result<SubGatewayKey, KeyError> {
    let found = host
        .get_sub_gateway_key(id)
        .map_err(|error| KeyError::internal("failed to load the key", error))?;
    match found {
        Some(key) if key.is_active() => Ok(key),
        _ => Err(KeyError::bad_request("key not found")),
    }
}

/// Removes the sub key's own snapshot entry and returns it so a failed
/// table write can restore it. The primary entry is never evicted: a value
/// shared with the primary can only exist after an out-of-model write
/// (`set_config` gates warn), and dropping it would 401 the primary until
/// the next rebuild for no security gain.
fn revoke_snapshot_value(
    host: &impl KeyHost,
    key_id: &str,
    value: &str,
) -> Option<CredentialEntry> {
    host.with_credential_snapshot_mut(|snapshot| match snapshot.get(value) {
        Some(entry) if entry.id == key_id => snapshot.remove(value),
        Some(entry) => {
            eprintln!(
                "warning: value of sub key `{key_id}` collides with the primary key entry \
                 (`{}`); keeping the primary snapshot entry",
                entry.id
            );
            None
        }
        None => None,
    })
}

fn restore_snapshot_entry(host: &impl KeyHost, value: &str, entry: CredentialEntry) {
    host.with_credential_snapshot_mut(|snapshot| {
        snapshot.insert(value.to_string(), entry);
    });
}

/// Creates a new enabled sub key and returns it; the response carries the
/// full value exactly once. Order: commit the table write first, then
/// rebuild the snapshot — worst case the new key starts authenticating
/// slightly late (fail-open).
pub fn create_sub_key(host: &impl KeyHost, name: &str) -> Result<SubGatewayKey, KeyError> {
    let name = validate_name(name)?;
    refresh_snapshot(host);
    let active_count = host
        .count_active_sub_gateway_keys()
        .map_err(|error| KeyError::internal("failed to count keys", error))?;
    if active_count >= MAX_ACTIVE_SUB_KEYS {
        return Err(KeyError::bad_request(format!(
            "at most {MAX_ACTIVE_SUB_KEYS} active keys are supported"
        )));
    }
    let (stored_values, snapshot) = host
        .load_unique_value_inputs()
        .map_err(|error| KeyError::internal("failed to load key values", error))?;
    let entry = SubGatewayKey {
        id: uuid::Uuid::new_v4().to_string(),
        key: generate_unique_value_from(&stored_values, &snapshot, host)?,
        name,
        enabled: true,
        deleted_at: None,
        created_at: Utc::now(),
    };
    let insert = host
        .insert_sub_gateway_key(&entry)
        .map_err(|error| KeyError::internal("failed to create the key", error));
    match insert {
        Ok(()) => {
            refresh_snapshot(host);
            Ok(entry)
        }
        Err(error) => Err(error),
    }
}

/// Renames a non-deleted sub key. The snapshot rebuild afterwards keeps
/// write-time name snapshots current.
pub fn rename_sub_key(host: &impl KeyHost, id: &str, name: &str) -> Result<(), KeyError> {
    let name = validate_name(name)?;
    refresh_snapshot(host);
    let renamed = host
        .rename_sub_gateway_key(id, &name)
        .map_err(|error| KeyError::internal("failed to rename the key", error))?;
    if !renamed {
        return Err(KeyError::bad_request("key not found"));
    }
    refresh_snapshot(host);
    Ok(())
}

/// Enables or disables a non-deleted sub key.
///
/// Enabling revalidates the stored value against the current primary value
/// (the unified gate's third arm) so the disable -> primary adopts value ->
/// re-enable bypass can never create a dual-attributed credential, then
/// commits before rebuilding (fail-open).
///
/// Disabling is a revocation: the snapshot drops the value before the table
/// write (fail-closed) and restores it when the write fails.
pub fn set_sub_key_enabled(host: &impl KeyHost, id: &str, enabled: bool) -> Result<(), KeyError> {
    refresh_snapshot(host);
    let current = active_sub_key(host, id)?;
    if enabled {
        let primary_value = host.primary_gateway_key();
        if current.key == primary_value {
            return Err(KeyError::bad_request(
                "key value collides with the primary key",
            ));
        }
        let updated = host
            .set_sub_gateway_key_enabled(id, true)
            .map_err(|error| KeyError::internal("failed to enable the key", error));
        return match updated {
            Ok(true) => {
                refresh_snapshot(host);
                Ok(())
            }
            Ok(false) => Err(KeyError::bad_request("key not found")),
            Err(error) => Err(error),
        };
    }

    if !current.enabled {
        return Ok(());
    }
    let revoked = revoke_snapshot_value(host, &current.id, &current.key);
    let updated = host
        .set_sub_gateway_key_enabled(id, false)
        .map_err(|error| KeyError::internal("failed to disable the key", error));
    match updated {
        Ok(true) => Ok(()),
        Ok(false) => {
            if let Some(entry) = revoked {
                restore_snapshot_entry(host, &current.key, entry);
            }
            Err(KeyError::bad_request("key not found"))
        }
        Err(error) => {
            if let Some(entry) = revoked {
                restore_snapshot_entry(host, &current.key, entry);
            }
            Err(error)
        }
    }
}

/// Assigns a fresh unique value to a non-deleted sub key. Revocation of the
/// old value is fail-closed: the snapshot swaps first and rolls back when
/// the table write fails. Regenerating a disabled key rotates its stored
/// value but does NOT grant the new value: disabled credentials never
/// authenticate (spec MUST), so the snapshot entry only appears once the
/// key is re-enabled (which revalidates the value against the primary).
pub fn regenerate_sub_key(host: &impl KeyHost, id: &str) -> Result<SubGatewayKey, KeyError> {
    refresh_snapshot(host);
    let current = active_sub_key(host, id)?;
    let (stored_values, snapshot) = host
        .load_unique_value_inputs()
        .map_err(|error| KeyError::internal("failed to load key values", error))?;
    let new_value = generate_unique_value_from(&stored_values, &snapshot, host)?;
    let revoked = revoke_snapshot_value(host, &current.id, &current.key);
    if current.enabled {
        let granted = CredentialEntry {
            id: current.id.clone(),
            name: current.name.clone(),
        };
        host.with_credential_snapshot_mut(|snapshot| {
            snapshot.insert(new_value.clone(), granted);
        });
    }
    let updated = host
        .update_sub_gateway_key_value(id, &new_value)
        .map_err(|error| KeyError::internal("failed to regenerate the key", error));
    match updated {
        Ok(true) => Ok(SubGatewayKey {
            key: new_value,
            ..current
        }),
        Ok(false) => {
            rollback_snapshot_swap(host, &current.key, revoked, &new_value);
            Err(KeyError::bad_request("key not found"))
        }
        Err(error) => {
            rollback_snapshot_swap(host, &current.key, revoked, &new_value);
            Err(error)
        }
    }
}

fn rollback_snapshot_swap(
    host: &impl KeyHost,
    old_value: &str,
    revoked: Option<CredentialEntry>,
    new_value: &str,
) {
    // Remove/insert on the HashMap cannot fail; if this ever changes, the
    // entry-point rebuild on the next key API operation converges anyway.
    host.with_credential_snapshot_mut(|snapshot| {
        snapshot.remove(new_value);
    });
    if let Some(entry) = revoked {
        restore_snapshot_entry(host, old_value, entry);
    }
}

/// Soft-deletes a non-deleted sub key: clears the plaintext, keeps id/name/
/// deleted_at for attribution. Revocation is fail-closed; the snapshot drops
/// the value before the table write and restores it on failure.
pub fn delete_sub_key(host: &impl KeyHost, id: &str, now: DateTime<Utc>) -> Result<(), KeyError> {
    refresh_snapshot(host);
    let current = active_sub_key(host, id)?;
    let revoked = revoke_snapshot_value(host, &current.id, &current.key);
    let deleted = host
        .soft_delete_sub_gateway_key(id, now)
        .map_err(|error| KeyError::internal("failed to delete the key", error));
    match deleted {
        Ok(true) => Ok(()),
        Ok(false) => {
            if let Some(entry) = revoked {
                restore_snapshot_entry(host, &current.key, entry);
            }
            Err(KeyError::bad_request("key not found"))
        }
        Err(error) => {
            if let Some(entry) = revoked {
                restore_snapshot_entry(host, &current.key, entry);
            }
            Err(error)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::{KeyCipher, StaticKeyCipher};
    use crate::db::Database;
    use crate::models::AppConfig;
    use crate::state::CoreStateInner;
    use std::collections::HashSet;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU64, Ordering};

    fn temp_state(label: &str) -> (PathBuf, Arc<CoreStateInner>) {
        let mut dir = std::env::temp_dir();
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock should be valid")
            .as_nanos();
        dir.push(format!("ocg-sub-keys-{label}-{nanos}"));
        fs::create_dir_all(&dir).expect("test data directory should be created");
        let db = Database::open(dir.clone()).expect("test database should open");
        let cipher: Arc<dyn KeyCipher + Send + Sync> = Arc::new(StaticKeyCipher::new("test"));
        let state = CoreStateInner::new(db, dir.clone(), cipher).expect("state should initialize");
        (dir, Arc::new(state))
    }

    fn snapshot_values(state: &CoreStateInner) -> HashSet<String> {
        state.credential_snapshot.read().keys().cloned().collect()
    }

    struct MemoryKeys {
        keys: std::sync::Mutex<Vec<SubGatewayKey>>,
        primary: std::sync::Mutex<String>,
        snapshot: std::sync::Mutex<CredentialSnapshot>,
        word: AtomicU64,
    }

    impl MemoryKeys {
        fn new(primary: &str) -> Self {
            let mut snapshot = CredentialSnapshot::new();
            if !primary.is_empty() {
                snapshot.insert(
                    primary.to_string(),
                    CredentialEntry {
                        id: PRIMARY_KEY_ID.to_string(),
                        name: PRIMARY_KEY_NAME.to_string(),
                    },
                );
            }
            Self {
                keys: std::sync::Mutex::new(Vec::new()),
                primary: std::sync::Mutex::new(primary.to_string()),
                snapshot: std::sync::Mutex::new(snapshot),
                word: AtomicU64::new(0),
            }
        }

        fn active_keys(&self) -> Vec<SubGatewayKey> {
            self.keys
                .lock()
                .expect("memory keys")
                .iter()
                .filter(|key| key.is_active())
                .cloned()
                .collect()
        }
    }

    impl KeyStore for MemoryKeys {
        fn list_active_sub_gateway_keys(&self) -> anyhow::Result<Vec<SubGatewayKey>> {
            Ok(self.active_keys())
        }
        fn get_sub_gateway_key(&self, id: &str) -> anyhow::Result<Option<SubGatewayKey>> {
            Ok(self
                .keys
                .lock()
                .expect("memory keys")
                .iter()
                .find(|key| key.id == id)
                .cloned())
        }
        fn count_active_sub_gateway_keys(&self) -> anyhow::Result<usize> {
            Ok(self.active_keys().len())
        }
        fn insert_sub_gateway_key(&self, key: &SubGatewayKey) -> anyhow::Result<()> {
            self.keys.lock().expect("memory keys").push(key.clone());
            Ok(())
        }
        fn rename_sub_gateway_key(&self, id: &str, name: &str) -> anyhow::Result<bool> {
            let mut keys = self.keys.lock().expect("memory keys");
            match keys.iter_mut().find(|key| key.id == id && key.is_active()) {
                Some(key) => {
                    key.name = name.to_string();
                    Ok(true)
                }
                None => Ok(false),
            }
        }
        fn set_sub_gateway_key_enabled(&self, id: &str, enabled: bool) -> anyhow::Result<bool> {
            let mut keys = self.keys.lock().expect("memory keys");
            match keys.iter_mut().find(|key| key.id == id && key.is_active()) {
                Some(key) => {
                    key.enabled = enabled;
                    Ok(true)
                }
                None => Ok(false),
            }
        }
        fn update_sub_gateway_key_value(&self, id: &str, new_value: &str) -> anyhow::Result<bool> {
            let mut keys = self.keys.lock().expect("memory keys");
            match keys.iter_mut().find(|key| key.id == id && key.is_active()) {
                Some(key) => {
                    key.key = new_value.to_string();
                    Ok(true)
                }
                None => Ok(false),
            }
        }
        fn soft_delete_sub_gateway_key(
            &self,
            id: &str,
            now: DateTime<Utc>,
        ) -> anyhow::Result<bool> {
            let mut keys = self.keys.lock().expect("memory keys");
            match keys.iter_mut().find(|key| key.id == id && key.is_active()) {
                Some(key) => {
                    key.key.clear();
                    key.enabled = false;
                    key.deleted_at = Some(now);
                    Ok(true)
                }
                None => Ok(false),
            }
        }
        fn active_sub_gateway_key_values(&self) -> anyhow::Result<Vec<String>> {
            Ok(self
                .active_keys()
                .into_iter()
                .map(|key| key.key)
                .filter(|value| !value.is_empty())
                .collect())
        }
        fn sub_gateway_key_value_exists(&self, value: &str) -> anyhow::Result<bool> {
            Ok(self
                .active_keys()
                .iter()
                .any(|key| key.key == value && !key.key.is_empty()))
        }
        fn random_word(&self) -> String {
            format!("w{:04}", self.word.fetch_add(1, Ordering::Relaxed))
        }
    }

    impl KeyHost for MemoryKeys {
        fn primary_gateway_key(&self) -> String {
            self.primary.lock().expect("primary").clone()
        }
        fn clone_credential_snapshot(&self) -> CredentialSnapshot {
            self.snapshot.lock().expect("snapshot").clone()
        }
        fn replace_credential_snapshot(&self, snapshot: CredentialSnapshot) {
            *self.snapshot.lock().expect("snapshot") = snapshot;
        }
        fn with_credential_snapshot_mut<R>(
            &self,
            f: impl FnOnce(&mut CredentialSnapshot) -> R,
        ) -> R {
            f(&mut self.snapshot.lock().expect("snapshot"))
        }
        fn load_unique_value_inputs(&self) -> anyhow::Result<(Vec<String>, CredentialSnapshot)> {
            Ok((
                self.active_sub_gateway_key_values()?,
                self.clone_credential_snapshot(),
            ))
        }
        fn load_snapshot_rebuild_inputs(&self) -> anyhow::Result<(Vec<SubGatewayKey>, String)> {
            Ok((self.active_keys(), self.primary_gateway_key()))
        }
    }

    #[test]
    fn create_returns_full_value_and_authenticates_via_snapshot() {
        let (dir, state) = temp_state("create");
        let primary = state.config().gateway_key;
        let created = create_sub_key(&state, " Laptop ").expect("sub key should create");
        assert_eq!(created.name, "Laptop");
        assert!(created.authenticates());
        assert_ne!(created.key, primary);
        let values = snapshot_values(&state);
        assert!(values.contains(&primary));
        assert!(values.contains(&created.key));
        let stored = state
            .db
            .lock()
            .get_sub_gateway_key(&created.id)
            .unwrap()
            .expect("created key should persist");
        assert_eq!(stored.key, created.key);

        drop(state);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn create_rejects_blank_overlong_names_and_the_active_ceiling() {
        let (dir, state) = temp_state("limits");
        assert!(matches!(
            create_sub_key(&state, "  "),
            Err(KeyError::BadRequest(_))
        ));
        assert!(matches!(
            create_sub_key(&state, &"x".repeat(65)),
            Err(KeyError::BadRequest(_))
        ));
        for index in 0..MAX_ACTIVE_SUB_KEYS {
            create_sub_key(&state, &format!("key-{index}"))
                .expect("keys below the ceiling should create");
        }
        let overflow = create_sub_key(&state, "overflow");
        assert_eq!(
            overflow.unwrap_err(),
            KeyError::bad_request(format!(
                "at most {MAX_ACTIVE_SUB_KEYS} active keys are supported"
            ))
        );
        // Tombstones do not count: deleting one frees a slot.
        let retired = state.db.lock().list_active_sub_gateway_keys().unwrap()[0]
            .id
            .clone();
        delete_sub_key(&state, &retired, Utc::now()).expect("delete should work");
        create_sub_key(&state, "fresh").expect("deleted key frees a slot");

        drop(state);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn rename_updates_the_name_snapshot_for_later_rows() {
        let (dir, state) = temp_state("rename");
        let created = create_sub_key(&state, "Laptop").expect("sub key should create");
        rename_sub_key(&state, &created.id, "Deck").expect("rename should work");
        assert_eq!(
            state.client_key_name(&created.id).as_deref(),
            Some("Deck"),
            "the snapshot must serve the new name to log writes"
        );
        assert!(matches!(
            rename_sub_key(&state, "missing", "x"),
            Err(KeyError::BadRequest(_))
        ));

        drop(state);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn disable_is_fail_closed_and_reenable_checks_the_primary_value() {
        let (dir, state) = temp_state("disable");
        let created = create_sub_key(&state, "Laptop").expect("sub key should create");

        set_sub_key_enabled(&state, &created.id, false).expect("disable should work");
        assert!(!snapshot_values(&state).contains(&created.key));
        let stored = state
            .db
            .lock()
            .get_sub_gateway_key(&created.id)
            .unwrap()
            .unwrap();
        assert!(!stored.enabled, "disabled keys keep their plaintext");
        assert_eq!(stored.key, created.key);

        // The bypass sequence: an unchecked writer makes the primary adopt
        // the disabled key's value, then re-enabling must be rejected.
        let mut config = state.config();
        config.gateway_key = created.key.clone();
        state
            .set_config(config)
            .expect("set_config itself carries no cross-tier gate");
        let re_enable = set_sub_key_enabled(&state, &created.id, true);
        assert_eq!(
            re_enable.unwrap_err(),
            KeyError::bad_request("key value collides with the primary key")
        );

        // Repair the collision and re-enable normally.
        let mut config = state.config();
        config.gateway_key = "ocg-primary-restored".to_string();
        state.set_config(config).expect("repair should save");
        set_sub_key_enabled(&state, &created.id, true).expect("re-enable should work");
        assert!(snapshot_values(&state).contains(&created.key));

        drop(state);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn regenerate_swaps_the_snapshot_value_and_keeps_attribution() {
        let (dir, state) = temp_state("regenerate");
        let created = create_sub_key(&state, "Laptop").expect("sub key should create");
        let regenerated = regenerate_sub_key(&state, &created.id).expect("regenerate should work");
        assert_ne!(regenerated.key, created.key);
        let values = snapshot_values(&state);
        assert!(!values.contains(&created.key), "the old value is revoked");
        assert!(values.contains(&regenerated.key));
        assert_eq!(regenerated.id, created.id);
        assert_eq!(
            state.client_key_name(&created.id).as_deref(),
            Some("Laptop")
        );

        drop(state);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn regenerating_a_disabled_sub_key_does_not_grant_the_new_value() {
        let (dir, state) = temp_state("regenerate-disabled");
        let created = create_sub_key(&state, "Laptop").expect("sub key should create");
        set_sub_key_enabled(&state, &created.id, false).expect("disable should work");
        let regenerated = regenerate_sub_key(&state, &created.id).expect("regenerate should work");
        assert!(
            state.credential_entry_for_value(&regenerated.key).is_none(),
            "a disabled key's fresh value must not authenticate"
        );
        let stored = state
            .db
            .lock()
            .get_sub_gateway_key(&created.id)
            .unwrap()
            .unwrap();
        assert!(!stored.enabled, "regeneration must not re-enable the key");
        assert_eq!(stored.key, regenerated.key);

        // Re-enabling (with a non-colliding value) puts the value back.
        set_sub_key_enabled(&state, &created.id, true).expect("re-enable should work");
        assert!(state.credential_entry_for_value(&regenerated.key).is_some());

        drop(state);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn delete_clears_plaintext_and_keeps_the_attribution_record() {
        let (dir, state) = temp_state("delete");
        let created = create_sub_key(&state, "Laptop").expect("sub key should create");
        delete_sub_key(&state, &created.id, Utc::now()).expect("delete should work");
        let tombstone = state
            .db
            .lock()
            .get_sub_gateway_key(&created.id)
            .unwrap()
            .expect("tombstone should persist");
        assert!(tombstone.deleted_at.is_some());
        assert!(tombstone.key.is_empty());
        assert_eq!(tombstone.name, "Laptop");
        assert!(!snapshot_values(&state).contains(&created.key));
        assert!(matches!(
            delete_sub_key(&state, &created.id, Utc::now()),
            Err(KeyError::BadRequest(_))
        ));

        drop(state);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn primary_value_gate_rejects_values_held_by_non_deleted_sub_keys() {
        let (dir, state) = temp_state("gate");
        let created = create_sub_key(&state, "Laptop").expect("sub key should create");
        set_sub_key_enabled(&state, &created.id, false).expect("disable should work");
        {
            let db = state.db.lock();
            assert!(ensure_primary_value_allowed(&db, &created.key).is_err());
        }
        delete_sub_key(&state, &created.id, Utc::now()).expect("delete should work");
        {
            let db = state.db.lock();
            ensure_primary_value_allowed(&db, &created.key)
                .expect("tombstoned values are free for the primary");
        }

        drop(state);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn primary_attribution_survives_an_out_of_model_value_collision() {
        let (dir, state) = temp_state("collision-hardening");
        let created = create_sub_key(&state, "Laptop").expect("sub key should create");
        // An unchecked writer makes the primary adopt the enabled sub key's
        // value (out of model; every real settings writer gates this).
        let mut config = state.config();
        config.gateway_key = created.key.clone();
        state.set_config(config).expect("save should work");
        // Any key API entry point rebuilds the snapshot: the shared value
        // stays attributed to the primary.
        refresh_snapshot(&state);
        let entry = state.credential_entry_for_value(&created.key).unwrap();
        assert_eq!(entry.id, crate::gateway_keys::PRIMARY_KEY_ID);
        // Revoking the sub key never evicts the primary's live entry.
        delete_sub_key(&state, &created.id, Utc::now()).expect("delete should work");
        assert!(
            state.credential_entry_for_value(&created.key).is_some(),
            "the primary keeps authenticating after the colliding sub key is deleted"
        );

        drop(state);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn set_config_refreshes_the_primary_snapshot_entry() {
        let (dir, state) = temp_state("primary-refresh");
        let mut config = AppConfig {
            gateway_key: "ocg-custom-primary".to_string(),
            ..state.config()
        };
        state.set_config(config.clone()).expect("save should work");
        assert!(snapshot_values(&state).contains("ocg-custom-primary"));
        assert_eq!(
            state.client_key_name(PRIMARY_KEY_ID).as_deref(),
            Some(PRIMARY_KEY_NAME)
        );

        config.gateway_key = "  ".to_string();
        assert!(state.set_config(config).is_err(), "blank keys are rejected");
        assert!(snapshot_values(&state).contains("ocg-custom-primary"));

        drop(state);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn key_store_seam_enforces_uniqueness_without_process_host() {
        let store = MemoryKeys::new("ocg-primary");
        let created = create_sub_key(&store, "Laptop").expect("memory host should create");
        assert!(ensure_primary_value_allowed(&store, &created.key).is_err());
        let rotated = generate_primary_value(&store, "ocg-primary").expect("rotate");
        assert_ne!(rotated, created.key);
        assert_ne!(rotated, "ocg-primary");
        assert!(rotated.starts_with("ocg-w"));
    }

    #[test]
    fn key_host_seam_revokes_fail_closed_without_process_host() {
        let store = MemoryKeys::new("ocg-primary");
        let created = create_sub_key(&store, "Laptop").expect("memory host should create");
        assert!(store.clone_credential_snapshot().contains_key(&created.key));
        set_sub_key_enabled(&store, &created.id, false).expect("disable");
        assert!(!store.clone_credential_snapshot().contains_key(&created.key));
        *store.primary.lock().expect("primary") = created.key.clone();
        assert_eq!(
            set_sub_key_enabled(&store, &created.id, true).unwrap_err(),
            KeyError::bad_request("key value collides with the primary key")
        );
        delete_sub_key(&store, &created.id, Utc::now()).expect("delete");
        let tombstone = store
            .get_sub_gateway_key(&created.id)
            .unwrap()
            .expect("tombstone");
        assert!(tombstone.deleted_at.is_some());
        assert!(tombstone.key.is_empty());
        assert_eq!(tombstone.name, "Laptop");
    }
}
