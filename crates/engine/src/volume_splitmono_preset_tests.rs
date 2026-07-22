//! Volume/audio invariants — PINNED (issue #792 split from volume_invariants_tests.rs).
//! Section moved verbatim; shared fixtures live in `volume_invariants_tests.rs`.
#![allow(unused_imports)]
use super::*;
use super::volume_invariants::*;

// ─────────────────────────────────────────────────────────────────────────
// G. Split-mono (#350 / #355) — solo and dual cases
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn g01_split_mono_solo_emits_at_unity_gain() {
    let (chain, registry) = chain_with_blocks(
        "g01",
        input_mono(vec![0, 1]),
        vec![],
        // split-mono: 2 channels in mono mode
        output(ChannelMode::Stereo, vec![0, 1]),
    );
    let peak = measure_steady_peak(&chain, &registry, 2, &[0.5, 0.0], 2, 4);
    assert!(
        (peak - 0.5).abs() < TOLERANCE,
        "split-mono solo must emit at unity; got {peak}"
    );
}

#[test]
fn g02_split_mono_dual_below_limiter_knee_sums() {
    let (chain, registry) = chain_with_blocks(
        "g02",
        input_mono(vec![0, 1]),
        vec![],
        output(ChannelMode::Stereo, vec![0, 1]),
    );
    let peak = measure_steady_peak(&chain, &registry, 2, &[0.3, 0.3], 2, 4);
    assert!(
        (peak - 0.6).abs() < TOLERANCE,
        "split-mono dual below knee must sum at unity per stream; got {peak}"
    );
}

#[test]
fn g03_split_mono_dual_above_knee_is_softly_saturated() {
    // Issue #496: pin the PROPERTIES instead of `peak ≈ tanh(sum)`.
    // The old tanh form was discontinuous + non-monotonic (RED in
    // runtime_dsp::tests). What this invariant really guards is "when
    // dual mono sums above the knee, the output stays bounded and
    // loud — no DAC clip, no quiet collapse".
    let (chain, registry) = chain_with_blocks(
        "g03",
        input_mono(vec![0, 1]),
        vec![],
        output(ChannelMode::Stereo, vec![0, 1]),
    );
    let peak = measure_steady_peak(&chain, &registry, 2, &[0.8, 0.8], 2, 4);
    assert!(
        peak <= 1.0,
        "split-mono dual sum must be bounded ≤ 1.0; got {peak}"
    );
    assert!(peak < 1.6, "must be reduced from raw sum 1.6; got {peak}");
    assert!(peak > 0.9, "must stay loud (no quiet collapse); got {peak}");
}

#[test]
fn g04_mono_input_broadcasts_to_both_output_channels() {
    let (chain, registry) = chain_with_blocks(
        "g04",
        input_mono(vec![0]),
        vec![],
        output(ChannelMode::Stereo, vec![0, 1]),
    );
    let peaks = measure_steady_per_channel_peak(&chain, &registry, 1, &[0.4], 2, 4);
    assert!(
        (peaks[0] - peaks[1]).abs() < TOLERANCE,
        "L peak {} must equal R peak {}",
        peaks[0],
        peaks[1]
    );
    assert!(
        peaks[0] > 0.3,
        "L must carry signal at unity; got {}",
        peaks[0]
    );
    assert!(
        peaks[1] > 0.3,
        "R must carry signal at unity; got {}",
        peaks[1]
    );
}

// ─────────────────────────────────────────────────────────────────────────
// H. Anti-revert structural pins
// ─────────────────────────────────────────────────────────────────────────

/// CONTRACT (CLAUDE.md invariant #10): split-mono solo must equal
/// single-mono solo at the same input level. A drift here means a
/// preemptive scale was reintroduced — search for `split_scale` in
/// runtime.rs and remove the attenuation.
#[test]
fn h01_split_mono_solo_equals_single_mono_solo() {
    let (split, split_registry) = chain_with_blocks(
        "h01_split",
        input_mono(vec![0, 1]),
        vec![],
        output(ChannelMode::Stereo, vec![0, 1]),
    );
    let (single, single_registry) = chain_with_blocks(
        "h01_single",
        input_mono(vec![0]),
        vec![],
        output(ChannelMode::Stereo, vec![0, 1]),
    );
    let split_peak = measure_steady_peak(&split, &split_registry, 2, &[0.5, 0.0], 2, 4);
    let single_peak = measure_steady_peak(&single, &single_registry, 1, &[0.5], 2, 4);
    assert!(
        (split_peak - single_peak).abs() < TOLERANCE,
        "split solo {split_peak} must equal single solo {single_peak} — \
         a drift means preemptive scaling was reintroduced"
    );
}

