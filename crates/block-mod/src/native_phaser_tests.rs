    use super::*;
    use std::f32::consts::TAU;

    #[test]
    fn silence_in_silence_out() {
        let mut ph = Phaser::new(0.5, 0.7, 0.5, 0.5, 44_100.0);
        for _ in 0..4096 {
            let out = ph.process_sample(0.0);
            assert!(out.abs() < 1e-20, "phaser of silence: {out}");
        }
    }

    #[test]
    fn sine_input_output_finite() {
        let mut ph = Phaser::new(0.5, 0.7, 0.5, 0.5, 44_100.0);
        let sr = 44_100.0_f32;
        for i in 0..4096 {
            let input = (TAU * 440.0 * i as f32 / sr).sin();
            let out = ph.process_sample(input);
            assert!(out.is_finite(), "non-finite at {i}");
        }
    }

    #[test]
    fn dry_mix_passes_input_through() {
        let mut ph = Phaser::new(0.5, 0.7, 0.5, 0.0, 44_100.0);
        let sr = 44_100.0_f32;
        for i in 0..1024 {
            let input = (TAU * 440.0 * i as f32 / sr).sin();
            let out = ph.process_sample(input);
            assert!((out - input).abs() < 1e-6, "mix=0 should be dry");
        }
    }

    #[test]
    fn output_bounded_with_max_feedback() {
        // tanh saturation guarantees feedback bounded.
        let mut ph = Phaser::new(0.5, 1.0, 0.95, 1.0, 44_100.0);
        for i in 0..44_100 {
            let input = ((i as f32 * 0.1).sin()).clamp(-1.0, 1.0);
            let out = ph.process_sample(input);
            assert!(out.abs() < 5.0, "diverged: {out} at {i}");
        }
    }

    #[test]
    fn shape_sweep_fixed_points() {
        // Endpoints should still be 0 and 1 regardless of skew strength.
        let ph = Phaser::new(0.5, 0.7, 0.5, 0.5, 44_100.0);
        assert!((ph.shape_sweep(0.0)).abs() < 0.01);
        assert!((ph.shape_sweep(1.0) - 1.0).abs() < 0.01);
        let mid = ph.shape_sweep(0.5);
        assert!((mid - 0.5).abs() < 0.05, "mid: {mid}");
    }
