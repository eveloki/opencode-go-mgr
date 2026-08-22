//! Effective provider/custom-endpoint contracts: merge, selection, and views.
//!
//! Persistence lives in [`crate::db`]. This module is the only merge/selection
//! seam: dashboard, materialize, and `/v1/models` read an immutable snapshot
//! captured at request entry. Request paths never discover or probe.

use crate::alias::ProviderMapping;
use crate::custom::CustomAccountRuntime;
use crate::kernel::ids::{
    COMMAND_CODE_PROVIDER_ID, CUSTOM_API_OFFERING_ID, CUSTOM_PROVIDER_ID, OPENCODE_PROVIDER_ID,
    OPENCODE_ZEN_FREE_PROVIDER_ID, SCNET_PROVIDER_ID, normalize_model_name,
};
use crate::kernel::protocol::{
    ApiFormat, command_code_protocol_profiles, is_known_model, supported_model_protocol_profiles,
};
use crate::kernel::zen::ZenFreeModelCatalog;
use crate::models::Account;
use crate::provider::{
    BUILTIN_PLANS, OPENCODE_CONSTRUCTABLE_PROTOCOLS, ProviderAdapterKind, ProviderRegistry,
    SCNET_TOKEN_PLAN_USABLE_MODELS, StructuralProbeCeiling, UpstreamProtocolKind,
};
use crate::redaction::sanitize_upstream_error_value_with_known_secret;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt;

pub const SCOPE_KIND_PROVIDER: &str = "provider";
pub const SCOPE_KIND_CUSTOM_ENDPOINT: &str = "custom_endpoint";

pub const CATALOG_SOURCE_STATIC: &str = "static";
pub const CATALOG_SOURCE_OFFICIAL_ZEN: &str = "official_zen";
pub const CATALOG_SOURCE_CUSTOM_DISCOVERY: &str = "custom_discovery";
pub const CATALOG_SOURCE_DECLARED: &str = "account_declared";

pub const NO_ENABLED_UPSTREAM_PROTOCOL: &str =
    "no enabled upstream protocol is available for this model";

const MAX_PROBE_ERROR_CHARS: usize = 500;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContractScopeKind {
    Provider,
    CustomEndpoint,
}

impl ContractScopeKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Provider => SCOPE_KIND_PROVIDER,
            Self::CustomEndpoint => SCOPE_KIND_CUSTOM_ENDPOINT,
        }
    }
}

impl TryFrom<&str> for ContractScopeKind {
    type Error = String;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            SCOPE_KIND_PROVIDER => Ok(Self::Provider),
            SCOPE_KIND_CUSTOM_ENDPOINT => Ok(Self::CustomEndpoint),
            other => Err(format!("unknown contract scope kind `{other}`")),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ContractScope {
    Provider(String),
    CustomEndpoint(String),
}

impl ContractScope {
    pub fn provider(provider_id: impl Into<String>) -> Self {
        Self::Provider(provider_id.into())
    }

    pub fn custom_endpoint(account_id: impl Into<String>) -> Self {
        Self::CustomEndpoint(account_id.into())
    }

    pub fn kind(&self) -> ContractScopeKind {
        match self {
            Self::Provider(_) => ContractScopeKind::Provider,
            Self::CustomEndpoint(_) => ContractScopeKind::CustomEndpoint,
        }
    }

    pub fn kind_str(&self) -> &'static str {
        self.kind().as_str()
    }

    pub fn id(&self) -> &str {
        match self {
            Self::Provider(id) | Self::CustomEndpoint(id) => id,
        }
    }

    pub fn parse(kind: &str, id: &str) -> Result<Self, String> {
        let id = id.trim();
        if id.is_empty() {
            return Err("contract scope id is required".to_string());
        }
        match ContractScopeKind::try_from(kind)? {
            ContractScopeKind::Provider => Ok(Self::provider(id)),
            ContractScopeKind::CustomEndpoint => Ok(Self::custom_endpoint(id)),
        }
    }

    pub fn from_account(account: &crate::models::Account) -> Option<Self> {
        Self::from_offering(
            &account.provider_id,
            &account.offering_id,
            Some(&account.id),
        )
    }

    pub fn from_mapping(mapping: &ProviderMapping) -> Option<Self> {
        Self::from_offering(mapping.provider_id, mapping.offering_id, None)
    }

    pub fn from_offering(
        provider_id: &str,
        offering_id: &str,
        account_id: Option<&str>,
    ) -> Option<Self> {
        match ProviderAdapterKind::from_offering(provider_id, offering_id)? {
            ProviderAdapterKind::ConfigurableHttp => account_id
                .map(str::trim)
                .filter(|id| !id.is_empty())
                .map(Self::custom_endpoint),
            kind => kind.provider_scope_id().map(Self::provider),
        }
    }
}

pub fn builtin_provider_scope_ids() -> [&'static str; 4] {
    [
        OPENCODE_PROVIDER_ID,
        OPENCODE_ZEN_FREE_PROVIDER_ID,
        COMMAND_CODE_PROVIDER_ID,
        SCNET_PROVIDER_ID,
    ]
}

pub fn adapter_kind_for_provider_scope(provider_id: &str) -> Option<ProviderAdapterKind> {
    match provider_id {
        OPENCODE_PROVIDER_ID => Some(ProviderAdapterKind::OpenCodeGo),
        OPENCODE_ZEN_FREE_PROVIDER_ID => Some(ProviderAdapterKind::ZenFree),
        COMMAND_CODE_PROVIDER_ID => Some(ProviderAdapterKind::CommandCodeGoat),
        SCNET_PROVIDER_ID => Some(ProviderAdapterKind::Scnet),
        CUSTOM_PROVIDER_ID => Some(ProviderAdapterKind::ConfigurableHttp),
        _ => None,
    }
}

pub fn parse_upstream_protocol(value: &str) -> Result<UpstreamProtocolKind, String> {
    UpstreamProtocolKind::try_from(value).map_err(|_| {
        format!(
            "unknown upstream protocol `{value}`; expected chat_completions, responses, or messages"
        )
    })
}

pub fn protocol_from_api(format: ApiFormat) -> Option<UpstreamProtocolKind> {
    match format {
        ApiFormat::ChatCompletions => Some(UpstreamProtocolKind::ChatCompletions),
        ApiFormat::Responses => Some(UpstreamProtocolKind::Responses),
        ApiFormat::Messages => Some(UpstreamProtocolKind::Messages),
        ApiFormat::Gemini => None,
    }
}

