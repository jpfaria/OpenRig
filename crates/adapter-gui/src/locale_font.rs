//! Responsibility: picks the font a locale needs.

use crate::locale_resolve::locale_for_runtime;

/// Pick the macOS font family that has the right glyph coverage for a
/// given locale. The Slint global `Locale.font-family` is bound to this
/// at boot and on every change-language so each script renders against
/// a face that actually contains its codepoints (no .notdef / tofu).
///
/// Latin locales keep "Bebas Neue" (the project's display font). CJK and
/// Devanagari locales pick the macOS-native face for their script. Empty
/// string at the end means "fall through to the system default" — a
/// safe last resort that activates the macOS font cascade.
pub fn font_family_for_locale(locale: &str) -> &'static str {
    match locale {
        "ja-JP" => "Hiragino Sans",
        // Hiragino Sans GB is the Simplified-Chinese sibling of Hiragino
        // Sans on macOS — covers zh-Hans-only codepoints (乐 贝 键 输 链)
        // that Hiragino Sans (CJK Unified only) renders as .notdef.
        "zh-CN" => "Hiragino Sans GB",
        "ko-KR" => "Apple SD Gothic Neo",
        "hi-IN" => "Kohinoor Devanagari",
        // pt-BR, en-US, es-ES, fr-FR, de-DE — all Latin
        _ => "Bebas Neue",
    }
}

/// Resolve the font family that the persisted/auto locale should use.
/// Pure helper for callers that want to seed Locale.font-family on a
/// freshly-created Slint Window without re-deriving the locale.
pub fn font_for_persisted_runtime() -> &'static str {
    let persisted = infra_filesystem::FilesystemStorage::load_gui_audio_settings()
        .ok()
        .flatten()
        .and_then(|s| s.language);
    let locale = locale_for_runtime(persisted.as_deref());
    font_family_for_locale(&locale)
}
