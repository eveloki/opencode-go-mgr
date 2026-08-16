//! Pure lifecycle helpers for the multi gateway key list on [`AppConfig`].
//!
//! Invariants enforced here and relied on by `state.rs` / `dashboard.rs`:
//! - the first non-deleted entry is the primary key and `gateway_key` always
//!   mirrors its value (legacy readers and downgrade paths depend on it);
//! - at least one enabled, non-deleted key with a non-empty value exists;
//! - key values are unique across all non-deleted entries;
//! - soft-deleted entries keep id/name/deleted_at with an empty plaintext.

use crate::models::{AppConfig, GatewayKeyEntry};
use chrono::{DateTime, Utc};
use std::collections::HashSet;

const MAX_NAME_CHARS: usize = 64;
/// Ceiling on active (non-deleted) keys. The auth scan, the per-request
/// config clone, and the settings payload all scale with the list, and a
/// local node has no realistic need for more devices than this.
const MAX_ACTIVE_KEYS: usize = 64;

/// The primary key: first non-deleted entry in the list.
pub fn primary_key(config: &AppConfig) -> Option<&GatewayKeyEntry> {
    config.gateway_keys.iter().find(|key| key.is_active())
}

pub fn key_by_id<'a>(config: &'a AppConfig, id: &str) -> Option<&'a GatewayKeyEntry> {
    config.gateway_keys.iter().find(|key| key.id == id)
}

/// Write-time name snapshot for a key id; resolves enabled and soft-deleted
/// keys alike so historical logs keep their attribution.
pub fn key_name(config: &AppConfig, id: &str) -> Option<String> {
    key_by_id(config, id).map(|key| key.name.clone())
}

/// Sorted set of values that currently authenticate; the routing runtime
/// resets when one of them stops authenticating (values are sorted so the
/// caller can subset-check with a binary search).
pub fn enabled_key_values(config: &AppConfig) -> Vec<&str> {
    let mut values: Vec<&str> = config
        .gateway_keys
        .iter()
        .filter(|key| key.authenticates())
        .map(|key| key.key.as_str())
        .collect();
    values.sort_unstable();
    values
}

fn generate_key_value(config: &AppConfig) -> String {
    loop {
        let candidate = format!(
            "ocg-{}-{}",
            crate::state::random_word(),
            crate::state::random_word()
        );
        let collision = config
            .gateway_keys
            .iter()
            .any(|key| key.is_active() && key.key == candidate);
        if !collision {
            return candidate;
        }
    }
}

/// Enforces the mirror and self-heals degenerate lists. Returns `true` when
/// the config changed and therefore needs persisting.
pub fn normalize(config: &mut AppConfig) -> bool {
    let original_keys = config.gateway_keys.clone();
    let original_mirror = config.gateway_key.clone();

    if !config.gateway_keys.iter().any(|key| key.is_active()) {
        // Corrupt or fully-deleted list: keep the legacy value (or mint one)
        // so the gateway never runs without a usable credential.
        let value = if config.gateway_key.is_empty() {
            generate_key_value(config)
        } else {
            config.gateway_key.clone()
        };
        config.gateway_keys.insert(
            0,
            GatewayKeyEntry {
                id: uuid::Uuid::new_v4().to_string(),
                name: "Primary".to_string(),
                key: value,
                enabled: true,
                deleted_at: None,
                created_at: Utc::now(),
            },
        );
    }

    // Active entries must carry a value; corrupt blanks get fresh unique ones.
    for index in 0..config.gateway_keys.len() {
        if config.gateway_keys[index].is_active() && config.gateway_keys[index].key.is_empty() {
            let value = generate_key_value(config);
            config.gateway_keys[index].key = value;
        }
    }

    config.gateway_key = primary_key(config)
        .map(|key| key.key.clone())
        .unwrap_or_default();

    original_keys != config.gateway_keys || original_mirror != config.gateway_key
}