pub fn protocol_to_api(protocol: UpstreamProtocolKind) -> ApiFormat {
    match protocol {
        UpstreamProtocolKind::ChatCompletions => ApiFormat::ChatCompletions,
        UpstreamProtocolKind::Responses => ApiFormat::Responses,
        UpstreamProtocolKind::Messages => ApiFormat::Messages,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContractEvidenceSource {
    Static,
    Preset,
    ProbeConfirmed,
    ProbeObserved,
}

impl ContractEvidenceSource {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Static => "static",
            Self::Preset => "preset",
            Self::ProbeConfirmed => "probe_confirmed",
            Self::ProbeObserved => "probe_observed",
        }
    }

    pub const fn confers_support(self) -> bool {
        matches!(self, Self::Static | Self::Preset | Self::ProbeConfirmed)
    }
}

impl TryFrom<&str> for ContractEvidenceSource {
    type Error = String;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "static" => Ok(Self::Static),
            "preset" => Ok(Self::Preset),
            "probe_confirmed" => Ok(Self::ProbeConfirmed),
            "probe_observed" => Ok(Self::ProbeObserved),
            other => Err(format!("unknown contract evidence source `{other}`")),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProbeResultKind {
    Success,
    Failure,
}

impl ProbeResultKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Success => "success",
            Self::Failure => "failure",
        }
    }
}

impl TryFrom<&str> for ProbeResultKind {
    type Error = String;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "success" => Ok(Self::Success),
            "failure" => Ok(Self::Failure),
            other => Err(format!("unknown probe result `{other}`")),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProtocolSwitches {
    pub chat_completions: bool,
    pub responses: bool,
    pub messages: bool,
}

impl Default for ProtocolSwitches {
    fn default() -> Self {
        Self {
            chat_completions: true,
            responses: true,
            messages: true,
        }
    }
}

impl ProtocolSwitches {
    pub fn is_enabled(self, protocol: UpstreamProtocolKind) -> bool {
        match protocol {
            UpstreamProtocolKind::ChatCompletions => self.chat_completions,
            UpstreamProtocolKind::Responses => self.responses,
            UpstreamProtocolKind::Messages => self.messages,
        }
    }

