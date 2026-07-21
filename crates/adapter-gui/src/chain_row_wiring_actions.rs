//! Chain-row reorder + DI-loop callback wiring (issue #792 split from
//! chain_row_wiring.rs — extracted from the former 470-line `wire`).

use slint::ComponentHandle;

use application::dispatcher::CommandDispatcher;

use crate::chain_row_wiring::{
    apply_move_chain_down, apply_move_chain_up, shift_selected_chain_index_after_swap, ChainRowCtx,
};
use crate::helpers::{clear_status, set_status_error};
use crate::project_ops::sync_project_dirty;
use crate::project_view::replace_project_chains;
use crate::AppWindow;

pub(crate) fn wire_reorder(window: &AppWindow, ctx: &ChainRowCtx) {
    let project_session = &ctx.project_session;
    let project_chains = &ctx.project_chains;
    let saved_project_snapshot = &ctx.saved_project_snapshot;
    let project_dirty = &ctx.project_dirty;
    let input_chain_devices = &ctx.input_chain_devices;
    let output_chain_devices = &ctx.output_chain_devices;
    let toast_timer = &ctx.toast_timer;
    let auto_save = ctx.auto_save;


    // ── on_move_chain_up ────────────────────────────────────────────────────
    {
        let weak_window = window.as_weak();
        let project_session = project_session.clone();
        let project_chains = project_chains.clone();
        let saved_project_snapshot = saved_project_snapshot.clone();
        let project_dirty = project_dirty.clone();
        let input_chain_devices = input_chain_devices.clone();
        let output_chain_devices = output_chain_devices.clone();
        let toast_timer = toast_timer.clone();
        window.on_move_chain_up(move |index| {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            let mut session_borrow = project_session.borrow_mut();
            let Some(session) = session_borrow.as_mut() else {
                return;
            };
            let outcome = match apply_move_chain_up(session, index as usize) {
                Ok(Some(outcome)) => outcome,
                Ok(None) => return, // no-op (already top or invalid slot)
                Err(err) => {
                    set_status_error(&window, &toast_timer, &err.to_string());
                    return;
                }
            };
            replace_project_chains(
                &project_chains,
                &session.project.borrow(),
                &input_chain_devices.borrow(),
                &output_chain_devices.borrow(),
                &[],
            );
            // The chain-row model now reflects the new order, but the
            // per-chain preset/scene model (`chain_rig_nav`) was still
            // pointing at the OLD index→input mapping — so the row at
            // the new slot kept showing the previous neighbour's
            // preset/scene combobox until something else refreshed it.
            crate::chain_rig_nav_wiring::refresh_chain_rig_nav(&window, session);
            let selected = window.get_selected_chain_block_chain_index();
            let updated = shift_selected_chain_index_after_swap(
                selected,
                outcome.previous_slot,
                outcome.new_slot,
            );
            if updated != selected {
                window.set_selected_chain_block_chain_index(updated);
            }
            sync_project_dirty(
                &window,
                session,
                &saved_project_snapshot,
                &project_dirty,
                auto_save,
            );
            clear_status(&window, &toast_timer);
        });
    }

    // ── on_move_chain_down ──────────────────────────────────────────────────
    {
        let weak_window = window.as_weak();
        let project_session = project_session.clone();
        let project_chains = project_chains.clone();
        let saved_project_snapshot = saved_project_snapshot.clone();
        let project_dirty = project_dirty.clone();
        let input_chain_devices = input_chain_devices.clone();
        let output_chain_devices = output_chain_devices.clone();
        let toast_timer = toast_timer.clone();
        window.on_move_chain_down(move |index| {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            let mut session_borrow = project_session.borrow_mut();
            let Some(session) = session_borrow.as_mut() else {
                return;
            };
            let outcome = match apply_move_chain_down(session, index as usize) {
                Ok(Some(outcome)) => outcome,
                Ok(None) => return,
                Err(err) => {
                    set_status_error(&window, &toast_timer, &err.to_string());
                    return;
                }
            };
            replace_project_chains(
                &project_chains,
                &session.project.borrow(),
                &input_chain_devices.borrow(),
                &output_chain_devices.borrow(),
                &[],
            );
            // Mirror of `on_move_chain_up`: the preset/scene combobox
            // model is independent from `project_chains` and would
            // otherwise keep the previous slot's labels.
            crate::chain_rig_nav_wiring::refresh_chain_rig_nav(&window, session);
            let selected = window.get_selected_chain_block_chain_index();
            let updated = shift_selected_chain_index_after_swap(
                selected,
                outcome.previous_slot,
                outcome.new_slot,
            );
            if updated != selected {
                window.set_selected_chain_block_chain_index(updated);
            }
            sync_project_dirty(
                &window,
                session,
                &saved_project_snapshot,
                &project_dirty,
                auto_save,
            );
            clear_status(&window, &toast_timer);
        });
    }
}

