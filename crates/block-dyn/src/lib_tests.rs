//! Tests for `block-dyn`. Lifted out of `lib.rs` so the production file
//! stays under the size cap. Re-attached as `mod tests` of the parent via
//! `#[cfg(test)] #[path = "lib_tests.rs"] mod tests;`.

use super::*;
use block_core::param::ParameterSet;
use block_core::{AudioChannelLayout, BlockProcessor, ModelAudioMode};
use domain::value_objects::ParameterValue;

// ── Helper ──────────────────────────────────────────────────────

fn default_params_for(model: &str) -> ParameterSet {
    let schema = dynamics_model_schema(model).expect("schema should exist");
    ParameterSet::default()
        .normalized_against(&schema)
        .expect("defaults should normalize")
}

fn process_silence(processor: &mut BlockProcessor, frames: usize) -> Vec<f32> {
    match processor {
        BlockProcessor::Mono(p) => {
            let mut buf = vec![0.0_f32; frames];
            p.process_block(&mut buf);
            buf
        }
        BlockProcessor::Stereo(p) => {
            let mut buf = vec![[0.0_f32; 2]; frames];
            p.process_block(&mut buf);
            buf.iter().flat_map(|pair| pair.iter().copied()).collect()
        }
    }
}

// ── Registry-level tests ────────────────────────────────────────

#[test]
fn supported_dyn_models_expose_schema() {
    for model in supported_models() {
        assert!(
            dynamics_model_schema(model).is_ok(),
            "expected '{model}' to have a valid schema"
        );
    }
}

#[test]
fn compressor_supported_models_is_subset_of_all() {
    let all = supported_models();
    for model in compressor_supported_models() {
        assert!(
            all.contains(model),
            "compressor model '{model}' missing from supported_models"
        );
    }
}

#[test]
fn gate_supported_models_is_subset_of_all() {
    let all = supported_models();
    for model in gate_supported_models() {
        assert!(
            all.contains(model),
            "gate model '{model}' missing from supported_models"
        );
    }
}

// ── Compressor: Studio Clean ────────────────────────────────────

#[test]
fn compressor_studio_clean_schema_has_expected_params() {
    let schema = dynamics_model_schema("compressor_studio_clean").expect("schema");
    assert_eq!(schema.effect_type, "dynamics");
    assert_eq!(schema.model, "compressor_studio_clean");
    assert_eq!(schema.audio_mode, ModelAudioMode::DualMono);
    let param_names: Vec<&str> = schema.parameters.iter().map(|p| p.path.as_str()).collect();
    assert!(param_names.contains(&"threshold"));
    assert!(param_names.contains(&"ratio"));
    assert!(param_names.contains(&"attack_ms"));
    assert!(param_names.contains(&"release_ms"));
    assert!(param_names.contains(&"makeup_gain"));
    assert!(param_names.contains(&"mix"));
}

#[test]
fn compressor_studio_clean_defaults_normalize() {
    let schema = dynamics_model_schema("compressor_studio_clean").expect("schema");
    let result = ParameterSet::default().normalized_against(&schema);
    assert!(result.is_ok(), "defaults should normalize");
}

#[test]
fn compressor_studio_clean_rejects_out_of_range() {
    let schema = dynamics_model_schema("compressor_studio_clean").expect("schema");
    let mut ps = ParameterSet::default();
    ps.insert("threshold", ParameterValue::Float(200.0)); // max is 100
    assert!(ps.normalized_against(&schema).is_err());
}

