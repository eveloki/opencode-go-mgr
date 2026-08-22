//! I/O-free Stage 1 kernels: identities, protocol catalogs, pricing types,
//! and Zen catalog parse/normalize.
//!
//! These modules must not import db, state, dashboard, gateway execution,
//! reqwest, rusqlite, tokio, filesystem, clocks, or process/host code.
//! Existing public paths keep compatibility re-exports on the original
//! modules.

pub mod ids;
pub mod pricing;
pub mod protocol;
pub mod zen;

#[cfg(test)]
mod dependency_guard {
    use std::fs;
    use std::path::PathBuf;

    const KERNEL_FILES: &[&str] = &["ids.rs", "pricing.rs", "protocol.rs", "zen.rs"];

    const FORBIDDEN_USE_PREFIXES: &[&str] = &[
        "use crate::db",
        "use crate::state",
        "use crate::dashboard",
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

    #[test]
    fn kernel_modules_do_not_import_io_or_control_plane() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/kernel");
        for name in KERNEL_FILES {
            let path = root.join(name);
            let source = fs::read_to_string(&path)
                .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
            for line in source.lines() {
                let trimmed = line.trim();
                if !trimmed.starts_with("use ") {
                    continue;
                }
                for prefix in FORBIDDEN_USE_PREFIXES {
                    assert!(
                        !trimmed.starts_with(prefix),
                        "{} imports I/O or control-plane code: {trimmed}",
                        path.display()
                    );
                }
            }
            for needle in ["Utc::now", "Instant::now", "SystemTime::now"] {
                assert!(
                    !source.contains(needle),
                    "{} must not read a clock (`{needle}`)",
                    path.display()
                );
            }
        }
    }
}
