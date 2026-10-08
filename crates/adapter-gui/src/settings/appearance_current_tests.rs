//! #398: a window opened after the choice paints in it.

use infra_filesystem::Appearance;

use super::appearance_current::{current, remember};

#[test]
fn the_last_choice_is_the_one_new_windows_open_in() {
    remember(Appearance::Dark);
    assert_eq!(current(), Appearance::Dark);
    remember(Appearance::Light);
    assert_eq!(current(), Appearance::Light);
    remember(Appearance::System);
    assert_eq!(current(), Appearance::System);
}
