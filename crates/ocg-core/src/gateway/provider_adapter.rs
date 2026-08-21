//! Provider offering adapters: endpoint, auth, and capability checks.
//!
//! Authentication belongs to the provider/offering, not the wire protocol.
//! [`resolve_route`] is the seam later GOAT / SCNet / Custom adapters should
//! implement. Alias and PinnedRaw candidates both materialize a [`RequestPlan`]
//! then call this seam. They must not probe a billable inference path to
//! discover protocol support.
//!
//! Production Command Code GOAT, SCNet, and Custom stay fail-closed here.
//! The GOAT loopback helper exists only for gateway integration tests.

use crate::gateway::free_models::{is_free_model, resolve_upstream_base};
use crate::gateway::protocol::{ApiFormat, RequestPlan, opencode_supports_upstream};
use crate::models::{Account, AppConfig, UpstreamChannel};
use crate::pricing::normalize_model_name;
use crate::provider::{
    ANONYMOUS_FREE_OFFERING_ID, COMMAND_CODE_PROVIDER_ID, CredentialKind, GO_OFFERING_ID,
    GOAT_OFFERING_ID, OPENCODE_PROVIDER_ID, OPENCODE_ZEN_FREE_PROVIDER_ID, QuotaScope,
    ZEN_FREE_ACCOUNT_ID,
};
use std::collections::{HashMap, HashSet};
use std::sync::{LazyLock, RwLock};

/// Authentication belongs to the provider/offering adapter, not to the wire
/// protocol. In particular, a Messages endpoint does not imply `x-api-key`
/// for every future provider.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum UpstreamAuth {
    OpenCodeProtocolDefault,
    Bearer,
    XApiKey,
    None,
}

