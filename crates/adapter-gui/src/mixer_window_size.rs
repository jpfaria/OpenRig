//! Responsibility: sizes the Mixer window to the strips of its open tab.
//! #1007 — the window opens showing every strip of the tab side by side and
//! re-fits when a tab switch changes the strip count. Past the screen the OS
//! clamps the window and the strip row scrolls sideways.

use std::cell::Cell;

use slint::ComponentHandle;

use crate::MixerWindow;

/// Horizontal distance between two strips (`MixerSection.strip-pitch`).
const STRIP_PITCH: f32 = 120.0;
/// The panel's 16 px margin on each side of the strip row.
const SIDE_MARGINS: f32 = 32.0;
/// The window's `min-width`.
const MIN_WIDTH: f32 = 360.0;

/// Logical width that shows `strip_count` strips without scrolling.
pub(crate) fn mixer_window_width(strip_count: usize) -> f32 {
    (strip_count as f32 * STRIP_PITCH + SIDE_MARGINS).max(MIN_WIDTH)
}

/// Resize `window` when its open tab's strip count differs from `fitted`.
pub(crate) fn fit_mixer_window(window: &MixerWindow, fitted: &Cell<Option<i32>>) {
    let count = window.get_shown_strip_count();
    if fitted.get() == Some(count) {
        return;
    }
    fitted.set(Some(count));
    let height = window
        .window()
        .size()
        .to_logical(window.window().scale_factor())
        .height;
    let width = mixer_window_width(count.max(0) as usize);
    window
        .window()
        .set_size(slint::WindowSize::Logical(slint::LogicalSize::new(
            width, height,
        )));
}

#[cfg(test)]
#[path = "mixer_window_size_tests.rs"]
mod tests;
