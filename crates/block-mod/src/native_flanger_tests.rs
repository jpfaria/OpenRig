    use super::*;
    use std::f32::consts::TAU;

    #[test]
    fn silence_in_silence_out() {
        let mut f = Flanger::new(0.4, 0.6, 0.5, 0.5, 44_100.0);
        for _ in 0..4096 {
            let out = f.process_sample(0.0);
            assert!(out.abs() < 1e-20, "flanger of silence: {out}");
        }
    }

    #[test]
    fn sine_input_output_finite() {
        let mut f = Flanger::new(0.4, 0.6, 0.5, 0.5, 44_100.0);
        let sr = 44_100.0_f32;
        for i in 0..4096 {
            let input = (TAU * 440.0 * i as f32 / sr).sin();
            let out = f.process_sample(input);
            assert!(out.is_finite(), "non-finite at {i}");
        }
    }

    #[test]
    fn dry_mix_passes_input_through() {
        let mut f = Flanger::new(0.4, 0.6, 0.5, 0.0, 44_100.0);
        let sr = 44_100.0_f32;
        for i in 0..1024 {
            let input = (TAU * 440.0 * i as f32 / sr).sin();
            let out = f.process_sample(input);
            assert!((out - input).abs() < 1e-6, "mix=0 should be dry");
        }
    }

    #[test]
    fn output_bounded_with_clamped_feedback() {
        let mut f = Flanger::new(0.4, 1.0, 1.5, 1.0, 44_100.0);
        for i in 0..44_100 {
            let input = ((i as f32 * 0.1).sin()).clamp(-1.0, 1.0);
            let out = f.process_sample(input);
            assert!(out.abs() < 50.0, "diverged: {out} at {i}");
        }
    }
