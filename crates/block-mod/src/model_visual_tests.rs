    use super::*;
    #[test]
    fn tremolo_sine_pinned() {
        let o = model_color_override("tremolo_sine").unwrap();
        assert_eq!(o.panel_bg, Some([0x1a, 0x30, 0x30]));
    }
    #[test]
    fn unknown_model_returns_none() {
        assert!(model_color_override("classic_chorus").is_none());
    }
