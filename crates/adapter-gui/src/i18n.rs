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
///
/// Linux-only: only the libintl/gettext consumer needs this. Other platforms
/// rely on Slint bundled translations.
#[cfg(target_os = "linux")]
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

#[cfg(target_os = "linux")]
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
