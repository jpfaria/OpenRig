//! Issue #780 — THE root cause: a parameter set on the plugin's `IEditController`
//! (exactly what the native editor does) must reach the audio component (DSP).
//!
//! For plugins whose controller is a SEPARATE object from the component (most
//! JUCE plugins, incl. ChowDSP), VST3 requires the host to connect the two via
//! `IConnectionPoint`. The engine load path did not, so editor edits changed the
//! controller but never the DSP — silent knobs. This drives `set_param` (which
//! calls `controller.setParamNormalized`, the editor's path) and asserts the
//! output changes. RED until the load path connects component <-> controller.
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

fn rms(v: &[f32]) -> f32 {
    (v.iter().map(|x| x * x).sum::<f32>() / v.len() as f32).sqrt()
}

fn sine(n: usize, amp: f32) -> Vec<f32> {
    (0..n)
        .map(|i| amp * (2.0 * std::f32::consts::PI * 220.0 * (i as f32) / SR as f32).sin())
        .collect()
}

/// Process 8 blocks of a fresh sine with NO process-data param changes, so the
/// output can only reflect values set on the controller (via `set_param`).
fn run(plugin: &mut vst3_host::Vst3Plugin) -> f32 {
    let n = 512;
    let dry = sine(n, 0.5);
    let mut ol = vec![0.0f32; n];
    let mut or = vec![0.0f32; n];
    for _ in 0..8 {
        let mut il = dry.clone();
        let mut ir = dry.clone();
        plugin.process_audio(&mut il, &mut ir, &mut ol, &mut or, n, &[]);
    }
    rms(&ol)
}

#[test]
fn a_controller_param_change_reaches_the_dsp() {
    let Some(entry) = chow() else { return };
    let uid = vst3_host::resolve_uid_for_model(&entry.model_id).unwrap();

    // Find a param whose controller value audibly changes the output. We set it
    // via the CONTROLLER (set_param → setParamNormalized), the editor's path —
    // NOT via process-data, so it only works if component<->controller are wired.
    let ids: Vec<u32> = {
        let probe =
            vst3_host::Vst3Plugin::load(&entry.info.bundle_path, &uid, SR, 2, 512, &[]).unwrap();
        let v = (0..probe.param_count())
            .filter_map(|i| probe.param_info(i).ok())
            .filter(|pi| !pi.title.to_lowercase().contains("bypass"))
            .map(|pi| pi.id)
            .collect();
        drop(probe);
        v
    };

    let mut best = 0.0f32;
    for id in ids {
        let mut lo =
            vst3_host::Vst3Plugin::load(&entry.info.bundle_path, &uid, SR, 2, 512, &[]).unwrap();
        lo.set_param(id, 0.0).unwrap();
        let a = run(&mut lo);
        drop(lo);

        let mut hi =
            vst3_host::Vst3Plugin::load(&entry.info.bundle_path, &uid, SR, 2, 512, &[]).unwrap();
        hi.set_param(id, 1.0).unwrap();
        let b = run(&mut hi);
        drop(hi);

        best = best.max((a - b).abs());
    }

    assert!(
        best > 1e-4,
        "setting a parameter on the controller did not change the DSP output \
         (max RMS delta {best}) — component<->controller are not connected, so the \
         native editor's edits never reach the audio (#780)"
    );
}
