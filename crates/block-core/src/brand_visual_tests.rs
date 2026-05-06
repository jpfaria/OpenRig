use super::*;

#[test]
fn known_brand_returns_some() {
    assert!(brand_colors("marshall").is_some());
    assert!(brand_colors("fender").is_some());
    assert!(brand_colors("boss").is_some());
}

#[test]
fn unknown_brand_returns_none() {
    assert!(brand_colors("not-a-brand").is_none());
    assert!(brand_colors("").is_none());
}

#[test]
fn santa_cruz_uses_underscore_id() {
    assert!(brand_colors("santa_cruz").is_some());
    assert!(brand_colors("santa-cruz").is_none());
}

#[test]
fn marshall_panel_bg_pinned() {
    let m = brand_colors("marshall").unwrap();
    assert_eq!(m.panel_bg, [0xb8, 0x98, 0x40]);
    assert_eq!(m.panel_text, [0x5a, 0x4a, 0x20]);
    assert_eq!(m.photo_offset_y, -0.2);
}

#[test]
fn compose_with_no_brand_returns_default() {
    assert_eq!(compose(None, None), ModelColorScheme::DEFAULT);
}

#[test]
fn compose_brand_only_returns_brand() {
    let b = brand_colors("marshall").unwrap();
    assert_eq!(compose(Some(b), None), b);
}

#[test]
fn compose_override_wins_over_brand() {
    let brand = brand_colors("marshall").unwrap();
    let over = ModelColorOverride {
        panel_bg: Some([0xff, 0x00, 0x00]),
        ..Default::default()
    };
    let merged = compose(Some(brand), Some(over));
    assert_eq!(merged.panel_bg, [0xff, 0x00, 0x00]);
    // Untouched fields keep brand value
    assert_eq!(merged.panel_text, brand.panel_text);
}

#[test]
fn compose_partial_override_keeps_brand_for_unset_fields() {
    let brand = brand_colors("fender").unwrap();
    let over = ModelColorOverride {
        model_font: Some("Dancing Script"),
        ..Default::default()
    };
    let merged = compose(Some(brand), Some(over));
    assert_eq!(merged.model_font, "Dancing Script");
    assert_eq!(merged.panel_bg, brand.panel_bg);
}
