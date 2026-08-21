//! Candidate request materialization for the alias runtime.
//!
//! # Adapter interface
//!
//! Later provider adapters should treat this module as the boundary between
//! a parsed client request and an upstream call:
//!
//! 1. Parse the client protocol **once** with
//!    [`parse_client_request`] / [`parse_gemini_request`].
//! 2. Resolve the requested name through [`crate::alias::resolve`]. Alias
//!    results may follow account order, sticky, and fallback. A unique raw
//!    upstream ID is pinned to its single mapping; overlapping raw IDs return
//!    [`crate::alias::AMBIGUOUS_MODEL_ID`].
//! 3. Build candidate plans from [`ResolvedModel`] mappings (and Alias
//!    `prefer_twin` when Prefer-mode context fits). Match accounts in saved
//!    account order through [`super::provider_adapter::supports_plan`], using
//!    mapping order only as the per-account tie-break. Protocol selection
//!    uses the OpenCode `MODEL_PROTOCOLS` table for Go/Zen upstream models
//!    and the Command Code-native table for GOAT raw IDs.
//!    **Never** trial a billable inference path.
//! 4. Ask [`super::provider_adapter::resolve_route`] for endpoint + auth.
//!    Production GOAT stays fail-closed (catalog unroutable, drafts disabled,
//!    no live adapter without the loopback test seam). The official slash raw
//!    ID pins to command-code/goat without stealing Go kebab aliases.
//!
//! OpenCode Go and Zen Free are implemented here. Claude Desktop
//! `sonnet` / `opus` / `haiku` aliases are rewritten to a configured Go
//! model before resolution; the original Claude name is kept as
//! `RequestPlan.client_model`.

use crate::alias::{ProviderMapping, ResolveError, ResolvedModel};
use crate::custom::CustomAccountRuntime;
use crate::gateway::free_models::{decide_route, resolve_upstream_base};
use crate::gateway::protocol::{
    ApiFormat, CustomRouteSpec, MaterializeSpec, ParsedClientRequest, ProtocolError, RequestPlan,
    materialize_parsed_request,
};
use crate::gateway::provider_adapter;
use crate::gateway::routing::RoutingCandidate;
use crate::models::{Account, AppConfig, FreeModelRouting, UpstreamChannel};
use crate::pricing::normalize_model_name;
use crate::provider::{ANONYMOUS_FREE_OFFERING_ID, OPENCODE_ZEN_FREE_PROVIDER_ID};
use axum::http::StatusCode;
use bytes::Bytes;

pub use crate::gateway::protocol::{
    parse_client_request as parse_client, parse_gemini_request as parse_gemini,
};

#[derive(Debug, Clone)]
pub(crate) struct MaterializedCandidate {
    pub routing: RoutingCandidate,
    pub plan: RequestPlan,
}

#[derive(Debug, Clone)]
pub(crate) struct MaterializedRouteSet {
    pub routes: Vec<MaterializedCandidate>,
    pub free_only: bool,
    pub incompatibility: Option<String>,
}

/// Diagnostics are not a candidate protocol decision. If a resolution can use
/// Custom, preserve the client wire format until each actual mapping/account is
/// materialized. Pure builtin resolutions keep their normal early validation.
pub(crate) fn diagnostic_forced_upstream(
    resolved: &ResolvedModel,
    client: ApiFormat,
) -> Option<ApiFormat> {
    let has_custom = match resolved {
        ResolvedModel::PinnedRaw { mapping, .. } => mapping.is_custom_api(),
        ResolvedModel::Alias { mappings, .. } => mappings
            .iter()
            .any(|mapping| mapping.routeable && mapping.is_custom_api()),
    };
    has_custom.then_some(client)
}

pub(crate) fn protocol_error_from_resolve(error: ResolveError) -> ProtocolError {
    match error.code() {
        Some(code) => ProtocolError::with_code(StatusCode::BAD_REQUEST, code, error.message()),
        None => ProtocolError::new(error.message()),
    }
}

/// Canonical registry alias persisted on forward logs for this resolution.
pub(crate) fn resolved_alias_from_model(resolved: &ResolvedModel) -> Option<String> {
    match resolved {
        ResolvedModel::Alias { alias, .. } => Some((*alias).to_string()),
        ResolvedModel::PinnedRaw { mapping, .. } => registry_alias_for_mapping(mapping),
    }
}

/// Registry alias for a unique raw mapping, when one is published.
pub(crate) fn registry_alias_for_mapping(mapping: &ProviderMapping) -> Option<String> {
    for published in crate::alias::published_aliases() {
        match crate::alias::resolve(published) {
            Ok(ResolvedModel::Alias {
                alias, mappings, ..
            }) => {
                if mappings.iter().any(|candidate| {
                    candidate.provider_id == mapping.provider_id
                        && candidate.offering_id == mapping.offering_id
                        && candidate.upstream_model == mapping.upstream_model
                }) {
                    return Some(alias.to_string());
                }
            }
            Ok(ResolvedModel::PinnedRaw { .. }) | Err(_) => {}
        }
    }
    None
}

