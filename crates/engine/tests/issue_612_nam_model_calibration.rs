//! Issue #612 — the NAM model must be driven at its CALIBRATED level,
//! not raw unity.
//!
//! The migration from `neural-amp-modeler-lv2` to the official
//! `NeuralAmpModelerCore` dropped the per-model gain staging: the old
//! engine drove each model at the level it was trained at (via the
//! engine's recommended dB adjustments), while the new wrapper applied
//! only the user knobs (default 0 dB = unity). A NAM is nonlinear, so
//! under-driving / not normalizing it sounds clean, dull and quiet —
//! "abafado / sem vida".
//!
//! The official core exposes the model's own loudness (`GetLoudness`,
//! `HasLoudness`). The wrapper now folds `target - GetLoudness()` into
//! the output gain so a model baked quiet (the fixture sits at
//! `loudness = -23.98 dB`) is normalized up toward the reference
//! instead of staying at unity. This calibration is suppressed when the
//! catalog audit already owns the output level
//! (`audit_overrides_baked_output == true`) so the two never
//! double-count.
//!
//! Contract proven here: for the SAME model + SAME DI, the calibrated
//! build (non-audited) is meaningfully LOUDER than the unity build
//! (audited / calibration suppressed), and the delta matches the
//! model's own loudness deficit. That is the objective, by-the-numbers
//! proof that the model is now driven hotter / correctly vs unity.

use std::path::PathBuf;

use block_core::MonoProcessor;
use nam::processor::{NamPluginParams, NamProcessor, DEFAULT_PLUGIN_PARAMS};

const SR: f32 = 48_000.0;

/// The bundled fixture model carries `loudness = -23.98 dB` in its NAM
/// metadata. Normalizing toward the -18 dB reference is a +5.98 dB boost
/// vs unity. We assert the calibrated build lands within a band around
/// that, not an exact value (the model is nonlinear, so the boost is not
/// a perfectly linear scaling of the steady-state RMS).
const FIXTURE_MODEL: &str =
    "tests/fixtures/plugins/nam/marshall_plexi/captures/angus_nano.nam";

fn model_path() -> String {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join(FIXTURE_MODEL)
        .to_str()
        .expect("utf8 path")
        .to_string()
}

/// Realistic guitar DI: peak ≈ 0.3 (≈ -10 dBFS), normal playing level.
fn di_sine(frames: usize) -> Vec<f32> {
    (0..frames)
        .map(|n| 0.3 * (2.0 * std::f32::consts::PI * 110.0 * n as f32 / SR).sin())
        .collect()
}

fn db(x: f32) -> f32 {
    20.0 * x.max(1e-9).log10()
}

fn tail_rms_db(buf: &[f32]) -> f32 {
    let tail = &buf[buf.len() / 2..];
    db((tail.iter().map(|v| v * v).sum::<f32>() / tail.len() as f32).sqrt())
}

/// Build a NAM processor over the fixture, run a DI, return tail RMS dBFS.
/// `audited` controls `audit_overrides_baked_output`: when true the model
/// loudness calibration is suppressed (the catalog audit owns the level),
/// when false the wrapper normalizes the model toward the reference.
fn run(audited: bool) -> f32 {
    let params = NamPluginParams {
        // Both knobs at unity so the ONLY difference between the two
        // builds is the model-loudness calibration.
        input_level_db: 0.0,
        output_level_db: 0.0,
        noise_gate_enabled: false,
        audit_overrides_baked_output: audited,
        ..DEFAULT_PLUGIN_PARAMS
    };
    let mut proc = NamProcessor::new(&model_path(), None, params, SR).expect("build NAM");
    let mut buf = di_sine(8_192);
    proc.process_block(&mut buf);
    tail_rms_db(&buf)
}

#[test]
fn model_is_driven_hotter_than_unity_via_loudness_calibration() {
    nam::register_builder();

    let unity_db = run(true); // calibration suppressed = the dull baseline
    let calibrated_db = run(false); // model normalized toward reference

    let delta = calibrated_db - unity_db;
    eprintln!(
        "issue#612 calibration: unity_rms={unity_db:.2} dBFS  \
         calibrated_rms={calibrated_db:.2} dBFS  delta={delta:+.2} dB"
    );

    // The model sits at loudness -23.98 dB; normalizing toward -18 dB is
    // ~+5.98 dB. The calibrated build must be clearly hotter than unity,
    // in the right magnitude band (not a tiny tweak, not a runaway boost).
    assert!(
        delta >= 3.0,
        "model is NOT driven hotter than unity: delta {delta:+.2} dB \
         (calibration lost ⇒ \"abafado\", issue #612)"
    );
    assert!(
        delta <= 9.0,
        "calibration overshoots: delta {delta:+.2} dB — too hot vs the \
         model's own loudness deficit (issue #612)"
    );

    // And it must land in a sane loud range, not a quiet fix.
    assert!(
        calibrated_db >= -18.0,
        "calibrated output too quiet: {calibrated_db:.2} dBFS (issue #612)"
    );
    assert!(buf_finite(), "produced NaN/Inf");
}

fn buf_finite() -> bool {
    let params = NamPluginParams {
        noise_gate_enabled: false,
        audit_overrides_baked_output: false,
        ..DEFAULT_PLUGIN_PARAMS
    };
    let mut proc = NamProcessor::new(&model_path(), None, params, SR).expect("build NAM");
    let mut buf = di_sine(2_048);
    proc.process_block(&mut buf);
    buf.iter().all(|s| s.is_finite())
}
