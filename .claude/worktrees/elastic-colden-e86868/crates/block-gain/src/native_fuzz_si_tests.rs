    use super::*;

    fn defaults() -> Settings { Settings { fuzz: 60.0, tone: 50.0, level: 50.0 } }

    #[test]
    fn shape_is_bounded() {
        for x in [-100.0_f32, -10.0, -1.0, 0.0, 1.0, 10.0, 100.0] {
            let y = FuzzProcessor::si_shape(x);
            assert!(y.abs() <= 1.1, "si_shape({x}) = {y}");
        }
    }

    #[test]
    fn shape_is_asymmetric() {
        // Positive clips at 0.85, negative at 1.0 → at very high drive,
        // the magnitudes should differ.
        let pos = FuzzProcessor::si_shape(50.0).abs();
        let neg = FuzzProcessor::si_shape(-50.0).abs();
        assert!((pos - neg).abs() > 0.05, "expected asymmetry: |+|={pos} |-|={neg}");
    }

    #[test]
    fn silence_input_produces_silence() {
        let mut p = FuzzProcessor::new(defaults(), 44_100.0);
        for _ in 0..2048 {
            assert!(p.process_sample(0.0).abs() < 1e-3);
        }
    }

    #[test]
    fn sine_input_finite_and_nonzero() {
        let mut p = FuzzProcessor::new(defaults(), 44_100.0);
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

    #[test]
    fn dc_input_is_blocked() {
        let mut p = FuzzProcessor::new(defaults(), 44_100.0);
        for _ in 0..8192 { let _ = p.process_sample(0.5); }
        let mut peak = 0.0_f32;
        for _ in 0..2048 { peak = peak.max(p.process_sample(0.5).abs()); }
        assert!(peak < 0.05, "DC was not blocked (peak {peak})");
    }
