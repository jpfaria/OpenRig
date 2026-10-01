//! Responsibility: drives the split editor's path-count buttons.
//!
//! #328 (spec §11): kind 0 of the split editor lists the split's paths. "+
//! path" appends an empty one; a remove button drops one — at once when it is
//! empty, through the remove-path confirmation when it holds blocks. After an
//! accepted edit the editor is reopened on the same split, so its paths and
//! per-path knobs follow what the project now holds.

use slint::{ComponentHandle, Global};

use domain::ids::BlockId;

use crate::chain_graph_wiring::chain_at;
use crate::graph_gesture_actions::{GestureError, RowsTarget};
use crate::helpers::set_status_error;
use crate::split_editor_wiring::SplitEditorWiringCtx;
use crate::split_path_dispatch::dispatch_split_path;
use crate::split_path_gestures::{add_path_command, remove_path_request, RemovePathRequest};
use crate::{AppWindow, ChainGraphOverlayState};

pub(crate) fn wire(window: &AppWindow, ctx: SplitEditorWiringCtx) {
    let ctx = std::rc::Rc::new(ctx);
    let state = ChainGraphOverlayState::get(window);
    {
        let (weak, ctx) = (window.as_weak(), ctx.clone());
        state.on_split_editor_add_path(move |chain_index, split_id| {
            let Some(window) = weak.upgrade() else {
                return;
            };
            let split_id = BlockId(split_id.to_string());
            run(&window, &ctx, chain_index, |chain| {
                add_path_command(chain, &split_id)
            });
        });
    }
    {
        let (weak, ctx) = (window.as_weak(), ctx.clone());
        state.on_split_editor_remove_path(move |chain_index, split_id, path| {
            let Some(window) = weak.upgrade() else {
                return;
            };
            let Ok(path) = usize::try_from(path) else {
                return;
            };
            let id = BlockId(split_id.to_string());
            let Some(chain) = chain_at(&ctx.project_session, chain_index) else {
                return;
            };
            match remove_path_request(&chain, &id, path) {
                Some(RemovePathRequest::Direct(_)) => run(&window, &ctx, chain_index, |chain| {
                    remove_command(chain, &id, path)
                }),
                Some(RemovePathRequest::Confirm { name, .. }) => {
                    let state = ChainGraphOverlayState::get(&window);
                    state.set_confirm_remove_path_chain_index(chain_index);
                    state.set_confirm_remove_path_split_id(split_id);
                    state.set_confirm_remove_path_index(path as i32);
                    state.set_confirm_remove_path_name(name.into());
                    state.set_confirm_remove_path_open(true);
                }
                None => {}
            }
        });
    }
    {
        let (weak, ctx) = (window.as_weak(), ctx.clone());
        state.on_confirm_remove_path(move || {
            let Some(window) = weak.upgrade() else {
                return;
            };
            let state = ChainGraphOverlayState::get(&window);
            state.set_confirm_remove_path_open(false);
            let chain_index = state.get_confirm_remove_path_chain_index();
            let id = BlockId(state.get_confirm_remove_path_split_id().to_string());
            let Ok(path) = usize::try_from(state.get_confirm_remove_path_index()) else {
                return;
            };
            run(&window, &ctx, chain_index, |chain| {
                remove_command(chain, &id, path)
            });
        });
    }
}

fn remove_command(
    chain: &project::chain::Chain,
    split_id: &BlockId,
    path: usize,
) -> Option<application::command::Command> {
    match remove_path_request(chain, split_id, path)? {
        RemovePathRequest::Direct(command) | RemovePathRequest::Confirm { command, .. } => {
            Some(command)
        }
    }
}

/// Dispatch the path edit, toast a refusal, then reopen the editor on the
/// split it shows so its rows follow the project.
fn run(
    window: &AppWindow,
    ctx: &SplitEditorWiringCtx,
    chain_index: i32,
    build: impl FnOnce(&project::chain::Chain) -> Option<application::command::Command>,
) {
    let Ok(index) = usize::try_from(chain_index) else {
        return;
    };
    let result = {
        let inputs = ctx.input_chain_devices.borrow();
        let outputs = ctx.output_chain_devices.borrow();
        let rows = RowsTarget {
            model: &ctx.project_chains,
            inputs: &inputs,
            outputs: &outputs,
        };
        dispatch_split_path(&ctx.project_session, index, build, &rows)
    };
    match result {
        Ok(()) => {}
        Err(GestureError::Failed(err)) => set_status_error(
            window,
            &ctx.toast_timer,
            &rust_i18n::t!("error-graph-action", err = err),
        ),
        Err(other) => log::warn!("[split-editor] path edit ignored: {other:?}"),
    }
    let state = ChainGraphOverlayState::get(window);
    if state.get_split_editor_open() {
        state.invoke_open_split_editor(
            state.get_split_editor_chain_index(),
            state.get_split_editor_split_id(),
            state.get_split_editor_kind(),
        );
    }
}
