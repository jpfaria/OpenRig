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

use crate::i18n::SUPPORTED_LANGUAGES;
use crate::AppWindow;

pub fn wire(window: &AppWindow) {
    let options: Vec<SharedString> = SUPPORTED_LANGUAGES
        .iter()
        .map(|l| SharedString::from(l.display))
        .collect();
    window.set_language_options(ModelRc::new(VecModel::from(options)));

    let initial_index = current_language_index();
    window.set_selected_language_index(initial_index);

    let weak = window.as_weak();
    window.on_change_language(move |idx: i32| {
        let Some(window) = weak.upgrade() else {
            return;
        };
        let lang = pick_language(idx);
        log::info!("language selector: persisting {:?}", lang);
        if let Err(e) = FilesystemStorage::save_gui_language(lang) {
            log::warn!("failed to persist language preference: {e}");
            return;
        }
        window.set_selected_language_index(idx);
    });
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

    #[test]
    fn pick_language_returns_code_for_explicit_choice() {
        assert_eq!(pick_language(1).as_deref(), Some("pt-BR"));
        assert_eq!(pick_language(2).as_deref(), Some("en-US"));
    }

    #[test]
    fn pick_language_returns_none_for_out_of_range() {
        assert!(pick_language(99).is_none());
        assert!(pick_language(-1).is_none());
    }
}
