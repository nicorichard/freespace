// Xcode simulator handlers.
//
// Simulator devices and runtimes are registered with CoreSimulator. Deleting
// their directories reclaims the bytes but leaves the registry inconsistent:
// Xcode keeps listing devices that cannot boot and runtimes that are gone.
// Both are removed through `simctl`, which updates the registry as it deletes.
//
// Neither operation is reversible — there is no Trash equivalent — so both
// handlers only ever act on one explicitly itemised identifier at a time. In
// particular `simctl delete all` and the batch `simctl delete unavailable` are
// never used.

use std::path::PathBuf;

use anyhow::Result;
use serde::Deserialize;

use super::exec;
use super::{Handler, HandlerItem};

/// Common probe: these tools only exist on macOS with Xcode's tools installed.
fn xcrun_available() -> bool {
    cfg!(target_os = "macos") && exec::program_exists("xcrun")
}

// ---------------------------------------------------------------------------
// Devices
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct DeviceList {
    devices: std::collections::HashMap<String, Vec<Device>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Device {
    udid: String,
    name: String,
    #[serde(default)]
    is_available: bool,
    #[serde(default)]
    state: String,
    #[serde(default)]
    data_path: Option<String>,
    #[serde(default)]
    data_path_size: Option<u64>,
    #[serde(default)]
    last_booted_at: Option<String>,
}

/// Simulator devices whose runtime is no longer installed.
pub struct SimulatorDevices;

impl SimulatorDevices {
    /// Parse `simctl list --json devices` output into cleanup candidates.
    ///
    /// Only unavailable devices are candidates: their runtime is gone, so they
    /// cannot boot and are pure dead weight. Available devices are working
    /// simulators and are never offered. Booted devices are skipped because
    /// deleting one out from under a running Simulator is a bad idea.
    fn parse(json: &str) -> Result<Vec<HandlerItem>> {
        let list: DeviceList = serde_json::from_str(json)?;
        let mut items = Vec::new();

        for (runtime, devices) in &list.devices {
            for device in devices {
                if device.is_available {
                    continue;
                }
                if !device.state.is_empty() && device.state != "Shutdown" {
                    continue;
                }

                let mut detail = vec![runtime_label(runtime)];
                if let Some(booted) = &device.last_booted_at {
                    detail.push(format!("last booted {}", short_date(booted)));
                } else {
                    detail.push("never booted".to_string());
                }

                items.push(HandlerItem {
                    id: device.udid.clone(),
                    name: device.name.clone(),
                    size: device.data_path_size,
                    // The device directory is the parent of `data`.
                    display_path: device
                        .data_path
                        .as_ref()
                        .map(PathBuf::from)
                        .and_then(|p| p.parent().map(PathBuf::from)),
                    detail: Some(detail.join(" · ")),
                    in_use: false,
                });
            }
        }

        items.sort_by(|a, b| b.size.cmp(&a.size).then_with(|| a.name.cmp(&b.name)));
        Ok(items)
    }
}

impl Handler for SimulatorDevices {
    fn id(&self) -> &'static str {
        "xcode.simulator-devices"
    }

    fn available(&self) -> bool {
        xcrun_available()
    }

    fn enumerate(&self) -> Result<Vec<HandlerItem>> {
        let json = exec::run_checked(&["xcrun", "simctl", "list", "--json", "devices"])?;
        Self::parse(&json)
    }

    fn describe_removal(&self, item_id: &str) -> String {
        exec::display_argv(&["xcrun", "simctl", "delete", item_id])
    }

    fn remove(&self, item_id: &str) -> Result<()> {
        exec::run_checked(&["xcrun", "simctl", "delete", item_id])?;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Runtimes
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Runtime {
    identifier: String,
    #[serde(default)]
    version: String,
    #[serde(default)]
    build: String,
    #[serde(default)]
    deletable: bool,
    #[serde(default)]
    size_bytes: Option<u64>,
    #[serde(default)]
    last_used_at: Option<String>,
    #[serde(default)]
    runtime_bundle_path: Option<String>,
    #[serde(default)]
    platform_identifier: Option<String>,
}

/// Installed simulator runtimes — multi-gigabyte platform images.
pub struct SimulatorRuntimes;

impl SimulatorRuntimes {
    /// Parse `simctl runtime list -j` output.
    ///
    /// Only `deletable` runtimes are candidates; a runtime pinned inside an
    /// installed Xcode reports `deletable: false` and must be left alone.
    /// `in_use_builds` holds the builds that installed Xcode SDKs actually
    /// resolve to (from `simctl runtime match list`); those are flagged rather
    /// than hidden, since removing one still works but forces a multi-gigabyte
    /// re-download the next time a build runs.
    fn parse(json: &str, in_use_builds: &[String]) -> Result<Vec<HandlerItem>> {
        let runtimes: std::collections::HashMap<String, Runtime> = serde_json::from_str(json)?;
        let mut items = Vec::new();

        for runtime in runtimes.values() {
            if !runtime.deletable {
                continue;
            }

            let in_use = in_use_builds.iter().any(|b| b == &runtime.build);

            let mut detail = Vec::new();
            if !runtime.build.is_empty() {
                detail.push(format!("build {}", runtime.build));
            }
            match &runtime.last_used_at {
                Some(used) => detail.push(format!("last used {}", short_date(used))),
                None => detail.push("never used".to_string()),
            }
            if in_use {
                detail.push("still selected by an installed Xcode SDK".to_string());
            }

            let platform = runtime
                .platform_identifier
                .as_deref()
                .map(platform_label)
                .unwrap_or("Simulator");

            items.push(HandlerItem {
                id: runtime.identifier.clone(),
                name: format!("{} {} runtime", platform, runtime.version),
                size: runtime.size_bytes,
                display_path: runtime.runtime_bundle_path.as_ref().map(PathBuf::from),
                detail: Some(detail.join(" · ")),
                in_use,
            });
        }

        items.sort_by(|a, b| b.size.cmp(&a.size).then_with(|| a.name.cmp(&b.name)));
        Ok(items)
    }

    /// Builds that installed Xcode SDKs currently resolve to, parsed from the
    /// `Chosen Runtime` lines of `simctl runtime match list`. A best-effort
    /// annotation: on failure we simply flag nothing.
    fn in_use_builds() -> Vec<String> {
        let Ok(output) = exec::run(&["xcrun", "simctl", "runtime", "match", "list"]) else {
            return Vec::new();
        };
        if output.status != Some(0) {
            return Vec::new();
        }

        output
            .stdout
            .lines()
            .filter_map(parse_chosen_runtime)
            .collect()
    }
}

impl Handler for SimulatorRuntimes {
    fn id(&self) -> &'static str {
        "xcode.simulator-runtimes"
    }

    fn available(&self) -> bool {
        xcrun_available()
    }

    fn enumerate(&self) -> Result<Vec<HandlerItem>> {
        let json = exec::run_checked(&["xcrun", "simctl", "runtime", "list", "-j"])?;
        Self::parse(&json, &Self::in_use_builds())
    }

    fn describe_removal(&self, item_id: &str) -> String {
        exec::display_argv(&["xcrun", "simctl", "runtime", "delete", item_id])
    }

    fn remove(&self, item_id: &str) -> Result<()> {
        exec::run_checked(&["xcrun", "simctl", "runtime", "delete", item_id])?;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Formatting helpers
// ---------------------------------------------------------------------------

/// Extract the build from one `Chosen Runtime:` line of
/// `simctl runtime match list`.
///
/// Two shapes appear: `<name> (<version> - <build>) - <identifier>` when an
/// installed runtime matched, or a bare SDK-default build when none did.
fn parse_chosen_runtime(line: &str) -> Option<String> {
    let rest = line.split_once("Chosen Runtime:")?.1.trim();
    let build = match rest.find('(') {
        Some(open) => rest[open + 1..]
            .split_once(')')
            .and_then(|(inner, _)| inner.rsplit(" - ").next())
            .map(str::trim)
            .unwrap_or(rest),
        None => rest,
    };
    (!build.is_empty()).then(|| build.to_string())
}

/// Turn `com.apple.CoreSimulator.SimRuntime.iOS-26-2` into `iOS 26.2`.
fn runtime_label(identifier: &str) -> String {
    let tail = identifier.rsplit('.').next().unwrap_or(identifier);
    match tail.split_once('-') {
        Some((platform, version)) => format!("{} {}", platform, version.replace('-', ".")),
        None => tail.to_string(),
    }
}

/// Turn `com.apple.platform.iphonesimulator` into `iOS`.
fn platform_label(identifier: &str) -> &'static str {
    match identifier.rsplit('.').next().unwrap_or("") {
        "iphonesimulator" => "iOS",
        "watchsimulator" => "watchOS",
        "appletvsimulator" => "tvOS",
        "xrsimulator" => "visionOS",
        _ => "Simulator",
    }
}

/// Trim an ISO-8601 timestamp to just the date.
fn short_date(timestamp: &str) -> &str {
    timestamp.split('T').next().unwrap_or(timestamp)
}

#[cfg(test)]
mod tests {
    use super::*;

    const DEVICES_JSON: &str = r#"{
      "devices" : {
        "com.apple.CoreSimulator.SimRuntime.iOS-26-2" : [
          {
            "lastBootedAt" : "2026-07-23T01:26:48Z",
            "dataPath" : "/Users/x/Library/Developer/CoreSimulator/Devices/AAA/data",
            "dataPathSize" : 3035803648,
            "udid" : "AAA",
            "isAvailable" : true,
            "state" : "Shutdown",
            "name" : "iPhone 17 Pro"
          }
        ],
        "com.apple.CoreSimulator.SimRuntime.iOS-16-4" : [
          {
            "dataPath" : "/Users/x/Library/Developer/CoreSimulator/Devices/BBB/data",
            "dataPathSize" : 500,
            "udid" : "BBB",
            "isAvailable" : false,
            "state" : "Shutdown",
            "name" : "iPhone 14"
          },
          {
            "dataPath" : "/Users/x/Library/Developer/CoreSimulator/Devices/CCC/data",
            "dataPathSize" : 900,
            "udid" : "CCC",
            "isAvailable" : false,
            "state" : "Booted",
            "name" : "iPhone 13"
          }
        ]
      }
    }"#;

    #[test]
    fn devices_only_offers_unavailable_shutdown_devices() {
        let items = SimulatorDevices::parse(DEVICES_JSON).unwrap();
        let ids: Vec<_> = items.iter().map(|i| i.id.as_str()).collect();
        assert_eq!(
            ids,
            vec!["BBB"],
            "available and booted devices are excluded"
        );
    }

    #[test]
    fn devices_carry_size_and_device_directory() {
        let items = SimulatorDevices::parse(DEVICES_JSON).unwrap();
        let item = &items[0];
        assert_eq!(item.size, Some(500));
        assert_eq!(item.name, "iPhone 14");
        // The device directory, not the `data` subdirectory.
        assert_eq!(
            item.display_path.as_deref().unwrap(),
            std::path::Path::new("/Users/x/Library/Developer/CoreSimulator/Devices/BBB")
        );
        assert!(item.detail.as_ref().unwrap().contains("iOS 16.4"));
    }

    #[test]
    fn devices_removal_command_is_itemised() {
        let cmd = SimulatorDevices.describe_removal("BBB");
        assert_eq!(cmd, "xcrun simctl delete BBB");
        assert!(!cmd.contains("all"));
        assert!(!cmd.contains("unavailable"));
    }

    const RUNTIMES_JSON: &str = r#"{
      "R1" : {
        "build" : "23C54",
        "deletable" : true,
        "identifier" : "R1",
        "lastUsedAt" : "2026-07-23T01:26:48Z",
        "platformIdentifier" : "com.apple.platform.iphonesimulator",
        "runtimeBundlePath" : "/Library/Developer/CoreSimulator/Volumes/iOS_23C54/x.simruntime",
        "sizeBytes" : 8381044573,
        "version" : "26.2"
      },
      "R2" : {
        "build" : "21A00",
        "deletable" : false,
        "identifier" : "R2",
        "platformIdentifier" : "com.apple.platform.watchsimulator",
        "sizeBytes" : 100,
        "version" : "10.0"
      }
    }"#;

    #[test]
    fn runtimes_skip_non_deletable() {
        let items = SimulatorRuntimes::parse(RUNTIMES_JSON, &[]).unwrap();
        let ids: Vec<_> = items.iter().map(|i| i.id.as_str()).collect();
        assert_eq!(ids, vec!["R1"], "pinned runtimes are never offered");
    }

    #[test]
    fn runtimes_report_size_and_label() {
        let items = SimulatorRuntimes::parse(RUNTIMES_JSON, &[]).unwrap();
        assert_eq!(items[0].size, Some(8381044573));
        assert_eq!(items[0].name, "iOS 26.2 runtime");
        assert!(!items[0].in_use);
    }

    #[test]
    fn runtimes_flag_builds_still_chosen_by_an_sdk() {
        let items = SimulatorRuntimes::parse(RUNTIMES_JSON, &["23C54".to_string()]).unwrap();
        assert!(items[0].in_use);
        assert!(items[0].detail.as_ref().unwrap().contains("still selected"));
    }

    #[test]
    fn runtimes_removal_command_uses_supported_api() {
        assert_eq!(
            SimulatorRuntimes.describe_removal("R1"),
            "xcrun simctl runtime delete R1"
        );
    }

    #[test]
    fn runtime_labels_are_human_readable() {
        assert_eq!(
            runtime_label("com.apple.CoreSimulator.SimRuntime.iOS-26-2"),
            "iOS 26.2"
        );
        assert_eq!(
            platform_label("com.apple.platform.watchsimulator"),
            "watchOS"
        );
        assert_eq!(short_date("2026-07-23T01:26:48Z"), "2026-07-23");
    }

    /// Exercises the real `simctl` on this machine. Ignored by default because
    /// it depends on what Xcode is installed; run with
    /// `cargo test -- --ignored live_simctl`.
    #[test]
    #[ignore]
    fn live_simctl_enumeration() {
        if !xcrun_available() {
            eprintln!("xcrun unavailable; nothing to check");
            return;
        }
        for handler in [
            &SimulatorDevices as &dyn Handler,
            &SimulatorRuntimes as &dyn Handler,
        ] {
            let items = handler
                .enumerate()
                .unwrap_or_else(|e| panic!("{} failed: {e}", handler.id()));
            eprintln!("{}: {} item(s)", handler.id(), items.len());
            for item in &items {
                eprintln!(
                    "  {} [{}] size={:?} in_use={} detail={:?}\n    -> {}",
                    item.name,
                    item.id,
                    item.size,
                    item.in_use,
                    item.detail,
                    handler.describe_removal(&item.id)
                );
                assert!(!item.id.is_empty(), "every item needs a stable id");
            }
        }
    }

    #[test]
    fn chosen_runtime_lines_yield_builds() {
        // Both shapes simctl emits: a matched runtime, and a bare SDK default.
        let matched = "    Chosen Runtime: iOS 26.2 (26.2 - 23C54) - com.apple.CoreSimulator.SimRuntime.iOS-26-2";
        let bare = "    Chosen Runtime: 23K50";
        assert_eq!(parse_chosen_runtime(matched).as_deref(), Some("23C54"));
        assert_eq!(parse_chosen_runtime(bare).as_deref(), Some("23K50"));
        assert_eq!(parse_chosen_runtime("    SDK Build: 23C53"), None);
    }

    #[test]
    fn malformed_json_is_an_error_not_a_panic() {
        assert!(SimulatorDevices::parse("{ not json").is_err());
        assert!(SimulatorRuntimes::parse("{ not json", &[]).is_err());
    }
}
