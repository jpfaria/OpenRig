//! Responsibility: remembers the scheme a window built later must open in.
//!
//! #398: the chain editor, the block editor, the compact view and the plugin
//! info window are built on demand, after the user chose; each one reads the
//! last choice as it is constructed.

use std::sync::atomic::{AtomicU8, Ordering};

use infra_filesystem::Appearance;
use slint::{ComponentHandle, Global};

use super::appearance::{appearance_for_index, index_for_appearance, theme_mode_for};

static CURRENT: AtomicU8 = AtomicU8::new(0);

pub(crate) fn remember(appearance: Appearance) {
    CURRENT.store(index_for_appearance(appearance) as u8, Ordering::Relaxed);
}

pub(crate) fn current() -> Appearance {
    appearance_for_index(i32::from(CURRENT.load(Ordering::Relaxed)))
}

/// Paints `window` in the current choice; each window has its own `Theme`.
pub(crate) fn apply<'a, C: ComponentHandle>(window: &'a C)
where
    crate::Theme<'a>: Global<'a, C>,
{
    crate::Theme::get(window).set_mode(theme_mode_for(current()));
}
