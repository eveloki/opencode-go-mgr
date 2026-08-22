//! Shared Dashboard V3 wire types and the JSON Schema catalog.
//!
//! Response objects always serialize nullable fields as `T | null` (never omitted).
//! Request optional fields may be omitted; `expectedRevision` is required on every
//! control-plane mutation. Plaintext keys must not appear here — `ConnectionInfo`
//! is the only future secret-bearing V3 DTO.

use schemars::JsonSchema;
use schemars::generate::{SchemaGenerator, SchemaSettings};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

use crate::state::CoreState;

/// JSON Schema `$defs` names for the kernel catalog.
///
/// Later leases append new names here and register the matching DTO. Existing
/// definition objects must stay byte-identical.
pub const CATALOG_TYPE_NAMES: &[&str] = &[
    "ControlRevision",
    "MutationAck",
    "MutationExpectation",
    "PricingRevision",
    "V3Error",
];

pub const ERROR_UNAUTHORIZED: &str = "unauthorized";
pub const ERROR_INVALID_JSON: &str = "invalidJson";
pub const ERROR_MISSING_EXPECTED_REVISION: &str = "missingExpectedRevision";
pub const ERROR_REVISION_CONFLICT: &str = "revisionConflict";

/// Live CAS token, process generation, and pricing snapshot id.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[schemars(rename_all = "camelCase", deny_unknown_fields)]
pub struct ControlRevision {
    pub revision: u64,
    pub process_generation: u64,
    pub pricing_revision: String,
}

impl ControlRevision {
    pub fn from_state(state: &CoreState) -> Self {
        Self {
            revision: state.settings_revision(),
            process_generation: state.process_generation(),
            pricing_revision: state.pricing_snapshot().revision.clone(),
        }
    }
}

/// Required mutation precondition. Wire field is `expectedRevision`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[schemars(rename_all = "camelCase")]
pub struct MutationExpectation {
    pub expected_revision: u64,
}

/// Successful control-plane mutation acknowledgement.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[schemars(rename_all = "camelCase", deny_unknown_fields)]
pub struct MutationAck {
    pub revision: u64,
    pub process_generation: u64,
}

/// Pricing snapshot identity. Distinct from the u64 settings CAS token.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[schemars(rename_all = "camelCase", deny_unknown_fields)]
pub struct PricingRevision {
    pub pricing_revision: String,
}

/// Stable non-2xx JSON envelope for every Dashboard V3 error.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[schemars(rename_all = "camelCase", deny_unknown_fields)]
pub struct V3Error {
    pub code: String,
    pub message: String,
    pub current_revision: Option<u64>,
    pub process_generation: Option<u64>,
}

impl V3Error {
    pub fn unauthorized() -> Self {
        Self {
            code: ERROR_UNAUTHORIZED.to_string(),
            message: "dashboard session is required".to_string(),
            current_revision: None,
            process_generation: None,
        }
    }

    pub fn invalid_json() -> Self {
        Self {
            code: ERROR_INVALID_JSON.to_string(),
            message: "request body must be valid JSON".to_string(),
            current_revision: None,
            process_generation: None,
        }
    }

    pub fn missing_expected_revision() -> Self {
        Self {
            code: ERROR_MISSING_EXPECTED_REVISION.to_string(),
            message: "expectedRevision is required".to_string(),
            current_revision: None,
            process_generation: None,
        }
    }

    pub fn revision_conflict(current_revision: u64, process_generation: u64) -> Self {
        Self {
            code: ERROR_REVISION_CONFLICT.to_string(),
            message: "settings changed since they were loaded; reload and try again".to_string(),
            current_revision: Some(current_revision),
            process_generation: Some(process_generation),
        }
    }
}

