// Integration tests for the filesystem scanner.

use std::fs;
use tempfile::TempDir;
use tokio::sync::mpsc;

use freespace::core::scanner::{self, ScanMessage};
use freespace::module::manifest::{Module, Target};

fn make_global_module(name: &str, path: &str) -> Module {
    Module {
        id: name.to_string(),
        name: name.to_string(),
        version: "1.0.0".to_string(),
        description: "test".to_string(),
        author: "tester".to_string(),
        platforms: vec!["macos".to_string(), "linux".to_string()],
        tags: vec![],
        icon: None,
        icon_color: None,
        targets: vec![Target {
            source: freespace::module::manifest::TargetSource::Paths(vec![path.to_string()]),
            description: None,
            restore: freespace::module::manifest::RestoreKind::default(),
            restore_steps: None,
            risk: freespace::module::manifest::RiskLevel::default(),
            ignore: vec![],
        }],
    }
}

fn make_local_module(name: &str, dir_name: &str) -> Module {
    Module {
        id: name.to_string(),
        name: name.to_string(),
        version: "1.0.0".to_string(),
        description: "test".to_string(),
        author: "tester".to_string(),
        platforms: vec!["macos".to_string(), "linux".to_string()],
        tags: vec![],
        icon: None,
        icon_color: None,
        targets: vec![Target {
            source: freespace::module::manifest::TargetSource::Paths(vec![format!(
                "**/{}",
                dir_name
            )]),
            description: None,
            restore: freespace::module::manifest::RestoreKind::default(),
            restore_steps: None,
            risk: freespace::module::manifest::RiskLevel::default(),
            ignore: vec![],
        }],
    }
}

#[tokio::test]
async fn scan_global_target() {
    let tmp = TempDir::new().unwrap();
    let cache_dir = tmp.path().join("cache");
    fs::create_dir(&cache_dir).unwrap();
    fs::write(cache_dir.join("data.bin"), vec![0u8; 2048]).unwrap();

    let module = make_global_module("test-cache", cache_dir.to_str().unwrap());

    let (tx, mut rx) = mpsc::unbounded_channel();
    scanner::start_scan(vec![module], tx, vec![]);

    let mut items_found = 0;
    let mut sized_count = 0;
    let mut got_complete = false;

    let timeout = tokio::time::sleep(std::time::Duration::from_secs(5));
    tokio::pin!(timeout);

    loop {
        tokio::select! {
            msg = rx.recv() => {
                match msg {
                    Some(ScanMessage::ItemDiscovered { module_index, item }) => {
                        assert_eq!(module_index, 0);
                        // Discovery phase: size is None
                        assert!(item.size.is_none());
                        items_found += 1;
                    }
                    Some(ScanMessage::ItemSized { module_index, size, .. }) => {
                        assert_eq!(module_index, 0);
                        // Disk usage may be >= written bytes due to block alignment
                        assert!(size >= 2048);
                        sized_count += 1;
                    }
                    Some(ScanMessage::ModuleComplete { .. }) => {
                        got_complete = true;
                    }
                    Some(ScanMessage::ScanComplete) => break,
                    None => break,
                    _ => {}
                }
            }
            _ = &mut timeout => panic!("scan timed out"),
        }
    }

    assert_eq!(items_found, 1);
    assert_eq!(sized_count, 1);
    assert!(got_complete);
}

#[tokio::test]
async fn scan_local_target() {
    let tmp = TempDir::new().unwrap();

    // Create: project-a/node_modules/
    let project_a = tmp.path().join("project-a");
    fs::create_dir_all(project_a.join("node_modules")).unwrap();
    fs::write(
        project_a.join("node_modules").join("dep.js"),
        vec![0u8; 512],
    )
    .unwrap();

    // Create: project-b/node_modules/
    let project_b = tmp.path().join("project-b");
    fs::create_dir_all(project_b.join("node_modules")).unwrap();

    let module = make_local_module("npm", "node_modules");
    let search_dirs = vec![tmp.path().to_path_buf()];

    let (tx, mut rx) = mpsc::unbounded_channel();
    scanner::start_scan(vec![module], tx, search_dirs);

    let mut items: Vec<String> = Vec::new();

    let timeout = tokio::time::sleep(std::time::Duration::from_secs(5));
    tokio::pin!(timeout);

    loop {
        tokio::select! {
            msg = rx.recv() => {
                match msg {
                    Some(ScanMessage::ItemDiscovered { item, .. }) => {
                        items.push(item.name.clone());
                    }
                    Some(ScanMessage::ScanComplete) => break,
                    None => break,
                    _ => {}
                }
            }
            _ = &mut timeout => panic!("scan timed out"),
        }
    }

    // Both projects should match
    assert_eq!(items.len(), 2);
}

