//! Internationalization runtime — locale detection, override, and gettext wiring.
//!
//! OpenRig translations follow the gettext convention: source-language strings
//! are marked with `@tr("...")` in Slint and looked up against `<lang>/
//! LC_MESSAGES/openrig.mo` at runtime via the gettext shared library.
//!
//! Source language is pt-BR. Fallback is passthrough (the source string).

use std::path::{Path, PathBuf};

/// gettext text domain — must match Slint's default (CARGO_PKG_NAME).
/// End users never see this; it's just the .mo file name on disk.
pub const TEXT_DOMAIN: &str = "adapter-gui";

/// Languages exposed in the language selector UI. Order matters — index 0 is
/// the "Auto" sentinel meaning "follow OS locale".
pub const SUPPORTED_LANGUAGES: &[Language] = &[
    Language {
        code: "auto",
        display: "Auto",
    },
    Language {
        code: "pt-BR",
        display: "Português (Brasil)",
    },
    Language {
        code: "en-US",
        display: "English (US)",
    },
];

#[derive(Debug, Clone, Copy)]
pub struct Language {
    pub code: &'static str,
    pub display: &'static str,
}

/// Resolve the locale we should activate. Order:
///   1. Explicit non-"auto" value persisted in `gui-settings.yaml`
///   2. OS locale (sys-locale)
///   3. Fallback to "pt-BR" (the source language)
pub fn resolve_locale(persisted: Option<&str>) -> String {
    if let Some(code) = persisted {
        if !code.is_empty() && !code.eq_ignore_ascii_case("auto") {
            return normalize(code);
        }
    }
    sys_locale::get_locale()
        .map(|s| normalize(&s))
        .unwrap_or_else(|| "pt-BR".to_string())
}

/// Normalize OS locale strings ("en_US.UTF-8", "pt_BR") to our canonical form
/// ("en-US", "pt-BR"). Unsupported locales fall back to the source language.
fn normalize(raw: &str) -> String {
    let head = raw.split('.').next().unwrap_or(raw);
    let head = head.replace('_', "-");
    let lower_lang = head.split('-').next().unwrap_or("").to_ascii_lowercase();

    match lower_lang.as_str() {
        "pt" => "pt-BR".to_string(),
        "en" => "en-US".to_string(),
        _ => "pt-BR".to_string(),
    }
}

/// Search the filesystem for a directory containing `<lang>/LC_MESSAGES/
/// openrig.mo`. Search order matches platform conventions:
///
/// 1. `OPENRIG_TRANSLATIONS_DIR` env var (developer override)
/// 2. `<exec_dir>/translations` (Windows next-to-exe, Mac.app/Resources)
/// 3. `<exec_dir>/../share/openrig/translations` (Linux FHS / .deb)
/// 4. `<exec_dir>/../Resources/translations` (Mac.app fallback)
/// 5. `CARGO_MANIFEST_DIR/translations` (debug builds running via `cargo run`)
///
/// Returns `None` when no `.mo` files can be found — gettext then returns the
/// source string (passthrough), which is acceptable for the source language.
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

/// Initialize gettext: bind text domain to the translations directory, set
/// active locale. Slint's `@tr(...)` looks up via `gettext()` once
/// `slint = { features = ["gettext"] }` is enabled, so this single call is
/// enough to localize the entire UI.
///
/// Failures are logged but never panic — translations are not load-bearing.
pub fn init_translations(persisted_language: Option<&str>) {
    let locale = resolve_locale(persisted_language);
    log::info!("i18n: resolved locale = {}", locale);

    use gettextrs::{bindtextdomain, setlocale, textdomain, LocaleCategory};

    let target = format!("{}.UTF-8", locale.replace('-', "_"));
    let applied = setlocale(LocaleCategory::LcMessages, target.clone());
    if applied.is_none() {
        // Some POSIX systems don't have the .UTF-8 alias compiled — try bare.
        let bare = locale.replace('-', "_");
        if setlocale(LocaleCategory::LcMessages, bare.clone()).is_none() {
            log::warn!(
                "i18n: setlocale rejected {:?} and {:?} — translations may not load",
                target,
                bare
            );
        }
    }

    match resolve_translations_dir() {
        Some(dir) => {
            log::info!("i18n: translations dir = {}", dir.display());
            if let Err(e) = bindtextdomain(TEXT_DOMAIN, dir) {
                log::warn!("i18n: bindtextdomain failed: {}", e);
            }
        }
        None => {
            log::info!("i18n: no translations dir found, using passthrough (source language)");
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
    fn normalize_falls_back_to_source_language_for_unsupported() {
        assert_eq!(normalize("ja_JP"), "pt-BR");
        assert_eq!(normalize("xx-YY"), "pt-BR");
        assert_eq!(normalize(""), "pt-BR");
    }

    #[test]
    fn normalize_collapses_pt_pt_to_pt_br() {
        // We only ship pt-BR; pt-PT should not silently drop to source either —
        // we route both Portuguese variants to pt-BR to maximize coverage.
        assert_eq!(normalize("pt_PT"), "pt-BR");
    }

    #[test]
    fn resolve_locale_prefers_persisted_when_not_auto() {
        assert_eq!(resolve_locale(Some("en-US")), "en-US");
        assert_eq!(resolve_locale(Some("pt-BR")), "pt-BR");
    }

    #[test]
    fn resolve_locale_treats_auto_as_unset() {
        // We can't pin the OS locale in the test environment, but we can
        // verify that "auto" is not echoed back literally.
        let result = resolve_locale(Some("auto"));
        assert!(
            result == "pt-BR" || result == "en-US",
            "auto resolution returned unexpected value: {}",
            result
        );
    }

    #[test]
    fn resolve_locale_treats_empty_as_unset() {
        let result = resolve_locale(Some(""));
        assert!(result == "pt-BR" || result == "en-US");
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
}