/// PIN: chain composition with a single pure-passthrough block (volume
/// at 100%) must preserve the same level as a chain WITHOUT that block.
/// Catches "block introduces hidden attenuation" silently.
#[test]
fn h02_neutral_block_addition_is_volume_preserving() {
    let (bare, bare_registry) = chain_with_blocks(
        "h02_bare",
        input_mono(vec![0]),
        vec![],
        output(ChannelMode::Stereo, vec![0, 1]),
    );
    let mut p = neutral_params("gain", "volume");
    p.insert("volume", ParameterValue::Float(100.0)); // unity point (issue #400 #3)
    p.insert("mute", ParameterValue::Bool(false));
    let (with_block, with_registry) = chain_with_blocks(
        "h02_with",
        input_mono(vec![0]),
        vec![core_block("v", "gain", "volume", p)],
        output(ChannelMode::Stereo, vec![0, 1]),
    );
    let bare_peak = measure_steady_peak(&bare, &bare_registry, 1, &[0.5], 2, 6);
    let with_peak = measure_steady_peak(&with_block, &with_registry, 1, &[0.5], 2, 6);
    assert!(
        (bare_peak - with_peak).abs() < 0.01,
        "neutral volume block must not change level; bare={bare_peak} with={with_peak}"
    );
}

/// PIN: mono → stereo bus broadcast must symmetric (L=R). Catches the
/// auto-pan regression of the original f38953a4 attempt at #350.
#[test]
fn h03_mono_to_stereo_bus_broadcast_is_symmetric() {
    let (chain, registry) = chain_with_blocks(
        "h03",
        input_mono(vec![0]),
        vec![],
        output(ChannelMode::Stereo, vec![0, 1]),
    );
    let peaks = measure_steady_per_channel_peak(&chain, &registry, 1, &[0.6], 2, 4);
    assert!(
        (peaks[0] - peaks[1]).abs() < 1e-5,
        "L {} and R {} must be EXACTLY equal — broadcast is symmetric, no auto-pan",
        peaks[0],
        peaks[1]
    );
}

// ─────────────────────────────────────────────────────────────────────────
// J. User-reported reproducer (Mac, 2026-04-28)
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn j01_user_reported_mac_volume_drop_does_not_recur() {
    let (chain, registry) = chain_with_blocks(
        "j01",
        input_mono(vec![0]),
        vec![],
        output(ChannelMode::Stereo, vec![0, 1]),
    );
    let peak = measure_steady_peak(&chain, &registry, 1, &[0.3], 2, 8);
    assert!(
        (peak - 0.3).abs() < TOLERANCE,
        "Mac single-mono setup must hold steady at unity; got {peak}"
    );
}

/// REGRESSION DOC: replicates the user's CLEAN chain on 2026-04-28
/// EXACTLY (input mono [0] → blackface_clean with their params →
/// output stereo). Tremolo OMITTED — user clarified it's not active.
/// Filter omitted — disabled in their YAML. Only the amp.
///
/// Measures peak + RMS for a 0.4-amplitude 440 Hz sine input. The
/// numbers in the test output are the engine's authoritative answer
/// to "what does the engine produce for this exact input?". If the
/// user hears something quieter than the test reports, the
/// discrepancy is upstream of engine code (CoreAudio device gain,
/// Scarlett monitor knob, system output volume slider, headphone
/// gain on the Scarlett front panel).
#[test]
fn j02_user_clean_chain_blackface_only_signature() {
    let mut p = neutral_params("amp", "blackface_clean");
    p.insert("gain", ParameterValue::Float(0.0));
    p.insert("bass", ParameterValue::Float(50.0));
    p.insert("middle", ParameterValue::Float(50.0));
    p.insert("treble", ParameterValue::Float(50.0));
    p.insert("master", ParameterValue::Float(100.0));
    p.insert("output", ParameterValue::Float(50.0));
    p.insert("bright", ParameterValue::Bool(true));
    p.insert("sag", ParameterValue::Float(14.0));
    p.insert("room_mix", ParameterValue::Float(14.0));
    p.insert("input", ParameterValue::Float(50.0));
    let (chain, registry) = chain_with_blocks(
        "j02",
        input_mono(vec![0]),
        vec![core_block("amp", "amp", "blackface_clean", p)],
        output(ChannelMode::Stereo, vec![0, 1]),
    );
    let runtime = build_runtime(&chain, &registry);
    let sr = SR;
    let mut all_samples: Vec<f32> = Vec::new();
    let mut phase = 0.0_f32;
    let inc = std::f32::consts::TAU * 440.0 / sr;
    for _ in 0..16 {
        let mut data = vec![0.0_f32; 256];
        for s in data.iter_mut() {
            *s = phase.sin() * 0.4;
            phase = (phase + inc).rem_euclid(std::f32::consts::TAU);
        }
        let out = drive_and_capture(&runtime, 1, &data, 2);
        all_samples.extend(out);
    }
    let steady = &all_samples[1024..];
    let peak = peak_abs(steady);
    let rms = (steady.iter().map(|s| s * s).sum::<f32>() / steady.len() as f32).sqrt();
    let peak_db = 20.0 * peak.log10();
    let rms_db = 20.0 * rms.log10();
    eprintln!(
        "[j02] blackface_clean signature: peak={peak} ({peak_db:.2} dBFS), \
         rms={rms} ({rms_db:.2} dBFS)"
    );
    assert!(
        peak > 0.5,
        "blackface_clean with master=100 + input 0.4 sine MUST output above 0.5 peak; \
         got {peak}. If this fails, engine is attenuating the signal — bug in code."
    );
    assert!(
        rms > 0.15,
        "blackface_clean RMS must be above 0.15; got {rms}. Low RMS = excessive limiter."
    );
}