/// #706 — "enabling the native compressor changes nothing in the sound".
/// Reproduces with the user's exact rig params. A 4:1 compressor fed a
/// signal that jumps from a quiet to a loud passage MUST reduce the
/// dynamic range between the two passages — regardless of makeup gain,
/// the loud/quiet output ratio has to come out smaller than it went in.
#[test]
fn issue_706_user_params_must_compress_dynamics() {
    let schema = dynamics_model_schema("compressor_studio_clean").expect("schema");
    let mut ps = ParameterSet::default();
    ps.insert("attack_ms", ParameterValue::Float(10.0));
    ps.insert("release_ms", ParameterValue::Float(80.0));
    ps.insert("ratio", ParameterValue::Float(4.0));
    ps.insert("threshold", ParameterValue::Float(70.0));
    ps.insert("mix", ParameterValue::Float(100.0));
    ps.insert("makeup_gain", ParameterValue::Float(50.0));
    let params = ps.normalized_against(&schema).expect("user params normalize");

    let sample_rate = 48_000.0_f32;
    let mut proc = match build_dynamics_processor_for_layout(
        "compressor_studio_clean",
        &params,
        sample_rate,
        AudioChannelLayout::Mono,
    )
    .expect("build mono compressor")
    {
        BlockProcessor::Mono(p) => p,
        BlockProcessor::Stereo(_) => panic!("expected mono"),
    };

    // 440 Hz sine: 0.5 s quiet (-26 dBFS) then 0.5 s loud (-2 dBFS).
    let half = (sample_rate * 0.5) as usize;
    let mut buf: Vec<f32> = (0..half * 2)
        .map(|i| {
            let amp = if i < half { 0.05 } else { 0.8 };
            amp * (2.0 * std::f32::consts::PI * 440.0 * i as f32 / sample_rate).sin()
        })
        .collect();
    let input = buf.clone();
    proc.process_block(&mut buf);

    // Steady-state RMS windows away from the attack edge.
    let rms = |s: &[f32]| (s.iter().map(|x| x * x).sum::<f32>() / s.len() as f32).sqrt();
    let q = half / 2;
    let in_ratio = rms(&input[half + q..]) / rms(&input[q..half]);
    let out_ratio = rms(&buf[half + q..]) / rms(&buf[q..half]);

    assert!(
        out_ratio < in_ratio * 0.9,
        "compressor with the user's params (ratio 4:1, threshold 70) did not \
         reduce dynamic range: loud/quiet ratio in={in_ratio:.2} out={out_ratio:.2} \
         — the block is audibly a no-op (issue #706)"
    );
}

#[test]
fn compressor_studio_clean_build_mono() {
    let params = default_params_for("compressor_studio_clean");
    let proc = build_dynamics_processor_for_layout(
        "compressor_studio_clean",
        &params,
        48_000.0,
        AudioChannelLayout::Mono,
    );
    assert!(proc.is_ok());
    assert!(matches!(proc.unwrap(), BlockProcessor::Mono(_)));
}

#[test]
fn compressor_studio_clean_build_stereo_fails() {
    let params = default_params_for("compressor_studio_clean");
    let result = build_dynamics_processor_for_layout(
        "compressor_studio_clean",
        &params,
        48_000.0,
        AudioChannelLayout::Stereo,
    );
    assert!(result.is_err());
}

#[test]
fn compressor_studio_clean_process_silence_no_nan() {
    let params = default_params_for("compressor_studio_clean");
    let mut proc = build_dynamics_processor_for_layout(
        "compressor_studio_clean",
        &params,
        48_000.0,
        AudioChannelLayout::Mono,
    )
    .expect("build");
    let output = process_silence(&mut proc, 256);
    assert!(output.iter().all(|s| !s.is_nan()), "output contains NaN");
}

#[test]
fn compressor_studio_clean_via_compressor_api() {
    let schema = compressor_model_schema("compressor_studio_clean").expect("schema");
    let params = ParameterSet::default()
        .normalized_against(&schema)
        .expect("defaults");
    let proc = build_compressor_processor_for_layout(
        "compressor_studio_clean",
        &params,
        48_000.0,
        AudioChannelLayout::Mono,
    );
    assert!(proc.is_ok());
}

// ── Gate: Basic Noise Gate ──────────────────────────────────────

#[test]
fn gate_basic_schema_has_expected_params() {
    let schema = dynamics_model_schema("gate_basic").expect("schema");
    assert_eq!(schema.effect_type, "dynamics");
    assert_eq!(schema.model, "gate_basic");
    assert_eq!(schema.audio_mode, ModelAudioMode::DualMono);
    let param_names: Vec<&str> = schema.parameters.iter().map(|p| p.path.as_str()).collect();
    assert!(param_names.contains(&"threshold"));
    assert!(param_names.contains(&"attack_ms"));
    assert!(param_names.contains(&"release_ms"));
}

#[test]
fn gate_basic_defaults_normalize() {
    let schema = dynamics_model_schema("gate_basic").expect("schema");
    let result = ParameterSet::default().normalized_against(&schema);
    assert!(result.is_ok());
}

#[test]
fn gate_basic_rejects_out_of_range() {
    let schema = dynamics_model_schema("gate_basic").expect("schema");
    let mut ps = ParameterSet::default();
    ps.insert("attack_ms", ParameterValue::Float(999.0)); // max is 100
    assert!(ps.normalized_against(&schema).is_err());
}

