//! the first-run setup wizard is actually on screen.
//!
//! The previous audio wizard kept its Rust wiring and its `show-audio-settings`
//! flag long after its page was emptied, so first launch showed nothing. These
//! checks pin that both shells render the page behind the flag Rust raises.

use std::path::Path;

fn ui(file: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("ui").join(file);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

#[test]
fn the_desktop_shell_renders_the_wizard() {
    let source = ui("desktop_main.slint");
    assert!(source.contains("if root.show-setup-wizard : SetupWizardPage"));
}

#[test]
fn the_touch_shell_renders_the_wizard() {
    let source = ui("touch_main.slint");
    assert!(source.contains("if root.show-setup-wizard : SetupWizardPage"));
}

#[test]
fn the_wizard_reuses_the_settings_sections() {
    let source = ui("pages/setup_wizard.slint");
    for section in [
        "SectionSystemLanguage",
        "SectionSystemAudio",
        "SectionSystemIoBindings",
        "SectionSystemMidiDevices",
        "SectionSystemTone3000Key",
    ] {
        assert!(source.contains(section), "wizard must reuse {section}");
    }
}

#[test]
fn the_language_step_reaches_the_language_handler_in_the_main_window() {
    // The wizard's language list fires `SettingsBridge.language-selected` on the
    // main window, so the handler has to be wired there too, not only on the
    // standalone settings window.
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/settings/language.rs");
    let source = std::fs::read_to_string(path).expect("read language.rs");
    assert!(
        source.matches("on_language_selected").count() >= 2,
        "language-selected must be wired on both windows"
    );
}
