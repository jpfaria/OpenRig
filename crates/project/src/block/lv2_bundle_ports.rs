//! Responsibility: scans the TTL ports of an LV2 disk package.

use std::collections::BTreeMap;
use std::path::PathBuf;

use plugin_loader::dispatch::Lv2Port;
use plugin_loader::manifest::Lv2Slot;

/// Ports of `plugin_uri` inside `package`. Prefers the deduplicated
/// `<package>/data/` TTL bundle and falls back to the legacy layout where
/// the TTLs sat next to the binary. `None` when no bundle can be read.
pub(crate) fn lv2_bundle_ports(
    package: &plugin_loader::LoadedPackage,
    plugin_uri: &str,
    binaries: &BTreeMap<Lv2Slot, PathBuf>,
) -> Option<Vec<Lv2Port>> {
    let data_dir = package.root.join("data");
    let bundle_dir = if data_dir.is_dir() {
        data_dir
    } else {
        let (_, rel_binary) = binaries.iter().next()?;
        package.root.join(rel_binary).parent()?.to_path_buf()
    };
    plugin_loader::dispatch::scan_lv2_ports(&bundle_dir, plugin_uri).ok()
}
