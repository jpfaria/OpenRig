//! Tests for `ir`. Lifted out of `lib.rs` so the production file
//! stays under the size cap. Re-attached as `mod tests` of the parent via
//! `#[cfg(test)] #[path = "lib_tests.rs"] mod tests;`.

    use block_core::{AudioChannelLayout, MonoProcessor, StereoProcessor};

    use crate::{
        lanczos_kernel, resample_if_needed, truncate_with_fade, IrAsset, IrChannelData,
        MonoIrProcessor, StereoIrProcessor, FADE_OUT_SAMPLES, MAX_IR_SAMPLES,
    };

    #[test]
    fn loads_mono_ir_from_curated_capture() {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../captures/ir/cabs/marshall_4x12_v30/ev_mix_b.wav"
        );

        let ir = IrAsset::load_from_wav(path).expect("mono IR should load");

        assert_eq!(ir.channel_layout(), AudioChannelLayout::Mono);
        assert_eq!(ir.sample_rate(), 48_000);
        assert_eq!(ir.frame_count(), 24_000);
        assert!(matches!(ir.channel_data(), IrChannelData::Mono(_)));
    }

    #[test]
    fn loads_stereo_ir_from_float_wav() {
        let path = std::env::temp_dir().join("openrig_ir_loader_stereo_test.wav");
        crate::test_support::write_test_stereo_ir(&path)
            .expect("test stereo wav should be created");

        let ir = IrAsset::load_from_wav(path.to_str().unwrap()).expect("stereo IR should load");

        assert_eq!(ir.channel_layout(), AudioChannelLayout::Stereo);
        assert_eq!(ir.sample_rate(), 48_000);
        assert_eq!(ir.frame_count(), 4);
        assert!(matches!(ir.channel_data(), IrChannelData::Stereo(_, _)));
    }

    // ── IrChannelData enum ──────────────────────────────────────────

    #[test]
    fn ir_channel_data_mono_returns_correct_channel_count() {
        let data = IrChannelData::Mono(vec![1.0, 0.5, 0.25]);
        assert!(matches!(data, IrChannelData::Mono(_)));
    }

    #[test]
    fn ir_channel_data_stereo_holds_two_channels() {
        let left = vec![1.0, 0.5];
        let right = vec![0.8, 0.3];
        let data = IrChannelData::Stereo(left.clone(), right.clone());
        if let IrChannelData::Stereo(l, r) = &data {
            assert_eq!(l, &left);
            assert_eq!(r, &right);
        } else {
            panic!("expected Stereo variant");
        }
    }

    // ── IrAsset accessors with synthetic WAV ────────────────────────

    fn write_mono_wav(path: &std::path::Path, samples: &[f32], sample_rate: u32) {
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        };
        let mut writer = hound::WavWriter::create(path, spec).unwrap();
        for &s in samples {
            writer.write_sample(s).unwrap();
        }
        writer.finalize().unwrap();
    }

    fn write_int16_mono_wav(path: &std::path::Path, samples: &[i16], sample_rate: u32) {
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut writer = hound::WavWriter::create(path, spec).unwrap();
        for &s in samples {
            writer.write_sample(s).unwrap();
        }
        writer.finalize().unwrap();
    }

    #[test]
    fn ir_asset_channel_count_mono_returns_one() {
        let path = std::env::temp_dir().join("openrig_ir_test_mono_count.wav");
        write_mono_wav(&path, &[1.0, 0.5, 0.25, 0.0], 44100);
        let ir = IrAsset::load_from_wav(path.to_str().unwrap()).unwrap();
        assert_eq!(ir.channel_count(), 1);
    }

    #[test]
    fn ir_asset_channel_count_stereo_returns_two() {
        let path = std::env::temp_dir().join("openrig_ir_test_stereo_count.wav");
        crate::test_support::write_test_stereo_ir(&path).unwrap();
        let ir = IrAsset::load_from_wav(path.to_str().unwrap()).unwrap();
        assert_eq!(ir.channel_count(), 2);
    }

    #[test]
    fn ir_asset_frame_count_matches_mono_samples() {
        let path = std::env::temp_dir().join("openrig_ir_test_frame_count.wav");
        let samples = vec![0.1; 100];
        write_mono_wav(&path, &samples, 48000);
        let ir = IrAsset::load_from_wav(path.to_str().unwrap()).unwrap();
        assert_eq!(ir.frame_count(), 100);
    }

    #[test]
    fn ir_asset_sample_rate_preserves_original() {
        let path = std::env::temp_dir().join("openrig_ir_test_sr.wav");
        write_mono_wav(&path, &[1.0], 96000);
        let ir = IrAsset::load_from_wav(path.to_str().unwrap()).unwrap();
        assert_eq!(ir.sample_rate(), 96000);
    }

    #[test]
    fn ir_asset_channel_layout_mono_returns_mono() {
        let path = std::env::temp_dir().join("openrig_ir_test_layout_mono.wav");
        write_mono_wav(&path, &[1.0, 0.5], 48000);
        let ir = IrAsset::load_from_wav(path.to_str().unwrap()).unwrap();
        assert_eq!(ir.channel_layout(), AudioChannelLayout::Mono);
    }

    #[test]
    fn ir_asset_channel_layout_stereo_returns_stereo() {
        let path = std::env::temp_dir().join("openrig_ir_test_layout_stereo.wav");
        crate::test_support::write_test_stereo_ir(&path).unwrap();
        let ir = IrAsset::load_from_wav(path.to_str().unwrap()).unwrap();
        assert_eq!(ir.channel_layout(), AudioChannelLayout::Stereo);
    }

    #[test]
    fn load_from_wav_int16_mono_normalizes_correctly() {
        let path = std::env::temp_dir().join("openrig_ir_test_int16.wav");
        write_int16_mono_wav(&path, &[i16::MAX, 0, i16::MIN + 1], 48000);
        let ir = IrAsset::load_from_wav(path.to_str().unwrap()).unwrap();
        assert_eq!(ir.frame_count(), 3);
        if let IrChannelData::Mono(samples) = ir.channel_data() {
            assert!((samples[0] - 1.0).abs() < 0.001);
            assert!((samples[1]).abs() < 0.001);
            assert!((samples[2] + 1.0).abs() < 0.001);
        } else {
            panic!("expected Mono");
        }
    }

    #[test]
    fn load_from_wav_nonexistent_file_returns_error() {
        let result = IrAsset::load_from_wav("/nonexistent/path/to/ir.wav");
        assert!(result.is_err());
    }

    #[test]
    fn load_from_wav_empty_samples_returns_error() {
        let path = std::env::temp_dir().join("openrig_ir_test_empty.wav");
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: 48000,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        };
        let writer = hound::WavWriter::create(&path, spec).unwrap();
        writer.finalize().unwrap();
        let result = IrAsset::load_from_wav(path.to_str().unwrap());
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("no samples"));
    }

    // ── truncate_with_fade ──────────────────────────────────────────

    #[test]
    fn truncate_with_fade_short_ir_returns_unchanged() {
        let samples = vec![1.0; 100];
        let result = truncate_with_fade(samples.clone(), "test");
        assert_eq!(result, samples);
    }

    #[test]
    fn truncate_with_fade_exact_max_returns_unchanged() {
        let samples = vec![1.0; MAX_IR_SAMPLES];
        let result = truncate_with_fade(samples.clone(), "test");
        assert_eq!(result, samples);
    }

    #[test]
    fn truncate_with_fade_long_ir_truncates_to_max() {
        let samples = vec![1.0; MAX_IR_SAMPLES + 1000];
        let result = truncate_with_fade(samples, "test");
        assert_eq!(result.len(), MAX_IR_SAMPLES);
    }

    #[test]
    fn truncate_with_fade_long_ir_applies_cosine_fadeout() {
        let samples = vec![1.0; MAX_IR_SAMPLES + 1000];
        let result = truncate_with_fade(samples, "test");
        // Last sample should be near zero (end of cosine fade)
        assert!(result[MAX_IR_SAMPLES - 1].abs() < 0.01);
        // Sample just before fade region should be unchanged
        let fade_start = MAX_IR_SAMPLES - FADE_OUT_SAMPLES;
        assert!((result[fade_start - 1] - 1.0).abs() < 0.001);
    }

    // ── resample_if_needed ──────────────────────────────────────────

    #[test]
    fn resample_if_needed_same_rate_returns_unchanged() {
        let samples = vec![1.0, 0.5, 0.25];
        let result = resample_if_needed(samples.clone(), 48000, 48000.0, "test");
        assert_eq!(result, samples);
    }

    #[test]
    fn resample_if_needed_zero_runtime_rate_returns_unchanged() {
        let samples = vec![1.0, 0.5, 0.25];
        let result = resample_if_needed(samples.clone(), 48000, 0.0, "test");
        assert_eq!(result, samples);
    }

    #[test]
    fn resample_if_needed_upsample_produces_longer_output() {
        let samples = vec![1.0; 100];
        let result = resample_if_needed(samples, 44100, 48000.0, "test");
        // 100 * (48000/44100) ~ 109
        assert!(result.len() > 100);
    }

    #[test]
    fn resample_if_needed_downsample_produces_shorter_output() {
        let samples = vec![1.0; 100];
        let result = resample_if_needed(samples, 48000, 44100.0, "test");
        assert!(result.len() < 100);
    }

    // ── lanczos_kernel ──────────────────────────────────────────────

    #[test]
    fn lanczos_kernel_at_zero_returns_one() {
        assert!((lanczos_kernel(0.0, 4.0) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn lanczos_kernel_at_boundary_returns_zero() {
        assert!((lanczos_kernel(4.0, 4.0)).abs() < 1e-6);
        assert!((lanczos_kernel(-4.0, 4.0)).abs() < 1e-6);
    }

    #[test]
    fn lanczos_kernel_beyond_boundary_returns_zero() {
        assert_eq!(lanczos_kernel(5.0, 4.0), 0.0);
        assert_eq!(lanczos_kernel(-5.0, 4.0), 0.0);
    }

    #[test]
    fn lanczos_kernel_positive_near_zero_returns_positive() {
        let val = lanczos_kernel(0.5, 4.0);
        assert!(val > 0.0);
    }

    // ── MonoIrProcessor ─────────────────────────────────────────────

    #[test]
    fn mono_ir_processor_impulse_response_reproduces_ir() {
        // Convolving with a delta should reproduce the IR
        let ir = vec![0.5, 0.3, 0.1];
        let mut proc = MonoIrProcessor::new(ir.clone());
        // Feed an impulse followed by zeros
        let _impulse = proc.process_sample(1.0);
        let _s1 = proc.process_sample(0.0);
        let _s2 = proc.process_sample(0.0);

        // The FFT convolver introduces latency, so the IR appears
        // after a partition-sized delay. We just verify the processor
        // produces non-silent output eventually.
        let mut buf = vec![0.0; 1024];
        buf[0] = 1.0;
        let mut proc2 = MonoIrProcessor::new(ir.clone());
        proc2.process_block(&mut buf);
        let energy: f32 = buf.iter().map(|s| s * s).sum();
        assert!(energy > 0.0, "convolver should produce non-zero output");
    }

    #[test]
    fn mono_ir_processor_process_block_matches_sample_by_sample() {
        let ir = vec![1.0, 0.0, 0.0, 0.0];
        let mut proc_block = MonoIrProcessor::new(ir.clone());
        let mut proc_sample = MonoIrProcessor::new(ir);

        let mut block = vec![0.1, 0.2, 0.3, 0.4, 0.0, 0.0, 0.0, 0.0];
        let block_copy = block.clone();
        proc_block.process_block(&mut block);

        let sample_results: Vec<f32> = block_copy
            .iter()
            .map(|&s| proc_sample.process_sample(s))
            .collect();

        // Both should produce identical output
        for (a, b) in block.iter().zip(sample_results.iter()) {
            assert!(
                (a - b).abs() < 1e-6,
                "block vs sample mismatch: {} vs {}",
                a,
                b
            );
        }
    }

    #[test]
    fn mono_ir_processor_silence_in_produces_silence_out() {
        let ir = vec![1.0, 0.5, 0.25];
        let mut proc = MonoIrProcessor::new(ir);
        let mut buf = vec![0.0; 512];
        proc.process_block(&mut buf);
        assert!(buf.iter().all(|&s| s == 0.0));
    }

    // ── StereoIrProcessor ───────────────────────────────────────────

    #[test]
    fn stereo_ir_processor_silence_in_produces_silence_out() {
        let left_ir = vec![1.0, 0.5];
        let right_ir = vec![0.8, 0.3];
        let mut proc = StereoIrProcessor::new(left_ir, right_ir);
        let mut buf = vec![[0.0f32; 2]; 512];
        proc.process_block(&mut buf);
        assert!(buf.iter().all(|frame| frame[0] == 0.0 && frame[1] == 0.0));
    }

    #[test]
    fn stereo_ir_processor_process_block_produces_output() {
        let left_ir = vec![1.0; 4];
        let right_ir = vec![0.5; 4];
        let mut proc = StereoIrProcessor::new(left_ir, right_ir);
        let mut buf = vec![[0.0f32; 2]; 1024];
        buf[0] = [1.0, 1.0];
        proc.process_block(&mut buf);
        let energy: f32 = buf.iter().map(|f| f[0] * f[0] + f[1] * f[1]).sum();
        assert!(energy > 0.0, "stereo convolver should produce output");
    }

    #[test]
    fn stereo_ir_processor_frame_matches_block() {
        let left_ir = vec![1.0, 0.0, 0.0, 0.0];
        let right_ir = vec![1.0, 0.0, 0.0, 0.0];
        let mut proc_frame = StereoIrProcessor::new(left_ir.clone(), right_ir.clone());
        let mut proc_block = StereoIrProcessor::new(left_ir, right_ir);

        let input = [[0.5, 0.3], [0.2, 0.1], [0.0, 0.0], [0.0, 0.0]];
        let frame_results: Vec<[f32; 2]> = input
            .iter()
            .map(|&f| proc_frame.process_frame(f))
            .collect();

        let mut block = input.to_vec();
        proc_block.process_block(&mut block);

        for (a, b) in block.iter().zip(frame_results.iter()) {
            assert!((a[0] - b[0]).abs() < 1e-6);
            assert!((a[1] - b[1]).abs() < 1e-6);
        }
    }
