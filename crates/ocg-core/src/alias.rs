//! Hardcoded unified model alias registry.
//!
//! Outbound clients should send stable lowercase kebab-case aliases. Existing
//! OpenCode Go model IDs are the preferred aliases in this slice. Case-folded
//! kebab spellings such as `GLM-5.2` are accepted. Names containing `/`, `_`,
//! or whitespace are treated as raw IDs and never folded onto a kebab alias
//! (`glm/5.2` is not `glm-5.2`). A raw upstream model ID is accepted only
//! when it uniquely selects one provider mapping; ambiguity returns
//! [`ResolveError::Ambiguous`] with code [`AMBIGUOUS_MODEL_ID`].
//!
//! Command Code GOAT has one official non-routeable mapping:
//! Alias `deepseek-v4-flash` → raw `deepseek/deepseek-v4-flash`. The kebab
//! alias stays Go-owned and published; the unique slash raw ID pins to GOAT
//! and is not production-selectable. SCNet stays fail-closed. Eligible
//! Custom capabilities overlay published aliases and resolve otherwise
//! unknown IDs without stealing Go/Zen mappings.
//! Later provider adapters consume [`ProviderMapping`] from
//! [`crate::gateway::materialize`]: parse the client protocol once, then
//! materialize model / protocol / endpoint / auth per candidate. Adapters must
//! not probe a billable inference path to discover protocol support. The
//! OpenCode `MODEL_PROTOCOLS` table stays Go-specific.

use crate::custom::custom_model_id_matches;
use crate::kernel::ids::{
    ANONYMOUS_FREE_OFFERING_ID, COMMAND_CODE_GOAT_DEEPSEEK_V4_FLASH_ALIAS,
    COMMAND_CODE_GOAT_DEEPSEEK_V4_FLASH_UPSTREAM, COMMAND_CODE_PROVIDER_ID, CUSTOM_API_OFFERING_ID,
    CUSTOM_PROVIDER_ID, GO_OFFERING_ID, GOAT_OFFERING_ID, OPENCODE_PROVIDER_ID,
    OPENCODE_ZEN_FREE_PROVIDER_ID, is_free_model,
};
use crate::kernel::protocol::supported_model_ids;
use crate::provider::is_custom_api;
use std::collections::BTreeMap;
use std::sync::OnceLock;

/// Machine-readable error code for a raw ID that matches more than one mapping.
pub const AMBIGUOUS_MODEL_ID: &str = "ambiguous_model_id";

/// One provider's upstream identity for a client-facing name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderMapping {
    pub provider_id: &'static str,
    pub offering_id: &'static str,
    pub upstream_model: String,
    /// Production-routeable mappings only. Reserved offerings stay false.
    pub routeable: bool,
}

impl ProviderMapping {
    pub fn is_opencode_go(&self) -> bool {
        self.provider_id == OPENCODE_PROVIDER_ID && self.offering_id == GO_OFFERING_ID
    }

    pub fn is_zen_free(&self) -> bool {
        self.provider_id == OPENCODE_ZEN_FREE_PROVIDER_ID
            && self.offering_id == ANONYMOUS_FREE_OFFERING_ID
    }

    pub fn is_command_code_goat(&self) -> bool {
        crate::provider::is_command_code_goat(self.provider_id, self.offering_id)
    }

    pub fn is_custom_api(&self) -> bool {
        is_custom_api(self.provider_id, self.offering_id)
    }
}

/// A preferred client-facing alias and its provider mappings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AliasEntry {
    pub alias: String,
    pub mappings: Vec<ProviderMapping>,
}

/// Result of looking up a client-supplied model name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolvedModel {
    /// Preferred alias. May follow account order, sticky, and fallback across
    /// routeable mappings (including Zen prefer overlay).
    Alias {
        requested: String,
        alias: String,
        mappings: Vec<ProviderMapping>,
    },
    /// Raw upstream ID that uniquely selected one routeable mapping. Pinned to
    /// that provider; no cross-provider fallback or prefer overlay.
    PinnedRaw {
        requested: String,
        mapping: ProviderMapping,
    },
}

impl ResolvedModel {
    pub fn requested(&self) -> &str {
        match self {
            Self::Alias { requested, .. } | Self::PinnedRaw { requested, .. } => requested,
        }
    }

    pub fn is_pinned(&self) -> bool {
        matches!(self, Self::PinnedRaw { .. })
    }

    pub fn routeable_mappings(&self) -> Vec<&ProviderMapping> {
        match self {
            Self::Alias { mappings, .. } => mappings
                .iter()
                .filter(|mapping| mapping.routeable)
                .collect(),
            Self::PinnedRaw { mapping, .. } if mapping.routeable => vec![mapping],
            Self::PinnedRaw { .. } => Vec::new(),
        }
    }

