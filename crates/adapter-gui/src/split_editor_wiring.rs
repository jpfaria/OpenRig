//! Responsibility: drives the split editor overlay.
//!
//! #328 (spec §1.2, §5.1). One overlay edits either end of the split it was
//! opened on (by id — a chain may hold a Mix and a Y): kind 0 shows the
//! split's knobs and its Mix / Y switch, kind 1 the mixer's. A knob
//! edit goes on the bus as a `SetBlockParameter*` on the split block
//! (`apply_parameter_to_block`), never through the drawer's persist, which
//! would rebuild the block from a model. Rows update in place so a knob being
//! dragged keeps its identity (#715).

use std::cell::RefCell;
use std::rc::Rc;

use slint::{ComponentHandle, Global, Model, ModelRc, Timer, VecModel};

use domain::ids::BlockId;
use domain::AudioDeviceDescriptor;
use project::block::{SplitEnd, MIN_SPLIT_PATHS};

use crate::block_param_apply::{apply_parameter_to_block, ApplyParamError, ParamValue};
use crate::chain_block_lists::split_by_id;
use crate::chain_graph_wiring::chain_at;
use crate::graph_gesture_actions::{GestureError, RowsTarget};
use crate::helpers::set_status_error;
use crate::split_editor_items::{option_value, split_editor_items, SplitEditorKind};
use crate::split_end_switch::set_split_end;
use crate::split_path_gestures::path_letters;
use crate::state::ProjectSession;
use crate::{AppWindow, BlockParameterItem, ChainGraphOverlayState, ProjectChainItem};

#[derive(Clone)]
pub(crate) struct SplitEditorWiringCtx {
    pub(crate) project_session: Rc<RefCell<Option<ProjectSession>>>,
    pub(crate) project_chains: Rc<VecModel<ProjectChainItem>>,
    pub(crate) input_chain_devices: Rc<RefCell<Vec<AudioDeviceDescriptor>>>,
    pub(crate) output_chain_devices: Rc<RefCell<Vec<AudioDeviceDescriptor>>>,
    pub(crate) toast_timer: Rc<Timer>,
}

struct Editor {
    ctx: SplitEditorWiringCtx,
    rows: Rc<VecModel<BlockParameterItem>>,
}

pub(crate) fn wire(window: &AppWindow, ctx: SplitEditorWiringCtx) {
    crate::split_path_wiring::wire(window, ctx.clone());
    let editor = Rc::new(Editor {
        ctx,
        rows: Rc::new(VecModel::default()),
    });
    let state = ChainGraphOverlayState::get(window);
    state.set_split_editor_items(ModelRc::from(editor.rows.clone()));
    state.set_split_editor_end_mix_label(rust_i18n::t!("picker-split-mix").to_string().into());
    state.set_split_editor_end_y_label(rust_i18n::t!("picker-split-y").to_string().into());
    {
        let (weak, editor) = (window.as_weak(), editor.clone());
        state.on_open_split_editor(move |chain_index, split_id, kind_index| {
            let Some(window) = weak.upgrade() else {
                return;
            };
            let Some(kind) = SplitEditorKind::from_index(kind_index) else {
                return;
            };
            let Some(chain) = chain_at(&editor.ctx.project_session, chain_index) else {
                return;
            };
            let Some((_, split)) = split_by_id(&chain, &BlockId(split_id.to_string())) else {
                return;
            };
            editor.rows.set_vec(split_editor_items(split, kind));
            let letters: Vec<slint::SharedString> =
                path_letters(&chain, &BlockId(split_id.to_string()))
                    .into_iter()
                    .map(Into::into)
                    .collect();
            let state = ChainGraphOverlayState::get(&window);
            state.set_split_editor_chain_index(chain_index);
            state.set_split_editor_kind(kind_index);
            state.set_split_editor_split_id(split_id);
            state.set_split_editor_end_y(split.end == SplitEnd::Y);
            state.set_split_editor_paths(ModelRc::new(VecModel::from(letters)));
            state.set_split_editor_can_remove_path(split.paths.len() > MIN_SPLIT_PATHS);
            state.set_split_editor_title(kind.title().into());
            state.set_split_editor_open(true);
        });
    }
    {
        let (weak, editor) = (window.as_weak(), editor.clone());
        state.on_split_editor_number(move |chain_index, split_id, path, value| {
            let Some(window) = weak.upgrade() else {
                return;
            };
            let value = ParamValue::Number(value as f64);
            commit(&window, &editor, chain_index, &split_id, &path, value);
        });
    }
    {
        let (weak, editor) = (window.as_weak(), editor.clone());
        state.on_split_editor_option(move |chain_index, split_id, path, index| {
            let Some(window) = weak.upgrade() else {
                return;
            };
            let kind = SplitEditorKind::from_index(
                ChainGraphOverlayState::get(&window).get_split_editor_kind(),
            );
            let Some(value) = kind.and_then(|k| option_value(k, &path, index as usize)) else {
                return;
            };
            let choice = ParamValue::Option {
                value,
                index: index as usize,
            };
            commit(&window, &editor, chain_index, &split_id, &path, choice);
        });
    }
    {
        let (weak, editor) = (window.as_weak(), editor.clone());
        state.on_split_editor_bool(move |chain_index, split_id, path, on| {
            let Some(window) = weak.upgrade() else {
                return;
            };
            commit(
                &window,
                &editor,
                chain_index,
                &split_id,
                &path,
                ParamValue::Bool(on),
            );
        });
    }
    {
        let (weak, editor) = (window.as_weak(), editor.clone());
        state.on_split_editor_set_end(move |chain_index, split_id, y| {
            let Some(window) = weak.upgrade() else {
                return;
            };
            switch_end(&window, &editor, chain_index, &split_id, y);
        });
    }
}

