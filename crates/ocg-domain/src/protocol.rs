//! I/O-free client/upstream protocol identities and static model catalogs.
//!
//! Request conversion, HTTP, and adapter execution stay in the host crate's
//! gateway protocol module. This module holds only the enums and tables
//! later control-plane and GatewayExecutor work can share without pulling
//! gateway I/O.

use super::ids::{
    COMMAND_CODE_GOAT_DEEPSEEK_V4_FLASH_ALIAS, COMMAND_CODE_GOAT_DEEPSEEK_V4_FLASH_UPSTREAM,
    normalize_model_name,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ApiFormat {
    ChatCompletions,
    Responses,
    Messages,
    /// Google Gemini generateContent wire format. This is client-only: OCG
    /// always translates it to a model's known native upstream protocol.
    Gemini,
}

impl ApiFormat {
    pub fn upstream_path(self) -> Option<&'static str> {
        match self {
            Self::ChatCompletions => Some("/v1/chat/completions"),
            Self::Responses => Some("/v1/responses"),
            Self::Messages => Some("/v1/messages"),
            Self::Gemini => None,
        }
    }
}

/// Hardcoded OpenCode-Go protocol profiles.
///
/// `preferred` matches the official Go docs endpoint table. `supported` is the
/// set of upstream protocols verified with a test account; update only after a
/// fresh probe. Request paths never trial protocols (double-billing risk).
///
/// Public only as the cross-crate bridge; `ocg_core::kernel::protocol` keeps
/// this type and its fields crate-private.
#[derive(Debug, Clone, Copy)]
#[doc(hidden)]
pub struct ModelProtocol {
    #[doc(hidden)]
    pub id: &'static str,
    #[doc(hidden)]
    pub preferred: ApiFormat,
    #[doc(hidden)]
    pub supported: &'static [ApiFormat],
    /// Aliases applied to `reasoning.effort` / `reasoning_effort` before forwarding
    /// or converting, for models whose upstream rejects a standard OCG effort.
    /// Empty slice = pass through unchanged.
    #[doc(hidden)]
    pub effort_aliases: &'static [(&'static str, &'static str)],
}

const NO_EFFORT_ALIASES: &[(&str, &str)] = &[];
const MUSE_SPARK_EFFORT_ALIASES: &[(&str, &str)] = &[("max", "xhigh")];
const NO_PROTOCOLS: &[ApiFormat] = &[];

/// Provider-specific dates for the direct protocol snapshots below. These are
/// protocol-evidence metadata, not model-catalog refresh timestamps.
pub const OPENCODE_GO_STATIC_PROTOCOL_SNAPSHOT_DATE: &str = "2026-08-27";
pub const ZEN_FREE_STATIC_PROTOCOL_SNAPSHOT_DATE: &str = "2026-08-27";
pub const COMMAND_CODE_GOAT_STATIC_PROTOCOL_SNAPSHOT_DATE: &str = "2026-08-27";
/// Backward-compatible Go-only name for the current static snapshot date.
pub const OPENCODE_STATIC_PROTOCOL_SNAPSHOT_DATE: &str = OPENCODE_GO_STATIC_PROTOCOL_SNAPSHOT_DATE;

// Snapshot support comes from the sanitized 2026-08-27 stream + non-stream
// sweep in docs/maintainer/evidence/protocol-probes/2026-08-27. GOAT
// MODEL_NOT_IN_PLAN responses remain evidence of protocol shape, but are not
// static support because the channel was not usable in this run.
const CHAT_ONLY: &[ApiFormat] = &[ApiFormat::ChatCompletions];
const RESPONSES_ONLY: &[ApiFormat] = &[ApiFormat::Responses];
const MESSAGES_ONLY: &[ApiFormat] = &[ApiFormat::Messages];
const CHAT_AND_MESSAGES: &[ApiFormat] = &[ApiFormat::ChatCompletions, ApiFormat::Messages];
const ALL_THREE: &[ApiFormat] = &[
    ApiFormat::ChatCompletions,
    ApiFormat::Responses,
    ApiFormat::Messages,
];

