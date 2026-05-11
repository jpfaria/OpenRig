    use super::*;

    fn default_reverb() -> GatedReverb {
        GatedReverb::new(Params::default(), 44_100.0)
    }

    #[test]
    fn silence_input_keeps_gate_closed() {
        let mut reverb = default_reverb();
        for i in 0..2048 {
            let [l, r] = StereoProcessor::process_frame(&mut reverb, [0.0, 0.0]);
            assert!(l.is_finite() && r.is_finite());
            assert!(l.abs() < 1e-9 && r.abs() < 1e-9, "gate must stay closed under silence (sample {i})");
        }
    }

    #[test]
    fn loud_burst_opens_gate_then_releases() {
        let mut reverb = default_reverb();
        let sr = 44_100.0_f32;
        // 50ms loud sine to trip the gate, then silence for >hold+release.
        let trigger_samples = (sr * 0.05) as usize;
        for i in 0..trigger_samples {
            let s = (2.0 * std::f32::consts::PI * 440.0 * i as f32 / sr).sin() * 0.8;
            let [l, _r] = StereoProcessor::process_frame(&mut reverb, [s, s]);
            assert!(l.is_finite());
        }
        // After ~hold (200ms) + release (30ms) + margin, gate gain should be back near 0.
        let settle = (sr * 0.6) as usize;
        let mut last_wet = 1.0_f32;
        for _ in 0..settle {
            let [l, _r] = StereoProcessor::process_frame(&mut reverb, [0.0, 0.0]);
            last_wet = l;
        }
        assert!(last_wet.abs() < 1e-3, "gate should have closed by now (got {last_wet})");
    }

    #[test]
    fn impulse_response_finite() {
        let mut reverb = default_reverb();
        for i in 0..44_100 {
            let input = if i < 100 { 0.5 } else { 0.0 };
            let [l, r] = StereoProcessor::process_frame(&mut reverb, [input, input]);
            assert!(l.is_finite() && r.is_finite(), "non-finite at sample {i}");
        }
    }

    #[test]
    fn mono_adapter_finite_under_burst() {
        let mut mono = GatedAsMono(default_reverb());
        let sr = 44_100.0_f32;
        for i in 0..4096 {
            let s = (2.0 * std::f32::consts::PI * 440.0 * i as f32 / sr).sin() * 0.6;
            assert!(MonoProcessor::process_sample(&mut mono, s).is_finite());
        }
    }