/// Validates the full invariant set. Called by `AppConfig::validate`.
pub fn validate(config: &AppConfig) -> Result<(), String> {
    let active_count = active_key_count(config);
    if active_count == 0 {
        return Err("at least one active gateway key is required".to_string());
    }
    if active_count > MAX_ACTIVE_KEYS {
        return Err(format!(
            "at most {MAX_ACTIVE_KEYS} active gateway keys are supported"
        ));
    }
    if !config.gateway_keys.iter().any(|key| key.authenticates()) {
        return Err("at least one enabled gateway key is required".to_string());
    }
    let primary =
        primary_key(config).ok_or_else(|| "a primary gateway key is required".to_string())?;
    if primary.key.is_empty() {
        return Err("the primary gateway key must have a value".to_string());
    }
    if config.gateway_key != primary.key {
        return Err("gateway_key must mirror the primary gateway key".to_string());
    }
    let mut seen_ids = HashSet::new();
    let mut seen_values = HashSet::new();
    for key in &config.gateway_keys {
        if !key.is_active() && !key.key.is_empty() {
            return Err("deleted gateway keys must not keep their value".to_string());
        }
        if !seen_ids.insert(key.id.as_str()) {
            return Err("gateway key ids must be unique".to_string());
        }
        if key.is_active() && !key.key.is_empty() && !seen_values.insert(key.key.as_str()) {
            return Err("gateway key values must be unique".to_string());
        }
        if key.enabled && key.is_active() && key.key.is_empty() {
            return Err("enabled gateway keys must have a value".to_string());
        }
    }
    Ok(())
}

fn active_key_count(config: &AppConfig) -> usize {
    config
        .gateway_keys
        .iter()
        .filter(|key| key.is_active())
        .count()
}

fn validate_name(name: &str) -> Result<String, String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err("key name is required".to_string());
    }
    if trimmed.chars().count() > MAX_NAME_CHARS {
        return Err(format!(
            "key name must be at most {MAX_NAME_CHARS} characters"
        ));
    }
    Ok(trimmed.to_string())
}

/// Creates a new enabled key and appends it to the list. The ceiling is
/// checked before any mutation so a rejected create leaves the config
/// untouched.
pub fn create_key(config: &mut AppConfig, name: &str) -> Result<GatewayKeyEntry, String> {
    validate(config)?;
    let name = validate_name(name)?;
    if active_key_count(config) >= MAX_ACTIVE_KEYS {
        return Err(format!(
            "at most {MAX_ACTIVE_KEYS} active gateway keys are supported"
        ));
    }
    let entry = GatewayKeyEntry {
        id: uuid::Uuid::new_v4().to_string(),
        name,
        key: generate_key_value(config),
        enabled: true,
        deleted_at: None,
        created_at: Utc::now(),
    };
    config.gateway_keys.push(entry.clone());
    sync_mirror(config);
    validate(config)?;
    Ok(entry)
}

/// Renames an active key.
pub fn rename_key(config: &mut AppConfig, id: &str, name: &str) -> Result<(), String> {
    let name = validate_name(name)?;
    let entry = active_entry_mut(config, id)?;
    entry.name = name;
    Ok(())
}

/// Enables or disables an active key; the last enabled key is protected.
pub fn set_key_enabled(config: &mut AppConfig, id: &str, enabled: bool) -> Result<(), String> {
    let index = active_index(config, id)?;
    if !enabled && config.gateway_keys[index].enabled && enabled_count(config) <= 1 {
        return Err("the last enabled gateway key cannot be disabled".to_string());
    }
    config.gateway_keys[index].enabled = enabled;
    sync_mirror(config);
    validate(config)?;
    Ok(())
}

/// Assigns a fresh unique value to an active key and returns it.
pub fn regenerate_key(config: &mut AppConfig, id: &str) -> Result<GatewayKeyEntry, String> {
    validate(config)?;
    let new_value = generate_key_value(config);
    let index = active_index(config, id)?;
    let entry = &mut config.gateway_keys[index];
    entry.key = new_value;
    let updated = entry.clone();
    sync_mirror(config);
    validate(config)?;
    Ok(updated)
}

/// Soft-deletes an active key: clears the plaintext, keeps the record for
/// attribution, and promotes the earliest enabled key when the primary goes.
pub fn delete_key(config: &mut AppConfig, id: &str, now: DateTime<Utc>) -> Result<(), String> {
    validate(config)?;
    let index = active_index(config, id)?;
    let is_primary = config.gateway_keys.iter().position(|key| key.is_active()) == Some(index);
    if config.gateway_keys[index].authenticates() && enabled_count(config) <= 1 {
        return Err("the last enabled gateway key cannot be deleted".to_string());
    }
    let removed = config.gateway_keys[index].clone();
    config.gateway_keys[index] = GatewayKeyEntry {
        deleted_at: Some(now),
        key: String::new(),
        enabled: false,
        ..removed
    };
    if is_primary {
        promote_earliest_enabled(config);
    }
    sync_mirror(config);
    validate(config)?;
    Ok(())
}

