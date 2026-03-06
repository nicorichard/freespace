// Integration tests for the filesystem scanner.

use std::fs;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use tempfile::TempDir;
use tokio::sync::mpsc;

use freespace::core::cache::SizeCache;
use freespace::core::scanner::{self, ScanMessage};
use freespace::module::manifest::{Module, Target};

fn empty_cache() -> Arc<Mutex<SizeCache>> {
    Arc::new(Mutex::new(SizeCache::empty()))
}

fn no_cancel() -> Arc<AtomicBool> {
    Arc::new(AtomicBool::new(false))
}

fn make_global_module(name: &str, path: &str) -> Module {
    Module {
        id: name.to_string(),
        name: name.to_string(),
        version: "1.0.0".to_string(),
        description: "test".to_string(),
        author: "tester".to_string(),
        platforms: vec!["macos".to_string(), "linux".to_string()],
        targets: vec![Target {
            path: path.to_string(),
            description: None,
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
        targets: vec![Target {
            path: format!("**/{}", dir_name),
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
    scanner::start_scan(vec![module], tx, vec![], empty_cache(), no_cancel());

    let mut items_found = 0;
    let mut sized_count = 0;
    let mut got_complete = false;

    let timeout = tokio::time::sleep(std::time::Duration::from_secs(5));
    tokio::pin!(timeout);

    loop {
        tokio::select! {
            msg = rx.recv() => {
                match msg {
                    Some(ScanMessage::ItemDiscovered { module_index, .. }) => {
                        assert_eq!(module_index, 0);
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
    scanner::start_scan(vec![module], tx, search_dirs, empty_cache(), no_cancel());

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
    scanner::start_scan(modules, tx, vec![], empty_cache(), no_cancel());

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
    scanner::start_scan(vec![module], tx, vec![], empty_cache(), no_cancel());

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
