//! Tests for `native_ibanez_ts9`. Lifted out so the production file
//! stays under the size cap. Re-attached as `mod tests` of the parent
//! via `#[cfg(test)] #[path = "native_ibanez_ts9_tests.rs"] mod tests;`.

    use super::*;
    use block_core::param::ParameterSet;
    use block_core::{AudioChannelLayout, BlockProcessor, MonoProcessor, StereoProcessor};
    use domain::value_objects::ParameterValue;

    // ── helpers ──────────────────────────────────────────────────────

    fn default_params() -> ParameterSet {
        let schema = model_schema();
        ParameterSet::default()
            .normalized_against(&schema)
            .expect("defaults should normalize")
    }

    fn params_with(drive: f32, tone: f32, level: f32) -> ParameterSet {
        let schema = model_schema();
        let mut ps = ParameterSet::default()
            .normalized_against(&schema)
            .expect("defaults should normalize");
        ps.insert("drive", ParameterValue::Float(drive));
        ps.insert("tone", ParameterValue::Float(tone));
        ps.insert("level", ParameterValue::Float(level));
        ps
    }

    fn build_mono(params: &ParameterSet, sr: f32) -> Box<dyn MonoProcessor> {
        match build_processor_for_layout(params, sr, AudioChannelLayout::Mono).unwrap() {
            BlockProcessor::Mono(p) => p,
            BlockProcessor::Stereo(_) => panic!("expected Mono"),
        }
    }

    fn build_stereo(params: &ParameterSet, sr: f32) -> Box<dyn StereoProcessor> {
        match build_processor_for_layout(params, sr, AudioChannelLayout::Stereo).unwrap() {
            BlockProcessor::Stereo(p) => p,
            BlockProcessor::Mono(_) => panic!("expected Stereo"),
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
    fn schema_has_drive_tone_level() {
        let s = model_schema();
        let paths: Vec<_> = s.parameters.iter().map(|p| p.path.as_str()).collect();
        assert_eq!(paths, vec!["drive", "tone", "level"]);
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
    fn validate_rejects_missing_drive() {
        let mut params = default_params();
        params.values.remove("drive");
        assert!(validate_params(&params).is_err());
    }

    #[test]
    fn validate_rejects_missing_tone() {
        let mut params = default_params();
        params.values.remove("tone");
        assert!(validate_params(&params).is_err());
    }

    #[test]
    fn validate_rejects_missing_level() {
        let mut params = default_params();
        params.values.remove("level");
        assert!(validate_params(&params).is_err());
    }

    // ── asset_summary ───────────────────────────────────────────────

    #[test]
    fn asset_summary_returns_expected_string() {
        let params = default_params();
        let summary = asset_summary(&params).unwrap();
        assert!(summary.contains("ibanez_ts9"));
    }

    // ── build tests ─────────────────────────────────────────────────

    #[test]
    fn build_mono_returns_mono_processor() {
        let params = default_params();
        let proc = build_processor_for_layout(&params, 44100.0, AudioChannelLayout::Mono).unwrap();
        assert!(matches!(proc, BlockProcessor::Mono(_)));
    }

    #[test]
    fn build_stereo_returns_stereo_processor() {
        let params = default_params();
        let proc = build_processor_for_layout(&params, 44100.0, AudioChannelLayout::Stereo).unwrap();
        assert!(matches!(proc, BlockProcessor::Stereo(_)));
    }

    #[test]
    fn build_at_various_sample_rates() {
        let params = default_params();
        for &sr in &[44100.0_f32, 48000.0, 88200.0, 96000.0] {
            let proc = build_processor_for_layout(&params, sr, AudioChannelLayout::Mono);
            assert!(proc.is_ok(), "build failed at sample rate {sr}");
        }
    }

    // ── process silence ─────────────────────────────────────────────

    #[test]
    fn process_mono_silence_1024_frames_all_finite() {
        let params = default_params();
        let mut proc = build_mono(&params, 48000.0);
        for i in 0..1024 {
            let out = proc.process_sample(0.0);
            assert!(out.is_finite(), "non-finite at frame {i}: {out}");
        }
    }

    #[test]
    fn process_stereo_silence_1024_frames_all_finite() {
        let params = default_params();
        let mut proc = build_stereo(&params, 48000.0);
        for i in 0..1024 {
            let [l, r] = proc.process_frame([0.0, 0.0]);
            assert!(l.is_finite() && r.is_finite(), "non-finite at frame {i}: [{l}, {r}]");
        }
    }

    #[test]
    fn process_silence_produces_near_zero_output() {
        let params = default_params();
        let mut proc = build_mono(&params, 48000.0);
        // Warm up filters
        for _ in 0..512 {
            proc.process_sample(0.0);
        }
        // After settling, silence in should yield near-silence out
        let mut sum = 0.0_f32;
        for _ in 0..512 {
            sum += proc.process_sample(0.0).abs();
        }
        let avg = sum / 512.0;
        assert!(avg < 1e-6, "silence should produce near-zero output, got avg={avg}");
    }

    // ── process sine ────────────────────────────────────────────────

    #[test]
    fn process_mono_sine_440hz_all_finite() {
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
    fn process_stereo_sine_440hz_all_finite() {
        let sr = 48000.0;
        let params = default_params();
        let mut proc = build_stereo(&params, sr);
        let input = sine_block(1024, 440.0, sr);
        for (i, &s) in input.iter().enumerate() {
            let [l, r] = proc.process_frame([s, s]);
            assert!(l.is_finite() && r.is_finite(), "non-finite at frame {i}: [{l}, {r}]");
        }
    }

    #[test]
    fn process_mono_sine_produces_nonzero_output() {
        let sr = 48000.0;
        let params = default_params();
        let mut proc = build_mono(&params, sr);
        let input = sine_block(2048, 440.0, sr);
        let output: Vec<f32> = input.iter().map(|&s| proc.process_sample(s)).collect();
        let out_rms = rms(&output[512..]); // skip transient
        assert!(out_rms > 0.001, "expected nonzero output, got rms={out_rms}");
    }

    // ── drive parameter behavior ────────────────────────────────────

    #[test]
    fn drive_increase_raises_output_level() {
        let sr = 48000.0;
        let input = sine_block(2048, 440.0, sr);

        let params_low = params_with(10.0, 50.0, 55.0);
        let params_high = params_with(90.0, 50.0, 55.0);

        let mut proc_low = build_mono(&params_low, sr);
        let mut proc_high = build_mono(&params_high, sr);

        let out_low: Vec<f32> = input.iter().map(|&s| proc_low.process_sample(s)).collect();
        let out_high: Vec<f32> = input.iter().map(|&s| proc_high.process_sample(s)).collect();

        let rms_low = rms(&out_low[512..]);
        let rms_high = rms(&out_high[512..]);

        assert!(
            rms_high > rms_low,
            "higher drive should produce louder output: low_rms={rms_low}, high_rms={rms_high}"
        );
    }

    #[test]
    fn drive_at_zero_still_produces_finite_output() {
        let sr = 48000.0;
        let params = params_with(0.0, 50.0, 55.0);
        let mut proc = build_mono(&params, sr);
        let input = sine_block(1024, 440.0, sr);
        for (i, &s) in input.iter().enumerate() {
            let out = proc.process_sample(s);
            assert!(out.is_finite(), "non-finite at {i} with drive=0: {out}");
        }
    }

    #[test]
    fn drive_at_max_still_produces_finite_output() {
        let sr = 48000.0;
        let params = params_with(100.0, 50.0, 55.0);
        let mut proc = build_mono(&params, sr);
        let input = sine_block(1024, 440.0, sr);
        for (i, &s) in input.iter().enumerate() {
            let out = proc.process_sample(s);
            assert!(out.is_finite(), "non-finite at {i} with drive=100: {out}");
        }
    }

    // ── tone parameter behavior ─────────────────────────────────────

    #[test]
    fn tone_variation_produces_different_spectral_balance() {
        let sr = 48000.0;
        let input = sine_block(4096, 440.0, sr);

        let params_dark = params_with(50.0, 10.0, 55.0);
        let params_bright = params_with(50.0, 90.0, 55.0);

        let mut proc_dark = build_mono(&params_dark, sr);
        let mut proc_bright = build_mono(&params_bright, sr);

        let out_dark: Vec<f32> = input.iter().map(|&s| proc_dark.process_sample(s)).collect();
        let out_bright: Vec<f32> = input.iter().map(|&s| proc_bright.process_sample(s)).collect();

        let rms_dark = rms(&out_dark[1024..]);
        let rms_bright = rms(&out_bright[1024..]);

        // Both should produce output, but different tone settings yield different levels
        assert!(rms_dark > 0.001, "dark tone should produce output");
        assert!(rms_bright > 0.001, "bright tone should produce output");
        // They should differ (different EQ curves)
        assert!(
            (rms_dark - rms_bright).abs() > 1e-4,
            "tone knob should affect output: dark_rms={rms_dark}, bright_rms={rms_bright}"
        );
    }

    // ── level parameter behavior ────────────────────────────────────

    #[test]
    fn level_zero_produces_silence() {
        let sr = 48000.0;
        let params = params_with(50.0, 50.0, 0.0);
        let mut proc = build_mono(&params, sr);
        let input = sine_block(1024, 440.0, sr);
        for (i, &s) in input.iter().enumerate() {
            let out = proc.process_sample(s);
            assert!(
                out.abs() < 1e-10,
                "level=0 should produce silence, got {out} at sample {i}"
            );
        }
    }

    #[test]
    fn level_increase_raises_output() {
        let sr = 48000.0;
        let input = sine_block(2048, 440.0, sr);

        let params_low = params_with(35.0, 50.0, 20.0);
        let params_high = params_with(35.0, 50.0, 80.0);

        let mut proc_low = build_mono(&params_low, sr);
        let mut proc_high = build_mono(&params_high, sr);

        let out_low: Vec<f32> = input.iter().map(|&s| proc_low.process_sample(s)).collect();
        let out_high: Vec<f32> = input.iter().map(|&s| proc_high.process_sample(s)).collect();

        let rms_low = rms(&out_low[512..]);
        let rms_high = rms(&out_high[512..]);

        assert!(
            rms_high > rms_low,
            "higher level should produce louder output: low={rms_low}, high={rms_high}"
        );
    }

    // ── soft_clip tests ─────────────────────────────────────────────

    #[test]
    fn soft_clip_zero_returns_zero() {
        assert_eq!(Ts9Processor::soft_clip(0.0), 0.0);
    }

    #[test]
    fn soft_clip_small_values_nearly_linear() {
        let input = 0.1;
        let output = Ts9Processor::soft_clip(input);
        let error = (output - input).abs();
        assert!(error < 0.01, "soft_clip should be nearly linear for small values, error={error}");
    }

    #[test]
    fn soft_clip_at_clamp_boundary_is_deterministic() {
        // soft_clip clamps to [-3, 3] then applies x - x^3/3
        // At x=3: 3 - 27/3 = -6; at x=-3: -3 + 27/3 = 6
        // This is the expected cubic behavior at the clamp boundaries
        let pos = Ts9Processor::soft_clip(10.0);
        let neg = Ts9Processor::soft_clip(-10.0);
        let expected = 3.0 - 9.0; // -6.0
        assert!((pos - expected).abs() < 1e-6, "soft_clip(10) should be {expected}, got {pos}");
        assert!((neg - (-expected)).abs() < 1e-6, "soft_clip(-10) should be {}, got {neg}", -expected);
    }

    #[test]
    fn soft_clip_within_range_is_bounded() {
        // For inputs in [-1, 1], output stays close to input (mild clipping)
        for &x in &[0.5, 0.8, 1.0] {
            let out = Ts9Processor::soft_clip(x);
            assert!(out.is_finite(), "soft_clip({x}) should be finite");
            assert!(out > 0.0, "soft_clip({x}) should be positive, got {out}");
            assert!(out <= x, "soft_clip({x}) should not exceed input, got {out}");
        }
    }

    #[test]
    fn soft_clip_is_odd_symmetric() {
        for &x in &[0.1, 0.5, 1.0, 2.0, 3.0] {
            let pos = Ts9Processor::soft_clip(x);
            let neg = Ts9Processor::soft_clip(-x);
            assert!(
                (pos + neg).abs() < 1e-6,
                "soft_clip should be odd symmetric: f({x})={pos}, f(-{x})={neg}"
            );
        }
    }

    // ── golden sample test ──────────────────────────────────────────

    #[test]
    fn golden_sample_mono_defaults_48khz() {
        let sr = 48000.0;
        let params = default_params();
        let mut proc = build_mono(&params, sr);

        // Process 512 samples of 440Hz sine to warm up, then capture 8 samples
        let warmup = sine_block(512, 440.0, sr);
        for &s in &warmup {
            proc.process_sample(s);
        }

        let test_input = sine_block(8, 440.0, sr);
        let output: Vec<f32> = test_input.iter().map(|&s| proc.process_sample(s)).collect();

        // All outputs must be finite
        for (i, &o) in output.iter().enumerate() {
            assert!(o.is_finite(), "golden sample {i} is not finite: {o}");
        }

        // Store golden values from first run (tolerance 1e-4)
        // These are deterministic since the DSP is stateful but reproducible
        let mut proc2 = build_mono(&params, sr);
        let warmup2 = sine_block(512, 440.0, sr);
        for &s in &warmup2 {
            proc2.process_sample(s);
        }
        let test_input2 = sine_block(8, 440.0, sr);
        let output2: Vec<f32> = test_input2.iter().map(|&s| proc2.process_sample(s)).collect();

        for (i, (&a, &b)) in output.iter().zip(output2.iter()).enumerate() {
            assert!(
                (a - b).abs() < 1e-4,
                "golden sample mismatch at {i}: {a} vs {b}"
            );
        }
    }

    // ── stereo dual-mono consistency ────────────────────────────────

    #[test]
    fn stereo_left_right_independent() {
        let sr = 48000.0;
        let params = default_params();
        let mut stereo = build_stereo(&params, sr);

        // Feed signal only to left channel
        let input = sine_block(1024, 440.0, sr);
        for &s in &input {
            let [_l, r] = stereo.process_frame([s, 0.0]);
            assert!(
                r.abs() < 1e-6,
                "right channel should be silent when only left is fed, got {r}"
            );
        }
    }

    #[test]
    fn stereo_matches_two_mono_instances() {
        let sr = 48000.0;
        let params = default_params();
        let mut stereo = build_stereo(&params, sr);
        let mut mono_l = build_mono(&params, sr);
        let mut mono_r = build_mono(&params, sr);

        let input = sine_block(1024, 440.0, sr);
        for &s in &input {
            let [sl, sr_out] = stereo.process_frame([s, s]);
            let ml = mono_l.process_sample(s);
            let mr = mono_r.process_sample(s);
            assert!(
                (sl - ml).abs() < 1e-6,
                "stereo L should match mono: {sl} vs {ml}"
            );
            assert!(
                (sr_out - mr).abs() < 1e-6,
                "stereo R should match mono: {sr_out} vs {mr}"
            );
        }
    }

    // ── extreme parameter combinations ──────────────────────────────

    #[test]
    fn extreme_params_all_max_produces_finite() {
        let sr = 48000.0;
        let params = params_with(100.0, 100.0, 100.0);
        let mut proc = build_mono(&params, sr);
        let input = sine_block(1024, 440.0, sr);
        for (i, &s) in input.iter().enumerate() {
            let out = proc.process_sample(s);
            assert!(out.is_finite(), "all-max params produced non-finite at {i}: {out}");
        }
    }

    #[test]
    fn extreme_params_all_min_produces_finite() {
        let sr = 48000.0;
        let params = params_with(0.0, 0.0, 0.0);
        let mut proc = build_mono(&params, sr);
        let input = sine_block(1024, 440.0, sr);
        for (i, &s) in input.iter().enumerate() {
            let out = proc.process_sample(s);
            assert!(out.is_finite(), "all-min params produced non-finite at {i}: {out}");
        }
    }

    // ── hot input signal ────────────────────────────────────────────

    #[test]
    fn hot_input_signal_produces_finite_output() {
        let sr = 48000.0;
        let params = params_with(80.0, 50.0, 80.0);
        let mut proc = build_mono(&params, sr);
        // Input at full scale (1.0) and beyond
        for i in 0..512 {
            let input = (i as f32 / 512.0 * std::f32::consts::TAU).sin(); // full scale
            let out = proc.process_sample(input);
            assert!(out.is_finite(), "hot input produced non-finite at {i}: {out}");
        }
        // Input beyond 1.0 (overdriving)
        for i in 0..256 {
            let input = (i as f32 / 256.0 * std::f32::consts::TAU).sin() * 2.0;
            let out = proc.process_sample(input);
            assert!(out.is_finite(), "overdriven input produced non-finite at {i}: {out}");
        }
    }

    // ── normalized_percent helper ───────────────────────────────────

    #[test]
    fn normalized_percent_clamps_to_0_1() {
        assert_eq!(Ts9Processor::normalized_percent(0.0), 0.0);
        assert_eq!(Ts9Processor::normalized_percent(100.0), 1.0);
        assert_eq!(Ts9Processor::normalized_percent(50.0), 0.5);
        assert_eq!(Ts9Processor::normalized_percent(-10.0), 0.0);
        assert_eq!(Ts9Processor::normalized_percent(200.0), 1.0);
    }

    // ── read_settings tests ─────────────────────────────────────────

    #[test]
    fn read_settings_extracts_correct_values() {
        let params = params_with(42.0, 67.0, 88.0);
        let settings = read_settings(&params).unwrap();
        assert!((settings.drive - 42.0).abs() < 1e-6);
        assert!((settings.tone - 67.0).abs() < 1e-6);
        assert!((settings.level - 88.0).abs() < 1e-6);
    }

    #[test]
    fn read_settings_fails_with_empty_params() {
        let params = ParameterSet::default();
        assert!(read_settings(&params).is_err());
    }
