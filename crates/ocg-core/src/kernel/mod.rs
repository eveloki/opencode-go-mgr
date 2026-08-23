//! I/O-free Stage 1 kernels: identities, protocol catalogs, pricing types,
//! and Zen catalog parse/normalize.
//!
//! These modules must not import db, state, dashboard, gateway execution,
//! reqwest, rusqlite, tokio, filesystem, clocks, or process/host code.
//! Existing public paths keep compatibility re-exports on the original
//! modules.

pub mod catalog;
pub mod ids;
pub mod pricing;
pub mod protocol;
pub mod zen;

#[cfg(test)]
mod dependency_guard {
    use std::collections::{BTreeMap, BTreeSet};
    use std::fs;
    use std::path::{Path, PathBuf};

    const KERNEL_FILES: &[&str] = &[
        "catalog.rs",
        "ids.rs",
        "pricing.rs",
        "protocol.rs",
        "zen.rs",
    ];

    const FORBIDDEN_USE_PREFIXES: &[&str] = &[
        "use crate::db",
        "use crate::state",
        "use crate::dashboard",
        "use crate::host_router",
        "use crate::host_gateway",
        "use crate::gateway_runtime",
        "use crate::routing_runtime",
        "use crate::gateway",
        "use crate::http_client",
        "use crate::custom",
        "use crate::custom_http",
        "use crate::auth",
        "use crate::browser",
        "use crate::console_usage",
        "use crate::go_usage",
        "use crate::usage_sync",
        "use crate::gateway_keys",
        "use reqwest",
        "use rusqlite",
        "use tokio",
        "use std::fs",
        "use std::process",
    ];

    const FORBIDDEN_KERNEL_CRATE_MODULES: &[&str] = &[
        "db",
        "state",
        "dashboard",
        "host_router",
        "host_gateway",
        "gateway_runtime",
        "routing_runtime",
        "gateway",
        "http_client",
        "custom",
        "custom_http",
        "auth",
        "browser",
        "console_usage",
        "go_usage",
        "usage_sync",
        "gateway_keys",
        "pricing",
    ];

    const EXPECTED_HOST_SCC: &[&str] = &[];

