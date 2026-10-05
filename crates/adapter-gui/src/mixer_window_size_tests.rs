//! #1007: the Mixer window opens wide enough to show every strip of the open
//! tab side by side, so the user never has to resize it.

use super::mixer_window_width;

#[test]
fn the_window_fits_every_strip_of_the_tab() {
    // 16 px margin each side + one 120 px pitch per strip.
    assert_eq!(mixer_window_width(1), 360.0);
    assert_eq!(mixer_window_width(4), 4.0 * 120.0 + 32.0);
    assert_eq!(mixer_window_width(12), 12.0 * 120.0 + 32.0);
}

#[test]
fn an_empty_or_short_tab_keeps_the_minimum_width() {
    assert_eq!(mixer_window_width(0), 360.0);
    assert_eq!(mixer_window_width(2), 360.0);
}

#[test]
fn more_strips_never_make_the_window_narrower() {
    let widths: Vec<f32> = (0..30).map(mixer_window_width).collect();
    assert!(widths.windows(2).all(|w| w[1] >= w[0]));
}
