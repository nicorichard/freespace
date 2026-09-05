// The built-in module catalog, vendored into the binary at compile time.
//
// These manifests ship with freespace so the tool is complete on first launch
// with no install step. They are ordinary `module.toml` files living in
// `catalog/` and parsed with the same `Module::parse` as user modules — the
// only difference is that they are embedded rather than read from disk.
//
// Users can disable individual catalog entries (or the whole catalog) via the
// `[modules]` section of `config.toml`, and can override any entry by
// installing a user module with the same id.

use crate::module::manifest::Module;

include!(concat!(env!("OUT_DIR"), "/catalog.rs"));

/// Parse every embedded manifest, filtering out other platforms and any ids the
/// user has disabled. Parse failures become warnings, matching
/// [`crate::module::manager::load_builtin_modules`].
pub fn load_catalog(disabled: &[String]) -> (Vec<Module>, Vec<String>) {
    let mut modules = Vec::new();
    let mut warnings = Vec::new();
    let current_platform = super::manager::current_platform();

    for (dir_name, contents) in CATALOG {
        match Module::parse(contents) {
            Ok(module) => {
                if disabled.iter().any(|d| d == &module.id) {
                    continue;
                }
                if module.platforms.iter().any(|p| p == &current_platform) {
                    modules.push(module);
                }
            }
            Err(e) => {
                // Unreachable in a tested build — `catalog_manifests_all_parse`
                // fails the test suite first — but a stray manifest should never
                // take the whole app down.
                warnings.push(format!("built-in module '{}' is invalid: {}", dir_name, e));
            }
        }
    }

    (modules, warnings)
}

/// Ids of every module in the catalog for the current platform, ignoring the
/// user's disabled list. Used to report what is available to re-enable.
pub fn catalog_ids() -> Vec<String> {
    let current_platform = super::manager::current_platform();
    CATALOG
        .iter()
        .filter_map(|(_, contents)| Module::parse(contents).ok())
        .filter(|m| m.platforms.iter().any(|p| p == &current_platform))
        .map(|m| m.id)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_is_not_empty() {
        assert!(
            !CATALOG.is_empty(),
            "the vendored catalog should contain modules"
        );
    }

    /// Every embedded manifest must parse. This is the guard that turns a
    /// malformed catalog entry into a test failure rather than a silent runtime
    /// warning that leaves the module missing from the UI.
    #[test]
    fn catalog_manifests_all_parse() {
        for (dir_name, contents) in CATALOG {
            if let Err(e) = Module::parse(contents) {
                panic!("catalog/{}/module.toml failed to parse: {}", dir_name, e);
            }
        }
    }

    /// Module ids must be unique, otherwise user-override precedence and the
    /// disabled list become ambiguous.
    #[test]
    fn catalog_ids_are_unique() {
        let mut seen = std::collections::HashSet::new();
        for (dir_name, contents) in CATALOG {
            let module = Module::parse(contents).expect("catalog manifest parses");
            assert!(
                seen.insert(module.id.clone()),
                "duplicate catalog module id '{}' (catalog/{})",
                module.id,
                dir_name
            );
        }
    }

    #[test]
    fn disabled_ids_are_filtered_out() {
        let (all, _) = load_catalog(&[]);
        assert!(!all.is_empty(), "expected catalog modules on this platform");
        let first = all[0].id.clone();

        let (filtered, _) = load_catalog(std::slice::from_ref(&first));
        assert!(
            !filtered.iter().any(|m| m.id == first),
            "disabled id '{}' should not load",
            first
        );
        assert_eq!(filtered.len(), all.len() - 1);
    }
}
