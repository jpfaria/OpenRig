use crate::registry::GainModelDefinition;
use crate::GainBackendKind;
use anyhow::Result;
use block_core::param::{
    bool_parameter, float_parameter, required_bool, required_f32, ModelParameterSchema,
    ParameterSet, ParameterUnit,
};
use block_core::{db_to_lin, AudioChannelLayout, BlockProcessor, ModelAudioMode, MonoProcessor};

pub const MODEL_ID: &str = "volume";
pub const DISPLAY_NAME: &str = "Volume";

struct VolumeProcessor {
    gain: f32,
    mute: bool,
}

impl MonoProcessor for VolumeProcessor {
    fn process_sample(&mut self, input: f32) -> f32 {
        if self.mute { 0.0 } else { input * self.gain }
    }
}

pub fn model_schema() -> ModelParameterSchema {
    ModelParameterSchema {
        effect_type: block_core::EFFECT_TYPE_GAIN.into(),
        model: MODEL_ID.into(),
        display_name: DISPLAY_NAME.into(),
        audio_mode: ModelAudioMode::DualMono,
        parameters: vec![
            float_parameter(
                "volume",
                "Volume",
                None,
                Some(80.0),
                0.0,
                100.0,
                1.0,
                ParameterUnit::Percent,
            ),
            bool_parameter("mute", "Mute", None, Some(false)),
        ],
    }
}

fn validate_params(params: &ParameterSet) -> Result<()> {
    let _ = required_f32(params, "volume").map_err(anyhow::Error::msg)?;
    Ok(())
}

fn asset_summary(_params: &ParameterSet) -> Result<String> {
    Ok("native='volume'".to_string())
}

fn percent_to_db(percent: f32) -> f32 {
    if percent <= 0.0 {
        -60.0
    } else {
        // 0% = -60dB, 80% = 0dB (unity), 100% = +12dB
        let normalized = percent / 100.0;
        -60.0 + normalized * 72.0 // linear map: 0→-60dB, 100→+12dB
    }
}

fn build(
    params: &ParameterSet,
    _sample_rate: f32,
    _layout: AudioChannelLayout,
) -> Result<BlockProcessor> {
    let volume_pct = required_f32(params, "volume").map_err(anyhow::Error::msg)?;
    let mute = required_bool(params, "mute").unwrap_or(false);
    let gain = db_to_lin(percent_to_db(volume_pct));

    Ok(BlockProcessor::Mono(Box::new(VolumeProcessor {
        gain,
        mute,
    })))
}

fn schema() -> Result<ModelParameterSchema> {
    Ok(model_schema())
}

pub const MODEL_DEFINITION: GainModelDefinition = GainModelDefinition {
    id: MODEL_ID,
    display_name: DISPLAY_NAME,
    brand: block_core::BRAND_NATIVE,
    backend_kind: GainBackendKind::Native,
    schema,
    validate: validate_params,
    asset_summary,
    build,
    supported_instruments: block_core::ALL_INSTRUMENTS,
    knob_layout: &[
        block_core::KnobLayoutEntry {
            param_key: "volume",
            svg_cx: 0.0,
            svg_cy: 0.0,
            svg_r: 0.0,
            min: 0.0,
            max: 100.0,
            step: 1.0,
        },
    ],
};

#[cfg(test)]
mod tests {
    use super::*;
    use block_core::param::ParameterSet;
    use block_core::{AudioChannelLayout, BlockProcessor, MonoProcessor};
    use domain::value_objects::ParameterValue;

    // ── helpers ──────────────────────────────────────────────────────

    fn default_params() -> ParameterSet {
        let schema = model_schema();
        ParameterSet::default()
            .normalized_against(&schema)
            .expect("defaults should normalize")
    }

    fn params_with_volume(vol: f32) -> ParameterSet {
        let schema = model_schema();
        let mut ps = ParameterSet::default()
            .normalized_against(&schema)
            .expect("defaults should normalize");
        ps.insert("volume", ParameterValue::Float(vol));
        ps
    }

    fn params_with_volume_and_mute(vol: f32, mute: bool) -> ParameterSet {
        let schema = model_schema();
        let mut ps = ParameterSet::default()
            .normalized_against(&schema)
            .expect("defaults should normalize");
        ps.insert("volume", ParameterValue::Float(vol));
        ps.insert("mute", ParameterValue::Bool(mute));
        ps
    }

    fn build_mono(params: &ParameterSet, sr: f32) -> Box<dyn MonoProcessor> {
        match build(params, sr, AudioChannelLayout::Mono).unwrap() {
            BlockProcessor::Mono(p) => p,
            BlockProcessor::Stereo(_) => panic!("expected Mono"),
        }
    }

    fn sine_block(n: usize, freq: f32, sr: f32) -> Vec<f32> {
        (0..n)
            .map(|i| (i as f32 / sr * freq * std::f32::consts::TAU).sin() * 0.5)
            .collect()
    }

    fn rms(samples: &[f32]) -> f32 {
        (samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32).sqrt()
    }

    // ── schema tests ────────────────────────────────────────────────