#[tokio::test]
async fn scan_multiple_modules() {
    let tmp = TempDir::new().unwrap();
    let cache_a = tmp.path().join("cache-a");
    let cache_b = tmp.path().join("cache-b");
    fs::create_dir(&cache_a).unwrap();
    fs::create_dir(&cache_b).unwrap();
    fs::write(cache_a.join("data"), vec![0u8; 100]).unwrap();
    fs::write(cache_b.join("data"), vec![0u8; 200]).unwrap();

    let modules = vec![
        make_global_module("mod-a", cache_a.to_str().unwrap()),
        make_global_module("mod-b", cache_b.to_str().unwrap()),
    ];

    let (tx, mut rx) = mpsc::unbounded_channel();
    scanner::start_scan(modules, tx, vec![]);

    let mut module_complete_count = 0;
    let mut item_count = 0;

    let timeout = tokio::time::sleep(std::time::Duration::from_secs(5));
    tokio::pin!(timeout);

    loop {
        tokio::select! {
            msg = rx.recv() => {
                match msg {
                    Some(ScanMessage::ItemDiscovered { .. }) => item_count += 1,
                    Some(ScanMessage::ModuleComplete { .. }) => module_complete_count += 1,
                    Some(ScanMessage::ScanComplete) => break,
                    None => break,
                    _ => {}
                }
            }
            _ = &mut timeout => panic!("scan timed out"),
        }
    }

    assert_eq!(item_count, 2);
    assert_eq!(module_complete_count, 2);
}

#[tokio::test]
async fn scan_glob_pattern() {
    let tmp = TempDir::new().unwrap();
    fs::create_dir(tmp.path().join("dir-one")).unwrap();
    fs::create_dir(tmp.path().join("dir-two")).unwrap();
    fs::write(tmp.path().join("file.txt"), b"").unwrap();

    let pattern = format!("{}/dir-*", tmp.path().display());
    let module = make_global_module("glob-test", &pattern);

    let (tx, mut rx) = mpsc::unbounded_channel();
    scanner::start_scan(vec![module], tx, vec![]);

    let mut item_count = 0;

    let timeout = tokio::time::sleep(std::time::Duration::from_secs(5));
    tokio::pin!(timeout);

    loop {
        tokio::select! {
            msg = rx.recv() => {
                match msg {
                    Some(ScanMessage::ItemDiscovered { .. }) => item_count += 1,
                    Some(ScanMessage::ScanComplete) => break,
                    None => break,
                    _ => {}
                }
            }
            _ = &mut timeout => panic!("scan timed out"),
        }
    }

    assert_eq!(item_count, 2); // dir-one and dir-two, not file.txt
}

/// End-to-end: the built-in `xcode-simulators` manifest resolves its handlers
/// and the scanner emits correctly-shaped items for them.
///
/// Ignored because what it finds depends on the machine's Xcode install; run
/// with `cargo test --test scanner_integration -- --ignored`.
#[tokio::test]
#[ignore]
async fn xcode_simulators_module_scans_end_to_end() {
    use freespace::core::cleaner::CleanupAction;
    use freespace::core::scanner::ScanMessage;

    let (builtins, warnings) = freespace::module::catalog::load_catalog(&[]);
    assert!(warnings.is_empty(), "catalog warnings: {warnings:?}");

    let Some(module) = builtins.into_iter().find(|m| m.id == "xcode-simulators") else {
        eprintln!("xcode-simulators not available on this platform");
        return;
    };

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    freespace::core::scanner::start_scan(vec![module], tx, Vec::new());

    let mut items = Vec::new();
    while let Some(msg) = rx.recv().await {
        match msg {
            ScanMessage::ItemDiscovered { item, .. } => items.push(item),
            ScanMessage::ModuleError { error, .. } => panic!("scan error: {error}"),
            ScanMessage::ScanComplete => break,
            _ => {}
        }
    }

    eprintln!("discovered {} simulator item(s)", items.len());
    for item in &items {
        eprintln!("  {} ({:?} bytes)", item.name, item.size);

        // Every item must be handler-backed, sized up front, and irreversible.
        assert!(
            matches!(item.action, CleanupAction::Handler { .. }),
            "{} should be handler-backed",
            item.name
        );
        assert!(!item.reversible(), "simulator removals cannot be undone");
        assert!(item.size.is_some(), "handlers supply their own sizes");

        // The identity must be synthetic, never a real path we might unlink.
        assert!(item.path.is_relative(), "identity must be relative");
        assert!(!item.path.exists(), "identity must not be a real file");

        // And the command shown must be the supported simctl API.
        let command = item.action.removal_command().expect("handler command");
        assert!(command.starts_with("xcrun simctl "), "got: {command}");
        eprintln!("    -> {command}");
    }
}

/// A handler failure must reach the user with its command's error text intact,
/// rather than being flattened into a generic "blocked by safety rules" flash.
#[test]
fn handler_failure_reason_reaches_the_result() {
    use freespace::core::cleaner::{delete_items, CleanupAction, CleanupItem, CleanupOptions};
    use std::sync::atomic::AtomicBool;

    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
    let cancel = AtomicBool::new(false);
    let opts = CleanupOptions {
        audit_log: false,
        ..Default::default()
    };

    // A handler id that is not registered fails without touching the system.
    let items = vec![CleanupItem {
        action: CleanupAction::Handler {
            handler: "not.a.real.handler",
            id: "XYZ".to_string(),
        },
        ..CleanupItem::from(std::path::PathBuf::from("freespace-handler/x/XYZ"))
    }];

    let result = delete_items(&items, &opts, &cancel, &tx);
    assert!(result.succeeded.is_empty());
    assert_eq!(result.failed.len(), 1);
    assert!(
        result.failed[0].1.contains("not.a.real.handler"),
        "reason should name the failing handler, got: {}",
        result.failed[0].1
    );
}
