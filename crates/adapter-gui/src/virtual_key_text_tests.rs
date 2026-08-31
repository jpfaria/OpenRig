//! #913 — what each on-screen key types.
//!
//! The three control keys are drawn as glyphs, so the translation is the only
//! thing standing between the user and a literal "⌫" in the text field. On the
//! Orange Pi this keyboard is the ONLY way to type, so a wrong mapping means a
//! field that cannot be corrected at all.

use super::key_text_for_label;

#[test]
fn backspace_types_the_delete_key_not_its_glyph() {
    let text = key_text_for_label("⌫");
    assert_eq!(
        text,
        slint::SharedString::from(slint::platform::Key::Backspace)
    );
    assert_ne!(text.as_str(), "⌫", "the glyph must never reach the field");
}

#[test]
fn return_types_the_return_key_not_its_glyph() {
    let text = key_text_for_label("⏎");
    assert_eq!(
        text,
        slint::SharedString::from(slint::platform::Key::Return)
    );
    assert_ne!(text.as_str(), "⏎");
}

#[test]
fn the_space_glyph_types_an_actual_space() {
    assert_eq!(key_text_for_label("⎵").as_str(), " ");
}

#[test]
fn an_ordinary_key_types_itself() {
    assert_eq!(key_text_for_label("a").as_str(), "a");
    assert_eq!(key_text_for_label("Z").as_str(), "Z");
    assert_eq!(key_text_for_label("7").as_str(), "7");
    assert_eq!(key_text_for_label("-").as_str(), "-");
}

#[test]
fn an_accented_key_types_itself() {
    // The chain names the owner types are Portuguese.
    assert_eq!(key_text_for_label("ç").as_str(), "ç");
    assert_eq!(key_text_for_label("ã").as_str(), "ã");
}

#[test]
fn an_empty_label_types_nothing() {
    assert_eq!(key_text_for_label("").as_str(), "");
}

#[test]
fn the_three_control_keys_are_distinct_from_each_other() {
    let keys = ["⌫", "⏎", "⎵"].map(key_text_for_label);
    assert_ne!(keys[0], keys[1]);
    assert_ne!(keys[1], keys[2]);
    assert_ne!(keys[0], keys[2]);
}
