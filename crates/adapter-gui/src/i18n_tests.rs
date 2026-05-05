
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
    for code in &[
        "pt-BR", "en-US", "de-DE", "es-ES", "fr-FR", "hi-IN", "ja-JP", "ko-KR", "zh-CN",
    ] {
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
    for code in &[
        "pt-BR", "en-US", "de-DE", "es-ES", "fr-FR", "hi-IN", "ja-JP", "ko-KR", "zh-CN",
    ] {
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
#[cfg(target_os = "linux")]
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
        path.join("pt_BR")
            .join("LC_MESSAGES")
            .join("adapter-gui.mo")
            .exists()
            || path
                .join("en_US")
                .join("LC_MESSAGES")
                .join("adapter-gui.mo")
                .exists(),
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
    let translations = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("translations");
    let entries =
        std::fs::read_dir(&translations).expect("crates/adapter-gui/translations/ must exist");
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
        hyphenated,
        posix
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
#[cfg(target_os = "linux")]
fn gettext_resolves_btn_new_project_in_en_us() {
    use gettextrs::{bindtextdomain, dgettext, setlocale, textdomain, LocaleCategory};

    let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("translations");
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
