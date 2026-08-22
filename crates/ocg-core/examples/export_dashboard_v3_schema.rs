//! Print the Dashboard V3 JSON Schema catalog to stdout.
//!
//! Used by `scripts/dashboard-v3-contract.mjs`. Output is deterministic.

fn main() {
    print!("{}", ocg_core::dashboard_v3::contract_schema_pretty());
}
