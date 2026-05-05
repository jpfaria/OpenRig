    use super::*;
    use block_core::param::ParameterDomain;
    use domain::value_objects::ParameterValue;

    fn make_params(rate_hz: f32, depth: f32, mix: f32) -> ParameterSet {
        let mut ps = ParameterSet::default();
        ps.insert("rate_hz", ParameterValue::Float(rate_hz));
        ps.insert("depth", ParameterValue::Float(depth));
        ps.insert("mix", ParameterValue::Float(mix));
        ps
    }

    #[test]
    fn schema_has_rate_depth_mix_parameters() {
        let schema = model_schema();
        let paths: Vec<&str> = schema.parameters.iter().map(|p| p.path.as_str()).collect();
        assert!(paths.contains(&"rate_hz"), "missing rate_hz");
        assert!(paths.contains(&"depth"), "missing depth");
        assert!(paths.contains(&"mix"), "missing mix");
    }

    #[test]
    fn schema_rate_range_is_0_1_to_5_hz() {
        let schema = model_schema();
        let rate = schema.parameters.iter().find(|p| p.path == "rate_hz").unwrap();
        let ParameterDomain::FloatRange { min, max, .. } = rate.domain else {
            panic!("expected FloatRange");
        };
        assert_eq!(min, 0.1);
        assert_eq!(max, 5.0);
    }

    #[test]
    fn dry_signal_passes_through_when_mix_is_zero() {
        let mut chorus = ClassicChorus::new(0.5, 0.5, 0.0, 44100.0);
        // With mix=0, output should equal input (no wet signal)
        let input = 1.0_f32;
        let output = chorus.process_sample(input);
        assert_eq!(output, input);
    }

    #[test]
    fn output_is_zero_for_silent_input() {
        let mut chorus = ClassicChorus::new(0.5, 0.5, 0.5, 44100.0);
        // Process many samples of silence
        for _ in 0..2000 {
            let out = chorus.process_sample(0.0);
            assert_eq!(out, 0.0, "silent input should produce silent output");
        }
    }

    #[test]
    fn model_definition_has_correct_id_and_display_name() {
        assert_eq!(MODEL_DEFINITION.id, "classic_chorus");
        assert_eq!(MODEL_DEFINITION.display_name, "Classic Chorus");
    }

    #[test]
    fn build_succeeds_with_valid_params() {
        let params = make_params(0.5, 50.0, 50.0);
        let result = build_processor(&params, 44100.0);
        assert!(result.is_ok());
    }

    #[test]
    fn schema_model_id_matches_constant() {
        let schema = model_schema();
        assert_eq!(schema.model, MODEL_ID);
    }

    #[test]
    fn process_sample_silence_output_finite() {
        let mut chorus = ClassicChorus::new(0.5, 0.5, 0.5, 44_100.0);
        for i in 0..1024 {
            let out = chorus.process_sample(0.0);
            assert!(out.is_finite(), "output not finite at sample {i}");
        }
    }

    #[test]
    fn process_sample_sine_output_finite_and_nonzero() {
        let mut chorus = ClassicChorus::new(0.5, 0.5, 0.5, 44_100.0);
        let sr = 44_100.0_f32;
        let mut any_nonzero = false;
        for i in 0..1024 {
            let input = (2.0 * std::f32::consts::PI * 440.0 * i as f32 / sr).sin();
            let out = chorus.process_sample(input);
            assert!(out.is_finite(), "output not finite at sample {i}");
            if out.abs() > 1e-10 {
                any_nonzero = true;
            }
        }
        assert!(any_nonzero, "expected non-zero output for sine input");
    }

    #[test]
    fn process_block_all_finite() {
        let mut chorus = ClassicChorus::new(0.5, 0.5, 0.5, 44_100.0);
        let sr = 44_100.0_f32;
        let mut buffer: Vec<f32> = (0..1024)
            .map(|i| (2.0 * std::f32::consts::PI * 440.0 * i as f32 / sr).sin())
            .collect();
        MonoProcessor::process_block(&mut chorus, &mut buffer);
        for (i, s) in buffer.iter().enumerate() {
            assert!(s.is_finite(), "output not finite at frame {i}");
        }
    }