#[test]
fn gate_basic_build_mono() {
    let params = default_params_for("gate_basic");
    let proc = build_dynamics_processor_for_layout(
        "gate_basic",
        &params,
        48_000.0,
        AudioChannelLayout::Mono,
    );
    assert!(proc.is_ok());
    assert!(matches!(proc.unwrap(), BlockProcessor::Mono(_)));
}

#[test]
fn gate_basic_build_stereo_fails() {
    let params = default_params_for("gate_basic");
    let result = build_dynamics_processor_for_layout(
        "gate_basic",
        &params,
        48_000.0,
        AudioChannelLayout::Stereo,
    );
    assert!(result.is_err());
}

#[test]
fn gate_basic_process_silence_no_nan() {
    let params = default_params_for("gate_basic");
    let mut proc = build_dynamics_processor_for_layout(
        "gate_basic",
        &params,
        48_000.0,
        AudioChannelLayout::Mono,
    )
    .expect("build");
    let output = process_silence(&mut proc, 256);
    assert!(output.iter().all(|s| !s.is_nan()), "output contains NaN");
}

#[test]
fn gate_basic_silence_stays_silent() {
    let params = default_params_for("gate_basic");
    let mut proc = build_dynamics_processor_for_layout(
        "gate_basic",
        &params,
        48_000.0,
        AudioChannelLayout::Mono,
    )
    .expect("build");
    let output = process_silence(&mut proc, 256);
    // Gate should not add energy to silence
    assert!(
        output.iter().all(|s| s.abs() < 1e-6),
        "gate should not add energy to silence"
    );
}

#[test]
fn gate_basic_via_gate_api() {
    let schema = gate_model_schema("gate_basic").expect("schema");
    let params = ParameterSet::default()
        .normalized_against(&schema)
        .expect("defaults");
    let proc =
        build_gate_processor_for_layout("gate_basic", &params, 48_000.0, AudioChannelLayout::Mono);
    assert!(proc.is_ok());
}

// ── Limiter: Brick Wall ─────────────────────────────────────────

#[test]
fn limiter_brickwall_schema_has_expected_params() {
    let schema = dynamics_model_schema("limiter_brickwall").expect("schema");
    assert_eq!(schema.effect_type, "dynamics");
    assert_eq!(schema.model, "limiter_brickwall");
    assert_eq!(schema.audio_mode, ModelAudioMode::DualMono);
    let param_names: Vec<&str> = schema.parameters.iter().map(|p| p.path.as_str()).collect();
    assert!(param_names.contains(&"threshold"));
    assert!(param_names.contains(&"release_ms"));
    assert!(param_names.contains(&"ceiling"));
    assert!(param_names.contains(&"lookahead_ms"));
    assert!(param_names.contains(&"knee_db"));
}

#[test]
fn limiter_brickwall_defaults_normalize() {
    let schema = dynamics_model_schema("limiter_brickwall").expect("schema");
    let result = ParameterSet::default().normalized_against(&schema);
    assert!(result.is_ok());
}

#[test]
fn limiter_brickwall_rejects_out_of_range() {
    let schema = dynamics_model_schema("limiter_brickwall").expect("schema");
    let mut ps = ParameterSet::default();
    ps.insert("threshold", ParameterValue::Float(5.0)); // max is 0.0
    assert!(ps.normalized_against(&schema).is_err());
}

#[test]
fn limiter_brickwall_build_mono() {
    let params = default_params_for("limiter_brickwall");
    let proc = build_dynamics_processor_for_layout(
        "limiter_brickwall",
        &params,
        48_000.0,
        AudioChannelLayout::Mono,
    );
    assert!(proc.is_ok());
    assert!(matches!(proc.unwrap(), BlockProcessor::Mono(_)));
}

#[test]
fn limiter_brickwall_build_stereo_succeeds() {
    let params = default_params_for("limiter_brickwall");
    let proc = build_dynamics_processor_for_layout(
        "limiter_brickwall",
        &params,
        48_000.0,
        AudioChannelLayout::Stereo,
    )
    .expect("stereo build");
    assert!(matches!(proc, BlockProcessor::Stereo(_)));
}

