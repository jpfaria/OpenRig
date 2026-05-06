    use super::*;

    fn default_reverb() -> ShimmerReverb {
        ShimmerReverb::new(Params::default(), 44_100.0)
    }

    #[test]
    fn octave_up_silence_in_silence_out() {
        let mut p = OctaveUp::new(2048);
        for _ in 0..8192 {
            assert!(p.step(0.0).abs() < 1e-6);
        }
    }

    #[test]
    fn octave_up_finite_for_sine_input() {
        let mut p = OctaveUp::new(2048);
        let sr = 44_100.0_f32;
        for i in 0..8192 {
            let s = (2.0 * std::f32::consts::PI * 220.0 * i as f32 / sr).sin();
            assert!(p.step(s).is_finite());
        }
    }

    #[test]
    fn impulse_response_finite() {
        let mut reverb = default_reverb();
        for i in 0..44_100 {
            let input = if i == 0 { 1.0 } else { 0.0 };
            let [l, r] = StereoProcessor::process_frame(&mut reverb, [input, input]);
            assert!(l.is_finite() && r.is_finite(), "non-finite at {i}");
        }
    }

    #[test]
    fn silence_input_produces_finite_silence() {
        let mut reverb = default_reverb();
        for i in 0..2048 {
            let [l, r] = StereoProcessor::process_frame(&mut reverb, [0.0, 0.0]);
            assert!(l.is_finite() && r.is_finite(), "non-finite at {i}");
        }
    }

    #[test]
    fn sine_input_produces_finite_nonzero_output() {
        let mut reverb = default_reverb();
        let sr = 44_100.0_f32;
        let mut any_nonzero = false;
        for i in 0..8192 {
            let s = (2.0 * std::f32::consts::PI * 440.0 * i as f32 / sr).sin();
            let [l, r] = StereoProcessor::process_frame(&mut reverb, [s, s]);
            assert!(l.is_finite() && r.is_finite());
            if l.abs() > 1e-6 || r.abs() > 1e-6 {
                any_nonzero = true;
            }
        }
        assert!(any_nonzero);
    }

    #[test]
    fn mono_adapter_runs_silence_and_sine() {
        let mut mono = ShimmerAsMono(default_reverb());
        for _ in 0..512 {
            assert!(MonoProcessor::process_sample(&mut mono, 0.0).is_finite());
        }
        let sr = 44_100.0_f32;
        let mut any_nonzero = false;
        for i in 0..4096 {
            let s = (2.0 * std::f32::consts::PI * 220.0 * i as f32 / sr).sin();
            let out = MonoProcessor::process_sample(&mut mono, s);
            assert!(out.is_finite());
            if out.abs() > 1e-6 { any_nonzero = true; }
        }
        assert!(any_nonzero);
    }
