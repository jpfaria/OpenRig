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

    fn goertzel_energy(sig: &[f32], freq: f32, sr: f32) -> f32 {
        let w = 2.0 * std::f32::consts::PI * freq / sr;
        let coeff = 2.0 * w.cos();
        let (mut s1, mut s2) = (0.0f32, 0.0f32);
        for &x in sig {
            let s0 = x + coeff * s1 - s2;
            s2 = s1;
            s1 = s0;
        }
        (s1 * s1 + s2 * s2 - coeff * s1 * s2).max(0.0)
    }

    #[test]
    fn octave_up_transposes_sine_up_one_octave() {
        let mut p = OctaveUp::new(2048);
        let sr = 44_100.0_f32;
        let f0 = 220.0;
        let out: Vec<f32> = (0..16384)
            .map(|i| p.step((2.0 * std::f32::consts::PI * f0 * i as f32 / sr).sin()))
            .collect();
        let tail = &out[4096..]; // skip the first grain while it fills
        let fund = goertzel_energy(tail, f0, sr);
        let octave = goertzel_energy(tail, f0 * 2.0, sr);
        assert!(
            octave > fund * 4.0,
            "octave-up shifter must make 2f dominate f: 2f={octave:.4}, f={fund:.4}"
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
