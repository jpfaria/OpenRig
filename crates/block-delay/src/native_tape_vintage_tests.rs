    use super::*;
    use block_core::MonoProcessor;

    #[test]
    fn tape_vintage_outputs_finite_values() {
        let mut delay = TapeVintageDelay::new(TapeVintageParams::default(), 48_000.0);
        for _ in 0..10_000 {
            let output = delay.process_sample(0.2);
            assert!(output.is_finite());
        }
    }

    #[test]
    fn process_frame_silence_output_is_finite() {
        let mut delay = TapeVintageDelay::new(TapeVintageParams::default(), 44100.0);
        for i in 0..1024 {
            let out = delay.process_sample(0.0);
            assert!(out.is_finite(), "non-finite at sample {i}: {out}");
        }
    }

    #[test]
    fn process_frame_sine_output_is_finite() {
        let mut delay = TapeVintageDelay::new(TapeVintageParams::default(), 44100.0);
        for i in 0..1024 {
            let input = (i as f32 / 44100.0 * 440.0 * std::f32::consts::TAU).sin() * 0.5;
            let out = delay.process_sample(input);
            assert!(out.is_finite(), "non-finite at sample {i}: {out}");
        }
    }

    #[test]
    fn process_block_1024_frames_all_finite() {
        let mut delay = TapeVintageDelay::new(TapeVintageParams::default(), 44100.0);
        let mut buf: Vec<f32> = (0..1024)
            .map(|i| (i as f32 / 44100.0 * 440.0 * std::f32::consts::TAU).sin() * 0.5)
            .collect();
        delay.process_block(&mut buf);
        for (i, &s) in buf.iter().enumerate() {
            assert!(s.is_finite(), "non-finite at index {i}: {s}");
        }
    }