#[allow(dead_code)]
fn _suppress_audio_frame_dead_code(f: AudioFrame) -> AudioFrame {
    f
}

// ─────────────────────────────────────────────────────────────────────────
// K. preset.volume (issue #440)
// ─────────────────────────────────────────────────────────────────────────
//
// Chain.volume é aplicado pelo engine no master output do
// process_output_f32. Estes tests verificam:
//   1. build_chain_runtime_state lê o chain.volume e seta o atomic.
//   2. update_chain_runtime_state (chain edit) propaga o volume novo.
//   3. process_output_f32 multiplica out pelo volume / 100.
//
// Sem esses tests, o handler Slint pode acionar o callback Rust e o
// usuário não ouve diferença porque o engine não está propagando.

const VOLUME_TOLERANCE: f32 = 0.01;

fn unity_passthrough_chain(id: &str, volume: f32) -> (Chain, Vec<IoBinding>) {
    let (mut chain, registry) = chain_with_blocks(
        id,
        input_mono(vec![0]),
        vec![],
        output(ChannelMode::Mono, vec![0]),
    );
    chain.volume = volume;
    (chain, registry)
}

#[test]
fn k01_chain_volume_100_is_unity() {
    let (chain, registry) = unity_passthrough_chain("k01", 100.0);
    let runtime = build_runtime(&chain, &registry);
    assert!(
        (runtime.volume_pct() - 100.0).abs() < VOLUME_TOLERANCE,
        "build_chain_runtime_state should propagate chain.volume=100 \
         to runtime.volume_pct(); got {}",
        runtime.volume_pct()
    );
    let peak = measure_steady_peak(&chain, &registry, 1, &[0.5], 1, 5);
    assert!(
        (peak - 0.5).abs() < VOLUME_TOLERANCE,
        "volume=100 should be unity; expected peak≈0.5, got {peak}"
    );
}

#[test]
fn k02_chain_volume_50_halves_output() {
    let (chain, registry) = unity_passthrough_chain("k02", 50.0);
    let runtime = build_runtime(&chain, &registry);
    assert!(
        (runtime.volume_pct() - 50.0).abs() < VOLUME_TOLERANCE,
        "chain.volume=50 should land on runtime.volume_pct(); got {}",
        runtime.volume_pct()
    );
    let peak = measure_steady_peak(&chain, &registry, 1, &[0.5], 1, 5);
    assert!(
        (peak - 0.25).abs() < VOLUME_TOLERANCE,
        "volume=50 attenuates by half; expected peak≈0.25, got {peak}"
    );
}

#[test]
fn k03_chain_volume_200_doubles_output() {
    let (chain, registry) = unity_passthrough_chain("k03", 200.0);
    let peak = measure_steady_peak(&chain, &registry, 1, &[0.3], 1, 5);
    // input 0.3 × 2.0 = 0.6
    assert!(
        (peak - 0.6).abs() < VOLUME_TOLERANCE,
        "volume=200 doubles; expected peak≈0.6, got {peak}"
    );
}

#[test]
fn k07_volume_boost_on_hot_signal_is_limited_not_clipped() {
    // The user's CPM 22 case: Chain.volume = 145 on a hot chain. Today the
    // master volume is multiplied AFTER the per-sample output limiter, so a
    // hot signal (≈0.9) limited to ≈0.72 then ×2.0 = 1.43 — hard clip at the
    // DAC, NOTHING limits after the multiply on the single-stream path.
    //
    // Contract (this file's header: "clipping is the output limiter's job"):
    // volume must be applied BEFORE the limiter so the limiter sees the
    // post-volume signal and holds it ≤ full scale. With the fix:
    //   0.9 × 2.0 = 1.8 → tanh(1.8) ≈ 0.947  (clip-free)
    // The k01–k04 invariants use sub-0.95 signals so they are unaffected
    // either way (tanh transparent below the knee).
    let (chain, registry) = unity_passthrough_chain("k07", 200.0);
    let peak = measure_steady_peak(&chain, &registry, 1, &[0.9], 1, 5);
    assert!(
        peak <= 1.0 + VOLUME_TOLERANCE,
        "hot signal × volume boost must be limited ≤ full scale, not hard \
         clipped; got peak {peak} (volume applied after the limiter = bug)"
    );
}

