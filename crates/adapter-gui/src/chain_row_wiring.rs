//! Wiring for the per-chain row actions on the main window.
//!
//! Owns `on_remove_chain` (confirms with the user, dispatches
//! `Command::RemoveChain`, kills its runtime, and refreshes the chain list)
//! and `on_toggle_chain_enabled` (dispatches `Command::ToggleChainEnabled`;
//! channel-conflict validation is performed inside the dispatcher via
//! `chain_validation::validate_no_channel_conflict`).
//!
//! `on_move_chain_up` and `on_move_chain_down` dispatch `Command::MoveChainUp`
//! and `Command::MoveChainDown` respectively; both are no-ops when the chain
//! is already at the boundary.

use std::cell::RefCell;
use std::rc::Rc;

use slint::{ComponentHandle, Timer, VecModel};

use application::command::Command;
use application::dispatcher::CommandDispatcher;
use infra_cpal::{AudioDeviceDescriptor, ProjectRuntimeController};

use crate::helpers::{clear_status, set_status_error};
use crate::project_ops::sync_project_dirty;
use crate::project_view::replace_project_chains;
use crate::state::ProjectSession;
use crate::sync_live_chain_runtime;
use crate::{remove_live_chain_runtime, AppWindow, ProjectChainItem};

pub(crate) struct ChainRowCtx {
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

pub(crate) fn wire(window: &AppWindow, ctx: ChainRowCtx) {
    let ChainRowCtx {
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

    // ── on_remove_chain ──────────────────────────────────────────────────────
    {
        let weak_window = window.as_weak();
        let project_session = project_session.clone();
        let project_chains = project_chains.clone();
        let project_runtime = project_runtime.clone();
        let saved_project_snapshot = saved_project_snapshot.clone();
        let project_dirty = project_dirty.clone();
        let input_chain_devices = input_chain_devices.clone();
        let output_chain_devices = output_chain_devices.clone();
        let toast_timer = toast_timer.clone();
        window.on_remove_chain(move |index| {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            // Resolve chain id + name for the dialog (immutable borrow).
            let (chain_id, chain_name) = {
                let session_borrow = project_session.borrow();
                let Some(session) = session_borrow.as_ref() else {
                    set_status_error(
                        &window,
                        &toast_timer,
                        &rust_i18n::t!("error-no-project-loaded"),
                    );
                    return;
                };
                let proj = session.project.borrow();
                let index = index as usize;
                if index >= proj.chains.len() {
                    set_status_error(&window, &toast_timer, &rust_i18n::t!("error-invalid-chain"));
                    return;
                }
                let chain = &proj.chains[index];
                let name = chain.description.clone().unwrap_or_else(|| {
                    rust_i18n::t!("default-chain-name", n = index + 1).to_string()
                });
                (chain.id.clone(), name)
            };
            // Confirmation dialog — UI concern, stays in the adapter.
            let confirmed = rfd::MessageDialog::new()
                .set_title(rust_i18n::t!("dialog-delete-chain").as_ref())
                .set_description(
                    rust_i18n::t!("dialog-confirm-delete-chain", name = chain_name).to_string(),
                )
                .set_buttons(rfd::MessageButtons::YesNo)
                .set_level(rfd::MessageLevel::Warning)
                .show();
            if !matches!(confirmed, rfd::MessageDialogResult::Yes) {
                return;
            }
            // Dispatch — the dispatcher mutates project via the shared Rc.
            let session_borrow = project_session.borrow();
            let Some(session) = session_borrow.as_ref() else {
                return;
            };
            if let Err(err) = session.dispatcher.dispatch(Command::RemoveChain {
                chain: chain_id.clone(),
            }) {
                set_status_error(&window, &toast_timer, &err.to_string());
                return;
            }
            // Rig session: also drop the RigInput, else any later
            // re-projection regenerates the deleted chain ("excluí a
            // chain e a view não atualizou / voltou").
            if let Some(rig) = &session.rig {
                if let Some(name) = chain_id.0.strip_prefix("rig:") {
                    rig.borrow_mut().remove_input(name);
                }
                crate::chain_rig_nav_wiring::refresh_chain_rig_nav(&window, session);
            }
            // Kill the live audio runtime for the removed chain.
            remove_live_chain_runtime(&project_runtime, &chain_id);
            replace_project_chains(
                &project_chains,
                &*session.project.borrow(),
                &input_chain_devices.borrow(),
                &output_chain_devices.borrow(),
            );
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

    // ── on_toggle_chain_enabled ──────────────────────────────────────────────
    // Channel-conflict validation is now inside the dispatcher
    // (chain_validation::validate_no_channel_conflict).
    {
        let weak_window = window.as_weak();
        let project_session = project_session.clone();
        let project_chains = project_chains.clone();
        let project_runtime = project_runtime.clone();
        let input_chain_devices = input_chain_devices.clone();
        let output_chain_devices = output_chain_devices.clone();
        let toast_timer = toast_timer.clone();
        window.on_toggle_chain_enabled(move |index| {
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
            // Resolve chain id (immutable borrow, then release before dispatch).
            let chain_id = {
                let proj = session.project.borrow();
                let Some(chain) = proj.chains.get(index as usize) else {
                    set_status_error(&window, &toast_timer, &rust_i18n::t!("error-invalid-chain"));
                    return;
                };
                chain.id.clone()
            };
            // Dispatch — validation + mutation inside the dispatcher.
            if let Err(err) = session.dispatcher.dispatch(Command::ToggleChainEnabled {
                chain: chain_id.clone(),
            }) {
                // Error could be a channel conflict or a missing chain.
                set_status_error(&window, &toast_timer, &err.to_string());
                return;
            }
            if let Err(error) = sync_live_chain_runtime(&project_runtime, session, &chain_id) {
                set_status_error(&window, &toast_timer, &error.to_string());
                return;
            }
            replace_project_chains(
                &project_chains,
                &*session.project.borrow(),
                &input_chain_devices.borrow(),
                &output_chain_devices.borrow(),
            );
            // enabled is runtime-only state — do NOT mark project as dirty
            clear_status(&window, &toast_timer);
        });
    }

    // ── on_chain_volume_changed ──────────────────────────────────────────────
    // Dispatches Command::SetChainVolume; updates live runtime and persists.
    {
        let weak_window = window.as_weak();
        let project_session = project_session.clone();
        let project_chains = project_chains.clone();
        let project_runtime = project_runtime.clone();
        let saved_project_snapshot = saved_project_snapshot.clone();
        let project_dirty = project_dirty.clone();
        let input_chain_devices = input_chain_devices.clone();
        let output_chain_devices = output_chain_devices.clone();
        let toast_timer = toast_timer.clone();
        window.on_chain_volume_changed(move |index, value| {
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
            // Resolve chain id in a scoped borrow — never hold across dispatch.
            let chain_id = {
                let proj = session.project.borrow();
                let Some(chain) = proj.chains.get(index as usize) else {
                    set_status_error(&window, &toast_timer, &rust_i18n::t!("error-invalid-chain"));
                    return;
                };
                chain.id.clone()
            };
            if let Err(err) = session.dispatcher.dispatch(Command::SetChainVolume {
                chain: chain_id.clone(),
                value: value as f32,
            }) {
                set_status_error(&window, &toast_timer, &err.to_string());
                return;
            }
            if let Err(error) = sync_live_chain_runtime(&project_runtime, session, &chain_id) {
                set_status_error(&window, &toast_timer, &error.to_string());
                return;
            }
            replace_project_chains(
                &project_chains,
                &*session.project.borrow(),
                &input_chain_devices.borrow(),
                &output_chain_devices.borrow(),
            );
            sync_project_dirty(
                &window,
                session,
                &saved_project_snapshot,
                &project_dirty,
                auto_save,
            );
        });
    }

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
            let chain_id = {
                let proj = session.project.borrow();
                let Some(chain) = proj.chains.get(index as usize) else {
                    return;
                };
                chain.id.clone()
            };
            // Dispatch — dispatcher performs the swap and returns ChainMoved or
            // an empty event list (no-op when already first).
            let result = session
                .dispatcher
                .dispatch(Command::MoveChainUp { chain: chain_id });
            match result {
                Ok(events) if events.is_empty() => {
                    // No-op: already at the top.
                    return;
                }
                Ok(_) => {}
                Err(err) => {
                    set_status_error(&window, &toast_timer, &err.to_string());
                    return;
                }
            }
            replace_project_chains(
                &project_chains,
                &*session.project.borrow(),
                &input_chain_devices.borrow(),
                &output_chain_devices.borrow(),
            );
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
            let chain_id = {
                let proj = session.project.borrow();
                let Some(chain) = proj.chains.get(index as usize) else {
                    return;
                };
                chain.id.clone()
            };
            let result = session
                .dispatcher
                .dispatch(Command::MoveChainDown { chain: chain_id });
            match result {
                Ok(events) if events.is_empty() => {
                    // No-op: already at the bottom.
                    return;
                }
                Ok(_) => {}
                Err(err) => {
                    set_status_error(&window, &toast_timer, &err.to_string());
                    return;
                }
            }
            replace_project_chains(
                &project_chains,
                &*session.project.borrow(),
                &input_chain_devices.borrow(),
                &output_chain_devices.borrow(),
            );
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
