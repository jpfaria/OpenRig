    use super::*;

    #[test]
    fn silence_in_silence_out() {
        let mut fs = FrequencyShifter::new(50.0, 1.0, 44_100.0);
        for _ in 0..2048 {
            let out = fs.process_sample(0.0);
            assert_eq!(out, 0.0, "shifter of silence must be silence");
        }
    }

    #[test]
    fn sine_input_output_finite() {
        let mut fs = FrequencyShifter::new(50.0, 1.0, 44_100.0);
        let sr = 44_100.0_f32;
        for i in 0..4096 {
            let input = (TAU * 440.0 * i as f32 / sr).sin();
            let out = fs.process_sample(input);
            assert!(out.is_finite(), "non-finite at {i}");
        }
    }

    #[test]
    fn dry_mix_passes_real_path() {
        // mix=0 returns the H1 (real) leg directly.
        let mut fs = FrequencyShifter::new(50.0, 0.0, 44_100.0);
        let mut h_ref = HilbertIir::new();
        let sr = 44_100.0_f32;
        for i in 0..2048 {
            let input = (TAU * 440.0 * i as f32 / sr).sin();
            let [expected_real, _] = h_ref.process(input);
            let out = fs.process_sample(input);
            assert!(
                (out - expected_real).abs() < 1e-6,
                "mix=0 should pass real leg unchanged at {i}: got {out} want {expected_real}"
            );
        }
    }

    #[test]
    fn zero_shift_close_to_real_path() {
        // shift=0 → e^(j·0) = 1 → wet = real. With mix=1, output =
        // real_leg, identical to the dry path with mix=0.
        let mut fs = FrequencyShifter::new(0.0, 1.0, 44_100.0);
        let mut h_ref = HilbertIir::new();
        let sr = 44_100.0_f32;
        for i in 0..2048 {
            let input = (TAU * 440.0 * i as f32 / sr).sin();
            let [expected_real, _] = h_ref.process(input);
            let out = fs.process_sample(input);
            assert!(
                (out - expected_real).abs() < 1e-5,
                "shift=0 should ≈ real leg at {i}: got {out} want {expected_real}"
            );
        }
    }

    #[test]
    fn output_bounded_for_unit_input() {
        let mut fs = FrequencyShifter::new(100.0, 1.0, 44_100.0);
        for _ in 0..44_100 {
            let out = fs.process_sample(1.0);
            assert!(out.abs() < 5.0, "shifter output {out} too large");
        }
    }