    pub fn set(&mut self, protocol: UpstreamProtocolKind, enabled: bool) {
        match protocol {
            UpstreamProtocolKind::ChatCompletions => self.chat_completions = enabled,
            UpstreamProtocolKind::Responses => self.responses = enabled,
            UpstreamProtocolKind::Messages => self.messages = enabled,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PersistedScopeRow {
    pub scope: ContractScope,
    pub catalog_models: Vec<String>,
    pub catalog_refreshed_at: Option<DateTime<Utc>>,
    pub catalog_source: String,
    pub catalog_source_url: String,
    pub switches: ProtocolSwitches,
    pub revision: u64,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PersistedModelProtocol {
    pub scope: ContractScope,
    pub model_id: String,
    pub protocol: UpstreamProtocolKind,
    pub source: ContractEvidenceSource,
    pub verified_at: Option<DateTime<Utc>>,
    pub observed_at: Option<DateTime<Utc>>,
    pub last_probe_result: Option<ProbeResultKind>,
    pub last_probe_at: Option<DateTime<Utc>>,
    pub last_probe_error: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PersistedContracts {
    pub scopes: HashMap<ContractScope, PersistedScopeRow>,
    pub evidence: HashMap<ContractScope, Vec<PersistedModelProtocol>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct EffectiveCatalog {
    pub source: String,
    pub source_url: String,
    pub refreshed_at: Option<DateTime<Utc>>,
    pub models: Vec<String>,
    pub refresh_supported: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct EffectiveProtocolEvidence {
    pub protocol: UpstreamProtocolKind,
    pub available: bool,
    pub enabled: bool,
    pub source: ContractEvidenceSource,
    pub verified_at: Option<DateTime<Utc>>,
    pub observed_at: Option<DateTime<Utc>>,
    pub last_probe_result: Option<ProbeResultKind>,
    pub last_probe_at: Option<DateTime<Utc>>,
    pub last_probe_error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct EffectiveModelContract {
    pub model_id: String,
    pub preferred_protocol: UpstreamProtocolKind,
    pub protocols: BTreeMap<String, EffectiveProtocolEvidence>,
    pub routable: bool,
    pub disabled_reasons: Vec<String>,
}

impl EffectiveModelContract {
    pub fn enabled_protocols(&self) -> Vec<UpstreamProtocolKind> {
        self.protocols
            .values()
            .filter(|row| row.enabled)
            .map(|row| row.protocol)
            .collect()
    }

    pub fn has_enabled_protocol(&self) -> bool {
        self.protocols.values().any(|row| row.enabled)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EffectiveScopeContract {
    pub scope: ContractScope,
    pub provider_id: String,
    pub adapter_kind: ProviderAdapterKind,
    pub catalog_routable: bool,
    pub production_inference: bool,
    pub switches: ProtocolSwitches,
    pub catalog: EffectiveCatalog,
    pub models: BTreeMap<String, EffectiveModelContract>,
    pub revision: u64,
    pub fallback_priority: &'static [UpstreamProtocolKind],
    pub disabled_reasons: Vec<String>,
}

impl EffectiveScopeContract {
    pub fn model(&self, model_id: &str) -> Option<&EffectiveModelContract> {
        let normalized = normalize_model_name(model_id);
        self.models
            .get(model_id)
            .or_else(|| self.models.get(&normalized))
            .or_else(|| {
                self.models
                    .values()
                    .find(|model| custom_or_case_match(&model.model_id, model_id))
            })
    }

    pub fn model_has_enabled_protocol(&self, model_id: &str) -> bool {
        self.model(model_id)
            .is_some_and(EffectiveModelContract::has_enabled_protocol)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct EffectiveContractSet {
    pub providers: BTreeMap<String, EffectiveScopeContract>,
    pub custom_endpoints: BTreeMap<String, EffectiveScopeContract>,
}

impl EffectiveContractSet {
    pub fn scope(&self, scope: &ContractScope) -> Option<&EffectiveScopeContract> {
        match scope {
            ContractScope::Provider(id) => self.providers.get(id),
            ContractScope::CustomEndpoint(id) => self.custom_endpoints.get(id),
        }
    }

    pub fn mapping_has_enabled_protocol(&self, mapping: &ProviderMapping) -> bool {
        let Some(scope) = ContractScope::from_mapping(mapping) else {
            return false;
        };
        self.scope(&scope)
            .is_some_and(|contract| contract.model_has_enabled_protocol(&mapping.upstream_model))
    }

    pub fn production_protocol_allowed(
        &self,
        account: &Account,
        model_id: &str,
        protocol: UpstreamProtocolKind,
    ) -> bool {
        let Some(scope) = ContractScope::from_account(account) else {
            return false;
        };
        self.scope(&scope)
            .and_then(|contract| contract.model(model_id))
            .and_then(|model| model.protocols.get(protocol.as_str()))
            .is_some_and(|row| row.available && row.enabled)
    }

    pub fn select_for_mapping(
        &self,
        mapping: &ProviderMapping,
        client: ApiFormat,
        model_id: &str,
    ) -> Result<ApiFormat, ProtocolSelectError> {
        let scope = ContractScope::from_mapping(mapping).ok_or_else(|| {
            ProtocolSelectError::new(format!(
                "no contract scope for `{}/{}`",
                mapping.provider_id, mapping.offering_id
            ))
        })?;
        self.select_upstream(&scope, client, model_id)
    }

    pub fn select_upstream(
        &self,
        scope: &ContractScope,
        client: ApiFormat,
        model_id: &str,
    ) -> Result<ApiFormat, ProtocolSelectError> {
        let contract = self.scope(scope).ok_or_else(|| {
            ProtocolSelectError::new(format!(
                "no effective contract for {} `{}`",
                scope.kind_str(),
                scope.id()
            ))
        })?;
        select_upstream_protocol(contract, client, model_id)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProtocolSelectError {
    pub message: String,
}

impl ProtocolSelectError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for ProtocolSelectError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for ProtocolSelectError {}

pub fn select_upstream_protocol(
    contract: &EffectiveScopeContract,
    client: ApiFormat,
    model_id: &str,
) -> Result<ApiFormat, ProtocolSelectError> {
    let model = contract.model(model_id).ok_or_else(|| {
        ProtocolSelectError::new(format!(
            "model `{model_id}` is not in the effective contract"
        ))
    })?;
    let available = model.enabled_protocols();
    if available.is_empty() {
        return Err(ProtocolSelectError::new(NO_ENABLED_UPSTREAM_PROTOCOL));
    }
    let preferred = model.preferred_protocol;
    if available.contains(&preferred) {
        return Ok(match protocol_from_api(client) {
            Some(client_protocol) if available.contains(&client_protocol) => {
                protocol_to_api(client_protocol)
            }
            _ => protocol_to_api(preferred),
        });
    }
    if let Some(client_protocol) = protocol_from_api(client)
        && available.contains(&client_protocol)
    {
        return Ok(protocol_to_api(client_protocol));
    }
    for protocol in contract.fallback_priority {
        if available.contains(protocol) {
            return Ok(protocol_to_api(*protocol));
        }
    }
    Err(ProtocolSelectError::new(NO_ENABLED_UPSTREAM_PROTOCOL))
}

pub fn safety_ceiling_protocols(
    adapter: ProviderAdapterKind,
    model_id: &str,
    declared: &[(String, UpstreamProtocolKind)],
) -> Vec<UpstreamProtocolKind> {
    let Some(descriptor) = representative_descriptor(adapter) else {
        return Vec::new();
    };
    match descriptor.protocol_probe.structural_ceiling {
        StructuralProbeCeiling::Unavailable => Vec::new(),
        StructuralProbeCeiling::OpenCodeConstructable => {
            if is_known_model(model_id) {
                OPENCODE_CONSTRUCTABLE_PROTOCOLS.to_vec()
            } else {
                Vec::new()
            }
        }
        StructuralProbeCeiling::ZenFreeConstructable => {
            if is_known_model(model_id) {
                OPENCODE_CONSTRUCTABLE_PROTOCOLS.to_vec()
            } else if crate::kernel::ids::is_free_model(model_id) {
                vec![UpstreamProtocolKind::ChatCompletions]
            } else {
                Vec::new()
            }
        }
        StructuralProbeCeiling::AccountDeclared => declared
            .iter()
            .filter(|(id, _)| crate::custom::custom_model_id_matches(id, model_id))
            .map(|(_, protocol)| *protocol)
            .collect(),
    }
}

pub fn static_verified_protocols(
    adapter: ProviderAdapterKind,
    model_id: &str,
    declared: &[(String, UpstreamProtocolKind)],
) -> Vec<UpstreamProtocolKind> {
    match adapter {
        ProviderAdapterKind::OpenCodeGo => opencode_supported(model_id).unwrap_or_default(),
        ProviderAdapterKind::ZenFree => {
            if let Some(supported) = opencode_supported(model_id) {
                supported.to_vec()
            } else if crate::kernel::ids::is_free_model(model_id) {
                vec![UpstreamProtocolKind::ChatCompletions]
            } else {
                Vec::new()
            }
        }
        ProviderAdapterKind::CommandCodeGoat => command_code_protocol_profiles()
            .find(|profile| profile.upstream_id.eq_ignore_ascii_case(model_id.trim()))
            .map(|profile| {
                profile
                    .supported_upstream
                    .iter()
                    .copied()
                    .filter_map(protocol_from_api)
                    .collect()
            })
            .unwrap_or_default(),
        ProviderAdapterKind::Scnet => {
            if SCNET_TOKEN_PLAN_USABLE_MODELS
                .iter()
                .any(|id| id.eq_ignore_ascii_case(model_id.trim()))
            {
                vec![
                    UpstreamProtocolKind::ChatCompletions,
                    UpstreamProtocolKind::Messages,
                ]
            } else {
                Vec::new()
            }
        }
        ProviderAdapterKind::ConfigurableHttp => declared
            .iter()
            .filter(|(id, _)| crate::custom::custom_model_id_matches(id, model_id))
            .map(|(_, protocol)| *protocol)
            .collect(),
    }
}

fn opencode_supported(model_id: &str) -> Option<Vec<UpstreamProtocolKind>> {
    opencode_profile(model_id).map(|(_, supported)| {
        supported
            .iter()
            .copied()
            .filter_map(protocol_from_api)
            .collect()
    })
}

fn opencode_profile(model_id: &str) -> Option<(ApiFormat, &'static [ApiFormat])> {
    let normalized = normalize_model_name(model_id);
    supported_model_protocol_profiles()
        .find(|(id, _, _)| *id == normalized)
        .map(|(_, preferred, supported)| (preferred, supported))
}

pub fn probe_may_add(
    adapter: ProviderAdapterKind,
    model_id: &str,
    protocol: UpstreamProtocolKind,
    declared: &[(String, UpstreamProtocolKind)],
) -> bool {
    representative_descriptor(adapter)
        .is_some_and(|descriptor| descriptor.protocol_probe.explicit_probe)
        && safety_ceiling_protocols(adapter, model_id, declared).contains(&protocol)
}

#[allow(clippy::too_many_arguments)]
pub fn apply_probe_observation(
    existing: Option<&PersistedModelProtocol>,
    scope: ContractScope,
    model_id: &str,
    protocol: UpstreamProtocolKind,
    success: bool,
    error: Option<String>,
    now: DateTime<Utc>,
    inside_ceiling: bool,
) -> Result<PersistedModelProtocol, String> {
    if success && !inside_ceiling {
        return Err(
            "probe success cannot add a model/protocol combination outside the adapter safety ceiling"
                .to_string(),
        );
    }
    let sanitized = error.map(|value| sanitize_probe_error(&value, None));
    if let Some(row) = existing {
        let mut next = row.clone();
        next.observed_at = Some(now);
        next.last_probe_at = Some(now);
        next.last_probe_result = Some(if success {
            ProbeResultKind::Success
        } else {
            ProbeResultKind::Failure
        });
        next.last_probe_error = if success { None } else { sanitized };
        if success {
            if next.verified_at.is_none() {
                next.verified_at = Some(now);
            }
            if !next.source.confers_support() {
                next.source = ContractEvidenceSource::ProbeConfirmed;
            }
        }
        return Ok(next);
    }
    if success {
        return Ok(PersistedModelProtocol {
            scope,
            model_id: model_id.to_string(),
            protocol,
            source: ContractEvidenceSource::ProbeConfirmed,
            verified_at: Some(now),
            observed_at: Some(now),
            last_probe_result: Some(ProbeResultKind::Success),
            last_probe_at: Some(now),
            last_probe_error: None,
        });
    }
    Ok(PersistedModelProtocol {
        scope,
        model_id: model_id.to_string(),
        protocol,
        source: ContractEvidenceSource::ProbeObserved,
        verified_at: None,
        observed_at: Some(now),
        last_probe_result: Some(ProbeResultKind::Failure),
        last_probe_at: Some(now),
        last_probe_error: sanitized,
    })
}

pub fn sanitize_probe_error(raw: &str, secret: Option<&str>) -> String {
    let value = secret.map_or_else(
        || sanitize_upstream_error_value_with_known_secret(raw, "").to_string(),
        |secret| sanitize_upstream_error_value_with_known_secret(raw, secret).to_string(),
    );
    truncate_chars(&strip_credential_urls(&value), MAX_PROBE_ERROR_CHARS)
}

fn strip_credential_urls(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let mut rest = input;
    while let Some(scheme_at) = rest.find("://") {
        let prefix_start = rest[..scheme_at]
            .rfind(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '+' || ch == '.' || ch == '-'))
            .map(|index| index + 1)
            .unwrap_or(0);
        output.push_str(&rest[..prefix_start]);
        let after_scheme = &rest[scheme_at + 3..];
        if let Some(at) = after_scheme.find('@') {
            let host_end = after_scheme[at + 1..]
                .find(|ch: char| ch == '/' || ch == '?' || ch == '#' || ch.is_whitespace())
                .map(|index| at + 1 + index)
                .unwrap_or(after_scheme.len());
            output.push_str(&rest[prefix_start..scheme_at + 3]);
            output.push_str(&after_scheme[at + 1..host_end]);
            rest = &after_scheme[host_end..];
        } else {
            output.push_str(&rest[prefix_start..scheme_at + 3]);
            rest = after_scheme;
        }
    }
    output.push_str(rest);
    output
}

fn truncate_chars(input: &str, max_chars: usize) -> String {
    let count = input.chars().count();
    if count <= max_chars {
        return input.to_string();
    }
    let mut truncated: String = input.chars().take(max_chars).collect();
    truncated.push('…');
    truncated
}

pub fn build_effective_contracts(
    zen_catalog: &ZenFreeModelCatalog,
    custom_runtimes: &[CustomAccountRuntime],
    persisted: PersistedContracts,
) -> EffectiveContractSet {
    let mut set = EffectiveContractSet::default();
    for provider_id in builtin_provider_scope_ids() {
        let scope = ContractScope::provider(provider_id);
        let adapter = adapter_kind_for_provider_scope(provider_id)
            .expect("builtin provider scopes map to adapters");
        let persisted_scope = persisted.scopes.get(&scope);
        let evidence = persisted.evidence.get(&scope).cloned().unwrap_or_default();
        let contract = merge_provider_scope(
            provider_id,
            adapter,
            zen_catalog,
            persisted_scope,
            &evidence,
        );
        set.providers.insert(provider_id.to_string(), contract);
    }
    for runtime in custom_runtimes {
        let scope = ContractScope::custom_endpoint(&runtime.account_id);
        let persisted_scope = persisted.scopes.get(&scope);
        let evidence = persisted.evidence.get(&scope).cloned().unwrap_or_default();
        let contract = merge_custom_scope(runtime, persisted_scope, &evidence);
        set.custom_endpoints
            .insert(runtime.account_id.clone(), contract);
    }
    set
}

fn representative_descriptor(
    adapter: ProviderAdapterKind,
) -> Option<crate::provider::ProviderDescriptor> {
    BUILTIN_PLANS.iter().find_map(|plan| {
        let kind = ProviderAdapterKind::from_offering(
            plan.offering.provider_id,
            plan.offering.offering_id,
        )?;
        (kind == adapter)
            .then(|| ProviderRegistry::get(plan.offering.provider_id, plan.offering.offering_id))
            .flatten()
    })
}

fn merge_provider_scope(
    provider_id: &str,
    adapter: ProviderAdapterKind,
    zen_catalog: &ZenFreeModelCatalog,
    persisted: Option<&PersistedScopeRow>,
    evidence: &[PersistedModelProtocol],
) -> EffectiveScopeContract {
    let descriptor = representative_descriptor(adapter).expect("adapter has a catalog offering");
    let switches = persisted.map(|row| row.switches).unwrap_or_default();
    let revision = persisted.map(|row| row.revision).unwrap_or(1);
    let (catalog, static_models) = match adapter {
        ProviderAdapterKind::OpenCodeGo => {
            let models: Vec<String> = supported_model_protocol_profiles()
                .map(|(id, _, _)| id.to_string())
                .collect();
            (
                EffectiveCatalog {
                    source: CATALOG_SOURCE_STATIC.to_string(),
                    source_url: String::new(),
                    refreshed_at: None,
                    models: models.clone(),
                    refresh_supported: false,
                },
                models,
            )
        }
        ProviderAdapterKind::ZenFree => {
            let models = persisted
                .filter(|row| !row.catalog_models.is_empty())
                .map(|row| row.catalog_models.clone())
                .unwrap_or_else(|| zen_catalog.models.clone());
            (
                EffectiveCatalog {
                    source: persisted
                        .map(|row| row.catalog_source.clone())
                        .filter(|source| !source.is_empty())
                        .unwrap_or_else(|| CATALOG_SOURCE_OFFICIAL_ZEN.to_string()),
                    source_url: persisted
                        .map(|row| row.catalog_source_url.clone())
                        .filter(|url| !url.is_empty())
                        .unwrap_or_else(|| zen_catalog.source_url.clone()),
                    refreshed_at: persisted
                        .and_then(|row| row.catalog_refreshed_at)
                        .or(zen_catalog.refreshed_at),
                    models: models.clone(),
                    refresh_supported: true,
                },
                models,
            )
        }
        ProviderAdapterKind::CommandCodeGoat => {
            let models: Vec<String> = command_code_protocol_profiles()
                .map(|profile| profile.upstream_id.to_string())
                .collect();
            (
                EffectiveCatalog {
                    source: CATALOG_SOURCE_STATIC.to_string(),
                    source_url: String::new(),
                    refreshed_at: None,
                    models: models.clone(),
                    refresh_supported: false,
                },
                models,
            )
        }
        ProviderAdapterKind::Scnet => {
            let models: Vec<String> = SCNET_TOKEN_PLAN_USABLE_MODELS
                .iter()
                .map(|id| (*id).to_string())
                .collect();
            (
                EffectiveCatalog {
                    source: CATALOG_SOURCE_STATIC.to_string(),
                    source_url: crate::provider::SCNET_TOKEN_PLAN_MODEL_SOURCE_URL.to_string(),
                    refreshed_at: None,
                    models: models.clone(),
                    refresh_supported: false,
                },
                models,
            )
        }
        ProviderAdapterKind::ConfigurableHttp => unreachable!("custom uses merge_custom_scope"),
    };

    let mut models = BTreeMap::new();
    for model_id in &static_models {
        models.insert(
            model_id.clone(),
            merge_model_contract(
                adapter,
                model_id,
                &[],
                ContractEvidenceSource::Static,
                evidence,
                switches,
                descriptor.inference.catalog_routable && descriptor.inference.production_inference,
            ),
        );
    }
    overlay_probe_confirmed_models(
        &mut models,
        adapter,
        &[],
        evidence,
        switches,
        descriptor.inference.catalog_routable && descriptor.inference.production_inference,
    );

    let mut disabled_reasons = Vec::new();
    if !descriptor.inference.catalog_routable {
        disabled_reasons.push("catalog offering is not routable".to_string());
    }
    if !descriptor.inference.production_inference {
        disabled_reasons.push("production inference is disabled".to_string());
    }

    EffectiveScopeContract {
        scope: ContractScope::provider(provider_id),
        provider_id: provider_id.to_string(),
        adapter_kind: adapter,
        catalog_routable: descriptor.inference.catalog_routable,
        production_inference: descriptor.inference.production_inference,
        switches,
        catalog,
        models,
        revision,
        fallback_priority: descriptor.protocol_probe.fallback_priority,
        disabled_reasons,
    }
}

fn merge_custom_scope(
    runtime: &CustomAccountRuntime,
    persisted: Option<&PersistedScopeRow>,
    evidence: &[PersistedModelProtocol],
) -> EffectiveScopeContract {
    let descriptor = ProviderRegistry::get(CUSTOM_PROVIDER_ID, CUSTOM_API_OFFERING_ID)
        .expect("custom offering is registered");
    let switches = persisted.map(|row| row.switches).unwrap_or_default();
    let revision = persisted.map(|row| row.revision).unwrap_or(1);
    let declared: Vec<(String, UpstreamProtocolKind)> = runtime
        .capabilities
        .iter()
        .map(|capability| (capability.model_id.clone(), capability.protocol))
        .collect();
    let catalog_models = persisted
        .map(|row| row.catalog_models.clone())
        .unwrap_or_default();
    let catalog = EffectiveCatalog {
        source: persisted
            .map(|row| row.catalog_source.clone())
            .filter(|source| !source.is_empty())
            .unwrap_or_else(|| CATALOG_SOURCE_CUSTOM_DISCOVERY.to_string()),
        source_url: persisted
            .map(|row| row.catalog_source_url.clone())
            .unwrap_or_default(),
        refreshed_at: persisted.and_then(|row| row.catalog_refreshed_at),
        models: catalog_models,
        refresh_supported: true,
    };

    let mut models = BTreeMap::new();
    let mut seen = HashSet::new();
    for (model_id, _) in &declared {
        if !seen.insert(model_id.to_ascii_lowercase()) {
            continue;
        }
        models.insert(
            model_id.clone(),
            merge_model_contract(
                ProviderAdapterKind::ConfigurableHttp,
                model_id,
                &declared,
                ContractEvidenceSource::Preset,
                evidence,
                switches,
                descriptor.inference.catalog_routable && descriptor.inference.production_inference,
            ),
        );
    }
    overlay_probe_confirmed_models(
        &mut models,
        ProviderAdapterKind::ConfigurableHttp,
        &declared,
        evidence,
        switches,
        descriptor.inference.catalog_routable && descriptor.inference.production_inference,
    );

    EffectiveScopeContract {
        scope: ContractScope::custom_endpoint(&runtime.account_id),
        provider_id: CUSTOM_PROVIDER_ID.to_string(),
        adapter_kind: ProviderAdapterKind::ConfigurableHttp,
        catalog_routable: descriptor.inference.catalog_routable,
        production_inference: descriptor.inference.production_inference,
        switches,
        catalog,
        models,
        revision,
        fallback_priority: descriptor.protocol_probe.fallback_priority,
        disabled_reasons: Vec::new(),
    }
}

fn preferred_protocol(
    adapter: ProviderAdapterKind,
    model_id: &str,
    declared: &[(String, UpstreamProtocolKind)],
) -> UpstreamProtocolKind {
    match adapter {
        ProviderAdapterKind::OpenCodeGo | ProviderAdapterKind::ZenFree => {
            opencode_profile(model_id)
                .and_then(|(preferred, _)| protocol_from_api(preferred))
                .unwrap_or(UpstreamProtocolKind::ChatCompletions)
        }
        ProviderAdapterKind::CommandCodeGoat => command_code_protocol_profiles()
            .find(|profile| profile.upstream_id.eq_ignore_ascii_case(model_id.trim()))
            .and_then(|profile| protocol_from_api(profile.preferred))
            .unwrap_or(UpstreamProtocolKind::ChatCompletions),
        ProviderAdapterKind::Scnet => UpstreamProtocolKind::ChatCompletions,
        ProviderAdapterKind::ConfigurableHttp => declared
            .iter()
            .find(|(id, _)| crate::custom::custom_model_id_matches(id, model_id))
            .map(|(_, protocol)| *protocol)
            .unwrap_or(UpstreamProtocolKind::ChatCompletions),
    }
}

fn merge_model_contract(
    adapter: ProviderAdapterKind,
    model_id: &str,
    declared: &[(String, UpstreamProtocolKind)],
    default_source: ContractEvidenceSource,
    evidence: &[PersistedModelProtocol],
    switches: ProtocolSwitches,
    adapter_routable: bool,
) -> EffectiveModelContract {
    let preferred = preferred_protocol(adapter, model_id, declared);
    let ceiling = safety_ceiling_protocols(adapter, model_id, declared);
    let static_verified = static_verified_protocols(adapter, model_id, declared);
    let mut protocols = BTreeMap::new();
    for protocol in UpstreamProtocolKind::ALL {
        let persisted = evidence
            .iter()
            .find(|row| custom_or_case_match(&row.model_id, model_id) && row.protocol == protocol);
        let in_ceiling = ceiling.contains(&protocol);
        let statically_verified = static_verified.contains(&protocol);
        if persisted.is_none() && !in_ceiling && !statically_verified {
            continue;
        }
        let source = persisted
            .map(|row| row.source)
            .unwrap_or(if statically_verified {
                default_source
            } else {
                ContractEvidenceSource::ProbeObserved
            });
        let available = source.confers_support() && (statically_verified || in_ceiling);
        let enabled = available && switches.is_enabled(protocol);
        protocols.insert(
            protocol.as_str().to_string(),
            EffectiveProtocolEvidence {
                protocol,
                available,
                enabled,
                source: if statically_verified && persisted.is_none() {
                    default_source
                } else {
                    persisted.map(|row| row.source).unwrap_or(source)
                },
                verified_at: persisted.and_then(|row| row.verified_at),
                observed_at: persisted.and_then(|row| row.observed_at),
                last_probe_result: persisted.and_then(|row| row.last_probe_result),
                last_probe_at: persisted.and_then(|row| row.last_probe_at),
                last_probe_error: persisted.and_then(|row| row.last_probe_error.clone()),
            },
        );
    }
    if !protocols.contains_key(preferred.as_str()) && static_verified.contains(&preferred) {
        protocols.insert(
            preferred.as_str().to_string(),
            EffectiveProtocolEvidence {
                protocol: preferred,
                available: true,
                enabled: switches.is_enabled(preferred),
                source: default_source,
                verified_at: None,
                observed_at: None,
                last_probe_result: None,
                last_probe_at: None,
                last_probe_error: None,
            },
        );
    }
    let mut disabled_reasons = Vec::new();
    if !adapter_routable {
        disabled_reasons.push("adapter safety ceiling forbids production routing".to_string());
    }
    if !protocols.values().any(|row| row.enabled) {
        disabled_reasons.push(NO_ENABLED_UPSTREAM_PROTOCOL.to_string());
    }
    EffectiveModelContract {
        model_id: model_id.to_string(),
        preferred_protocol: preferred,
        routable: adapter_routable && protocols.values().any(|row| row.enabled),
        protocols,
        disabled_reasons,
    }
}

fn overlay_probe_confirmed_models(
    models: &mut BTreeMap<String, EffectiveModelContract>,
    adapter: ProviderAdapterKind,
    declared: &[(String, UpstreamProtocolKind)],
    evidence: &[PersistedModelProtocol],
    switches: ProtocolSwitches,
    adapter_routable: bool,
) {
    let mut extra: HashSet<String> = HashSet::new();
    for row in evidence {
        if !row.source.confers_support() {
            continue;
        }
        if models
            .keys()
            .any(|id| custom_or_case_match(id, &row.model_id))
        {
            continue;
        }
        if !probe_may_add(adapter, &row.model_id, row.protocol, declared) {
            continue;
        }
        extra.insert(row.model_id.clone());
    }
    for model_id in extra {
        models.insert(
            model_id.clone(),
            merge_model_contract(
                adapter,
                &model_id,
                declared,
                ContractEvidenceSource::ProbeConfirmed,
                evidence,
                switches,
                adapter_routable,
            ),
        );
    }
}

fn custom_or_case_match(left: &str, right: &str) -> bool {
    crate::custom::custom_model_id_matches(left, right) || left.eq_ignore_ascii_case(right)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::custom::CustomAccountRuntime;
    use crate::kernel::ids::COMMAND_CODE_GOAT_DEEPSEEK_V4_FLASH_UPSTREAM;
    use crate::models::{AccountCustomConfig, AccountModelCapability};
    use crate::provider::{
        CUSTOM_API_OFFERING_ID, ConnectionVerificationStatus, UpstreamAuthScheme,
    };

    fn empty_persisted() -> PersistedContracts {
        PersistedContracts::default()
    }

    fn zen_seed() -> ZenFreeModelCatalog {
        ZenFreeModelCatalog::default()
    }

    fn go_contract() -> EffectiveScopeContract {
        build_effective_contracts(&zen_seed(), &[], empty_persisted())
            .providers
            .remove(OPENCODE_PROVIDER_ID)
            .unwrap()
    }

    #[test]
    fn scnet_offerings_share_one_provider_scope() {
        let basic = ContractScope::from_offering(
            SCNET_PROVIDER_ID,
            crate::provider::SCNET_TOKEN_PLAN_BASIC_OFFERING_ID,
            Some("a"),
        );
        let premium = ContractScope::from_offering(
            SCNET_PROVIDER_ID,
            crate::provider::SCNET_TOKEN_PLAN_PREMIUM_OFFERING_ID,
            Some("b"),
        );
        assert_eq!(basic, premium);
        assert_eq!(basic, Some(ContractScope::provider(SCNET_PROVIDER_ID)));
    }

    #[test]
    fn custom_endpoints_are_isolated_by_account() {
        let left =
            ContractScope::from_offering(CUSTOM_PROVIDER_ID, CUSTOM_API_OFFERING_ID, Some("one"));
        let right =
            ContractScope::from_offering(CUSTOM_PROVIDER_ID, CUSTOM_API_OFFERING_ID, Some("two"));
        assert_ne!(left, right);
        assert!(matches!(left, Some(ContractScope::CustomEndpoint(id)) if id == "one"));
    }

    #[test]
    fn probe_success_adds_inside_ceiling_and_failure_does_not_remove_static() {
        let now = Utc::now();
        let scope = ContractScope::provider(OPENCODE_PROVIDER_ID);
        let static_row = PersistedModelProtocol {
            scope: scope.clone(),
            model_id: "glm-5.2".into(),
            protocol: UpstreamProtocolKind::ChatCompletions,
            source: ContractEvidenceSource::Static,
            verified_at: None,
            observed_at: None,
            last_probe_result: None,
            last_probe_at: None,
            last_probe_error: None,
        };
        let failed = apply_probe_observation(
            Some(&static_row),
            scope.clone(),
            "glm-5.2",
            UpstreamProtocolKind::ChatCompletions,
            false,
            Some("upstream 500".into()),
            now,
            true,
        )
        .unwrap();
        assert_eq!(failed.source, ContractEvidenceSource::Static);
        assert!(failed.source.confers_support());
        assert_eq!(failed.last_probe_result, Some(ProbeResultKind::Failure));

        let added = apply_probe_observation(
            None,
            scope,
            "glm-5.2",
            UpstreamProtocolKind::Messages,
            true,
            None,
            now,
            true,
        )
        .unwrap();
        assert_eq!(added.source, ContractEvidenceSource::ProbeConfirmed);

        let rejected = apply_probe_observation(
            None,
            ContractScope::provider(OPENCODE_PROVIDER_ID),
            "not-a-catalog-model",
            UpstreamProtocolKind::ChatCompletions,
            true,
            None,
            now,
            false,
        );
        assert!(rejected.is_err());
    }

    #[test]
    fn opencode_ceiling_is_constructable_paths_not_static_model_protocols() {
        let grok_ceiling =
            safety_ceiling_protocols(ProviderAdapterKind::OpenCodeGo, "grok-4.5", &[]);
        let grok_static =
            static_verified_protocols(ProviderAdapterKind::OpenCodeGo, "grok-4.5", &[]);
        assert!(grok_ceiling.contains(&UpstreamProtocolKind::ChatCompletions));
        assert!(grok_ceiling.contains(&UpstreamProtocolKind::Responses));
        assert!(grok_ceiling.contains(&UpstreamProtocolKind::Messages));
        assert_eq!(grok_static, vec![UpstreamProtocolKind::Responses]);
        assert!(probe_may_add(
            ProviderAdapterKind::OpenCodeGo,
            "grok-4.5",
            UpstreamProtocolKind::ChatCompletions,
            &[],
        ));

        let unknown_zen =
            safety_ceiling_protocols(ProviderAdapterKind::ZenFree, "brand-new-promo-free", &[]);
        assert_eq!(unknown_zen, vec![UpstreamProtocolKind::ChatCompletions]);
        assert!(!probe_may_add(
            ProviderAdapterKind::CommandCodeGoat,
            COMMAND_CODE_GOAT_DEEPSEEK_V4_FLASH_UPSTREAM,
            UpstreamProtocolKind::ChatCompletions,
            &[],
        ));
    }

    #[test]
    fn probe_confirmed_opencode_extra_protocol_becomes_effective() {
        let mut persisted = empty_persisted();
        let now = Utc::now();
        let scope = ContractScope::provider(OPENCODE_PROVIDER_ID);
        persisted.evidence.insert(
            scope.clone(),
            vec![PersistedModelProtocol {
                scope,
                model_id: "grok-4.5".into(),
                protocol: UpstreamProtocolKind::ChatCompletions,
                source: ContractEvidenceSource::ProbeConfirmed,
                verified_at: Some(now),
                observed_at: Some(now),
                last_probe_result: Some(ProbeResultKind::Success),
                last_probe_at: Some(now),
                last_probe_error: None,
            }],
        );
        let go = build_effective_contracts(&zen_seed(), &[], persisted)
            .providers
            .remove(OPENCODE_PROVIDER_ID)
            .unwrap();
        let grok = go.model("grok-4.5").unwrap();
        assert!(grok.protocols.get("chat_completions").unwrap().available);
        assert!(grok.protocols.get("chat_completions").unwrap().enabled);
        assert!(grok.protocols.get("responses").unwrap().available);
        assert_eq!(
            grok.protocols.get("chat_completions").unwrap().source,
            ContractEvidenceSource::ProbeConfirmed
        );
    }

    #[test]
    fn probe_failure_does_not_add_or_remove_static_support() {
        let mut persisted = empty_persisted();
        let now = Utc::now();
        let scope = ContractScope::provider(OPENCODE_PROVIDER_ID);
        persisted.evidence.insert(
            scope.clone(),
            vec![PersistedModelProtocol {
                scope,
                model_id: "grok-4.5".into(),
                protocol: UpstreamProtocolKind::ChatCompletions,
                source: ContractEvidenceSource::ProbeObserved,
                verified_at: None,
                observed_at: Some(now),
                last_probe_result: Some(ProbeResultKind::Failure),
                last_probe_at: Some(now),
                last_probe_error: Some("upstream 500".into()),
            }],
        );
        let go = build_effective_contracts(&zen_seed(), &[], persisted)
            .providers
            .remove(OPENCODE_PROVIDER_ID)
            .unwrap();
        let grok = go.model("grok-4.5").unwrap();
        assert!(!grok.protocols.get("chat_completions").unwrap().available);
        assert!(grok.protocols.get("responses").unwrap().available);
        assert!(grok.routable);
    }

    #[test]
    fn switch_disables_without_destroying_evidence_and_reenable_needs_no_probe() {
        let mut persisted = empty_persisted();
        let scope = ContractScope::provider(OPENCODE_PROVIDER_ID);
        persisted.scopes.insert(
            scope.clone(),
            PersistedScopeRow {
                scope: scope.clone(),
                catalog_models: Vec::new(),
                catalog_refreshed_at: None,
                catalog_source: String::new(),
                catalog_source_url: String::new(),
                switches: ProtocolSwitches {
                    chat_completions: false,
                    responses: true,
                    messages: true,
                },
                revision: 2,
                updated_at: Utc::now(),
            },
        );
        let set = build_effective_contracts(&zen_seed(), &[], persisted);
        let go = set.providers.get(OPENCODE_PROVIDER_ID).unwrap();
        let glm = go.model("glm-5.3").unwrap();
        let chat = glm.protocols.get("chat_completions").unwrap();
        assert!(chat.available);
        assert!(!chat.enabled);
        assert!(!glm.routable);

        let grok = go.model("grok-4.5").unwrap();
        assert!(grok.routable);
        assert!(grok.protocols.get("responses").unwrap().enabled);
    }

    #[test]
    fn protocol_fallback_prefers_client_then_adapter_priority() {
        let mut go = go_contract();
        go.switches.chat_completions = false;
        let glm = go.models.get_mut("glm-5.2").unwrap();
        glm.protocols.get_mut("chat_completions").unwrap().enabled = false;
        glm.protocols.get_mut("responses").unwrap().enabled = true;
        glm.protocols.get_mut("messages").unwrap().enabled = true;
        glm.routable = true;

        let selected = select_upstream_protocol(&go, ApiFormat::Messages, "glm-5.2").unwrap();
        assert_eq!(selected, ApiFormat::Messages);

        let selected = select_upstream_protocol(&go, ApiFormat::Gemini, "glm-5.2").unwrap();
        assert_eq!(selected, ApiFormat::Responses);
    }

    #[test]
    fn no_valid_protocol_fails_locally() {
        let mut go = go_contract();
        go.switches = ProtocolSwitches {
            chat_completions: false,
            responses: false,
            messages: false,
        };
        for model in go.models.values_mut() {
            for evidence in model.protocols.values_mut() {
                evidence.enabled = false;
            }
            model.routable = false;
        }
        let error =
            select_upstream_protocol(&go, ApiFormat::ChatCompletions, "glm-5.3").unwrap_err();
        assert_eq!(error.message, NO_ENABLED_UPSTREAM_PROTOCOL);
    }

    #[test]
    fn goat_and_scnet_remain_unroutable_after_probe_success() {
        let now = Utc::now();
        let mut persisted = empty_persisted();
        persisted.evidence.insert(
            ContractScope::provider(COMMAND_CODE_PROVIDER_ID),
            vec![PersistedModelProtocol {
                scope: ContractScope::provider(COMMAND_CODE_PROVIDER_ID),
                model_id: COMMAND_CODE_GOAT_DEEPSEEK_V4_FLASH_UPSTREAM.into(),
                protocol: UpstreamProtocolKind::ChatCompletions,
                source: ContractEvidenceSource::ProbeConfirmed,
                verified_at: Some(now),
                observed_at: Some(now),
                last_probe_result: Some(ProbeResultKind::Success),
                last_probe_at: Some(now),
                last_probe_error: None,
            }],
        );
        persisted.evidence.insert(
            ContractScope::provider(SCNET_PROVIDER_ID),
            vec![PersistedModelProtocol {
                scope: ContractScope::provider(SCNET_PROVIDER_ID),
                model_id: "GLM-5.2".into(),
                protocol: UpstreamProtocolKind::ChatCompletions,
                source: ContractEvidenceSource::ProbeConfirmed,
                verified_at: Some(now),
                observed_at: Some(now),
                last_probe_result: Some(ProbeResultKind::Success),
                last_probe_at: Some(now),
                last_probe_error: None,
            }],
        );
        let set = build_effective_contracts(&zen_seed(), &[], persisted);
        let goat = set.providers.get(COMMAND_CODE_PROVIDER_ID).unwrap();
        assert!(!goat.catalog_routable);
        assert!(!goat.production_inference);
        assert!(
            !goat
                .model(COMMAND_CODE_GOAT_DEEPSEEK_V4_FLASH_UPSTREAM)
                .unwrap()
                .routable
        );
        let scnet = set.providers.get(SCNET_PROVIDER_ID).unwrap();
        assert!(!scnet.catalog_routable);
        assert!(!scnet.model("GLM-5.2").unwrap().routable);
        assert!(!ProviderAdapterKind::CommandCodeGoat.protocol_probe_supported());
        assert!(!ProviderAdapterKind::Scnet.protocol_probe_supported());
    }

    #[test]
    fn custom_discovery_does_not_become_routable_without_declaration() {
        let runtime = CustomAccountRuntime {
            account_id: "custom-1".into(),
            enabled: true,
            verification_status: ConnectionVerificationStatus::Verified,
            setup_ready: true,
            has_key: true,
            config: AccountCustomConfig {
                account_id: "custom-1".into(),
                base_url: "https://api.example.com/v1".into(),
                upstream_protocol: UpstreamProtocolKind::ChatCompletions,
                auth_scheme: UpstreamAuthScheme::Bearer,
                created_at: Utc::now(),
                updated_at: Utc::now(),
            },
            capabilities: vec![AccountModelCapability {
                account_id: "custom-1".into(),
                model_id: "declared-model".into(),
                protocol: UpstreamProtocolKind::ChatCompletions,
                verified_at: None,
                source: "manual".into(),
            }],
        };
        let mut persisted = empty_persisted();
        let scope = ContractScope::custom_endpoint("custom-1");
        persisted.scopes.insert(
            scope.clone(),
            PersistedScopeRow {
                scope: scope.clone(),
                catalog_models: vec!["discovered-only".into()],
                catalog_refreshed_at: Some(Utc::now()),
                catalog_source: CATALOG_SOURCE_CUSTOM_DISCOVERY.into(),
                catalog_source_url: String::new(),
                switches: ProtocolSwitches::default(),
                revision: 1,
                updated_at: Utc::now(),
            },
        );
        let set = build_effective_contracts(&zen_seed(), &[runtime], persisted);
        let custom = set.custom_endpoints.get("custom-1").unwrap();
        assert!(
            custom
                .catalog
                .models
                .contains(&"discovered-only".to_string())
        );
        assert!(custom.model("declared-model").unwrap().routable);
        assert!(custom.model("discovered-only").is_none());
    }

    #[test]
    fn sanitize_probe_error_strips_userinfo_and_truncates() {
        let raw = format!(
            "failed https://user:secret@api.example.com/v1 {}",
            "x".repeat(600)
        );
        let sanitized = sanitize_probe_error(&raw, Some("secret"));
        assert!(!sanitized.contains("user:secret"));
        assert!(!sanitized.contains("secret"));
        assert!(sanitized.chars().count() <= MAX_PROBE_ERROR_CHARS + 1);
    }
}
