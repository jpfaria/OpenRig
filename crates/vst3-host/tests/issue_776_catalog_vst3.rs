//! Issue #776 — validation battery for a *catalog* VST3 (ChowCentaur, shipped
//! in the OpenRig plugins folder). Proves discovery + load work AND pins the
//! processing bugs the user hit ("knob does nothing / enable-disable does
//! nothing"): several of these are RED until the passthrough is fixed.
//!
//! Real-plugin tests: env-gated on `OPENRIG_TEST_VST3_DIR` (the plugins `vst3/`
//! dir, e.g. `<OpenRig-plugins>/plugins/source/vst3`). They skip cleanly when it
//! is unset so CI stays green. Run locally with:
//!   OPENRIG_TEST_VST3_DIR=<.../vst3> cargo test -p vst3-host \
//!     --test issue_776_catalog_vst3 -- --test-threads=1
//! `--test-threads=1` is required: ChowCentaur (like most JUCE plugins) refuses
//! *concurrent* instantiation, so parallel loads fault. This is also the root of
//! the reorder crash (#778) — a rebuild loads a 2nd instance while the 1st lives.
//!
//! NOTE: an earlier "passthrough" diagnosis was a test bug — ChowCentaur exposes
//! TWO `Bypass` params; driving *all* params high turned bypass ON. With bypass
//! left at its default the plugin processes correctly (t10/t11).

use std::path::PathBuf;

const SR: f64 = 48_000.0;

fn plugins_vst3_dir() -> Option<PathBuf> {
    std::env::var_os("OPENRIG_TEST_VST3_DIR").map(PathBuf::from)
}

/// Init the catalog against the plugins dir and return the ChowCentaur entry, or
/// `None` to skip (env unset / plugin absent).
fn chow() -> Option<&'static vst3_host::Vst3CatalogEntry> {
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

fn rms(v: &[f32]) -> f32 {
    (v.iter().map(|x| x * x).sum::<f32>() / v.len() as f32).sqrt()
}

fn sine(n: usize, amp: f32) -> Vec<f32> {
    (0..n)
        .map(|i| amp * (2.0 * std::f32::consts::PI * 220.0 * (i as f32) / SR as f32).sin())
        .collect()
}

/// Process `blocks` blocks of a fresh sine each and return the last output-left.
/// `pending` is delivered on the first block only (the audio-thread param path).
fn process_last(
    plugin: &mut vst3_host::Vst3Plugin,
    pending: &[(u32, f64)],
    blocks: usize,
) -> Vec<f32> {
    let n = 512;
    let dry = sine(n, 0.5);
    let mut ol = vec![0.0f32; n];
    let mut or = vec![0.0f32; n];
    for b in 0..blocks {
        let mut il = dry.clone();
        let mut ir = dry.clone();
        let p: &[(u32, f64)] = if b == 0 { pending } else { &[] };
        plugin.process_audio(&mut il, &mut ir, &mut ol, &mut or, n, p);
    }
    ol
}

// ── Discovery ───────────────────────────────────────────────────────────────

#[test]
fn t01_catalog_vst3_is_discovered_from_the_plugins_folder() {
    let Some(entry) = chow() else { return };
    assert!(entry.info.bundle_path.exists(), "bundle path should exist");
}

#[test]
fn t02_discovered_model_id_uses_the_vst3_scheme() {
    let Some(entry) = chow() else { return };
    assert!(
        entry.model_id.starts_with("vst3:") && entry.model_id.contains("ChowCentaur"),
        "model_id should be vst3:<stem>:<class>, got {}",
        entry.model_id
    );
}

#[test]
fn t03_catalog_vst3_appears_in_vst3_catalog() {
    if chow().is_none() {
        return;
    }
    let count = vst3_host::vst3_catalog()
        .iter()
        .filter(|e| e.model_id.contains("ChowCentaur"))
        .count();
    assert_eq!(count, 1, "exactly one ChowCentaur entry expected");
}

#[test]
fn t04_find_vst3_plugin_resolves_the_catalog_model_id() {
    let Some(entry) = chow() else { return };
    assert!(
        vst3_host::find_vst3_plugin(entry.model_id).is_some(),
        "find_vst3_plugin must resolve the discovered catalog id"
    );
}

#[test]
fn t05_resolve_uid_returns_a_real_uid() {
    let Some(entry) = chow() else { return };
    let uid = vst3_host::resolve_uid_for_model(entry.model_id).expect("uid resolves");
    assert_ne!(uid, [0u8; 16], "uid must be resolved (lazy from the bundle)");
}

// ── Load ─────────────────────────────────────────────────────────────────────

#[test]
fn t06_loads_successfully() {
    let Some(entry) = chow() else { return };
    let uid = vst3_host::resolve_uid_for_model(entry.model_id).expect("uid");
    assert!(
        vst3_host::Vst3Plugin::load(&entry.info.bundle_path, &uid, SR, 2, 512, &[]).is_ok(),
        "ChowCentaur must load"
    );
}

