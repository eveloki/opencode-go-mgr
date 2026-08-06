//! OpenCode Zen free-model allowlist, mapping, and route resolution.

use crate::gateway::protocol::ApiFormat;
use crate::models::{FreeModelRouting, UpstreamChannel};
use crate::pricing::normalize_model_name;
use bytes::Bytes;
use serde_json::Value;

/// Default free-usage cooldown when upstream omits a reset hint.
pub const DEFAULT_FREE_COOLDOWN_MINUTES: i64 = 30;

#[derive(Debug, Clone, Copy)]
struct FreeModelProfile {
    id: &'static str,
    /// Soft context window used for prefer-mode gating (tokens).
    context_tokens: u64,
}

const FREE_MODELS: &[FreeModelProfile] = &[
    FreeModelProfile {
        id: "big-pickle",
        context_tokens: 200_000,
    },
    FreeModelProfile {
        id: "deepseek-v4-flash-free",
        context_tokens: 200_000,
    },
    FreeModelProfile {
        id: "mimo-v2.5-free",
        context_tokens: 200_000,
    },
    FreeModelProfile {
        id: "ling-3.0-flash-free",
        context_tokens: 262_144,
    },
    FreeModelProfile {
        id: "laguna-s-2.1-free",
        context_tokens: 256_000,
    },
    FreeModelProfile {
        id: "longcat-2.0-free",
        context_tokens: 1_000_000,
    },
    FreeModelProfile {
        id: "north-mini-code-free",
        context_tokens: 256_000,
    },
    FreeModelProfile {
        id: "nemotron-3-ultra-free",
        context_tokens: 1_000_000,
    },
];

/// Go model id -> free twin id (prefer mode only).
const FREE_MAPPINGS: &[(&str, &str)] = &[
    ("deepseek-v4-flash", "deepseek-v4-flash-free"),
    ("mimo-v2.5", "mimo-v2.5-free"),
];

const CONTEXT_SAFETY_RATIO: f64 = 0.9;
/// Reserved headroom so short output still fits inside the free window.
const CONTEXT_OUTPUT_RESERVE: u64 = 4_096;

pub fn is_free_model(model: &str) -> bool {
    let normalized = normalize_model_name(model);
    FREE_MODELS.iter().any(|profile| profile.id == normalized)
}

pub fn free_model_ids() -> impl Iterator<Item = &'static str> {
    FREE_MODELS.iter().map(|profile| profile.id)
}

pub fn free_context_tokens(model: &str) -> Option<u64> {
    let normalized = normalize_model_name(model);
    FREE_MODELS
        .iter()
        .find(|profile| profile.id == normalized)
        .map(|profile| profile.context_tokens)
}

pub fn mapped_free_for(go_model: &str) -> Option<&'static str> {
    let normalized = normalize_model_name(go_model);
    FREE_MAPPINGS
        .iter()
        .find(|(go, _)| *go == normalized)
        .map(|(_, free)| *free)
}

/// Derive the Zen free base URL from the configured Go/Zen upstream.
///
/// - `…/zen/go` → `…/zen`
/// - `…/zen` → unchanged
/// - anything else → `None` (free channel unavailable)
pub fn derive_free_upstream_base(go_base: &str) -> Option<String> {
    let trimmed = go_base.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        return None;
    }
    let lower = trimmed.to_ascii_lowercase();
    if lower.ends_with("/zen/go") {
        Some(trimmed[..trimmed.len() - "/go".len()].to_string())
    } else if lower.ends_with("/zen") {
        Some(trimmed.to_string())
    } else {
        None
    }
}

pub fn resolve_upstream_base(channel: UpstreamChannel, go_base: &str) -> Result<String, String> {
    match channel {
        UpstreamChannel::Go => Ok(go_base.trim_end_matches('/').to_string()),
        UpstreamChannel::Free => derive_free_upstream_base(go_base).ok_or_else(|| {
            "Zen free models require an OpenCode Zen upstream (…/zen or …/zen/go); custom upstream cannot serve free models".to_string()
        }),
    }
}

#[derive(Debug, Clone)]
pub struct RouteDecision {
    pub channel: UpstreamChannel,
    pub model: String,
    /// Client-requested model before prefer mapping.
    pub original_model: String,
    pub mapped_from: Option<String>,
    /// When true, free-pool exhaustion may fall back to Go with `original_model`.
    pub allow_go_fallback: bool,
}

/// Decide channel + resolved model before account selection.
pub fn decide_route(
    policy: FreeModelRouting,
    requested_model: &str,
    _client: ApiFormat,
    _upstream: ApiFormat,
    body: &Bytes,
) -> Result<RouteDecision, String> {
    let original_model = requested_model.to_string();
    let free = is_free_model(requested_model);

    if free {
        return match policy {
            FreeModelRouting::Deny => Err(format!(
                "free model `{requested_model}` is disabled by free_model_routing=deny"
            )),
            FreeModelRouting::Explicit | FreeModelRouting::Prefer => Ok(RouteDecision {
                channel: UpstreamChannel::Free,
                model: normalize_model_name(requested_model).to_string(),
                original_model,
                mapped_from: None,
                allow_go_fallback: false,
            }),
        };
    }

    if policy != FreeModelRouting::Prefer {
        return Ok(RouteDecision {
            channel: UpstreamChannel::Go,
            model: original_model.clone(),
            original_model,
            mapped_from: None,
            allow_go_fallback: false,
        });
    }

    let Some(free_id) = mapped_free_for(requested_model) else {
        return Ok(RouteDecision {
            channel: UpstreamChannel::Go,
            model: original_model.clone(),
            original_model,
            mapped_from: None,
            allow_go_fallback: false,
        });
    };

    // Free twins are Chat-only; prepare_request converts client formats to Chat when needed.

    let Some(context_limit) = free_context_tokens(free_id) else {
        return Ok(RouteDecision {
            channel: UpstreamChannel::Go,
            model: original_model.clone(),
            original_model,
            mapped_from: None,
            allow_go_fallback: false,
        });
    };

    let estimated = estimate_request_tokens(body);
    let budget = ((context_limit as f64) * CONTEXT_SAFETY_RATIO) as u64;
    let budget = budget.saturating_sub(CONTEXT_OUTPUT_RESERVE);
    if estimated > budget {
        return Ok(RouteDecision {
            channel: UpstreamChannel::Go,
            model: original_model.clone(),
            original_model,
            mapped_from: None,
            allow_go_fallback: false,
        });
    }

    Ok(RouteDecision {
        channel: UpstreamChannel::Free,
        model: free_id.to_string(),
        original_model: original_model.clone(),
        mapped_from: Some(original_model),
        allow_go_fallback: true,
    })
}