#[test]
fn limiter_brickwall_process_silence_no_nan() {
    let params = default_params_for("limiter_brickwall");
    let mut proc = build_dynamics_processor_for_layout(
        "limiter_brickwall",
        &params,
        48_000.0,
        AudioChannelLayout::Mono,
    )
    .expect("build");
    let output = process_silence(&mut proc, 256);
    assert!(output.iter().all(|s| !s.is_nan()), "output contains NaN");
}

// ── Registry-level process tests for all native models ──────────

fn native_dyn_models() -> Vec<&'static str> {
    supported_models()
        .iter()
        .copied()
        .filter(|m| dyn_type_label(m) == "NATIVE")
        .collect()
}

#[test]
fn native_dyn_process_sine_mono_produces_finite() {
    for model in native_dyn_models() {
        let params = default_params_for(model);
        let mut proc =
            build_dynamics_processor_for_layout(model, &params, 44100.0, AudioChannelLayout::Mono)
                .expect("build");
        match &mut proc {
            BlockProcessor::Mono(ref mut p) => {
                for i in 0..1024 {
                    let input = (i as f32 / 44100.0 * 440.0 * std::f32::consts::TAU).sin() * 0.5;
                    let out = p.process_sample(input);
                    assert!(
                        out.is_finite(),
                        "{model} mono produced non-finite at sample {i}: {out}"
                    );
                }
            }
            _ => panic!("{model} expected Mono processor"),
        }
    }
}

#[test]
fn native_dyn_process_block_1024_silence_all_finite() {
    for model in native_dyn_models() {
        let params = default_params_for(model);
        let mut proc =
            build_dynamics_processor_for_layout(model, &params, 44100.0, AudioChannelLayout::Mono)
                .expect("build");
        let output = process_silence(&mut proc, 1024);
        assert!(
            output.iter().all(|s| s.is_finite()),
            "{model} block silence contains non-finite"
        );
    }
}

#[test]
fn native_dyn_process_block_1024_sine_all_finite() {
    for model in native_dyn_models() {
        let params = default_params_for(model);
        let mut proc =
            build_dynamics_processor_for_layout(model, &params, 44100.0, AudioChannelLayout::Mono)
                .expect("build");
        match &mut proc {
            BlockProcessor::Mono(ref mut p) => {
                let mut buf: Vec<f32> = (0..1024)
                    .map(|i| (i as f32 / 44100.0 * 440.0 * std::f32::consts::TAU).sin() * 0.5)
                    .collect();
                p.process_block(&mut buf);
                for (i, &s) in buf.iter().enumerate() {
                    assert!(s.is_finite(), "{model} block sine non-finite at {i}: {s}");
                }
            }
            _ => panic!("{model} expected Mono processor"),
        }
    }
}

// ── Display name / brand / type label ───────────────────────────

#[test]
fn dyn_display_name_returns_correct_values() {
    assert_eq!(
        dyn_display_name("compressor_studio_clean"),
        "Studio Clean Compressor"
    );
    assert_eq!(dyn_display_name("gate_basic"), "Noise Gate");
    assert_eq!(dyn_display_name("limiter_brickwall"), "Brick Wall Limiter");
}

#[test]
fn dyn_type_label_native_models() {
    assert_eq!(dyn_type_label("compressor_studio_clean"), "NATIVE");
    assert_eq!(dyn_type_label("gate_basic"), "NATIVE");
    assert_eq!(dyn_type_label("limiter_brickwall"), "NATIVE");
}

#[test]
fn dyn_display_name_unknown_returns_empty() {
    assert_eq!(dyn_display_name("nonexistent_model"), "");
}

#[test]
fn dyn_type_label_unknown_returns_empty() {
    assert_eq!(dyn_type_label("nonexistent_model"), "");
}

#[test]
fn dyn_model_visual_returns_some_for_native() {
    let visual = dyn_model_visual("compressor_studio_clean");
    assert!(visual.is_some());
    let v = visual.unwrap();
    assert_eq!(v.type_label, "NATIVE");
}

#[test]
fn dyn_model_visual_returns_none_for_unknown() {
    assert!(dyn_model_visual("nonexistent_model").is_none());
}

#[test]
fn is_dyn_model_available_false_for_uncataloged_disk_model() {
    // Issue #606 (sibling family): an uninstalled disk-package dynamics
    // model must report UNAVAILABLE rather than optimistically true.
    assert!(
        !crate::is_dyn_model_available("lv2_some_compressor_not_installed"),
        "an uninstalled disk-package dynamics model must be unavailable"
    );
}