#[test]
fn t07_exposes_parameters() {
    let Some(entry) = chow() else { return };
    let uid = vst3_host::resolve_uid_for_model(entry.model_id).expect("uid");
    let plugin = vst3_host::Vst3Plugin::load(&entry.info.bundle_path, &uid, SR, 2, 512, &[]).unwrap();
    assert!(plugin.param_count() > 0, "plugin should expose parameters");
}

#[test]
fn t08_reports_valid_channel_counts() {
    let Some(entry) = chow() else { return };
    let uid = vst3_host::resolve_uid_for_model(entry.model_id).expect("uid");
    let plugin = vst3_host::Vst3Plugin::load(&entry.info.bundle_path, &uid, SR, 2, 512, &[]).unwrap();
    assert!(
        plugin.num_input_channels > 0 && plugin.num_output_channels > 0,
        "channel counts must be positive: in={} out={}",
        plugin.num_input_channels,
        plugin.num_output_channels
    );
}

// ── Processing (the user-reported bug: "does nothing to the sound") ───────────

#[test]
fn t09_process_audio_is_not_silent() {
    let Some(entry) = chow() else { return };
    let uid = vst3_host::resolve_uid_for_model(entry.model_id).expect("uid");
    let mut plugin =
        vst3_host::Vst3Plugin::load(&entry.info.bundle_path, &uid, SR, 2, 512, &[]).unwrap();
    let out = process_last(&mut plugin, &[], 8);
    assert!(rms(&out) > 1e-4, "output must not be silent");
}

/// Discovered param id for a given title (e.g. "Gain"). ChowCentaur also exposes
/// TWO "Bypass" params — a test must never drive those, or it silences the DSP.
fn param_id(plugin: &vst3_host::Vst3Plugin, title: &str) -> Option<u32> {
    (0..plugin.param_count())
        .filter_map(|i| plugin.param_info(i).ok())
        .find(|p| p.title == title)
        .map(|p| p.id)
}

#[test]
fn t10_process_audio_alters_the_signal_at_defaults() {
    let Some(entry) = chow() else { return };
    let uid = vst3_host::resolve_uid_for_model(entry.model_id).expect("uid");
    // Defaults = what the app block uses (no params seeded). Bypass defaults OFF,
    // so the overdrive must colour the signal.
    let mut plugin =
        vst3_host::Vst3Plugin::load(&entry.info.bundle_path, &uid, SR, 2, 512, &[]).unwrap();
    let out = process_last(&mut plugin, &[], 8);
    let dry = sine(512, 0.5);
    let max_diff = out
        .iter()
        .zip(dry.iter())
        .map(|(o, i)| (o - i).abs())
        .fold(0.0f32, f32::max);
    assert!(
        max_diff > 1e-3,
        "output == input — the plugin did not process at defaults (max|out-in|={max_diff})"
    );
}

#[test]
fn t11_gain_param_changes_the_output() {
    let Some(entry) = chow() else { return };
    let uid = vst3_host::resolve_uid_for_model(entry.model_id).expect("uid");
    let load = || vst3_host::Vst3Plugin::load(&entry.info.bundle_path, &uid, SR, 2, 512, &[]).unwrap();
    let mut lo = load();
    let mut hi = load();
    let gain = param_id(&lo, "Gain").expect("ChowCentaur exposes a Gain param");
    let out_lo = process_last(&mut lo, &[(gain, 0.05)], 8);
    let out_hi = process_last(&mut hi, &[(gain, 0.95)], 8);
    let diff = out_lo
        .iter()
        .zip(out_hi.iter())
        .map(|(a, b)| (a - b).abs())
        .fold(0.0f32, f32::max);
    assert!(
        diff > 1e-3,
        "the Gain param has no audible effect (max diff low/high = {diff})"
    );
}

#[test]
fn t12_pending_param_change_does_not_crash() {
    // The native-editor edit path (performEdit -> channel -> process(pending)).
    let Some(entry) = chow() else { return };
    let uid = vst3_host::resolve_uid_for_model(entry.model_id).expect("uid");
    let mut plugin =
        vst3_host::Vst3Plugin::load(&entry.info.bundle_path, &uid, SR, 2, 512, &[]).unwrap();
    let id = plugin.param_info(0).expect("has a param").id;
    let _ = process_last(&mut plugin, &[(id, 0.7)], 4);
}

// ── Teardown marshaling (issue #778) ──────────────────────────────────────────

#[test]
fn t13_dropping_a_loaded_plugin_off_main_does_not_run_teardown_inline() {
    let Some(entry) = chow() else { return };
    let uid = vst3_host::resolve_uid_for_model(entry.model_id).expect("uid");
    vst3_host::mark_main_thread(); // this test thread is "main"
    let bundle = entry.info.bundle_path.clone();
    // Load + drop on a background thread: the deferral must move teardown to the
    // main thread's drain (this must not crash on any thread).
    std::thread::spawn(move || {
        let plugin = vst3_host::Vst3Plugin::load(&bundle, &uid, SR, 2, 512, &[]).unwrap();
        drop(plugin);
    })
    .join()
    .unwrap();
    // Draining on the main thread runs whatever was deferred.
    vst3_host::drain_main_thread_deferred();
}
