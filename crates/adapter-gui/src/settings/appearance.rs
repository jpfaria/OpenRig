//! Responsibility: maps the Settings → Appearance segments to schemes.
//!
//! #398: System / Light / Dark, in that order, each driving one `ThemeMode`.
//! The section's wiring lives in `appearance_wiring`.

use infra_filesystem::Appearance;

use crate::ThemeMode;

/// The segment order of the section: System, Light, Dark.
pub(crate) fn appearance_for_index(index: i32) -> Appearance {
    match index {
        1 => Appearance::Light,
        2 => Appearance::Dark,
        _ => Appearance::System,
    }
}

pub(crate) fn index_for_appearance(appearance: Appearance) -> i32 {
    match appearance {
        Appearance::System => 0,
        Appearance::Light => 1,
        Appearance::Dark => 2,
    }
}

pub(crate) fn theme_mode_for(appearance: Appearance) -> ThemeMode {
    match appearance {
        Appearance::System => ThemeMode::System,
        Appearance::Light => ThemeMode::Light,
        Appearance::Dark => ThemeMode::Dark,
    }
}
