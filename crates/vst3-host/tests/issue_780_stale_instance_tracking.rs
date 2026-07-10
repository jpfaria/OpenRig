//! Issue #780 — when the engine rebuilds a block's VST3 instance it re-registers
//! a context under the SAME block key. The GUI must then close that block's open
//! editor window (it is bound to the now-dead old instance) before the old
//! instance is torn down. The registry tracks which block keys had their context
//! REPLACED so the GUI tick can act on them.
//!
//! Env-gated on OPENRIG_TEST_VST3_DIR; run with --test-threads=1.

use std::path::PathBuf;

const SR: f64 = 48_000.0;

fn chow() -> Option<&'static vst3_host::Vst3CatalogEntry> {
    let dir = std::env::var_os("OPENRIG_TEST_VST3_DIR").map(PathBuf::from)?;
    vst3_host::init_vst3_catalog(SR, &[dir]);
    vst3_host::vst3_catalog().iter().find(|e| {
        e.info
            .bundle_path
            .file_name()
            .and_then(|s| s.to_str())
            .map(|s| s.eq_ignore_ascii_case("ChowCentaur.vst3"))
            .unwrap_or(false)
    })
}

fn register(entry: &vst3_host::Vst3CatalogEntry, key: &str) {
    let uid = vst3_host::resolve_uid_for_model(&entry.model_id).unwrap();
    let plugin =
        vst3_host::Vst3Plugin::load(&entry.info.bundle_path, &uid, SR, 2, 512, &[]).unwrap();
    let _ = vst3_host::register_vst3_gui_context(
        key,
        &entry.model_id,
        plugin.controller_clone(),
        plugin.library_arc(),
    );
    // keep the plugin alive for the duration of registration only
    drop(plugin);
}

#[test]
fn re_registering_a_key_marks_it_replaced() {
    let Some(entry) = chow() else { return };
    // drain any leftovers from other tests sharing the process registry
    let _ = vst3_host::take_replaced_instances();

    register(entry, "stale-A"); // first build — NOT a replacement
    assert!(
        !vst3_host::take_replaced_instances().contains(&"stale-A".to_string()),
        "the first registration of a key is not a replacement"
    );

    register(entry, "stale-A"); // rebuild — REPLACES the existing context
    let replaced = vst3_host::take_replaced_instances();
    assert!(
        replaced.contains(&"stale-A".to_string()),
        "re-registering an existing key must mark it replaced so the GUI closes its stale editor"
    );

    // take_* drains — a second call is empty until another replacement happens.
    assert!(
        vst3_host::take_replaced_instances().is_empty(),
        "take_replaced_instances drains the set"
    );
}
