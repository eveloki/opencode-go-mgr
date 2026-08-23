//! Inference Gateway protocol, policy, and execution boundaries.

/// Data-only single-attempt transport boundary.
///
/// Public only as the cross-crate bridge; the host crate's `gateway::attempt`
/// facade keeps these items crate-private.
#[doc(hidden)]
pub mod attempt;

#[cfg(test)]
mod source_boundary {
    use std::fs;
    use std::path::{Path, PathBuf};
    use syn::parse::Parser;
    use syn::punctuated::Punctuated;
    use syn::visit::{self, Visit};
    use syn::{Attribute, ExprCall, ExprMethodCall, Item, ItemUse, LitStr, Macro, Meta, Token};

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

    #[test]
    fn ocg_gateway_dependencies_stay_inside_the_slice_boundary() {
        let manifest_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml");
        let manifest = fs::read_to_string(&manifest_path)
            .unwrap_or_else(|error| panic!("read {}: {error}", manifest_path.display()));
        assert_manifest_boundary(&manifest_path, &manifest);
    }

    #[test]
    fn production_sources_name_no_host_io_or_credential_storage() {
        let src_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut scanned = Vec::new();
        visit_rust_files(&src_root, &mut |path| {
            scanned.push(path.to_path_buf());
            let source = fs::read_to_string(path)
                .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
            assert_production_source_boundary(path, &source);
        });
        for required in ["attempt.rs", "lib.rs"] {
            assert!(
                scanned.iter().any(|path| {
                    path.file_name().and_then(|name| name.to_str()) == Some(required)
                }),
                "source boundary guard must scan {required}, scanned={scanned:?}"
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
                fn helper() { let _ = std::fs::read("fixture"); }
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
        assert!(
            std::panic::catch_unwind(|| {
                assert_production_source_boundary(Path::new("fixture.rs"), source);
            })
            .is_err(),
            "source guard must reject: {source}"
        );
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
            if segments.starts_with(&["std".to_string(), "fs".to_string()])
                || segments.starts_with(&["std".to_string(), "process".to_string()])
            {
                self.violations.push(segments.join("::"));
            }
            visit::visit_path(self, path);
        }

        fn visit_item_use(&mut self, item: &'ast ItemUse) {
            let mut paths = Vec::new();
            flatten_use_tree(Vec::new(), &item.tree, &mut paths);
            for path in paths {
                if path.starts_with(&["std".to_string(), "fs".to_string()])
                    || path.starts_with(&["std".to_string(), "process".to_string()])
                {
                    self.violations.push(path.join("::"));
                }
            }
            visit::visit_item_use(self, item);
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
            let compact = mac
                .tokens
                .to_string()
                .chars()
                .filter(|ch| !ch.is_whitespace())
                .collect::<String>();
            for forbidden in [
                "std::fs",
                "std::process",
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
            ] {
                if compact.contains(forbidden) {
                    self.violations.push(format!("macro token `{forbidden}`"));
                }
            }
            visit::visit_macro(self, mac);
        }
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
            syn::UseTree::Glob(_) => output.push(prefix),
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
