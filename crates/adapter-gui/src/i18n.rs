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
/// Only pt-BR and en-US currently have real translations; the others are
/// listed so users can pick them, but render passthrough (source pt-BR)
/// until a translator fills the corresponding `.po` and `.yml` files.
/// Tracked as a follow-up issue.
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

#[derive(Debug, Clone, Copy)]
pub struct Language {
    pub code: &'static str,
    pub display: &'static str,
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

    if cfg!(debug_assertions) {
        let dev = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("translations");
        if has_any_mo(&dev) {
            return Some(dev);
        }
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

/// Initialize both catalogs:
/// - gettext (Slint side) via `bindtextdomain` + `setlocale` so `@tr(...)`
///   resolves against `<lang>/LC_MESSAGES/adapter-gui.mo`.
/// - rust-i18n (Rust side) via `rust_i18n::set_locale` so `t!("...")`
///   resolves against `crates/adapter-gui/locales/<lang>.yml`.
///
/// Failures are logged but never panic — translations are not load-bearing.
pub fn init_translations(persisted_language: Option<&str>) {
    let locale = resolve_locale(persisted_language);
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
    fn supported_languages_starts_with_auto_sentinel() {
        assert_eq!(SUPPORTED_LANGUAGES[0].code, "auto");
    }

    #[test]
    fn supported_languages_contains_source_and_english() {
        let codes: Vec<_> = SUPPORTED_LANGUAGES.iter().map(|l| l.code).collect();
        assert!(codes.contains(&"pt-BR"));
        assert!(codes.contains(&"en-US"));
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
