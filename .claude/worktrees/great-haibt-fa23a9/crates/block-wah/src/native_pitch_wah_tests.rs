    use super::*;
    use std::f32::consts::TAU;

    fn p() -> PitchWahParams { PitchWahParams { sensitivity: 1.0, range: 0.8, q: 4.0, mix: 1.0 } }

    #[test]
    fn silence_in_silence_out() {
        let mut w = PitchWah::new(p(), 44_100.0);
        for _ in 0..2048 {
            let out = w.process_sample(0.0);
            assert!(out.abs() < 1e-20, "silence: {out}");
        }
    }

    #[test]
    fn sine_input_finite() {
        let mut w = PitchWah::new(p(), 44_100.0);
        let sr = 44_100.0_f32;
        for i in 0..8192 {
            let x = (TAU * 440.0 * i as f32 / sr).sin();
            let y = w.process_sample(x);
            assert!(y.is_finite(), "non-finite at {i}");
        }
    }

    #[test]
    fn dry_mix_passes_input_through() {
        let mut w = PitchWah::new(PitchWahParams { sensitivity: 1.0, range: 0.8, q: 4.0, mix: 0.0 }, 44_100.0);
        let sr = 44_100.0_f32;
        for i in 0..1024 {
            let x = (TAU * 440.0 * i as f32 / sr).sin();
            let y = w.process_sample(x);
            assert!((y - x).abs() < 1e-6, "mix=0 should be dry");
        }
    }

    #[test]
    fn output_bounded() {
        let mut w = PitchWah::new(p(), 44_100.0);
        for i in 0..44_100 {
            let x = ((i as f32 * 0.1).sin()).clamp(-1.0, 1.0);
            let y = w.process_sample(x);
            assert!(y.abs() < 10.0, "diverged: {y} at {i}");
        }
    }
