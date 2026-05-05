
use super::*;
#[test]
fn cry_classic_pinned() {
    let o = model_color_override("cry_classic").unwrap();
    assert_eq!(o.panel_bg, Some([0x34, 0x24, 0x1a]));
}
#[test]
fn unknown_model_returns_none() {
    assert!(model_color_override("not_a_wah").is_none());
}
