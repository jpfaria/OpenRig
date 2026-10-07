//! Responsibility: keeps the windows built on demand in the chosen scheme while they are open.
//!
//! #398: the compact view, the block editor, the chain editor and the plugin
//! info window are built after the user chose. Each one opens in the current
//! choice and is held weakly here, so a scheme picked while it is open
//! repaints it too; a closed window drops out on the next repaint.

use std::cell::RefCell;

use infra_filesystem::Appearance;
use slint::{ComponentHandle, Global};

use super::appearance::theme_mode_for;
use crate::ThemeMode;

type Follower = Box<dyn Fn(ThemeMode) -> bool>;

thread_local! {
    static FOLLOWERS: RefCell<Vec<Follower>> = const { RefCell::new(Vec::new()) };
}

/// Paints `window` in the current choice and keeps it following later picks.
pub(crate) fn follow<C: ComponentHandle + 'static>(window: &C)
where
    for<'a> crate::Theme<'a>: Global<'a, C>,
{
    super::appearance_current::apply(window);
    let weak = window.as_weak();
    FOLLOWERS.with(|f| {
        f.borrow_mut()
            .push(Box::new(move |mode| match weak.upgrade() {
                Some(w) => {
                    crate::Theme::get(&w).set_mode(mode);
                    true
                }
                None => false,
            }))
    });
}

/// Repaints every window still open; forgets the closed ones.
pub(crate) fn repaint_all(appearance: Appearance) {
    let mode = theme_mode_for(appearance);
    FOLLOWERS.with(|f| f.borrow_mut().retain(|paint| paint(mode)));
}

#[cfg(test)]
pub(crate) fn followers() -> usize {
    FOLLOWERS.with(|f| f.borrow().len())
}