    #[test]
    fn schema_model_id_matches() {
        let s = model_schema();
        assert_eq!(s.model, MODEL_ID);
        assert_eq!(s.effect_type, "gain");
    }

    #[test]
    fn schema_has_volume_and_mute_params() {
        let s = model_schema();
        let paths: Vec<_> = s.parameters.iter().map(|p| p.path.as_str()).collect();
        assert_eq!(paths, vec!["volume", "mute"]);
    }

    #[test]
    fn schema_audio_mode_is_dual_mono() {
        assert_eq!(model_schema().audio_mode, block_core::ModelAudioMode::DualMono);
    }

    // ── validate tests ──────────────────────────────────────────────

    #[test]
    fn validate_accepts_defaults() {
        let params = default_params();
        assert!(validate_params(&params).is_ok());
    }

    #[test]
    fn validate_rejects_missing_volume() {
        let mut params = default_params();
        params.values.remove("volume");
        assert!(validate_params(&params).is_err());
    }

    // ── asset_summary ───────────────────────────────────────────────

    #[test]
    fn asset_summary_returns_expected_string() {
        let params = default_params();
        let summary = asset_summary(&params).unwrap();
        assert!(summary.contains("volume"));
    }

    // ── build tests ─────────────────────────────────────────────────

    #[test]
    fn build_always_returns_mono_regardless_of_layout() {
        let params = default_params();
        // Even for stereo layout, volume returns Mono (engine wraps to dual-mono)
        let proc_mono = build(&params, 48000.0, AudioChannelLayout::Mono).unwrap();
        assert!(matches!(proc_mono, BlockProcessor::Mono(_)));

        let proc_stereo = build(&params, 48000.0, AudioChannelLayout::Stereo).unwrap();
        assert!(matches!(proc_stereo, BlockProcessor::Mono(_)));
    }

    #[test]
    fn build_at_various_sample_rates() {
        let params = default_params();
        for &sr in &[44100.0_f32, 48000.0, 88200.0, 96000.0] {
            assert!(build(&params, sr, AudioChannelLayout::Mono).is_ok(), "build failed at {sr}");
        }
    }

    // ── process silence ─────────────────────────────────────────────

    #[test]
    fn process_silence_1024_frames_all_finite() {
        let params = default_params();
        let mut proc = build_mono(&params, 48000.0);
        for i in 0..1024 {
            let out = proc.process_sample(0.0);
            assert!(out.is_finite(), "non-finite at frame {i}: {out}");
        }
    }

    #[test]
    fn process_silence_produces_zero_output() {
        let params = default_params();
        let mut proc = build_mono(&params, 48000.0);
        for i in 0..256 {
            let out = proc.process_sample(0.0);
            assert_eq!(out, 0.0, "silence in should produce silence out at {i}");
        }
    }

    // ── process sine ────────────────────────────────────────────────

    #[test]
    fn process_sine_440hz_all_finite() {
        let sr = 48000.0;
        let params = default_params();
        let mut proc = build_mono(&params, sr);
        let input = sine_block(1024, 440.0, sr);
        for (i, &s) in input.iter().enumerate() {
            let out = proc.process_sample(s);
            assert!(out.is_finite(), "non-finite at sample {i}: {out}");
        }
    }

    #[test]
    fn process_sine_produces_nonzero_output_at_default_volume() {
        let sr = 48000.0;
        let params = default_params(); // volume=80%
        let mut proc = build_mono(&params, sr);
        let input = sine_block(1024, 440.0, sr);
        let output: Vec<f32> = input.iter().map(|&s| proc.process_sample(s)).collect();
        let out_rms = rms(&output);
        assert!(out_rms > 0.01, "default volume should produce audible output, rms={out_rms}");
    }

    // ── volume parameter behavior ───────────────────────────────────

    #[test]
    fn volume_zero_produces_near_silence() {
        let sr = 48000.0;
        let params = params_with_volume(0.0);
        let mut proc = build_mono(&params, sr);
        let input = sine_block(512, 440.0, sr);
        for (i, &s) in input.iter().enumerate() {
            let out = proc.process_sample(s);
            assert!(
                out.abs() < 1e-3,
                "volume=0 should produce near-silence, got {out} at sample {i}"
            );
        }
    }

    #[test]
    fn volume_increase_raises_output_level() {
        let sr = 48000.0;
        let input = sine_block(1024, 440.0, sr);

        let params_low = params_with_volume(30.0);
        let params_high = params_with_volume(90.0);

        let mut proc_low = build_mono(&params_low, sr);
        let mut proc_high = build_mono(&params_high, sr);

        let out_low: Vec<f32> = input.iter().map(|&s| proc_low.process_sample(s)).collect();
        let out_high: Vec<f32> = input.iter().map(|&s| proc_high.process_sample(s)).collect();

        let rms_low = rms(&out_low);
        let rms_high = rms(&out_high);

        assert!(
            rms_high > rms_low,
            "higher volume should produce louder output: low={rms_low}, high={rms_high}"
        );
    }