fn commit(
    window: &AppWindow,
    editor: &Editor,
    chain_index: i32,
    split_id: &str,
    path: &str,
    value: ParamValue,
) {
    let Some(chain) = chain_at(&editor.ctx.project_session, chain_index) else {
        return;
    };
    let result = {
        let inputs = editor.ctx.input_chain_devices.borrow();
        let outputs = editor.ctx.output_chain_devices.borrow();
        apply_parameter_to_block(
            &editor.ctx.project_session,
            chain.id.clone(),
            BlockId(split_id.to_string()),
            path,
            value,
            &editor.ctx.project_chains,
            &inputs,
            &outputs,
        )
    };
    match result {
        Ok(_) => refresh_rows(window, editor, chain_index),
        Err(ApplyParamError::Failed(err)) => set_status_error(
            window,
            &editor.ctx.toast_timer,
            &rust_i18n::t!("error-graph-action", err = err),
        ),
        Err(ApplyParamError::NotAddressable) => {
            log::warn!("[split-editor] split {split_id} is gone")
        }
    }
}

/// The Mix / Y switch of the split `split_id`: `SetSplitEnd`, a refusal shown
/// as a toast; the lit segment always follows what the project holds
/// afterwards.
fn switch_end(window: &AppWindow, editor: &Editor, chain_index: i32, split_id: &str, y: bool) {
    let Ok(index) = usize::try_from(chain_index) else {
        return;
    };
    let split_id = BlockId(split_id.to_string());
    let end = if y { SplitEnd::Y } else { SplitEnd::Mix };
    let result = {
        let inputs = editor.ctx.input_chain_devices.borrow();
        let outputs = editor.ctx.output_chain_devices.borrow();
        let rows = RowsTarget {
            model: &editor.ctx.project_chains,
            inputs: &inputs,
            outputs: &outputs,
        };
        set_split_end(&editor.ctx.project_session, index, &split_id, end, &rows)
    };
    match result {
        Ok(()) => {}
        Err(GestureError::Failed(err)) => set_status_error(
            window,
            &editor.ctx.toast_timer,
            &rust_i18n::t!("error-graph-action", err = err),
        ),
        Err(other) => log::warn!("[split-editor] end switch ignored: {other:?}"),
    }
    let now_y = chain_at(&editor.ctx.project_session, chain_index)
        .and_then(|chain| split_by_id(&chain, &split_id).map(|(_, split)| split.end == SplitEnd::Y))
        .unwrap_or(false);
    ChainGraphOverlayState::get(window).set_split_editor_end_y(now_y);
}

/// Same row count ⇒ update each row in place (the knob under the pointer
/// keeps its identity); otherwise swap the list.
fn refresh_rows(window: &AppWindow, editor: &Editor, chain_index: i32) {
    let state = ChainGraphOverlayState::get(window);
    let Some(kind) = SplitEditorKind::from_index(state.get_split_editor_kind()) else {
        return;
    };
    let Some(chain) = chain_at(&editor.ctx.project_session, chain_index) else {
        return;
    };
    let split_id = BlockId(state.get_split_editor_split_id().to_string());
    let Some((_, split)) = split_by_id(&chain, &split_id) else {
        return;
    };
    let fresh = split_editor_items(split, kind);
    if fresh.len() != editor.rows.row_count() {
        editor.rows.set_vec(fresh);
        return;
    }
    for (index, row) in fresh.into_iter().enumerate() {
        editor.rows.set_row_data(index, row);
    }
}
