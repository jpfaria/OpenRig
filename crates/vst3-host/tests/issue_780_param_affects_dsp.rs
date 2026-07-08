//! Issue #780 follow-up — reproduce "editing a param has no audible effect".
//! Layered, headless, env-gated on OPENRIG_TEST_VST3_DIR (--test-threads=1).
//!
//! Layer 1 (this file): does driving a plugin parameter to two extremes change
//! the DSP output at all? If NOT, the break is in the plugin-level parameter
//! application (deeper than the editor→channel wiring).

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

/// Process 8 blocks of a fresh sine with `pending` on block 0; return last out-L RMS.
fn run(plugin: &mut vst3_host::Vst3Plugin, pending: &[(u32, f64)]) -> f32 {
    let n = 512;
    let dry = sine(n, 0.5);
    let mut ol = vec![0.0f32; n];
    let mut or = vec![0.0f32; n];
    for b in 0..8 {
        let mut il = dry.clone();
        let mut ir = dry.clone();
        let p: &[(u32, f64)] = if b == 0 { pending } else { &[] };
        plugin.process_audio(&mut il, &mut ir, &mut ol, &mut or, n, p);
    }
    rms(&ol)
}

#[test]
fn some_parameter_audibly_changes_the_output() {
    let Some(entry) = chow() else { return };
    let uid = vst3_host::resolve_uid_for_model(&entry.model_id).unwrap();

    // For each parameter, drive a fresh instance to min then max and compare the
    // output RMS. At least one non-bypass param MUST change the sound — if none
    // does, plugin-level param application is broken (the user's symptom).
    let probe = vst3_host::Vst3Plugin::load(&entry.info.bundle_path, &uid, SR, 2, 512, &[]).unwrap();
    let ids: Vec<u32> = (0..probe.param_count())
        .filter_map(|i| probe.param_info(i).ok())
        .filter(|pi| !pi.title.to_lowercase().contains("bypass"))
        .map(|pi| pi.id)
        .collect();
    drop(probe);

    let mut best_delta = 0.0f32;
    let mut best_id = 0u32;
    for id in ids {
        let mut lo =
            vst3_host::Vst3Plugin::load(&entry.info.bundle_path, &uid, SR, 2, 512, &[]).unwrap();
        let a = run(&mut lo, &[(id, 0.0)]);
        drop(lo);
        let mut hi =
            vst3_host::Vst3Plugin::load(&entry.info.bundle_path, &uid, SR, 2, 512, &[]).unwrap();
        let b = run(&mut hi, &[(id, 1.0)]);
        drop(hi);
        let delta = (a - b).abs();
        if delta > best_delta {
            best_delta = delta;
            best_id = id;
        }
    }

    assert!(
        best_delta > 1e-4,
        "no parameter changed the DSP output (max RMS delta {best_delta} at id {best_id}) — \
         plugin-level parameter application is broken"
    );
}