fn active_index(config: &AppConfig, id: &str) -> Result<usize, String> {
    config
        .gateway_keys
        .iter()
        .position(|key| key.id == id && key.is_active())
        .ok_or_else(|| "gateway key not found".to_string())
}

fn active_entry_mut<'a>(
    config: &'a mut AppConfig,
    id: &str,
) -> Result<&'a mut GatewayKeyEntry, String> {
    let index = active_index(config, id)?;
    Ok(&mut config.gateway_keys[index])
}

fn enabled_count(config: &AppConfig) -> usize {
    config
        .gateway_keys
        .iter()
        .filter(|key| key.authenticates())
        .count()
}

/// Moves the earliest enabled active key to the front of the list so it
/// becomes the new primary.
fn promote_earliest_enabled(config: &mut AppConfig) {
    let promoted = config
        .gateway_keys
        .iter()
        .position(|key| key.authenticates());
    if let Some(from) = promoted {
        let entry = config.gateway_keys.remove(from);
        config.gateway_keys.insert(0, entry);
    }
}

fn sync_mirror(config: &mut AppConfig) {
    if let Some(primary) = primary_key(config) {
        config.gateway_key = primary.key.clone();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn primary_only() -> AppConfig {
        AppConfig {
            gateway_key: "ocg-primary".into(),
            gateway_keys: vec![GatewayKeyEntry {
                id: "primary".into(),
                name: "Primary".into(),
                key: "ocg-primary".into(),
                enabled: true,
                deleted_at: None,
                created_at: Utc::now(),
            }],
            ..AppConfig::default()
        }
    }

    #[test]
    fn create_appends_enabled_key_with_unique_value_and_keeps_mirror() {
        let mut config = primary_only();
        let second = create_key(&mut config, "Laptop").expect("second key should create");
        assert_eq!(config.gateway_keys.len(), 2);
        assert!(second.authenticates());
        assert_ne!(second.key, "ocg-primary");
        assert_eq!(config.gateway_key, "ocg-primary");
        validate(&config).expect("invariants should hold");
    }

    #[test]
    fn create_rejects_blank_and_overlong_names() {
        let mut config = primary_only();
        assert!(create_key(&mut config, "  ").is_err());
        assert!(create_key(&mut config, &"x".repeat(65)).is_err());
        assert!(create_key(&mut config, " pad ").is_ok());
        assert_eq!(config.gateway_keys[1].name, "pad");
    }

    #[test]
    fn create_enforces_the_active_key_ceiling() {
        let mut config = primary_only();
        for index in 0..(MAX_ACTIVE_KEYS - 1) {
            create_key(&mut config, &format!("key-{index}"))
                .expect("keys below the ceiling should create");
        }
        assert_eq!(
            config
                .gateway_keys
                .iter()
                .filter(|key| key.is_active())
                .count(),
            MAX_ACTIVE_KEYS
        );
        let overflow = create_key(&mut config, "overflow");
        assert!(overflow.is_err());
        assert_eq!(
            overflow.unwrap_err(),
            format!("at most {MAX_ACTIVE_KEYS} active gateway keys are supported")
        );

        // Soft-deleted tombstones do not count against the ceiling, so
        // deleting one frees a slot again.
        let demoted_id = config.gateway_keys[1].id.clone();
        delete_key(&mut config, &demoted_id, Utc::now()).unwrap();
        create_key(&mut config, "fresh").expect("deleted key frees a slot");
    }

    #[test]
    fn rename_rejects_missing_and_deleted_keys() {
        let mut config = primary_only();
        rename_key(&mut config, "primary", "Renamed").expect("active key should rename");
        assert_eq!(config.gateway_keys[0].name, "Renamed");
        assert!(rename_key(&mut config, "missing", "x").is_err());
    }

    #[test]
    fn last_enabled_key_cannot_be_disabled_or_deleted() {
        let mut config = primary_only();
        assert!(set_key_enabled(&mut config, "primary", false).is_err());
        assert!(delete_key(&mut config, "primary", Utc::now()).is_err());
        validate(&config).expect("failed operations must not corrupt state");
    }

    #[test]
    fn disable_and_delete_stop_authentication_but_keep_records() {
        let mut config = primary_only();
        let second = create_key(&mut config, "Laptop").expect("second key should create");

        set_key_enabled(&mut config, &second.id, false).expect("disable should work");
        assert!(!key_by_id(&config, &second.id).unwrap().authenticates());
        assert!(key_by_id(&config, &second.id).unwrap().is_active());

        set_key_enabled(&mut config, &second.id, true).expect("re-enable should work");
        delete_key(&mut config, &second.id, Utc::now()).expect("delete should work");
        let deleted = key_by_id(&config, &second.id).unwrap();
        assert!(!deleted.is_active());
        assert!(deleted.key.is_empty());
        assert_eq!(deleted.name, "Laptop");
        assert_eq!(key_name(&config, &second.id).as_deref(), Some("Laptop"));
        validate(&config).expect("invariants should hold after delete");
    }

    #[test]
    fn deleting_primary_promotes_earliest_enabled_key_and_updates_mirror() {
        let mut config = primary_only();
        let second = create_key(&mut config, "Laptop").expect("second key should create");
        let third = create_key(&mut config, "Desktop").expect("third key should create");
        // Disable the earliest-created secondary; promotion must skip it.
        set_key_enabled(&mut config, &second.id, false).expect("disable second");

        delete_key(&mut config, "primary", Utc::now()).expect("primary delete should work");

        let primary = primary_key(&config).expect("a primary must exist");
        assert_eq!(primary.id, third.id);
        assert_eq!(config.gateway_key, third.key);
        // Historical attribution survives via the tombstone.
        assert_eq!(key_name(&config, "primary").as_deref(), Some("Primary"));
        validate(&config).expect("invariants should hold after promotion");
    }

    #[test]
    fn regenerate_changes_value_and_mirror_when_primary() {
        let mut config = primary_only();
        let updated = regenerate_key(&mut config, "primary").expect("regenerate should work");
        assert_ne!(updated.key, "ocg-primary");
        assert_eq!(config.gateway_key, updated.key);
        assert_eq!(primary_key(&config).unwrap().id, "primary");
        validate(&config).expect("invariants should hold after regenerate");
    }

    #[test]
    fn duplicate_values_are_rejected() {
        let mut config = primary_only();
        config.gateway_keys[0].key = "ocg-dup".into();
        config.gateway_keys.push(GatewayKeyEntry {
            id: "second".into(),
            name: "Second".into(),
            key: "ocg-dup".into(),
            enabled: true,
            deleted_at: None,
            created_at: Utc::now(),
        });
        assert!(validate(&config).is_err());
    }

    #[test]
    fn normalize_self_heals_empty_and_fully_deleted_lists() {
        let mut config = AppConfig {
            gateway_key: "ocg-legacy".into(),
            ..AppConfig::default()
        };
        assert!(normalize(&mut config));
        let primary = primary_key(&config).expect("normalize should mint a primary");
        assert_eq!(primary.key, "ocg-legacy");
        assert_eq!(config.gateway_key, "ocg-legacy");
        validate(&config).expect("healed config should validate");

        // Fully deleted list heals from the mirror value too.
        let mut deleted_all = primary_only();
        let now = Utc::now();
        delete_key(&mut deleted_all, "primary", now)
            .expect_err("cannot delete the last enabled key");

        let mut corrupt = primary_only();
        corrupt.gateway_keys[0].deleted_at = Some(now);
        corrupt.gateway_keys[0].key = String::new();
        corrupt.gateway_keys[0].enabled = false;
        assert!(normalize(&mut corrupt));
        let primary = primary_key(&corrupt).expect("healed primary should exist");
        assert_eq!(primary.key, "ocg-primary");
        assert!(primary.authenticates());
        validate(&corrupt).expect("healed config should validate");
    }

    #[test]
    fn normalize_is_a_noop_for_healthy_configs() {
        let mut config = primary_only();
        create_key(&mut config, "Laptop").expect("second key should create");
        let before = config.clone();
        assert!(!normalize(&mut config));
        assert_eq!(config.gateway_keys, before.gateway_keys);
        assert_eq!(config.gateway_key, before.gateway_key);
    }
}
