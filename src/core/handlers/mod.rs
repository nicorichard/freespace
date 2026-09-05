// Built-in cleanup handlers.
//
// A handler encapsulates knowledge that is *not* path-shaped: how to correctly
// enumerate and remove a class of disk consumer using its own tool's supported
// interface. Trashing `~/Library/Developer/CoreSimulator/Devices` removes the
// bytes but leaves CoreSimulator's registry pointing at devices that no longer
// exist; `xcrun simctl delete <UDID>` is the correct operation. That knowledge
// is code, so it lives here in the binary rather than in a manifest.
//
// Manifests select a handler by name (`handler = "xcode.simulator-devices"`)
// from the fixed set registered below, and `Module::parse` rejects any name not
// in it. A manifest therefore cannot introduce a new command — it can only ask
// freespace to do something it already knows how to do — which is what keeps
// modules safe to install without a trust model.

pub mod exec;
pub mod xcode;

use std::path::PathBuf;

/// One item discovered by a handler.
pub struct HandlerItem {
    /// Stable identifier passed back to [`Handler::remove`] (e.g. a UDID).
    pub id: String,
    /// Display name for the item list.
    pub name: String,
    /// Size in bytes, supplied by the handler. Handlers generally get this from
    /// their tool directly, so these items need no filesystem sizing pass.
    pub size: Option<u64>,
    /// Real filesystem location, when one exists. Shown to the user for
    /// context; it is **not** what gets removed.
    pub display_path: Option<PathBuf>,
    /// Extra context shown beneath the item (version, last-used date, ...).
    pub detail: Option<String>,
    /// Set when the item is still usable and removing it has a real cost, so
    /// the UI can warn beyond the target's declared risk level.
    pub in_use: bool,
}

/// A built-in integration with an external tool.
pub trait Handler: Send + Sync {
    /// Stable id used in manifests. Namespaced by tool, e.g. `xcode.simulator-devices`.
    fn id(&self) -> &'static str;

    /// Whether this handler can run here — right platform, tool installed.
    /// When false the target yields no items and no error.
    fn available(&self) -> bool;

    /// Discover removable items.
    fn enumerate(&self) -> anyhow::Result<Vec<HandlerItem>>;

    /// The exact command that [`Handler::remove`] will run, for display in the
    /// confirmation screen. What the user is shown must be what runs.
    fn describe_removal(&self, item_id: &str) -> String;

    /// Remove one item. `item_id` is a [`HandlerItem::id`] this handler produced.
    fn remove(&self, item_id: &str) -> anyhow::Result<()>;
}

/// Every handler compiled into this binary.
static HANDLERS: &[&(dyn Handler + Sync)] = &[&xcode::SimulatorDevices, &xcode::SimulatorRuntimes];

/// Look up a handler by manifest id.
pub fn get(id: &str) -> Option<&'static (dyn Handler + Sync)> {
    HANDLERS.iter().copied().find(|h| h.id() == id)
}

/// The canonical `&'static str` for a handler id, so parsed manifests can hold
/// a `&'static str` rather than an owned string.
pub fn resolve_id(id: &str) -> Option<&'static str> {
    get(id).map(|h| h.id())
}

/// All registered handler ids, for error messages and documentation.
pub fn all_ids() -> Vec<&'static str> {
    HANDLERS.iter().map(|h| h.id()).collect()
}

/// Prefix marking a synthetic handler-item identity path.
pub const IDENTITY_PREFIX: &str = "freespace-handler";

/// Build the synthetic identity used to key a handler item through the app's
/// path-based selection machinery.
///
/// Handler items cannot be keyed by their real path: simulator runtimes live
/// under `/System/Library/AssetsV2` and `/Library/Developer/CoreSimulator`,
/// which `safety.rs` deny- and warn-lists, and in any case the path is not what
/// gets removed. The identity is deliberately **relative**, so it can never
/// collide with a real target path (all of which are absolute after tilde
/// expansion) and can never be mistaken for something to unlink.
pub fn identity_path(handler_id: &str, item_id: &str) -> PathBuf {
    PathBuf::from(IDENTITY_PREFIX)
        .join(handler_id)
        .join(item_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registered_ids_resolve() {
        for id in all_ids() {
            assert!(get(id).is_some(), "handler '{id}' should resolve");
            assert_eq!(resolve_id(id), Some(id));
        }
    }

    #[test]
    fn unknown_id_does_not_resolve() {
        assert!(get("not.a.handler").is_none());
        assert!(resolve_id("not.a.handler").is_none());
    }

    #[test]
    fn handler_ids_are_unique() {
        let mut seen = std::collections::HashSet::new();
        for id in all_ids() {
            assert!(seen.insert(id), "duplicate handler id '{id}'");
        }
    }

    /// The synthetic identity must never look like a real filesystem target,
    /// because the whole safety model assumes absolute paths are things on disk.
    #[test]
    fn identity_paths_are_relative_and_namespaced() {
        let path = identity_path("xcode.simulator-devices", "ABC-123");
        assert!(
            path.is_relative(),
            "identity path must be relative: {}",
            path.display()
        );
        assert!(path.starts_with(IDENTITY_PREFIX));
        assert!(path.to_string_lossy().ends_with("ABC-123"));
    }

    #[test]
    fn identity_paths_are_distinct_per_item_and_handler() {
        assert_ne!(identity_path("a", "1"), identity_path("a", "2"));
        assert_ne!(identity_path("a", "1"), identity_path("b", "1"));
    }
}