#[test]
fn k04_chain_volume_zero_silences_output() {
    let (chain, registry) = unity_passthrough_chain("k04", 0.0);
    let peak = measure_steady_peak(&chain, &registry, 1, &[0.5], 1, 5);
    assert!(
        peak < VOLUME_TOLERANCE,
        "volume=0 should silence output; got peak {peak}"
    );
}

#[test]
fn k05_update_chain_runtime_state_propagates_volume() {
    // Cenário do bug que o usuário reportou: chain construída com
    // volume=100, slider arrasta pra 150, engine deve VER 150 sem
    // teardown. update_chain_runtime_state é o path que o handler
    // chain_volume_changed dispara via sync_live_chain_runtime → upsert
    // → update_chain_runtime_state.
    let (chain100, registry) = unity_passthrough_chain("k05", 100.0);
    let runtime = build_runtime(&chain100, &registry);
    assert!((runtime.volume_pct() - 100.0).abs() < VOLUME_TOLERANCE);

    let mut chain150 = chain100.clone();
    chain150.volume = 150.0;
    super::update_chain_runtime_state(
        &runtime,
        &chain150,
        SR,
        false,
        &[DEFAULT_ELASTIC_TARGET],
        &registry,
    )
    .expect("update_chain_runtime_state should propagate volume");
    assert!(
        (runtime.volume_pct() - 150.0).abs() < VOLUME_TOLERANCE,
        "after update_chain_runtime_state with chain.volume=150, \
         runtime.volume_pct() should be 150; got {}",
        runtime.volume_pct()
    );

    // Sanity: process_output_f32 reflete o novo volume sem rebuild.
    let data = const_interleaved(&[0.4], 256);
    // Drain initial fade-in callbacks.
    for _ in 0..3 {
        let _ = drive_and_capture(&runtime, 1, &data, 1);
    }
    let out = drive_and_capture(&runtime, 1, &data, 1);
    let peak = peak_abs(&out);
    // input 0.4 × 1.5 = 0.6
    assert!(
        (peak - 0.6).abs() < VOLUME_TOLERANCE,
        "after update to volume=150, peak should be ≈0.6 (0.4 × 1.5); got {peak}"
    );
}

#[test]
fn k06_runtime_graph_upsert_propagates_volume_on_existing_chain() {
    // Reproduz exatamente o caminho que o slider dispara em produção:
    // chain_row_wiring::on_chain_volume_changed → sync_live_chain_runtime →
    // ProjectRuntimeController::upsert_chain → upsert_chain_with_resolved →
    // RuntimeGraph::upsert_chain (chain já existe → update_chain_runtime_state).
    //
    // Se este test passar mas o app não responder ao slider, o bug está
    // FORA do engine (Slint callback, Rust handler, ou outra camada).
    let (chain_v100, registry) = unity_passthrough_chain("k06", 100.0);
    let mut graph = crate::runtime_graph::RuntimeGraph {
        chains: std::collections::HashMap::new(),
    };

    let runtime = graph
        .upsert_chain(
            &chain_v100,
            SR,
            &std::collections::HashMap::new(),
            false,
            &[DEFAULT_ELASTIC_TARGET],
            &registry,
        )
        .expect("first upsert builds chain runtime");
    assert!(
        (runtime.volume_pct() - 100.0).abs() < VOLUME_TOLERANCE,
        "first upsert: volume_pct should be 100; got {}",
        runtime.volume_pct()
    );

    // Slider arrasta de 100 pra 175. Handler atualiza chain.volume e
    // re-upserta no graph. Como a chain já existe, vai pro path de
    // update_chain_runtime_state — DEVE refletir sem teardown.
    let mut chain_v175 = chain_v100.clone();
    chain_v175.volume = 175.0;
    let runtime_after = graph
        .upsert_chain(
            &chain_v175,
            SR,
            &std::collections::HashMap::new(),
            false,
            &[DEFAULT_ELASTIC_TARGET],
            &registry,
        )
        .expect("re-upsert updates volume in place");
    assert!(
        Arc::ptr_eq(&runtime, &runtime_after),
        "re-upsert with existing chain should return the SAME Arc, \
         confirming update_chain_runtime_state ran (not rebuild)"
    );
    assert!(
        (runtime_after.volume_pct() - 175.0).abs() < VOLUME_TOLERANCE,
        "after re-upsert with chain.volume=175, runtime.volume_pct() \
         should be 175; got {}",
        runtime_after.volume_pct()
    );
}