/// Request / alias / upstream identity persisted on every forward log row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NativeLogIdentity {
    pub requested_model: String,
    pub resolved_alias: Option<String>,
    pub upstream_model: String,
}

/// Carry materialization identity into logs without inferring at the DB layer.
pub(crate) fn native_log_identity(plan: &RequestPlan) -> NativeLogIdentity {
    let requested_model = plan.log_requested_model().to_string();
    let upstream_model = plan.log_upstream_model().to_string();
    let resolved_alias = plan
        .resolved_alias
        .clone()
        .filter(|alias| !alias.is_empty())
        .or_else(|| resolved_alias_for_name(&requested_model))
        .or_else(|| {
            plan.original_model
                .as_deref()
                .and_then(resolved_alias_for_name)
        })
        .or_else(|| resolved_alias_for_name(&upstream_model));
    NativeLogIdentity {
        requested_model,
        resolved_alias,
        upstream_model,
    }
}

pub(crate) fn resolved_alias_for_name(name: &str) -> Option<String> {
    match crate::alias::resolve(name) {
        Ok(resolved) => resolved_alias_from_model(&resolved),
        Err(_) => None,
    }
}

/// Preserve original casing when the client name already identifies this mapping.
pub(crate) fn upstream_model_for(requested: &str, canonical: &str) -> String {
    if normalize_model_name(requested) == normalize_model_name(canonical) {
        requested.to_string()
    } else {
        canonical.to_string()
    }
}

