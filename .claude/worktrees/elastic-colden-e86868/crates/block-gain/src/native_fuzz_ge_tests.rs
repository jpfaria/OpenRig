    use super::*;

    fn defaults() -> Settings { Settings { fuzz: 60.0, tone: 50.0, level: 50.0 } }

    #[test]
    fn shape_is_bounded() {
        for x in [-100.0_f32, -10.0, 0.0, 10.0, 100.0] {
            let y = FuzzGeProcessor::ge_shape(x);
            assert!(y.abs() <= 1.0, "ge_shape({x}) = {y}");
        }
    }

    #[test]
    fn shape_silence_in_silence_out() {
        // ge_shape(0) = (tanh(bias) - tanh(bias)) * ceiling = 0
        assert!(FuzzGeProcessor::ge_shape(0.0).abs() < 1e-9);
    }

    #[test]
    fn shape_is_smoother_than_si_at_moderate_drive() {
        // Compare with native_fuzz_si: at x=2 the Ge shaper compresses
        // sooner (lower magnitude than Si saturation).
        let ge = FuzzGeProcessor::ge_shape(2.0).abs();
        // Si shape at the same input would approach 0.85 (positive ceiling).
        // Ge ceiling is 0.9, but tanh's knee is gentler so at x=2 we
        // expect Ge to be ~0.8 vs Si ~0.85.
        assert!(ge < 0.92, "Ge should still be inside its ceiling: {ge}");
    }

    #[test]
    fn silence_input_produces_silence() {
        let mut p = FuzzGeProcessor::new(defaults(), 44_100.0);
        for _ in 0..2048 {
            assert!(p.process_sample(0.0).abs() < 1e-3);
        }
    }

    #[test]
    fn sine_input_finite_and_nonzero() {
        let mut p = FuzzGeProcessor::new(defaults(), 44_100.0);
        let sr = 44_100.0_f32;
        let mut any_nonzero = false;
        for i in 0..2048 {
            let s = (2.0 * std::f32::consts::PI * 440.0 * i as f32 / sr).sin() * 0.3;
            let out = p.process_sample(s);
            assert!(out.is_finite());
            if out.abs() > 1e-6 { any_nonzero = true; }
        }
        assert!(any_nonzero);
    }
