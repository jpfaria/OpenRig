//! EQ signal-integrity tests (issue #792 split from
//! audio_signal_integrity_tests.rs). Shares the runtime/scan fixtures with
//! the base suite via super::audio_signal_integrity.

use domain::io_binding::ChannelMode;
use domain::value_objects::ParameterValue;
use project::param::ParameterSet;

use super::audio_signal_integrity::{
    build_runtime, chain_with_blocks, core_block, drive_capture_steady, input_mono,
    neutral_params, output, scan_finite, scan_for_click, scan_within_magnitude, BUFFER_FRAMES, SR,
};
use super::audio_signal_integrity::SineGen;


/// Helper: build EQ params with one band boosted by `gain_db` at `freq_hz`,
/// rest at defaults.
fn eq_with_one_band_boosted(band_index: usize, freq_hz: f32, gain_db: f32) -> ParameterSet {
    let mut params = neutral_params("filter", "eq_eight_band_parametric");
    let n = band_index + 1;
    params.insert(format!("band{n}_freq"), ParameterValue::Float(freq_hz));
    params.insert(format!("band{n}_gain"), ParameterValue::Float(gain_db));
    params
}

#[test]
fn eq_eight_band_one_band_max_boost_does_not_overshoot_input() {
    // Boost band 5 (1 kHz) by +24 dB (max in schema). Feed a 1 kHz sine
    // at 0.05 amplitude (well below clipping). The boosted band should
    // amplify by ~16x → output peak ≈ 0.8. No clipping expected.
    //
    // If the EQ's filter ringing or numerical error causes overshoot
    // above 1.0 (= the chain limiter engages), we capture it.
    let params = eq_with_one_band_boosted(4, 1_000.0, 24.0);
    let (chain, registry) = chain_with_blocks(
        "eq8-1k-+24",
        input_mono(vec![0]),
        vec![core_block(
            "eq8",
            "filter",
            "eq_eight_band_parametric",
            params,
        )],
        output(ChannelMode::Mono, vec![0]),
    );
    let runtime = build_runtime(&chain, &registry);
    // Input at 0.05 → expected boosted peak around 0.8. Run long enough
    // for filter to settle.
    let mut gen = SineGen::new(1_000.0, SR, 0.05);
    let captured = drive_capture_steady(&runtime, &mut gen, 1, 1, 64, 16);

    scan_finite("eq8_1k_+24_finite", &captured).expect("EQ +24dB at 1k must produce finite output");
    let peak = captured.iter().fold(0.0_f32, |a, &b| a.max(b.abs()));
    eprintln!("[eq8 1k +24dB] input peak 0.05 → output peak {peak:.4}");
    assert!(
        peak <= 1.0,
        "EQ +24dB band overshoot above ±1.0 at safe input level: peak {peak}"
    );
}

#[test]
fn eq_eight_band_output_trim_attenuates_uniformly() {
    // The fix for the user's reported clipping: output_db parameter
    // applies a pre-computed linear gain at the end of process_sample.
    // -6 dB ≈ 0.5012 linear → output peak ≈ 0.5 of unity-EQ output.
    let mut params = neutral_params("filter", "eq_eight_band_parametric");
    params.insert("output_db", ParameterValue::Float(-6.0));

    let (chain, registry) = chain_with_blocks(
        "eq8-out-trim",
        input_mono(vec![0]),
        vec![core_block(
            "eq8",
            "filter",
            "eq_eight_band_parametric",
            params,
        )],
        output(ChannelMode::Mono, vec![0]),
    );
    let runtime = build_runtime(&chain, &registry);
    let mut gen = SineGen::new(220.0, SR, 0.5);
    let captured = drive_capture_steady(&runtime, &mut gen, 1, 1, 32, 8);
    scan_finite("eq8_out_trim_-6dB", &captured).expect("output trim must not produce NaN");
    let peak = captured.iter().fold(0.0_f32, |a, &b| a.max(b.abs()));
    let expected = 0.5_f32 * 10.0_f32.powf(-6.0 / 20.0); // ≈ 0.2506
    assert!(
        (peak - expected).abs() < 0.05,
        "output_db -6 dB should attenuate 0.5 input → ≈{expected}, got {peak}"
    );
}

