//! Wiring for opening a VST3 plugin's native editor window.
//!
//! Stores the returned handle in the shared `Vst3EditorRegistry` so the OS
//! keeps the editor window alive after the callback returns; re-opening the
//! same model replaces (and closes) the previous editor first.

use std::cell::RefCell;
use std::rc::Rc;

use crate::AppWindow;

pub(crate) fn wire(
    window: &AppWindow,
    vst3_editor_handles: Rc<RefCell<project::vst3_editor::Vst3EditorRegistry>>,
    vst3_sample_rate: f64,
) {
    window.on_open_vst3_editor(move |model_id| {
        let res = vst3_editor_handles
            .borrow_mut()
            .open_or_focus(model_id.as_str(), || {
                project::vst3_editor::open_vst3_editor(model_id.as_str(), vst3_sample_rate)
            });
        if let Err(e) = res {
            log::error!("VST3 editor: failed to open '{}': {}", model_id, e);
        }
    });
}
