//! Responsibility: tracks one detached block editor window.

use std::rc::Rc;

use slint::Timer;

use crate::BlockEditorWindow;

pub(crate) struct BlockWindow {
    pub(crate) chain_index: usize,
    pub(crate) block_index: usize,
    /// #328: the split path `block_index` counts in; part of the window's identity.
    pub(crate) path: Option<project::block::PathRef>,
    pub(crate) window: BlockEditorWindow,
    #[allow(dead_code)]
    pub(crate) stream_timer: Option<Rc<Timer>>,
}
