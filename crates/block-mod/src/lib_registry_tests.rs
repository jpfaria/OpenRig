//! block-mod native registry-level + display-helper tests (issue #792
//! split from lib_tests.rs). Shares default_params via super::tests.

use block_core::{AudioChannelLayout, BlockProcessor};

use super::tests::default_params;
use super::*;

// ── native registry-level process tests ───────────────────────────

fn native_mod_models() -> Vec<&'static str> {
    supported_models()
        .iter()
        .copied()
        .filter(|m| mod_type_label(m) == "NATIVE")
        .collect()
}

#[test]
fn native_mod_build_mono_at_44100() {
    for model in native_mod_models() {
        let params = default_params(model);
        let result = build_modulation_processor_for_layout(
            model,
            &params,
            44_100.0,
            AudioChannelLayout::Mono,
        );
        assert!(
            result.is_ok(),
            "{model} should build mono at 44100Hz: {:?}",
            result.err()
        );
    }
}

#[test]
fn native_mod_build_stereo_at_44100() {
    for model in native_mod_models() {
        let params = default_params(model);
        let result = build_modulation_processor_for_layout(
            model,
            &params,
            44_100.0,
            AudioChannelLayout::Stereo,
        );
        assert!(
            result.is_ok(),
            "{model} should build stereo at 44100Hz: {:?}",
            result.err()
        );
    }
}

#[test]
fn native_mod_process_silence_1024_all_finite() {
    for model in native_mod_models() {
        let params = default_params(model);
        let mut proc = build_modulation_processor_for_layout(
            model,
            &params,
            44_100.0,
            AudioChannelLayout::Mono,
        )
        .expect("build");
        match &mut proc {
            BlockProcessor::Mono(p) => {
                for i in 0..1024 {
                    let out = p.process_sample(0.0);
                    assert!(out.is_finite(), "{model} mono not finite at sample {i}");
                }
            }
            BlockProcessor::Stereo(p) => {
                for i in 0..1024 {
                    let [l, r] = p.process_frame([0.0, 0.0]);
                    assert!(
                        l.is_finite() && r.is_finite(),
                        "{model} stereo not finite at sample {i}"
                    );
                }
            }
        }
    }
}

#[test]
fn native_mod_process_sine_1024_all_finite_and_nonzero() {
    let sr = 44_100.0_f32;
    for model in native_mod_models() {
        let params = default_params(model);
        let mut proc =
            build_modulation_processor_for_layout(model, &params, sr, AudioChannelLayout::Mono)
                .expect("build");
        let mut any_nonzero = false;
        match &mut proc {
            BlockProcessor::Mono(p) => {
                for i in 0..1024 {
                    let input = (2.0 * std::f32::consts::PI * 440.0 * i as f32 / sr).sin();
                    let out = p.process_sample(input);
                    assert!(
                        out.is_finite(),
                        "{model} mono sine not finite at sample {i}"
                    );
                    if out.abs() > 1e-10 {
                        any_nonzero = true;
                    }
                }
            }
            BlockProcessor::Stereo(p) => {
                for i in 0..1024 {
                    let input = (2.0 * std::f32::consts::PI * 440.0 * i as f32 / sr).sin();
                    let [l, r] = p.process_frame([input, input]);
                    assert!(
                        l.is_finite() && r.is_finite(),
                        "{model} stereo sine not finite at sample {i}"
                    );
                    if l.abs() > 1e-10 || r.abs() > 1e-10 {
                        any_nonzero = true;
                    }
                }
            }
        }
        assert!(any_nonzero, "{model} produced all zeros for sine input");
    }
}

#[test]
fn native_mod_process_sine_stereo_1024_all_finite() {
    let sr = 44_100.0_f32;
    for model in native_mod_models() {
        let params = default_params(model);
        let mut proc =
            build_modulation_processor_for_layout(model, &params, sr, AudioChannelLayout::Stereo)
                .expect("build");
        match &mut proc {
            BlockProcessor::Stereo(p) => {
                for i in 0..1024 {
                    let input = (2.0 * std::f32::consts::PI * 440.0 * i as f32 / sr).sin();
                    let [l, r] = p.process_frame([input, input]);
                    assert!(
                        l.is_finite() && r.is_finite(),
                        "{model} stereo not finite at sample {i}"
                    );
                }
            }
            _ => panic!("{model} stereo build returned Mono processor"),
        }
    }
}

#[test]
fn native_mod_process_block_mono_all_finite() {
    let sr = 44_100.0_f32;
    for model in native_mod_models() {
        let params = default_params(model);
        let mut proc =
            build_modulation_processor_for_layout(model, &params, sr, AudioChannelLayout::Mono)
                .expect("build");
        match &mut proc {
            BlockProcessor::Mono(p) => {
                let mut buf: Vec<f32> = (0..1024)
                    .map(|i| (2.0 * std::f32::consts::PI * 440.0 * i as f32 / sr).sin())
                    .collect();
                p.process_block(&mut buf);
                for (i, s) in buf.iter().enumerate() {
                    assert!(s.is_finite(), "{model} mono block not finite at frame {i}");
                }
            }
            BlockProcessor::Stereo(p) => {
                let mut buf: Vec<[f32; 2]> = (0..1024)
                    .map(|i| {
                        let s = (2.0 * std::f32::consts::PI * 440.0 * i as f32 / sr).sin();
                        [s, s]
                    })
                    .collect();
                p.process_block(&mut buf);
                for (i, [l, r]) in buf.iter().enumerate() {
                    assert!(
                        l.is_finite() && r.is_finite(),
                        "{model} stereo block not finite at frame {i}"
                    );
                }
            }
        }
    }
}

// ── display_name / brand / type_label helpers ────────────────────

#[test]
fn classic_chorus_display_name_matches() {
    assert_eq!(mod_display_name("classic_chorus"), "Classic Chorus");
}

#[test]
fn ensemble_chorus_display_name_matches() {
    assert_eq!(mod_display_name("ensemble_chorus"), "Ensemble Chorus");
}

#[test]
fn stereo_chorus_display_name_matches() {
    assert_eq!(mod_display_name("stereo_chorus"), "Stereo Chorus");
}

#[test]
fn tremolo_sine_display_name_matches() {
    assert_eq!(mod_display_name("tremolo_sine"), "Sine Tremolo");
}

#[test]
fn vibrato_display_name_matches() {
    assert_eq!(mod_display_name("vibrato"), "Vibrato");
}

#[test]
fn native_models_type_label_is_native() {
    for model in &[
        "classic_chorus",
        "ensemble_chorus",
        "stereo_chorus",
        "tremolo_sine",
        "vibrato",
    ] {
        assert_eq!(
            mod_type_label(model),
            "NATIVE",
            "wrong type_label for {}",
            model
        );
    }
}

#[test]
fn unknown_model_returns_empty_strings() {
    assert_eq!(mod_display_name("nonexistent"), "");
    assert_eq!(mod_brand("nonexistent"), "");
    assert_eq!(mod_type_label("nonexistent"), "");
}
