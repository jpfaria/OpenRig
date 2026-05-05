
use super::*;

fn item(display_name: &str, brand: &str) -> BlockModelPickerItem {
    BlockModelPickerItem {
        effect_type: "gain".into(),
        model_id: "test_model".into(),
        label: display_name.into(),
        display_name: display_name.into(),
        subtitle: "".into(),
        icon_kind: "gain".into(),
        brand: brand.into(),
        type_label: "NAM".into(),
        panel_bg: slint::Color::from_argb_u8(255, 0, 0, 0),
        panel_text: slint::Color::from_argb_u8(255, 255, 255, 255),
        brand_strip_bg: slint::Color::from_argb_u8(255, 0, 0, 0),
        model_font: "".into(),
        photo_offset_x: 0.0,
        photo_offset_y: 0.0,
        available: true,
        thumbnail_path: "".into(),
    }
}

#[test]
fn normalize_lowercases_and_strips_hyphens_and_spaces() {
    assert_eq!(normalize("Boss DS-1"), "bossds1");
    assert_eq!(normalize("Marshall JCM 800"), "marshalljcm800");
    assert_eq!(normalize("AC-30"), "ac30");
    assert_eq!(normalize(""), "");
    assert_eq!(normalize("---   "), "");
}

#[test]
fn model_matches_returns_true_for_empty_query() {
    assert!(model_matches("", "Boss DS-1", "boss"));
    assert!(model_matches("", "", ""));
}

#[test]
fn model_matches_finds_substring_in_name() {
    assert!(model_matches("ds", "Boss DS-1", "boss"));
}

#[test]
fn model_matches_finds_substring_in_brand() {
    assert!(model_matches("marshall", "JCM 800", "marshall"));
}

#[test]
fn model_matches_ignores_case() {
    assert!(model_matches("BOSS", "Boss DS-1", "boss"));
    assert!(model_matches("boss", "BOSS DS-1", "BOSS"));
}

#[test]
fn model_matches_ignores_hyphens() {
    assert!(model_matches("ds1", "DS-1", "boss"));
    assert!(model_matches("ds-1", "DS1", "boss"));
}

#[test]
fn model_matches_ignores_spaces() {
    assert!(model_matches("jcm800", "JCM 800", "marshall"));
    assert!(model_matches("jcm 800", "JCM800", "marshall"));
}

#[test]
fn model_matches_concatenates_brand_and_name() {
    assert!(model_matches("marshalljcm", "JCM 800", "marshall"));
    assert!(model_matches("marshall jcm", "JCM 800", "marshall"));
}

#[test]
fn model_matches_returns_false_when_no_match() {
    assert!(!model_matches("xyz", "Boss DS-1", "boss"));
}

#[test]
fn filter_models_preserves_order() {
    let items = vec![
        item("Boss DS-1", "boss"),
        item("Boss MT-2", "boss"),
        item("Marshall JCM 800", "marshall"),
    ];
    let filtered = filter_models(&items, "boss");
    assert_eq!(filtered.len(), 2);
    assert_eq!(filtered[0].display_name, "Boss DS-1");
    assert_eq!(filtered[1].display_name, "Boss MT-2");
}

#[test]
fn filter_models_returns_empty_when_no_match() {
    let items = vec![
        item("Boss DS-1", "boss"),
        item("Marshall JCM 800", "marshall"),
    ];
    let filtered = filter_models(&items, "fender");
    assert!(filtered.is_empty());
}
