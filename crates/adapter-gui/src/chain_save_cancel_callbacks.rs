//! `on_save_chain` and `on_cancel_chain` — finalize the chain editor flow.
//!
//! Save validates the active draft (must have at least one input + one
//! output, every group must have a device + channels) and either replaces
//! the existing chain at `draft.editing_index` or appends a new one. The
//! channel-conflict check (`Chain::validate_channel_conflicts`) blocks the
//! save when two groups would fight over the same physical channel. On
//! success: live runtime resync, project rows refresh, dirty marker, status
//! cleared, chain editor hidden.
//!
//! Cancel discards the draft and hides the editor — no audio side effects.
//!
//! Wired once from `run_desktop_app`.

use std::cell::RefCell;
use std::rc::Rc;

use slint::{ComponentHandle, Model, Timer, VecModel};

use application::command::Command;
use application::dispatcher::CommandDispatcher;
use infra_cpal::{AudioDeviceDescriptor, ProjectRuntimeController};

use crate::chain_editor::chain_from_draft;
use crate::helpers::{clear_status, set_status_error, set_status_warning};
use crate::project_ops::sync_project_dirty;
use crate::project_view::replace_project_chains;
use crate::runtime_lifecycle::sync_live_chain_runtime;
use crate::state::{ChainDraft, ProjectSession};
use crate::{AppWindow, ProjectChainItem};

pub(crate) struct ChainSaveCancelCtx {
    pub chain_draft: Rc<RefCell<Option<ChainDraft>>>,
    pub project_session: Rc<RefCell<Option<ProjectSession>>>,
    pub project_chains: Rc<VecModel<ProjectChainItem>>,
    pub project_runtime: Rc<RefCell<Option<ProjectRuntimeController>>>,
    pub saved_project_snapshot: Rc<RefCell<Option<String>>>,
    pub project_dirty: Rc<RefCell<bool>>,
    pub input_chain_devices: Rc<RefCell<Vec<AudioDeviceDescriptor>>>,
    pub output_chain_devices: Rc<RefCell<Vec<AudioDeviceDescriptor>>>,
    pub toast_timer: Rc<Timer>,
    pub auto_save: bool,
}

pub(crate) fn wire(window: &AppWindow, ctx: ChainSaveCancelCtx) {
    let ChainSaveCancelCtx {
        chain_draft,
        project_session,
        project_chains,
        project_runtime,
        saved_project_snapshot,
        project_dirty,
        input_chain_devices,
        output_chain_devices,
        toast_timer,
        auto_save,
    } = ctx;

    // on_save_chain
    {
        let weak_window = window.as_weak();
        let chain_draft = chain_draft.clone();
        let project_session = project_session.clone();
        let project_chains = project_chains.clone();
        let project_runtime = project_runtime.clone();
        let saved_project_snapshot = saved_project_snapshot.clone();
        let project_dirty = project_dirty.clone();
        let input_chain_devices = input_chain_devices.clone();
        let output_chain_devices = output_chain_devices.clone();
        let toast_timer = toast_timer.clone();
        window.on_save_chain(move || {
            log::info!("on_save_chain triggered");
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            let mut session_borrow = project_session.borrow_mut();
            let Some(session) = session_borrow.as_mut() else {
                set_status_error(
                    &window,
                    &toast_timer,
                    &rust_i18n::t!("error-no-project-loaded"),
                );
                return;
            };
            let draft = match chain_draft.borrow().clone() {
                Some(draft) => draft,
                None => {
                    set_status_error(
                        &window,
                        &toast_timer,
                        &rust_i18n::t!("error-no-chain-editing"),
                    );
                    return;
                }
            };
            // #716: the chain's I/O comes from its selected bindings.
            if draft.io_binding_ids.is_empty() {
                set_status_warning(&window, &toast_timer, &rust_i18n::t!("warn-select-binding"));
                return;
            }
            let editing_index = draft.editing_index;
            log::debug!(
                "[save_chain] editing_index={:?}, draft.instrument='{}'",
                editing_index,
                draft.instrument
            );
            let existing_chain =
                editing_index.and_then(|index| session.project.borrow().chains.get(index).cloned());
            let chain = chain_from_draft(&draft, existing_chain.as_ref());
            // #716: materialize bound I/O blocks from the selected bindings so
            // the saved chain has input/output (valid project + audible). No-op
            // when no bindings are selected.
            let chain =
                project::binding_discovery::resolve_bound_io_blocks(&chain, &session.io_bindings.borrow());
            if let Err(msg) = chain.validate_channel_conflicts() {
                set_status_warning(&window, &toast_timer, &msg);
                return;
            }
            log::info!(
                "=== CHAIN SAVED: id='{}', name={:?}, instrument='{}', editing={:?} ===",
                chain.id.0,
                chain.description,
                chain.instrument,
                editing_index
            );
            let chain_id = chain.id.clone();
            if let Err(error) = session.dispatcher.dispatch(Command::SaveChain { chain }) {
                set_status_error(&window, &toast_timer, &error.to_string());
                return;
            }
            if let Err(error) = sync_live_chain_runtime(&project_runtime, session, &chain_id) {
                set_status_error(&window, &toast_timer, &error.to_string());
                return;
            }
            replace_project_chains(
                &project_chains,
                &*session.project.borrow(),
                &*input_chain_devices.borrow(),
                &*output_chain_devices.borrow(),
            &[]
            );
            // The chains screen has its own preset/scene combobox model
            // (chain_rig_nav) — replace_project_chains alone leaves it
            // stale, so the new chain shows "Chain 1" with an empty
            // preset selector until something else triggers a refresh.
            crate::chain_rig_nav_wiring::refresh_chain_rig_nav(&window, session);
            *chain_draft.borrow_mut() = None;
            sync_project_dirty(
                &window,
                session,
                &saved_project_snapshot,
                &project_dirty,
                auto_save,
            );
            clear_status(&window, &toast_timer);
            window.set_show_chain_editor(false);
        });
    }

    // on_cancel_chain
    {
        let weak_window = window.as_weak();
        let chain_draft = chain_draft.clone();
        let toast_timer = toast_timer.clone();
        window.on_cancel_chain(move || {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            *chain_draft.borrow_mut() = None;
            clear_status(&window, &toast_timer);
            window.set_show_chain_editor(false);
        });
    }

    // on_toggle_binding (#716, fullscreen surface): mirror the secondary
    // window's handler — flip a checklist row and sync the full selection
    // into the draft so the AppWindow save persists `io_binding_ids`.
    {
        let weak_window = window.as_weak();
        let chain_draft = chain_draft.clone();
        window.on_toggle_binding(move |index, on| {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            let model = window.get_chain_editor_bindings();
            if let Some(mut choice) = model.row_data(index as usize) {
                choice.selected = on;
                model.set_row_data(index as usize, choice);
            }
            let choices: Vec<crate::ChainBindingChoice> = model.iter().collect();
            if let Some(draft) = chain_draft.borrow_mut().as_mut() {
                draft.io_binding_ids =
                    crate::chain_binding_choices::selected_binding_ids(&choices);
            }
        });
    }
}