const GO_SNAPSHOT: &[(&str, ApiFormat)] = &[
    ("deepseek-v4-flash", ApiFormat::ChatCompletions),
    ("deepseek-v4-flash", ApiFormat::Responses),
    ("deepseek-v4-flash", ApiFormat::Messages),
    ("deepseek-v4-flash-vision-exp", ApiFormat::ChatCompletions),
    ("deepseek-v4-flash-vision-exp", ApiFormat::Responses),
    ("deepseek-v4-flash-vision-exp", ApiFormat::Messages),
    ("deepseek-v4-pro", ApiFormat::ChatCompletions),
    ("deepseek-v4-pro", ApiFormat::Responses),
    ("deepseek-v4-pro", ApiFormat::Messages),
    ("glm-5", ApiFormat::ChatCompletions),
    ("glm-5.1", ApiFormat::ChatCompletions),
    ("glm-5.2", ApiFormat::ChatCompletions),
    ("glm-5.3", ApiFormat::ChatCompletions),
    ("glm-5.3-flash", ApiFormat::ChatCompletions),
    ("gpt-5.6-luna", ApiFormat::Responses),
    ("grok-4.5", ApiFormat::Responses),
    ("grok-4.6", ApiFormat::Responses),
    ("hy3", ApiFormat::ChatCompletions),
    ("kimi-k2.5", ApiFormat::ChatCompletions),
    ("kimi-k2.6", ApiFormat::ChatCompletions),
    ("kimi-k2.7-code", ApiFormat::ChatCompletions),
    ("kimi-k3", ApiFormat::ChatCompletions),
    ("kimi-k3", ApiFormat::Messages),
    ("longcat-2.0", ApiFormat::ChatCompletions),
    ("mimo-v2.5", ApiFormat::ChatCompletions),
    ("mimo-v2.5-pro", ApiFormat::ChatCompletions),
    ("minimax-m2.5", ApiFormat::ChatCompletions),
    ("minimax-m2.5", ApiFormat::Messages),
    ("minimax-m2.7", ApiFormat::Messages),
    ("minimax-m3", ApiFormat::ChatCompletions),
    ("minimax-m3", ApiFormat::Messages),
    ("muse-spark-1.2-contributor", ApiFormat::Responses),
    ("qwen3.5-plus", ApiFormat::ChatCompletions),
    ("qwen3.5-plus", ApiFormat::Messages),
    ("qwen3.6-plus", ApiFormat::ChatCompletions),
    ("qwen3.6-plus", ApiFormat::Messages),
    ("qwen3.7-max", ApiFormat::ChatCompletions),
    ("qwen3.7-max", ApiFormat::Messages),
    ("qwen3.7-plus", ApiFormat::ChatCompletions),
    ("qwen3.7-plus", ApiFormat::Messages),
    ("qwen3.8-max", ApiFormat::ChatCompletions),
    ("qwen3.8-max", ApiFormat::Messages),
];
const ZEN_SNAPSHOT: &[(&str, ApiFormat)] = &[
    ("hy3-free", ApiFormat::ChatCompletions),
    ("nemotron-3-ultra-free", ApiFormat::ChatCompletions),
    ("muse-spark-1.2-contributor-free", ApiFormat::Responses),
];
const GOAT_SNAPSHOT: &[(&str, ApiFormat)] = &[
    ("deepseek/deepseek-v4-flash", ApiFormat::ChatCompletions),
    (
        "deepseek/deepseek-v4-flash-vision-exp",
        ApiFormat::ChatCompletions,
    ),
    ("deepseek/deepseek-v4-pro", ApiFormat::ChatCompletions),
    ("google/gemini-3.7-flash", ApiFormat::ChatCompletions),
    ("gpt-5.6-luna", ApiFormat::ChatCompletions),
    ("gpt-5.6-sol", ApiFormat::ChatCompletions),
    ("meta/muse-spark-1.2", ApiFormat::ChatCompletions),
    (
        "meta/muse-spark-1.2-contributor",
        ApiFormat::ChatCompletions,
    ),
    ("MiniMaxAI/MiniMax-M2.5", ApiFormat::ChatCompletions),
    ("MiniMaxAI/MiniMax-M2.7", ApiFormat::ChatCompletions),
    ("MiniMaxAI/MiniMax-M3", ApiFormat::ChatCompletions),
    ("moonshotai/Kimi-K2.5", ApiFormat::ChatCompletions),
    ("moonshotai/Kimi-K2.6", ApiFormat::ChatCompletions),
    ("moonshotai/Kimi-K2.7-Code", ApiFormat::ChatCompletions),
    (
        "moonshotai/Kimi-K2.7-Code-Highspeed",
        ApiFormat::ChatCompletions,
    ),
    ("moonshotai/Kimi-K3", ApiFormat::ChatCompletions),
    (
        "nvidia/nemotron-3-ultra-550b-a55b",
        ApiFormat::ChatCompletions,
    ),
    ("poolside/laguna-s-2.1-free", ApiFormat::ChatCompletions),
    ("Qwen/Qwen3.6-Max-Preview", ApiFormat::ChatCompletions),
    ("Qwen/Qwen3.6-Plus", ApiFormat::ChatCompletions),
    ("Qwen/Qwen3.7-Flash", ApiFormat::ChatCompletions),
    ("Qwen/Qwen3.7-Max", ApiFormat::ChatCompletions),
    ("Qwen/Qwen3.7-Plus", ApiFormat::ChatCompletions),
    ("Qwen/Qwen3.8-27B", ApiFormat::ChatCompletions),
    ("Qwen/Qwen3.8-Max", ApiFormat::ChatCompletions),
    ("stepfun/Step-3.5-Flash", ApiFormat::ChatCompletions),
    ("stepfun/Step-3.7-Flash", ApiFormat::ChatCompletions),
    ("tencent/hy3-paid", ApiFormat::ChatCompletions),
    ("thinkingmachines/inkling", ApiFormat::ChatCompletions),
    ("thinkingmachines/inkling-small", ApiFormat::ChatCompletions),
    ("xai/grok-4.5", ApiFormat::ChatCompletions),
    ("xai/grok-4.6", ApiFormat::ChatCompletions),
    ("xiaomi/mimo-v2.5", ApiFormat::ChatCompletions),
    ("xiaomi/mimo-v2.5-pro", ApiFormat::ChatCompletions),
    ("zai-org/GLM-5", ApiFormat::ChatCompletions),
    ("zai-org/GLM-5.1", ApiFormat::ChatCompletions),
    ("zai-org/GLM-5.2", ApiFormat::ChatCompletions),
    ("zai-org/GLM-5.2-Fast", ApiFormat::ChatCompletions),
    ("zai-org/GLM-5.3", ApiFormat::ChatCompletions),
];

