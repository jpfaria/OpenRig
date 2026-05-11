    use super::*;
    use std::f32::consts::TAU;

    fn p() -> TalkBoxParams {
        TalkBoxParams { vowel: 0.0, intensity: 8.0, mix: 1.0 }
    }

    #[test]
    fn silence_in_silence_out() {
        let mut t = TalkBox::new(p(), 44_100.0);
        for _ in 0..2048 {
            let out = t.process_sample(0.0);
            assert!(out.abs() < 1e-20, "talk-box silence: {out}");
        }
    }

    #[test]
    fn sine_input_finite() {
        let mut t = TalkBox::new(p(), 44_100.0);
        let sr = 44_100.0_f32;
        for i in 0..4096 {
            let x = (TAU * 440.0 * i as f32 / sr).sin();
            let y = t.process_sample(x);
            assert!(y.is_finite(), "non-finite at {i}");
        }
    }

    #[test]
    fn vowel_interpolation_endpoints_match_table() {
        assert_eq!(interpolate_vowel(0.0), FORMANTS[0]);
        assert_eq!(interpolate_vowel(1.0), FORMANTS[1]);
        assert_eq!(interpolate_vowel(4.0), FORMANTS[4]);
    }

    #[test]
    fn dry_mix_passes_input_through() {
        let mut t = TalkBox::new(
            TalkBoxParams { vowel: 0.0, intensity: 8.0, mix: 0.0 },
            44_100.0,
        );
        let sr = 44_100.0_f32;
        for i in 0..1024 {
            let x = (TAU * 440.0 * i as f32 / sr).sin();
            let y = t.process_sample(x);
            assert!((y - x).abs() < 1e-6, "mix=0 should be dry");
        }
    }
