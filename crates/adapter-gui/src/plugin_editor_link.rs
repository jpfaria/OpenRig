//! Responsibility: lets any window open a plugin in the parameter editor.
//!
//! The info windows are built far from the editor's wiring; the editor
//! registers itself here once, and an info window asks it whether a plugin
//! can be edited and opens it.

use std::cell::RefCell;
use std::rc::Rc;

#[derive(Clone)]
pub(crate) struct EditorLink {
    /// The plugin is the user's and has captures to edit.
    pub editable: Rc<dyn Fn(&str) -> bool>,
    pub open: Rc<dyn Fn(&str)>,
}

thread_local! {
    static LINK: RefCell<Option<EditorLink>> = const { RefCell::new(None) };
}

pub(crate) fn register(link: EditorLink) {
    LINK.with(|l| *l.borrow_mut() = Some(link));
}

/// `false` until the editor registered.
pub(crate) fn is_editable(plugin_id: &str) -> bool {
    current().is_some_and(|link| (link.editable)(plugin_id))
}

pub(crate) fn open(plugin_id: &str) {
    if let Some(link) = current() {
        (link.open)(plugin_id);
    }
}

fn current() -> Option<EditorLink> {
    LINK.with(|l| l.borrow().clone())
}

#[cfg(test)]
#[path = "plugin_editor_link_tests.rs"]
mod tests;
