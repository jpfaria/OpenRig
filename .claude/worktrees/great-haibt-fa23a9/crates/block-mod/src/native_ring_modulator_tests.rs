    use super::*;

    #[test]
    fn silence_in_silence_out() {
        let mut rm = RingModulator::new(220.0, 1.0, 44_100.0);
        for _ in 0..2048 {
            let out = rm.process_sample(0.0);
            // DC blocker may produce tiny denormal-flushed values
            // before reaching steady state, so allow a femto-tolerance.
            assert!(out.abs() < 1e-20, "ring mod of silence: {out}");
        }
    }

    #[test]
    fn sine_input_output_finite_and_nonzero() {
        let mut rm = RingModulator::new(220.0, 1.0, 44_100.0);
        let sr = 44_100.0_f32;
        let mut any_nonzero = false;
        for i in 0..2048 {
            let input = (TAU * 440.0 * i as f32 / sr).sin();
            let out = rm.process_sample(input);
            assert!(out.is_finite(), "non-finite at {i}");
            if out.abs() > 1e-3 {
                any_nonzero = true;
            }
        }
        assert!(any_nonzero, "expected non-zero ring-mod output");
    }

    #[test]
    fn output_bounded_for_unit_input() {
        let mut rm = RingModulator::new(220.0, 1.0, 44_100.0);
        for _ in 0..2048 {
            let out = rm.process_sample(1.0);
            // Wet path passes through oversampler+DC blocker so peak
            // is bounded by ~|input| with small filter ringing
            // headroom.
            assert!(out.abs() < 1.5, "ring-mod output {out} out of bounds");
        }
    }

    #[test]
    fn dry_mix_passes_input_through() {
        let mut rm = RingModulator::new(220.0, 0.0, 44_100.0);
        let sr = 44_100.0_f32;
        for i in 0..1024 {
            let input = (TAU * 440.0 * i as f32 / sr).sin();
            let out = rm.process_sample(input);
            // mix=0 returns input directly (1-0)*input + 0*wet
            assert!((out - input).abs() < 1e-6, "mix=0 should be dry");
        }
    }
