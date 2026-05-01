//! Internationalization runtime — locale detection, override, and i18n wiring.
//!
//! OpenRig has two translation catalogs because Slint's `@tr(...)` macro
//! only speaks gettext, while the Rust side uses the more idiomatic
//! `rust-i18n` framework (YAML-based, `t!()` macro, no external tools).
//!
//! Both catalogs are kept in sync by sharing the same locale code (pt-BR /
//! en-US). The selector in the UI sets the locale on both at startup.
//!
//! Source language (where the keys live) is pt-BR. Default UI language for
//! distribution is **en-US** — when the OS locale is not pt or en, the app
//! falls back to en-US, not to the Portuguese source. Portuguese variants
//! (pt-PT, pt-AO) route to pt-BR because we ship a Portuguese translation.
//!
//! See `docs/i18n.md` for the full flow and the rationale for two catalogs.

use std::path::{Path, PathBuf};

/// gettext text domain for the Slint side — must match Slint's default
/// (`CARGO_PKG_NAME`). End users never see this; it's just the .mo file
/// name on disk.
pub const TEXT_DOMAIN: &str = "adapter-gui";

/// Languages exposed in the language selector UI. Order matters:
/// - Index 0 is the "Auto" sentinel meaning "follow OS locale".
/// - The rest is alphabetical by Portuguese display name (matches the
///   codebase source language).
///
/// Only locales listed in [`LOCALES_WITH_TRANSLATIONS`] currently have
/// real translations; the others are kept in the selector so users see
/// them coming, but `effective_locale()` redirects them to en-US under
/// the hood until a translator fills the corresponding `.po` and `.yml`
/// files (otherwise Slint's bundled-translation runtime returns empty
/// strings for empty msgstr — the UI blanks out instead of falling back
/// to msgid like classic gettext).
pub const SUPPORTED_LANGUAGES: &[Language] = &[
    Language { code: "auto",  display: "Auto" },
    Language { code: "de-DE", display: "Alemão" },
    Language { code: "zh-CN", display: "Chinês" },
    Language { code: "ko-KR", display: "Coreano" },
    Language { code: "es-ES", display: "Espanhol" },
    Language { code: "fr-FR", display: "Francês" },
    Language { code: "hi-IN", display: "Hindi" },
    Language { code: "en-US", display: "Inglês (US)" },
    Language { code: "ja-JP", display: "Japonês" },
    Language { code: "pt-BR", display: "Português (Brasil)" },
];

/// Locales with real translations populated in both gettext (`.po`) and
/// rust-i18n (`.yml`) catalogs. Picking any other locale silently falls
/// back to en-US — see [`effective_locale`].
///
/// Add a code here as soon as its `.po` and `.yml` are fully populated.
pub const LOCALES_WITH_TRANSLATIONS: &[&str] = &[
    "pt-BR", "en-US", "de-DE", "es-ES", "fr-FR", "hi-IN", "ja-JP", "ko-KR", "zh-CN",
];

#[derive(Debug, Clone, Copy)]
pub struct Language {
    pub code: &'static str,
    pub display: &'static str,
}

/// Convert a requested locale into the one to actually activate. If the
/// locale isn't in [`LOCALES_WITH_TRANSLATIONS`], silently fall back to
/// en-US — Slint's bundled translations return empty strings for
/// skeleton `.po` files (all `msgstr ""`), which blanks every text in
/// the UI. Falling back to a populated locale keeps the app readable.
///
/// Stop-gap until each shipped locale has real translations. Once a
/// `.po` is populated, add its code to [`LOCALES_WITH_TRANSLATIONS`].
pub fn effective_locale(requested: &str) -> String {
    if LOCALES_WITH_TRANSLATIONS.iter().any(|c| *c == requested) {
        requested.to_string()
    } else {
        "en-US".to_string()
    }
}

