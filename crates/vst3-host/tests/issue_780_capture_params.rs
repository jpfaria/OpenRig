//! Issue #780 — capture live VST3 controller values for persistence.
//! Env-gated on OPENRIG_TEST_VST3_DIR (skips when unset). Run with
//! --test-threads=1 (JUCE plugins refuse concurrent instantiation).
use std::path::PathBuf;

const SR: f64 = 48_000.0;

fn plugins_vst3_dir() -> Option<PathBuf> {
    std::env::var_os("OPENRIG_TEST_VST3_DIR").map(PathBuf::from)
}

fn chow_entry() -> Option<&'static vst3_host::Vst3CatalogEntry> {
    let dir = plugins_vst3_dir()?;
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

fn load_and_register(entry: &vst3_host::Vst3CatalogEntry, key: &str) -> vst3_host::Vst3Plugin {
    let uid = vst3_host::resolve_uid_for_model(&entry.model_id).unwrap();
    let plugin =
        vst3_host::Vst3Plugin::load(&entry.info.bundle_path, &uid, SR, 2, 512, &[]).unwrap();
    let _channel = vst3_host::register_vst3_gui_context(
        key,
        &entry.model_id,
        plugin.controller_clone(),
        plugin.library_arc(),
    );
    plugin
}

#[test]
fn capture_reads_a_native_editor_edit_and_omits_defaults() {
    let Some(entry) = chow_entry() else { return };
    let plugin = load_and_register(entry, "blk-A");

    // Pick the first param and move it away from its default. The native editor
    // drives the controller with exactly this call.
    let info = plugin.param_info(0).expect("has a param");
    let default = info.default_normalized;
    let target = if default < 0.5 { 0.9 } else { 0.1 };
    plugin.set_param(info.id, target).unwrap();

    let captured = vst3_host::capture_vst3_params("blk-A").expect("context registered");
    let got = captured
        .iter()
        .find(|(id, _)| *id == info.id)
        .expect("edited param must be captured");
    assert!(
        (got.1 - target).abs() < 1e-3,
        "captured {} want {}",
        got.1,
        target
    );
    // No captured value should equal its default (defaults are omitted).
    // We at least prove the edited one is non-default.
    assert!((got.1 - default).abs() > 1e-6, "edited value must be non-default");
    drop(plugin);
}

#[test]
fn two_same_model_instances_do_not_collide() {
    let Some(entry) = chow_entry() else { return };
    let a = load_and_register(entry, "blk-A");
    let b = load_and_register(entry, "blk-B");
    let id = a.param_info(0).unwrap().id;
    a.set_param(id, 0.2).unwrap();
    b.set_param(id, 0.8).unwrap();

    let ca = vst3_host::capture_vst3_params("blk-A").unwrap();
    let cb = vst3_host::capture_vst3_params("blk-B").unwrap();
    let va = ca
        .iter()
        .find(|(i, _)| *i == id)
        .map(|(_, v)| *v)
        .unwrap_or(0.0);
    let vb = cb
        .iter()
        .find(|(i, _)| *i == id)
        .map(|(_, v)| *v)
        .unwrap_or(0.0);
    assert!((va - 0.2).abs() < 1e-3, "blk-A should keep 0.2, got {va}");
    assert!((vb - 0.8).abs() < 1e-3, "blk-B should keep 0.8, got {vb}");
    drop(a);
    drop(b);
}
