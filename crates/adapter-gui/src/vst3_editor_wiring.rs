//! Wiring for opening a VST3 plugin's native editor window.
//!
//! Stores the returned handle in the shared `vst3_editor_handles` vec so the
//! OS keeps the editor window alive after the callback returns.

use std::cell::RefCell;
use std::rc::Rc;

use crate::AppWindow;

pub(crate) fn wire(
    window: &AppWindow,
    vst3_editor_handles: Rc<RefCell<Vec<Box<dyn project::vst3_editor::PluginEditorHandle>>>>,
    vst3_sample_rate: f64,
) {
    window.on_open_vst3_editor(move |model_id| {
        match project::vst3_editor::open_vst3_editor(model_id.as_str(), vst3_sample_rate) {
            Ok(handle) => {
                vst3_editor_handles.borrow_mut().push(handle);
            }
            Err(e) => {
                log::error!("VST3 editor: failed to open '{}': {}", model_id, e);
            }
        }
    });
}