#[test]
fn eq_eight_band_default_output_db_is_unity() {
    // Pin the default: output_db defaults to 0 dB → unity gain, same
    // behaviour as before the parameter existed. Existing project YAMLs
    // that don't carry output_db are normalized with 0.0 and run
    // identically.
    let params = neutral_params("filter", "eq_eight_band_parametric");

    let (chain, registry) = chain_with_blocks(
        "eq8-default-trim",
        input_mono(vec![0]),
        vec![core_block(
            "eq8",
            "filter",
            "eq_eight_band_parametric",
            params,
        )],
        output(ChannelMode::Mono, vec![0]),
    );
    let runtime = build_runtime(&chain, &registry);
    let mut gen = SineGen::new(220.0, SR, 0.5);
    let captured = drive_capture_steady(&runtime, &mut gen, 1, 1, 32, 8);
    let peak = captured.iter().fold(0.0_f32, |a, &b| a.max(b.abs()));
    assert!(
        (peak - 0.5).abs() < 0.05,
        "EQ default output_db should be unity, got peak {peak} for 0.5 input"
    );
}

#[test]
fn eq_eight_band_smile_curve_typical_user_config() {
    // Smile/V curve: bass + treble boosted, mids cut (or flat). Very
    // common preset for guitar tone. Tests realistic worst-case.
    //  band1 (62 Hz)  +9 dB   bass shelf-ish
    //  band2 (125)    +6 dB
    //  band3 (250)    +3 dB
    //  band4 (500)    -3 dB
    //  band5 (1k)     -3 dB   mid scoop
    //  band6 (2k)     +3 dB
    //  band7 (4k)     +6 dB
    //  band8 (8k)     +9 dB   treble shelf-ish
    let mut params = neutral_params("filter", "eq_eight_band_parametric");
    params.insert("band1_gain", ParameterValue::Float(9.0));
    params.insert("band2_gain", ParameterValue::Float(6.0));
    params.insert("band3_gain", ParameterValue::Float(3.0));
    params.insert("band4_gain", ParameterValue::Float(-3.0));
    params.insert("band5_gain", ParameterValue::Float(-3.0));
    params.insert("band6_gain", ParameterValue::Float(3.0));
    params.insert("band7_gain", ParameterValue::Float(6.0));
    params.insert("band8_gain", ParameterValue::Float(9.0));

    let (chain, registry) = chain_with_blocks(
        "eq8-smile",
        input_mono(vec![0]),
        vec![core_block(
            "eq8",
            "filter",
            "eq_eight_band_parametric",
            params,
        )],
        output(ChannelMode::Mono, vec![0]),
    );
    // Input at 0.5 (already a hot guitar level). Sweep across the
    // boosted band centers and find the worst-case overshoot.
    let mut max_overshoot = 0.0_f32;
    let mut worst_freq = 0.0_f32;
    for &freq in &[
        62.0_f32, 125.0, 250.0, 500.0, 1000.0, 2000.0, 4000.0, 8000.0,
    ] {
        let runtime = build_runtime(&chain, &registry); // fresh runtime per freq, no leftover state
        let mut gen = SineGen::new(freq, SR, 0.5);
        let captured = drive_capture_steady(&runtime, &mut gen, 1, 1, 64, 16);
        scan_finite(&format!("smile_{freq}Hz_finite"), &captured)
            .expect("smile EQ must not produce NaN");
        let peak = captured.iter().fold(0.0_f32, |a, &b| a.max(b.abs()));
        if peak > max_overshoot {
            max_overshoot = peak;
            worst_freq = freq;
        }
        eprintln!("[eq8 smile] {freq:>5}Hz @ 0.5 → peak {peak:.4}");
    }
    eprintln!("[eq8 smile] worst case: {worst_freq}Hz, peak {max_overshoot:.4}");

    // The smile curve at +9 dB extremes amplifies by ~2.8x linear at the
    // boosted band centers. Input 0.5 → expected peak ~1.4 → limiter
    // engages → output peak <= 1.0. Document the failure mode: limiter
    // is doing its job, but the user hears tanh saturation as "clipping".
    assert!(
        max_overshoot <= 1.0,
        "Output limiter failed: peak {max_overshoot} at {worst_freq}Hz exceeds ±1.0"
    );
    let _ = max_overshoot;
}

