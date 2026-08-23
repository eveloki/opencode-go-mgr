//! Inference Gateway protocol, policy, and execution boundaries.

/// Hardcoded I/O-free client alias registry and raw-ID parser.
///
/// Public only as the cross-crate bridge; the host crate's `alias`
/// compatibility facade keeps the historical public paths.
#[doc(hidden)]
pub mod alias;

/// Data-only single-attempt transport boundary.
///
/// Public only as the cross-crate bridge; the host crate's `gateway::attempt`
/// facade keeps these items crate-private.
#[doc(hidden)]
pub mod attempt;

/// Pure attempt-adjacent provider/transport error classification policy.
///
/// Public only as the cross-crate bridge; the host crate's `gateway::classify`
/// facade keeps these items crate-private.
#[doc(hidden)]
pub mod classify;

/// Pure secret-free in-memory selection state machine.
///
/// Public only as the cross-crate bridge; a later host facade should keep
/// historical routing-runtime paths crate-private. Do not glob-reexport.
#[doc(hidden)]
pub mod selector;

#[cfg(test)]
mod source_boundary {
    use std::collections::{BTreeMap, BTreeSet};
    use std::fs;
    use std::path::{Path, PathBuf};
    use syn::parse::Parser;
    use syn::punctuated::Punctuated;
    use syn::visit::{self, Visit};
    use syn::{
        Attribute, ExprCall, ExprMethodCall, Item, ItemExternCrate, ItemUse, LitStr, Macro, Meta,
        Token,
    };

    const ALLOWED_DEPENDENCIES: &[&str] = &["anyhow", "ocg-domain"];
    const ALLOWED_DEV_DEPENDENCIES: &[&str] = &["syn", "toml"];
    const FORBIDDEN_IDENTIFIERS: &[&str] = &[
        "CoreState",
        "Database",
        "reqwest",
        "rusqlite",
        "tokio",
        "axum",
        "KeyCipher",
        "decrypt_key",
        "key_cipher",
        "ocg_core",
        "password",
        "cooldown",
        "chrono",
        "parking_lot",
        "credentials",
        "Mutex",
    ];
    const SELECTOR_FORBIDDEN_IDENTIFIERS: &[&str] = &["Account", "anyhow"];
    const FORBIDDEN_STD_MODULES: &[&str] = &["env", "fs", "net", "process"];
    const FORBIDDEN_COMPILE_TIME_MACROS: &[&str] = &[
        "include",
        "env",
        "option_env",
        "include_str",
        "include_bytes",
    ];
    const FORBIDDEN_COMPILE_TIME_MACRO_ROOTS: &[&str] = &["core", "std"];
    const FORBIDDEN_MACRO_TOKENS: &[&str] = &[
        "std::env",
        "std::fs",
        "std::net",
        "std::process",
        "include!",
        "CoreState",
        "Database",
        "reqwest",
        "rusqlite",
        "tokio",
        "axum",
        "KeyCipher",
        "decrypt_key",
        "key_cipher",
        "ocg_core",
        "encrypt(",
        "INSERT",
        "chrono",
        "parking_lot",
        "password",
        "cooldown",
        "credentials",
        "Mutex",
    ];

    #[test]
    fn ocg_gateway_dependencies_stay_inside_the_slice_boundary() {
        let manifest_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml");
        let manifest = fs::read_to_string(&manifest_path)
            .unwrap_or_else(|error| panic!("read {}: {error}", manifest_path.display()));
        assert_manifest_boundary(&manifest_path, &manifest);
    }

    #[test]
    fn production_sources_name_no_host_io_or_credential_storage() {
        let crate_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let src_root = crate_root.join("src");
        let mut scanned = Vec::new();
        visit_rust_files(&src_root, &mut |path| {
            scanned.push(path.to_path_buf());
            let source = fs::read_to_string(path)
                .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
            assert_production_source_boundary(path, &source);
        });
        for required in [
            "alias.rs",
            "attempt.rs",
            "classify.rs",
            "lib.rs",
            "selector.rs",
        ] {
            assert!(
                scanned.iter().any(|path| {
                    path.file_name().and_then(|name| name.to_str()) == Some(required)
                }),
                "source boundary guard must scan {required}, scanned={scanned:?}"
            );
        }

        let manifest_path = crate_root.join("Cargo.toml");
        let manifest = fs::read_to_string(&manifest_path)
            .unwrap_or_else(|error| panic!("read {}: {error}", manifest_path.display()));
        let build_scripts = build_script_paths(&crate_root, &manifest);
        for path in &build_scripts {
            scanned.push(path.clone());
            let source = fs::read_to_string(path)
                .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
            assert_production_source_boundary(path, &source);
        }
        let default_build = crate_root.join("build.rs");
        if default_build.is_file() {
            assert!(
                scanned.iter().any(|path| path == &default_build),
                "source boundary guard must scan build.rs when present, scanned={scanned:?}"
            );
        }

        let lib_source = fs::read_to_string(src_root.join("lib.rs"))
            .unwrap_or_else(|error| panic!("read lib.rs: {error}"));
        let lib_production = lib_source
            .split("#[cfg(test)]")
            .next()
            .expect("production lib.rs precedes tests");
        assert!(
            lib_production.contains("pub mod selector"),
            "lib.rs must declare the selector module bridge"
        );
        for forbidden in [
            "pub use crate::selector",
            "pub use selector::",
            "selector::*",
        ] {
            assert!(
                !lib_production.contains(forbidden),
                "lib.rs must not glob or root-reexport selector via `{forbidden}`"
            );
        }
        assert_selector_contract_types(&src_root.join("selector.rs"));
    }

    #[test]
    fn manifest_guard_rejects_dotted_dependency_subtables() {
        for extra in [
            "[dependencies.serde]\nversion = \"1\"\n",
            "[dev-dependencies.foo]\nversion = \"1\"\n",
        ] {
            let manifest = format!("{}\n{extra}", valid_gateway_manifest());
            assert_manifest_rejects(&manifest);
        }
    }

    #[test]
    fn production_source_guard_continues_after_test_only_items() {
        assert_source_rejects(
            r#"
            #[cfg(test)]
            mod tests { fn helper() {} }

            fn production() { let _ = std::fs::read("secret"); }
            "#,
        );
    }

    #[test]
    fn production_source_guard_skips_only_items_excluded_from_production() {
        assert_production_source_boundary(
            Path::new("fixture.rs"),
            r#"
            fn production() { let _ = 1; }

            #[cfg(test)]
            mod tests {
                fn helper() {
                    let _ = std::fs::read("fixture");
                    let _ = std::net::Ipv4Addr::LOCALHOST;
                    let _ = std::env::var("TEST");
                    include!("fixture.rs");
                }
            }
            "#,
        );
        assert_source_rejects(
            r#"
            #[cfg(any(test, windows))]
            fn production_on_windows() { let _ = std::process::id(); }
            "#,
        );
    }

