//! #398: a window built on demand keeps following the scheme while it is open.

use infra_filesystem::Appearance;
use slint::Global;

use super::appearance_followers::{follow, followers, repaint_all};
use crate::{CompactChainViewWindow, Theme, ThemeMode};

#[test]
fn an_open_compact_window_takes_a_scheme_chosen_after_it_opened() {
    i_slint_backend_testing::init_no_event_loop();
    let w = CompactChainViewWindow::new().unwrap();
    follow(&w);
    repaint_all(Appearance::Light);
    assert_eq!(Theme::get(&w).get_mode(), ThemeMode::Light);
    repaint_all(Appearance::Dark);
    assert_eq!(Theme::get(&w).get_mode(), ThemeMode::Dark);
}

#[test]
fn a_closed_window_is_forgotten() {
    i_slint_backend_testing::init_no_event_loop();
    let w = CompactChainViewWindow::new().unwrap();
    follow(&w);
    assert_eq!(followers(), 1);
    drop(w);
    repaint_all(Appearance::Light);
    assert_eq!(followers(), 0);
}
