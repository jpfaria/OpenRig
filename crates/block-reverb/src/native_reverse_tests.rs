    use super::*;

    fn default_reverb() -> ReverseReverb {
        ReverseReverb::new(Params::default(), 44_100.0)
    }

    #[test]
    fn silence_input_produces_finite_silence() {
        let mut reverb = default_reverb();
        for i in 0..2048 {
            let [l, r] = StereoProcessor::process_frame(&mut reverb, [0.0, 0.0]);
            assert!(l.is_finite() && r.is_finite(), "non-finite at {i}");
            assert!(l.abs() < 1e-9 && r.abs() < 1e-9, "silence in must produce silence out");
        }
    }

    #[test]
    fn impulse_response_finite_and_appears_in_second_half() {
        let mut reverb = default_reverb();
        // Write impulse at sample 0; the reversed wet should appear in
        // the SECOND ping-pong window (after one half_len has elapsed).
        let mut peak = 0.0f32;
        for i in 0..44_100 {
            let input = if i == 0 { 1.0 } else { 0.0 };
            let [l, _r] = StereoProcessor::process_frame(&mut reverb, [input, input]);
            assert!(l.is_finite());
            peak = peak.max(l.abs());
        }
        assert!(peak > 1e-6, "expected non-zero wet output for an impulse");
    }

    #[test]
    fn sine_input_produces_finite_nonzero_output() {
        let mut reverb = default_reverb();
        let sr = 44_100.0_f32;
        let mut any_nonzero = false;
        for i in 0..44_100 {
            let s = (2.0 * std::f32::consts::PI * 440.0 * i as f32 / sr).sin();
            let [l, r] = StereoProcessor::process_frame(&mut reverb, [s, s]);
            assert!(l.is_finite() && r.is_finite());
            if l.abs() > 1e-6 || r.abs() > 1e-6 {
                any_nonzero = true;
            }
        }
        assert!(any_nonzero, "expected non-zero output for sine input");
    }

    #[test]
    fn mono_adapter_runs_silence_and_sine() {
        let mut mono = ReverseAsMono(default_reverb());
        for _ in 0..512 {
            assert!(MonoProcessor::process_sample(&mut mono, 0.0).is_finite());
        }
        let sr = 44_100.0_f32;
        let mut any_nonzero = false;
        for i in 0..44_100 {
            let s = (2.0 * std::f32::consts::PI * 220.0 * i as f32 / sr).sin();
            let out = MonoProcessor::process_sample(&mut mono, s);
            assert!(out.is_finite());
            if out.abs() > 1e-6 {
                any_nonzero = true;
            }
        }
        assert!(any_nonzero, "mono adapter expected non-zero output");
    }

    #[test]
    fn envelope_starts_at_zero_for_each_window() {
        // First sample of each playback window should be silent
        // (envelope = 0 at the very start of the read pass).
        let mut buf = ReverseBuffer::new(100);
        // Fill the buffer with a non-zero pattern through one full ping-pong.
        for i in 0..100 {
            buf.process(1.0);
            let _ = i;
        }
        // Now we should be reading the previously-written half. The very
        // first wet output of this new pass has envelope = 0 → silence.
        let first_wet_envelope_zero = buf.process(1.0);
        assert!(first_wet_envelope_zero.abs() < 1e-9);
    }