    #[test]
    fn selector_source_guard_rejects_account_and_anyhow() {
        for source in [
            r#"fn production() { let _ = Account; }"#,
            r#"fn production() { let _ = anyhow::Error::msg("x"); }"#,
        ] {
            assert_source_rejects_path(Path::new("selector.rs"), source);
        }
        assert_production_source_boundary(
            Path::new("fixture.rs"),
            r#"fn production() { let _ = Account; }"#,
        );
    }

    #[test]
    fn selector_contract_accepts_minimal_kernel_shape() {
        let violations = selector_structural_violations(&valid_selector_contract_fixture());
        assert!(
            violations.is_empty(),
            "minimal selector fixture must satisfy the structural contract: {violations:?}"
        );
    }

    #[test]
    fn selector_contract_rejects_adversarial_policy_secret_lock_and_domain_fixtures() {
        for (source, needle) in [
            (
                selector_fixture_inserting(
                    "base_availability: BaseAvailability,",
                    "base_availability: BaseAvailability,\n    provider_id: &'a str,",
                ),
                "provider_id",
            ),
            (
                selector_fixture_with_extra_item("struct ExtraPolicy { provider_id: String }"),
                "provider_id",
            ),
            (
                selector_fixture_inserting(
                    "last_seen: Instant,",
                    "last_seen: Instant,\n    api_key: String,",
                ),
                "api_key",
            ),
            (
                selector_fixture_inserting(
                    "resolved_model: String,",
                    "resolved_model: String,\n    secret: String,",
                ),
                "secret",
            ),
            (
                selector_fixture_inserting(
                    "use std::time::{Duration, Instant};",
                    "use std::time::{Duration, Instant};\nuse ocg_domain::provider::ProviderAdapterKind;",
                ),
                "ocg_domain::provider::ProviderAdapterKind",
            ),
            (
                selector_fixture_inserting(
                    "conversations: ConversationMap,",
                    "conversations: std::sync::RwLock<ConversationMap>,",
                ),
                "RwLock",
            ),
            (
                selector_fixture_inserting(
                    "conversations: ConversationMap,",
                    "conversations: ConversationMap,\n    lock: std::sync::Mutex<()>,",
                ),
                "Mutex",
            ),
            (selector_fixture_with_renamed_rwlock_state(), "RwLock"),
        ] {
            assert_selector_contract_rejects(&source, needle);
        }
    }

    #[test]
    fn production_source_guard_rejects_host_networking_and_environment_access() {
        for source in [
            r#"fn production() { let _ = std::net::TcpListener::bind("127.0.0.1:0"); }"#,
            r#"use std::net::SocketAddr;"#,
            r#"use std::{net::TcpStream, time::Duration};"#,
            r#"fn production() { let _ = std::env::var("SECRET"); }"#,
            r#"use std::env;"#,
            r#"use std::env::{var, var_os};"#,
            r#"use std::*;"#,
        ] {
            assert_source_rejects(source);
        }
    }

    #[test]
    fn production_source_guard_rejects_std_alias_bypasses() {
        for source in [
            r#"
            use std as platform;
            fn production() { let _ = platform::net::TcpListener::bind("127.0.0.1:0"); }
            "#,
            r#"
            extern crate std as platform;
            fn production() { let _ = platform::env::var("SECRET"); }
            "#,
        ] {
            assert_source_rejects(source);
        }
    }

    #[test]
    fn production_source_guard_rejects_include_indirection() {
        for source in [
            r#"include!("hidden.rs");"#,
            r#"std::include!("hidden.rs");"#,
            r#"fn production() { include!("../outside.rs"); }"#,
            r#"macro_rules! pull { () => { include!("hidden.rs"); } }"#,
        ] {
            assert_source_rejects(source);
        }
    }

    #[test]
    fn selector_source_guard_rejects_compile_time_macro_bypasses() {
        for source in [
            r#"fn production() { include!("hidden.rs"); }"#,
            r#"fn production() { env!("SECRET"); }"#,
            r#"fn production() { option_env!("SECRET"); }"#,
            r#"fn production() { include_str!("hidden.txt"); }"#,
            r#"fn production() { include_bytes!("hidden.bin"); }"#,
            r#"fn production() { std::include!("hidden.rs"); }"#,
            r#"fn production() { std::env!("SECRET"); }"#,
            r#"fn production() { std::option_env!("SECRET"); }"#,
            r#"fn production() { std::include_str!("hidden.txt"); }"#,
            r#"fn production() { std::include_bytes!("hidden.bin"); }"#,
            r#"fn production() { core::include!("hidden.rs"); }"#,
            r#"fn production() { core::env!("SECRET"); }"#,
            r#"fn production() { core::option_env!("SECRET"); }"#,
            r#"fn production() { core::include_str!("hidden.txt"); }"#,
            r#"fn production() { core::include_bytes!("hidden.bin"); }"#,
            r#"
                use std::include_str as load_source;
                fn production() { load_source!("hidden.txt"); }
            "#,
            r#"
                use core::include_str as load_source;
                fn production() { load_source!("hidden.txt"); }
            "#,
        ] {
            assert_source_rejects_path(Path::new("selector.rs"), source);
        }
    }

    #[test]
    fn build_script_guard_rejects_host_io_and_include_bypass() {
        for source in [
            r#"fn main() { let _ = std::fs::read("Cargo.toml"); }"#,
            r#"fn main() { let _ = std::env::var("OUT_DIR"); }"#,
            r#"fn main() { let _ = std::net::UdpSocket::bind("0.0.0.0:0"); }"#,
            r#"fn main() { let _ = std::process::id(); }"#,
            r#"include!("host.rs"); fn main() {}"#,
        ] {
            assert_source_rejects_path(Path::new("build.rs"), source);
        }
        assert_production_source_boundary(Path::new("build.rs"), "fn main() {}");
    }

    #[test]
    fn build_script_paths_include_declared_and_default_scripts() {
        let crate_root = Path::new("fixture-crate");
        let declared = valid_gateway_manifest().replace(
            "version = \"1.0.0\"",
            "version = \"1.0.0\"\n            build = \"custom-build.rs\"",
        );
        let paths = build_script_paths(crate_root, &declared);
        assert!(
            paths.iter().any(|path| path.ends_with("custom-build.rs")),
            "declared package.build path must be scanned, got {paths:?}"
        );
        assert_eq!(
            build_script_paths(crate_root, valid_gateway_manifest()),
            Vec::<PathBuf>::new()
        );
    }

    fn valid_gateway_manifest() -> &'static str {
        r#"
            [package]
            name = "ocg-gateway"
            version = "1.0.0"

            [dependencies]
            anyhow = "1"
            ocg-domain = { path = "../ocg-domain" }