#[test]
fn eq_eight_band_full_scale_with_band_boost_clips_through_limiter() {
    // The user's reported case: input is already hot (e.g. amp output)
    // and EQ has a boosted band. The cumulative output exceeds 1.0,
    // engages the chain limiter, and the user hears tanh saturation —
    // that's the audible "clip".
    //
    // This test does NOT assert the output stays under 1.0 — it can't
    // (the user explicitly asked for +24 dB on a full-scale signal).
    // The test asserts the FAILURE MODE: the output limiter is what
    // catches the overshoot, NOT NaN, NOT a click. tanh-saturated
    // signal is musical distortion; raw clipping is a buzz.
    //
    // The fix for the user's complaint isn't the limiter (working as
    // designed) — it's giving the EQ an output trim parameter so they
    // can compensate for boost without re-tuning every band.
    let params = eq_with_one_band_boosted(4, 1_000.0, 24.0);
    let (chain, registry) = chain_with_blocks(
        "eq8-1k-+24-fullscale",
        input_mono(vec![0]),
        vec![core_block(
            "eq8",
            "filter",
            "eq_eight_band_parametric",
            params,
        )],
        output(ChannelMode::Mono, vec![0]),
    );
    let runtime = build_runtime(&chain, &registry);
    let mut gen = SineGen::new(1_000.0, SR, 1.0);
    let captured = drive_capture_steady(&runtime, &mut gen, 1, 1, 64, 16);

    scan_finite("eq8_1k_+24_fullscale_finite", &captured)
        .expect("EQ must not produce NaN even when limiter saturates");

    // Limiter (tanh above 0.95) keeps every sample within ±tanh(∞) = ±1.0.
    // No sample should ever exceed 1.0.
    let peak = captured.iter().fold(0.0_f32, |a, &b| a.max(b.abs()));
    assert!(
        peak <= 1.0,
        "Limiter failed to bound output ≤ ±1.0: peak = {peak}"
    );
}

#[test]
fn eq_eight_band_at_defaults_full_scale_no_overshoot() {
    // Full-scale ±1.0 sine through EQ. At defaults the EQ is unity, so
    // output peak should be ≤ 1.0 (with tiny filter-init transient).
    // If the cascade introduces ringing that pushes any sample above
    // 1.0, the limiter (tanh > 0.95) engages and the user hears
    // saturation — that's the reported audible "clip".
    let eq_params = neutral_params("filter", "eq_eight_band_parametric");
    let (chain, registry) = chain_with_blocks(
        "eq8-fullscale",
        input_mono(vec![0]),
        vec![core_block(
            "eq8",
            "filter",
            "eq_eight_band_parametric",
            eq_params,
        )],
        output(ChannelMode::Mono, vec![0]),
    );
    let runtime = build_runtime(&chain, &registry);
    let mut gen = SineGen::new(220.0, SR, 1.0);
    let captured = drive_capture_steady(&runtime, &mut gen, 1, 1, 64, 16);

    scan_finite("eq8_fullscale_finite", &captured).expect("EQ must not produce NaN at full scale");
    let max_abs = captured.iter().fold(0.0_f32, |a, &b| a.max(b.abs()));
    // Full-scale sine through unity EQ → output peak ≤ 1.0 (limiter
    // soft-saturates at 0.95, so peak <= ~0.95 + tanh tail).
    // If the output peak is much above input peak, the EQ is amplifying
    // by accident.
    assert!(
        max_abs <= 1.0,
        "EQ at defaults clipped on full-scale input: peak = {max_abs}"
    );
}

#[test]
fn long_run_steady_state_no_clicks_8000_samples() {
    // Soak: 125 callbacks × 64 frames = 8000 samples ≈ 167 ms of audio.
    // Catches periodic glitches that only appear after warmup or
    // ring-buffer wrap-around.
    let (chain, registry) = chain_with_blocks(
        "pipe-soak",
        input_mono(vec![0]),
        vec![],
        output(ChannelMode::Mono, vec![0]),
    );
    let runtime = build_runtime(&chain, &registry);
    let mut gen = SineGen::new(220.0, SR, 0.5);
    let captured = drive_capture_steady(&runtime, &mut gen, 1, 1, 125, 4);
    assert_eq!(captured.len(), 121 * BUFFER_FRAMES);

    scan_for_click("soak", &captured, 1, 0.1).expect("audio integrity violated during soak");
    scan_within_magnitude("soak_magnitude", &captured, 1.0).expect("output exceeded ±1.0");
}
