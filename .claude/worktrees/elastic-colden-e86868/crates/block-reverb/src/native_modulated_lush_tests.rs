    use super::*;

    fn default_reverb() -> LushReverb {
        LushReverb::new(Params::default(), 44_100.0)
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
        for i in 0..4096 {
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
    fn mod_delay_with_zero_depth_acts_as_static_delay() {
        let sr = 44_100.0_f32;
        let mut d = ModDelay::new(100.0, 0.0, 1.0, sr);
        // Write 200 samples then read — should reproduce the input N samples ago.
        for i in 0..200 {
            let s = i as f32 * 0.01;
            // Read first to populate the LFO advance, then write (matches the
            // ordering used inside StereoProcessor::process_frame).
            let _ = d.read();
            d.write(s);
        }
        // Now read — should be approximately sample (current_write - 100).
        // Since we just wrote 200 samples, current write_idx points at 200%len.
        // Sample at offset 100 back is what we wrote at index 100, value 0.01 * 100 = 1.0.
        let out = d.read();
        assert!((out - 1.0).abs() < 0.01, "expected ~1.0, got {out}");
    }

    #[test]
    fn mono_adapter_runs_silence_and_sine() {
        let mut mono = LushAsMono(default_reverb());
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