            [dev-dependencies]
            syn = { version = "2", features = ["full", "visit"] }
            toml = "0.9"
        "#
    }

    fn assert_manifest_rejects(manifest: &str) {
        assert!(
            std::panic::catch_unwind(|| {
                assert_manifest_boundary(Path::new("fixture.toml"), manifest);
            })
            .is_err(),
            "manifest guard must reject: {manifest}"
        );
    }

    fn assert_source_rejects(source: &str) {
        assert_source_rejects_path(Path::new("fixture.rs"), source);
    }

    fn assert_source_rejects_path(path: &Path, source: &str) {
        let path = path.to_path_buf();
        assert!(
            std::panic::catch_unwind(|| {
                assert_production_source_boundary(&path, source);
            })
            .is_err(),
            "source guard must reject {}: {source}",
            path.display()
        );
    }

    fn build_script_paths(crate_root: &Path, manifest: &str) -> Vec<PathBuf> {
        let parsed: toml::Value = toml::from_str(manifest)
            .unwrap_or_else(|error| panic!("parse build-script manifest: {error}"));
        let mut paths = Vec::new();
        if let Some(relative) = parsed
            .get("package")
            .and_then(|package| package.get("build"))
            .and_then(toml::Value::as_str)
        {
            paths.push(crate_root.join(relative));
        }
        let default_build = crate_root.join("build.rs");
        if default_build.is_file() && !paths.contains(&default_build) {
            paths.push(default_build);
        }
        paths
    }

    fn assert_manifest_boundary(manifest_path: &Path, manifest: &str) {
        let parsed: toml::Value = toml::from_str(manifest)
            .unwrap_or_else(|error| panic!("parse {}: {error}", manifest_path.display()));
        let root = parsed
            .as_table()
            .unwrap_or_else(|| panic!("{} must contain a TOML table", manifest_path.display()));

        assert_dependency_table(manifest_path, root, "dependencies", ALLOWED_DEPENDENCIES);
        assert_dependency_table(
            manifest_path,
            root,
            "dev-dependencies",
            ALLOWED_DEV_DEPENDENCIES,
        );
        assert_dependency_table(manifest_path, root, "build-dependencies", &[]);
        assert!(
            !root.contains_key("target"),
            "{} must not declare target-specific dependencies",
            manifest_path.display()
        );
    }

    fn assert_dependency_table(
        manifest_path: &Path,
        root: &toml::Table,
        table_name: &str,
        allowed: &[&str],
    ) {
        let Some(value) = root.get(table_name) else {
            assert!(
                allowed.is_empty(),
                "{} must declare [{table_name}] with {allowed:?}",
                manifest_path.display()
            );
            return;
        };
        let table = value.as_table().unwrap_or_else(|| {
            panic!("{} [{table_name}] must be a table", manifest_path.display())
        });
        let mut aliases = table.keys().map(String::as_str).collect::<Vec<_>>();
        aliases.sort_unstable();
        assert_eq!(
            aliases,
            allowed,
            "{} [{table_name}] must be only {allowed:?}, got {aliases:?}",
            manifest_path.display()
        );
        for (alias, spec) in table {
            let package = spec
                .as_table()
                .and_then(|details| details.get("package"))
                .and_then(toml::Value::as_str)
                .unwrap_or(alias);
            assert!(
                allowed.contains(&package),
                "{} [{table_name}] alias `{alias}` must not resolve to package `{package}`",
                manifest_path.display()
            );
        }
    }

    fn extra_forbidden_identifiers(path: &Path) -> &'static [&'static str] {
        if path.file_name().and_then(|name| name.to_str()) == Some("selector.rs") {
            SELECTOR_FORBIDDEN_IDENTIFIERS
        } else {
            &[]
        }
    }

    fn assert_production_source_boundary(path: &Path, source: &str) {
        let syntax = syn::parse_file(source)
            .unwrap_or_else(|error| panic!("parse {}: {error}", path.display()));
        let mut visitor = BoundaryVisitor {
            violations: Vec::new(),
            extra_forbidden: extra_forbidden_identifiers(path),
            forbidden_macro_aliases: BTreeSet::new(),
        };
        visitor.visit_file(&syntax);
        assert!(
            visitor.violations.is_empty(),
            "{} production source violates ocg-gateway boundary: {:?}",
            path.display(),
            visitor.violations
        );
    }

    fn assert_selector_contract_types(path: &Path) {
        let source = fs::read_to_string(path)
            .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
        assert_selector_structural_contract(path, &source);
    }

    fn assert_selector_structural_contract(path: &Path, source: &str) {
        let violations = selector_structural_violations(source);
        assert!(
            violations.is_empty(),
            "{} selector structural contract violated: {violations:?}",
            path.display()
        );
    }

    fn assert_selector_contract_rejects(source: &str, needle: &str) {
        let violations = selector_structural_violations(source);
        assert!(
            violations
                .iter()
                .any(|violation| violation.contains(needle)),
            "selector contract must reject `{needle}`, violations={violations:?}"
        );
    }

    fn selector_fixture_inserting(snippet: &str, replacement: &str) -> String {
        let source = valid_selector_contract_fixture();
        assert!(
            source.contains(snippet),
            "selector fixture must contain `{snippet}`"
        );
        source.replace(snippet, replacement)
    }

    fn selector_fixture_with_extra_item(item: &str) -> String {
        format!("{}\n{item}\n", valid_selector_contract_fixture())
    }

    fn selector_fixture_with_renamed_rwlock_state() -> String {
        selector_fixture_inserting(
            "use std::time::{Duration, Instant};",
            "use std::time::{Duration, Instant};\nuse std::sync::RwLock as Shared;",
        )
        .replace(
            "conversations: ConversationMap,",
            "conversations: Shared<ConversationMap>,",
        )
    }

    fn valid_selector_contract_fixture() -> String {
        r#"
use ocg_domain::account::UpstreamChannel;
use std::collections::{HashMap, VecDeque};
use std::time::{Duration, Instant};

pub const CONVERSATION_TTL: Duration = Duration::from_secs(1);
pub const MAX_CONVERSATIONS: usize = 1;

pub enum SelectionPolicy {
    StrictPriority,
    StickyGlobal,
    RoundRobin,
}

pub enum BaseAvailability {
    Available,
    Unavailable,
}

pub enum SelectionError {
    DuplicateAccountId { first: usize, duplicate: usize },
}

pub struct Candidate<'a> {
    account_id: &'a str,
    channel: UpstreamChannel,
    resolved_model: &'a str,
    base_availability: BaseAvailability,
}

pub struct Selection {
    candidate_index: usize,
}

pub struct BindingSnapshot {
    account_id: String,
    channel: UpstreamChannel,
    resolved_model: String,
}

struct ConversationBinding {
    account_id: String,
    channel: UpstreamChannel,
    resolved_model: String,
    last_seen: Instant,
}

struct ConversationMap {
    entries: HashMap<String, ConversationBinding>,
    order: VecDeque<String>,
}

