    use super::*;

    fn default_reverb() -> SpringReverb {
        SpringReverb::new(Params::default(), 44_100.0)
    }

    #[test]
    fn allpass_dispersion_silence_in_silence_out() {
        let mut ap = AllpassDispersion::new(ALLPASS_COEF);
        for _ in 0..1024 {
            assert!(ap.process(0.0).abs() < 1e-9);
        }
    }

    #[test]
    fn allpass_dispersion_unity_amplitude_response() {
        // First-order allpass should preserve magnitude (it's all phase).
        // Drive with a sine and check the RMS doesn't change > a few dB.
        let mut ap = AllpassDispersion::new(ALLPASS_COEF);
        let sr = 44_100.0_f32;
        let mut energy_in = 0.0;
        let mut energy_out = 0.0;
        // skip the first 200 samples (transient)
        for i in 0..2048 {
            let x = (2.0 * std::f32::consts::PI * 440.0 * i as f32 / sr).sin();
            let y = ap.process(x);
            if i > 200 {
                energy_in += x * x;
                energy_out += y * y;
            }
        }
        let ratio = (energy_out / energy_in).sqrt();
        assert!((ratio - 1.0).abs() < 0.05, "allpass changed amplitude: ratio={ratio}");
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
    fn sine_in_band_produces_finite_nonzero_output() {
        // Pick 800 Hz which is well within the spring's bandpass window.
        let mut reverb = default_reverb();
        let sr = 44_100.0_f32;
        let mut any_nonzero = false;
        for i in 0..8192 {
            let s = (2.0 * std::f32::consts::PI * 800.0 * i as f32 / sr).sin();
            let [l, r] = StereoProcessor::process_frame(&mut reverb, [s, s]);
            assert!(l.is_finite() && r.is_finite());
            if l.abs() > 1e-6 || r.abs() > 1e-6 {
                any_nonzero = true;
            }
        }
        assert!(any_nonzero, "expected non-zero output for in-band sine");
    }

    #[test]
    fn mono_adapter_runs_silence_and_sine() {
        let mut mono = SpringAsMono(default_reverb());
        for _ in 0..512 {
            assert!(MonoProcessor::process_sample(&mut mono, 0.0).is_finite());
        }
        let sr = 44_100.0_f32;
        let mut any_nonzero = false;
        for i in 0..4096 {
            let s = (2.0 * std::f32::consts::PI * 800.0 * i as f32 / sr).sin();
            let out = MonoProcessor::process_sample(&mut mono, s);
            assert!(out.is_finite());
            if out.abs() > 1e-6 { any_nonzero = true; }
        }
        assert!(any_nonzero);
    }