struct MappingPlan {
    mapping: ProviderMapping,
    plan: RequestPlan,
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn materialize_account_routes(
    accounts: &[Account],
    config: &AppConfig,
    parsed: &ParsedClientRequest,
    resolved: &ResolvedModel,
    client_model: &str,
    routing_model: &str,
    client_body: &Bytes,
    free_available: bool,
    custom_runtimes: &std::collections::HashMap<String, CustomAccountRuntime>,
) -> Result<MaterializedRouteSet, ProtocolError> {
    match resolved {
        ResolvedModel::PinnedRaw { mapping, .. } => {
            let plan = materialize_mapping_plan(
                config,
                parsed,
                client_model,
                routing_model,
                mapping,
                resolved_alias_from_model(resolved),
                None,
                false,
            )?;
            collect_mapping_plans(
                accounts,
                config,
                parsed,
                client_model,
                routing_model,
                resolved_alias_from_model(resolved),
                vec![MappingPlan {
                    mapping: mapping.clone(),
                    plan,
                }],
                false,
                Vec::new(),
                custom_runtimes,
            )
        }
        ResolvedModel::Alias {
            mappings,
            prefer_twin,
            alias,
            ..
        } => {
            let routeable: Vec<ProviderMapping> = mappings
                .iter()
                .filter(|mapping| mapping.routeable)
                .cloned()
                .collect();
            let zen_only =
                !routeable.is_empty() && routeable.iter().all(|mapping| mapping.is_zen_free());
            if zen_only {
                decide_route(
                    config.free_model_routing,
                    routing_model,
                    parsed.client,
                    parsed.client,
                    client_body,
                )
                .map_err(ProtocolError::new)?;
            }

            let mut plans = Vec::new();
            let mut rejected = Vec::new();
            let mut first_materialization_error = None;
            let resolved_alias = Some(alias.to_string());
            for mapping in &routeable {
                if mapping.is_zen_free() && !free_available && !zen_only {
                    continue;
                }
                match materialize_mapping_plan(
                    config,
                    parsed,
                    client_model,
                    routing_model,
                    mapping,
                    resolved_alias.clone(),
                    None,
                    false,
                ) {
                    Ok(plan) => plans.push(MappingPlan {
                        mapping: mapping.clone(),
                        plan,
                    }),
                    Err(error) => {
                        rejected.push(format!(
                            "{}/{} mapping `{}`: {error}",
                            mapping.provider_id, mapping.offering_id, mapping.upstream_model
                        ));
                        first_materialization_error.get_or_insert(error);
                    }
                }
            }

            if let Some(twin) = *prefer_twin
                && should_overlay_prefer_twin(config, alias, parsed, client_body)
                && free_available
                && !plans
                    .iter()
                    .any(|candidate| candidate.plan.channel == UpstreamChannel::Free)
            {
                let twin_mapping = ProviderMapping {
                    provider_id: OPENCODE_ZEN_FREE_PROVIDER_ID,
                    offering_id: ANONYMOUS_FREE_OFFERING_ID,
                    upstream_model: twin,
                    routeable: true,
                };
                match materialize_mapping_plan(
                    config,
                    parsed,
                    client_model,
                    routing_model,
                    &twin_mapping,
                    resolved_alias.clone(),
                    Some(routing_model.to_string()),
                    true,
                ) {
                    Ok(plan) => plans.push(MappingPlan {
                        mapping: twin_mapping,
                        plan,
                    }),
                    Err(error) => {
                        rejected.push(format!(
                            "{}/{} mapping `{}`: {error}",
                            twin_mapping.provider_id,
                            twin_mapping.offering_id,
                            twin_mapping.upstream_model
                        ));
                        first_materialization_error.get_or_insert(error);
                    }
                }
            }

            // Preserve the existing pure-builtin 400 when every actual
            // mapping rejects the request. Mixed resolutions continue so a
            // compatible Custom account can still be materialized below.
            if plans.is_empty()
                && let Some(error) = first_materialization_error
            {
                return Err(error);
            }

            collect_mapping_plans(
                accounts,
                config,
                parsed,
                client_model,
                routing_model,
                Some(alias.to_string()),
                plans,
                zen_only,
                rejected,
                custom_runtimes,
            )
        }
    }
}

fn should_overlay_prefer_twin(
    config: &AppConfig,
    alias: &str,
    parsed: &ParsedClientRequest,
    client_body: &Bytes,
) -> bool {
    if config.free_model_routing != FreeModelRouting::Prefer {
        return false;
    }
    decide_route(
        FreeModelRouting::Prefer,
        alias,
        parsed.client,
        parsed.client,
        client_body,
    )
    .is_ok_and(|decision| decision.allow_go_fallback && decision.channel == UpstreamChannel::Free)
}

#[allow(clippy::too_many_arguments)]
fn materialize_mapping_plan(
    config: &AppConfig,
    parsed: &ParsedClientRequest,
    client_model: &str,
    routing_model: &str,
    mapping: &ProviderMapping,
    resolved_alias: Option<String>,
    original_model: Option<String>,
    allow_go_fallback: bool,
) -> Result<RequestPlan, ProtocolError> {
    let channel = if mapping.is_zen_free() {
        UpstreamChannel::Free
    } else {
        // GOAT / SCNet / Custom share the Go channel discriminator. Custom is
        // rematerialized per account with that account's configured protocol.
        UpstreamChannel::Go
    };
    let model = if mapping.is_custom_api() {
        routing_model.to_string()
    } else if original_model.is_some() {
        mapping.upstream_model.to_string()
    } else {
        upstream_model_for(routing_model, mapping.upstream_model)
    };
    let forced_upstream = mapping.is_custom_api().then_some(parsed.client);
    materialize_channel_plan(
        config,
        parsed,
        client_model,
        &model,
        resolved_alias,
        channel,
        original_model,
        allow_go_fallback,
        forced_upstream,
        None,
    )
}

#[allow(clippy::too_many_arguments)]
fn materialize_channel_plan(
    config: &AppConfig,
    parsed: &ParsedClientRequest,
    client_model: &str,
    model: &str,
    resolved_alias: Option<String>,
    channel: UpstreamChannel,
    original_model: Option<String>,
    allow_go_fallback: bool,
    forced_upstream: Option<ApiFormat>,
    custom_route: Option<CustomRouteSpec>,
) -> Result<RequestPlan, ProtocolError> {
    let base =
        resolve_upstream_base(channel, &config.upstream_base_url).map_err(ProtocolError::new)?;
    materialize_parsed_request(
        parsed,
        &MaterializeSpec {
            client_model: client_model.to_string(),
            upstream_model: model.to_string(),
            resolved_alias,
            channel,
            upstream_base_override: match channel {
                UpstreamChannel::Free => Some(base),
                UpstreamChannel::Go => None,
            },
            original_model,
            allow_go_fallback,
            forced_upstream,
            custom_route,
        },
    )
}

fn materialize_custom_account_plan(
    account: &Account,
    runtime: Option<&CustomAccountRuntime>,
    config: &AppConfig,
    parsed: &ParsedClientRequest,
    client_model: &str,
    routing_model: &str,
    resolved_alias: Option<String>,
) -> Result<RequestPlan, ProtocolError> {
    let runtime = runtime.ok_or_else(|| {
        ProtocolError::new(format!(
            "Custom account `{}` is missing a persisted base URL, protocol, and auth scheme",
            account.name
        ))
    })?;
    if !runtime.eligible() {
        return Err(ProtocolError::new(format!(
            "Custom account `{}` is not enabled and verified",
            account.name
        )));
    }
    let capability = runtime.capability_matching(routing_model).ok_or_else(|| {
        ProtocolError::new(format!(
            "Custom account `{}` did not declare model `{routing_model}`",
            account.name
        ))
    })?;
    let resolved_alias = resolved_alias
        .filter(|alias| !alias.is_empty())
        .or_else(|| Some(capability.model_id.clone()));
    materialize_channel_plan(
        config,
        parsed,
        client_model,
        &capability.model_id,
        resolved_alias,
        UpstreamChannel::Go,
        None,
        false,
        Some(crate::custom::api_format_for_custom_protocol(
            runtime.config.upstream_protocol,
        )),
        Some(CustomRouteSpec {
            base_url: runtime.config.base_url.clone(),
            auth_scheme: runtime.config.auth_scheme,
        }),
    )
}

#[allow(clippy::too_many_arguments)]
fn collect_mapping_plans(
    accounts: &[Account],
    config: &AppConfig,
    parsed: &ParsedClientRequest,
    client_model: &str,
    routing_model: &str,
    resolved_alias: Option<String>,
    plans: Vec<MappingPlan>,
    free_only: bool,
    mut rejected: Vec<String>,
    custom_runtimes: &std::collections::HashMap<String, CustomAccountRuntime>,
) -> Result<MaterializedRouteSet, ProtocolError> {
    let mut routes = Vec::new();
    for account in accounts {
        for candidate in &plans {
            if account.provider_id != candidate.mapping.provider_id
                || account.offering_id != candidate.mapping.offering_id
            {
                continue;
            }
            if routes.iter().any(|route: &MaterializedCandidate| {
                route.routing.account.id == account.id
                    && route.routing.channel == candidate.plan.channel
            }) {
                continue;
            }
            let plan = if candidate.mapping.is_custom_api() {
                match materialize_custom_account_plan(
                    account,
                    custom_runtimes.get(&account.id),
                    config,
                    parsed,
                    client_model,
                    routing_model,
                    resolved_alias.clone(),
                ) {
                    Ok(plan) => plan,
                    Err(error) => {
                        rejected.push(format!(
                            "{}/{} account `{}`: {error}",
                            account.provider_id, account.offering_id, account.name
                        ));
                        continue;
                    }
                }
            } else {
                candidate.plan.clone()
            };
            match provider_adapter::supports_plan(account, config, &plan) {
                Ok(()) => {
                    routes.push(MaterializedCandidate {
                        routing: RoutingCandidate {
                            account: account.clone(),
                            channel: plan.channel,
                            resolved_model: plan.model.clone(),
                        },
                        plan,
                    });
                    break;
                }
                Err(error) => rejected.push(format!(
                    "{}/{} account `{}`: {error}",
                    account.provider_id, account.offering_id, account.name
                )),
            }
        }
    }
    let incompatibility = (routes.is_empty() && !rejected.is_empty()).then(|| {
        format!(
            "no compatible provider account for model `{client_model}` and {:?}: {}",
            parsed.client,
            rejected.join("; ")
        )
    });
    Ok(MaterializedRouteSet {
        routes,
        free_only,
        incompatibility,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::alias::{self, ResolvedModel};
    use crate::crypto::{KeyCipher, StaticKeyCipher};
    use crate::custom::CustomAccountRuntime;
    use crate::gateway::protocol::{ApiFormat, parse_client_request};
    use crate::gateway::provider_adapter::install_goat_loopback_route_for_test;
    use crate::models::{
        Account, AccountCustomConfig, AccountModelCapability, AccountSetupStep, AccountType,
        AppConfig, FreeModelRouting,
    };
    use crate::provider::{
        COMMAND_CODE_GOAT_DEEPSEEK_V4_FLASH_ALIAS, COMMAND_CODE_GOAT_DEEPSEEK_V4_FLASH_UPSTREAM,
        COMMAND_CODE_PROVIDER_ID, CUSTOM_API_OFFERING_ID, CUSTOM_PROVIDER_ID,
        ConnectionVerificationStatus, CredentialKind, GO_OFFERING_ID, GOAT_OFFERING_ID,
        OPENCODE_PROVIDER_ID, QuotaScope, UpstreamAuthScheme, UpstreamProtocolKind,
        ZEN_FREE_ACCOUNT_ID, ZEN_FREE_ACCOUNT_NAME,
    };
    use chrono::Utc;
    use serde_json::json;
    use std::sync::Arc;

    fn chat_body(model: &str) -> Bytes {
        Bytes::from(
            serde_json::to_vec(&json!({
                "model": model,
                "messages": [{"role": "user", "content": "hi"}]
            }))
            .unwrap(),
        )
    }

    fn account(
        id: &str,
        provider_id: &str,
        offering_id: &str,
        credential_kind: CredentialKind,
        quota_scope: QuotaScope,
    ) -> Account {
        let cipher: Arc<dyn KeyCipher + Send + Sync> = Arc::new(StaticKeyCipher::new("test"));
        Account {
            id: id.into(),
            provider_id: provider_id.into(),
            offering_id: offering_id.into(),
            credential_kind,
            quota_scope,
            free_alias_enabled: false,
            name: id.into(),
            username: None,
            password_cipher: None,
            key_cipher: cipher.encrypt("key").unwrap(),
            enabled: true,
            account_type: AccountType::Key,
            setup_step: AccountSetupStep::Ready,
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
            notes: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    fn go_account(id: &str) -> Account {
        account(
            id,
            OPENCODE_PROVIDER_ID,
            GO_OFFERING_ID,
            CredentialKind::ApiKey,
            QuotaScope::Key,
        )
    }

    fn zen_account() -> Account {
        let mut item = account(
            ZEN_FREE_ACCOUNT_ID,
            OPENCODE_ZEN_FREE_PROVIDER_ID,
            ANONYMOUS_FREE_OFFERING_ID,
            CredentialKind::None,
            QuotaScope::EgressIp,
        );
        item.name = ZEN_FREE_ACCOUNT_NAME.into();
        item
    }

    fn goat_account(id: &str) -> Account {
        account(
            id,
            COMMAND_CODE_PROVIDER_ID,
            GOAT_OFFERING_ID,
            CredentialKind::ApiKey,
            QuotaScope::Key,
        )
    }

    fn routes_for(
        model: &str,
        accounts: &[Account],
        config: &AppConfig,
        free_available: bool,
    ) -> MaterializedRouteSet {
        let body = chat_body(model);
        let parsed = parse_client_request(ApiFormat::ChatCompletions, body.clone()).unwrap();
        let resolved = alias::resolve(model).unwrap();
        materialize_account_routes(
            accounts,
            config,
            &parsed,
            &resolved,
            &parsed.requested_model,
            model,
            &body,
            free_available,
            &std::collections::HashMap::new(),
        )
        .unwrap()
    }

    #[test]
    fn go_alias_materializes_opencode_go_candidates() {
        let config = AppConfig::default();
        let set = routes_for(
            "glm-5.2",
            &[go_account("go-1"), zen_account()],
            &config,
            true,
        );
        assert_eq!(set.routes.len(), 1);
        assert_eq!(set.routes[0].routing.account.id, "go-1");
        assert_eq!(set.routes[0].plan.model, "glm-5.2");
        assert_eq!(set.routes[0].plan.client_model, "glm-5.2");
        assert_eq!(set.routes[0].plan.channel, UpstreamChannel::Go);
        assert!(!set.free_only);
        let identity = native_log_identity(&set.routes[0].plan);
        assert_eq!(identity.requested_model, "glm-5.2");
        assert_eq!(identity.resolved_alias.as_deref(), Some("glm-5.2"));
        assert_eq!(identity.upstream_model, "glm-5.2");
    }

    #[test]
    fn mixed_case_go_alias_preserves_requested_casing() {
        let config = AppConfig::default();
        let set = routes_for("MiniMax-M3", &[go_account("go-1")], &config, true);
        assert_eq!(set.routes[0].plan.model, "MiniMax-M3");
        assert_eq!(set.routes[0].plan.client_model, "MiniMax-M3");
        assert_eq!(set.routes[0].plan.upstream, ApiFormat::ChatCompletions);
        let identity = native_log_identity(&set.routes[0].plan);
        assert_eq!(identity.requested_model, "MiniMax-M3");
        assert_eq!(identity.resolved_alias.as_deref(), Some("minimax-m3"));
        assert_eq!(identity.upstream_model, "MiniMax-M3");
    }

    #[test]
    fn zen_free_alias_materializes_anonymous_channel() {
        let config = AppConfig::default();
        let set = routes_for(
            "deepseek-v4-flash-free",
            &[go_account("go-1"), zen_account()],
            &config,
            true,
        );
        assert!(set.free_only);
        assert_eq!(set.routes.len(), 1);
        assert_eq!(set.routes[0].routing.account.id, ZEN_FREE_ACCOUNT_ID);
        assert_eq!(set.routes[0].plan.channel, UpstreamChannel::Free);
        assert_eq!(set.routes[0].plan.model, "deepseek-v4-flash-free");
        assert!(set.routes[0].plan.upstream_base_override.is_some());
    }

    #[test]
    fn prefer_twin_builds_go_and_free_candidates() {
        let config = AppConfig {
            free_model_routing: FreeModelRouting::Prefer,
            ..AppConfig::default()
        };
        let set = routes_for(
            "deepseek-v4-flash",
            &[go_account("go-1"), zen_account()],
            &config,
            true,
        );
        assert_eq!(set.routes.len(), 2);
        assert_eq!(set.routes[0].routing.account.id, "go-1");
        assert_eq!(set.routes[0].plan.channel, UpstreamChannel::Go);
        assert_eq!(set.routes[1].routing.account.id, ZEN_FREE_ACCOUNT_ID);
        assert_eq!(set.routes[1].plan.channel, UpstreamChannel::Free);
        assert_eq!(set.routes[1].plan.model, "deepseek-v4-flash-free");
        assert_eq!(set.routes[1].plan.client_model, "deepseek-v4-flash");
        assert_eq!(
            set.routes[1].plan.original_model.as_deref(),
            Some("deepseek-v4-flash")
        );
        assert!(set.routes[1].plan.allow_go_fallback);
        let free_identity = native_log_identity(&set.routes[1].plan);
        assert_eq!(free_identity.requested_model, "deepseek-v4-flash");
        assert_eq!(
            free_identity.resolved_alias.as_deref(),
            Some("deepseek-v4-flash")
        );
        assert_eq!(free_identity.upstream_model, "deepseek-v4-flash-free");
    }

    #[test]
    fn pinned_raw_skips_prefer_overlay() {
        let config = AppConfig {
            free_model_routing: FreeModelRouting::Prefer,
            ..AppConfig::default()
        };
        let body = chat_body("vendor.gadget-v1");
        let parsed = parse_client_request(ApiFormat::ChatCompletions, body.clone()).unwrap();
        let resolved = ResolvedModel::PinnedRaw {
            requested: "vendor.gadget-v1".into(),
            mapping: crate::alias::ProviderMapping {
                provider_id: OPENCODE_PROVIDER_ID,
                offering_id: GO_OFFERING_ID,
                upstream_model: "deepseek-v4-flash",
                routeable: true,
            },
        };
        let set = materialize_account_routes(
            &[go_account("go-1"), zen_account()],
            &config,
            &parsed,
            &resolved,
            "vendor.gadget-v1",
            "vendor.gadget-v1",
            &body,
            true,
            &std::collections::HashMap::new(),
        )
        .unwrap();
        assert_eq!(set.routes.len(), 1);
        assert_eq!(set.routes[0].routing.account.id, "go-1");
        assert_eq!(set.routes[0].plan.channel, UpstreamChannel::Go);
        assert_eq!(set.routes[0].plan.model, "deepseek-v4-flash");
        assert_eq!(set.routes[0].plan.client_model, "vendor.gadget-v1");
        assert!(!set.routes[0].plan.allow_go_fallback);
        assert!(set.routes[0].plan.original_model.is_none());
        let identity = native_log_identity(&set.routes[0].plan);
        assert_eq!(identity.requested_model, "vendor.gadget-v1");
        assert_eq!(
            identity.resolved_alias.as_deref(),
            Some("deepseek-v4-flash")
        );
        assert_eq!(identity.upstream_model, "deepseek-v4-flash");
    }

    #[test]
    fn mapping_plans_follow_registry_order_while_candidates_keep_account_order() {
        let config = AppConfig::default();
        let body = chat_body("widget");
        let parsed = parse_client_request(ApiFormat::ChatCompletions, body.clone()).unwrap();
        let resolved = ResolvedModel::Alias {
            requested: "widget".into(),
            alias: "widget",
            mappings: vec![
                crate::alias::ProviderMapping {
                    provider_id: OPENCODE_ZEN_FREE_PROVIDER_ID,
                    offering_id: ANONYMOUS_FREE_OFFERING_ID,
                    upstream_model: "deepseek-v4-flash-free",
                    routeable: true,
                },
                crate::alias::ProviderMapping {
                    provider_id: OPENCODE_PROVIDER_ID,
                    offering_id: GO_OFFERING_ID,
                    upstream_model: "glm-5.2",
                    routeable: true,
                },
            ],
            prefer_twin: None,
        };
        let set = materialize_account_routes(
            &[go_account("go-1"), zen_account()],
            &config,
            &parsed,
            &resolved,
            "widget",
            "widget",
            &body,
            true,
            &std::collections::HashMap::new(),
        )
        .unwrap();
        assert_eq!(set.routes.len(), 2);
        assert_eq!(set.routes[0].routing.account.id, "go-1");
        assert_eq!(set.routes[0].plan.channel, UpstreamChannel::Go);
        assert_eq!(set.routes[0].plan.model, "glm-5.2");
        assert_eq!(set.routes[1].routing.account.id, ZEN_FREE_ACCOUNT_ID);
        assert_eq!(set.routes[1].plan.channel, UpstreamChannel::Free);
        assert_eq!(set.routes[1].plan.model, "deepseek-v4-flash-free");
    }

    #[test]
    fn pinned_raw_unimplemented_provider_is_fail_closed_through_adapter() {
        let config = AppConfig::default();
        let body = chat_body(COMMAND_CODE_GOAT_DEEPSEEK_V4_FLASH_UPSTREAM);
        let parsed = parse_client_request(ApiFormat::ChatCompletions, body.clone()).unwrap();
        let resolved = ResolvedModel::PinnedRaw {
            requested: COMMAND_CODE_GOAT_DEEPSEEK_V4_FLASH_UPSTREAM.into(),
            mapping: crate::alias::ProviderMapping {
                provider_id: COMMAND_CODE_PROVIDER_ID,
                offering_id: GOAT_OFFERING_ID,
                upstream_model: COMMAND_CODE_GOAT_DEEPSEEK_V4_FLASH_UPSTREAM,
                routeable: true,
            },
        };
        let set = materialize_account_routes(
            &[goat_account("goat-1"), go_account("go-1")],
            &config,
            &parsed,
            &resolved,
            COMMAND_CODE_GOAT_DEEPSEEK_V4_FLASH_UPSTREAM,
            COMMAND_CODE_GOAT_DEEPSEEK_V4_FLASH_UPSTREAM,
            &body,
            true,
            &std::collections::HashMap::new(),
        )
        .unwrap();
        assert!(set.routes.is_empty());
        assert!(set.incompatibility.as_deref().is_some_and(|message| {
            message.contains("not verified")
                || message.contains("disabled")
                || message.contains("unsupported")
        }));
    }

    #[test]
    fn goat_without_loopback_is_fail_closed() {
        let config = AppConfig::default();
        let set = routes_for(
            "glm-5.2",
            &[goat_account("goat-1"), go_account("go-1")],
            &config,
            true,
        );
        assert_eq!(set.routes.len(), 1);
        assert_eq!(set.routes[0].routing.account.id, "go-1");
    }

    #[test]
    fn goat_alias_does_not_steal_go_requests_even_with_loopback() {
        let config = AppConfig::default();
        let goat = goat_account("goat-loop-alias");
        let _guard =
            install_goat_loopback_route_for_test(goat.id.clone(), "http://127.0.0.1:9").unwrap();
        let set = routes_for(
            COMMAND_CODE_GOAT_DEEPSEEK_V4_FLASH_ALIAS,
            &[goat, go_account("go-1")],
            &config,
            true,
        );
        assert_eq!(set.routes.len(), 1);
        assert_eq!(set.routes[0].routing.account.id, "go-1");
        assert_eq!(
            set.routes[0].plan.model,
            COMMAND_CODE_GOAT_DEEPSEEK_V4_FLASH_ALIAS
        );
        assert_eq!(set.routes[0].plan.upstream, ApiFormat::ChatCompletions);
    }

    #[test]
    fn goat_slash_raw_pins_through_loopback_as_chat() {
        let config = AppConfig::default();
        let goat = goat_account("goat-loop-raw");
        let _guard =
            install_goat_loopback_route_for_test(goat.id.clone(), "http://127.0.0.1:9").unwrap();
        let set = routes_for(
            COMMAND_CODE_GOAT_DEEPSEEK_V4_FLASH_UPSTREAM,
            &[goat, go_account("go-1")],
            &config,
            true,
        );
        assert_eq!(set.routes.len(), 1);
        assert_eq!(set.routes[0].routing.account.id, "goat-loop-raw");
        assert_eq!(
            set.routes[0].plan.model,
            COMMAND_CODE_GOAT_DEEPSEEK_V4_FLASH_UPSTREAM
        );
        assert_eq!(set.routes[0].plan.upstream, ApiFormat::ChatCompletions);
        assert_eq!(
            set.routes[0].plan.client_model,
            COMMAND_CODE_GOAT_DEEPSEEK_V4_FLASH_UPSTREAM
        );
    }

    #[test]
    fn goat_slash_raw_without_loopback_is_fail_closed() {
        let config = AppConfig::default();
        let set = routes_for(
            COMMAND_CODE_GOAT_DEEPSEEK_V4_FLASH_UPSTREAM,
            &[goat_account("goat-1"), go_account("go-1")],
            &config,
            true,
        );
        assert!(set.routes.is_empty());
        assert!(set.incompatibility.as_deref().is_some_and(|message| {
            message.contains("not verified")
                || message.contains("disabled")
                || message.contains("unsupported")
        }));
    }

    #[test]
    fn resolve_error_exposes_ambiguous_code() {
        let error = protocol_error_from_resolve(crate::alias::ResolveError::Ambiguous {
            requested: "shared-raw".into(),
            mappings: vec![
                crate::alias::ProviderMapping {
                    provider_id: OPENCODE_PROVIDER_ID,
                    offering_id: GO_OFFERING_ID,
                    upstream_model: "shared-raw",
                    routeable: true,
                },
                crate::alias::ProviderMapping {
                    provider_id: OPENCODE_ZEN_FREE_PROVIDER_ID,
                    offering_id: ANONYMOUS_FREE_OFFERING_ID,
                    upstream_model: "shared-raw",
                    routeable: true,
                },
            ],
        });
        assert_eq!(error.code, Some(crate::alias::AMBIGUOUS_MODEL_ID));
        assert!(error.message.contains("alias"));
    }

    #[test]
    fn parse_helpers_are_reexported_for_adapters() {
        let parsed = parse_client(ApiFormat::ChatCompletions, chat_body("glm-5.2")).unwrap();
        assert_eq!(parsed.requested_model, "glm-5.2");
        let gemini = parse_gemini(
            "glm-5.2".into(),
            false,
            Bytes::from(
                serde_json::to_vec(&json!({"contents":[{"role":"user","parts":[{"text":"hi"}]}]}))
                    .unwrap(),
            ),
        )
        .unwrap();
        assert_eq!(gemini.client, ApiFormat::Gemini);
    }

    #[test]
    fn claude_desktop_identity_keeps_client_name_and_mapped_alias() {
        let body = Bytes::from(
            serde_json::to_vec(&json!({
                "model": crate::models::CLAUDE_DESKTOP_OPUS_ALIAS,
                "max_tokens": 1,
                "messages": [{"role": "user", "content": "hi"}]
            }))
            .unwrap(),
        );
        let parsed = parse_client_request(ApiFormat::Messages, body).unwrap();
        let plan = materialize_parsed_request(
            &parsed,
            &MaterializeSpec {
                client_model: parsed.requested_model.clone(),
                upstream_model: "glm-5.2".into(),
                resolved_alias: Some("glm-5.2".into()),
                channel: UpstreamChannel::Go,
                upstream_base_override: None,
                original_model: None,
                allow_go_fallback: false,
                forced_upstream: None,
                custom_route: None,
            },
        )
        .unwrap();
        let identity = native_log_identity(&plan);
        assert_eq!(
            identity.requested_model,
            crate::models::CLAUDE_DESKTOP_OPUS_ALIAS
        );
        assert_eq!(identity.resolved_alias.as_deref(), Some("glm-5.2"));
        assert_eq!(identity.upstream_model, "glm-5.2");
    }

    fn custom_account(id: &str) -> Account {
        account(
            id,
            CUSTOM_PROVIDER_ID,
            CUSTOM_API_OFFERING_ID,
            CredentialKind::ApiKey,
            QuotaScope::Key,
        )
    }

    fn custom_runtime(
        account_id: &str,
        model_id: &str,
        protocol: UpstreamProtocolKind,
    ) -> CustomAccountRuntime {
        CustomAccountRuntime {
            account_id: account_id.into(),
            enabled: true,
            verification_status: ConnectionVerificationStatus::Verified,
            setup_ready: true,
            has_key: true,
            config: AccountCustomConfig {
                account_id: account_id.into(),
                base_url: "http://127.0.0.1:9".into(),
                upstream_protocol: protocol,
                auth_scheme: UpstreamAuthScheme::Bearer,
                created_at: Utc::now(),
                updated_at: Utc::now(),
            },
            capabilities: vec![AccountModelCapability {
                account_id: account_id.into(),
                model_id: model_id.into(),
                protocol,
                verified_at: None,
                source: "manual".into(),
            }],
        }
    }

    #[test]
    fn custom_candidate_diagnostic_passthrough_keeps_client_protocol() {
        let resolved =
            alias::resolve_with_custom("local-custom", &["local-custom".into()]).unwrap();
        assert_eq!(
            diagnostic_forced_upstream(&resolved, ApiFormat::Responses),
            Some(ApiFormat::Responses)
        );
        assert_eq!(
            diagnostic_forced_upstream(&resolved, ApiFormat::Messages),
            Some(ApiFormat::Messages)
        );
        let mixed = alias::resolve_with_custom("hy3", &["hy3".into()]).unwrap();
        assert_eq!(
            diagnostic_forced_upstream(&mixed, ApiFormat::Responses),
            Some(ApiFormat::Responses)
        );
        let builtin = alias::resolve("hy3").unwrap();
        assert_eq!(
            diagnostic_forced_upstream(&builtin, ApiFormat::Responses),
            None
        );
    }

    #[test]
    fn custom_native_responses_structured_format_does_not_guess_chat() {
        let body = Bytes::from(
            serde_json::to_vec(&json!({
                "model": "local-custom",
                "input": "hi",
                "store": false,
                "text": {
                    "format": {
                        "type": "json_schema",
                        "name": "answer",
                        "schema": {"type": "object"}
                    }
                }
            }))
            .unwrap(),
        );
        let parsed = parse_client_request(ApiFormat::Responses, body.clone()).unwrap();
        let resolved =
            alias::resolve_with_custom("local-custom", &["local-custom".into()]).unwrap();
        let account = custom_account("custom-1");
        let runtime = custom_runtime("custom-1", "local-custom", UpstreamProtocolKind::Responses);
        let mut runtimes = std::collections::HashMap::new();
        runtimes.insert(account.id.clone(), runtime);
        let set = materialize_account_routes(
            &[account],
            &AppConfig::default(),
            &parsed,
            &resolved,
            &parsed.requested_model,
            "local-custom",
            &body,
            false,
            &runtimes,
        )
        .expect("native Responses structured output must not be rejected via Chat conversion");
        assert_eq!(set.routes.len(), 1);
        assert_eq!(set.routes[0].plan.upstream, ApiFormat::Responses);
        assert_eq!(set.routes[0].plan.client, ApiFormat::Responses);
        let upstream: serde_json::Value = serde_json::from_slice(&set.routes[0].plan.body).unwrap();
        assert_eq!(upstream["text"]["format"]["type"], "json_schema");
    }

    #[test]
    fn custom_native_messages_structured_format_does_not_guess_chat() {
        let body = Bytes::from(
            serde_json::to_vec(&json!({
                "model": "local-custom",
                "max_tokens": 16,
                "messages": [{"role": "user", "content": "hi"}],
                "output_config": {
                    "format": {"type": "json_schema", "schema": {"type": "object"}}
                }
            }))
            .unwrap(),
        );
        let parsed = parse_client_request(ApiFormat::Messages, body.clone()).unwrap();
        let resolved =
            alias::resolve_with_custom("local-custom", &["local-custom".into()]).unwrap();
        let account = custom_account("custom-1");
        let runtime = custom_runtime("custom-1", "local-custom", UpstreamProtocolKind::Messages);
        let mut runtimes = std::collections::HashMap::new();
        runtimes.insert(account.id.clone(), runtime);
        let set = materialize_account_routes(
            &[account],
            &AppConfig::default(),
            &parsed,
            &resolved,
            &parsed.requested_model,
            "local-custom",
            &body,
            false,
            &runtimes,
        )
        .expect("native Messages structured output must not be rejected via Chat conversion");
        assert_eq!(set.routes.len(), 1);
        assert_eq!(set.routes[0].plan.upstream, ApiFormat::Messages);
        let upstream: serde_json::Value = serde_json::from_slice(&set.routes[0].plan.body).unwrap();
        assert_eq!(upstream["output_config"]["format"]["type"], "json_schema");
    }
}