    /// Alias requests may follow account order, sticky, and fallback.
    /// Unique raw IDs stay pinned to one provider mapping.
    pub fn allows_cross_account_fallback(&self) -> bool {
        matches!(self, Self::Alias { .. })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolveError {
    Unknown {
        requested: String,
    },
    Ambiguous {
        requested: String,
        mappings: Vec<ProviderMapping>,
    },
}

impl ResolveError {
    pub fn code(&self) -> Option<&'static str> {
        match self {
            Self::Ambiguous { .. } => Some(AMBIGUOUS_MODEL_ID),
            Self::Unknown { .. } => None,
        }
    }

    pub fn message(&self) -> String {
        match self {
            Self::Unknown { requested } => format!("unknown model `{requested}`"),
            Self::Ambiguous {
                requested,
                mappings,
            } => {
                let providers = mappings
                    .iter()
                    .map(|mapping| {
                        format!(
                            "{}/{}:{}",
                            mapping.provider_id, mapping.offering_id, mapping.upstream_model
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(", ");
                format!(
                    "{AMBIGUOUS_MODEL_ID}: requested model `{requested}` matches multiple provider mappings ({providers}); send a preferred alias instead of this raw id"
                )
            }
        }
    }
}

static REGISTRY: OnceLock<Registry> = OnceLock::new();

fn registry() -> &'static Registry {
    REGISTRY.get_or_init(build_builtin_registry)
}

fn build_builtin_registry() -> Registry {
    build_registry(&crate::kernel::zen::ZenFreeModelCatalog::default().models)
}

fn build_registry(zen_free_models: &[String]) -> Registry {
    let mut specs = Vec::new();
    for id in supported_model_ids() {
        if id == "big-pickle" || is_free_model(id) {
            continue;
        } else {
            specs.push(AliasEntry {
                alias: id.to_string(),
                mappings: go_alias_mappings(id),
            });
        }
    }
    let mut registry = registry_from_entries(specs);
    for model in zen_free_models {
        if !is_free_model(model) {
            continue;
        }
        if let Some(alias) = crate::kernel::zen::stripped_free_alias(model) {
            insert_mapping(&mut registry, model, zen_mapping(model));
            insert_mapping(&mut registry, alias, zen_mapping(model));
        }
    }
    registry
}

fn go_mapping(upstream_model: &'static str) -> ProviderMapping {
    ProviderMapping {
        provider_id: OPENCODE_PROVIDER_ID,
        offering_id: GO_OFFERING_ID,
        upstream_model: upstream_model.to_string(),
        routeable: true,
    }
}

fn goat_deepseek_v4_flash_mapping() -> ProviderMapping {
    ProviderMapping {
        provider_id: COMMAND_CODE_PROVIDER_ID,
        offering_id: GOAT_OFFERING_ID,
        upstream_model: COMMAND_CODE_GOAT_DEEPSEEK_V4_FLASH_UPSTREAM.to_string(),
        routeable: false,
    }
}

fn go_alias_mappings(upstream_model: &'static str) -> Vec<ProviderMapping> {
    let mut mappings = vec![go_mapping(upstream_model)];
    if upstream_model == COMMAND_CODE_GOAT_DEEPSEEK_V4_FLASH_ALIAS {
        mappings.push(goat_deepseek_v4_flash_mapping());
    }
    mappings
}

fn zen_mapping(upstream_model: &str) -> ProviderMapping {
    ProviderMapping {
        provider_id: OPENCODE_ZEN_FREE_PROVIDER_ID,
        offering_id: ANONYMOUS_FREE_OFFERING_ID,
        upstream_model: upstream_model.to_string(),
        routeable: true,
    }
}

fn custom_mapping(upstream_model: &str) -> ProviderMapping {
    ProviderMapping {
        provider_id: CUSTOM_PROVIDER_ID,
        offering_id: CUSTOM_API_OFFERING_ID,
        upstream_model: upstream_model.to_string(),
        routeable: true,
    }
}

/// Sentinel upstream id for Custom-only resolutions. Per-candidate materialization
/// uses the account's declared capability ID instead of this value.
pub const CUSTOM_DYNAMIC_UPSTREAM: &str = "";

struct Registry {
    aliases: BTreeMap<String, AliasEntry>,
    /// Exact upstream model ID → every mapping that uses it.
    raw_exact: BTreeMap<String, Vec<ProviderMapping>>,
    /// Lowercased exact upstream model ID → every mapping that uses it.
    /// Separator characters are preserved; `glm/5.2` never joins `glm-5.2`.
    raw_folded: BTreeMap<String, Vec<ProviderMapping>>,
}

fn registry_from_entries(entries: Vec<AliasEntry>) -> Registry {
    let mut aliases = BTreeMap::new();
    let mut raw_exact: BTreeMap<String, Vec<ProviderMapping>> = BTreeMap::new();
    let mut raw_folded: BTreeMap<String, Vec<ProviderMapping>> = BTreeMap::new();
    for entry in entries {
        debug_assert!(
            !looks_raw_shaped(&entry.alias),
            "published aliases must be kebab-case without slash, space, or underscore"
        );
        for mapping in &entry.mappings {
            raw_exact
                .entry(mapping.upstream_model.to_string())
                .or_default()
                .push(mapping.clone());
            raw_folded
                .entry(mapping.upstream_model.to_lowercase())
                .or_default()
                .push(mapping.clone());
        }
        aliases.insert(entry.alias.to_lowercase(), entry);
    }
    Registry {
        aliases,
        raw_exact,
        raw_folded,
    }
}

fn insert_mapping(registry: &mut Registry, alias: &str, mapping: ProviderMapping) {
    let key = alias.to_ascii_lowercase();
    let entry = registry.aliases.entry(key).or_insert_with(|| AliasEntry {
        alias: alias.to_string(),
        mappings: Vec::new(),
    });
    if !entry.mappings.contains(&mapping) {
        entry.mappings.push(mapping.clone());
    }
    for (index, raw_key) in [
        (&mut registry.raw_exact, mapping.upstream_model.clone()),
        (
            &mut registry.raw_folded,
            mapping.upstream_model.to_lowercase(),
        ),
    ] {
        let mappings = index.entry(raw_key).or_default();
        if !mappings.contains(&mapping) {
            mappings.push(mapping.clone());
        }
    }
}

/// Slash, underscore, or whitespace means "treat as a raw ID": never fold those
/// characters into `-` and then hit a kebab alias (`glm/5.2` ≠ `glm-5.2`).
fn looks_raw_shaped(name: &str) -> bool {
    name.chars()
        .any(|ch| ch == '/' || ch == '_' || ch.is_whitespace())
}

fn pin_or_ambiguous(
    requested: String,
    mappings: &[ProviderMapping],
) -> Result<ResolvedModel, ResolveError> {
    match mappings {
        [mapping] => Ok(ResolvedModel::PinnedRaw {
            requested,
            mapping: mapping.clone(),
        }),
        [] => Err(ResolveError::Unknown { requested }),
        _ => Err(ResolveError::Ambiguous {
            requested,
            mappings: mappings.to_vec(),
        }),
    }
}

/// Resolve a client-supplied model name against the builtin registry.
pub fn resolve(requested: &str) -> Result<ResolvedModel, ResolveError> {
    resolve_in(registry(), requested)
}

/// Resolve against the builtin registry, then overlay eligible Custom
/// capability IDs. Published aliases keep their Go/Zen mappings and gain
/// compatible Custom candidates. Distinct provider raw-ID conflicts stay
/// [`ResolveError::Ambiguous`]. Unknown names resolve from Custom only.
pub fn resolve_with_custom(
    requested: &str,
    custom_model_ids: &[String],
) -> Result<ResolvedModel, ResolveError> {
    let custom_hit = custom_model_ids
        .iter()
        .any(|id| custom_model_id_matches(id, requested));
    match resolve(requested) {
        Ok(ResolvedModel::Alias {
            requested,
            alias,
            mut mappings,
        }) => {
            if custom_hit && !mappings.iter().any(|mapping| mapping.is_custom_api()) {
                mappings.push(custom_mapping(&alias));
            }
            Ok(ResolvedModel::Alias {
                requested,
                alias,
                mappings,
            })
        }
        Ok(ResolvedModel::PinnedRaw { requested, mapping }) => {
            if custom_hit && !mapping.is_custom_api() {
                return Err(ResolveError::Ambiguous {
                    requested,
                    mappings: vec![mapping, custom_mapping(CUSTOM_DYNAMIC_UPSTREAM)],
                });
            }
            Ok(ResolvedModel::PinnedRaw { requested, mapping })
        }
        Err(ResolveError::Unknown { requested }) if custom_hit => Ok(ResolvedModel::PinnedRaw {
            requested,
            mapping: custom_mapping(CUSTOM_DYNAMIC_UPSTREAM),
        }),
        other => other,
    }
}

/// Resolve against the current persisted Zen Free catalog and eligible Custom
/// capabilities. Zen mappings are rebuilt from the small bounded snapshot so a
/// successful manual refresh takes effect without restarting the Gateway.
pub fn resolve_with_provider_models(
    requested: &str,
    zen_free_models: &[String],
    custom_model_ids: &[String],
) -> Result<ResolvedModel, ResolveError> {
    let registry = build_registry(zen_free_models);
    resolve_with_custom_in(&registry, requested, custom_model_ids)
}

fn resolve_with_custom_in(
    registry: &Registry,
    requested: &str,
    custom_model_ids: &[String],
) -> Result<ResolvedModel, ResolveError> {
    let custom_hit = custom_model_ids
        .iter()
        .any(|id| custom_model_id_matches(id, requested));
    match resolve_in(registry, requested) {
        Ok(ResolvedModel::Alias {
            requested,
            alias,
            mut mappings,
        }) => {
            if custom_hit && !mappings.iter().any(|mapping| mapping.is_custom_api()) {
                mappings.push(custom_mapping(&alias));
            }
            Ok(ResolvedModel::Alias {
                requested,
                alias,
                mappings,
            })
        }
        Ok(ResolvedModel::PinnedRaw { requested, mapping }) => {
            if custom_hit && !mapping.is_custom_api() {
                return Err(ResolveError::Ambiguous {
                    requested,
                    mappings: vec![mapping, custom_mapping(CUSTOM_DYNAMIC_UPSTREAM)],
                });
            }
            Ok(ResolvedModel::PinnedRaw { requested, mapping })
        }
        Err(ResolveError::Unknown { requested }) if custom_hit => Ok(ResolvedModel::PinnedRaw {
            requested,
            mapping: custom_mapping(CUSTOM_DYNAMIC_UPSTREAM),
        }),
        other => other,
    }
}

fn resolve_in(registry: &Registry, requested: &str) -> Result<ResolvedModel, ResolveError> {
    let original = requested.to_string();
    let trimmed = requested.trim();
    if trimmed.is_empty() {
        return Err(ResolveError::Unknown {
            requested: original,
        });
    }

    // Raw-looking IDs are resolved against exact (then case-folded exact) raw
    // keys before any alias spelling. Separator folding is never applied.
    if looks_raw_shaped(trimmed) {
        if let Some(mappings) = registry.raw_exact.get(trimmed) {
            return pin_or_ambiguous(original, mappings);
        }
        if let Some(mappings) = registry.raw_folded.get(&trimmed.to_lowercase()) {
            return pin_or_ambiguous(original, mappings);
        }
        return Err(ResolveError::Unknown {
            requested: original,
        });
    }

    let folded = trimmed.to_lowercase();
    if let Some(entry) = registry.aliases.get(&folded) {
        return Ok(ResolvedModel::Alias {
            requested: original,
            alias: entry.alias.clone(),
            mappings: entry.mappings.clone(),
        });
    }
    if let Some(mappings) = registry.raw_exact.get(trimmed) {
        return pin_or_ambiguous(original, mappings);
    }
    if let Some(mappings) = registry.raw_folded.get(&folded) {
        return pin_or_ambiguous(original, mappings);
    }
    Err(ResolveError::Unknown {
        requested: original,
    })
}

/// Preferred aliases present in the registry, including fail-closed-only names.
/// Client `GET /v1/models` uses [`published_routeable_aliases`] instead.
pub fn published_aliases() -> Vec<String> {
    registry()
        .aliases
        .values()
        .map(|entry| entry.alias.clone())
        .collect()
}

/// A routeable preferred alias advertised by `GET /v1/models`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublishedAlias {
    pub alias: String,
    pub owned_by: &'static str,
}

/// Routeable preferred aliases that `GET /v1/models` exposes, in deterministic
/// registry order. `owned_by` is the first routeable mapping's `provider_id`.
/// Non-routeable GOAT / SCNet / Custom mappings stay unpublished.
///
/// First-wins `owned_by` is only the client list advertisement. Catalog and
/// application-model discovery use [`routeable_aliases_for`], which keeps an
/// alias under every offering that currently has a routeable mapping.
pub fn published_routeable_aliases() -> Vec<PublishedAlias> {
    published_routeable_in(registry())
}

pub fn published_routeable_aliases_with_zen(zen_free_models: &[String]) -> Vec<PublishedAlias> {
    published_routeable_in(&build_registry(zen_free_models))
}

fn published_routeable_in(registry: &Registry) -> Vec<PublishedAlias> {
    registry
        .aliases
        .values()
        .filter_map(|entry| {
            entry
                .mappings
                .iter()
                .find(|mapping| mapping.routeable)
                .map(|mapping| PublishedAlias {
                    alias: entry.alias.clone(),
                    owned_by: mapping.provider_id,
                })
        })
        .collect()
}

/// Preferred aliases that currently have a routeable mapping for this
/// provider/offering, in deterministic registry order. Raw upstream IDs are
/// never returned. Unroutable mappings (GOAT / SCNet / Custom today) yield an
/// empty list without a hardcoded per-plan alias set.
pub fn routeable_aliases_for(provider_id: &str, offering_id: &str) -> Vec<String> {
    routeable_aliases_for_in(registry(), provider_id, offering_id)
}

fn routeable_aliases_for_in(
    registry: &Registry,
    provider_id: &str,
    offering_id: &str,
) -> Vec<String> {
    registry
        .aliases
        .values()
        .filter(|entry| {
            entry.mappings.iter().any(|mapping| {
                mapping.routeable
                    && mapping.provider_id == provider_id
                    && mapping.offering_id == offering_id
            })
        })
        .map(|entry| entry.alias.clone())
        .collect()
}

pub fn routeable_aliases_for_with_zen(
    provider_id: &str,
    offering_id: &str,
    zen_free_models: &[String],
) -> Vec<String> {
    routeable_aliases_for_in(&build_registry(zen_free_models), provider_id, offering_id)
}

pub fn is_published_alias(name: &str) -> bool {
    matches!(resolve(name), Ok(ResolvedModel::Alias { .. }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gateway::free_models::free_model_ids;
    use crate::kernel::ids::{
        CUSTOM_API_OFFERING_ID, CUSTOM_PROVIDER_ID, SCNET_PROVIDER_ID,
        SCNET_TOKEN_PLAN_OFFERING_IDS,
    };

    #[test]
    fn go_model_ids_are_preferred_aliases() {
        let resolved = resolve("glm-5.2").expect("known Go model");
        match resolved {
            ResolvedModel::Alias {
                alias, mappings, ..
            } => {
                assert_eq!(alias, "glm-5.2");
                assert_eq!(mappings.len(), 1);
                assert!(mappings[0].is_opencode_go());
                assert!(mappings[0].routeable);
                assert_eq!(mappings[0].upstream_model, "glm-5.2");
            }
            other => panic!("expected alias, got {other:?}"),
        }
    }

    #[test]
    fn alias_lookup_is_case_insensitive_kebab() {
        let resolved = resolve("GLM-5.2").expect("case-folded alias");
        assert!(matches!(resolved, ResolvedModel::Alias { alias, .. } if alias == "glm-5.2"));
        assert!(is_published_alias("Grok-4.5"));
        assert!(is_published_alias(" glm-5.2 "));
        for alias in published_aliases() {
            assert_eq!(alias, alias.to_lowercase());
            assert!(!alias.is_empty());
            assert!(!looks_raw_shaped(&alias));
        }
    }

    #[test]
    fn raw_looking_names_do_not_collapse_onto_kebab_aliases() {
        for name in ["glm/5.2", "GLM_5.2", "Grok 4.5", "glm 5.2"] {
            match resolve(name) {
                Err(ResolveError::Unknown { requested }) => assert_eq!(requested, name),
                other => panic!("`{name}` must not collapse onto a kebab alias, got {other:?}"),
            }
            assert!(!is_published_alias(name));
        }
        assert!(matches!(
            resolve("glm-5.2").unwrap(),
            ResolvedModel::Alias { alias, .. } if alias == "glm-5.2"
        ));
    }

    #[test]
    fn discovered_free_ids_and_stripped_aliases_share_the_zen_mapping() {
        let resolved = resolve("deepseek-v4-flash-free").expect("Zen model id");
        match resolved {
            ResolvedModel::Alias {
                alias, mappings, ..
            } => {
                assert_eq!(alias, "deepseek-v4-flash-free");
                assert_eq!(mappings.len(), 1);
                assert!(mappings[0].is_zen_free());
                assert_eq!(mappings[0].upstream_model, "deepseek-v4-flash-free");
            }
            other => panic!("expected alias, got {other:?}"),
        }
    }

    #[test]
    fn shared_aliases_record_go_and_zen_mappings_in_the_registry() {
        match resolve("mimo-v2.5").unwrap() {
            ResolvedModel::Alias { mappings, .. } => {
                assert_eq!(mappings.len(), 2);
                assert!(mappings[0].is_opencode_go());
                assert!(mappings[1].is_zen_free());
                assert_eq!(mappings[1].upstream_model, "mimo-v2.5-free");
            }
            other => panic!("expected alias, got {other:?}"),
        }
        match resolve("glm-5.2").unwrap() {
            ResolvedModel::Alias { mappings, .. } => assert_eq!(mappings.len(), 1),
            other => panic!("expected alias, got {other:?}"),
        }
    }

    #[test]
    fn refreshed_zen_models_publish_raw_and_stripped_aliases() {
        let models = vec!["brand-new-coder-free".to_string()];
        for requested in ["brand-new-coder-free", "brand-new-coder"] {
            match resolve_with_provider_models(requested, &models, &[]).unwrap() {
                ResolvedModel::Alias {
                    alias, mappings, ..
                } => {
                    assert_eq!(alias, requested);
                    assert_eq!(mappings.len(), 1);
                    assert!(mappings[0].is_zen_free());
                    assert_eq!(mappings[0].upstream_model, "brand-new-coder-free");
                }
                other => panic!("expected dynamic Zen alias, got {other:?}"),
            }
        }
        let published = published_routeable_aliases_with_zen(&models);
        assert!(
            published
                .iter()
                .any(|entry| entry.alias == "brand-new-coder")
        );
        assert!(
            published
                .iter()
                .any(|entry| entry.alias == "brand-new-coder-free")
        );
    }

    #[test]
    fn refreshed_catalog_cannot_steal_go_only_ox_alpha_free() {
        let models = vec!["ox-alpha-free".to_string()];
        match resolve_with_provider_models("ox-alpha-free", &models, &[]).unwrap() {
            ResolvedModel::Alias { mappings, .. } => {
                assert!(mappings.iter().all(ProviderMapping::is_opencode_go));
            }
            other => panic!("expected Go alias, got {other:?}"),
        }
        assert!(resolve_with_provider_models("ox-alpha", &models, &[]).is_err());
    }

    #[test]
    fn registry_covers_every_opencode_protocol_id() {
        let aliases = published_aliases();
        for id in supported_model_ids() {
            if id == "big-pickle" || (is_free_model(id) && !free_model_ids().any(|free| free == id))
            {
                continue;
            }
            assert!(
                aliases.iter().any(|alias| alias == id),
                "MODEL_PROTOCOLS id `{id}` must have an alias"
            );
        }
        for id in free_model_ids() {
            assert!(
                aliases.iter().any(|alias| alias == id),
                "free model `{id}` must have an alias"
            );
            assert!(resolve(id).unwrap().routeable_mappings()[0].is_zen_free());
        }
        assert!(!aliases.iter().any(|alias| alias.contains("goat")));
        assert!(!aliases.iter().any(|alias| alias.contains("scnet")));
        assert!(!aliases.iter().any(|alias| alias.contains("custom")));
    }

    #[test]
    fn published_routeable_aliases_use_routeable_provider_ownership() {
        let published = published_routeable_aliases();
        assert!(!published.is_empty());
        let ids: Vec<&str> = published.iter().map(|item| item.alias.as_str()).collect();
        let mut sorted = ids.clone();
        sorted.sort_unstable();
        assert_eq!(ids, sorted, "GET /v1/models order must be deterministic");
        assert_eq!(
            published.len(),
            published_aliases().len(),
            "builtin aliases currently all have a routeable mapping"
        );
        for item in &published {
            assert!(!looks_raw_shaped(&item.alias));
            assert_ne!(item.alias, COMMAND_CODE_GOAT_DEEPSEEK_V4_FLASH_UPSTREAM);
            match resolve(&item.alias).unwrap() {
                ResolvedModel::Alias { mappings, .. } => {
                    let routeable = mappings
                        .iter()
                        .find(|mapping| mapping.routeable)
                        .expect("published alias must have a routeable mapping");
                    assert_eq!(item.owned_by, routeable.provider_id);
                    assert_ne!(item.owned_by, COMMAND_CODE_PROVIDER_ID);
                    assert_ne!(item.owned_by, crate::provider::SCNET_PROVIDER_ID);
                    assert_ne!(item.owned_by, crate::provider::CUSTOM_PROVIDER_ID);
                }
                other => panic!("published id must be an alias, got {other:?}"),
            }
        }
        let go = published
            .iter()
            .find(|item| item.alias == "glm-5.2")
            .expect("Go alias");
        assert_eq!(go.owned_by, OPENCODE_PROVIDER_ID);
        let zen_free = published
            .iter()
            .find(|item| item.alias == "deepseek-v4-flash-free")
            .expect("discovered Zen model id");
        assert_eq!(zen_free.owned_by, OPENCODE_ZEN_FREE_PROVIDER_ID);
        let goat_alias = published
            .iter()
            .find(|item| item.alias == COMMAND_CODE_GOAT_DEEPSEEK_V4_FLASH_ALIAS)
            .expect("Go still owns the kebab alias");
        assert_eq!(goat_alias.owned_by, OPENCODE_PROVIDER_ID);
        assert!(!published.iter().any(|item| item.alias.contains('/')));
    }

    #[test]
    fn slash_prefixed_goat_raw_pins_to_command_code_and_does_not_steal_go() {
        match resolve(COMMAND_CODE_GOAT_DEEPSEEK_V4_FLASH_UPSTREAM) {
            Ok(ResolvedModel::PinnedRaw { mapping, .. }) => {
                assert!(mapping.is_command_code_goat());
                assert!(!mapping.routeable);
                assert_eq!(
                    mapping.upstream_model,
                    COMMAND_CODE_GOAT_DEEPSEEK_V4_FLASH_UPSTREAM
                );
                assert!(
                    ResolvedModel::PinnedRaw {
                        requested: COMMAND_CODE_GOAT_DEEPSEEK_V4_FLASH_UPSTREAM.into(),
                        mapping: mapping.clone(),
                    }
                    .routeable_mappings()
                    .is_empty()
                );
            }
            other => panic!("GOAT raw id must uniquely pin to command-code/goat, got {other:?}"),
        }
        match resolve(COMMAND_CODE_GOAT_DEEPSEEK_V4_FLASH_ALIAS).unwrap() {
            ResolvedModel::Alias {
                alias, mappings, ..
            } => {
                assert_eq!(alias, COMMAND_CODE_GOAT_DEEPSEEK_V4_FLASH_ALIAS);
                assert!(mappings.iter().any(|mapping| mapping.is_opencode_go()));
                assert!(
                    mappings
                        .iter()
                        .any(|mapping| mapping.is_command_code_goat() && !mapping.routeable)
                );
                let routeable = mappings
                    .iter()
                    .filter(|mapping| mapping.routeable)
                    .collect::<Vec<_>>();
                assert_eq!(routeable.len(), 2);
                assert!(routeable.iter().any(|mapping| mapping.is_opencode_go()));
                assert!(routeable.iter().any(|mapping| mapping.is_zen_free()));
                assert_eq!(
                    routeable
                        .iter()
                        .find(|mapping| mapping.is_opencode_go())
                        .unwrap()
                        .upstream_model,
                    COMMAND_CODE_GOAT_DEEPSEEK_V4_FLASH_ALIAS
                );
            }
            other => panic!("expected published Go alias, got {other:?}"),
        }
        assert!(is_published_alias(
            COMMAND_CODE_GOAT_DEEPSEEK_V4_FLASH_ALIAS
        ));
        assert!(!is_published_alias(
            COMMAND_CODE_GOAT_DEEPSEEK_V4_FLASH_UPSTREAM
        ));
        assert!(
            !published_aliases().iter().any(|alias| alias.contains('/')
                || *alias == COMMAND_CODE_GOAT_DEEPSEEK_V4_FLASH_UPSTREAM)
        );
    }

    #[test]
    fn unknown_names_are_not_aliases() {
        match resolve("definitely-not-a-model") {
            Err(ResolveError::Unknown { requested }) => {
                assert_eq!(requested, "definitely-not-a-model");
                assert!(
                    ResolveError::Unknown {
                        requested: requested.clone()
                    }
                    .code()
                    .is_none()
                );
            }
            other => panic!("expected unknown, got {other:?}"),
        }
    }

    #[test]
    fn unique_raw_id_pins_to_one_mapping() {
        let registry = registry_from_entries(vec![
            AliasEntry {
                alias: "widget".into(),
                mappings: vec![go_mapping("widget")],
            },
            AliasEntry {
                alias: "gadget".into(),
                mappings: vec![ProviderMapping {
                    provider_id: OPENCODE_PROVIDER_ID,
                    offering_id: GO_OFFERING_ID,
                    upstream_model: "vendor.gadget-v1".into(),
                    routeable: true,
                }],
            },
        ]);
        match resolve_in(&registry, "vendor.gadget-v1").unwrap() {
            ResolvedModel::PinnedRaw { mapping, .. } => {
                assert_eq!(mapping.upstream_model, "vendor.gadget-v1");
                assert!(mapping.is_opencode_go());
            }
            other => panic!("expected pinned raw, got {other:?}"),
        }
        // Alias still wins when a kebab string is both an alias and a raw ID.
        assert!(matches!(
            resolve_in(&registry, "widget").unwrap(),
            ResolvedModel::Alias { alias, .. } if alias == "widget"
        ));
        // Exact slash-form raw IDs pin without collapsing onto a kebab alias.
        let slash_registry = registry_from_entries(vec![AliasEntry {
            alias: "widget".into(),
            mappings: vec![ProviderMapping {
                provider_id: OPENCODE_PROVIDER_ID,
                offering_id: GO_OFFERING_ID,
                upstream_model: "vendor/widget-v1".into(),
                routeable: true,
            }],
        }]);
        match resolve_in(&slash_registry, "vendor/widget-v1").unwrap() {
            ResolvedModel::PinnedRaw { mapping, .. } => {
                assert_eq!(mapping.upstream_model, "vendor/widget-v1");
            }
            other => panic!("exact slash raw must pin, got {other:?}"),
        }
        assert!(matches!(
            resolve_in(&slash_registry, "vendor-widget-v1"),
            Err(ResolveError::Unknown { .. })
        ));
    }

    #[test]
    fn overlapping_raw_ids_return_ambiguous_model_id() {
        let registry = registry_from_entries(vec![
            AliasEntry {
                alias: "alpha".into(),
                mappings: vec![go_mapping("shared-raw")],
            },
            AliasEntry {
                alias: "beta".into(),
                mappings: vec![zen_mapping("shared-raw")],
            },
        ]);
        match resolve_in(&registry, "shared-raw") {
            Err(error) => {
                assert_eq!(error.code(), Some(AMBIGUOUS_MODEL_ID));
                let message = error.message();
                assert!(message.contains(AMBIGUOUS_MODEL_ID));
                assert!(message.contains("shared-raw"));
                assert!(message.contains("opencode/go"));
                assert!(message.contains("opencode-zen-free"));
            }
            other => panic!("expected ambiguous, got {other:?}"),
        }
        // Preferred aliases still resolve even when their upstream IDs overlap.
        assert!(matches!(
            resolve_in(&registry, "alpha").unwrap(),
            ResolvedModel::Alias { alias, .. } if alias == "alpha"
        ));
    }

    #[test]
    fn fail_closed_raw_mapping_is_not_routeable() {
        let registry = registry_from_entries(vec![AliasEntry {
            alias: "visible".into(),
            mappings: vec![ProviderMapping {
                provider_id: "command-code",
                offering_id: "goat",
                upstream_model: "goat-only-raw".into(),
                routeable: false,
            }],
        }]);
        match resolve_in(&registry, "goat-only-raw") {
            Ok(ResolvedModel::PinnedRaw { mapping, .. }) => {
                assert!(!mapping.routeable);
                assert_eq!(mapping.provider_id, "command-code");
                assert!(
                    ResolvedModel::PinnedRaw {
                        requested: "goat-only-raw".into(),
                        mapping: mapping.clone(),
                    }
                    .routeable_mappings()
                    .is_empty()
                );
            }
            other => {
                panic!("fail-closed unique raw must pin without being routeable, got {other:?}")
            }
        }
        match resolve_in(&registry, "visible").unwrap() {
            ResolvedModel::Alias { mappings, .. } => {
                assert!(!mappings[0].routeable);
                assert!(
                    ResolvedModel::Alias {
                        requested: "visible".into(),
                        alias: "visible".into(),
                        mappings: mappings.clone(),
                    }
                    .routeable_mappings()
                    .is_empty()
                );
            }
            other => panic!("expected alias, got {other:?}"),
        }
        assert!(
            published_routeable_in(&registry).is_empty(),
            "fail-closed aliases must stay off GET /v1/models"
        );
    }

    #[test]
    fn catalog_aliases_are_routeable_mappings_in_registry_order() {
        let go = routeable_aliases_for(OPENCODE_PROVIDER_ID, GO_OFFERING_ID);
        let zen = routeable_aliases_for(OPENCODE_ZEN_FREE_PROVIDER_ID, ANONYMOUS_FREE_OFFERING_ID);
        assert!(!go.is_empty());
        assert!(!zen.is_empty());
        let mut sorted_go = go.clone();
        sorted_go.sort_unstable();
        assert_eq!(go, sorted_go, "catalog aliases must be deterministic");
        let mut sorted_zen = zen.clone();
        sorted_zen.sort_unstable();
        assert_eq!(zen, sorted_zen);

        for alias in go.iter().chain(zen.iter()) {
            assert!(!looks_raw_shaped(alias));
            assert_ne!(*alias, COMMAND_CODE_GOAT_DEEPSEEK_V4_FLASH_UPSTREAM);
            assert!(!alias.contains('/'));
        }
        assert!(go.iter().any(|alias| alias == "glm-5.2"));
        assert!(
            go.iter()
                .any(|alias| alias == COMMAND_CODE_GOAT_DEEPSEEK_V4_FLASH_ALIAS)
        );
        assert!(go.iter().any(|alias| alias == "minimax-m2.7-highspeed"));
        assert!(!go.iter().any(|alias| alias == "deepseek-v4-flash-free"));
        assert!(!zen.iter().any(|alias| alias == "glm-5.2"));
        assert!(zen.iter().any(|alias| alias == "deepseek-v4-flash-free"));
        for id in free_model_ids() {
            assert!(
                zen.iter().any(|alias| alias == id),
                "Zen catalog must include `{id}`"
            );
            assert!(
                !go.iter().any(|alias| alias == id),
                "Go catalog must not include free `{id}`"
            );
        }
        for id in supported_model_ids().filter(|id| !is_free_model(id)) {
            if id != "big-pickle" {
                assert!(
                    go.iter().any(|alias| alias == id),
                    "Go catalog must include `{id}`"
                );
            }
            let has_free_twin = free_model_ids().any(|free| {
                crate::kernel::zen::stripped_free_alias(free).is_some_and(|alias| alias == id)
            });
            assert_eq!(
                zen.iter().any(|alias| alias == id),
                has_free_twin,
                "Zen stripped aliases must match the refreshed Free catalog for `{id}`"
            );
        }

        assert!(routeable_aliases_for(COMMAND_CODE_PROVIDER_ID, GOAT_OFFERING_ID).is_empty());
        assert!(
            routeable_aliases_for(CUSTOM_PROVIDER_ID, CUSTOM_API_OFFERING_ID).is_empty(),
            "Custom catalog aliases stay empty; client IDs come from account capabilities"
        );
        for offering_id in SCNET_TOKEN_PLAN_OFFERING_IDS {
            assert!(
                routeable_aliases_for(SCNET_PROVIDER_ID, offering_id).is_empty(),
                "unroutable scnet/{offering_id} must not publish aliases"
            );
        }
    }

    #[test]
    fn catalog_aliases_keep_every_routeable_offering_not_first_wins_owner() {
        let registry = registry_from_entries(vec![AliasEntry {
            alias: "shared".into(),
            mappings: vec![zen_mapping("shared"), go_mapping("shared")],
        }]);
        let published = published_routeable_in(&registry);
        assert_eq!(published.len(), 1);
        assert_eq!(published[0].alias, "shared");
        assert_eq!(
            published[0].owned_by, OPENCODE_ZEN_FREE_PROVIDER_ID,
            "GET /v1/models owned_by stays first-wins"
        );
        assert_eq!(
            routeable_aliases_for_in(&registry, OPENCODE_PROVIDER_ID, GO_OFFERING_ID),
            ["shared"]
        );
        assert_eq!(
            routeable_aliases_for_in(
                &registry,
                OPENCODE_ZEN_FREE_PROVIDER_ID,
                ANONYMOUS_FREE_OFFERING_ID
            ),
            ["shared"]
        );
        assert!(
            routeable_aliases_for_in(&registry, COMMAND_CODE_PROVIDER_ID, GOAT_OFFERING_ID)
                .is_empty()
        );
    }

    #[test]
    fn custom_overlay_does_not_steal_published_aliases_and_resolves_unknown_ids() {
        match resolve_with_custom("glm-5.2", &["glm-5.2".into()]).unwrap() {
            ResolvedModel::Alias {
                alias, mappings, ..
            } => {
                assert_eq!(alias, "glm-5.2");
                assert!(mappings.iter().any(|mapping| mapping.is_opencode_go()));
                assert!(mappings.iter().any(|mapping| mapping.is_custom_api()));
                let routeable = mappings
                    .iter()
                    .filter(|mapping| mapping.routeable)
                    .collect::<Vec<_>>();
                assert!(routeable.iter().any(|mapping| mapping.is_opencode_go()));
                assert!(routeable.iter().any(|mapping| mapping.is_custom_api()));
            }
            other => panic!("expected alias overlay, got {other:?}"),
        }
        match resolve_with_custom("my-local-model", &["my-local-model".into()]).unwrap() {
            ResolvedModel::PinnedRaw { mapping, .. } => {
                assert!(mapping.is_custom_api());
                assert!(mapping.routeable);
            }
            other => panic!("expected custom-only pin, got {other:?}"),
        }
        match resolve_with_custom(
            COMMAND_CODE_GOAT_DEEPSEEK_V4_FLASH_UPSTREAM,
            &[COMMAND_CODE_GOAT_DEEPSEEK_V4_FLASH_UPSTREAM.into()],
        ) {
            Err(error) => {
                assert_eq!(error.code(), Some(AMBIGUOUS_MODEL_ID));
                assert!(error.message().contains("command-code/goat"));
                assert!(error.message().contains("custom/api"));
            }
            other => panic!("GOAT raw overlapping Custom must be ambiguous, got {other:?}"),
        }
        assert!(matches!(
            resolve_with_custom("definitely-not-a-model", &[]),
            Err(ResolveError::Unknown { .. })
        ));
    }
}