/// Cheap heuristic: sum string lengths in the JSON body / 4.
pub fn estimate_request_tokens(body: &Bytes) -> u64 {
    let Ok(value) = serde_json::from_slice::<Value>(body) else {
        return (body.len() as u64 / 4).max(1);
    };
    let chars = count_string_chars(&value);
    (chars / 4).max(1)
}

fn count_string_chars(value: &Value) -> u64 {
    match value {
        Value::String(text) => text.chars().count() as u64,
        Value::Array(items) => items.iter().map(count_string_chars).sum(),
        Value::Object(map) => map.values().map(count_string_chars).sum(),
        _ => 0,
    }
}

pub fn rewrite_body_model(body: &Bytes, model: &str) -> Result<Bytes, String> {
    let mut value: Value =
        serde_json::from_slice(body).map_err(|error| format!("invalid JSON request: {error}"))?;
    let object = value
        .as_object_mut()
        .ok_or_else(|| "request must be a JSON object".to_string())?;
    object.insert("model".to_string(), Value::String(model.to_string()));
    serde_json::to_vec(&value)
        .map(Bytes::from)
        .map_err(|error| format!("failed to encode request: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{FreeModelRouting, UpstreamChannel};
    use serde_json::json;

    #[test]
    fn detects_free_allowlist_and_mappings() {
        assert!(is_free_model("deepseek-v4-flash-free"));
        assert!(is_free_model("big-pickle"));
        assert!(!is_free_model("deepseek-v4-flash"));
        assert_eq!(
            mapped_free_for("deepseek-v4-flash"),
            Some("deepseek-v4-flash-free")
        );
        assert_eq!(mapped_free_for("mimo-v2.5"), Some("mimo-v2.5-free"));
        assert_eq!(mapped_free_for("mimo-v2.5-pro"), None);
    }

    #[test]
    fn derives_free_base_from_go_or_zen() {
        assert_eq!(
            derive_free_upstream_base("https://opencode.ai/zen/go"),
            Some("https://opencode.ai/zen".into())
        );
        assert_eq!(
            derive_free_upstream_base("https://opencode.ai/zen/go/"),
            Some("https://opencode.ai/zen".into())
        );
        assert_eq!(
            derive_free_upstream_base("https://opencode.ai/zen"),
            Some("https://opencode.ai/zen".into())
        );
        assert_eq!(derive_free_upstream_base("https://example.com/v1"), None);
    }

    #[test]
    fn deny_blocks_explicit_free() {
        let err = decide_route(
            FreeModelRouting::Deny,
            "big-pickle",
            ApiFormat::ChatCompletions,
            ApiFormat::ChatCompletions,
            &Bytes::from_static(br#"{"model":"big-pickle","messages":[]}"#),
        )
        .unwrap_err();
        assert!(err.contains("disabled"));
    }

    #[test]
    fn prefer_maps_short_requests_and_keeps_long_on_go() {
        let short = Bytes::from(
            serde_json::to_vec(&json!({
                "model": "deepseek-v4-flash",
                "messages": [{"role":"user","content":"hi"}]
            }))
            .unwrap(),
        );
        let decision = decide_route(
            FreeModelRouting::Prefer,
            "deepseek-v4-flash",
            ApiFormat::ChatCompletions,
            ApiFormat::ChatCompletions,
            &short,
        )
        .unwrap();
        assert_eq!(decision.channel, UpstreamChannel::Free);
        assert_eq!(decision.model, "deepseek-v4-flash-free");
        assert!(decision.allow_go_fallback);

        let long_text = "x".repeat(900_000);
        let long = Bytes::from(
            serde_json::to_vec(&json!({
                "model": "deepseek-v4-flash",
                "messages": [{"role":"user","content": long_text}]
            }))
            .unwrap(),
        );
        let decision = decide_route(
            FreeModelRouting::Prefer,
            "deepseek-v4-flash",
            ApiFormat::ChatCompletions,
            ApiFormat::ChatCompletions,
            &long,
        )
        .unwrap();
        assert_eq!(decision.channel, UpstreamChannel::Go);
        assert_eq!(decision.model, "deepseek-v4-flash");
        assert!(!decision.allow_go_fallback);
    }

    #[test]
    fn explicit_keeps_go_models_on_go() {
        let body = Bytes::from_static(br#"{"model":"glm-5.2","messages":[]}"#);
        let decision = decide_route(
            FreeModelRouting::Explicit,
            "glm-5.2",
            ApiFormat::ChatCompletions,
            ApiFormat::ChatCompletions,
            &body,
        )
        .unwrap();
        assert_eq!(decision.channel, UpstreamChannel::Go);
        assert!(!decision.allow_go_fallback);
    }
}