/// Returns the human-readable name of a language for display in the
/// LanguageSelector dropdown, localized to the active UI locale. Each
/// shipped UI locale lists languages in its OWN script (Japanese UI =
/// Japanese names, Chinese UI = Chinese names, etc.).
///
/// Hand-curated tables instead of `@tr(...)` because the language list
/// is built in Rust and passed to Slint as a `[string]` model — the
/// gettext catalog is the wrong layer for it. Unknown locales fall
/// through `effective_locale` to en-US.
pub fn display_name(lang_code: &str, ui_locale: &str) -> &'static str {
    let active_ui = effective_locale(ui_locale);
    match active_ui.as_str() {
        "pt-BR" => match lang_code {
            "auto" => "Auto",
            "de-DE" => "Alemão",
            "zh-CN" => "Chinês",
            "ko-KR" => "Coreano",
            "es-ES" => "Espanhol",
            "fr-FR" => "Francês",
            "hi-IN" => "Hindi",
            "en-US" => "Inglês (US)",
            "ja-JP" => "Japonês",
            "pt-BR" => "Português (Brasil)",
            _ => echo_unknown(lang_code),
        },
        "de-DE" => match lang_code {
            "auto" => "Auto",
            "de-DE" => "Deutsch",
            "zh-CN" => "Chinesisch",
            "ko-KR" => "Koreanisch",
            "es-ES" => "Spanisch",
            "fr-FR" => "Französisch",
            "hi-IN" => "Hindi",
            "en-US" => "Englisch (US)",
            "ja-JP" => "Japanisch",
            "pt-BR" => "Portugiesisch (Brasilien)",
            _ => echo_unknown(lang_code),
        },
        "es-ES" => match lang_code {
            "auto" => "Auto",
            "de-DE" => "Alemán",
            "zh-CN" => "Chino",
            "ko-KR" => "Coreano",
            "es-ES" => "Español",
            "fr-FR" => "Francés",
            "hi-IN" => "Hindi",
            "en-US" => "Inglés (EE. UU.)",
            "ja-JP" => "Japonés",
            "pt-BR" => "Portugués (Brasil)",
            _ => echo_unknown(lang_code),
        },
        "fr-FR" => match lang_code {
            "auto" => "Auto",
            "de-DE" => "Allemand",
            "zh-CN" => "Chinois",
            "ko-KR" => "Coréen",
            "es-ES" => "Espagnol",
            "fr-FR" => "Français",
            "hi-IN" => "Hindi",
            "en-US" => "Anglais (US)",
            "ja-JP" => "Japonais",
            "pt-BR" => "Portugais (Brésil)",
            _ => echo_unknown(lang_code),
        },
        "hi-IN" => match lang_code {
            "auto" => "स्वत:",
            "de-DE" => "जर्मन",
            "zh-CN" => "चीनी",
            "ko-KR" => "कोरियाई",
            "es-ES" => "स्पेनिश",
            "fr-FR" => "फ्रेंच",
            "hi-IN" => "हिन्दी",
            "en-US" => "अंग्रेज़ी (US)",
            "ja-JP" => "जापानी",
            "pt-BR" => "पुर्तगाली (ब्राज़ील)",
            _ => echo_unknown(lang_code),
        },
        "ja-JP" => match lang_code {
            "auto" => "自動",
            "de-DE" => "ドイツ語",
            "zh-CN" => "中国語",
            "ko-KR" => "韓国語",
            "es-ES" => "スペイン語",
            "fr-FR" => "フランス語",
            "hi-IN" => "ヒンディー語",
            "en-US" => "英語 (US)",
            "ja-JP" => "日本語",
            "pt-BR" => "ポルトガル語 (ブラジル)",
            _ => echo_unknown(lang_code),
        },
        "ko-KR" => match lang_code {
            "auto" => "자동",
            "de-DE" => "독일어",
            "zh-CN" => "중국어",
            "ko-KR" => "한국어",
            "es-ES" => "스페인어",
            "fr-FR" => "프랑스어",
            "hi-IN" => "힌디어",
            "en-US" => "영어 (US)",
            "ja-JP" => "일본어",
            "pt-BR" => "포르투갈어 (브라질)",
            _ => echo_unknown(lang_code),
        },
        "zh-CN" => match lang_code {
            "auto" => "自动",
            "de-DE" => "德语",
            "zh-CN" => "中文",
            "ko-KR" => "韩语",
            "es-ES" => "西班牙语",
            "fr-FR" => "法语",
            "hi-IN" => "印地语",
            "en-US" => "英语 (US)",
            "ja-JP" => "日语",
            "pt-BR" => "葡萄牙语 (巴西)",
            _ => echo_unknown(lang_code),
        },
        // Default branch covers en-US plus any locale that fell back to
        // en-US via `effective_locale`.
        _ => match lang_code {
            "auto" => "Auto",
            "de-DE" => "German",
            "zh-CN" => "Chinese",
            "ko-KR" => "Korean",
            "es-ES" => "Spanish",
            "fr-FR" => "French",
            "hi-IN" => "Hindi",
            "en-US" => "English (US)",
            "ja-JP" => "Japanese",
            "pt-BR" => "Portuguese (Brazil)",
            _ => echo_unknown(lang_code),
        },
    }
}

