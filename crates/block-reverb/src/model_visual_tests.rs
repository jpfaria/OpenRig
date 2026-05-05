    use super::*;
    #[test]
    fn plate_foundation_pinned() {
        let o = model_color_override("plate_foundation").unwrap();
        assert_eq!(o.panel_bg, Some([0x20, 0x28, 0x34]));
    }
    #[test]
    fn unknown_model_returns_none() {
        assert!(model_color_override("hall").is_none());
    }
