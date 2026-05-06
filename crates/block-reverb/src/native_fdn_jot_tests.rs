    use super::*;

    fn default_reverb() -> FdnReverb {
        FdnReverb::new(Params::default(), 44_100.0)
    }

    #[test]
    fn hadamard_is_self_inverse_up_to_scale() {
        let mut x: [f32; N] = [1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0];
        let original = x;
        hadamard8(&mut x);
        hadamard8(&mut x);
        // H * H = I (with our 1/sqrt(N) normalisation per call → /N total).
        // After two calls the result equals original input.
        for i in 0..N {
            assert!(
                (x[i] - original[i]).abs() < 1e-4,
                "h(h(x))[{i}] = {} != {}",
                x[i], original[i],
            );
        }
    }

    #[test]
    fn hadamard_preserves_energy() {
        let mut x: [f32; N] = [0.5, -0.3, 1.0, 0.0, 0.7, -0.2, 0.4, -0.1];
        let energy_in: f32 = x.iter().map(|v| v * v).sum();
        hadamard8(&mut x);
        let energy_out: f32 = x.iter().map(|v| v * v).sum();
        assert!(
            (energy_in - energy_out).abs() < 1e-5,
            "energy in {} != energy out {}",
            energy_in, energy_out
        );
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
        for i in 0..2048 {
            let s = (2.0 * std::f32::consts::PI * 440.0 * i as f32 / sr).sin();
            let [l, r] = StereoProcessor::process_frame(&mut reverb, [s, s]);
            assert!(l.is_finite() && r.is_finite());
            if l.abs() > 1e-6 || r.abs() > 1e-6 {
                any_nonzero = true;
            }
        }
        assert!(any_nonzero, "expected non-zero output");
    }

    #[test]
    fn mono_adapter_runs_silence_and_sine() {
        let mut mono = FdnAsMono(default_reverb());
        for _ in 0..512 {
            assert!(MonoProcessor::process_sample(&mut mono, 0.0).is_finite());
        }
        let sr = 44_100.0_f32;
        let mut any_nonzero = false;
        for i in 0..2048 {
            let s = (2.0 * std::f32::consts::PI * 220.0 * i as f32 / sr).sin();
            let out = MonoProcessor::process_sample(&mut mono, s);
            assert!(out.is_finite());
            if out.abs() > 1e-6 {
                any_nonzero = true;
            }
        }
        assert!(any_nonzero, "mono adapter expected non-zero output");
    }
