//! Compatibility facade for [`ocg_domain::protocol`].
//!
//! Public items match the historical `ocg_core::kernel::protocol` surface.
//! `ModelProtocol` and `model_protocol` stay crate-private.

pub use ocg_domain::protocol::{
    ApiFormat, COMMAND_CODE_GOAT_STATIC_PROTOCOL_SNAPSHOT_DATE, CommandCodeModelProtocol,
    OLLAMA_CLOUD_STATIC_PROTOCOL_SNAPSHOT_DATE, OPENCODE_GO_STATIC_PROTOCOL_SNAPSHOT_DATE,
    OPENCODE_STATIC_PROTOCOL_SNAPSHOT_DATE, OllamaCloudModelProtocol,
    ZEN_FREE_STATIC_PROTOCOL_SNAPSHOT_DATE, command_code_model_protocol,
    command_code_protocol_profiles, command_code_supports_upstream, is_known_model,
    ollama_cloud_includes_model, ollama_cloud_model_protocol, ollama_cloud_protocol_seed_ids,
    ollama_cloud_shared_alias_stems, ollama_cloud_supported_formats,
    ollama_cloud_supports_upstream, opencode_supports_upstream, snapshot_protocols,
    supported_model_ids, supported_model_protocol_profiles, supported_model_protocols,
};

pub(crate) use ocg_domain::protocol::model_protocol;
// Keep the historical crate-private type path even though call sites name
// the value through `model_protocol` rather than `ModelProtocol`.
#[allow(unused_imports)]
pub(crate) use ocg_domain::protocol::ModelProtocol;
