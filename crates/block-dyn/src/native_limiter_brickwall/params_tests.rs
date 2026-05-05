    use super::*;

    #[test]
    fn defaults_are_valid_against_schema() {
        let schema = model_schema();
        let result = ParameterSet::default().normalized_against(&schema);
        assert!(
            result.is_ok(),
            "defaults must normalize: {:?}",
            result.err()
        );
    }

    #[test]
    fn params_from_set_reads_defaults() {
        let schema = model_schema();
        let ps = ParameterSet::default()
            .normalized_against(&schema)
            .expect("defaults");
        let params = params_from_set(&ps).expect("parse");
        assert_eq!(params, LimiterParams::default());
    }

    #[test]
    fn schema_has_all_expected_params() {
        let schema = model_schema();
        let names: Vec<&str> = schema.parameters.iter().map(|p| p.path.as_str()).collect();
        for required in &[
            "threshold",
            "ceiling",
            "release_ms",
            "lookahead_ms",
            "knee_db",
        ] {
            assert!(names.contains(required), "missing param: {required}");
        }
    }
