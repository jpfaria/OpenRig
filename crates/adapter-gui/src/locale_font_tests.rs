//! #913 — the font family each locale needs to render without tofu.
//!
//! A script whose face lacks its codepoints renders .notdef boxes, which on the
//! Orange Pi is what the user actually sees. Each non-Latin locale must map to
//! the macOS face that covers its script, and every Latin locale must keep the
//! project's display font rather than silently falling through.

use super::{font_family_for_locale, font_for_persisted_runtime};

#[test]
fn each_non_latin_script_gets_the_face_that_covers_it() {
    assert_eq!(font_family_for_locale("ja-JP"), "Hiragino Sans");
    assert_eq!(
        font_family_for_locale("zh-CN"),
        "Hiragino Sans GB",
        "Simplified Chinese needs the GB sibling — plain Hiragino Sans renders \
         zh-Hans-only codepoints as .notdef"
    );
    assert_eq!(font_family_for_locale("ko-KR"), "Apple SD Gothic Neo");
    assert_eq!(font_family_for_locale("hi-IN"), "Kohinoor Devanagari");
}

#[test]
fn every_latin_locale_keeps_the_projects_display_font() {
    for locale in ["pt-BR", "en-US", "es-ES", "fr-FR", "de-DE"] {
        assert_eq!(font_family_for_locale(locale), "Bebas Neue", "{locale}");
    }
}

#[test]
fn an_unknown_locale_falls_back_to_the_display_font_rather_than_nothing() {
    assert_eq!(font_family_for_locale("xx-YY"), "Bebas Neue");
    assert_eq!(font_family_for_locale(""), "Bebas Neue");
}

#[test]
fn the_cjk_faces_are_distinct_from_each_other() {
    let faces = ["ja-JP", "zh-CN", "ko-KR", "hi-IN"].map(font_family_for_locale);
    let mut sorted = faces.to_vec();
    sorted.sort_unstable();
    sorted.dedup();
    assert_eq!(
        sorted.len(),
        faces.len(),
        "two scripts sharing a face is how tofu got shipped"
    );
}

#[test]
fn the_boot_font_is_one_of_the_families_a_locale_can_ask_for() {
    // Reads the persisted GUI setting; whatever it holds, the answer must be a
    // face the mapping above can produce — never an empty string.
    let boot = font_for_persisted_runtime();
    let known = ["ja-JP", "zh-CN", "ko-KR", "hi-IN", "en-US"].map(font_family_for_locale);
    assert!(known.contains(&boot), "unexpected boot font: {boot:?}");
}
