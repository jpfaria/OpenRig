    use super::*;

    #[test]
    fn process_sample_silence_output_finite() {
        let mut eq = ThreeBandEq::new(0.0, 200.0, 0.0, 1000.0, 1.0, 0.0, 6000.0, 44_100.0);
        for i in 0..1024 {
            let out = MonoProcessor::process_sample(&mut eq, 0.0);
            assert!(out.is_finite(), "output not finite at sample {i}");
        }
    }

    #[test]
    fn process_sample_silence_produces_zero() {
        let mut eq = ThreeBandEq::new(0.0, 200.0, 0.0, 1000.0, 1.0, 0.0, 6000.0, 44_100.0);
        for i in 0..1024 {
            let out = MonoProcessor::process_sample(&mut eq, 0.0);
            assert!(out.abs() < 1e-10, "flat EQ should not add energy to silence at sample {i}");
        }
    }

    #[test]
    fn process_sample_sine_output_finite_and_nonzero() {
        let mut eq = ThreeBandEq::new(0.0, 200.0, 0.0, 1000.0, 1.0, 0.0, 6000.0, 44_100.0);
        let sr = 44_100.0_f32;
        let mut any_nonzero = false;
        for i in 0..1024 {
            let input = (2.0 * std::f32::consts::PI * 440.0 * i as f32 / sr).sin();
            let out = MonoProcessor::process_sample(&mut eq, input);
            assert!(out.is_finite(), "output not finite at sample {i}");
            if out.abs() > 1e-10 {
                any_nonzero = true;
            }
        }
        assert!(any_nonzero, "expected non-zero output for sine input");
    }

    #[test]
    fn process_block_all_finite() {
        let mut eq = ThreeBandEq::new(3.0, 200.0, -2.0, 1000.0, 1.5, 1.0, 6000.0, 44_100.0);
        let sr = 44_100.0_f32;
        let mut buffer: Vec<f32> = (0..1024)
            .map(|i| (2.0 * std::f32::consts::PI * 440.0 * i as f32 / sr).sin())
            .collect();
        MonoProcessor::process_block(&mut eq, &mut buffer);
        for (i, s) in buffer.iter().enumerate() {
            assert!(s.is_finite(), "output not finite at frame {i}");
        }
    }

    #[test]
    fn process_sample_with_boost_increases_energy() {
        let sr = 44_100.0_f32;
        // Flat EQ
        let mut eq_flat = ThreeBandEq::new(0.0, 200.0, 0.0, 1000.0, 1.0, 0.0, 6000.0, sr);
        // Mid boost +12dB
        let mut eq_boost = ThreeBandEq::new(0.0, 200.0, 12.0, 1000.0, 1.0, 0.0, 6000.0, sr);

        let samples: Vec<f32> = (0..4096)
            .map(|i| (2.0 * std::f32::consts::PI * 1000.0 * i as f32 / sr).sin())
            .collect();

        // Warm up
        for &s in &samples[..2048] {
            eq_flat.process_sample(s);
            eq_boost.process_sample(s);
        }

        // Measure RMS
        let rms_flat: f32 = (samples[2048..].iter()
            .map(|&s| { let o = eq_flat.process_sample(s); o * o })
            .sum::<f32>() / 2048.0).sqrt();
        let rms_boost: f32 = (samples[2048..].iter()
            .map(|&s| { let o = eq_boost.process_sample(s); o * o })
            .sum::<f32>() / 2048.0).sqrt();

        assert!(rms_boost > rms_flat * 2.0,
            "boosted EQ should be significantly louder: flat={rms_flat}, boost={rms_boost}");
    }
