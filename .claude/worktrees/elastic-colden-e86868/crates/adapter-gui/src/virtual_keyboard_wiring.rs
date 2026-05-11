//! Wiring for the on-screen virtual keyboard's key events.
//!
//! Translates the keyboard label (Backspace `⌫`, Return `⏎`, Space `⎵`,
//! everything else verbatim) into a Slint `KeyPressed`+`KeyReleased` pair
//! dispatched to the focused element.

use slint::ComponentHandle;

use crate::AppWindow;

pub(crate) fn wire(window: &AppWindow) {
    let weak_window = window.as_weak();
    window.on_virtual_key_pressed(move |label| {
        let Some(win) = weak_window.upgrade() else {
            return;
        };
        let text: slint::SharedString = match label.as_str() {
            "⌫" => slint::platform::Key::Backspace.into(),
            "⏎" => slint::platform::Key::Return.into(),
            "⎵" => " ".into(),
            s => s.into(),
        };
        let _ = win
            .window()
            .dispatch_event(slint::platform::WindowEvent::KeyPressed { text: text.clone() });
        let _ = win
            .window()
            .dispatch_event(slint::platform::WindowEvent::KeyReleased { text });
    });
}