pub struct SelectorState {
    global_account_id: Option<String>,
    round_robin_after: Option<String>,
    conversations: ConversationMap,
}
"#
        .to_string()
    }

    fn selector_structural_violations(source: &str) -> Vec<String> {
        let syntax = syn::parse_file(source)
            .unwrap_or_else(|error| panic!("parse selector contract source: {error}"));

        let mut imports = ImportCollector::default();
        imports.visit_file(&syntax);

        let mut aliases = BTreeMap::new();
        for path in &imports.paths {
            if let Some(name) = path.rsplit("::").next() {
                aliases.insert(
                    name.to_string(),
                    path.split("::").map(str::to_string).collect(),
                );
            }
        }

        let mut analyzer = SelectorAnalyzer {
            aliases,
            structs: BTreeMap::new(),
            enums: BTreeMap::new(),
            consts: BTreeMap::new(),
            violations: imports.violations,
        };
        analyzer.visit_file(&syntax);
        analyzer.finish(imports.paths);
        analyzer.violations.sort();
        analyzer.violations.dedup();
        analyzer.violations
    }

    #[derive(Default)]
    struct ImportCollector {
        paths: Vec<String>,
        violations: Vec<String>,
    }

    impl<'ast> Visit<'ast> for ImportCollector {
        fn visit_item(&mut self, item: &'ast Item) {
            if item_excluded_from_production(item) {
                return;
            }
            visit::visit_item(self, item);
        }

        fn visit_item_use(&mut self, item: &'ast ItemUse) {
            reject_selector_use_tree_shape(&item.tree, &mut self.violations);
            let mut nested = Vec::new();
            flatten_use_tree(Vec::new(), &item.tree, &mut nested);
            self.paths
                .extend(nested.into_iter().map(|path| path.join("::")));
            visit::visit_item_use(self, item);
        }
    }

    fn reject_selector_use_tree_shape(tree: &syn::UseTree, violations: &mut Vec<String>) {
        match tree {
            syn::UseTree::Rename(rename) => {
                violations.push(format!(
                    "selector import alias `{}` is not allowed",
                    rename.rename
                ));
            }
            syn::UseTree::Glob(_) => {
                violations.push("selector glob import is not allowed".to_string());
            }
            syn::UseTree::Path(path) => reject_selector_use_tree_shape(&path.tree, violations),
            syn::UseTree::Group(group) => {
                for tree in &group.items {
                    reject_selector_use_tree_shape(tree, violations);
                }
            }
            syn::UseTree::Name(_) => {}
        }
    }

    struct RecordedStruct {
        visibility: &'static str,
        lifetimes: Vec<String>,
        fields: Vec<(String, String)>,
    }

    struct RecordedEnum {
        visibility: &'static str,
        variants: Vec<(String, Vec<(String, String)>)>,
    }

    struct RecordedConst {
        visibility: &'static str,
        ty: String,
    }

    struct SelectorAnalyzer {
        aliases: BTreeMap<String, Vec<String>>,
        structs: BTreeMap<String, RecordedStruct>,
        enums: BTreeMap<String, RecordedEnum>,
        consts: BTreeMap<String, RecordedConst>,
        violations: Vec<String>,
    }

    impl SelectorAnalyzer {
        fn finish(&mut self, mut imported: Vec<String>) {
            imported.sort();
            imported.dedup();
            let allowed = expected_selector_imports();
            if imported != allowed {
                self.violations.push(format!(
                    "selector.rs imports must be exactly {allowed:?}, got {imported:?}"
                ));
            }

            self.compare_consts();
            self.compare_structs();
            self.compare_enums();
        }

        fn compare_consts(&mut self) {
            let expected = expected_selector_consts();
            for (name, (visibility, ty)) in &expected {
                match self.consts.get(*name) {
                    None => self
                        .violations
                        .push(format!("selector.rs must declare const {name}")),
                    Some(got) => {
                        if got.visibility != *visibility || got.ty != *ty {
                            self.violations.push(format!(
                                "const {name} must be {visibility} {ty}, got {} {}",
                                got.visibility, got.ty
                            ));
                        }
                    }
                }
            }
            for (name, got) in &self.consts {
                if expected.contains_key(name.as_str()) {
                    continue;
                }
                if got.visibility != "private" {
                    self.violations
                        .push(format!("unexpected public selector const {name}"));
                }
            }
        }

        fn compare_structs(&mut self) {
            let expected = expected_selector_structs();
            for (name, expected) in &expected {
                match self.structs.get(*name) {
                    None => self
                        .violations
                        .push(format!("selector.rs must declare struct {name}")),
                    Some(got) => {
                        if got.visibility != expected.visibility {
                            self.violations.push(format!(
                                "struct {name} must be {}, got {}",
                                expected.visibility, got.visibility
                            ));
                        }
                        if got.lifetimes != expected.lifetimes {
                            self.violations.push(format!(
                                "struct {name} lifetimes must be {:?}, got {:?}",
                                expected.lifetimes, got.lifetimes
                            ));
                        }
                        if got.fields != expected.fields {
                            self.violations.push(format!(
                                "struct {name} fields must be {:?}, got {:?}",
                                expected.fields, got.fields
                            ));
                        }
                    }
                }
            }
            for (name, got) in &self.structs {
                if expected.contains_key(name.as_str()) {
                    continue;
                }
                if got.visibility != "private" {
                    self.violations
                        .push(format!("unexpected public selector struct {name}"));
                }
            }
        }

        fn compare_enums(&mut self) {
            let expected = expected_selector_enums();
            for (name, expected) in &expected {
                match self.enums.get(*name) {
                    None => self
                        .violations
                        .push(format!("selector.rs must declare enum {name}")),
                    Some(got) => {
                        if got.visibility != expected.visibility {
                            self.violations.push(format!(
                                "enum {name} must be {}, got {}",
                                expected.visibility, got.visibility
                            ));
                        }
                        if got.variants != expected.variants {
                            self.violations.push(format!(
                                "enum {name} variants must be {:?}, got {:?}",
                                expected.variants, got.variants
                            ));
                        }
                    }
                }
            }
            for name in self.enums.keys() {
                if !expected.contains_key(name.as_str()) {
                    self.violations
                        .push(format!("unexpected selector enum {name}"));
                }
            }
        }
    }

    impl<'ast> Visit<'ast> for SelectorAnalyzer {
        fn visit_item(&mut self, item: &'ast Item) {
            if item_excluded_from_production(item) {
                return;
            }
            match item {
                Item::Use(_)
                | Item::Fn(_)
                | Item::Impl(_)
                | Item::Const(_)
                | Item::Enum(_)
                | Item::Struct(_) => {}
                other => self.violations.push(format!(
                    "unexpected selector item {}",
                    selector_item_kind(other)
                )),
            }
            visit::visit_item(self, item);
        }

        fn visit_item_struct(&mut self, item: &'ast syn::ItemStruct) {
            let name = item.ident.to_string();
            let mut fields = Vec::new();
            match &item.fields {
                syn::Fields::Named(named) => {
                    for field in &named.named {
                        let field_name = field
                            .ident
                            .as_ref()
                            .map(ToString::to_string)
                            .unwrap_or_default();
                        if let Some(needle) = forbidden_selector_field_name(&field_name) {
                            self.violations.push(format!(
                                "selector field `{field_name}` is forbidden ({needle})"
                            ));
                        }
                        if !matches!(field.vis, syn::Visibility::Inherited) {
                            self.violations
                                .push(format!("struct {name} field {field_name} must be private"));
                        }
                        fields.push((field_name, normalize_type(&field.ty, &self.aliases)));
                    }
                }
                syn::Fields::Unnamed(_) => {
                    self.violations
                        .push(format!("struct {name} must have named fields"));
                }
                syn::Fields::Unit => {
                    self.violations
                        .push(format!("struct {name} must have named fields"));
                }
            }
            self.structs.insert(
                name,
                RecordedStruct {
                    visibility: visibility_kind(&item.vis),
                    lifetimes: struct_lifetimes(&item.generics),
                    fields,
                },
            );
            visit::visit_item_struct(self, item);
        }

        fn visit_item_enum(&mut self, item: &'ast syn::ItemEnum) {
            let name = item.ident.to_string();
            let mut variants = Vec::new();
            for variant in &item.variants {
                let variant_name = variant.ident.to_string();
                if let Some(needle) = forbidden_selector_surface_name(&variant_name) {
                    self.violations.push(format!(
                        "selector variant `{variant_name}` is forbidden ({needle})"
                    ));
                }
                let fields = match &variant.fields {
                    syn::Fields::Named(named) => named
                        .named
                        .iter()
                        .map(|field| {
                            let field_name = field
                                .ident
                                .as_ref()
                                .map(ToString::to_string)
                                .unwrap_or_default();
                            if let Some(needle) = forbidden_selector_field_name(&field_name) {
                                self.violations.push(format!(
                                    "selector field `{field_name}` is forbidden ({needle})"
                                ));
                            }
                            (field_name, normalize_type(&field.ty, &self.aliases))
                        })
                        .collect(),
                    syn::Fields::Unnamed(unnamed) => unnamed
                        .unnamed
                        .iter()
                        .enumerate()
                        .map(|(index, field)| {
                            (index.to_string(), normalize_type(&field.ty, &self.aliases))
                        })
                        .collect(),
                    syn::Fields::Unit => Vec::new(),
                };
                variants.push((variant_name, fields));
            }
            self.enums.insert(
                name,
                RecordedEnum {
                    visibility: visibility_kind(&item.vis),
                    variants,
                },
            );
            visit::visit_item_enum(self, item);
        }

        fn visit_item_const(&mut self, item: &'ast syn::ItemConst) {
            let name = item.ident.to_string();
            if let Some(needle) = forbidden_selector_surface_name(&name) {
                self.violations
                    .push(format!("selector const `{name}` is forbidden ({needle})"));
            }
            self.consts.insert(
                name,
                RecordedConst {
                    visibility: visibility_kind(&item.vis),
                    ty: normalize_type(&item.ty, &self.aliases),
                },
            );
            visit::visit_item_const(self, item);
        }

        fn visit_fn_arg(&mut self, arg: &'ast syn::FnArg) {
            if let syn::FnArg::Typed(typed) = arg
                && let syn::Pat::Ident(ident) = typed.pat.as_ref()
                && let Some(needle) = forbidden_selector_surface_name(&ident.ident.to_string())
            {
                self.violations.push(format!(
                    "selector argument `{}` is forbidden ({needle})",
                    ident.ident
                ));
            }
            visit::visit_fn_arg(self, arg);
        }

        fn visit_path(&mut self, path: &'ast syn::Path) {
            let segments = path
                .segments
                .iter()
                .map(|segment| segment.ident.to_string())
                .collect::<Vec<_>>();
            if let Some(violation) = selector_forbidden_path(&segments) {
                self.violations.push(violation);
            }
            visit::visit_path(self, path);
        }
    }

    fn expected_selector_imports() -> Vec<String> {
        [
            "ocg_domain::account::UpstreamChannel",
            "std::collections::HashMap",
            "std::collections::VecDeque",
            "std::time::Duration",
            "std::time::Instant",
        ]
        .into_iter()
        .map(str::to_string)
        .collect()
    }

    fn expected_selector_consts() -> BTreeMap<&'static str, (&'static str, &'static str)> {
        BTreeMap::from([
            ("CONVERSATION_TTL", ("pub", "std::time::Duration")),
            ("MAX_CONVERSATIONS", ("pub", "usize")),
        ])
    }

    fn expected_selector_structs() -> BTreeMap<&'static str, RecordedStruct> {
        BTreeMap::from([
            (
                "Candidate",
                RecordedStruct {
                    visibility: "pub",
                    lifetimes: vec!["'a".to_string()],
                    fields: vec![
                        ("account_id".to_string(), "&'a str".to_string()),
                        (
                            "channel".to_string(),
                            "ocg_domain::account::UpstreamChannel".to_string(),
                        ),
                        ("resolved_model".to_string(), "&'a str".to_string()),
                        (
                            "base_availability".to_string(),
                            "BaseAvailability".to_string(),
                        ),
                    ],
                },
            ),
            (
                "Selection",
                RecordedStruct {
                    visibility: "pub",
                    lifetimes: Vec::new(),
                    fields: vec![("candidate_index".to_string(), "usize".to_string())],
                },
            ),
            (
                "BindingSnapshot",
                RecordedStruct {
                    visibility: "pub",
                    lifetimes: Vec::new(),
                    fields: vec![
                        ("account_id".to_string(), "String".to_string()),
                        (
                            "channel".to_string(),
                            "ocg_domain::account::UpstreamChannel".to_string(),
                        ),
                        ("resolved_model".to_string(), "String".to_string()),
                    ],
                },
            ),
            (
                "ConversationBinding",
                RecordedStruct {
                    visibility: "private",
                    lifetimes: Vec::new(),
                    fields: vec![
                        ("account_id".to_string(), "String".to_string()),
                        (
                            "channel".to_string(),
                            "ocg_domain::account::UpstreamChannel".to_string(),
                        ),
                        ("resolved_model".to_string(), "String".to_string()),
                        ("last_seen".to_string(), "std::time::Instant".to_string()),
                    ],
                },
            ),
            (
                "ConversationMap",
                RecordedStruct {
                    visibility: "private",
                    lifetimes: Vec::new(),
                    fields: vec![
                        (
                            "entries".to_string(),
                            "std::collections::HashMap<String, ConversationBinding>".to_string(),
                        ),
                        (
                            "order".to_string(),
                            "std::collections::VecDeque<String>".to_string(),
                        ),
                    ],
                },
            ),
            (
                "SelectorState",
                RecordedStruct {
                    visibility: "pub",
                    lifetimes: Vec::new(),
                    fields: vec![
                        (
                            "global_account_id".to_string(),
                            "Option<String>".to_string(),
                        ),
                        (
                            "round_robin_after".to_string(),
                            "Option<String>".to_string(),
                        ),
                        ("conversations".to_string(), "ConversationMap".to_string()),
                    ],
                },
            ),
        ])
    }

    fn expected_selector_enums() -> BTreeMap<&'static str, RecordedEnum> {
        BTreeMap::from([
            (
                "SelectionPolicy",
                RecordedEnum {
                    visibility: "pub",
                    variants: vec![
                        ("StrictPriority".to_string(), Vec::new()),
                        ("StickyGlobal".to_string(), Vec::new()),
                        ("RoundRobin".to_string(), Vec::new()),
                    ],
                },
            ),
            (
                "BaseAvailability",
                RecordedEnum {
                    visibility: "pub",
                    variants: vec![
                        ("Available".to_string(), Vec::new()),
                        ("Unavailable".to_string(), Vec::new()),
                    ],
                },
            ),
            (
                "SelectionError",
                RecordedEnum {
                    visibility: "pub",
                    variants: vec![(
                        "DuplicateAccountId".to_string(),
                        vec![
                            ("first".to_string(), "usize".to_string()),
                            ("duplicate".to_string(), "usize".to_string()),
                        ],
                    )],
                },
            ),
        ])
    }

    fn visibility_kind(vis: &syn::Visibility) -> &'static str {
        match vis {
            syn::Visibility::Public(_) => "pub",
            syn::Visibility::Inherited => "private",
            syn::Visibility::Restricted(_) => "restricted",
        }
    }

    fn struct_lifetimes(generics: &syn::Generics) -> Vec<String> {
        generics
            .params
            .iter()
            .filter_map(|param| match param {
                syn::GenericParam::Lifetime(lifetime) => Some(lifetime.lifetime.to_string()),
                _ => None,
            })
            .collect()
    }

    fn selector_item_kind(item: &Item) -> &'static str {
        match item {
            Item::Trait(_) => "trait",
            Item::Type(_) => "type alias",
            Item::Union(_) => "union",
            Item::Mod(_) => "mod",
            Item::Static(_) => "static",
            Item::ForeignMod(_) => "foreign mod",
            Item::ExternCrate(_) => "extern crate",
            Item::TraitAlias(_) => "trait alias",
            Item::Macro(_) => "macro",
            Item::Verbatim(_) => "verbatim",
            _ => "item",
        }
    }

    fn forbidden_selector_field_name(name: &str) -> Option<&'static str> {
        if let Some(needle) = forbidden_selector_surface_name(name) {
            return Some(needle);
        }
        let lower = name.to_ascii_lowercase();
        if lower == "key" || (lower.ends_with("_key") && lower != "conversation_key") {
            Some("key")
        } else {
            None
        }
    }

    fn forbidden_selector_surface_name(name: &str) -> Option<&'static str> {
        let lower = name.to_ascii_lowercase();
        [
            "provider_id",
            "offering_id",
            "api_key",
            "secret",
            "password",
            "credential",
            "key_cipher",
            "decrypt_key",
        ]
        .into_iter()
        .find(|needle| lower == *needle || lower.contains(needle))
    }

    fn selector_forbidden_path(segments: &[String]) -> Option<String> {
        if segments.is_empty() {
            return None;
        }
        let joined = segments.join("::");
        if segments[0] == "ocg_domain" {
            return (joined != "ocg_domain::account::UpstreamChannel").then_some(joined);
        }
        if matches!(
            segments[0].as_str(),
            "anyhow"
                | "parking_lot"
                | "ocg_core"
                | "tokio"
                | "axum"
                | "reqwest"
                | "rusqlite"
                | "chrono"
        ) {
            return Some(joined);
        }
        if segments[0] == "crate"
            && segments
                .get(1)
                .is_some_and(|module| matches!(module.as_str(), "alias" | "attempt" | "classify"))
        {
            return Some(joined);
        }
        if segments[0] == "std"
            && segments.get(1).is_some_and(|module| {
                matches!(
                    module.as_str(),
                    "sync" | "thread" | "fs" | "env" | "net" | "process" | "io"
                )
            })
        {
            return Some(joined);
        }
        if segments.last().is_some_and(|last| {
            matches!(
                last.as_str(),
                "Mutex" | "RwLock" | "OnceLock" | "Account" | "KeyCipher"
            )
        }) {
            return Some(joined);
        }
        None
    }

    fn normalize_type(ty: &syn::Type, aliases: &BTreeMap<String, Vec<String>>) -> String {
        match ty {
            syn::Type::Reference(reference) => {
                let mut rendered = String::from("&");
                if let Some(lifetime) = &reference.lifetime {
                    rendered.push_str(&lifetime.to_string());
                    rendered.push(' ');
                }
                if reference.mutability.is_some() {
                    rendered.push_str("mut ");
                }
                rendered.push_str(&normalize_type(reference.elem.as_ref(), aliases));
                rendered
            }
            syn::Type::Path(path) => normalize_type_path(path, aliases),
            syn::Type::Tuple(tuple) => {
                let elems = tuple
                    .elems
                    .iter()
                    .map(|elem| normalize_type(elem, aliases))
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("({elems})")
            }
            syn::Type::Slice(slice) => {
                format!("[{}]", normalize_type(slice.elem.as_ref(), aliases))
            }
            syn::Type::Paren(paren) => normalize_type(paren.elem.as_ref(), aliases),
            syn::Type::Array(array) => {
                format!("[{}; _]", normalize_type(array.elem.as_ref(), aliases))
            }
            syn::Type::Ptr(ptr) => {
                let mut rendered = if ptr.mutability.is_some() {
                    String::from("*mut ")
                } else {
                    String::from("*const ")
                };
                rendered.push_str(&normalize_type(ptr.elem.as_ref(), aliases));
                rendered
            }
            _ => "unsupported-type".to_string(),
        }
    }

    fn normalize_type_path(
        path: &syn::TypePath,
        aliases: &BTreeMap<String, Vec<String>>,
    ) -> String {
        let raw = path
            .path
            .segments
            .iter()
            .map(|segment| segment.ident.to_string())
            .collect::<Vec<_>>();
        let resolved = resolve_selector_path(raw, aliases);
        let mut rendered = resolved.join("::");
        if let Some(last) = path.path.segments.last()
            && let syn::PathArguments::AngleBracketed(args) = &last.arguments
        {
            rendered.push('<');
            rendered.push_str(&normalize_generic_args(args, aliases));
            rendered.push('>');
        }
        rendered
    }

    fn resolve_selector_path(
        raw: Vec<String>,
        aliases: &BTreeMap<String, Vec<String>>,
    ) -> Vec<String> {
        let resolved = if let Some(mapped) = raw
            .first()
            .and_then(|first| aliases.get(first))
            .filter(|mapped| mapped.last() == raw.first())
        {
            let mut full = mapped.clone();
            if raw.len() > 1 {
                full.extend_from_slice(&raw[1..]);
            }
            full
        } else {
            raw
        };
        collapse_prelude_path(resolved)
    }

    fn collapse_prelude_path(segments: Vec<String>) -> Vec<String> {
        match segments.as_slice() {
            [a, b, c] if a == "std" && b == "string" && c == "String" => {
                vec!["String".to_string()]
            }
            [a, b, c] if a == "std" && b == "option" && c == "Option" => {
                vec!["Option".to_string()]
            }
            [a, b, c] if a == "std" && b == "result" && c == "Result" => {
                vec!["Result".to_string()]
            }
            [a, b, c] if a == "std" && b == "vec" && c == "Vec" => vec!["Vec".to_string()],
            [a, b, c] if a == "std" && b == "boxed" && c == "Box" => vec!["Box".to_string()],
            _ => segments,
        }
    }

    fn normalize_generic_args(
        args: &syn::AngleBracketedGenericArguments,
        aliases: &BTreeMap<String, Vec<String>>,
    ) -> String {
        args.args
            .iter()
            .map(|arg| match arg {
                syn::GenericArgument::Lifetime(lifetime) => lifetime.to_string(),
                syn::GenericArgument::Type(ty) => normalize_type(ty, aliases),
                syn::GenericArgument::Const(_) => "_".to_string(),
                syn::GenericArgument::AssocType(assoc) => {
                    format!("{}={}", assoc.ident, normalize_type(&assoc.ty, aliases))
                }
                _ => "_".to_string(),
            })
            .collect::<Vec<_>>()
            .join(", ")
    }

    struct BoundaryVisitor {
        violations: Vec<String>,
        extra_forbidden: &'static [&'static str],
        forbidden_macro_aliases: BTreeSet<String>,
    }

    impl<'ast> Visit<'ast> for BoundaryVisitor {
        fn visit_file(&mut self, file: &'ast syn::File) {
            for item in &file.items {
                if !item_excluded_from_production(item)
                    && let Item::Use(item_use) = item
                {
                    collect_forbidden_macro_aliases(
                        Vec::new(),
                        &item_use.tree,
                        &mut self.forbidden_macro_aliases,
                    );
                }
            }
            visit::visit_file(self, file);
        }

        fn visit_item(&mut self, item: &'ast Item) {
            if item_excluded_from_production(item) {
                return;
            }
            visit::visit_item(self, item);
        }

        fn visit_ident(&mut self, ident: &'ast syn::Ident) {
            let name = ident.to_string();
            if FORBIDDEN_IDENTIFIERS.contains(&name.as_str())
                || self.extra_forbidden.contains(&name.as_str())
            {
                self.violations.push(name);
            }
        }

        fn visit_path(&mut self, path: &'ast syn::Path) {
            let segments = path
                .segments
                .iter()
                .map(|segment| segment.ident.to_string())
                .collect::<Vec<_>>();
            if let Some(violation) = forbidden_std_path(&segments) {
                self.violations.push(violation);
            }
            visit::visit_path(self, path);
        }

        fn visit_item_use(&mut self, item: &'ast ItemUse) {
            if use_tree_aliases_std(&item.tree) {
                self.violations.push("std alias".to_string());
            }
            let mut paths = Vec::new();
            flatten_use_tree(Vec::new(), &item.tree, &mut paths);
            for path in paths {
                if let Some(violation) = forbidden_std_path(&path) {
                    self.violations.push(violation);
                }
            }
            visit::visit_item_use(self, item);
        }

        fn visit_item_extern_crate(&mut self, item: &'ast ItemExternCrate) {
            if item.ident == "std" {
                self.violations.push("extern crate std".to_string());
            }
            visit::visit_item_extern_crate(self, item);
        }

        fn visit_expr_call(&mut self, call: &'ast ExprCall) {
            if let syn::Expr::Path(path) = call.func.as_ref() {
                if path
                    .path
                    .segments
                    .last()
                    .is_some_and(|segment| segment.ident == "encrypt")
                {
                    self.violations.push("encrypt(...)".to_string());
                }
            }
            visit::visit_expr_call(self, call);
        }

        fn visit_expr_method_call(&mut self, call: &'ast ExprMethodCall) {
            if call.method == "encrypt" {
                self.violations.push(".encrypt(...)".to_string());
            }
            visit::visit_expr_method_call(self, call);
        }

        fn visit_lit_str(&mut self, literal: &'ast LitStr) {
            if literal.value().contains("INSERT ") {
                self.violations.push("SQL INSERT literal".to_string());
            }
        }

        fn visit_macro(&mut self, mac: &'ast Macro) {
            if let Some(violation) =
                forbidden_compile_time_macro_path(&mac.path, &self.forbidden_macro_aliases)
            {
                self.violations.push(violation);
            }
            let compact = mac
                .tokens
                .to_string()
                .chars()
                .filter(|ch| !ch.is_whitespace())
                .collect::<String>();
            for forbidden in FORBIDDEN_MACRO_TOKENS {
                if compact.contains(forbidden) {
                    self.violations.push(format!("macro token `{forbidden}`"));
                }
            }
            visit::visit_macro(self, mac);
        }
    }

    fn forbidden_std_path(segments: &[String]) -> Option<String> {
        match segments {
            [std, module, ..]
                if std == "std" && FORBIDDEN_STD_MODULES.contains(&module.as_str()) =>
            {
                Some(segments.join("::"))
            }
            [std, glob] if std == "std" && glob == "*" => Some("std::*".to_string()),
            _ => None,
        }
    }

    fn use_tree_aliases_std(tree: &syn::UseTree) -> bool {
        match tree {
            syn::UseTree::Rename(rename) => rename.ident == "std",
            syn::UseTree::Path(path) if path.ident == "std" => {
                use_tree_aliases_std_after_root(&path.tree)
            }
            syn::UseTree::Group(group) => group.items.iter().any(use_tree_aliases_std),
            _ => false,
        }
    }

    fn use_tree_aliases_std_after_root(tree: &syn::UseTree) -> bool {
        match tree {
            syn::UseTree::Rename(rename) => rename.ident == "self",
            syn::UseTree::Group(group) => group.items.iter().any(use_tree_aliases_std_after_root),
            _ => false,
        }
    }

    fn forbidden_compile_time_macro_path(
        path: &syn::Path,
        aliases: &BTreeSet<String>,
    ) -> Option<String> {
        let segments = path
            .segments
            .iter()
            .map(|segment| segment.ident.to_string())
            .collect::<Vec<_>>();
        match segments.as_slice() {
            [name] if FORBIDDEN_COMPILE_TIME_MACROS.contains(&name.as_str()) => {
                Some(format!("{name}!"))
            }
            [root, name]
                if FORBIDDEN_COMPILE_TIME_MACRO_ROOTS.contains(&root.as_str())
                    && FORBIDDEN_COMPILE_TIME_MACROS.contains(&name.as_str()) =>
            {
                Some(format!("{root}::{name}!"))
            }
            [alias] if aliases.contains(alias) => Some(format!("macro alias `{alias}!`")),
            _ => None,
        }
    }

    fn collect_forbidden_macro_aliases(
        mut prefix: Vec<String>,
        tree: &syn::UseTree,
        aliases: &mut BTreeSet<String>,
    ) {
        match tree {
            syn::UseTree::Path(path) => {
                prefix.push(path.ident.to_string());
                collect_forbidden_macro_aliases(prefix, &path.tree, aliases);
            }
            syn::UseTree::Name(name) => {
                prefix.push(name.ident.to_string());
                if is_forbidden_compile_time_macro_import(&prefix) {
                    aliases.insert(name.ident.to_string());
                }
            }
            syn::UseTree::Rename(rename) => {
                prefix.push(rename.ident.to_string());
                if is_forbidden_compile_time_macro_import(&prefix) {
                    aliases.insert(rename.rename.to_string());
                }
            }
            syn::UseTree::Glob(_) => {}
            syn::UseTree::Group(group) => {
                for tree in &group.items {
                    collect_forbidden_macro_aliases(prefix.clone(), tree, aliases);
                }
            }
        }
    }

    fn is_forbidden_compile_time_macro_import(path: &[String]) -> bool {
        matches!(path, [root, name]
            if FORBIDDEN_COMPILE_TIME_MACRO_ROOTS.contains(&root.as_str())
                && FORBIDDEN_COMPILE_TIME_MACROS.contains(&name.as_str()))
    }

    fn flatten_use_tree(
        mut prefix: Vec<String>,
        tree: &syn::UseTree,
        output: &mut Vec<Vec<String>>,
    ) {
        match tree {
            syn::UseTree::Path(path) => {
                prefix.push(path.ident.to_string());
                flatten_use_tree(prefix, &path.tree, output);
            }
            syn::UseTree::Name(name) => {
                prefix.push(name.ident.to_string());
                output.push(prefix);
            }
            syn::UseTree::Rename(rename) => {
                prefix.push(rename.ident.to_string());
                output.push(prefix);
            }
            syn::UseTree::Glob(_) => {
                prefix.push("*".to_string());
                output.push(prefix);
            }
            syn::UseTree::Group(group) => {
                for tree in &group.items {
                    flatten_use_tree(prefix.clone(), tree, output);
                }
            }
        }
    }

    #[derive(Clone, Copy, PartialEq, Eq)]
    enum CfgTruth {
        True,
        False,
        Unknown,
    }

    fn item_excluded_from_production(item: &Item) -> bool {
        item_attributes(item)
            .iter()
            .filter(|attribute| attribute.path().is_ident("cfg"))
            .any(|attribute| cfg_attribute_truth(attribute) == CfgTruth::False)
    }

    fn cfg_attribute_truth(attribute: &Attribute) -> CfgTruth {
        let Meta::List(list) = &attribute.meta else {
            return CfgTruth::Unknown;
        };
        syn::parse2::<Meta>(list.tokens.clone())
            .map(|meta| cfg_truth(&meta))
            .unwrap_or(CfgTruth::Unknown)
    }

    fn cfg_truth(meta: &Meta) -> CfgTruth {
        match meta {
            Meta::Path(path) if path.is_ident("test") => CfgTruth::False,
            Meta::Path(_) | Meta::NameValue(_) => CfgTruth::Unknown,
            Meta::List(list) => {
                let Ok(items) =
                    Punctuated::<Meta, Token![,]>::parse_terminated.parse2(list.tokens.clone())
                else {
                    return CfgTruth::Unknown;
                };
                if list.path.is_ident("all") {
                    fold_all(items.iter().map(cfg_truth))
                } else if list.path.is_ident("any") {
                    fold_any(items.iter().map(cfg_truth))
                } else if list.path.is_ident("not") && items.len() == 1 {
                    match cfg_truth(&items[0]) {
                        CfgTruth::True => CfgTruth::False,
                        CfgTruth::False => CfgTruth::True,
                        CfgTruth::Unknown => CfgTruth::Unknown,
                    }
                } else {
                    CfgTruth::Unknown
                }
            }
        }
    }

    fn fold_all(values: impl Iterator<Item = CfgTruth>) -> CfgTruth {
        let mut unknown = false;
        for value in values {
            match value {
                CfgTruth::False => return CfgTruth::False,
                CfgTruth::Unknown => unknown = true,
                CfgTruth::True => {}
            }
        }
        if unknown {
            CfgTruth::Unknown
        } else {
            CfgTruth::True
        }
    }

    fn fold_any(values: impl Iterator<Item = CfgTruth>) -> CfgTruth {
        let mut unknown = false;
        for value in values {
            match value {
                CfgTruth::True => return CfgTruth::True,
                CfgTruth::Unknown => unknown = true,
                CfgTruth::False => {}
            }
        }
        if unknown {
            CfgTruth::Unknown
        } else {
            CfgTruth::False
        }
    }

    fn item_attributes(item: &Item) -> &[Attribute] {
        match item {
            Item::Const(item) => &item.attrs,
            Item::Enum(item) => &item.attrs,
            Item::ExternCrate(item) => &item.attrs,
            Item::Fn(item) => &item.attrs,
            Item::ForeignMod(item) => &item.attrs,
            Item::Impl(item) => &item.attrs,
            Item::Macro(item) => &item.attrs,
            Item::Mod(item) => &item.attrs,
            Item::Static(item) => &item.attrs,
            Item::Struct(item) => &item.attrs,
            Item::Trait(item) => &item.attrs,
            Item::TraitAlias(item) => &item.attrs,
            Item::Type(item) => &item.attrs,
            Item::Union(item) => &item.attrs,
            Item::Use(item) => &item.attrs,
            Item::Verbatim(_) => &[],
            _ => &[],
        }
    }

    fn visit_rust_files(dir: &Path, visit: &mut impl FnMut(&Path)) {
        let mut entries = fs::read_dir(dir)
            .unwrap_or_else(|error| panic!("read {}: {error}", dir.display()))
            .map(|entry| entry.expect("directory entry").path())
            .collect::<Vec<_>>();
        entries.sort();
        for path in entries {
            if path.is_dir() {
                visit_rust_files(&path, visit);
            } else if path.extension().and_then(|extension| extension.to_str()) == Some("rs") {
                visit(&path);
            }
        }
    }
}