    #[test]
    fn kernel_modules_do_not_import_io_or_control_plane() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/kernel");
        for name in KERNEL_FILES {
            let path = root.join(name);
            let source = fs::read_to_string(&path)
                .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
            let production = production_source(&source);
            for line in production.lines() {
                let trimmed = line.trim();
                if trimmed.starts_with("use ") {
                    for prefix in FORBIDDEN_USE_PREFIXES {
                        assert!(
                            !trimmed.starts_with(prefix),
                            "{} imports I/O or control-plane code: {trimmed}",
                            path.display()
                        );
                    }
                }
            }
            for module in crate_path_roots(&production) {
                assert!(
                    !FORBIDDEN_KERNEL_CRATE_MODULES.contains(&module.as_str()),
                    "{} has a qualified production path into `{module}`",
                    path.display()
                );
            }
            for needle in ["Utc::now", "Instant::now", "SystemTime::now"] {
                assert!(
                    !production.contains(needle),
                    "{} must not read a clock (`{needle}`)",
                    path.display()
                );
            }
        }
    }

    #[test]
    fn alias_does_not_import_custom_module() {
        let src_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src");
        let alias = production_source(&read_to_string(&src_root.join("alias.rs")));
        assert!(
            !alias_imports_custom(&alias),
            "alias.rs must not import or reach crate::custom; use kernel::ids for the shared matcher"
        );
        assert!(
            alias.contains("kernel::ids") && alias.contains("custom_model_id_matches"),
            "alias.rs must use the shared matcher from kernel::ids"
        );
    }

    #[test]
    fn alias_custom_import_guard_catches_supported_forms() {
        for source in [
            "use crate::custom::custom_model_id_matches;",
            "use crate::{kernel::ids::looks_raw_shaped, custom::custom_model_id_matches};",
            "use super::custom::custom_model_id_matches;",
            "use super::{kernel::ids::looks_raw_shaped, custom::custom_model_id_matches};",
        ] {
            assert!(
                alias_imports_custom(source),
                "alias->custom guard must catch supported import form: {source}"
            );
        }
        assert!(
            !alias_imports_custom("use crate::kernel::ids::custom_model_id_matches;"),
            "the narrow guard must allow the kernel matcher import"
        );
    }

    #[test]
    fn contract_and_v3_account_sources_do_not_import_gateway_utilities() {
        let src_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src");
        for relative in ["provider_contracts.rs", "dashboard_v3/accounts.rs"] {
            let path = src_root.join(relative);
            let production = production_source(&read_to_string(&path));
            assert!(
                !crate_path_roots(&production).contains("gateway"),
                "{relative} production source must not contain crate::gateway"
            );
            for line in production.lines() {
                let trimmed = line.trim();
                if trimmed.starts_with("use ") {
                    assert!(
                        !trimmed.starts_with("use crate::gateway"),
                        "{relative} imports gateway: {trimmed}"
                    );
                }
            }
        }
    }

    #[test]
    fn redaction_module_is_a_pure_dag_leaf() {
        let src_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src");
        let path = src_root.join("redaction.rs");
        let production = production_source(&read_to_string(&path));
        for line in production.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("use ") {
                for prefix in FORBIDDEN_USE_PREFIXES {
                    assert!(
                        !trimmed.starts_with(prefix),
                        "redaction.rs imports I/O or control-plane code: {trimmed}"
                    );
                }
            }
        }
        for module in crate_path_roots(&production) {
            assert!(
                !FORBIDDEN_KERNEL_CRATE_MODULES.contains(&module.as_str()),
                "redaction.rs has a qualified production path into `{module}`"
            );
            assert_ne!(module, "gateway", "redaction.rs must not depend on gateway");
        }
        for needle in ["Utc::now", "Instant::now", "SystemTime::now"] {
            assert!(
                !production.contains(needle),
                "redaction.rs must not read a clock (`{needle}`)"
            );
        }
        assert!(
            crate_path_roots(&production).is_empty(),
            "redaction.rs must remain a crate-level DAG leaf, got {:?}",
            crate_path_roots(&production)
        );
    }

    #[test]
    fn upstream_limit_is_a_pure_dag_leaf_consumed_by_dashboard_without_gateway() {
        let src_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src");
        let path = src_root.join("upstream_limit.rs");
        let production = production_source(&read_to_string(&path));
        for line in production.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("use ") {
                for prefix in FORBIDDEN_USE_PREFIXES {
                    assert!(
                        !trimmed.starts_with(prefix),
                        "upstream_limit.rs imports I/O or control-plane code: {trimmed}"
                    );
                }
            }
        }
        let roots = crate_path_roots(&production);
        for module in &roots {
            assert!(
                !FORBIDDEN_KERNEL_CRATE_MODULES.contains(&module.as_str()),
                "upstream_limit.rs has a qualified production path into `{module}`"
            );
            assert_ne!(
                module.as_str(),
                "gateway",
                "upstream_limit.rs must not depend on gateway"
            );
            assert_ne!(
                module.as_str(),
                "dashboard",
                "upstream_limit.rs must not depend on dashboard"
            );
            assert_ne!(
                module.as_str(),
                "dashboard_v3",
                "upstream_limit.rs must not depend on dashboard_v3"
            );
            assert_ne!(
                module.as_str(),
                "state",
                "upstream_limit.rs must not depend on state"
            );
            assert_ne!(
                module.as_str(),
                "db",
                "upstream_limit.rs must not depend on db"
            );
            assert_ne!(
                module.as_str(),
                "provider",
                "upstream_limit.rs must not depend on provider adapters"
            );
        }
        assert_eq!(
            roots,
            named_set(&["models"]),
            "upstream_limit.rs may depend only on models for UsageWindowKind, got {roots:?}"
        );
        for needle in ["Utc::now", "Instant::now", "SystemTime::now"] {
            assert!(
                !production.contains(needle),
                "upstream_limit.rs must not read a clock (`{needle}`)"
            );
        }

        let dashboard = production_source(&read_to_string(&src_root.join("dashboard.rs")));
        assert!(
            crate_path_roots(&dashboard).contains("upstream_limit"),
            "dashboard production source must consume crate::upstream_limit directly"
        );
        assert!(
            !dashboard.contains("gateway::limit"),
            "dashboard must not reach limit parsers through crate::gateway"
        );
        for line in dashboard.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("use ") {
                assert!(
                    !trimmed.contains("gateway::limit")
                        && !trimmed.starts_with("use crate::gateway::limit"),
                    "dashboard imports gateway limit parsers: {trimmed}"
                );
            }
        }
    }

    #[test]
    fn production_graph_has_the_expected_remaining_scc() {
        let src_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src");
        let lib_source = production_source(&read_to_string(&src_root.join("lib.rs")));
        let modules = declared_modules(&lib_source);
        assert!(
            modules.contains("db")
                && modules.contains("pricing")
                && modules.contains("kernel")
                && modules.contains("redaction")
                && modules.contains("upstream_limit")
                && modules.contains("host_router")
                && modules.contains("host_gateway")
                && modules.contains("gateway_runtime")
                && modules.contains("routing_runtime"),
            "lib.rs should declare the production modules under test, got {modules:?}"
        );

        let db_source = production_source(&read_to_string(&src_root.join("db.rs")));
        assert!(
            !crate_path_roots(&db_source).contains("pricing"),
            "db production source must not reference the clocked pricing module"
        );
        assert!(
            !crate_path_roots(&db_source).contains("gateway_keys"),
            "db production source must not reference gateway_keys"
        );
        assert!(
            db_source.contains("CURRENT_SCHEMA_VERSION: i32 = 26"),
            "schema version must remain 26"
        );

        let graph = production_graph(&src_root, &modules);
        assert!(
            EXPECTED_HOST_SCC.is_empty(),
            "Phase 1 cut must not whitelist a multi-node host SCC"
        );
        assert!(
            !graph
                .get("provider")
                .is_some_and(|edges| edges.contains("go_usage")),
            "provider catalog facts must not depend on the Go usage HTTP client"
        );
        let db_component = tarjan(&graph)
            .into_iter()
            .find(|component| component.contains("db"))
            .expect("db module should exist in the production graph");
        assert_eq!(
            db_component.len(),
            1,
            "db must not remain in a production SCC after the contract/redaction inversion, db_component={db_component:?}"
        );
        assert!(
            !graph
                .get("db")
                .is_some_and(|edges| edges.contains("pricing")),
            "db must not depend on pricing"
        );
        assert!(
            !graph
                .get("db")
                .is_some_and(|edges| edges.contains("gateway_keys")),
            "db must not depend on gateway_keys"
        );
        assert!(
            !graph
                .get("gateway_keys")
                .is_some_and(|edges| edges.contains("state") || edges.contains("db")),
            "gateway_keys must not depend on state or db"
        );
        assert!(
            !graph
                .get("usage_sync")
                .is_some_and(|edges| edges.contains("state") || edges.contains("db")),
            "usage_sync must not depend on state or db"
        );
        assert!(
            !graph
                .get("provider_contracts")
                .is_some_and(|edges| edges.contains("gateway")),
            "provider_contracts must not depend on gateway"
        );
        assert!(
            !graph
                .get("dashboard_v3")
                .is_some_and(|edges| edges.contains("gateway") || edges.contains("dashboard")),
            "dashboard_v3 must not depend on gateway or dashboard"
        );
        assert!(
            !graph.get("protocol_probe").is_some_and(|edges| {
                edges.contains("dashboard") || edges.contains("dashboard_v3")
            }),
            "protocol_probe must not depend on dashboard or dashboard_v3"
        );
        assert!(
            graph
                .get("protocol_probe")
                .is_some_and(|edges| { edges.contains("gateway") && edges.contains("state") }),
            "protocol_probe still depends on gateway/state for probe transport"
        );
        assert!(
            !graph.get("gateway").is_some_and(|edges| {
                edges.contains("dashboard")
                    || edges.contains("dashboard_v3")
                    || edges.contains("host_router")
                    || edges.contains("host_gateway")
                    || edges.contains("protocol_probe")
            }),
            "gateway must not import dashboard mounts, host composition, or protocol_probe, graph={graph:?}"
        );
        assert!(
            !graph.get("state").is_some_and(|edges| {
                edges.contains("dashboard")
                    || edges.contains("dashboard_v3")
                    || edges.contains("host_router")
                    || edges.contains("protocol_probe")
            }),
            "state must not import dashboard mounts, host_router, or protocol_probe, graph={graph:?}"
        );
        assert!(
            !graph.get("state").is_some_and(|edges| {
                edges.contains("gateway") || edges.contains("host_gateway")
            }),
            "state must not depend on gateway or the host rebind adapter, graph={graph:?}"
        );
        assert!(
            graph
                .get("gateway")
                .is_some_and(|edges| edges.contains("state")),
            "gateway -> state may remain one-way after the Phase 1 cut, graph={graph:?}"
        );
        assert!(
            graph.get("state").is_some_and(|edges| {
                edges.contains("gateway_runtime") && edges.contains("routing_runtime")
            }),
            "state must own the extracted runtime slots without a gateway edge, graph={graph:?}"
        );
        assert!(
            graph
                .get("host_gateway")
                .is_some_and(|edges| { edges.contains("gateway") && edges.contains("state") }),
            "host_gateway must adapt gateway lifecycle onto state, graph={graph:?}"
        );
        assert!(
            !graph.get("host_gateway").is_some_and(|edges| {
                edges.contains("dashboard")
                    || edges.contains("dashboard_v3")
                    || edges.contains("protocol_probe")
                    || edges.contains("usage_sync")
                    || edges.contains("db")
                    || edges.contains("http_client")
            }),
            "host_gateway must stay a rebind adapter, graph={graph:?}"
        );
        assert!(
            !graph.get("gateway_runtime").is_some_and(|edges| {
                edges.contains("gateway")
                    || edges.contains("state")
                    || edges.contains("host_gateway")
                    || edges.contains("host_router")
            }),
            "gateway_runtime must stay outside state/gateway, graph={graph:?}"
        );
        assert!(
            !graph.get("routing_runtime").is_some_and(|edges| {
                edges.contains("gateway")
                    || edges.contains("state")
                    || edges.contains("host_gateway")
                    || edges.contains("host_router")
            }),
            "routing_runtime must stay outside state/gateway, graph={graph:?}"
        );
        assert!(
            graph.get("host_router").is_some_and(|edges| {
                edges.contains("dashboard")
                    && edges.contains("dashboard_v3")
                    && edges.contains("gateway")
            }),
            "host_router must compose dashboard mounts onto gateway, graph={graph:?}"
        );
        assert!(
            !graph.get("host_router").is_some_and(|edges| {
                edges.contains("protocol_probe")
                    || edges.contains("usage_sync")
                    || edges.contains("db")
                    || edges.contains("http_client")
            }),
            "host_router must stay composition-only, graph={graph:?}"
        );
        let protocol_probe_source =
            production_source(&read_to_string(&src_root.join("protocol_probe.rs")));
        for needle in [
            "forward_once",
            "client_for",
            "gateway::executor",
            "gateway::forwarder",
        ] {
            assert!(
                !protocol_probe_source.contains(needle),
                "protocol_probe must not call {needle}"
            );
        }
        assert_eq!(
            graph.get("redaction").cloned().unwrap_or_default(),
            BTreeSet::new(),
            "redaction must be a production DAG leaf, graph={graph:?}"
        );
        assert_eq!(
            graph.get("upstream_limit").cloned().unwrap_or_default(),
            named_set(&["models"]),
            "upstream_limit must stay an I/O-free DAG leaf beside models, graph={graph:?}"
        );
        assert!(
            graph
                .get("dashboard")
                .is_some_and(|edges| edges.contains("upstream_limit")),
            "dashboard must consume upstream_limit without a gateway parser edge"
        );
        assert!(
            graph
                .get("gateway")
                .is_some_and(|edges| edges.contains("upstream_limit")),
            "gateway must still depend on the extracted upstream_limit leaf"
        );
        assert!(
            !graph.get("upstream_limit").is_some_and(|edges| {
                edges.contains("gateway")
                    || edges.contains("dashboard")
                    || edges.contains("dashboard_v3")
                    || edges.contains("state")
                    || edges.contains("db")
                    || edges.contains("provider")
            }),
            "upstream_limit must not grow a control-plane or adapter edge, graph={graph:?}"
        );

        let gateway_keys_source =
            production_source(&read_to_string(&src_root.join("gateway_keys.rs")));
        let usage_sync_source = production_source(&read_to_string(&src_root.join("usage_sync.rs")));
        let account_control_source =
            production_source(&read_to_string(&src_root.join("account_control.rs")));
        for (name, source) in [
            ("gateway_keys.rs", &gateway_keys_source),
            ("usage_sync.rs", &usage_sync_source),
            ("account_control.rs", &account_control_source),
        ] {
            assert!(
                !source.contains("crate::state"),
                "{name} production source must not import crate::state"
            );
            assert!(
                !source.contains("CoreState"),
                "{name} production source must not name CoreState"
            );
        }
        for forbidden in ["state", "dashboard", "dashboard_v3", "gateway"] {
            assert!(
                !crate_path_roots(&account_control_source).contains(forbidden),
                "account_control production source must not import {forbidden}"
            );
        }
        assert!(
            !graph.get("account_control").is_some_and(|edges| {
                edges.contains("state")
                    || edges.contains("dashboard")
                    || edges.contains("dashboard_v3")
                    || edges.contains("gateway")
            }),
            "account_control must not depend on host SCC modules, graph={graph:?}"
        );
        let account_control_component = tarjan(&graph)
            .into_iter()
            .find(|component| component.contains("account_control"))
            .expect("account_control module should exist in the production graph");
        assert_eq!(
            account_control_component.len(),
            1,
            "account_control must not join a production SCC, account_control_component={account_control_component:?}"
        );

        // The Phase 1 cut moved GatewayHandle and RoutingRuntime out of both
        // `gateway` and `state`. Settings rebind goes through GatewayRebindHost,
        // implemented only in host_gateway. gateway -> state remains one-way.
        // HTTP router composition stays in host_router. Tarjan runs against the
        // complete measured production graph; any multi-node SCC is a failure.
        let mut nontrivial: Vec<BTreeSet<String>> = tarjan(&graph)
            .into_iter()
            .filter(|component| component.len() > 1)
            .collect();
        nontrivial.sort();
        assert!(
            nontrivial.is_empty(),
            "production graph must have no multi-node SCC, sccs={nontrivial:?}, graph={graph:?}"
        );
        for name in [
            "gateway",
            "state",
            "dashboard",
            "dashboard_v3",
            "protocol_probe",
            "host_router",
            "host_gateway",
            "gateway_runtime",
            "routing_runtime",
        ] {
            let component = tarjan(&graph)
                .into_iter()
                .find(|component| component.contains(name))
                .unwrap_or_else(|| panic!("{name} module should exist in the production graph"));
            assert_eq!(
                component.len(),
                1,
                "{name} must not remain in a multi-node production SCC, component={component:?}, graph={graph:?}"
            );
        }
    }

    #[test]
    fn host_router_is_composition_only() {
        let src_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src");
        let host_router_raw = read_to_string(&src_root.join("host_router.rs"));
        let host_router = production_source(&host_router_raw);
        let gateway_mod = production_source(&read_to_string(&src_root.join("gateway/mod.rs")));
        let listener = production_source(&read_to_string(&src_root.join("gateway/listener.rs")));

        assert!(
            host_router_raw.contains("\"/dashboard/api/v3\"")
                && host_router_raw.contains("\"/dashboard/api\"")
                && host_router_raw.contains("\"/dashboard\"")
                && host_router_raw.contains("\"/dashboard/\"")
                && host_router_raw.contains("\"/dashboard/assets/{*path}\"")
                && host_router.contains("dashboard_v3::api_router")
                && host_router.contains("dashboard::api_router")
                && host_router.contains("dashboard::serve_index")
                && host_router.contains("dashboard::serve_asset")
                && host_router.contains("inference_router")
                && host_router.contains("fn build_router")
                && host_router.contains("impl GatewayRouterHost for CoreState")
                && host_router.contains("fn compose_router"),
            "host_router must own the dashboard mounts and inference merge"
        );
        assert!(
            !host_router.contains("impl GatewayLifecycle"),
            "host_router must not add inherent GatewayLifecycle methods"
        );
        for needle in [
            "forward_once",
            "GatewayExecutor",
            "axum::serve",
            "TcpListener",
            "start_gateway",
            "ControlPlaneWorkers",
            "protocol_probe",
        ] {
            assert!(
                !host_router.contains(needle),
                "host_router must not take on runtime work ({needle})"
            );
        }

        let host_roots = crate_path_roots(&host_router);
        for required in ["dashboard", "dashboard_v3", "gateway"] {
            assert!(
                host_roots.contains(required),
                "host_router must depend on {required}, roots={host_roots:?}"
            );
        }
        for forbidden in [
            "protocol_probe",
            "usage_sync",
            "db",
            "http_client",
            "provider_contracts",
        ] {
            assert!(
                !host_roots.contains(forbidden),
                "host_router must not depend on {forbidden}, roots={host_roots:?}"
            );
        }

        assert!(
            gateway_mod.contains("fn inference_router")
                && !gateway_mod.contains("crate::dashboard")
                && !crate_path_roots(&gateway_mod).contains("dashboard")
                && !crate_path_roots(&gateway_mod).contains("dashboard_v3")
                && !crate_path_roots(&gateway_mod).contains("host_router"),
            "gateway inference assembly must not mount dashboards"
        );
        assert!(
            listener.contains("GatewayRouterHost")
                && listener.contains("GatewayRouterHost>::compose_router")
                && !crate_path_roots(&listener).contains("dashboard")
                && !crate_path_roots(&listener).contains("dashboard_v3")
                && !crate_path_roots(&listener).contains("host_router"),
            "listener must consume composed routes through GatewayRouterHost without importing dashboard mounts"
        );

        let mut gateway_source = String::new();
        visit_rust_files(&src_root.join("gateway"), &mut |path| {
            gateway_source.push_str(&production_source(&read_to_string(path)));
            gateway_source.push('\n');
        });
        let gateway_roots = crate_path_roots(&gateway_source);
        for forbidden in [
            "dashboard",
            "dashboard_v3",
            "host_router",
            "host_gateway",
            "protocol_probe",
        ] {
            assert!(
                !gateway_roots.contains(forbidden),
                "gateway production sources must not import {forbidden}, roots={gateway_roots:?}"
            );
        }
    }

    #[test]
    fn host_gateway_is_rebind_adapter() {
        let src_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src");
        let host_gateway_raw = read_to_string(&src_root.join("host_gateway.rs"));
        let host_gateway = production_source(&host_gateway_raw);
        let state = production_source(&read_to_string(&src_root.join("state.rs")));
        let runtime = production_source(&read_to_string(&src_root.join("gateway_runtime.rs")));

        assert!(
            host_gateway.contains("impl GatewayRebindHost for CoreState")
                && host_gateway.contains("GatewayLifecycle::rebind")
                && host_gateway.contains("rebind_from_serving_request"),
            "host_gateway must implement the rebind port against GatewayLifecycle"
        );
        assert!(
            !host_gateway.contains("impl GatewayLifecycle"),
            "host_gateway must not add inherent GatewayLifecycle methods"
        );
        for needle in [
            "forward_once",
            "GatewayExecutor",
            "axum::serve",
            "TcpListener",
            "start_gateway",
            "ControlPlaneWorkers",
            "protocol_probe",
            "inference_router",
            "dashboard::",
        ] {
            assert!(
                !host_gateway.contains(needle),
                "host_gateway must not take on runtime or dashboard work ({needle})"
            );
        }

        let host_roots = crate_path_roots(&host_gateway);
        for required in ["gateway", "state"] {
            assert!(
                host_roots.contains(required),
                "host_gateway must depend on {required}, roots={host_roots:?}"
            );
        }
        for forbidden in [
            "dashboard",
            "dashboard_v3",
            "protocol_probe",
            "usage_sync",
            "db",
            "http_client",
            "provider_contracts",
        ] {
            assert!(
                !host_roots.contains(forbidden),
                "host_gateway must not depend on {forbidden}, roots={host_roots:?}"
            );
        }

        assert!(
            state.contains("GatewayRebindHost::rebind")
                && state.contains("rebind_from_serving_request")
                && state.contains("rebind_gateway_listener_if_port_changed"),
            "state must rebind through GatewayRebindHost"
        );
        assert!(
            !crate_path_roots(&state).contains("gateway"),
            "state production source must not have a crate::gateway edge"
        );
        assert!(
            !crate_path_roots(&state).contains("host_gateway"),
            "state must not import the host rebind adapter"
        );
        assert!(
            runtime.contains("pub trait GatewayRebindHost")
                && runtime.contains("pub struct GatewayHandle")
                && crate_path_roots(&runtime).is_empty(),
            "gateway_runtime must stay a crate-level DAG leaf, roots={:?}",
            crate_path_roots(&runtime)
        );
    }

    fn named_set(names: &[&str]) -> BTreeSet<String> {
        names.iter().map(|name| (*name).to_string()).collect()
    }

    fn production_graph(
        src_root: &Path,
        modules: &BTreeSet<String>,
    ) -> BTreeMap<String, BTreeSet<String>> {
        let mut graph: BTreeMap<String, BTreeSet<String>> = modules
            .iter()
            .cloned()
            .map(|name| (name, BTreeSet::new()))
            .collect();
        visit_rust_files(src_root, &mut |path| {
            let Some(from) = module_of(src_root, path, modules) else {
                return;
            };
            let production = production_source(&read_to_string(path));
            for target in crate_path_roots(&production) {
                if target != from && modules.contains(&target) {
                    graph.entry(from.clone()).or_default().insert(target);
                }
            }
        });
        graph
    }

    fn declared_modules(lib_source: &str) -> BTreeSet<String> {
        let mut modules = BTreeSet::new();
        let chars: Vec<char> = lib_source.chars().collect();
        let mut i = 0;
        while i < chars.len() {
            skip_ws_idx(&chars, &mut i);
            if match_keyword(&chars, i, "pub") {
                i += 3;
                skip_ws_idx(&chars, &mut i);
                if match_prefix(&chars, i, "(crate)") {
                    i += "(crate)".len();
                    skip_ws_idx(&chars, &mut i);
                }
            }
            if match_keyword(&chars, i, "mod") {
                i += 3;
                skip_ws_idx(&chars, &mut i);
                if let Some(name) = take_ident_idx(&chars, &mut i) {
                    skip_ws_idx(&chars, &mut i);
                    if i < chars.len() && chars[i] == ';' {
                        modules.insert(name);
                    }
                }
                continue;
            }
            i += 1;
        }
        modules
    }

    fn module_of(src_root: &Path, path: &Path, modules: &BTreeSet<String>) -> Option<String> {
        let rel = path.strip_prefix(src_root).ok()?;
        let mut components = rel.components();
        let first = components.next()?.as_os_str().to_string_lossy();
        let name = first.strip_suffix(".rs").unwrap_or(&first);
        if name == "lib" {
            return None;
        }
        modules.contains(name).then(|| name.to_string())
    }

    fn visit_rust_files(root: &Path, visit: &mut impl FnMut(&Path)) {
        let mut stack = vec![root.to_path_buf()];
        while let Some(dir) = stack.pop() {
            let entries = fs::read_dir(&dir)
                .unwrap_or_else(|error| panic!("read {}: {error}", dir.display()));
            for entry in entries {
                let entry = entry.unwrap_or_else(|error| panic!("dir entry: {error}"));
                let path = entry.path();
                if path.is_dir() {
                    stack.push(path);
                } else if path.extension().and_then(|ext| ext.to_str()) == Some("rs") {
                    visit(&path);
                }
            }
        }
    }

    fn production_source(source: &str) -> String {
        strip_cfg_test_items(&strip_comments_and_strings(source))
    }

    fn strip_comments_and_strings(source: &str) -> String {
        let chars: Vec<char> = source.chars().collect();
        let mut out = String::with_capacity(chars.len());
        let mut i = 0;
        while i < chars.len() {
            let ch = chars[i];
            let next = chars.get(i + 1).copied();
            if ch == '/' && next == Some('/') {
                while i < chars.len() && chars[i] != '\n' {
                    out.push(' ');
                    i += 1;
                }
                continue;
            }
            if ch == '/' && next == Some('*') {
                let mut depth = 1;
                out.push(' ');
                out.push(' ');
                i += 2;
                while i < chars.len() && depth > 0 {
                    if chars[i] == '/' && chars.get(i + 1) == Some(&'*') {
                        depth += 1;
                        out.push(' ');
                        out.push(' ');
                        i += 2;
                        continue;
                    }
                    if chars[i] == '*' && chars.get(i + 1) == Some(&'/') {
                        depth -= 1;
                        out.push(' ');
                        out.push(' ');
                        i += 2;
                        continue;
                    }
                    out.push(if chars[i] == '\n' { '\n' } else { ' ' });
                    i += 1;
                }
                continue;
            }
            if (ch == 'r' || (ch == 'b' && next == Some('r'))) && looks_like_raw_string(&chars, i) {
                let start = if ch == 'b' { i + 1 } else { i };
                let mut hashes = 0;
                let mut j = start + 1;
                while j < chars.len() && chars[j] == '#' {
                    hashes += 1;
                    j += 1;
                }
                while i < j + 1 {
                    out.push(' ');
                    i += 1;
                }
                while i < chars.len() {
                    if chars[i] == '"' && raw_string_end(&chars, i + 1, hashes) {
                        out.push(' ');
                        i += 1;
                        for _ in 0..hashes {
                            out.push(' ');
                            i += 1;
                        }
                        break;
                    }
                    out.push(if chars[i] == '\n' { '\n' } else { ' ' });
                    i += 1;
                }
                continue;
            }
            if (ch == 'b' && next == Some('"')) || ch == '"' {
                if ch == 'b' {
                    out.push(' ');
                    i += 1;
                }
                out.push(' ');
                i += 1;
                while i < chars.len() {
                    if chars[i] == '\\' {
                        out.push(' ');
                        out.push(' ');
                        i += 2;
                        continue;
                    }
                    if chars[i] == '"' {
                        out.push(' ');
                        i += 1;
                        break;
                    }
                    out.push(if chars[i] == '\n' { '\n' } else { ' ' });
                    i += 1;
                }
                continue;
            }
            if ch == '\'' {
                if let Some(&ident_start) = chars.get(i + 1) {
                    if is_ident_start(ident_start) {
                        if chars.get(i + 2) == Some(&'\'') {
                            out.push(' ');
                            out.push(' ');
                            out.push(' ');
                            i += 3;
                            continue;
                        }
                        out.push(ch);
                        i += 1;
                        continue;
                    }
                }
                out.push(' ');
                i += 1;
                if i < chars.len() && chars[i] == '\\' {
                    out.push(' ');
                    i += 1;
                }
                if i < chars.len() {
                    out.push(if chars[i] == '\n' { '\n' } else { ' ' });
                    i += 1;
                }
                if i < chars.len() && chars[i] == '\'' {
                    out.push(' ');
                    i += 1;
                }
                continue;
            }
            out.push(ch);
            i += 1;
        }
        out
    }

    fn looks_like_raw_string(chars: &[char], i: usize) -> bool {
        let start = if chars[i] == 'b' { i + 1 } else { i };
        if start >= chars.len() || chars[start] != 'r' {
            return false;
        }
        let mut j = start + 1;
        while j < chars.len() && chars[j] == '#' {
            j += 1;
        }
        j < chars.len() && chars[j] == '"'
    }

    fn raw_string_end(chars: &[char], mut i: usize, hashes: usize) -> bool {
        for _ in 0..hashes {
            if i >= chars.len() || chars[i] != '#' {
                return false;
            }
            i += 1;
        }
        true
    }

    fn strip_cfg_test_items(source: &str) -> String {
        let chars: Vec<char> = source.chars().collect();
        let mut out = String::with_capacity(chars.len());
        let mut i = 0;
        while i < chars.len() {
            if chars[i] == '#' {
                let start = i;
                let mut cursor = i;
                let mut saw_cfg_test = false;
                loop {
                    skip_ws_idx(&chars, &mut cursor);
                    let Some(end) = parse_attribute(&chars, cursor) else {
                        break;
                    };
                    let attr: String = chars[cursor..end].iter().collect();
                    if attr_is_cfg_test(&attr) {
                        saw_cfg_test = true;
                    }
                    cursor = end;
                }
                if saw_cfg_test {
                    let end = skip_item(&chars, cursor);
                    for ch in &chars[start..end] {
                        out.push(if *ch == '\n' { '\n' } else { ' ' });
                    }
                    i = end;
                    continue;
                }
            }
            out.push(chars[i]);
            i += 1;
        }
        out
    }

    fn parse_attribute(chars: &[char], i: usize) -> Option<usize> {
        if i >= chars.len() || chars[i] != '#' {
            return None;
        }
        let mut j = i + 1;
        if j < chars.len() && chars[j] == '!' {
            j += 1;
        }
        skip_ws_idx(chars, &mut j);
        if j >= chars.len() || chars[j] != '[' {
            return None;
        }
        let mut depth = 0;
        while j < chars.len() {
            match chars[j] {
                '[' => depth += 1,
                ']' => {
                    depth -= 1;
                    if depth == 0 {
                        return Some(j + 1);
                    }
                }
                _ => {}
            }
            j += 1;
        }
        None
    }

    fn attr_is_cfg_test(attr: &str) -> bool {
        let compact: String = attr.chars().filter(|ch| !ch.is_whitespace()).collect();
        if compact.contains("cfg(not(test)") {
            return false;
        }
        compact.contains("cfg(test)")
            || compact.contains("cfg(all(test")
            || compact.contains("cfg(any(test")
    }

    fn skip_item(chars: &[char], mut i: usize) -> usize {
        skip_ws_idx(chars, &mut i);
        let mut paren = 0i32;
        while i < chars.len() {
            match chars[i] {
                '(' => paren += 1,
                ')' => paren = paren.saturating_sub(1),
                '{' if paren == 0 => return skip_balanced(chars, i, '{', '}'),
                ';' if paren == 0 => return i + 1,
                _ => {}
            }
            i += 1;
        }
        chars.len()
    }

    fn skip_balanced(chars: &[char], mut i: usize, open: char, close: char) -> usize {
        let mut depth = 0;
        while i < chars.len() {
            if chars[i] == open {
                depth += 1;
            } else if chars[i] == close {
                depth -= 1;
                if depth == 0 {
                    return i + 1;
                }
            }
            i += 1;
        }
        chars.len()
    }

    fn crate_path_roots(source: &str) -> BTreeSet<String> {
        let chars: Vec<char> = source.chars().collect();
        let mut roots = BTreeSet::new();
        let mut i = 0;
        while i + 7 <= chars.len() {
            if match_prefix(&chars, i, "crate::") {
                let boundary = i == 0 || !is_ident_continue(chars[i - 1]);
                if boundary {
                    i += 7;
                    if let Some(name) = take_ident_idx(&chars, &mut i) {
                        roots.insert(name);
                        continue;
                    }
                }
            }
            i += 1;
        }
        roots
    }

    // This is intentionally a narrow source guard, not Rust module resolution.
    // `production_source` removes comments, strings, and cfg(test) fixtures first.
    fn alias_imports_custom(source: &str) -> bool {
        source.split(';').any(|statement| {
            let compact: String = statement.chars().filter(|ch| !ch.is_whitespace()).collect();
            compact.contains("crate::custom")
                || compact.contains("super::custom")
                || grouped_import_contains_custom(&compact, "usecrate::{")
                || grouped_import_contains_custom(&compact, "usesuper::{")
        })
    }

    fn grouped_import_contains_custom(statement: &str, prefix: &str) -> bool {
        let Some(items) = statement.strip_prefix(prefix) else {
            return false;
        };
        items.split(',').any(|item| {
            let item = item.trim_end_matches('}');
            item == "custom" || item.starts_with("custom::")
        })
    }

    fn tarjan(graph: &BTreeMap<String, BTreeSet<String>>) -> Vec<BTreeSet<String>> {
        let mut index = 0usize;
        let mut stack = Vec::new();
        let mut indices = BTreeMap::new();
        let mut lowlink = BTreeMap::new();
        let mut on_stack = BTreeSet::new();
        let mut sccs = Vec::new();

        #[allow(clippy::too_many_arguments)]
        fn connect(
            node: &str,
            graph: &BTreeMap<String, BTreeSet<String>>,
            index: &mut usize,
            stack: &mut Vec<String>,
            indices: &mut BTreeMap<String, usize>,
            lowlink: &mut BTreeMap<String, usize>,
            on_stack: &mut BTreeSet<String>,
            sccs: &mut Vec<BTreeSet<String>>,
        ) {
            indices.insert(node.to_string(), *index);
            lowlink.insert(node.to_string(), *index);
            *index += 1;
            stack.push(node.to_string());
            on_stack.insert(node.to_string());
            for next in graph.get(node).into_iter().flatten() {
                if !indices.contains_key(next) {
                    connect(next, graph, index, stack, indices, lowlink, on_stack, sccs);
                    let next_low = lowlink[next];
                    let current = lowlink.get_mut(node).expect("lowlink");
                    *current = (*current).min(next_low);
                } else if on_stack.contains(next) {
                    let next_index = indices[next];
                    let current = lowlink.get_mut(node).expect("lowlink");
                    *current = (*current).min(next_index);
                }
            }
            if lowlink[node] == indices[node] {
                let mut component = BTreeSet::new();
                loop {
                    let item = stack.pop().expect("scc stack");
                    on_stack.remove(&item);
                    let done = item == node;
                    component.insert(item);
                    if done {
                        break;
                    }
                }
                sccs.push(component);
            }
        }

        for node in graph.keys() {
            if !indices.contains_key(node) {
                connect(
                    node,
                    graph,
                    &mut index,
                    &mut stack,
                    &mut indices,
                    &mut lowlink,
                    &mut on_stack,
                    &mut sccs,
                );
            }
        }
        sccs
    }

    fn read_to_string(path: &Path) -> String {
        fs::read_to_string(path).unwrap_or_else(|error| panic!("read {}: {error}", path.display()))
    }

    fn skip_ws_idx(chars: &[char], i: &mut usize) {
        while *i < chars.len() && chars[*i].is_whitespace() {
            *i += 1;
        }
    }

    fn take_ident_idx(chars: &[char], i: &mut usize) -> Option<String> {
        if *i >= chars.len() || !is_ident_start(chars[*i]) {
            return None;
        }
        let start = *i;
        *i += 1;
        while *i < chars.len() && is_ident_continue(chars[*i]) {
            *i += 1;
        }
        Some(chars[start..*i].iter().collect())
    }

    fn match_prefix(chars: &[char], i: usize, needle: &str) -> bool {
        for (offset, ch) in needle.chars().enumerate() {
            if chars.get(i + offset) != Some(&ch) {
                return false;
            }
        }
        true
    }

    fn match_keyword(chars: &[char], i: usize, needle: &str) -> bool {
        if !match_prefix(chars, i, needle) {
            return false;
        }
        let end = i + needle.chars().count();
        end == chars.len() || !is_ident_continue(chars[end])
    }

    fn is_ident_start(ch: char) -> bool {
        ch == '_' || ch.is_ascii_alphabetic()
    }

    fn is_ident_continue(ch: char) -> bool {
        ch == '_' || ch.is_ascii_alphanumeric()
    }
}