/// Defensive fallback for an unknown lang code. The function only fires
/// when callers pass a code outside SUPPORTED_LANGUAGES — every supported
/// code is matched explicitly in `display_name`. Returning a literal "?"
/// keeps the UI showing *something* instead of nothing.
fn echo_unknown(_code: &str) -> &'static str {
    "?"
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
fn normalize(raw: &str) -> String {
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

/// Search the filesystem for the gettext catalog directory containing
/// `<lang>/LC_MESSAGES/adapter-gui.mo`. Same path order as before:
///
/// 1. `OPENRIG_TRANSLATIONS_DIR` env var (developer override)
/// 2. `<exec_dir>/translations` (Windows next-to-exe, Mac.app/Resources)
/// 3. `<exec_dir>/../share/openrig/translations` (Linux FHS / .deb)
/// 4. `<exec_dir>/../Resources/translations` (Mac.app fallback)
/// 5. `CARGO_MANIFEST_DIR/translations` (debug builds running via `cargo run`)
pub fn resolve_translations_dir() -> Option<PathBuf> {
    if let Ok(env_dir) = std::env::var("OPENRIG_TRANSLATIONS_DIR") {
        let p = PathBuf::from(env_dir);
        if has_any_mo(&p) {
            return Some(p);
        }
    }

    let exec = std::env::current_exe().ok()?;
    let exec_dir = exec.parent()?;

    let candidates = [
        exec_dir.join("translations"),
        exec_dir
            .join("..")
            .join("share")
            .join("openrig")
            .join("translations"),
        exec_dir.join("..").join("Resources").join("translations"),
    ];
    for c in &candidates {
        if has_any_mo(c) {
            return Some(c.clone());
        }
    }

    // CARGO_MANIFEST_DIR is the absolute path to the crate at compile time.
    // On the developer machine it's the source tree and the .mo files live
    // there next to the .po. On a user's machine after a deb/dmg install
    // that path doesn't exist — we check existence before trusting it, so
    // it's safe to attempt outside debug builds too.
    // Without this, `cargo run --release` (or any non-bundled run) would
    // skip every candidate and dgettext would fall back to the msgid,
    // surfacing as "BTN-NEW-PROJECT" leaking into the UI.
    let dev = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("translations");
    if has_any_mo(&dev) {
        return Some(dev);
    }

    None
}

fn has_any_mo(dir: &Path) -> bool {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return false;
    };
    for entry in rd.flatten() {
        let mo = entry
            .path()
            .join("LC_MESSAGES")
            .join(format!("{}.mo", TEXT_DOMAIN));
        if mo.exists() {
            return true;
        }
    }
    false
}

