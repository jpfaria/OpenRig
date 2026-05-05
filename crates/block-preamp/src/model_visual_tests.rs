    use super::*;

    #[test]
    fn known_models_return_some() {
        assert!(model_color_override("american_clean").is_some());
        assert!(model_color_override("brit_crunch").is_some());
        assert!(model_color_override("modern_high_gain").is_some());
    }

    #[test]
    fn unknown_model_returns_none() {
        assert!(model_color_override("nam_marshall_plexi").is_none());
    }
