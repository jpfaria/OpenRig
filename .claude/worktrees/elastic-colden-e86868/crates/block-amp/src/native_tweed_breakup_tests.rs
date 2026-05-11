    use super::*;
    fn default_params() -> block_core::param::ParameterSet {
        let schema = native_core::model_schema(MODEL_ID, DISPLAY_NAME, DEFAULTS);
        block_core::param::ParameterSet::default()
            .normalized_against(&schema)
            .expect("defaults should normalize")
    }

    #[test]
    fn process_frame_silence_output_is_finite() {
        let params = default_params();
        let mut proc = match build(&params, 44100.0, AudioChannelLayout::Mono).unwrap() {
            BlockProcessor::Mono(p) => p,
            _ => panic!("expected Mono"),
        };
        for i in 0..1024 {
            let out = proc.process_sample(0.0);
            assert!(out.is_finite(), "non-finite at sample {i}: {out}");
        }
    }

    #[test]
    fn process_frame_sine_output_is_finite() {
        let params = default_params();
        let mut proc = match build(&params, 44100.0, AudioChannelLayout::Mono).unwrap() {
            BlockProcessor::Mono(p) => p,
            _ => panic!("expected Mono"),
        };
        for i in 0..1024 {
            let input = (i as f32 / 44100.0 * 440.0 * std::f32::consts::TAU).sin() * 0.5;
            let out = proc.process_sample(input);
            assert!(out.is_finite(), "non-finite at sample {i}: {out}");
        }
    }

    #[test]
    fn process_block_1024_frames_all_finite() {
        let params = default_params();
        let mut proc = match build(&params, 44100.0, AudioChannelLayout::Mono).unwrap() {
            BlockProcessor::Mono(p) => p,
            _ => panic!("expected Mono"),
        };
        let mut buf = vec![0.0_f32; 1024];
        for (i, sample) in buf.iter_mut().enumerate() {
            *sample = (i as f32 / 44100.0 * 440.0 * std::f32::consts::TAU).sin() * 0.5;
        }
        proc.process_block(&mut buf);
        for (i, &s) in buf.iter().enumerate() {
            assert!(s.is_finite(), "non-finite at index {i}: {s}");
        }
    }
