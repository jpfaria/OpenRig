//! Wires the language selector dropdown into the gui-settings.yaml roundtrip.
//!
//! Reads the persisted `language` field, computes the index in
//! `SUPPORTED_LANGUAGES` so the dropdown highlights the right entry, and
//! handles the `change-language` callback by persisting the new choice.
//!
//! gettext only resolves strings on process startup, so changing the locale
//! does not reflow the UI in place — the user sees the new language on the
//! next launch. Live re-rendering is left for a follow-up issue.

use slint::{ComponentHandle, ModelRc, SharedString, VecModel};

use infra_filesystem::FilesystemStorage;

use crate::i18n::{display_name, locale_for_runtime, SUPPORTED_LANGUAGES};
use crate::AppWindow;

pub fn wire(window: &AppWindow) {
    let initial_locale = locale_for_runtime(read_persisted_language().as_deref());
    set_language_options(window, &initial_locale);

    let initial_index = current_language_index();
    window.set_selected_language_index(initial_index);

    let weak = window.as_weak();
    window.on_change_language(move |idx: i32| {
        let Some(window) = weak.upgrade() else {
            return;
        };
        let lang = pick_language(idx);
        log::info!("language selector: persisting {:?}", lang);
        if let Err(e) = FilesystemStorage::save_gui_language(lang.clone()) {
            log::warn!("failed to persist language preference: {e}");
            return;
        }
        // Live update: re-select the bundled translation so visible strings
        // reflect the new locale immediately. No restart needed for Slint
        // bundled translations (unlike runtime gettext, which is locked
        // once libintl reads its env vars).
        crate::i18n::apply_bundled_translation(lang.as_deref());
        // Rebuild the dropdown labels in the new UI locale — otherwise
        // the language list itself stays in the previous language and
        // the selector reads "Alemão / Chinês" while the rest of the UI
        // is in English.
        let new_locale = locale_for_runtime(lang.as_deref());
        set_language_options(&window, &new_locale);
        window.set_selected_language_index(idx);
    });
}

/// Build the dropdown labels using `display_name` for the given UI locale
/// and push them into the AppWindow's `language-options` model.
fn set_language_options(window: &AppWindow, ui_locale: &str) {
    let options = build_language_options(ui_locale);
    let shared: Vec<SharedString> = options.into_iter().map(SharedString::from).collect();
    window.set_language_options(ModelRc::new(VecModel::from(shared)));
}

/// Pure helper used by tests AND by the runtime wiring. Returns the list
/// of human-readable language names in the current UI locale, in the same
/// order as `SUPPORTED_LANGUAGES`.
pub fn build_language_options(ui_locale: &str) -> Vec<String> {
    SUPPORTED_LANGUAGES
        .iter()
        .map(|l| display_name(l.code, ui_locale).to_string())
        .collect()
}

fn read_persisted_language() -> Option<String> {
    FilesystemStorage::load_gui_audio_settings()
        .ok()
        .flatten()
        .and_then(|s| s.language)
}

/// Index in `SUPPORTED_LANGUAGES` matching the persisted language code, or
/// 0 ("Auto") when nothing is persisted or the persisted code is unknown.
fn current_language_index() -> i32 {
    let Some(persisted) = FilesystemStorage::load_gui_audio_settings()
        .ok()
        .flatten()
        .and_then(|s| s.language)
    else {
        return 0;
    };

    SUPPORTED_LANGUAGES
        .iter()
        .position(|l| l.code.eq_ignore_ascii_case(&persisted))
        .map(|i| i as i32)
        .unwrap_or(0)
}

/// Convert a dropdown index back into a persistable string. Index 0 is the
/// "Auto" sentinel — we store None so gui-settings.yaml stays minimal and the
/// next OS-locale change is honored automatically.
fn pick_language(idx: i32) -> Option<String> {
    let i = usize::try_from(idx).ok()?;
    let lang = SUPPORTED_LANGUAGES.get(i)?;
    if lang.code.eq_ignore_ascii_case("auto") {
        None
    } else {
        Some(lang.code.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pick_language_returns_none_for_auto_index() {
        assert!(pick_language(0).is_none());
    }

    /// SUPPORTED_LANGUAGES order: [auto, de-DE, zh-CN, ko-KR, es-ES,
    /// fr-FR, hi-IN, en-US, ja-JP, pt-BR]. Index 1 is German, 7 is
    /// English (US), 9 is Portuguese (Brazil).
    #[test]
    fn pick_language_returns_code_for_explicit_choice() {
        assert_eq!(pick_language(1).as_deref(), Some("de-DE"));
        assert_eq!(pick_language(7).as_deref(), Some("en-US"));
        assert_eq!(pick_language(9).as_deref(), Some("pt-BR"));
    }

    #[test]
    fn pick_language_returns_none_for_out_of_range() {
        assert!(pick_language(99).is_none());
        assert!(pick_language(-1).is_none());
    }

    /// The dropdown labels must localize to the active UI language.
    /// Picking en-US must yield English names; picking pt-BR must yield
    /// Portuguese names. Skeleton UI locales (fr-FR, ja-JP, etc.) fall
    /// back to en-US — same redirection as the runtime translations.
    #[test]
    fn build_language_options_uses_pt_br_names_when_ui_is_pt_br() {
        let opts = build_language_options("pt-BR");
        assert_eq!(opts[0], "Auto");
        assert_eq!(opts[1], "Alemão");
        assert_eq!(opts[7], "Inglês (US)");
        assert_eq!(opts[9], "Português (Brasil)");
    }

    #[test]
    fn build_language_options_uses_en_us_names_when_ui_is_en_us() {
        let opts = build_language_options("en-US");
        assert_eq!(opts[0], "Auto");
        assert_eq!(opts[1], "German");
        assert_eq!(opts[7], "English (US)");
        assert_eq!(opts[9], "Portuguese (Brazil)");
    }

    #[test]
    fn build_language_options_falls_back_to_en_us_for_skeleton_ui_locale() {
        let opts = build_language_options("fr-FR");
        assert_eq!(opts[1], "German");
        assert_eq!(opts[7], "English (US)");
    }

    #[test]
    fn build_language_options_has_one_entry_per_supported_language() {
        let opts = build_language_options("en-US");
        assert_eq!(opts.len(), SUPPORTED_LANGUAGES.len());
    }
}
