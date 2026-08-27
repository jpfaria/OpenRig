//! #913 — switching the two translation catalogs together.
//!
//! OpenRig carries two: gettext for Slint's `@tr(...)` and rust-i18n for the
//! strings Rust injects through setters. `init_translations` and
//! `apply_bundled_translation` must move BOTH — a switch that moved only the
//! Slint side left the injected strings frozen in the boot locale, which the
//! user saw as "Salvar chain" inside an otherwise Japanese UI.
//!
//! The active locale is process-wide, so this is ONE test rather than several:
//! parallel cases would each restore it under the others and the assertions
//! would race. The original locale is put back before returning, because the
//! rest of the suite reads translated strings.

use super::{apply_bundled_translation, init_translations};

#[test]
fn both_entry_points_move_the_rust_catalog_and_survive_a_locale_they_cannot_apply() {
    let original = rust_i18n::locale().to_string();

    // A persisted language with a real catalog is activated as asked.
    init_translations(Some("ja-JP"));
    assert_eq!(rust_i18n::locale().to_string(), "ja-JP");

    apply_bundled_translation(Some("pt-BR"));
    assert_eq!(
        rust_i18n::locale().to_string(),
        "pt-BR",
        "the Slint side alone is not enough — Rust-injected strings need this"
    );

    // No persisted language: the OS-derived fallback still lands somewhere.
    init_translations(None);
    assert!(!rust_i18n::locale().to_string().is_empty());

    // An unknown tag is redirected to a populated locale rather than blanking
    // the UI with empty msgstr, and never panics.
    init_translations(Some("xx-YY"));
    assert_eq!(rust_i18n::locale().to_string(), "en-US");
    apply_bundled_translation(Some("xx-YY"));
    assert_eq!(rust_i18n::locale().to_string(), "en-US");

    // Slint refuses to select a bundled translation before the first component
    // exists; the failure is logged, not fatal, and the Rust side still moved.
    apply_bundled_translation(Some("es-ES"));
    assert_eq!(rust_i18n::locale().to_string(), "es-ES");

    rust_i18n::set_locale(&original);
}