pub fn snapshot_protocols(provider_id: &str, model_id: &str) -> Vec<ApiFormat> {
    let rows = match provider_id {
        "opencode" => GO_SNAPSHOT,
        "opencode-zen-free" => ZEN_SNAPSHOT,
        "command-code" => GOAT_SNAPSHOT,
        _ => &[],
    };
    rows.iter()
        .filter(|(id, _)| id.eq_ignore_ascii_case(model_id))
        .map(|(_, protocol)| *protocol)
        .collect()
}

const MODEL_PROTOCOLS: &[ModelProtocol] = &[
    ModelProtocol {
        id: "grok-4.6",
        preferred: ApiFormat::Responses,
        supported: RESPONSES_ONLY,
        effort_aliases: NO_EFFORT_ALIASES,
    },
    ModelProtocol {
        id: "grok-4.5",
        preferred: ApiFormat::Responses,
        supported: RESPONSES_ONLY,
        effort_aliases: NO_EFFORT_ALIASES,
    },
    ModelProtocol {
        id: "glm-5.3-flash",
        preferred: ApiFormat::ChatCompletions,
        supported: CHAT_ONLY,
        effort_aliases: NO_EFFORT_ALIASES,
    },
    ModelProtocol {
        id: "glm-5.3",
        preferred: ApiFormat::ChatCompletions,
        supported: CHAT_ONLY,
        effort_aliases: NO_EFFORT_ALIASES,
    },
    ModelProtocol {
        id: "glm-5.2",
        preferred: ApiFormat::ChatCompletions,
        supported: CHAT_ONLY,
        effort_aliases: NO_EFFORT_ALIASES,
    },
    ModelProtocol {
        id: "glm-5.1",
        preferred: ApiFormat::ChatCompletions,
        supported: CHAT_ONLY,
        effort_aliases: NO_EFFORT_ALIASES,
    },
    ModelProtocol {
        id: "glm-5",
        preferred: ApiFormat::ChatCompletions,
        supported: CHAT_ONLY,
        effort_aliases: NO_EFFORT_ALIASES,
    },
    ModelProtocol {
        id: "gpt-5.6-luna",
        preferred: ApiFormat::Responses,
        supported: RESPONSES_ONLY,
        effort_aliases: NO_EFFORT_ALIASES,
    },
    ModelProtocol {
        id: "muse-spark-1.2",
        preferred: ApiFormat::Responses,
        supported: NO_PROTOCOLS,
        effort_aliases: MUSE_SPARK_EFFORT_ALIASES,
    },
    ModelProtocol {
        id: "muse-spark-1.2-contributor",
        preferred: ApiFormat::Responses,
        supported: RESPONSES_ONLY,
        effort_aliases: MUSE_SPARK_EFFORT_ALIASES,
    },
    ModelProtocol {
        id: "muse-spark-1.2-contributor-free",
        preferred: ApiFormat::Responses,
        supported: RESPONSES_ONLY,
        effort_aliases: MUSE_SPARK_EFFORT_ALIASES,
    },
    ModelProtocol {
        id: "kimi-k3",
        preferred: ApiFormat::ChatCompletions,
        supported: CHAT_AND_MESSAGES,
        effort_aliases: NO_EFFORT_ALIASES,
    },
    ModelProtocol {
        id: "kimi-k2.7-code",
        preferred: ApiFormat::ChatCompletions,
        supported: CHAT_ONLY,
        effort_aliases: NO_EFFORT_ALIASES,
    },
    ModelProtocol {
        id: "kimi-k2.6",
        preferred: ApiFormat::ChatCompletions,
        supported: CHAT_ONLY,
        effort_aliases: NO_EFFORT_ALIASES,
    },
    ModelProtocol {
        id: "kimi-k2.5",
        preferred: ApiFormat::ChatCompletions,
        supported: CHAT_ONLY,
        effort_aliases: NO_EFFORT_ALIASES,
    },
    ModelProtocol {
        id: "deepseek-v4-pro",
        preferred: ApiFormat::ChatCompletions,
        supported: ALL_THREE,
        effort_aliases: NO_EFFORT_ALIASES,
    },
    ModelProtocol {
        id: "deepseek-v4-flash",
        preferred: ApiFormat::ChatCompletions,
        supported: ALL_THREE,
        effort_aliases: NO_EFFORT_ALIASES,
    },
    ModelProtocol {
        id: "deepseek-v4-flash-vision-exp",
        preferred: ApiFormat::ChatCompletions,
        supported: ALL_THREE,
        effort_aliases: NO_EFFORT_ALIASES,
    },
    ModelProtocol {
        id: "mimo-v2.5",
        preferred: ApiFormat::ChatCompletions,
        supported: CHAT_ONLY,
        effort_aliases: NO_EFFORT_ALIASES,
    },
    ModelProtocol {
        id: "mimo-v2.5-pro",
        preferred: ApiFormat::ChatCompletions,
        supported: CHAT_ONLY,
        effort_aliases: NO_EFFORT_ALIASES,
    },
    ModelProtocol {
        id: "hy3",
        preferred: ApiFormat::ChatCompletions,
        supported: CHAT_ONLY,
        effort_aliases: NO_EFFORT_ALIASES,
    },
    ModelProtocol {
        id: "longcat-2.0",
        preferred: ApiFormat::ChatCompletions,
        supported: CHAT_ONLY,
        effort_aliases: NO_EFFORT_ALIASES,
    },
    ModelProtocol {
        // Official Go docs: Ox Alpha Free, Chat Completions on `/zen/go`.
        // The id contains `free` but this is not a Zen promo model.
        id: "ox-alpha-free",
        preferred: ApiFormat::ChatCompletions,
        supported: NO_PROTOCOLS,
        effort_aliases: NO_EFFORT_ALIASES,
    },
    ModelProtocol {
        id: "minimax-m3",
        preferred: ApiFormat::Messages,
        supported: CHAT_AND_MESSAGES,
        effort_aliases: NO_EFFORT_ALIASES,
    },
    ModelProtocol {
        id: "minimax-m2.7",
        preferred: ApiFormat::Messages,
        supported: MESSAGES_ONLY,
        effort_aliases: NO_EFFORT_ALIASES,
    },
    ModelProtocol {
        id: "minimax-m2.7-highspeed",
        preferred: ApiFormat::Messages,
        supported: NO_PROTOCOLS,
        effort_aliases: NO_EFFORT_ALIASES,
    },
    ModelProtocol {
        id: "minimax-m2.5",
        preferred: ApiFormat::Messages,
        supported: CHAT_AND_MESSAGES,
        effort_aliases: NO_EFFORT_ALIASES,
    },
    ModelProtocol {
        id: "minimax-m2.5-highspeed",
        preferred: ApiFormat::Messages,
        supported: NO_PROTOCOLS,
        effort_aliases: NO_EFFORT_ALIASES,
    },
    ModelProtocol {
        id: "qwen3.8-max",
        preferred: ApiFormat::Messages,
        supported: CHAT_AND_MESSAGES,
        effort_aliases: NO_EFFORT_ALIASES,
    },
    ModelProtocol {
        id: "qwen3.7-max",
        preferred: ApiFormat::Messages,
        supported: CHAT_AND_MESSAGES,
        effort_aliases: NO_EFFORT_ALIASES,
    },
    ModelProtocol {
        id: "qwen3.7-plus",
        preferred: ApiFormat::Messages,
        supported: CHAT_AND_MESSAGES,
        effort_aliases: NO_EFFORT_ALIASES,
    },
    ModelProtocol {
        id: "qwen3.6-plus",
        preferred: ApiFormat::Messages,
        supported: CHAT_AND_MESSAGES,
        effort_aliases: NO_EFFORT_ALIASES,
    },
    ModelProtocol {
        id: "qwen3.5-plus",
        preferred: ApiFormat::Messages,
        supported: CHAT_AND_MESSAGES,
        effort_aliases: NO_EFFORT_ALIASES,
    },
    ModelProtocol {
        id: "big-pickle",
        preferred: ApiFormat::ChatCompletions,
        supported: NO_PROTOCOLS,
        effort_aliases: NO_EFFORT_ALIASES,
    },
    ModelProtocol {
        id: "hy3-free",
        preferred: ApiFormat::ChatCompletions,
        supported: CHAT_ONLY,
        effort_aliases: NO_EFFORT_ALIASES,
    },
    ModelProtocol {
        id: "deepseek-v4-flash-free",
        preferred: ApiFormat::ChatCompletions,
        supported: NO_PROTOCOLS,
        effort_aliases: NO_EFFORT_ALIASES,
    },
    ModelProtocol {
        id: "mimo-v2.5-free",
        preferred: ApiFormat::ChatCompletions,
        supported: NO_PROTOCOLS,
        effort_aliases: NO_EFFORT_ALIASES,
    },
    ModelProtocol {
        id: "ling-3.0-flash-free",
        preferred: ApiFormat::ChatCompletions,
        supported: NO_PROTOCOLS,
        effort_aliases: NO_EFFORT_ALIASES,
    },
    ModelProtocol {
        id: "laguna-s-2.1-free",
        preferred: ApiFormat::ChatCompletions,
        supported: NO_PROTOCOLS,
        effort_aliases: NO_EFFORT_ALIASES,
    },
    ModelProtocol {
        id: "longcat-2.0-free",
        preferred: ApiFormat::ChatCompletions,
        supported: NO_PROTOCOLS,
        effort_aliases: NO_EFFORT_ALIASES,
    },
    ModelProtocol {
        id: "north-mini-code-free",
        preferred: ApiFormat::ChatCompletions,
        supported: NO_PROTOCOLS,
        effort_aliases: NO_EFFORT_ALIASES,
    },
    ModelProtocol {
        id: "nemotron-3-ultra-free",
        preferred: ApiFormat::ChatCompletions,
        supported: CHAT_ONLY,
        effort_aliases: NO_EFFORT_ALIASES,
    },
    ModelProtocol {
        id: "nemotron-3.5-lightning-free",
        preferred: ApiFormat::ChatCompletions,
        supported: NO_PROTOCOLS,
        effort_aliases: NO_EFFORT_ALIASES,
    },
];

