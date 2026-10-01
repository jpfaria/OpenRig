//! Responsibility: names the window seam the waveform editor's wiring talks to.
//! #1022 — the main window and the compact chain view each host their own
//! waveform editor (a Slint global is per window). The editor wiring is the
//! same for both; only where its five callbacks live differs: the main window
//! declares them at its root, the compact view on its `CompactLooper` global.

use slint::ComponentHandle;

use crate::{AppWindow, CompactChainViewWindow, CompactLooper, LoopEditKind, LooperEditor};

pub(crate) trait LooperEditorHost: ComponentHandle + 'static {
    fn editor(&self) -> LooperEditor<'_>;
    fn on_edit(&self, f: impl FnMut(i32, i32) + 'static);
    fn on_edit_apply(&self, f: impl FnMut(i32, i32, LoopEditKind, f32, f32) + 'static);
    fn on_edit_play_stop(&self, f: impl FnMut(i32, i32) + 'static);
    fn on_edit_undo(&self, f: impl FnMut(i32, i32) + 'static);
    fn on_edit_redo(&self, f: impl FnMut(i32, i32) + 'static);
}

impl LooperEditorHost for AppWindow {
    fn editor(&self) -> LooperEditor<'_> {
        self.global::<LooperEditor>()
    }
    fn on_edit(&self, f: impl FnMut(i32, i32) + 'static) {
        self.on_looper_edit(f);
    }
    fn on_edit_apply(&self, f: impl FnMut(i32, i32, LoopEditKind, f32, f32) + 'static) {
        self.on_looper_edit_apply(f);
    }
    fn on_edit_play_stop(&self, f: impl FnMut(i32, i32) + 'static) {
        self.on_looper_edit_play_stop(f);
    }
    fn on_edit_undo(&self, f: impl FnMut(i32, i32) + 'static) {
        self.on_looper_edit_undo(f);
    }
    fn on_edit_redo(&self, f: impl FnMut(i32, i32) + 'static) {
        self.on_looper_edit_redo(f);
    }
}

impl LooperEditorHost for CompactChainViewWindow {
    fn editor(&self) -> LooperEditor<'_> {
        self.global::<LooperEditor>()
    }
    fn on_edit(&self, f: impl FnMut(i32, i32) + 'static) {
        self.global::<CompactLooper>().on_edit(f);
    }
    fn on_edit_apply(&self, f: impl FnMut(i32, i32, LoopEditKind, f32, f32) + 'static) {
        self.global::<CompactLooper>().on_edit_apply(f);
    }
    fn on_edit_play_stop(&self, f: impl FnMut(i32, i32) + 'static) {
        self.global::<CompactLooper>().on_edit_play_stop(f);
    }
    fn on_edit_undo(&self, f: impl FnMut(i32, i32) + 'static) {
        self.global::<CompactLooper>().on_edit_undo(f);
    }
    fn on_edit_redo(&self, f: impl FnMut(i32, i32) + 'static) {
        self.global::<CompactLooper>().on_edit_redo(f);
    }
}
