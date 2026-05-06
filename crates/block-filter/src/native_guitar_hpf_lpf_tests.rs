    use super::*;
    use block_core::MonoProcessor;

    fn sine_rms(freq_hz: f32, sample_rate: f32, low_cut: f32, high_cut: f32) -> f32 {
        let mut eq = GuitarHpfLpf::new(low_cut, high_cut, sample_rate);
        let samples: Vec<f32> = (0..4096)
            .map(|i| (2.0 * std::f32::consts::PI * freq_hz * i as f32 / sample_rate).sin())
            .collect();
        for &s in &samples[..2048] {
            eq.process_sample(s);
        }
        let out: Vec<f32> = samples[2048..].iter().map(|&s| eq.process_sample(s)).collect();
        (out.iter().map(|x| x * x).sum::<f32>() / out.len() as f32).sqrt()
    }

    fn db(ratio: f32) -> f32 {
        20.0 * ratio.log10()
    }

    #[test]
    fn hpf_attenuates_50hz() {
        let sr = 48000.0;
        let rms_filtered = sine_rms(50.0, sr, 100.0, 0.0);
        let rms_passthrough = sine_rms(50.0, sr, 0.0, 0.0);
        let attenuation_db = db(rms_filtered / rms_passthrough);
        assert!(
            attenuation_db <= -20.0,
            "Expected >= 20dB attenuation at 50Hz with HPF@100Hz, got {:.2} dB",
            attenuation_db
        );
    }

    #[test]
    fn lpf_attenuates_15khz() {
        let sr = 48000.0;
        let rms_filtered = sine_rms(15000.0, sr, 0.0, 100.0);
        let rms_passthrough = sine_rms(15000.0, sr, 0.0, 0.0);
        let attenuation_db = db(rms_filtered / rms_passthrough);
        assert!(
            attenuation_db <= -20.0,
            "Expected >= 20dB attenuation at 15kHz with LPF@7kHz, got {:.2} dB",
            attenuation_db
        );
    }

    #[test]
    fn passthrough_at_zero_percent() {
        let sr = 48000.0;
        let rms_filtered = sine_rms(1000.0, sr, 0.0, 0.0);
        let rms_passthrough = 1.0_f32 / 2.0_f32.sqrt();
        let attenuation_db = db(rms_filtered / rms_passthrough);
        assert!(
            attenuation_db.abs() < 0.1,
            "Expected < 0.1dB change at 1kHz with no filtering, got {:.4} dB",
            attenuation_db
        );
    }

    #[test]
    fn process_sample_silence_output_finite() {
        let mut eq = GuitarHpfLpf::new(50.0, 50.0, 44_100.0);
        for i in 0..1024 {
            let out = eq.process_sample(0.0);
            assert!(out.is_finite(), "output not finite at sample {i}");
        }
    }

    #[test]
    fn process_sample_sine_output_finite_and_nonzero() {
        let mut eq = GuitarHpfLpf::new(50.0, 50.0, 44_100.0);
        let sr = 44_100.0_f32;
        let mut any_nonzero = false;
        for i in 0..1024 {
            let input = (2.0 * std::f32::consts::PI * 440.0 * i as f32 / sr).sin();
            let out = eq.process_sample(input);
            assert!(out.is_finite(), "output not finite at sample {i}");
            if out.abs() > 1e-10 {
                any_nonzero = true;
            }
        }
        assert!(any_nonzero, "expected non-zero output for sine input");
    }

    #[test]
    fn process_block_all_finite() {
        let mut eq = GuitarHpfLpf::new(50.0, 50.0, 44_100.0);
        let sr = 44_100.0_f32;
        let mut buffer: Vec<f32> = (0..1024)
            .map(|i| (2.0 * std::f32::consts::PI * 440.0 * i as f32 / sr).sin())
            .collect();
        eq.process_block(&mut buffer);
        for (i, s) in buffer.iter().enumerate() {
            assert!(s.is_finite(), "output not finite at frame {i}");
        }
    }

    #[test]
    fn mid_frequencies_pass_at_full_percent() {
        let sr = 48000.0;
        let rms_filtered = sine_rms(1000.0, sr, 100.0, 100.0);
        let rms_passthrough = sine_rms(1000.0, sr, 0.0, 0.0);
        let attenuation_db = db(rms_filtered / rms_passthrough);
        assert!(
            attenuation_db.abs() < 0.1,
            "Expected < 0.1dB change at 1kHz inside passband (HPF@100Hz, LPF@7kHz), got {:.4} dB",
            attenuation_db
        );
    }