/// Returns every model ID with a known preferred upstream protocol.
pub fn supported_model_ids() -> impl Iterator<Item = &'static str> {
    MODEL_PROTOCOLS.iter().map(|profile| profile.id)
}

/// Provider adapters use the same probed OpenCode matrix as request planning;
/// this prevents a mixed-provider selector from probing an unsupported
/// model/protocol pair merely because that account appears earlier.
pub fn opencode_supports_upstream(model: &str, upstream: ApiFormat) -> bool {
    snapshot_protocols("opencode", model).contains(&upstream)
}

/// Command Code GOAT protocol profiles, independent of OpenCode `MODEL_PROTOCOLS`.
/// Lookup is exact (case-insensitive) on the upstream raw ID. Slash IDs are
/// never folded onto kebab OpenCode aliases, so `deepseek/deepseek-v4-flash`
/// cannot steal Go's `deepseek-v4-flash` protocol row.
///
/// Models outside this seed table still follow the official split: Anthropic
/// IDs use Messages; OpenAI and open-source IDs use Chat Completions. There is
/// no Responses upstream.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CommandCodeModelProtocol {
    pub alias: &'static str,
    pub upstream_id: &'static str,
    pub preferred: ApiFormat,
    pub supported_upstream: &'static [ApiFormat],
}

