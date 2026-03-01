// Integration tests for module installation from local paths.

use tempfile::TempDir;

use freespace::module::installer;

fn fixture_path(name: &str) -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("modules")
        .join(name)
}

#[test]
fn install_single_module_from_local_path() {
    let dest = TempDir::new().unwrap();
    let source = fixture_path("single-module");

    let results = installer::install(source.to_str().unwrap(), dest.path()).unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].name, "test-single");

    // Verify symlink was created
    let installed = &results[0].installed_to;
    assert!(installed.exists());
    assert!(installed.symlink_metadata().unwrap().is_symlink());
    assert!(installed.join("module.toml").exists());
}

#[test]
fn install_nonexistent_local_path() {
    let dest = TempDir::new().unwrap();
    let result = installer::install("/nonexistent/path/xyz123", dest.path());
    assert!(result.is_err());
}

#[test]
fn read_source_info_from_fixture() {
    // Fixture doesn't have source.toml — should return None
    let source = fixture_path("single-module");
    assert!(installer::read_source_info(&source).is_none());
}

#[test]
#[ignore] // Requires network access
fn install_from_github() {
    // This test is ignored by default — run with: cargo test -- --ignored
    // It would test: installer::install("github:user/repo", dest.path())
}