#[derive(Debug, Clone)]
pub(crate) struct ResolvedProviderRoute {
    pub base_url: String,
    pub upstream: ApiFormat,
    pub auth: UpstreamAuth,
    pub credential_account_id: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoopbackTestAuth {
    Bearer,
    XApiKey,
}

#[derive(Debug, Clone)]
struct GoatLoopbackRoute {
    base_url: String,
    models: HashSet<String>,
    protocols: HashSet<ApiFormat>,
    auth: LoopbackTestAuth,
}

static GOAT_LOOPBACK_ROUTES: LazyLock<RwLock<HashMap<String, GoatLoopbackRoute>>> =
    LazyLock::new(|| RwLock::new(HashMap::new()));

/// RAII guard for the integration-only GOAT seam. The production adapter has
/// no endpoint or protocol guesses: without a live guard, GOAT is unsupported.
#[doc(hidden)]
pub struct GoatLoopbackRouteGuard {
    account_id: String,
    base_url: String,
}

impl Drop for GoatLoopbackRouteGuard {
    fn drop(&mut self) {
        if let Ok(mut routes) = GOAT_LOOPBACK_ROUTES.write()
            && routes
                .get(&self.account_id)
                .is_some_and(|route| route.base_url == self.base_url)
        {
            routes.remove(&self.account_id);
        }
    }
}

/// Installs a loopback-only fake route used by gateway integration tests.
/// This deliberately cannot configure a remote production endpoint.
#[doc(hidden)]
pub fn install_goat_loopback_route_for_test(
    account_id: impl Into<String>,
    base_url: impl Into<String>,
    models: impl IntoIterator<Item = impl AsRef<str>>,
    protocols: impl IntoIterator<Item = ApiFormat>,
    auth: LoopbackTestAuth,
) -> Result<GoatLoopbackRouteGuard, String> {
    let account_id = account_id.into();
    let base_url = base_url.into();
    ensure_loopback_base(&base_url)?;
    let route = GoatLoopbackRoute {
        base_url: base_url.trim_end_matches('/').to_string(),
        models: models
            .into_iter()
            .map(|model| normalize_model_name(model.as_ref()).to_string())
            .collect(),
        protocols: protocols.into_iter().collect(),
        auth,
    };
    if route.models.is_empty() || route.protocols.is_empty() {
        return Err("GOAT loopback test route requires models and protocols".to_string());
    }
    let guard = GoatLoopbackRouteGuard {
        account_id: account_id.clone(),
        base_url: route.base_url.clone(),
    };
    GOAT_LOOPBACK_ROUTES
        .write()
        .map_err(|_| "GOAT loopback route lock is poisoned".to_string())?
        .insert(account_id, route);
    Ok(guard)
}

pub(crate) fn supports_plan(
    account: &Account,
    config: &AppConfig,
    plan: &RequestPlan,
) -> Result<(), String> {
    resolve_route(account, config, plan).map(|_| ())
}

pub(crate) fn resolve_route(
    account: &Account,
    config: &AppConfig,
    plan: &RequestPlan,
) -> Result<ResolvedProviderRoute, String> {
    match (account.provider_id.as_str(), account.offering_id.as_str()) {
        (OPENCODE_PROVIDER_ID, GO_OFFERING_ID) => {
            require_binding(account, CredentialKind::ApiKey, QuotaScope::Key)?;
            if plan.channel != UpstreamChannel::Go {
                return Err("OpenCode Go does not serve the Zen free channel".to_string());
            }
            if !opencode_supports_upstream(&plan.model, plan.upstream) {
                return Err(format!(
                    "OpenCode Go has no verified support for model `{}` over {:?}",
                    plan.model, plan.upstream
                ));
            }
            Ok(ResolvedProviderRoute {
                base_url: config.upstream_base_url.trim_end_matches('/').to_string(),
                upstream: plan.upstream,
                auth: UpstreamAuth::OpenCodeProtocolDefault,
                credential_account_id: Some(account.id.clone()),
            })
        }
        (OPENCODE_ZEN_FREE_PROVIDER_ID, ANONYMOUS_FREE_OFFERING_ID) => {
            require_binding(account, CredentialKind::None, QuotaScope::EgressIp)?;
            if account.id != ZEN_FREE_ACCOUNT_ID {
                return Err("Zen Free route must use the reserved singleton account".to_string());
            }
            if plan.channel != UpstreamChannel::Free || !is_free_model(&plan.model) {
                return Err(format!(
                    "Zen Free does not support routed model `{}` on this channel",
                    plan.model
                ));
            }
            if !opencode_supports_upstream(&plan.model, plan.upstream) {
                return Err(format!(
                    "Zen Free has no verified support for model `{}` over {:?}",
                    plan.model, plan.upstream
                ));
            }
            let base_url = plan.upstream_base_override.clone().map_or_else(
                || resolve_upstream_base(UpstreamChannel::Free, &config.upstream_base_url),
                Ok,
            )?;
            Ok(ResolvedProviderRoute {
                base_url,
                upstream: plan.upstream,
                auth: UpstreamAuth::None,
                credential_account_id: None,
            })
        }
        (COMMAND_CODE_PROVIDER_ID, GOAT_OFFERING_ID) => {
            require_binding(account, CredentialKind::ApiKey, QuotaScope::Key)?;
            if plan.channel != UpstreamChannel::Go {
                return Err("Command Code GOAT does not serve the Zen free channel".to_string());
            }
            let routes = GOAT_LOOPBACK_ROUTES
                .read()
                .map_err(|_| "GOAT loopback route lock is poisoned".to_string())?;
            let route = routes.get(&account.id).ok_or_else(|| {
                "Command Code GOAT production inference endpoint, auth, protocol, and model catalog are not verified; route is disabled"
                    .to_string()
            })?;
            let normalized = normalize_model_name(&plan.model);
            if !route.models.contains(&normalized) || !route.protocols.contains(&plan.upstream) {
                return Err(format!(
                    "Command Code GOAT test route does not support model `{}` over {:?}",
                    plan.model, plan.upstream
                ));
            }
            Ok(ResolvedProviderRoute {
                base_url: route.base_url.clone(),
                upstream: plan.upstream,
                auth: match route.auth {
                    LoopbackTestAuth::Bearer => UpstreamAuth::Bearer,
                    LoopbackTestAuth::XApiKey => UpstreamAuth::XApiKey,
                },
                credential_account_id: Some(account.id.clone()),
            })
        }
        _ => Err(format!(
            "unsupported provider offering `{}/{}`",
            account.provider_id, account.offering_id
        )),
    }
}

pub(crate) fn supports_model_discovery(account: &Account) -> bool {
    account.provider_id == OPENCODE_PROVIDER_ID
        && account.offering_id == GO_OFFERING_ID
        && account.credential_kind == CredentialKind::ApiKey
        && account.quota_scope == QuotaScope::Key
}

fn require_binding(
    account: &Account,
    credential_kind: CredentialKind,
    quota_scope: QuotaScope,
) -> Result<(), String> {
    if account.credential_kind != credential_kind || account.quota_scope != quota_scope {
        return Err(format!(
            "provider binding mismatch for account `{}`",
            account.id
        ));
    }
    Ok(())
}

fn ensure_loopback_base(base_url: &str) -> Result<(), String> {
    let url = reqwest::Url::parse(base_url).map_err(|error| error.to_string())?;
    if url.scheme() != "http"
        || !matches!(
            url.host_str(),
            Some("localhost") | Some("127.0.0.1") | Some("::1") | Some("[::1]")
        )
    {
        return Err("GOAT test route must be an HTTP loopback URL".to_string());
    }
    Ok(())
}
