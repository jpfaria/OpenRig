use super::sanitize_label;

#[test]
fn sanitize_label_strips_leading_emoji_keeps_text() {
    // Issue #424: Bogner Ecstasy capture grid labels arrive from the
    // third-party manifest as `"✋ 4X12"`. The emoji renders as tofu on
    // Windows because no shipped font carries a glyph for U+270B.
    assert_eq!(sanitize_label("✋ 4X12"), "4X12");
}

#[test]
fn sanitize_label_strips_pictograph_keeps_text() {
    assert_eq!(sanitize_label("📦 Cabinet"), "Cabinet");
    assert_eq!(sanitize_label("🔥 Drive"), "Drive");
}

#[test]
fn sanitize_label_strips_dingbats_and_misc_symbols() {
    // ★ (U+2605) and ☎ (U+260E) are inside the Misc Symbols / Dingbats
    // block — same tofu story as full emoji.
    assert_eq!(sanitize_label("★ Lead"), "Lead");
    assert_eq!(sanitize_label("☎ Phone"), "Phone");
}

#[test]
fn sanitize_label_passes_clean_text_unchanged() {
    assert_eq!(sanitize_label("4X12"), "4X12");
    assert_eq!(sanitize_label("Drive A"), "Drive A");
    // Punctuation, slashes and digits stay.
    assert_eq!(sanitize_label("Drive/Gain"), "Drive/Gain");
}

#[test]
fn sanitize_label_keeps_cjk_arabic_devanagari() {
    // Pinned: stripping must not touch scripts the project ships fonts
    // for via the locale cascade (issue #424 fix would regress JP/ZH/HI
    // users otherwise).
    assert_eq!(sanitize_label("輸入"), "輸入");
    assert_eq!(sanitize_label("ドライブ"), "ドライブ");
    assert_eq!(sanitize_label("الإدخال"), "الإدخال");
    assert_eq!(sanitize_label("ध्वनि"), "ध्वनि");
}

#[test]
fn sanitize_label_falls_back_to_raw_when_only_emoji() {
    // A label that's nothing but emoji (no fallback text) is rare but
    // legal in third-party manifests. Returning empty would render an
    // anonymous selector position; keep the raw codepoint so the user
    // can still tell positions apart even if they show as tofu.
    assert_eq!(sanitize_label("✋"), "✋");
    assert_eq!(sanitize_label("   ✋   "), "   ✋   ");
}

#[test]
fn sanitize_label_drops_zwj_compound_emoji() {
    // ZWJ-compound emoji like family-glyphs leave nothing renderable
    // after the pictograph base is stripped; the leftover ZWJs would
    // otherwise still trip up shaping.
    let zwj = "\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F466} Family";
    assert_eq!(sanitize_label(zwj), "Family");
}

#[test]
fn sanitize_label_drops_variation_selector_after_base() {
    // ❤️ = U+2764 (heart, inside the Misc Symbols range we strip) +
    // U+FE0F (variation selector forcing emoji presentation). Both go.
    assert_eq!(sanitize_label("\u{2764}\u{FE0F} Love"), "Love");
}

#[test]
fn sanitize_label_trims_resulting_whitespace_both_sides() {
    assert_eq!(sanitize_label("  ✋  4X12  "), "4X12");
}