const COMMAND_CODE_MODEL_PROTOCOLS: &[CommandCodeModelProtocol] = &[CommandCodeModelProtocol {
    alias: COMMAND_CODE_GOAT_DEEPSEEK_V4_FLASH_ALIAS,
    upstream_id: COMMAND_CODE_GOAT_DEEPSEEK_V4_FLASH_UPSTREAM,
    preferred: ApiFormat::ChatCompletions,
    supported_upstream: CHAT_ONLY,
}];

/// Exact Command Code raw-ID lookup. Does not consult OpenCode `MODEL_PROTOCOLS`
/// and does not slash-fold onto a kebab alias.
pub fn command_code_model_protocol(model: &str) -> Option<&'static CommandCodeModelProtocol> {
    let trimmed = model.trim();
    if trimmed.is_empty() {
        return None;
    }
    COMMAND_CODE_MODEL_PROTOCOLS
        .iter()
        .find(|profile| profile.upstream_id.eq_ignore_ascii_case(trimmed))
}

pub fn command_code_protocol_profiles() -> impl Iterator<Item = &'static CommandCodeModelProtocol> {
    COMMAND_CODE_MODEL_PROTOCOLS.iter()
}

/// Official Command Code family split: Anthropic models speak Messages;
/// everything else speaks Chat Completions.
pub fn command_code_is_anthropic_model(model: &str) -> bool {
    let trimmed = model.trim();
    if trimmed.is_empty() {
        return false;
    }
    let lower = trimmed.to_ascii_lowercase();
    let leaf = lower.rsplit('/').next().unwrap_or(lower.as_str());
    leaf.starts_with("claude") || lower.starts_with("anthropic/")
}

