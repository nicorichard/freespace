//! Live checks against the machine's own `simctl`.
//!
//! Ignored by default: they create a simulator device and then delete it
//! through the handler, so they mutate real CoreSimulator state. The device is
//! one this test makes; nothing pre-existing is touched.
//!
//! Run with `cargo test --test xcode_handler_live -- --ignored --nocapture`.

use std::process::Command;

use freespace::core::handlers::xcode::SimulatorDevices;
use freespace::core::handlers::Handler;

const TEST_DEVICE_NAME: &str = "freespace-live-test-device";

/// Run a simctl subcommand, returning stdout on success.
fn simctl(args: &[&str]) -> Result<String, String> {
    let output = Command::new("xcrun")
        .arg("simctl")
        .args(args)
        .output()
        .map_err(|e| format!("spawning xcrun: {e}"))?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// A device type and runtime the machine can actually instantiate, borrowed
/// from a device that already exists.
fn creatable_pair() -> Option<(String, String)> {
    let json = simctl(&["list", "--json", "devices"]).ok()?;
    let parsed: serde_json::Value = serde_json::from_str(&json).ok()?;
    for (runtime, devices) in parsed.get("devices")?.as_object()? {
        for device in devices.as_array()? {
            if device.get("isAvailable")?.as_bool() != Some(true) {
                continue;
            }
            if let Some(kind) = device.get("deviceTypeIdentifier").and_then(|v| v.as_str()) {
                return Some((kind.to_string(), runtime.clone()));
            }
        }
    }
    None
}

/// Whether simctl still knows about a UDID.
fn device_exists(udid: &str) -> bool {
    simctl(&["list", "--json", "devices"]).is_ok_and(|json| json.contains(udid))
}

/// The device handler's removal really does unregister a device.
///
/// Unavailable devices are the only ones the handler ever offers, and a healthy
/// machine has none — so this drives `remove` directly against a device the
/// test creates for the purpose.
#[test]
#[ignore]
fn simulator_device_removal_runs_against_real_simctl() {
    let handler = SimulatorDevices;
    if !handler.available() {
        eprintln!("xcrun is unavailable — skipping");
        return;
    }

    let Some((device_type, runtime)) = creatable_pair() else {
        eprintln!("no installed runtime to create a device on — skipping");
        return;
    };

    let udid = match simctl(&["create", TEST_DEVICE_NAME, &device_type, &runtime]) {
        Ok(out) => out.trim().to_string(),
        Err(e) => {
            eprintln!("could not create a scratch device ({e}) — skipping");
            return;
        }
    };
    eprintln!("created {TEST_DEVICE_NAME} {udid} on {runtime}");

    assert_eq!(udid.len(), 36, "simctl create returns a UDID, got {udid:?}");
    assert!(
        device_exists(&udid),
        "the scratch device should be registered"
    );

    assert_eq!(
        handler.describe_removal(&udid),
        format!("xcrun simctl delete {udid}")
    );

    let removal = handler.remove(&udid);
    assert!(
        !device_exists(&udid),
        "{udid} is still registered; delete it with `xcrun simctl delete {udid}`"
    );
    removal.expect("handler removal should succeed");
    eprintln!("removed {udid} through the handler");
}
