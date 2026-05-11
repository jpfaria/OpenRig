//! Wires the language selector dropdown into the gui-settings.yaml roundtrip.
//!
//! Reads the persisted `language` field, computes the index in
//! `SUPPORTED_LANGUAGES` so the dropdown highlights the right entry, and
//! handles the `change-language` callback by persisting the new choice.
//!
//! gettext only resolves strings on process startup, so changing the locale
//! does not reflow the UI in place — the user sees the new language on the
//! next launch. Live re-rendering is left for a follow-up issue.

use slint::{ComponentHandle, Global, ModelRc, SharedString, VecModel};

use infra_filesystem::FilesystemStorage;

use crate::i18n::{display_name, font_family_for_locale, locale_for_runtime, SUPPORTED_LANGUAGES};
use crate::{AppWindow, Locale};

/// `apply_font_to_all_windows` is invoked on every language change with the
/// new font family. The caller (desktop_app) wires it to a closure that
/// holds weak refs to every secondary Window and propagates the font there
/// — necessary because each Window is its own Slint root with an isolated
/// Locale global, so setting Locale on AppWindow alone leaves the rest
/// rendering with the boot-time font.
pub fn wire(window: &AppWindow, apply_font_to_all_windows: impl Fn(&str) + 'static) {
    let initial_locale = locale_for_runtime(read_persisted_language().as_deref());
    set_language_options(window, &initial_locale);
    // Boot-time font: must match the locale the bundled translations were
    // selected against, otherwise the first frame renders tofu before any
    // language change happens.
    let boot_font = font_family_for_locale(&initial_locale);
    eprintln!(
        "i18n.font: boot locale={} → font_family={}",
        initial_locale, boot_font
    );
    Locale::get(window).set_font_family(boot_font.into());

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
        // Swap default-font-family on the Slint side so CJK/Devanagari
        // glyphs render against a face that actually contains them
        // (Bebas Neue is Latin-only and produces tofu □□ in ja/zh/ko/hi).
        let new_locale_for_font = locale_for_runtime(lang.as_deref());
        let new_font = font_family_for_locale(&new_locale_for_font);
        eprintln!(
            "i18n.font: change locale={} → font_family={}",
            new_locale_for_font, new_font
        );
        Locale::get(&window).set_font_family(new_font.into());
        apply_font_to_all_windows(new_font);
        // Rebuild the dropdown labels in the new UI locale — otherwise
        // the language list itself stays in the previous language and
        // the selector reads "Alemão / Chinês" while the rest of the UI
        // is in English.
        let new_locale = locale_for_runtime(lang.as_deref());
        set_language_options(&window, &new_locale);
        // Refresh strings that Rust injects into Slint (titles, save labels,
        // etc.). Slint's bundled translations don't cover them — they go
        // through rust_i18n::t!() at the moment Rust calls set_*. Without
        // this, properties stay frozen in the previous locale.
        refresh_rust_injected_strings(&window);
        window.set_selected_language_index(idx);
    });
}

/// Re-apply Slint properties that Rust pushes via `set_*(t!(...))` and
/// whose text comes from translations (not from data files). Called on
/// language change so labels reflect the new locale without an app
/// restart. NEVER touch properties whose values come from user data —
/// `project_title`, for example, is the loaded project's actual name
/// and would be clobbered if we re-applied a translated default here.
fn refresh_rust_injected_strings(window: &AppWindow) {
    use slint::SharedString;
    // Default the chain editor labels to "create" mode. If the user is
    // currently in edit mode, they'll see the create-mode wording until
    // they reopen the editor — acceptable UX cost for keeping the wiring
    // generic. apply_chain_editor_labels in chain_editor.rs covers the
    // edit-mode case when the editor opens.
    window.set_chain_editor_title(SharedString::from(
        rust_i18n::t!("title-new-chain").as_ref(),
    ));
    window.set_chain_editor_save_label(SharedString::from(
        rust_i18n::t!("btn-create-chain").as_ref(),
    ));
}

/// Build the dropdown labels using `display_name` for the given UI locale
/// and push them into the AppWindow's `language-options` model.
/// Also pushes the parallel `language-codes` model so Slint can look up
/// the country flag SVG for each row via @image-url ternary.
fn set_language_options(window: &AppWindow, ui_locale: &str) {
    let options = build_language_options(ui_locale);
    let shared: Vec<SharedString> = options.into_iter().map(SharedString::from).collect();
    window.set_language_options(ModelRc::new(VecModel::from(shared)));

    let codes: Vec<SharedString> = SUPPORTED_LANGUAGES
        .iter()
        .map(|l| SharedString::from(l.code))
        .collect();
    window.set_language_codes(ModelRc::new(VecModel::from(codes)));
}

/// Pure helper used by tests AND by the runtime wiring. Returns the list
/// of dropdown labels in the current UI locale, in the same order as
/// `SUPPORTED_LANGUAGES`. The flag is rendered separately by Slint via
/// the parallel `language-codes` array (see `LanguageSelector.slint`),
/// so this returns just the localized name.
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
#[path = "language_wiring_tests.rs"]
mod tests;
