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

#[cfg(test)]
mod source_boundary {
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
    ];
    const FORBIDDEN_STD_MODULES: &[&str] = &["env", "fs", "net", "process"];
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
        for required in ["alias.rs", "attempt.rs", "classify.rs", "lib.rs"] {
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

    fn assert_production_source_boundary(path: &Path, source: &str) {
        let syntax = syn::parse_file(source)
            .unwrap_or_else(|error| panic!("parse {}: {error}", path.display()));
        let mut visitor = BoundaryVisitor::default();
        visitor.visit_file(&syntax);
        assert!(
            visitor.violations.is_empty(),
            "{} production source violates ocg-gateway boundary: {:?}",
            path.display(),
            visitor.violations
        );
    }

    #[derive(Default)]
    struct BoundaryVisitor {
        violations: Vec<String>,
    }

    impl<'ast> Visit<'ast> for BoundaryVisitor {
        fn visit_item(&mut self, item: &'ast Item) {
            if item_excluded_from_production(item) {
                return;
            }
            visit::visit_item(self, item);
        }

        fn visit_ident(&mut self, ident: &'ast syn::Ident) {
            let name = ident.to_string();
            if FORBIDDEN_IDENTIFIERS.contains(&name.as_str()) {
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
            if is_include_macro_path(&mac.path) {
                self.violations.push("include!".to_string());
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

    fn is_include_macro_path(path: &syn::Path) -> bool {
        path.is_ident("include")
            || path
                .segments
                .iter()
                .map(|segment| segment.ident.to_string())
                .eq(["std", "include"])
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
