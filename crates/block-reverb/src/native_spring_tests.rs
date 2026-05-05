    use super::*;

    #[test]
    fn spring_output_is_finite() {
        let mut reverb = SpringReverb::new(Params::default(), 44_100.0);
        for i in 0..10_000 {
            let input = if i % 100 == 0 { 0.5 } else { 0.0 };
            let [l, r] = StereoProcessor::process_frame(&mut reverb, [input, input]);
            assert!(l.is_finite(), "left output not finite at sample {i}");
            assert!(r.is_finite(), "right output not finite at sample {i}");
        }
    }

    #[test]
    fn process_frame_silence_output_finite() {
        let mut reverb = SpringReverb::new(Params::default(), 44_100.0);
        for i in 0..1024 {
            let [l, r] = StereoProcessor::process_frame(&mut reverb, [0.0, 0.0]);
            assert!(l.is_finite(), "left NaN/Inf at sample {i}");
            assert!(r.is_finite(), "right NaN/Inf at sample {i}");
        }
    }

    #[test]
    fn process_frame_sine_output_finite_and_nonzero() {
        let mut reverb = SpringReverb::new(Params::default(), 44_100.0);
        let sr = 44_100.0_f32;
        let mut any_nonzero = false;
        for i in 0..1024 {
            let input = (2.0 * std::f32::consts::PI * 440.0 * i as f32 / sr).sin();
            let [l, r] = StereoProcessor::process_frame(&mut reverb, [input, input]);
            assert!(l.is_finite(), "left not finite at sample {i}");
            assert!(r.is_finite(), "right not finite at sample {i}");
            if l.abs() > 1e-10 || r.abs() > 1e-10 {
                any_nonzero = true;
            }
        }
        assert!(any_nonzero, "expected non-zero output for sine input");
    }

    #[test]
    fn process_block_stereo_all_finite() {
        let mut reverb = SpringReverb::new(Params::default(), 44_100.0);
        let sr = 44_100.0_f32;
        let mut buffer: Vec<[f32; 2]> = (0..1024)
            .map(|i| {
                let s = (2.0 * std::f32::consts::PI * 440.0 * i as f32 / sr).sin();
                [s, s]
            })
            .collect();
        StereoProcessor::process_block(&mut reverb, &mut buffer);
        for (i, [l, r]) in buffer.iter().enumerate() {
            assert!(l.is_finite(), "left not finite at frame {i}");
            assert!(r.is_finite(), "right not finite at frame {i}");
        }
    }

    #[test]
    fn process_frame_mono_adapter_silence_finite() {
        let mut mono = SpringAsMono(SpringReverb::new(Params::default(), 44_100.0));
        for i in 0..1024 {
            let out = MonoProcessor::process_sample(&mut mono, 0.0);
            assert!(out.is_finite(), "mono output not finite at sample {i}");
        }
    }

    #[test]
    fn process_frame_mono_adapter_sine_finite_and_nonzero() {
        let mut mono = SpringAsMono(SpringReverb::new(Params::default(), 44_100.0));
        let sr = 44_100.0_f32;
        let mut any_nonzero = false;
        for i in 0..1024 {
            let input = (2.0 * std::f32::consts::PI * 440.0 * i as f32 / sr).sin();
            let out = MonoProcessor::process_sample(&mut mono, input);
            assert!(out.is_finite(), "mono not finite at sample {i}");
            if out.abs() > 1e-10 {
                any_nonzero = true;
            }
        }
        assert!(any_nonzero, "expected non-zero output for sine input (mono)");
    }
