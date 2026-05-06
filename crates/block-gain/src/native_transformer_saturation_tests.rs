    use super::*;

    fn defaults() -> Settings {
        Settings { drive: 40.0, color: 60.0, warmth: 50.0, level: 50.0 }
    }

    #[test]
    fn shape_silence_in_silence_out() {
        for c in [0.0_f32, 0.3, 0.7, 1.0] {
            assert!(TransformerProcessor::shape(0.0, c).abs() < 1e-9);
        }
    }

    #[test]
    fn shape_is_symmetric_around_zero() {
        // Transformer saturation should be odd-symmetric: shape(-x) == -shape(x).
        for c in [0.0_f32, 0.5, 1.0] {
            for x in [0.1_f32, 0.5, 1.0, 5.0] {
                let p = TransformerProcessor::shape(x, c);
                let n = TransformerProcessor::shape(-x, c);
                assert!((p + n).abs() < 1e-6, "shape({x}, {c})={p} vs shape(-{x},{c})={n}");
            }
        }
    }

    #[test]
    fn shape_is_bounded() {
        for c in [0.0_f32, 1.0] {
            for x in [-100.0_f32, -10.0, 10.0, 100.0] {
                let y = TransformerProcessor::shape(x, c);
                // tanh + 0.3 * tanh^3 → max output ~1.3 at extreme drive.
                assert!(y.abs() <= 1.5, "shape({x}, {c}) = {y}");
            }
        }
    }

    #[test]
    fn silence_input_produces_silence() {
        let mut p = TransformerProcessor::new(defaults(), 44_100.0);
        for _ in 0..2048 {
            assert!(p.process_sample(0.0).abs() < 1e-3);
        }
    }

    #[test]
    fn sine_input_finite_and_nonzero() {
        let mut p = TransformerProcessor::new(defaults(), 44_100.0);
        let sr = 44_100.0_f32;
        let mut any_nonzero = false;
        for i in 0..2048 {
            let s = (2.0 * std::f32::consts::PI * 440.0 * i as f32 / sr).sin() * 0.4;
            let out = p.process_sample(s);
            assert!(out.is_finite());
            if out.abs() > 1e-6 { any_nonzero = true; }
        }
        assert!(any_nonzero);
    }
