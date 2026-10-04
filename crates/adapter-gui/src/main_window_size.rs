//! Responsibility: the size the main window opens at.

/// #324: wide enough for a chain row with its DI, looper and player controls.
pub(crate) const MAIN_WINDOW_WIDTH: f32 = 1324.0;
pub(crate) const MAIN_WINDOW_HEIGHT: f32 = 620.0;

pub(crate) fn initial_size() -> slint::WindowSize {
    slint::WindowSize::Logical(slint::LogicalSize::new(
        MAIN_WINDOW_WIDTH,
        MAIN_WINDOW_HEIGHT,
    ))
}

#[cfg(test)]
#[path = "main_window_size_tests.rs"]
mod tests;
