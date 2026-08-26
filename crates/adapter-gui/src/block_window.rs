//! Responsibility: tracks one detached block editor window.

use std::rc::Rc;

use slint::Timer;

use crate::BlockEditorWindow;

pub(crate) struct BlockWindow {
    pub(crate) chain_index: usize,
    pub(crate) block_index: usize,
    pub(crate) window: BlockEditorWindow,
    #[allow(dead_code)]
    pub(crate) stream_timer: Option<Rc<Timer>>,
}