/// Preferred upstream for a Command Code model ID. Seed-table rows win;
/// unknown non-empty IDs follow the Anthropic/Chat family rule.
pub fn command_code_preferred_format(model: &str) -> Option<ApiFormat> {
    if let Some(profile) = command_code_model_protocol(model) {
        return Some(profile.preferred);
    }
    if model.trim().is_empty() {
        return None;
    }
    Some(if command_code_is_anthropic_model(model) {
        ApiFormat::Messages
    } else {
        ApiFormat::ChatCompletions
    })
}

pub fn command_code_supported_formats(model: &str) -> &'static [ApiFormat] {
    if let Some(profile) = command_code_model_protocol(model) {
        return profile.supported_upstream;
    }
    if model.trim().is_empty() {
        return &[];
    }
    if command_code_is_anthropic_model(model) {
        MESSAGES_ONLY
    } else {
        CHAT_ONLY
    }
}

pub fn command_code_supports_upstream(model: &str, upstream: ApiFormat) -> bool {
    command_code_supported_formats(model).contains(&upstream)
}

/// Returns (id, preferred protocol) for every known OpenCode catalog model;
/// backs the proxy list picker's protocol hints.
pub fn supported_model_protocols() -> impl Iterator<Item = (&'static str, ApiFormat)> {
    MODEL_PROTOCOLS
        .iter()
        .map(|profile| (profile.id, profile.preferred))
}

