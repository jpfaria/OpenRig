//! Save / cancel callbacks for the per-instance `ChainEditorWindow`.
//!
//! `on_save_chain` validates the active draft and either replaces the
//! existing chain at `draft.editing_index` or appends a new one. The
//! channel-conflict check (`Chain::validate_channel_conflicts`) blocks the
//! save when two groups would fight over the same physical channel. On
//! success: live runtime resync, project rows refresh, dirty marker, status
//! cleared, chain editor hidden.
//!
//! `on_cancel_chain` discards the draft and hides the editor — no audio
//! side effects.
//!
//! Distinct from `chain_save_cancel_callbacks` (which wires AppWindow): this
//! module wires the secondary `ChainEditorWindow`.

use std::cell::RefCell;
use std::rc::Rc;

use slint::{ComponentHandle, Timer, VecModel};

use infra_cpal::{AudioDeviceDescriptor, ProjectRuntimeController};

use crate::chain_editor::chain_from_draft;
use crate::helpers::clear_status;
use crate::project_ops::sync_project_dirty;
use crate::project_view::replace_project_chains;
use crate::state::{ChainDraft, ProjectSession};
use crate::sync_live_chain_runtime;
use crate::{AppWindow, ChainEditorWindow, ProjectChainItem};

#[allow(clippy::too_many_arguments)]
pub(crate) fn wire(
    editor_window: &ChainEditorWindow,
    weak_window: slint::Weak<AppWindow>,
    chain_draft: Rc<RefCell<Option<ChainDraft>>>,
    project_session: Rc<RefCell<Option<ProjectSession>>>,
    project_chains: Rc<VecModel<ProjectChainItem>>,
    project_runtime: Rc<RefCell<Option<ProjectRuntimeController>>>,
    saved_project_snapshot: Rc<RefCell<Option<String>>>,
    project_dirty: Rc<RefCell<bool>>,
    input_chain_devices: Rc<RefCell<Vec<AudioDeviceDescriptor>>>,
    output_chain_devices: Rc<RefCell<Vec<AudioDeviceDescriptor>>>,
    toast_timer: Rc<Timer>,
    auto_save: bool,
) {
    // on_save_chain
    {
        let weak_window = weak_window.clone();
        let weak_chain_window = editor_window.as_weak();
        let chain_draft = chain_draft.clone();
        let project_session = project_session.clone();
        let project_chains = project_chains.clone();
        let project_runtime = project_runtime.clone();
        let saved_project_snapshot = saved_project_snapshot.clone();
        let project_dirty = project_dirty.clone();
        let input_chain_devices = input_chain_devices.clone();
        let output_chain_devices = output_chain_devices.clone();
        let toast_timer = toast_timer.clone();
        editor_window.on_save_chain(move || {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            let Some(chain_window) = weak_chain_window.upgrade() else {
                return;
            };
            let mut session_borrow = project_session.borrow_mut();
            let Some(session) = session_borrow.as_mut() else {
                chain_window.set_status_message(
                    rust_i18n::t!("error-no-project-loaded").to_string().into(),
                );
                return;
            };
            let draft = match chain_draft.borrow().clone() {
                Some(draft) => draft,
                None => {
                    chain_window.set_status_message(
                        rust_i18n::t!("error-no-chain-editing").to_string().into(),
                    );
                    return;
                }
            };
            if draft.inputs.is_empty() {
                chain_window.set_status_message(rust_i18n::t!("warn-add-input").to_string().into());
                return;
            }
            if draft.outputs.is_empty() {
                chain_window
                    .set_status_message(rust_i18n::t!("warn-add-output").to_string().into());
                return;
            }
            for (i, input) in draft.inputs.iter().enumerate() {
                if input.device_id.is_none() {
                    chain_window.set_status_message(
                        rust_i18n::t!("error-input-no-device-numbered", n = i + 1)
                            .to_string()
                            .into(),
                    );
                    return;
                }
                if input.channels.is_empty() {
                    chain_window.set_status_message(
                        rust_i18n::t!("error-input-no-channels-numbered", n = i + 1)
                            .to_string()
                            .into(),
                    );
                    return;
                }
            }
            for (i, output) in draft.outputs.iter().enumerate() {
                if output.device_id.is_none() {
                    chain_window.set_status_message(
                        rust_i18n::t!("error-output-no-device-numbered", n = i + 1)
                            .to_string()
                            .into(),
                    );
                    return;
                }
                if output.channels.is_empty() {
                    chain_window.set_status_message(
                        rust_i18n::t!("error-output-no-channels-numbered", n = i + 1)
                            .to_string()
                            .into(),
                    );
                    return;
                }
            }
            let editing_index = draft.editing_index;
            log::debug!(
                "[save_chain] editing_index={:?}, draft.instrument='{}'",
                editing_index,
                draft.instrument
            );
            let existing_chain =
                editing_index.and_then(|index| session.project.chains.get(index).cloned());
            let chain = chain_from_draft(&draft, existing_chain.as_ref());
            if let Err(msg) = chain.validate_channel_conflicts() {
                chain_window.set_status_message(msg.into());
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
            if let Some(index) = editing_index {
                if let Some(current) = session.project.chains.get_mut(index) {
                    *current = chain;
                }
            } else {
                session.project.chains.push(chain);
            }
            if let Err(error) = sync_live_chain_runtime(&project_runtime, session, &chain_id) {
                chain_window.set_status_message(error.to_string().into());
                return;
            }
            replace_project_chains(
                &project_chains,
                &session.project,
                &*input_chain_devices.borrow(),
                &*output_chain_devices.borrow(),
            );
            *chain_draft.borrow_mut() = None;
            sync_project_dirty(
                &window,
                session,
                &saved_project_snapshot,
                &project_dirty,
                auto_save,
            );
            chain_window.set_status_message("".into());
            clear_status(&window, &toast_timer);
            window.set_show_chain_editor(false);
            let _ = chain_window.hide();
        });
    }
    // on_cancel_chain
    {
        let weak_window = weak_window.clone();
        let weak_chain_window = editor_window.as_weak();
        let chain_draft = chain_draft.clone();
        let toast_timer = toast_timer.clone();
        editor_window.on_cancel_chain(move || {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            let Some(chain_window) = weak_chain_window.upgrade() else {
                return;
            };
            *chain_draft.borrow_mut() = None;
            chain_window.set_status_message("".into());
            clear_status(&window, &toast_timer);
            window.set_show_chain_editor(false);
            let _ = chain_window.hide();
        });
    }
}
