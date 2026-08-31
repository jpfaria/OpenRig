//! Responsibility: translates an on-screen key label into the text it types.
//!
//! Split out of `virtual_keyboard_wiring` (#913). Dispatching the event pair is
//! screen work; deciding what a key MEANS is not — the three control keys are
//! drawn as glyphs, and a translation that let one through verbatim would type
//! "⌫" into the field instead of deleting a character.

use slint::SharedString;

/// The text a virtual key produces. Control keys map to their Slint key code;
/// every other label types itself.
pub(crate) fn key_text_for_label(label: &str) -> SharedString {
    match label {
        "⌫" => slint::platform::Key::Backspace.into(),
        "⏎" => slint::platform::Key::Return.into(),
        "⎵" => " ".into(),
        other => other.into(),
    }
}

#[cfg(test)]
#[path = "virtual_key_text_tests.rs"]
mod tests;
