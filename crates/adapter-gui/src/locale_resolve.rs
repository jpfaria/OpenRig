//! Responsibility: decides which locale the app runs in.

/// Locales with real translations populated in both gettext (`.po`) and
/// rust-i18n (`.yml`) catalogs. Picking any other locale silently falls
/// back to en-US — see [`effective_locale`].
///
/// Add a code here as soon as its `.po` and `.yml` are fully populated.
pub const LOCALES_WITH_TRANSLATIONS: &[&str] = &[
    "pt-BR", "en-US", "de-DE", "es-ES", "fr-FR", "hi-IN", "ja-JP", "ko-KR", "zh-CN",
];

/// Convert a requested locale into the one to actually activate. If the
/// locale isn't in [`LOCALES_WITH_TRANSLATIONS`], silently fall back to
/// en-US — Slint's bundled translations return empty strings for
/// skeleton `.po` files (all `msgstr ""`), which blanks every text in
/// the UI. Falling back to a populated locale keeps the app readable.
///
/// Stop-gap until each shipped locale has real translations. Once a
/// `.po` is populated, add its code to [`LOCALES_WITH_TRANSLATIONS`].
pub fn effective_locale(requested: &str) -> String {
    if LOCALES_WITH_TRANSLATIONS.contains(&requested) {
        requested.to_string()
    } else {
        "en-US".to_string()
    }
}

/// Single-source-of-truth for "what locale should Slint and gettext
/// actually use right now": resolves the persisted/auto preference into
/// a canonical BCP 47 code, then routes any skeleton-translation locale
/// to en-US so the UI stays readable. Both `apply_bundled_translation`
/// and `init_translations` MUST go through this function.
pub fn locale_for_runtime(persisted: Option<&str>) -> String {
    let resolved = resolve_locale(persisted);
    effective_locale(&resolved)
}

/// Resolve the locale we should activate. Order:
///   1. Explicit non-"auto" value persisted in `gui-settings.yaml`
///   2. OS locale (sys-locale)
///   3. Fallback to "en-US" — the default UI language for distribution
pub fn resolve_locale(persisted: Option<&str>) -> String {
    if let Some(code) = persisted {
        if !code.is_empty() && !code.eq_ignore_ascii_case("auto") {
            return normalize(code);
        }
    }
    sys_locale::get_locale()
        .map(|s| normalize(&s))
        .unwrap_or_else(|| "en-US".to_string())
}

/// Normalize OS locale strings ("en_US.UTF-8", "pt_BR") to one of the
/// supported canonical codes. Unsupported locales fall back to "en-US" —
/// English is the default UI language.
///
/// Regional variants collapse to the closest shipped translation: pt-PT
/// routes to pt-BR, en-GB routes to en-US, zh-TW routes to zh-CN, and so
/// on. This is "best-effort coverage" rather than dialectal accuracy.
pub(crate) fn normalize(raw: &str) -> String {
    let head = raw.split('.').next().unwrap_or(raw);
    let head = head.replace('_', "-");
    let lower_lang = head.split('-').next().unwrap_or("").to_ascii_lowercase();

    match lower_lang.as_str() {
        "pt" => "pt-BR".to_string(),
        "en" => "en-US".to_string(),
        "es" => "es-ES".to_string(),
        "fr" => "fr-FR".to_string(),
        "de" => "de-DE".to_string(),
        "it" => "es-ES".to_string(), // closest Romance language we ship
        "ja" => "ja-JP".to_string(),
        "ko" => "ko-KR".to_string(),
        "zh" => "zh-CN".to_string(),
        "hi" => "hi-IN".to_string(),
        _ => "en-US".to_string(),
    }
}
