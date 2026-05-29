//! Issue #542 (reopened) — red-first guard for the OpenRig-side root cause
//! of "CAB/IR estourando o som".
//!
//! The installed app registers plugins with
//! `registry::init_many(&[bundled_root, user_root])` (adapter-gui
//! `desktop_app.rs`). The bundled copy inside `OpenRig.app/Contents/
//! Resources/plugins` ships IR manifests with `output_gain_db: 0.0`
//! (uncalibrated), while the user's `plugins_path` points at a calibrated
//! copy (e.g. `-18.3` for `ir_marshall_4x12_v30/ev_mix_b`). With
//! `output_gain_db: 0.0` the runtime `wrap_with_output_gain_db` is a no-op
//! → the IR convolution runs raw → ~+18 dB hot → the limiter is slammed
//! and the user hears "estourado".
//!
//! The user's plugins_path IS calibrated, yet it still clips — because the
//! bundled (uncalibrated) entry, registered FIRST, shadows it. The fix is
//! a registry-precedence contract: when the same `model_id` exists in more
//! than one root, the LATER root (the user's) must win over the earlier
//! (bundled) one. This test pins that contract.

use std::fs;
use std::path::{Path, PathBuf};

use plugin_loader::manifest::Backend;

fn write_ir_plugin(root: &Path, model_id: &str, output_gain_db: f64) {
    let plugin_dir = root.join("ir").join("test_cab");
    fs::create_dir_all(&plugin_dir).expect("create plugin dir");
    // The capture references a wav; create an (empty) placeholder so disk
    // discovery does not skip the package over a missing file.
    fs::write(plugin_dir.join("imp.wav"), b"").expect("write placeholder wav");
    let manifest = format!(
        "manifest_version: 1\n\
         id: {model_id}\n\
         display_name: Test Cab\n\
         brand: test\n\
         type: cab\n\
         backend: ir\n\
         parameters:\n\
         - name: capture\n  \
           display_name: Capture\n  \
           values:\n  \
           - main\n\
         captures:\n\
         - values:\n    \
             capture: main\n  \
           file: imp.wav\n  \
           output_gain_db: {output_gain_db}\n"
    );
    fs::write(plugin_dir.join("manifest.yaml"), manifest).expect("write manifest");
}

fn capture_output_gain_db(model_id: &str) -> Option<f32> {
    let pkg = plugin_loader::registry::find(model_id)?;
    match &pkg.manifest.backend {
        Backend::Ir { captures, .. } => captures.first().and_then(|c| c.output_gain_db),
        _ => None,
    }
}

#[test]
fn user_plugins_root_overrides_bundled_on_model_id_collision() {
    // Unique temp scratch (no rand available; fixed name, cleaned first).
    let base = std::env::temp_dir().join("openrig_issue542_precedence");
    let _ = fs::remove_dir_all(&base);
    let bundled: PathBuf = base.join("bundled");
    let user: PathBuf = base.join("user");

    // Same model_id in both roots; bundled is UNCALIBRATED (0.0), user is
    // CALIBRATED (-18.3) — exactly the installed-app topology.
    write_ir_plugin(&bundled, "ir_test_cab", 0.0);
    write_ir_plugin(&user, "ir_test_cab", -18.3);

    // Exact order used by the live app: bundled first, user second.
    plugin_loader::registry::init_many(&[bundled, user]);

    let resolved = capture_output_gain_db("ir_test_cab")
        .expect("ir_test_cab must be discoverable from the registry");

    assert!(
        (resolved - (-18.3)).abs() < 1e-3,
        "REGRESSION (#542): user plugins_path is calibrated (-18.3 dB) but the registry \
         resolved output_gain_db = {resolved:.4}. The bundled (uncalibrated, 0.0 dB) copy \
         is shadowing the user's calibrated plugin. With 0.0 dB the IR runs raw → +18 dB → \
         'estourado'. The user's plugins root must take precedence over the bundled one."
    );
}
