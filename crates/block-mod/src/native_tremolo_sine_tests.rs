    use super::*;

    #[test]
    fn process_sample_silence_output_finite() {
        let mut trem = SineTremolo::new(4.0, 0.5, 44_100.0);
        for i in 0..1024 {
            let out = MonoProcessor::process_sample(&mut trem, 0.0);
            assert!(out.is_finite(), "output not finite at sample {i}");
        }
    }

    #[test]
    fn process_sample_silence_is_zero() {
        let mut trem = SineTremolo::new(4.0, 0.5, 44_100.0);
        for _ in 0..1024 {
            let out = MonoProcessor::process_sample(&mut trem, 0.0);
            assert_eq!(out, 0.0, "tremolo of silence should be silence");
        }
    }

    #[test]
    fn process_sample_sine_output_finite_and_nonzero() {
        let mut trem = SineTremolo::new(4.0, 0.5, 44_100.0);
        let sr = 44_100.0_f32;
        let mut any_nonzero = false;
        for i in 0..1024 {
            let input = (2.0 * std::f32::consts::PI * 440.0 * i as f32 / sr).sin();
            let out = MonoProcessor::process_sample(&mut trem, input);
            assert!(out.is_finite(), "output not finite at sample {i}");
            if out.abs() > 1e-10 {
                any_nonzero = true;
            }
        }
        assert!(any_nonzero, "expected non-zero output for sine input");
    }

    #[test]
    fn process_block_all_finite() {
        let mut trem = SineTremolo::new(4.0, 0.5, 44_100.0);
        let sr = 44_100.0_f32;
        let mut buffer: Vec<f32> = (0..1024)
            .map(|i| (2.0 * std::f32::consts::PI * 440.0 * i as f32 / sr).sin())
            .collect();
        MonoProcessor::process_block(&mut trem, &mut buffer);
        for (i, s) in buffer.iter().enumerate() {
            assert!(s.is_finite(), "output not finite at frame {i}");
        }
    }

    #[test]
    fn process_sample_output_bounded_by_input() {
        let mut trem = SineTremolo::new(4.0, 1.0, 44_100.0);
        for _ in 0..1024 {
            let out = MonoProcessor::process_sample(&mut trem, 1.0);
            assert!(out >= 0.0 && out <= 1.0,
                "tremolo output {out} should be in [0,1] for unit input with full depth");
        }
    }