pub(crate) fn wire_di_loop(window: &AppWindow, ctx: &ChainRowCtx) {
    let project_session = &ctx.project_session;
    let project_runtime = &ctx.project_runtime;
    let toast_timer = &ctx.toast_timer;

    // ── on_di_loop_source_selected ───────────────────────────────────────────
    // User picked a bundled id from the ComboBox (NOT the file-picker
    // sentinel). Dispatch SetChainDiLoopSource immediately; play is a
    // separate action via on_di_loop_play.
    {
        let weak_window = window.as_weak();
        let project_session = project_session.clone();
        let toast_timer = toast_timer.clone();
        window.on_di_loop_source_selected(move |index, source_str| {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            let session_borrow = project_session.borrow();
            let Some(session) = session_borrow.as_ref() else {
                set_status_error(
                    &window,
                    &toast_timer,
                    &rust_i18n::t!("error-no-project-loaded"),
                );
                return;
            };
            let chain_id = {
                let proj = session.project.borrow();
                let Some(chain) = proj.chains.get(index as usize) else {
                    return;
                };
                chain.id.clone()
            };
            // Bundled id → Bundled source; the file label of an already-loaded
            // File (#661) → no-op (dispatcher already holds it). Sentinel is
            // routed to choose-file by the ComboBox, never here.
            let bundled_ids = crate::di_loop_ui_sources::bundled_di_loop_ids();
            let bundled_refs: Vec<&str> = bundled_ids.iter().map(|s| s.as_str()).collect();
            let Some(source) =
                crate::di_loop_ui_sources::parse_di_loop_source(&source_str, &bundled_refs)
            else {
                return;
            };
            let cmds = crate::di_loop_wiring::di_loop_commands(
                chain_id,
                crate::di_loop_wiring::DiLoopIntent::SelectSource { source },
            );
            for cmd in cmds {
                if let Err(err) = session.dispatcher.dispatch(cmd) {
                    set_status_error(&window, &toast_timer, &err.to_string());
                    return;
                }
            }
        });
    }

    // ── on_di_loop_choose_file ───────────────────────────────────────────────
    // Wired separately in di_loop_chooser_wiring.rs (uses the native file
    // dialog crate; chain_row_wiring.rs is forbidden from that — issue #511).

    // ── on_di_loop_play ─────────────────────────────────────────────────────
    // User pressed ▶. Dispatch SetChainDiLoopEnabled { enabled: true } AND
    // apply the arc to the chain runtime immediately (mirrors wire_mute_inline
    // in tuner_wiring.rs — dispatch + apply in the same callback, no polling).
    {
        let project_session = project_session.clone();
        let project_runtime = project_runtime.clone();
        window.on_di_loop_play(move |index| {
            let session_borrow = project_session.borrow();
            let Some(session) = session_borrow.as_ref() else {
                return;
            };
            let chain_id = {
                let proj = session.project.borrow();
                let Some(chain) = proj.chains.get(index as usize) else {
                    return;
                };
                chain.id.clone()
            };
            crate::di_loop_wiring::play_chain_di_loop(
                &project_runtime,
                &session.dispatcher,
                &chain_id,
            );
        });
    }

    // ── on_di_loop_stop ──────────────────────────────────────────────────────
    // User pressed ■. Dispatch SetChainDiLoopEnabled { enabled: false } AND
    // clear the chain runtime immediately.
    {
        let project_session = project_session.clone();
        let project_runtime = project_runtime.clone();
        window.on_di_loop_stop(move |index| {
            let session_borrow = project_session.borrow();
            let Some(session) = session_borrow.as_ref() else {
                return;
            };
            let chain_id = {
                let proj = session.project.borrow();
                let Some(chain) = proj.chains.get(index as usize) else {
                    return;
                };
                chain.id.clone()
            };
            crate::di_loop_wiring::stop_chain_di_loop(
                &project_runtime,
                &session.dispatcher,
                &chain_id,
            );
        });
    }
}

