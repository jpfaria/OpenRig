//! Responsibility: connects the compact view's looper section to the main window's looper wiring.
//! #1022 — the compact chain view shows the chain's looper panel inline. Its
//! actions are forwarded to the main window's looper callbacks, the ones the
//! chain row's panel fires, so both views share one dispatch path. Its rows are
//! the main row's live loopers, mirrored by the compact view's poll.

use slint::{ComponentHandle, Global};

use crate::{AppWindow, CompactChainViewWindow, CompactLooper, ProjectChainItem};

/// Forward every looper action of the compact section to the main window.
pub(crate) fn wire(main: &AppWindow, compact: &CompactChainViewWindow) {
    let actions = CompactLooper::get(compact);
    macro_rules! forward {
        ($on:ident => $invoke:ident, |$($a:ident),*|) => {{
            let weak = main.as_weak();
            actions.$on(move |$($a),*| {
                if let Some(m) = weak.upgrade() {
                    m.$invoke($($a),*);
                }
            });
        }};
    }
    forward!(on_add => invoke_looper_add, |ci|);
    forward!(on_record => invoke_looper_record, |ci, uid|);
    forward!(on_play_stop => invoke_looper_play_stop, |ci, uid|);
    forward!(on_play_all => invoke_looper_play_all, |ci|);
    forward!(on_stop_all => invoke_looper_stop_all, |ci|);
    forward!(on_undo => invoke_looper_undo, |ci, uid|);
    forward!(on_redo => invoke_looper_redo, |ci, uid|);
    forward!(on_clear => invoke_looper_clear, |ci, uid|);
    forward!(on_remove => invoke_looper_remove, |ci, uid|);
    forward!(on_mix_changed => invoke_looper_mix_changed, |ci, uid, v|);
    forward!(on_decay_changed => invoke_looper_decay_changed, |ci, uid, v|);
    forward!(on_speed_picked => invoke_looper_speed_picked, |ci, uid, v|);
    forward!(on_reverse_toggled => invoke_looper_reverse_toggled, |ci, uid, v|);
    forward!(on_input_picked => invoke_looper_input_picked, |ci, uid, v|);
    forward!(on_output_picked => invoke_looper_output_picked, |ci, uid, v|);
    forward!(on_preset_picked => invoke_looper_preset_picked, |ci, uid, v|);
}

/// Show the main row's loopers and their endpoint/preset options.
pub(crate) fn mirror_looper_row(compact: &CompactChainViewWindow, row: &ProjectChainItem) {
    let shown = CompactLooper::get(compact);
    shown.set_loopers(row.loopers.clone());
    shown.set_input_options(row.looper_input_options.clone());
    shown.set_output_options(row.looper_output_options.clone());
    shown.set_preset_options(row.looper_preset_options.clone());
}

#[cfg(test)]
#[path = "compact_looper_wiring_tests.rs"]
mod tests;