    #[test]
    fn volume_100_boosts_above_unity() {
        let sr = 48000.0;
        let params_80 = params_with_volume(80.0);
        let params_100 = params_with_volume(100.0);

        let input = sine_block(1024, 440.0, sr);

        let mut proc_80 = build_mono(&params_80, sr);
        let mut proc_100 = build_mono(&params_100, sr);

        let out_80: Vec<f32> = input.iter().map(|&s| proc_80.process_sample(s)).collect();
        let out_100: Vec<f32> = input.iter().map(|&s| proc_100.process_sample(s)).collect();

        let rms_80 = rms(&out_80);
        let rms_100 = rms(&out_100);

        assert!(
            rms_100 > rms_80,
            "volume=100 should boost above volume=80: 80={rms_80}, 100={rms_100}"
        );
    }

    // ── mute parameter behavior ─────────────────────────────────────

    #[test]
    fn mute_true_produces_silence() {
        let sr = 48000.0;
        let params = params_with_volume_and_mute(80.0, true);
        let mut proc = build_mono(&params, sr);
        let input = sine_block(512, 440.0, sr);
        for (i, &s) in input.iter().enumerate() {
            let out = proc.process_sample(s);
            assert_eq!(out, 0.0, "mute=true should produce silence, got {out} at sample {i}");
        }
    }

    #[test]
    fn mute_false_passes_signal() {
        let sr = 48000.0;
        let params = params_with_volume_and_mute(80.0, false);
        let mut proc = build_mono(&params, sr);
        let input = sine_block(512, 440.0, sr);
        let output: Vec<f32> = input.iter().map(|&s| proc.process_sample(s)).collect();
        let out_rms = rms(&output);
        assert!(out_rms > 0.01, "mute=false should pass signal, rms={out_rms}");
    }

    // ── percent_to_db tests ─────────────────────────────────────────

    #[test]
    fn percent_to_db_zero_returns_minus_60() {
        assert_eq!(percent_to_db(0.0), -60.0);
    }

    #[test]
    fn percent_to_db_negative_returns_minus_60() {
        assert_eq!(percent_to_db(-10.0), -60.0);
    }

    #[test]
    fn percent_to_db_100_returns_plus_12() {
        let db = percent_to_db(100.0);
        assert!((db - 12.0).abs() < 1e-4, "100% should be +12dB, got {db}");
    }

    #[test]
    fn percent_to_db_80_is_near_unity() {
        // 80% → -60 + 0.8*72 = -60 + 57.6 = -2.4 dB (close to unity)
        let db = percent_to_db(80.0);
        assert!(db > -5.0 && db < 1.0, "80% should be near unity, got {db}dB");
    }

    #[test]
    fn percent_to_db_monotonically_increasing() {
        let mut prev = percent_to_db(0.0);
        for pct in (1..=100).step_by(1) {
            let db = percent_to_db(pct as f32);
            assert!(db >= prev, "percent_to_db should be monotonic: {prev} -> {db} at {pct}%");
            prev = db;
        }
    }

    // ── golden sample test ──────────────────────────────────────────

    #[test]
    fn golden_sample_deterministic_at_80_percent() {
        let sr = 48000.0;
        let params = params_with_volume(80.0);

        let mut proc1 = build_mono(&params, sr);
        let mut proc2 = build_mono(&params, sr);

        let input = sine_block(64, 440.0, sr);

        let out1: Vec<f32> = input.iter().map(|&s| proc1.process_sample(s)).collect();
        let out2: Vec<f32> = input.iter().map(|&s| proc2.process_sample(s)).collect();

        for (i, (&a, &b)) in out1.iter().zip(out2.iter()).enumerate() {
            assert!(
                (a - b).abs() < 1e-4,
                "golden sample mismatch at {i}: {a} vs {b}"
            );
        }
    }

    // ── volume is a simple gain (linearity check) ───────────────────

    #[test]
    fn volume_processor_is_linear_when_not_muted() {
        let sr = 48000.0;
        let params = params_with_volume_and_mute(80.0, false);
        let mut proc = build_mono(&params, sr);

        let out1 = proc.process_sample(0.5);
        // Re-build to reset state (volume has no state, but good practice)
        let mut proc2 = build_mono(&params, sr);
        let out2 = proc2.process_sample(1.0);

        // For a pure gain, output should scale linearly: out(1.0) = 2 * out(0.5)
        assert!(
            (out2 - 2.0 * out1).abs() < 1e-6,
            "volume should be linear: out(1.0)={out2}, 2*out(0.5)={}", 2.0 * out1
        );
    }

    // ── hot input signal ────────────────────────────────────────────

    #[test]
    fn hot_input_signal_produces_finite_output() {
        let sr = 48000.0;
        let params = params_with_volume(100.0);
        let mut proc = build_mono(&params, sr);
        for i in 0..256 {
            let input = (i as f32 / 256.0 * std::f32::consts::TAU).sin() * 5.0;
            let out = proc.process_sample(input);
            assert!(out.is_finite(), "hot input produced non-finite at {i}: {out}");
        }
    }
}
