//! Responsibility: fills the model catalogs a render needs before a chain loads.

use std::path::PathBuf;

/// Register the native models, discover the disk packages under
/// `plugin_roots` and scan their VST3 bundles. Mirrors the GUI bootstrap in
/// adapter-gui — without it, disk-package models (NAM captures, IR cabs, LV2
/// plugins) aren't visible to the schema lookup and the preset loader drops
/// every block that references one (#552), and every VST3 block fails with
/// "not found in catalog" (#938).
pub fn init_plugin_catalogs(plugin_roots: &[PathBuf], sample_rate_hz: u32) {
    engine::native_registry::register_all_natives();
    plugin_loader::registry::init_many(plugin_roots);
    project::vst3_editor::init_vst3_catalog(sample_rate_hz as f64, plugin_roots);
}
