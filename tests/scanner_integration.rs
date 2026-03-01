// Integration tests for the filesystem scanner.

use std::fs;
use tempfile::TempDir;
use tokio::sync::mpsc;

use freespace::core::scanner::{self, ScanMessage};
use freespace::module::manifest::{Module, Target};

fn make_global_module(name: &str, path: &str) -> Module {
    Module {
        name: name.to_string(),
        version: "1.0.0".to_string(),
        description: "test".to_string(),
        author: "tester".to_string(),
        platforms: vec!["macos".to_string(), "linux".to_string()],
        targets: vec![Target {
            path: Some(path.to_string()),
            name: None,
            indicator: None,
            description: None,
        }],
    }
}

fn make_local_module(name: &str, dir_name: &str, indicator: Option<&str>) -> Module {
    Module {
        name: name.to_string(),
        version: "1.0.0".to_string(),
        description: "test".to_string(),
        author: "tester".to_string(),
        platforms: vec!["macos".to_string(), "linux".to_string()],
        targets: vec![Target {
            path: None,
            name: Some(dir_name.to_string()),
            indicator: indicator.map(|s| s.to_string()),
            description: None,
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
    let mut got_complete = false;

    let timeout = tokio::time::sleep(std::time::Duration::from_secs(5));
    tokio::pin!(timeout);

    loop {
        tokio::select! {
            msg = rx.recv() => {
                match msg {
                    Some(ScanMessage::ItemDiscovered { module_index, item }) => {
                        assert_eq!(module_index, 0);
                        assert_eq!(item.size, Some(2048));
                        items_found += 1;
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
    assert!(got_complete);
}

#[tokio::test]
async fn scan_local_target_with_indicator() {
    let tmp = TempDir::new().unwrap();

    // Create: project-a/node_modules/ + project-a/package.json
    let project_a = tmp.path().join("project-a");
    fs::create_dir_all(project_a.join("node_modules")).unwrap();
    fs::write(project_a.join("package.json"), "{}").unwrap();
    fs::write(
        project_a.join("node_modules").join("dep.js"),
        vec![0u8; 512],
    )
    .unwrap();

    // Create: project-b/node_modules/ (NO package.json — should be skipped)
    let project_b = tmp.path().join("project-b");
    fs::create_dir_all(project_b.join("node_modules")).unwrap();

    let module = make_local_module("npm", "node_modules", Some("package.json"));
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

    // Only project-a should match (has package.json indicator)
    assert_eq!(items.len(), 1);
    assert!(items[0].contains("project-a"));
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
