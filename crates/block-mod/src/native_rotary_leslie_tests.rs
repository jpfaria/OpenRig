    use super::*;

    #[test]
    fn silence_in_silence_out() {
        let mut l = LeslieRotary::new(1.0, 1.0, 44_100.0);
        for _ in 0..8192 {
            let [a, b] = l.process_stereo(0.0);
            assert!(a.abs() < 1e-20 && b.abs() < 1e-20);
        }
    }

    #[test]
    fn sine_input_output_finite() {
        let mut l = LeslieRotary::new(1.0, 1.0, 44_100.0);
        let sr = 44_100.0_f32;
        for i in 0..8192 {
            let input = (TAU * 440.0 * i as f32 / sr).sin();
            let [a, b] = l.process_stereo(input);
            assert!(a.is_finite() && b.is_finite(), "non-finite at {i}");
        }
    }

    #[test]
    fn dry_mix_passes_input_through() {
        let mut l = LeslieRotary::new(1.0, 0.0, 44_100.0);
        let sr = 44_100.0_f32;
        for i in 0..1024 {
            let input = (TAU * 440.0 * i as f32 / sr).sin();
            let [a, b] = l.process_stereo(input);
            assert!((a - input).abs() < 1e-6, "L mix=0 should be dry");
            assert!((b - input).abs() < 1e-6, "R mix=0 should be dry");
        }
    }

    #[test]
    fn output_bounded_for_unit_input() {
        let mut l = LeslieRotary::new(1.0, 1.0, 44_100.0);
        for _ in 0..44_100 {
            let [a, b] = l.process_stereo(1.0);
            assert!(a.abs() < 5.0 && b.abs() < 5.0, "rotary output too large");
        }
    }