/// Apply the resolved locale to Slint's bundled translations. Must be
/// called AFTER `AppWindow::new()` — Slint requires the first component
/// to exist before bundled translations can be selected.
///
/// Locale code in BCP 47 form (e.g. `pt-BR`, `en-US`) is converted to
/// the POSIX form (`pt_BR`, `en_US`) that the bundled translations are
/// indexed by — this matches the `<lang>/adapter-gui.po` filenames the
/// `slint_build::with_bundled_translations` ingested at compile time.
pub fn apply_bundled_translation(persisted_language: Option<&str>) {
    let locale = locale_for_runtime(persisted_language);
    // CRITICAL: switch BOTH catalogs. Slint's bundled translations only
    // covers strings that flow through @tr(...). Strings that Rust injects
    // into the Slint window via setters (set_chain_editor_title, etc.)
    // resolve through rust_i18n::t!() and need their own locale switch —
    // without this, those properties stay frozen in the boot locale and
    // surface as 'Salvar chain' when the rest of the UI is in Japanese.
    rust_i18n::set_locale(&locale);
    let posix = locale.replace('-', "_");
    match slint::select_bundled_translation(&posix) {
        Ok(()) => log::info!("i18n: slint bundled translation = {}", posix),
        Err(e) => log::warn!("i18n: slint select_bundled_translation({}) failed: {}", posix, e),
    }
}