/// Deterministic JSON Schema catalog for the checked-in V3 contract.
///
/// Response types are generated with the serialize contract so `Option` fields
/// stay required `T | null`. Request types use the deserialize contract so
/// optional fields may be omitted. Adding a DTO later must append a `$defs`
/// entry without renaming existing definitions.
pub fn contract_schema() -> Value {
    let mut serialize = SchemaSettings::draft2020_12()
        .for_serialize()
        .into_generator();
    include_type::<ControlRevision>(&mut serialize);
    include_type::<MutationAck>(&mut serialize);
    include_type::<PricingRevision>(&mut serialize);
    include_type::<V3Error>(&mut serialize);
    let mut defs = serialize.take_definitions(true);

    let mut deserialize = SchemaSettings::draft2020_12().into_generator();
    include_type::<MutationExpectation>(&mut deserialize);
    for (name, schema) in deserialize.take_definitions(true) {
        defs.entry(name).or_insert(schema);
    }

    for name in CATALOG_TYPE_NAMES {
        if !defs.contains_key(*name) {
            panic!("dashboard v3 schema catalog is missing $defs/{name}");
        }
    }

    json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "title": "DashboardApiV3",
        "$comment": "Extensible Dashboard V3 contract catalog. Add new $defs for later DTOs; do not rename or reshape existing definitions. ConnectionInfo is the only future plaintext Key DTO.",
        "anyOf": catalog_refs(&defs),
        "$defs": defs,
    })
}

/// Pretty-printed catalog JSON with a trailing newline.
pub fn contract_schema_pretty() -> String {
    let mut encoded = serde_json::to_string_pretty(&contract_schema())
        .expect("dashboard v3 schema should serialize");
    if !encoded.ends_with('\n') {
        encoded.push('\n');
    }
    encoded
}

fn include_type<T: JsonSchema>(generator: &mut SchemaGenerator) {
    generator.subschema_for::<T>();
}

fn catalog_refs(defs: &Map<String, Value>) -> Vec<Value> {
    CATALOG_TYPE_NAMES
        .iter()
        .filter(|name| defs.contains_key(**name))
        .map(|name| json!({ "$ref": format!("#/$defs/{name}") }))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn wire_fields_are_camel_case() {
        let revision = ControlRevision {
            revision: 7,
            process_generation: 9,
            pricing_revision: "seed".into(),
        };
        assert_eq!(
            serde_json::to_value(&revision).unwrap(),
            json!({
                "revision": 7,
                "processGeneration": 9,
                "pricingRevision": "seed",
            })
        );

        let parsed: MutationExpectation =
            serde_json::from_value(json!({ "expectedRevision": 3 })).unwrap();
        assert_eq!(parsed.expected_revision, 3);
        assert!(
            serde_json::from_value::<MutationExpectation>(json!({ "expected_revision": 3 }))
                .is_err()
        );
    }

    #[test]
    fn error_envelope_always_emits_nullable_fields() {
        let error = V3Error::missing_expected_revision();
        let value = serde_json::to_value(&error).unwrap();
        assert_eq!(value["code"], "missingExpectedRevision");
        assert_eq!(value["currentRevision"], Value::Null);
        assert_eq!(value["processGeneration"], Value::Null);
        assert!(!value.as_object().unwrap().contains_key("current_revision"));
    }

    #[test]
    fn schema_catalog_is_extensible_and_names_kernel_types() {
        let schema = contract_schema();
        let defs = schema["$defs"].as_object().expect("catalog $defs");
        for name in CATALOG_TYPE_NAMES {
            assert!(defs.contains_key(*name), "missing {name}");
        }
        let required_error = defs["V3Error"]["required"]
            .as_array()
            .expect("V3Error.required");
        for field in ["code", "message", "currentRevision", "processGeneration"] {
            assert!(
                required_error.iter().any(|value| value == field),
                "{field} must stay required so responses emit T|null"
            );
        }
        let expectation_required = defs["MutationExpectation"]["required"]
            .as_array()
            .expect("MutationExpectation.required");
        assert_eq!(expectation_required, &vec![json!("expectedRevision")]);
        assert_eq!(schema["title"], "DashboardApiV3");
    }
}