/// Returns the canonical model ID, preferred protocol, and every directly
/// supported OpenCode Go upstream protocol. Dashboard account tests consume
/// this same probed matrix so the UI never offers a billable trial pair that
/// request routing itself considers unsupported.
pub fn supported_model_protocol_profiles()
-> impl Iterator<Item = (&'static str, ApiFormat, &'static [ApiFormat])> {
    MODEL_PROTOCOLS
        .iter()
        .map(|profile| (profile.id, profile.preferred, profile.supported))
}

/// True when the OpenCode protocol catalog contains the model ID.
pub fn is_known_model(model: &str) -> bool {
    model_protocol(model).is_some()
}

#[doc(hidden)]
pub fn model_protocol(model: &str) -> Option<&'static ModelProtocol> {
    let normalized = normalize_model_name(model);
    MODEL_PROTOCOLS
        .iter()
        .find(|profile| profile.id == normalized)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protocol_snapshot_has_exact_provider_counts_and_known_boundaries() {
        assert_eq!(GO_SNAPSHOT.len(), 42);
        assert_eq!(ZEN_SNAPSHOT.len(), 3);
        assert_eq!(GOAT_SNAPSHOT.len(), 39);
        for profile in MODEL_PROTOCOLS {
            let provider_id = if profile.id.ends_with("-free") && profile.id != "ox-alpha-free" {
                "opencode-zen-free"
            } else {
                "opencode"
            };
            assert_eq!(
                profile.supported,
                snapshot_protocols(provider_id, profile.id),
                "profile support drifted from the static snapshot for {}",
                profile.id
            );
        }
        for model in [
            "deepseek/deepseek-v4-flash-vision-exp",
            "meta/muse-spark-1.2-contributor",
            "moonshotai/Kimi-K2.7-Code-Highspeed",
            "nvidia/nemotron-3-ultra-550b-a55b",
        ] {
            assert!(
                snapshot_protocols("command-code", model).contains(&ApiFormat::ChatCompletions)
            );
        }
        assert!(
            snapshot_protocols("opencode-zen-free", "hy3-free")
                .contains(&ApiFormat::ChatCompletions)
        );
        assert!(
            snapshot_protocols("opencode-zen-free", "muse-spark-1.2-contributor-free")
                .contains(&ApiFormat::Responses)
        );
        assert!(snapshot_protocols("command-code", "stealth/ox-alpha").is_empty());
    }

    #[test]
    fn command_code_family_rules_split_anthropic_from_chat() {
        assert!(command_code_is_anthropic_model("claude-sonnet-4-6"));
        assert!(command_code_is_anthropic_model("anthropic/claude-opus-4-6"));
        assert!(command_code_is_anthropic_model("Claude-Haiku-4-5"));
        assert!(!command_code_is_anthropic_model(
            COMMAND_CODE_GOAT_DEEPSEEK_V4_FLASH_UPSTREAM
        ));
        assert!(!command_code_is_anthropic_model("gpt-5.4"));
        assert_eq!(
            command_code_preferred_format("claude-sonnet-4-6"),
            Some(ApiFormat::Messages)
        );
        assert_eq!(
            command_code_preferred_format(COMMAND_CODE_GOAT_DEEPSEEK_V4_FLASH_UPSTREAM),
            Some(ApiFormat::ChatCompletions)
        );
        assert!(command_code_supports_upstream(
            "claude-sonnet-4-6",
            ApiFormat::Messages
        ));
        assert!(!command_code_supports_upstream(
            "claude-sonnet-4-6",
            ApiFormat::ChatCompletions
        ));
        assert!(command_code_supports_upstream(
            COMMAND_CODE_GOAT_DEEPSEEK_V4_FLASH_UPSTREAM,
            ApiFormat::ChatCompletions
        ));
        assert!(!command_code_supports_upstream(
            COMMAND_CODE_GOAT_DEEPSEEK_V4_FLASH_UPSTREAM,
            ApiFormat::Responses
        ));
        assert!(command_code_supports_upstream(
            "minimax-m2.7",
            ApiFormat::ChatCompletions
        ));
        assert!(!command_code_supports_upstream(
            "",
            ApiFormat::ChatCompletions
        ));
        assert!(
            command_code_model_protocol(COMMAND_CODE_GOAT_DEEPSEEK_V4_FLASH_ALIAS).is_none(),
            "kebab Go aliases must not resolve through the Command Code seed table"
        );
    }
}