/// Initialize both catalogs:
/// - gettext (Slint side) via `bindtextdomain` + `setlocale` so `@tr(...)`
///   resolves against `<lang>/LC_MESSAGES/adapter-gui.mo`.
/// - rust-i18n (Rust side) via `rust_i18n::set_locale` so `t!("...")`
///   resolves against `crates/adapter-gui/locales/<lang>.yml`.
///
/// Failures are logged but never panic — translations are not load-bearing.
pub fn init_translations(persisted_language: Option<&str>) {
    let locale = locale_for_runtime(persisted_language);
    log::info!("i18n: resolved locale = {}", locale);

    // Rust side: rust-i18n. The `i18n!("locales")` macro at crate root
    // already loaded the YAML catalogs at compile time; we just need to
    // pick which locale is active.
    rust_i18n::set_locale(&locale);

    // Slint side: gettext.
    //
    // gettext picks the active language from environment vars first
    // (LANGUAGE, then LC_ALL, then LC_MESSAGES, then LANG). setlocale
    // alone is NOT enough on macOS / glibc-less platforms: libintl re-reads
    // the env on each lookup and ignores in-process setlocale changes.
    //
    // We set LANGUAGE explicitly because it has the highest priority and
    // works consistently on Linux/macOS/Windows. This is safe — we own
    // the process and only adjust at startup, before any UI renders.
    use gettextrs::{bindtextdomain, setlocale, textdomain, LocaleCategory};

    let posix = locale.replace('-', "_");
    std::env::set_var("LANGUAGE", &posix);

    let target = format!("{}.UTF-8", posix);
    let applied = setlocale(LocaleCategory::LcMessages, target.clone());
    if applied.is_none() {
        if setlocale(LocaleCategory::LcMessages, posix.clone()).is_none() {
            log::warn!(
                "i18n: setlocale rejected {:?} and {:?} — Slint translations \
                 will rely on the LANGUAGE env var only",
                target, posix
            );
        }
    }

    match resolve_translations_dir() {
        Some(dir) => {
            log::info!("i18n: gettext translations dir = {}", dir.display());
            if let Err(e) = bindtextdomain(TEXT_DOMAIN, dir) {
                log::warn!("i18n: bindtextdomain failed: {}", e);
            }
        }
        None => {
            log::info!("i18n: no gettext translations dir found, Slint will passthrough source");
        }
    }

    if let Err(e) = textdomain(TEXT_DOMAIN) {
        log::warn!("i18n: textdomain failed: {}", e);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_handles_posix_utf8_suffix() {
        assert_eq!(normalize("pt_BR.UTF-8"), "pt-BR");
        assert_eq!(normalize("en_US.UTF-8"), "en-US");
    }

    #[test]
    fn normalize_handles_dash_form() {
        assert_eq!(normalize("pt-BR"), "pt-BR");
        assert_eq!(normalize("en-US"), "en-US");
    }

    #[test]
    fn normalize_falls_back_to_english_for_unsupported() {
        // Unknown locales (Latin "xx-YY", empty) fall back to en-US — the
        // default UI language for distribution.
        assert_eq!(normalize("xx-YY"), "en-US");
        assert_eq!(normalize(""), "en-US");
    }

    #[test]
    fn normalize_collapses_regional_variants() {
        // pt-PT, pt-AO → pt-BR (Portuguese family)
        assert_eq!(normalize("pt_PT"), "pt-BR");
        assert_eq!(normalize("pt-AO"), "pt-BR");
        // en-GB → en-US
        assert_eq!(normalize("en_GB"), "en-US");
        // zh-TW → zh-CN
        assert_eq!(normalize("zh_TW"), "zh-CN");
        // es-419 → es-ES
        assert_eq!(normalize("es-419"), "es-ES");
    }

    #[test]
    fn normalize_routes_each_supported_language_family() {
        assert_eq!(normalize("de_DE"), "de-DE");
        assert_eq!(normalize("fr_FR"), "fr-FR");
        assert_eq!(normalize("es_ES"), "es-ES");
        assert_eq!(normalize("ja_JP"), "ja-JP");
        assert_eq!(normalize("ko_KR"), "ko-KR");
        assert_eq!(normalize("zh_CN"), "zh-CN");
        assert_eq!(normalize("hi_IN"), "hi-IN");
    }

    #[test]
    fn normalize_routes_italian_to_spanish() {
        // We don't ship Italian; route to es-ES as the closest Romance
        // language. Better than dropping to en-US.
        assert_eq!(normalize("it_IT"), "es-ES");
    }

    #[test]
    fn resolve_locale_prefers_persisted_when_not_auto() {
        assert_eq!(resolve_locale(Some("en-US")), "en-US");
        assert_eq!(resolve_locale(Some("pt-BR")), "pt-BR");
    }

    fn is_supported_code(code: &str) -> bool {
        SUPPORTED_LANGUAGES
            .iter()
            .any(|l| l.code == code && l.code != "auto")
    }

    #[test]
    fn resolve_locale_treats_auto_as_unset() {
        // Result depends on the test host's OS locale — accept any of our
        // shipped codes (the resolver should never echo "auto" back).
        let result = resolve_locale(Some("auto"));
        assert!(
            is_supported_code(&result),
            "auto resolution returned unsupported code: {}",
            result
        );
    }

    #[test]
    fn resolve_locale_treats_empty_as_unset() {
        let result = resolve_locale(Some(""));
        assert!(is_supported_code(&result));
    }

    #[test]
    fn effective_locale_passes_through_pt_br() {
        assert_eq!(effective_locale("pt-BR"), "pt-BR");
    }

    #[test]
    fn effective_locale_passes_through_en_us() {
        assert_eq!(effective_locale("en-US"), "en-US");
    }

    /// All 9 shipped locales now have populated translations. They must
    /// pass through `effective_locale` unchanged. Regression test for the
    /// "blank UI" bug: if a `.po` ever regresses to skeleton-only state,
    /// remove its code from LOCALES_WITH_TRANSLATIONS — never let an
    /// empty `.po` reach Slint's runtime, which renders msgstr "" as
    /// empty strings (not msgid fallback).
    #[test]
    fn effective_locale_passes_through_every_shipped_locale() {
        for code in &["pt-BR", "en-US", "de-DE", "es-ES", "fr-FR", "hi-IN", "ja-JP", "ko-KR", "zh-CN"] {
            assert_eq!(
                effective_locale(code),
                *code,
                "locale {:?} should pass through to Slint",
                code
            );
        }
    }

    /// A locale code outside SUPPORTED_LANGUAGES must fall back to en-US
    /// rather than crash or pass an unsupported value to Slint.
    #[test]
    fn effective_locale_falls_back_for_unknown_code() {
        assert_eq!(effective_locale("xx-YY"), "en-US");
        assert_eq!(effective_locale(""), "en-US");
    }

    /// Composes `resolve_locale` (BCP47 normalization) with `effective_locale`
    /// (skeleton fallback). This is the single function `apply_bundled_translation`
    /// and `init_translations` must use to decide what locale Slint and gettext
    /// should activate. Today every shipped locale is populated so every code
    /// passes through; the fallback path triggers only for codes outside
    /// SUPPORTED_LANGUAGES (defensive).
    #[test]
    fn locale_for_runtime_passes_through_every_shipped_locale() {
        for code in &["pt-BR", "en-US", "de-DE", "es-ES", "fr-FR", "hi-IN", "ja-JP", "ko-KR", "zh-CN"] {
            assert_eq!(locale_for_runtime(Some(code)), *code);
        }
    }

    /// OS locale strings come from `sys_locale::get_locale` in POSIX form
    /// like "fr_FR.UTF-8". `locale_for_runtime` must normalize them into
    /// the canonical BCP 47 code (`fr-FR`) before applying the fallback.
    #[test]
    fn locale_for_runtime_normalizes_posix_input() {
        assert_eq!(locale_for_runtime(Some("fr_FR.UTF-8")), "fr-FR");
        assert_eq!(locale_for_runtime(Some("ja_JP")), "ja-JP");
        assert_eq!(locale_for_runtime(Some("pt_BR.UTF-8")), "pt-BR");
    }

    /// A locale code outside SUPPORTED_LANGUAGES must redirect to en-US.
    #[test]
    fn locale_for_runtime_falls_back_for_unknown_code() {
        // "xx-YY" normalizes to "xx-YY" (unknown), then falls back via
        // resolve_locale's normalize → "en-US"; effective_locale passes
        // it through. End-to-end: "en-US".
        assert_eq!(locale_for_runtime(Some("xx-YY")), "en-US");
    }

    /// Sanity rail: every code in `LOCALES_WITH_TRANSLATIONS` must be a
    /// valid entry in `SUPPORTED_LANGUAGES`. Catches typos that would
    /// silently break the fallback.
    #[test]
    fn locales_with_translations_are_all_in_supported_list() {
        let supported: Vec<&str> = SUPPORTED_LANGUAGES.iter().map(|l| l.code).collect();
        for code in LOCALES_WITH_TRANSLATIONS {
            assert!(
                supported.contains(code),
                "{:?} is in LOCALES_WITH_TRANSLATIONS but not in SUPPORTED_LANGUAGES",
                code
            );
        }
    }

    /// The list of languages exposed in the LanguageSelector UI must be
    /// localized to the active UI language. Showing "Alemão" / "Chinês"
    /// when the rest of the UI is in English is jarring (and was reported
    /// by the user). For each shipped UI locale, every language in
    /// SUPPORTED_LANGUAGES has a localized display name.
    /// Each shipped UI locale must list the languages in its OWN script.
    /// Showing pt-BR names while the UI is in Japanese (or English names
    /// while the UI is in Korean) is jarring and was reported by the user.
    #[test]
    fn display_name_localizes_into_de_de() {
        assert_eq!(display_name("de-DE", "de-DE"), "Deutsch");
        assert_eq!(display_name("zh-CN", "de-DE"), "Chinesisch");
        assert_eq!(display_name("ja-JP", "de-DE"), "Japanisch");
        assert_eq!(display_name("auto", "de-DE"), "Auto");
    }

    #[test]
    fn display_name_localizes_into_es_es() {
        assert_eq!(display_name("de-DE", "es-ES"), "Alemán");
        assert_eq!(display_name("es-ES", "es-ES"), "Español");
        assert_eq!(display_name("ja-JP", "es-ES"), "Japonés");
    }

    #[test]
    fn display_name_localizes_into_fr_fr() {
        assert_eq!(display_name("de-DE", "fr-FR"), "Allemand");
        assert_eq!(display_name("fr-FR", "fr-FR"), "Français");
        assert_eq!(display_name("zh-CN", "fr-FR"), "Chinois");
    }

    #[test]
    fn display_name_localizes_into_hi_in() {
        assert_eq!(display_name("hi-IN", "hi-IN"), "हिन्दी");
        assert_eq!(display_name("de-DE", "hi-IN"), "जर्मन");
    }

    #[test]
    fn display_name_localizes_into_ja_jp() {
        assert_eq!(display_name("ja-JP", "ja-JP"), "日本語");
        assert_eq!(display_name("zh-CN", "ja-JP"), "中国語");
        assert_eq!(display_name("de-DE", "ja-JP"), "ドイツ語");
        assert_eq!(display_name("auto", "ja-JP"), "自動");
    }

    #[test]
    fn display_name_localizes_into_ko_kr() {
        assert_eq!(display_name("ko-KR", "ko-KR"), "한국어");
        assert_eq!(display_name("ja-JP", "ko-KR"), "일본어");
        assert_eq!(display_name("auto", "ko-KR"), "자동");
    }

    #[test]
    fn display_name_localizes_into_zh_cn() {
        assert_eq!(display_name("zh-CN", "zh-CN"), "中文");
        assert_eq!(display_name("ja-JP", "zh-CN"), "日语");
        assert_eq!(display_name("auto", "zh-CN"), "自动");
    }

    #[test]
    fn display_name_localizes_into_pt_br() {
        assert_eq!(display_name("auto", "pt-BR"), "Auto");
        assert_eq!(display_name("de-DE", "pt-BR"), "Alemão");
        assert_eq!(display_name("zh-CN", "pt-BR"), "Chinês");
        assert_eq!(display_name("ko-KR", "pt-BR"), "Coreano");
        assert_eq!(display_name("es-ES", "pt-BR"), "Espanhol");
        assert_eq!(display_name("fr-FR", "pt-BR"), "Francês");
        assert_eq!(display_name("hi-IN", "pt-BR"), "Hindi");
        assert_eq!(display_name("en-US", "pt-BR"), "Inglês (US)");
        assert_eq!(display_name("ja-JP", "pt-BR"), "Japonês");
        assert_eq!(display_name("pt-BR", "pt-BR"), "Português (Brasil)");
    }

    #[test]
    fn display_name_localizes_into_en_us() {
        assert_eq!(display_name("auto", "en-US"), "Auto");
        assert_eq!(display_name("de-DE", "en-US"), "German");
        assert_eq!(display_name("zh-CN", "en-US"), "Chinese");
        assert_eq!(display_name("ko-KR", "en-US"), "Korean");
        assert_eq!(display_name("es-ES", "en-US"), "Spanish");
        assert_eq!(display_name("fr-FR", "en-US"), "French");
        assert_eq!(display_name("hi-IN", "en-US"), "Hindi");
        assert_eq!(display_name("en-US", "en-US"), "English (US)");
        assert_eq!(display_name("ja-JP", "en-US"), "Japanese");
        assert_eq!(display_name("pt-BR", "en-US"), "Portuguese (Brazil)");
    }

    /// Defensive: if a UI locale is outside SUPPORTED_LANGUAGES,
    /// `effective_locale` redirects to en-US, and `display_name` uses
    /// English names for the dropdown.
    #[test]
    fn display_name_falls_back_to_english_for_unsupported_ui_locale() {
        assert_eq!(display_name("de-DE", "xx-YY"), "German");
        assert_eq!(display_name("ja-JP", ""), "Japanese");
    }

    #[test]
    fn display_name_unknown_lang_code_returns_placeholder() {
        // Defensive: an unknown lang code should not panic, just return
        // a placeholder ("?") so the UI shows SOMETHING. Function returns
        // `&'static str`, so we can't echo arbitrary input.
        assert_eq!(display_name("xx-YY", "en-US"), "?");
    }

    #[test]
    fn supported_languages_starts_with_auto_sentinel() {
        assert_eq!(SUPPORTED_LANGUAGES[0].code, "auto");
    }

    #[test]
    fn supported_languages_contains_source_and_english() {
        let codes: Vec<_> = SUPPORTED_LANGUAGES.iter().map(|l| l.code).collect();
        assert!(codes.contains(&"pt-BR"));
        assert!(codes.contains(&"en-US"));
    }

    /// `resolve_translations_dir` must find catalogs regardless of build
    /// profile. Earlier the CARGO_MANIFEST_DIR fallback was gated on
    /// `cfg!(debug_assertions)`, which made `cargo run --release` (or any
    /// non-bundled run) skip every candidate, so dgettext silently echoed
    /// the msgid back — surfacing as "BTN-NEW-PROJECT" in the UI.
    #[test]
    fn resolve_translations_dir_finds_source_tree_in_any_profile() {
        let dir = resolve_translations_dir();
        assert!(
            dir.is_some(),
            "resolve_translations_dir returned None — gettext lookup will \
             fail. Likely cause: CARGO_MANIFEST_DIR fallback was gated \
             behind cfg!(debug_assertions) again."
        );
        let path = dir.unwrap();
        assert!(
            path.join("pt_BR").join("LC_MESSAGES").join("adapter-gui.mo").exists()
                || path.join("en_US").join("LC_MESSAGES").join("adapter-gui.mo").exists(),
            "resolve_translations_dir returned {:?} but it has no .mo files",
            path
        );
    }

    /// gettext on Unix derives the catalog path from `setlocale(LC_MESSAGES,
    /// "en_US.UTF-8")` and looks under `<dir>/en_US/LC_MESSAGES/<domain>.mo` —
    /// underscore, not hyphen. If we ever add a hyphenated locale dir again,
    /// dgettext silently echoes back the msgid (the bug behind the original
    /// "BTN-NEW-PROJECT" leak).
    #[test]
    fn translation_dirs_use_posix_underscore_not_bcp47_hyphen() {
        let translations = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("translations");
        let entries = std::fs::read_dir(&translations)
            .expect("crates/adapter-gui/translations/ must exist");
        let mut hyphenated = Vec::new();
        let mut posix = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let Some(name) = path.file_name().and_then(|s| s.to_str()) else {
                continue;
            };
            if name.contains('-') {
                hyphenated.push(name.to_string());
            } else if name.contains('_') {
                posix.push(name.to_string());
            }
        }
        assert!(
            hyphenated.is_empty(),
            "Found BCP 47 hyphen locale dirs: {:?}. \
             gettext lookup will fail for these — rename to POSIX underscore \
             (e.g. pt_BR, en_US). Coexistent POSIX dirs: {:?}",
            hyphenated, posix
        );
        assert!(
            !posix.is_empty(),
            "No POSIX locale dirs found under translations/ — pipeline broken"
        );
    }

    /// End-to-end: bind the catalog, set locale, perform the same dgettext
    /// call Slint runtime makes for `@tr("btn-new-project")` inside the
    /// `ProjectLauncherPage` component, and assert dgettext returns the
    /// translation, NOT the mangled msgid.
    ///
    /// Marked `#[ignore]` because:
    ///   1. It mutates global gettext state, which conflicts with parallel
    ///      tests running on the same process.
    ///   2. It requires `.mo` files to exist on disk (build.rs writes them).
    ///      `cargo test --workspace` runs the build, but isolated invocations
    ///      that skip the build (rare) would fail spuriously.
    /// Run with `cargo test -p adapter-gui --lib gettext_resolves -- --ignored`.
    #[test]
    #[ignore = "mutates global gettext state; run with --ignored"]
    fn gettext_resolves_btn_new_project_in_en_us() {
        use gettextrs::{bindtextdomain, dgettext, setlocale, textdomain, LocaleCategory};

        let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("translations");
        std::env::set_var("LANGUAGE", "en_US");
        bindtextdomain(TEXT_DOMAIN, dir).expect("bindtextdomain");
        textdomain(TEXT_DOMAIN).expect("textdomain");
        setlocale(LocaleCategory::LcMessages, "en_US.UTF-8");

        // The mangled msgid is exactly what i-slint-core emits for
        // @tr("btn-new-project") inside ProjectLauncherPage:
        // <ctx>\u{4}<msgid>
        let mangled = format!("ProjectLauncherPage\u{4}btn-new-project");
        let result = dgettext(TEXT_DOMAIN, mangled.clone());

        assert_ne!(
            result, mangled,
            "dgettext echoed back the mangled msgid — translation lookup \
             FAILED. This is the bug that surfaces as 'BTN-NEW-PROJECT' \
             leaking into the UI. Likely cause: dir layout regressed to \
             BCP 47 hyphen, or .mo file missing the key."
        );
        // Slint demangles by taking the part after \u{4}.
        let demangled = result.split('\u{4}').last().unwrap_or(&result);
        assert_eq!(
            demangled, "New project",
            "Translation resolved but does not match expected en-US value"
        );
    }
}
