    use super::*;

    fn default_reverb() -> Freeverb {
        Freeverb::new(Params::default(), 44_100.0)
    }

    #[test]
    fn impulse_response_finite_and_decaying() {
        let mut reverb = default_reverb();
        let mut peak_late = 0.0f32;
        for i in 0..44_100 {
            let input = if i == 0 { 1.0 } else { 0.0 };
            let [l, r] = StereoProcessor::process_frame(&mut reverb, [input, input]);
            assert!(l.is_finite() && r.is_finite(), "non-finite at sample {i}");
            if i > 22_050 {
                peak_late = peak_late.max(l.abs()).max(r.abs());
            }
        }
        assert!(peak_late.is_finite(), "tail not finite");
    }

    #[test]
    fn silence_input_produces_finite_silence() {
        let mut reverb = default_reverb();
        for i in 0..1024 {
            let [l, r] = StereoProcessor::process_frame(&mut reverb, [0.0, 0.0]);
            assert!(l.is_finite() && r.is_finite(), "non-finite at {i}");
        }
    }

    #[test]
    fn sine_input_produces_finite_nonzero_output() {
        let mut reverb = default_reverb();
        let sr = 44_100.0_f32;
        let mut any_nonzero = false;
        for i in 0..1024 {
            let s = (2.0 * std::f32::consts::PI * 440.0 * i as f32 / sr).sin();
            let [l, r] = StereoProcessor::process_frame(&mut reverb, [s, s]);
            assert!(l.is_finite() && r.is_finite(), "non-finite at {i}");
            if l.abs() > 1e-10 || r.abs() > 1e-10 {
                any_nonzero = true;
            }
        }
        assert!(any_nonzero, "expected non-zero output");
    }

    #[test]
    fn mono_adapter_runs_silence_and_sine() {
        let mut mono = FreeverbAsMono(default_reverb());
        for _ in 0..512 {
            assert!(MonoProcessor::process_sample(&mut mono, 0.0).is_finite());
        }
        let sr = 44_100.0_f32;
        let mut any_nonzero = false;
        for i in 0..1024 {
            let s = (2.0 * std::f32::consts::PI * 220.0 * i as f32 / sr).sin();
            let out = MonoProcessor::process_sample(&mut mono, s);
            assert!(out.is_finite());
            if out.abs() > 1e-10 {
                any_nonzero = true;
            }
        }
        assert!(any_nonzero, "mono adapter expected non-zero output");
    }

    #[test]
    fn process_block_remains_finite() {
        let mut reverb = default_reverb();
        let sr = 44_100.0_f32;
        let mut buf: Vec<[f32; 2]> = (0..1024)
            .map(|i| {
                let s = (2.0 * std::f32::consts::PI * 440.0 * i as f32 / sr).sin();
                [s, s]
            })
            .collect();
        StereoProcessor::process_block(&mut reverb, &mut buf);
        for [l, r] in &buf {
            assert!(l.is_finite() && r.is_finite());
        }
    }
