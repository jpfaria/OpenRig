//! #398: the three segments of Settings → Appearance and what each drives.

use infra_filesystem::Appearance;

use super::appearance::{appearance_for_index, index_for_appearance, theme_mode_for};
use crate::ThemeMode;

#[test]
fn the_segments_read_system_light_dark_in_order() {
    assert_eq!(appearance_for_index(0), Appearance::System);
    assert_eq!(appearance_for_index(1), Appearance::Light);
    assert_eq!(appearance_for_index(2), Appearance::Dark);
    for appearance in [Appearance::System, Appearance::Light, Appearance::Dark] {
        assert_eq!(
            appearance_for_index(index_for_appearance(appearance)),
            appearance
        );
    }
}

#[test]
fn an_unknown_segment_follows_the_system() {
    assert_eq!(appearance_for_index(-1), Appearance::System);
    assert_eq!(appearance_for_index(3), Appearance::System);
}

#[test]
fn each_choice_drives_the_theme_mode() {
    assert_eq!(theme_mode_for(Appearance::System), ThemeMode::System);
    assert_eq!(theme_mode_for(Appearance::Light), ThemeMode::Light);
    assert_eq!(theme_mode_for(Appearance::Dark), ThemeMode::Dark);
}
