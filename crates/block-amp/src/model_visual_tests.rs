    use super::*;

    #[test]
    fn known_models_return_some() {
        assert!(model_color_override("blackface_clean").is_some());
        assert!(model_color_override("chime").is_some());
        assert!(model_color_override("tweed_breakup").is_some());
    }

    #[test]
    fn unknown_model_returns_none() {
        assert!(model_color_override("nam_marshall_plexi").is_none());
        assert!(model_color_override("").is_none());
    }

    #[test]
    fn blackface_clean_pinned() {
        let o = model_color_override("blackface_clean").unwrap();
        assert_eq!(o.panel_bg, Some([0x28, 0x30, 0x38]));
        assert_eq!(o.model_font, Some("Dancing Script"));
    }
