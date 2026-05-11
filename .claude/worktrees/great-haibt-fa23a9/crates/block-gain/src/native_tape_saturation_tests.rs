    use super::*;

    fn defaults() -> Settings {
        Settings { drive: 40.0, hysteresis: 50.0, wow: 20.0, warmth: 60.0, level: 50.0 }
    }

    #[test]
    fn shape_silence_in_silence_out() {
        for h in [0.0_f32, 0.5, 1.0] {
            assert!(TapeProcessor::shape(0.0, 0.0, h).abs() < 1e-9);
        }
    }

    #[test]
    fn shape_with_memory_offsets_curve() {
        // With memory > 0 and hyst > 0, shape(x) shifts by (mem * hyst * 0.3).
        let no_mem = TapeProcessor::shape(0.5, 0.0, 1.0);
        let with_mem = TapeProcessor::shape(0.5, 0.5, 1.0);
        assert!((no_mem - with_mem).abs() > 0.01, "memory should shift the curve");
    }

    #[test]
    fn shape_is_bounded() {
        for h in [0.0_f32, 1.0] {
            for x in [-100.0_f32, -10.0, 10.0, 100.0] {
                let y = TapeProcessor::shape(x, 0.5, h);
                assert!(y.abs() <= 1.05, "shape({x}, 0.5, {h}) = {y}");
            }
        }
    }

    #[test]
    fn silence_input_produces_silence() {
        let mut p = TapeProcessor::new(defaults(), 44_100.0);
        for _ in 0..2048 {
            assert!(p.process_sample(0.0).abs() < 1e-3);
        }
    }

    #[test]
    fn sine_input_finite_and_nonzero() {
        let mut p = TapeProcessor::new(defaults(), 44_100.0);
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

    #[test]
    fn dc_input_is_blocked() {
        let mut p = TapeProcessor::new(defaults(), 44_100.0);
        for _ in 0..8192 { let _ = p.process_sample(0.5); }
        let mut peak = 0.0_f32;
        for _ in 0..2048 { peak = peak.max(p.process_sample(0.5).abs()); }
        assert!(peak < 0.05, "DC was not blocked (peak {peak})");
    }

    #[test]
    fn wow_at_zero_does_not_modulate() {
        let mut p = TapeProcessor::new(
            Settings { drive: 0.0, hysteresis: 0.0, wow: 0.0, warmth: 100.0, level: 50.0 },
            44_100.0,
        );
        let sr = 44_100.0_f32;
        // Drive feeds a steady sine; with wow=0 there should be no
        // amplitude/pitch modulation across periods. Just check it
        // remains finite over a long window — modulation tests would
        // need an FFT to verify quantitatively.
        for i in 0..44_100 {
            let s = (2.0 * std::f32::consts::PI * 440.0 * i as f32 / sr).sin() * 0.2;
            assert!(p.process_sample(s).is_finite());
        }
    }
