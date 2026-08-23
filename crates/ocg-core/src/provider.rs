//! Provider catalog compatibility facade plus Custom URL inspection and
//! persistence-shaped quota, credit, pricing, and usage-sync records.
//!
//! Pure catalog, adapter, registry, and binding types live in
//! [`ocg_domain::provider`] and are re-exported here item-by-item so
//! `ocg_core::provider::*` paths stay stable.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

pub use crate::kernel::catalog::{
    CatalogParseError, CredentialKind, QuotaScope, UpstreamAuthScheme, UpstreamProtocolKind,
};
pub use crate::kernel::ids::{
    ANONYMOUS_FREE_OFFERING_ID, COMMAND_CODE_GOAT_DEEPSEEK_V4_FLASH_ALIAS,
    COMMAND_CODE_GOAT_DEEPSEEK_V4_FLASH_UPSTREAM, COMMAND_CODE_PROVIDER_ID, CUSTOM_API_OFFERING_ID,
    CUSTOM_PROVIDER_ID, GO_OFFERING_ID, GOAT_OFFERING_ID, OPENCODE_PROVIDER_ID,
    OPENCODE_ZEN_FREE_PROVIDER_ID, SCNET_PROVIDER_ID, SCNET_TOKEN_PLAN_BASIC_OFFERING_ID,
    SCNET_TOKEN_PLAN_OFFERING_IDS, SCNET_TOKEN_PLAN_PREMIUM_OFFERING_ID,
    SCNET_TOKEN_PLAN_STANDARD_OFFERING_ID, ZEN_FREE_ACCOUNT_ID, ZEN_FREE_ACCOUNT_NAME,
};

pub use ocg_domain::provider::{
    BUILTIN_OFFERINGS, BUILTIN_PLANS, BuiltinOffering, BuiltinPlan, COMMAND_CODE_GOAT_BASE_URL,
    COMMAND_CODE_GOAT_CHAT_COMPLETIONS_PATH, COMMAND_CODE_GOAT_HOST,
    COMMAND_CODE_GOAT_MESSAGES_PATH, COMMAND_CODE_GOAT_MODELS_PATH, COMMAND_CODE_GOAT_QUOTA_5H,
    COMMAND_CODE_GOAT_QUOTA_MONTH, COMMAND_CODE_GOAT_QUOTA_WEEK, CardActionsDescriptor,
    CardCapabilities, CardVerifyAction, CommandCodeGoatAdapter, ConfigurableHttpAdapter,
    ConnectionVerificationStatus, CreationAvailability, ErrorCooldownDescriptor,
    ErrorPolicyAdapter, InferenceAdapter, InferenceAuthDescriptor, InferenceChannelKind,
    InferenceOriginKind, InferenceRoutingDescriptor, ModelCatalogAdapter, ModelCatalogDescriptor,
    ModelCatalogKind, OPENCODE_CONSTRUCTABLE_PROTOCOLS, OpenCodeGoAdapter,
    PROTOCOL_FALLBACK_CHAT_MESSAGES, PROTOCOL_FALLBACK_CHAT_RESPONSES_MESSAGES, PlanFormField,
    PlanRiskNotice, PricingAdapter, PricingDescriptor, ProtocolMatrixKind, ProtocolProbeAdapter,
    ProtocolProbeDescriptor, ProviderAdapterKind, ProviderBindingError, ProviderCapabilities,
    ProviderDescriptor, ProviderRegistry, QUOTA_WINDOW_FIVE_HOURS, QUOTA_WINDOW_FREE,
    QUOTA_WINDOW_MONTH, QUOTA_WINDOW_WEEK, SCNET_RISK_ACKNOWLEDGEMENT_BODY,
    SCNET_RISK_ACKNOWLEDGEMENT_CONTENT_HASH, SCNET_RISK_ACKNOWLEDGEMENT_ID,
    SCNET_RISK_ACKNOWLEDGEMENT_SOURCE_URL, SCNET_RISK_ACKNOWLEDGEMENT_VERSION,
    SCNET_TOKEN_PLAN_ANTHROPIC_BASE_URL, SCNET_TOKEN_PLAN_CHAT_COMPLETIONS_PATH,
    SCNET_TOKEN_PLAN_DOCUMENTED_ENDPOINTS, SCNET_TOKEN_PLAN_ENDPOINT_SOURCE_URL,
    SCNET_TOKEN_PLAN_EXCLUDED_PRICING_TABLE_OR_FAQ_MODELS, SCNET_TOKEN_PLAN_KEY_PREFIX,
    SCNET_TOKEN_PLAN_MESSAGES_PATH, SCNET_TOKEN_PLAN_MODEL_SNAPSHOT,
    SCNET_TOKEN_PLAN_MODEL_SNAPSHOT_VERSION, SCNET_TOKEN_PLAN_MODEL_SOURCE,
    SCNET_TOKEN_PLAN_MODEL_SOURCE_URL, SCNET_TOKEN_PLAN_OFFICIAL_BASIC_NAME,
    SCNET_TOKEN_PLAN_OFFICIAL_PREMIUM_NAME, SCNET_TOKEN_PLAN_OFFICIAL_STANDARD_NAME,
    SCNET_TOKEN_PLAN_OPENAI_BASE_URL, SCNET_TOKEN_PLAN_USABLE_MODELS,
    SCNET_TOKEN_PLAN_USAGE_RESTRICTIONS, ScnetAdapter, ScnetTokenPlanDocumentedEndpoints,
    ScnetTokenPlanModelSnapshot, ScnetTokenPlanUsageRestrictions, StructuralProbeCeiling,
    UsageAdapter, UsageContractKind, UsageDescriptor, VerificationAdapter, VerificationDescriptor,
    VerificationPolicy, ZenFreeAdapter, acknowledgement_content_hash, builtin_offering,
    builtin_plan, custom_endpoint_relative_path, default_credential_kind, default_offering_id,
    default_provider_id, default_quota_scope, default_verification_status,
    ensure_enabled_offering_is_routable, ensure_offering_can_enable, is_command_code_goat,
    is_custom_api, is_scnet_token_plan, offering_allows_enablement, plan_allows_enablement,
    plan_requires_custom_config, scnet_token_plan_model_snapshot,
    scnet_token_plan_official_offering_name, validate_account_binding, validate_custom_model_id,
    validate_plan_key,
};

