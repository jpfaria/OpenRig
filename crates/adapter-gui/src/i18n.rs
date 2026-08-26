//! Responsibility: runs the translation lookup at runtime.
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

#[cfg(target_os = "linux")]
use std::path::{Path, PathBuf};

/// gettext text domain for the Slint side — must match Slint's default
/// (`CARGO_PKG_NAME`). End users never see this; it's just the .mo file
/// name on disk. Linux-only: Windows/macOS rely on bundled translations.
#[cfg(target_os = "linux")]
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
    Language {
        code: "auto",
        display: "Auto",
    },
    Language {
        code: "de-DE",
        display: "Alemão",
    },
    Language {
        code: "zh-CN",
        display: "Chinês",
    },
    Language {
        code: "ko-KR",
        display: "Coreano",
    },
    Language {
        code: "es-ES",
        display: "Espanhol",
    },
    Language {
        code: "fr-FR",
        display: "Francês",
    },
    Language {
        code: "hi-IN",
        display: "Hindi",
    },
    Language {
        code: "en-US",
        display: "Inglês (US)",
    },
    Language {
        code: "ja-JP",
        display: "Japonês",
    },
    Language {
        code: "pt-BR",
        display: "Português (Brasil)",
    },
];

pub use crate::language_names::{display_name, Language};
pub use crate::locale_font::{font_family_for_locale, font_for_persisted_runtime};
#[cfg(test)]
pub(crate) use crate::locale_resolve::normalize;
#[cfg(test)]
pub use crate::locale_resolve::{effective_locale, LOCALES_WITH_TRANSLATIONS};
pub use crate::locale_resolve::{locale_for_runtime, resolve_locale};
#[cfg(target_os = "linux")]
pub use crate::translations_dir::resolve_translations_dir;

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
        Err(e) => log::warn!(
            "i18n: slint select_bundled_translation({}) failed: {}",
            posix,
            e
        ),
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
    // We set LANGUAGE explicitly because it has the highest priority where
    // libintl is available. Gated to Linux because gettext-rs / libintl
    // are not first-class on Windows/macOS in our build matrix; on those
    // platforms Slint @tr(...) falls back to the bundled translations
    // already activated by `apply_bundled_translation`, and rust-i18n
    // continues to drive Rust-side strings.
    #[cfg(target_os = "linux")]
    {
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
                    target,
                    posix
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
                log::info!(
                    "i18n: no gettext translations dir found, Slint will passthrough source"
                );
            }
        }

        if let Err(e) = textdomain(TEXT_DOMAIN) {
            log::warn!("i18n: textdomain failed: {}", e);
        }
    }

    #[cfg(not(target_os = "linux"))]
    {
        // Non-Linux: rely on Slint bundled translations + rust-i18n only.
        // The `locale` value is consumed by `apply_bundled_translation`
        // (caller's responsibility) and by the rust_i18n::set_locale above.
        let _ = locale;
    }
}

#[cfg(test)]
#[path = "i18n_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "i18n_catalog_tests.rs"]
mod catalog_tests;