/// Structured Custom URL host taken from [`reqwest::Url::host`], not `host_str`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CustomUrlHost {
    Ip(IpAddr),
    Domain(String),
}

/// Syntactic Custom URL inspection shared by persistence and HTTP joining.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CustomUrlTarget {
    pub host: CustomUrlHost,
}

/// Syntactic Custom base-URL gate. Administrators explicitly trust Custom
/// destinations, so any http/https origin is accepted. Credentials and
/// non-HTTP(S) schemes stay rejected; DNS / IP / hostname policy is not applied.
pub fn validate_custom_base_url(value: &str) -> Result<String, ProviderBindingError> {
    let value = value.trim();
    if value.is_empty() {
        return Err(ProviderBindingError::InvalidCustomBaseUrl(
            "base URL is required".to_string(),
        ));
    }
    if value.len() > 2048 {
        return Err(ProviderBindingError::InvalidCustomBaseUrl(
            "base URL is too long".to_string(),
        ));
    }
    let parsed = reqwest::Url::parse(value).map_err(|error| {
        ProviderBindingError::InvalidCustomBaseUrl(format!("invalid base URL: {error}"))
    })?;
    inspect_custom_url(&parsed)?;
    if parsed.query().is_some() || parsed.fragment().is_some() {
        return Err(ProviderBindingError::InvalidCustomBaseUrl(
            "base URL must not include a query or fragment".to_string(),
        ));
    }
    Ok(parsed.as_str().trim_end_matches('/').to_string())
}

/// Inspect scheme, credentials, and host of a Custom URL.
///
/// Uses [`reqwest::Url::host`] so bracketed IPv6 and IPv4-mapped literals are
/// the parser's IP variants. `host_str().parse::<IpAddr>()` treats `[::ffff:…]`
/// as a hostname and is the bypass this function exists to close.
pub fn inspect_custom_url(parsed: &reqwest::Url) -> Result<CustomUrlTarget, ProviderBindingError> {
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err(ProviderBindingError::InvalidCustomBaseUrl(
            "base URL must use http or https".to_string(),
        ));
    }
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return Err(ProviderBindingError::InvalidCustomBaseUrl(
            "base URL must not include credentials".to_string(),
        ));
    }
    Ok(CustomUrlTarget {
        host: custom_url_host(parsed)?,
    })
}

fn custom_url_host(parsed: &reqwest::Url) -> Result<CustomUrlHost, ProviderBindingError> {
    let host = parsed.host().ok_or_else(|| {
        ProviderBindingError::InvalidCustomBaseUrl("base URL must include a host".to_string())
    })?;
    // `url::Host` is not a direct dependency (manifests stay frozen). IPv6
    // Display includes brackets; strip them to recover the parsed `Ipv6Addr`.
    let rendered = host.to_string();
    if let Some(inside) = rendered
        .strip_prefix('[')
        .and_then(|value| value.strip_suffix(']'))
    {
        let ip = inside.parse::<Ipv6Addr>().map_err(|_| {
            ProviderBindingError::InvalidCustomBaseUrl("base URL IPv6 host is invalid".to_string())
        })?;
        return Ok(CustomUrlHost::Ip(IpAddr::V6(ip)));
    }
    if let Ok(ip) = rendered.parse::<Ipv4Addr>() {
        return Ok(CustomUrlHost::Ip(IpAddr::V4(ip)));
    }
    Ok(CustomUrlHost::Domain(rendered.to_ascii_lowercase()))
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QuotaWindow {
    pub account_id: String,
    pub window_kind: String,
    pub used: f64,
    pub limit_value: Option<f64>,
    pub started_at: Option<DateTime<Utc>>,
    pub resets_at: Option<DateTime<Utc>>,
    pub calibration_offset: f64,
    pub unit: String,
    pub source: String,
    pub observed_at: Option<DateTime<Utc>>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CreditBalance {
    pub account_id: String,
    pub balance_kind: String,
    pub amount: f64,
    pub unit: String,
    pub source: String,
    pub observed_at: Option<DateTime<Utc>>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderPricingSnapshot {
    pub provider_id: String,
    pub offering_id: String,
    pub revision: String,
    pub activated_at: String,
    pub document_updated_at: Option<String>,
    pub source_url: String,
    pub content_hash: String,
    pub snapshot_json: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderUsageSyncState {
    pub account_id: String,
    pub last_success_at: Option<DateTime<Utc>>,
    pub last_attempt_at: Option<DateTime<Utc>>,
    pub next_eligible_at: Option<DateTime<Utc>>,
    pub failure_streak: i64,
    pub last_expedited_at: Option<DateTime<Utc>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_parse_errors_map_to_provider_binding_errors() {
        assert!(matches!(
            ProviderBindingError::from(CredentialKind::try_from("cookie").unwrap_err()),
            ProviderBindingError::UnknownCredentialKind(value) if value == "cookie"
        ));
        assert!(matches!(
            ProviderBindingError::from(QuotaScope::try_from("account").unwrap_err()),
            ProviderBindingError::UnknownQuotaScope(value) if value == "account"
        ));
        assert!(matches!(
            ProviderBindingError::from(UpstreamProtocolKind::try_from("gemini").unwrap_err()),
            ProviderBindingError::UnknownUpstreamProtocol(value) if value == "gemini"
        ));
        assert!(matches!(
            ProviderBindingError::from(UpstreamAuthScheme::try_from("basic").unwrap_err()),
            ProviderBindingError::UnknownAuthScheme(value) if value == "basic"
        ));
    }

    #[test]
    fn builtin_pairs_derive_credential_and_quota_scope() {
        let goat = builtin_offering(COMMAND_CODE_PROVIDER_ID, GOAT_OFFERING_ID).unwrap();
        assert_eq!(goat.credential_kind, CredentialKind::ApiKey);
        assert_eq!(goat.quota_scope, QuotaScope::Key);

        let free =
            builtin_offering(OPENCODE_ZEN_FREE_PROVIDER_ID, ANONYMOUS_FREE_OFFERING_ID).unwrap();
        assert_eq!(free.credential_kind, CredentialKind::None);
        assert_eq!(free.quota_scope, QuotaScope::EgressIp);
        assert_eq!(free.singleton_account_id, Some(ZEN_FREE_ACCOUNT_ID));
    }

    #[test]
    fn singleton_and_pair_validation_is_fail_closed() {
        assert!(
            validate_account_binding(
                "account-1",
                OPENCODE_PROVIDER_ID,
                GO_OFFERING_ID,
                CredentialKind::ApiKey,
                QuotaScope::Key,
            )
            .is_ok()
        );
        assert!(
            validate_account_binding(
                "account-1",
                OPENCODE_ZEN_FREE_PROVIDER_ID,
                ANONYMOUS_FREE_OFFERING_ID,
                CredentialKind::None,
                QuotaScope::EgressIp,
            )
            .is_err()
        );
        assert!(
            validate_account_binding(
                ZEN_FREE_ACCOUNT_ID,
                OPENCODE_PROVIDER_ID,
                GO_OFFERING_ID,
                CredentialKind::ApiKey,
                QuotaScope::Key,
            )
            .is_err()
        );
        assert!(
            validate_account_binding(
                "account-1",
                "unknown-provider",
                "unknown-offering",
                CredentialKind::ApiKey,
                QuotaScope::Key,
            )
            .is_err()
        );
    }

    #[test]
    fn catalog_hardcodes_plans_and_keeps_unverified_offerings_unroutable() {
        assert_eq!(BUILTIN_PLANS.len(), 7);
        let goat = builtin_plan(COMMAND_CODE_PROVIDER_ID, GOAT_OFFERING_ID).unwrap();
        assert!(!goat.routable);
        assert_eq!(goat.verification_policy, VerificationPolicy::Required);
        assert_eq!(goat.verification_runtime_availability, "unavailable");
        assert_eq!(goat.creation_availability, CreationAvailability::Available);
        assert_eq!(goat.pricing_availability, "unavailable");
        assert_eq!(goat.usage_availability, "unavailable");
        assert_eq!(goat.auth_schemes, &[UpstreamAuthScheme::Bearer]);
        assert_eq!(
            goat.upstream_protocols,
            &[
                UpstreamProtocolKind::ChatCompletions,
                UpstreamProtocolKind::Messages,
            ]
        );
        assert!(
            !goat
                .upstream_protocols
                .contains(&UpstreamProtocolKind::Responses)
        );
        assert_eq!(goat.model_source, "builtin_command_code_protocol_table");
        assert_eq!(
            COMMAND_CODE_GOAT_BASE_URL,
            "https://api.commandcode.ai/provider/v1"
        );
        assert_eq!(COMMAND_CODE_GOAT_HOST, "api.commandcode.ai");
        assert_eq!(COMMAND_CODE_GOAT_CHAT_COMPLETIONS_PATH, "/chat/completions");
        assert_eq!(COMMAND_CODE_GOAT_MESSAGES_PATH, "/messages");
        assert_eq!(COMMAND_CODE_GOAT_MODELS_PATH, "/models");
        assert_eq!(
            COMMAND_CODE_GOAT_DEEPSEEK_V4_FLASH_UPSTREAM,
            "deepseek/deepseek-v4-flash"
        );
        assert!(is_command_code_goat(
            COMMAND_CODE_PROVIDER_ID,
            GOAT_OFFERING_ID
        ));
        assert!(!is_command_code_goat(OPENCODE_PROVIDER_ID, GO_OFFERING_ID));

        let basic = builtin_plan(SCNET_PROVIDER_ID, SCNET_TOKEN_PLAN_BASIC_OFFERING_ID).unwrap();
        assert!(!basic.routable);
        assert_eq!(
            basic.risk_notice.unwrap().acknowledgement_id,
            SCNET_RISK_ACKNOWLEDGEMENT_ID
        );
        assert!(
            validate_plan_key(basic, "sk-live-not-token")
                .unwrap_err()
                .to_string()
                .contains(SCNET_TOKEN_PLAN_KEY_PREFIX)
        );
        assert!(validate_plan_key(basic, "sk-tp-live").is_ok());

        let custom = builtin_plan(CUSTOM_PROVIDER_ID, CUSTOM_API_OFFERING_ID).unwrap();
        assert!(custom.routable);
        assert_eq!(custom.verification_runtime_availability, "available");
        assert_eq!(custom.verification_policy, VerificationPolicy::Required);
        assert_eq!(custom.pricing_availability, "unpriced");
        assert_eq!(custom.usage_availability, "unavailable");
        assert!(plan_requires_custom_config(custom));
        assert!(is_custom_api(CUSTOM_PROVIDER_ID, CUSTOM_API_OFFERING_ID));
        assert!(!is_custom_api(OPENCODE_PROVIDER_ID, GO_OFFERING_ID));

        let go = builtin_plan(OPENCODE_PROVIDER_ID, GO_OFFERING_ID).unwrap();
        assert!(go.routable);
        assert_eq!(
            default_verification_status(go),
            ConnectionVerificationStatus::NotRequired
        );
    }

    #[test]
    fn catalog_enablement_gate_is_fail_closed_for_unroutable_plans() {
        for plan in BUILTIN_PLANS {
            let provider_id = plan.offering.provider_id;
            let offering_id = plan.offering.offering_id;
            assert_eq!(
                plan_allows_enablement(plan),
                plan.routable,
                "{provider_id}/{offering_id}"
            );
            assert_eq!(
                offering_allows_enablement(provider_id, offering_id),
                plan.routable,
                "{provider_id}/{offering_id}"
            );
            assert!(
                ensure_enabled_offering_is_routable(provider_id, offering_id, false).is_ok(),
                "disabled drafts must stay writable: {provider_id}/{offering_id}"
            );
            let enabled = ensure_enabled_offering_is_routable(provider_id, offering_id, true);
            if plan.routable {
                enabled.expect("routable offerings may enable");
                ensure_offering_can_enable(provider_id, offering_id).unwrap();
            } else {
                let error = enabled.expect_err("unroutable offerings must reject enabled=true");
                assert!(
                    matches!(
                        error,
                        ProviderBindingError::EnablementNotRoutable {
                            provider_id: rejected_provider,
                            offering_id: rejected_offering,
                            display_name,
                        } if rejected_provider == provider_id
                            && rejected_offering == offering_id
                            && display_name == plan.display_name
                    ),
                    "{error:?}"
                );
                assert!(error.to_string().contains("not routable"), "{}", error);
            }
        }
        assert!(!offering_allows_enablement(
            "unknown-provider",
            "unknown-offering"
        ));
        assert!(matches!(
            ensure_offering_can_enable("unknown-provider", "unknown-offering"),
            Err(ProviderBindingError::UnknownOffering { .. })
        ));
        let zen = builtin_plan(OPENCODE_ZEN_FREE_PROVIDER_ID, ANONYMOUS_FREE_OFFERING_ID).unwrap();
        assert!(plan_allows_enablement(zen));
        let go = builtin_plan(OPENCODE_PROVIDER_ID, GO_OFFERING_ID).unwrap();
        assert!(plan_allows_enablement(go));
    }

    #[test]
    fn custom_base_url_trusts_administrator_http_origins_and_rejects_credentials() {
        assert!(validate_custom_base_url("https://api.example.com/v1").is_ok());
        assert!(validate_custom_base_url("http://127.0.0.1:8080/v1").is_ok());
        assert!(validate_custom_base_url("http://localhost:3000").is_ok());
        assert!(validate_custom_base_url("http://app.localhost/v1").is_ok());
        assert!(validate_custom_base_url("http://api.example.com/v1").is_ok());
        assert!(validate_custom_base_url("https://192.168.1.8/v1").is_ok());
        assert!(validate_custom_base_url("http://10.0.0.1:9000/v1").is_ok());
        assert!(validate_custom_base_url("https://169.254.169.254/latest").is_ok());
        assert!(validate_custom_base_url("http://metadata.google.internal/").is_ok());
        assert!(validate_custom_base_url("https://[::ffff:169.254.169.254]/").is_ok());
        assert!(validate_custom_base_url("https://[2001:db8::1]/v1").is_ok());
        assert!(validate_custom_base_url("https://user:pass@api.example.com").is_err());
        assert!(validate_custom_base_url("https://api.example.com/v1?x=1").is_err());
        assert!(validate_custom_base_url("https://api.example.com/v1#frag").is_err());
        assert!(validate_custom_base_url("javascript:alert(1)").is_err());
        assert!(validate_custom_base_url("ftp://api.example.com/v1").is_err());
        assert_eq!(
            validate_custom_model_id("deepseek/deepseek-v4-flash").unwrap(),
            "deepseek/deepseek-v4-flash"
        );
        assert!(validate_custom_model_id("").is_err());
        assert_eq!(
            custom_endpoint_relative_path(UpstreamProtocolKind::ChatCompletions),
            "chat/completions"
        );
        assert_eq!(
            custom_endpoint_relative_path(UpstreamProtocolKind::Responses),
            "responses"
        );
        assert_eq!(
            custom_endpoint_relative_path(UpstreamProtocolKind::Messages),
            "messages"
        );
    }

    #[test]
    fn custom_url_host_uses_url_host_not_bracketed_host_str() {
        assert!(validate_custom_base_url("http://[::ffff:127.0.0.1]/v1").is_ok());
        assert!(validate_custom_base_url("http://[::1]/v1").is_ok());
        let mapped_loopback = validate_custom_base_url("http://[::ffff:127.0.0.1]/v1").unwrap();
        let parsed = reqwest::Url::parse(&mapped_loopback).unwrap();
        match inspect_custom_url(&parsed).unwrap().host {
            CustomUrlHost::Ip(ip) => {
                assert_eq!(ip, "::ffff:127.0.0.1".parse::<IpAddr>().unwrap());
            }
            CustomUrlHost::Domain(domain) => {
                panic!("mapped loopback must stay an IP host, got {domain}")
            }
        }
        let metadata = validate_custom_base_url("https://[::ffff:169.254.169.254]/latest").unwrap();
        let parsed = reqwest::Url::parse(&metadata).unwrap();
        match inspect_custom_url(&parsed).unwrap().host {
            CustomUrlHost::Ip(_) => {}
            CustomUrlHost::Domain(domain) => {
                panic!("mapped metadata IP must stay an IP host, got {domain}")
            }
        }
    }

    #[test]
    fn custom_base_url_normalizes_decimal_loopback_literals() {
        assert_eq!(
            validate_custom_base_url("http://127.1:8080/v1").unwrap(),
            "http://127.0.0.1:8080/v1"
        );
        assert_eq!(
            validate_custom_base_url("http://127.0.1/v1").unwrap(),
            "http://127.0.0.1/v1"
        );
        let parsed = reqwest::Url::parse("http://127.1/v1").unwrap();
        match inspect_custom_url(&parsed).unwrap().host {
            CustomUrlHost::Ip(ip) => assert_eq!(ip, "127.0.0.1".parse::<IpAddr>().unwrap()),
            CustomUrlHost::Domain(domain) => panic!("127.1 must not stay a domain: {domain}"),
        }
    }

    #[test]
    fn scnet_acknowledgement_hash_is_stable() {
        let notice = builtin_plan(SCNET_PROVIDER_ID, SCNET_TOKEN_PLAN_BASIC_OFFERING_ID)
            .unwrap()
            .risk_notice
            .unwrap();
        assert_eq!(notice.acknowledgement_id, SCNET_RISK_ACKNOWLEDGEMENT_ID);
        assert_eq!(notice.version, SCNET_RISK_ACKNOWLEDGEMENT_VERSION);
        assert_eq!(notice.source_url, SCNET_RISK_ACKNOWLEDGEMENT_SOURCE_URL);
        assert_eq!(notice.body, SCNET_RISK_ACKNOWLEDGEMENT_BODY);
        assert_eq!(
            notice.content_hash(),
            SCNET_RISK_ACKNOWLEDGEMENT_CONTENT_HASH
        );
        assert_eq!(
            notice.content_hash(),
            acknowledgement_content_hash(SCNET_RISK_ACKNOWLEDGEMENT_BODY)
        );
        assert_ne!(notice.content_hash(), acknowledgement_content_hash("other"));
    }

    #[test]
    fn scnet_token_plans_share_official_usable_model_snapshot() {
        assert_eq!(
            SCNET_TOKEN_PLAN_USABLE_MODELS,
            [
                "GLM-5.2",
                "GLM-5",
                "GLM-5.1",
                "Kimi-K3",
                "Kimi-K2.7-Code",
                "Kimi-K2.6",
                "Kimi-K2.5",
                "DeepSeek-V4-Flash",
                "DeepSeek-V3.2",
                "MiniMax-M3",
                "MiniMax-M2.7",
                "MiniMax-M2.5",
                "MiMo-V2.5-Pro",
            ]
        );
        let expected = SCNET_TOKEN_PLAN_MODEL_SNAPSHOT;
        let mut previous: Option<BuiltinPlan> = None;
        for offering_id in SCNET_TOKEN_PLAN_OFFERING_IDS {
            let plan = builtin_plan(SCNET_PROVIDER_ID, offering_id).unwrap();
            assert!(is_scnet_token_plan(
                plan.offering.provider_id,
                plan.offering.offering_id
            ));
            assert_eq!(plan.model_source, SCNET_TOKEN_PLAN_MODEL_SOURCE);
            assert_eq!(
                scnet_token_plan_model_snapshot(
                    plan.offering.provider_id,
                    plan.offering.offering_id
                ),
                Some(expected)
            );
            assert!(std::ptr::eq(
                expected.upstream_models,
                SCNET_TOKEN_PLAN_USABLE_MODELS
            ));
            if let Some(previous) = previous {
                assert_eq!(previous.model_source, plan.model_source);
                assert_eq!(
                    scnet_token_plan_model_snapshot(
                        previous.offering.provider_id,
                        previous.offering.offering_id
                    )
                    .unwrap()
                    .upstream_models,
                    expected.upstream_models
                );
            }
            previous = Some(plan);
        }
        assert_eq!(
            scnet_token_plan_official_offering_name(SCNET_TOKEN_PLAN_BASIC_OFFERING_ID),
            Some(SCNET_TOKEN_PLAN_OFFICIAL_BASIC_NAME)
        );
        assert_eq!(
            scnet_token_plan_official_offering_name(SCNET_TOKEN_PLAN_STANDARD_OFFERING_ID),
            Some(SCNET_TOKEN_PLAN_OFFICIAL_STANDARD_NAME)
        );
        assert_eq!(
            scnet_token_plan_official_offering_name(SCNET_TOKEN_PLAN_PREMIUM_OFFERING_ID),
            Some(SCNET_TOKEN_PLAN_OFFICIAL_PREMIUM_NAME)
        );
    }

    #[test]
    fn scnet_token_plan_excludes_pricing_table_and_faq_extras() {
        for extra in SCNET_TOKEN_PLAN_EXCLUDED_PRICING_TABLE_OR_FAQ_MODELS {
            assert!(
                !SCNET_TOKEN_PLAN_USABLE_MODELS.contains(extra),
                "{extra} is a pricing-table/FAQ extra and must not enter the usable snapshot"
            );
        }
        assert!(!SCNET_TOKEN_PLAN_USABLE_MODELS.contains(&"glm-5.2"));
        assert!(!SCNET_TOKEN_PLAN_USABLE_MODELS.contains(&"DeepSeek-V4-Pro"));
        assert!(!SCNET_TOKEN_PLAN_USABLE_MODELS.contains(&"Qwen3-235B-A22B"));
    }

    #[test]
    fn scnet_token_plans_stay_fail_closed_without_quota_windows() {
        for offering_id in SCNET_TOKEN_PLAN_OFFERING_IDS {
            let plan = builtin_plan(SCNET_PROVIDER_ID, offering_id).unwrap();
            assert!(!plan.routable);
            assert_eq!(plan.verification_policy, VerificationPolicy::Required);
            assert_eq!(plan.verification_runtime_availability, "unavailable");
            assert_eq!(plan.pricing_availability, "unavailable");
            assert_eq!(plan.usage_availability, "unavailable");
            assert_eq!(plan.quota_unit, "credits");
            assert_ne!(plan.quota_unit, QUOTA_WINDOW_FIVE_HOURS);
            assert_ne!(plan.quota_unit, QUOTA_WINDOW_WEEK);
            assert_eq!(plan.key_prefix, Some(SCNET_TOKEN_PLAN_KEY_PREFIX));
            assert_eq!(plan.auth_schemes, &[UpstreamAuthScheme::Bearer]);
            assert_eq!(
                plan.upstream_protocols,
                &[
                    UpstreamProtocolKind::ChatCompletions,
                    UpstreamProtocolKind::Messages
                ]
            );
            assert!(
                !plan
                    .upstream_protocols
                    .contains(&UpstreamProtocolKind::Responses)
            );
            assert!(validate_plan_key(plan, "sk-tp-live").is_ok());
            assert!(
                validate_plan_key(plan, "sk-live-not-token")
                    .unwrap_err()
                    .to_string()
                    .contains(SCNET_TOKEN_PLAN_KEY_PREFIX)
            );
        }
        const {
            assert!(!SCNET_TOKEN_PLAN_USAGE_RESTRICTIONS.quota_status_rest_established);
            assert!(!SCNET_TOKEN_PLAN_USAGE_RESTRICTIONS.non_billable_verification_established);
            assert!(SCNET_TOKEN_PLAN_USAGE_RESTRICTIONS.custom_application_backends_prohibited);
            assert!(SCNET_TOKEN_PLAN_USAGE_RESTRICTIONS.automation_scripts_prohibited);
            assert!(SCNET_TOKEN_PLAN_USAGE_RESTRICTIONS.non_interactive_batch_calls_prohibited);
            assert!(
                SCNET_TOKEN_PLAN_USAGE_RESTRICTIONS.curl_style_non_interactive_calls_prohibited
            );
        };
        assert_eq!(
            SCNET_TOKEN_PLAN_DOCUMENTED_ENDPOINTS.auth_scheme,
            UpstreamAuthScheme::Bearer
        );
        assert_eq!(
            SCNET_TOKEN_PLAN_DOCUMENTED_ENDPOINTS.openai_base_url,
            "https://api.scnet.cn/api/llm/v1"
        );
        assert_eq!(
            SCNET_TOKEN_PLAN_DOCUMENTED_ENDPOINTS.anthropic_base_url,
            "https://api.scnet.cn/api/llm/anthropic"
        );
        assert_eq!(
            SCNET_TOKEN_PLAN_DOCUMENTED_ENDPOINTS.chat_completions_path,
            "/chat/completions"
        );
        assert_eq!(
            SCNET_TOKEN_PLAN_DOCUMENTED_ENDPOINTS.messages_path,
            "/v1/messages"
        );
        let goat = builtin_plan(COMMAND_CODE_PROVIDER_ID, GOAT_OFFERING_ID).unwrap();
        assert_eq!(goat.model_source, "builtin_command_code_protocol_table");
        assert!(
            scnet_token_plan_model_snapshot(COMMAND_CODE_PROVIDER_ID, GOAT_OFFERING_ID).is_none()
        );
    }

    #[test]
    fn provider_registry_is_exhaustive_for_plans_and_adapter_kinds() {
        let mut seen = std::collections::HashSet::new();
        assert_eq!(ProviderRegistry::iter().count(), BUILTIN_PLANS.len());
        for plan in BUILTIN_PLANS {
            let kind = ProviderAdapterKind::from_offering(
                plan.offering.provider_id,
                plan.offering.offering_id,
            )
            .expect("every catalog plan has an adapter kind");
            seen.insert(kind);
            let descriptor =
                ProviderRegistry::get(plan.offering.provider_id, plan.offering.offering_id)
                    .expect("every catalog plan has a composed descriptor");
            assert_eq!(descriptor.kind, kind);
            assert_eq!(descriptor.provider_id, plan.offering.provider_id);
            assert_eq!(descriptor.offering_id, plan.offering.offering_id);
            assert_eq!(descriptor.inference.catalog_routable, plan.routable);
            assert_eq!(
                descriptor.inference.credential_kind,
                plan.offering.credential_kind
            );
            assert_eq!(descriptor.inference.quota_scope, plan.offering.quota_scope);
            assert_eq!(descriptor.verification.policy, plan.verification_policy);
            assert_eq!(
                descriptor.verification.runtime_availability,
                plan.verification_runtime_availability
            );
            assert_eq!(descriptor.pricing.availability, plan.pricing_availability);
            assert_eq!(
                descriptor.usage.catalog_availability,
                plan.usage_availability
            );
            assert_eq!(
                descriptor.usage.manual_calibration,
                plan.manual_usage_calibration
            );
            assert_eq!(descriptor.model_catalog.catalog_source, plan.model_source);
            assert_eq!(
                descriptor.card_actions.managed_registration,
                plan.managed_registration
            );
            assert_eq!(
                descriptor.card_actions.persisted_enable_allowed,
                plan.routable
            );
            assert!(!descriptor.protocol_probe.request_path_may_trial);
            assert!(!descriptor.protocol_probe.fallback_priority.is_empty());
            assert_eq!(
                descriptor.protocol_probe.explicit_probe,
                descriptor.card_actions.protocol_probe
            );
            assert_eq!(
                descriptor.card_actions.catalog_refresh,
                kind.catalog_refresh_supported()
            );
            assert_eq!(
                descriptor.card_actions.protocol_probe,
                kind.protocol_probe_supported()
            );
            assert!(!descriptor.verification.uses_get_models);
            match kind {
                ProviderAdapterKind::OpenCodeGo
                | ProviderAdapterKind::ZenFree
                | ProviderAdapterKind::ConfigurableHttp => {
                    assert!(descriptor.inference.production_inference);
                }
                ProviderAdapterKind::CommandCodeGoat | ProviderAdapterKind::Scnet => {
                    assert!(!descriptor.inference.production_inference);
                    assert!(!descriptor.inference.catalog_routable);
                    assert_eq!(descriptor.verification.runtime_availability, "unavailable");
                    assert_eq!(
                        descriptor.card_actions.connection_verify,
                        CardVerifyAction::UnavailableNotImplemented
                    );
                }
            }
        }
        for kind in ProviderAdapterKind::ALL {
            assert!(
                seen.contains(&kind),
                "{kind:?} must be wired to at least one catalog offering"
            );
        }
        assert_eq!(seen.len(), ProviderAdapterKind::ALL.len());
        assert!(ProviderAdapterKind::from_offering("unknown", "unknown").is_none());
        assert!(ProviderRegistry::get("unknown", "unknown").is_none());
        assert_eq!(ProviderAdapterKind::ALL.len(), 5);
    }

    #[test]
    fn adapter_descriptors_preserve_current_capability_decisions() {
        let go = ProviderRegistry::get(OPENCODE_PROVIDER_ID, GO_OFFERING_ID).unwrap();
        assert_eq!(go.kind, ProviderAdapterKind::OpenCodeGo);
        assert_eq!(
            go.inference.auth,
            InferenceAuthDescriptor::OpenCodeProtocolDefault
        );
        assert!(go.inference.follow_redirects);
        assert_eq!(go.inference.origin, InferenceOriginKind::ConfigUpstreamBase);
        assert!(go.usage.automatic_sync);
        assert!(go.usage.authoritative_for_quota);
        assert_eq!(
            go.usage.endpoint,
            Some(crate::kernel::catalog::OPENCODE_GO_USAGE_URL)
        );
        assert_eq!(go.usage.endpoint, Some(crate::go_usage::GO_USAGE_URL));
        assert_eq!(
            crate::go_usage::GO_USAGE_URL,
            "https://opencode.ai/zen/go/v1/usage"
        );
        assert_eq!(go.usage.contract, UsageContractKind::Authoritative);
        assert!(go.usage.publishes_capability);
        assert!(go.error_cooldown.parse_opencode_go_windows_on_429);
        assert!(go.error_cooldown.schedule_official_go_usage_after_429);
        assert_eq!(
            go.protocol_probe.matrix,
            ProtocolMatrixKind::OpenCodeModelProtocols
        );
        assert!(go.protocol_probe.explicit_probe);
        assert_eq!(
            go.protocol_probe.structural_ceiling,
            StructuralProbeCeiling::OpenCodeConstructable
        );
        assert_eq!(
            go.card_actions.connection_verify,
            CardVerifyAction::Optional
        );
        assert!(go.card_actions.usage_refresh);
        assert!(go.card_actions.protocol_probe);
        assert!(!go.card_actions.catalog_refresh);

        let zen = ProviderRegistry::get(OPENCODE_ZEN_FREE_PROVIDER_ID, ANONYMOUS_FREE_OFFERING_ID)
            .unwrap();
        assert_eq!(zen.kind, ProviderAdapterKind::ZenFree);
        assert_eq!(zen.inference.auth, InferenceAuthDescriptor::None);
        assert_eq!(zen.inference.credential_kind, CredentialKind::None);
        assert_eq!(zen.inference.quota_scope, QuotaScope::EgressIp);
        assert_eq!(zen.inference.channel, Some(InferenceChannelKind::Free));
        assert!(zen.inference.follow_redirects);
        assert!(zen.model_catalog.admin_explicit_refresh);
        assert!(zen.protocol_probe.unknown_zen_free_defaults_to_chat);
        assert!(zen.protocol_probe.explicit_probe);
        assert_eq!(
            zen.protocol_probe.structural_ceiling,
            StructuralProbeCeiling::ZenFreeConstructable
        );
        assert!(!zen.usage.experimental);
        assert!(zen.error_cooldown.egress_ip_shared_free_cooldown);
        assert!(zen.error_cooldown.inference_401_passthrough);
        assert!(zen.error_cooldown.success_cost_state_free);
        assert!(zen.card_actions.fetch_zen_models);
        assert!(zen.card_actions.protocol_probe);
        assert!(zen.card_actions.catalog_refresh);
        assert_eq!(
            zen.card_actions.connection_verify,
            CardVerifyAction::NotApplicable
        );

        let goat = ProviderRegistry::get(COMMAND_CODE_PROVIDER_ID, GOAT_OFFERING_ID).unwrap();
        assert_eq!(goat.kind, ProviderAdapterKind::CommandCodeGoat);
        assert!(goat.inference.loopback_test_seam_only);
        assert!(!goat.inference.follow_redirects);
        assert_eq!(goat.inference.auth, InferenceAuthDescriptor::Bearer);
        assert!(goat.usage.experimental);
        assert!(!goat.usage.publishes_capability || goat.usage.endpoint.is_none());
        assert!(goat.usage.publishes_capability);
        assert_eq!(
            goat.usage.contract,
            UsageContractKind::ExperimentalUnavailable
        );
        assert!(goat.usage.manual_calibration);
        assert!(goat.error_cooldown.generic_provider_key_cooldown);
        assert_eq!(
            goat.protocol_probe.matrix,
            ProtocolMatrixKind::CommandCodeNative
        );
        assert!(!goat.protocol_probe.explicit_probe);
        assert_eq!(
            goat.protocol_probe.structural_ceiling,
            StructuralProbeCeiling::Unavailable
        );
        assert!(!goat.card_actions.protocol_probe);
        assert!(!goat.card_actions.catalog_refresh);

        let scnet =
            ProviderRegistry::get(SCNET_PROVIDER_ID, SCNET_TOKEN_PLAN_BASIC_OFFERING_ID).unwrap();
        assert_eq!(scnet.kind, ProviderAdapterKind::Scnet);
        assert!(scnet.model_catalog.snapshot_is_adapter_input_only);
        assert!(!scnet.usage.publishes_capability);
        assert_eq!(scnet.inference.origin, InferenceOriginKind::None);
        assert!(scnet.card_actions.risk_acknowledgement);
        assert_eq!(
            scnet.protocol_probe.matrix,
            ProtocolMatrixKind::DocumentedChatAndMessages
        );
        assert!(!scnet.protocol_probe.explicit_probe);
        assert_eq!(
            scnet.protocol_probe.structural_ceiling,
            StructuralProbeCeiling::Unavailable
        );
        assert!(!scnet.card_actions.protocol_probe);

        let custom = ProviderRegistry::get(CUSTOM_PROVIDER_ID, CUSTOM_API_OFFERING_ID).unwrap();
        assert_eq!(custom.kind, ProviderAdapterKind::ConfigurableHttp);
        assert_eq!(
            custom.inference.auth,
            InferenceAuthDescriptor::ConfigurableBearerOrXApiKey
        );
        assert!(!custom.inference.follow_redirects);
        assert_eq!(
            custom.inference.origin,
            InferenceOriginKind::AccountConfigured
        );
        assert!(custom.model_catalog.overlays_declared_ids);
        assert!(custom.verification.never_auto_enable);
        assert!(custom.verification.probe_first_declared_model);
        assert!(!custom.usage.publishes_capability);
        assert_eq!(
            custom.card_actions.connection_verify,
            CardVerifyAction::AvailableThenExplicitEnable
        );
        assert!(custom.card_actions.protocol_and_auth_immutable_after_create);
        assert!(custom.card_actions.discover_models);
        assert!(custom.card_actions.protocol_probe);
        assert!(custom.card_actions.catalog_refresh);
        assert_eq!(
            custom.protocol_probe.structural_ceiling,
            StructuralProbeCeiling::AccountDeclared
        );
        assert!(custom.error_cooldown.generic_provider_key_cooldown);

        assert_ne!(go.kind, ProviderAdapterKind::ConfigurableHttp);
        assert_ne!(zen.kind, ProviderAdapterKind::ConfigurableHttp);
        assert_ne!(goat.kind, ProviderAdapterKind::ConfigurableHttp);
        assert_ne!(scnet.kind, ProviderAdapterKind::ConfigurableHttp);
        assert_eq!(
            ProviderAdapterKind::from_offering(CUSTOM_PROVIDER_ID, CUSTOM_API_OFFERING_ID),
            Some(ProviderAdapterKind::ConfigurableHttp)
        );
    }

    #[test]
    fn composed_capability_contracts_delegate_from_concrete_adapters() {
        fn compose<A>(adapter: A, plan: BuiltinPlan) -> ProviderCapabilities
        where
            A: ModelCatalogAdapter
                + InferenceAdapter
                + ProtocolProbeAdapter
                + VerificationAdapter
                + UsageAdapter
                + PricingAdapter
                + ErrorPolicyAdapter
                + CardCapabilities,
        {
            ProviderCapabilities::compose(adapter, plan)
        }

        for plan in BUILTIN_PLANS {
            let kind = ProviderAdapterKind::from_offering(
                plan.offering.provider_id,
                plan.offering.offering_id,
            )
            .expect("every catalog plan has an adapter kind");
            let from_adapter = match kind {
                ProviderAdapterKind::OpenCodeGo => compose(OpenCodeGoAdapter, plan),
                ProviderAdapterKind::ZenFree => compose(ZenFreeAdapter, plan),
                ProviderAdapterKind::CommandCodeGoat => compose(CommandCodeGoatAdapter, plan),
                ProviderAdapterKind::Scnet => compose(ScnetAdapter, plan),
                ProviderAdapterKind::ConfigurableHttp => compose(ConfigurableHttpAdapter, plan),
            };
            let descriptor =
                ProviderRegistry::get(plan.offering.provider_id, plan.offering.offering_id)
                    .expect("every catalog plan has a composed descriptor");
            assert_eq!(kind.compose_capabilities(plan), from_adapter);
            assert_eq!(descriptor.capabilities(), from_adapter);
            assert_eq!(descriptor.model_catalog, from_adapter.model_catalog);
            assert_eq!(descriptor.inference, from_adapter.inference);
            assert_eq!(descriptor.protocol_probe, from_adapter.protocol_probe);
            assert_eq!(descriptor.verification, from_adapter.verification);
            assert_eq!(descriptor.usage, from_adapter.usage);
            assert_eq!(descriptor.pricing, from_adapter.pricing);
            assert_eq!(descriptor.error_cooldown, from_adapter.error_cooldown);
            assert_eq!(descriptor.card_actions, from_adapter.card_actions);
        }

        let go_plan = builtin_plan(OPENCODE_PROVIDER_ID, GO_OFFERING_ID).unwrap();
        assert_eq!(
            OpenCodeGoAdapter::usage(go_plan).contract,
            UsageContractKind::Authoritative
        );
        assert!(OpenCodeGoAdapter::error_policy(go_plan).parse_opencode_go_windows_on_429);
        let custom_plan = builtin_plan(CUSTOM_PROVIDER_ID, CUSTOM_API_OFFERING_ID).unwrap();
        assert!(ConfigurableHttpAdapter::verification(custom_plan).probe_first_declared_model);
        assert!(ConfigurableHttpAdapter::model_catalog(custom_plan).overlays_declared_ids);
        assert!(!ConfigurableHttpAdapter::inference(custom_plan).follow_redirects);
        let goat_plan = builtin_plan(COMMAND_CODE_PROVIDER_ID, GOAT_OFFERING_ID).unwrap();
        assert!(!CommandCodeGoatAdapter::inference(goat_plan).production_inference);
        let scnet_plan =
            builtin_plan(SCNET_PROVIDER_ID, SCNET_TOKEN_PLAN_BASIC_OFFERING_ID).unwrap();
        assert!(!ScnetAdapter::inference(scnet_plan).production_inference);
        let zen_plan =
            builtin_plan(OPENCODE_ZEN_FREE_PROVIDER_ID, ANONYMOUS_FREE_OFFERING_ID).unwrap();
        assert!(ZenFreeAdapter::protocol_probe(zen_plan).unknown_zen_free_defaults_to_chat);
        assert!(ZenFreeAdapter::card_capabilities(zen_plan).fetch_zen_models);
    }
    #[test]
    fn custom_base_url_errors_keep_existing_variants_and_messages() {
        assert_eq!(
            validate_custom_base_url("").unwrap_err(),
            ProviderBindingError::InvalidCustomBaseUrl("base URL is required".to_string())
        );
        assert_eq!(
            validate_custom_base_url("   ").unwrap_err(),
            ProviderBindingError::InvalidCustomBaseUrl("base URL is required".to_string())
        );
        let too_long = format!("https://api.example.com/{}", "a".repeat(2048));
        assert_eq!(
            validate_custom_base_url(&too_long).unwrap_err(),
            ProviderBindingError::InvalidCustomBaseUrl("base URL is too long".to_string())
        );
        let parsed_err = validate_custom_base_url("not a url").unwrap_err();
        match parsed_err {
            ProviderBindingError::InvalidCustomBaseUrl(message) => {
                assert!(message.starts_with("invalid base URL: "), "{message}");
            }
            other => panic!("expected InvalidCustomBaseUrl, got {other:?}"),
        }
        assert_eq!(
            validate_custom_base_url("https://api.example.com/v1?x=1").unwrap_err(),
            ProviderBindingError::InvalidCustomBaseUrl(
                "base URL must not include a query or fragment".to_string()
            )
        );
        assert_eq!(
            validate_custom_base_url("https://api.example.com/v1#frag").unwrap_err(),
            ProviderBindingError::InvalidCustomBaseUrl(
                "base URL must not include a query or fragment".to_string()
            )
        );
        assert_eq!(
            validate_custom_base_url("ftp://api.example.com/v1").unwrap_err(),
            ProviderBindingError::InvalidCustomBaseUrl(
                "base URL must use http or https".to_string()
            )
        );
        assert_eq!(
            validate_custom_base_url("javascript:alert(1)").unwrap_err(),
            ProviderBindingError::InvalidCustomBaseUrl(
                "base URL must use http or https".to_string()
            )
        );
        assert_eq!(
            validate_custom_base_url("https://user:pass@api.example.com").unwrap_err(),
            ProviderBindingError::InvalidCustomBaseUrl(
                "base URL must not include credentials".to_string()
            )
        );
        let hostless = reqwest::Url::parse("file:///tmp").unwrap();
        assert_eq!(
            inspect_custom_url(&hostless).unwrap_err(),
            ProviderBindingError::InvalidCustomBaseUrl(
                "base URL must use http or https".to_string()
            )
        );
    }

    #[test]
    fn historical_provider_facade_reexports_moved_symbols() {
        let _ = BUILTIN_PLANS;
        let _ = BUILTIN_OFFERINGS;
        let _ = COMMAND_CODE_GOAT_BASE_URL;
        let _ = SCNET_TOKEN_PLAN_USABLE_MODELS;
        let _ = ProviderAdapterKind::ALL;
        let _ = ProviderRegistry::iter();
        let _ = OpenCodeGoAdapter;
        let _ = ZenFreeAdapter;
        let _ = CommandCodeGoatAdapter;
        let _ = ScnetAdapter;
        let _ = ConfigurableHttpAdapter;
        let _ = acknowledgement_content_hash("x");
        assert!(plan_allows_enablement(
            builtin_plan(OPENCODE_PROVIDER_ID, GO_OFFERING_ID).unwrap()
        ));
        assert_eq!(
            std::any::type_name::<ProviderBindingError>(),
            "ocg_domain::provider::ProviderBindingError"
        );
        assert_eq!(
            std::any::type_name::<BuiltinPlan>(),
            "ocg_domain::provider::BuiltinPlan"
        );
        assert_eq!(
            std::any::type_name::<CustomUrlHost>(),
            "ocg_core::provider::CustomUrlHost"
        );
        assert_eq!(
            std::any::type_name::<QuotaWindow>(),
            "ocg_core::provider::QuotaWindow"
        );
    }
}
