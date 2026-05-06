//! Wiring for the per-chain row actions on the main window.
//!
//! Owns `on_remove_chain` (confirms with the user, removes the chain from
//! the session, kills its runtime, and refreshes the chain list) and
//! `on_toggle_chain_enabled` (toggles enabled state with a channel-conflict
//! pre-check against other enabled chains).

use std::cell::RefCell;
use std::rc::Rc;

use slint::{ComponentHandle, Timer, VecModel};

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
            let chain_name = {
                let session_borrow = project_session.borrow();
                let Some(session) = session_borrow.as_ref() else {
                    set_status_error(
                        &window,
                        &toast_timer,
                        &rust_i18n::t!("error-no-project-loaded"),
                    );
                    return;
                };
                let index = index as usize;
                if index >= session.project.chains.len() {
                    set_status_error(&window, &toast_timer, &rust_i18n::t!("error-invalid-chain"));
                    return;
                }
                session.project.chains[index]
                    .description
                    .clone()
                    .unwrap_or_else(|| {
                        rust_i18n::t!("default-chain-name", n = index + 1).to_string()
                    })
            };
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
            let mut session_borrow = project_session.borrow_mut();
            let Some(session) = session_borrow.as_mut() else {
                return;
            };
            let index = index as usize;
            if index >= session.project.chains.len() {
                return;
            }
            let removed_chain_id = session.project.chains[index].id.clone();
            session.project.chains.remove(index);
            remove_live_chain_runtime(&project_runtime, &removed_chain_id);
            replace_project_chains(
                &project_chains,
                &session.project,
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
            let index = index as usize;
            let Some(chain) = session.project.chains.get(index) else {
                set_status_error(&window, &toast_timer, &rust_i18n::t!("error-invalid-chain"));
                return;
            };
            let will_enable = !chain.enabled;
            log::info!(
                "on_toggle_chain_enabled: index={}, will_enable={}",
                index,
                will_enable
            );
            // Check channel conflict before enabling
            if will_enable {
                let chain_id = chain.id.clone();
                let our_inputs = chain.input_blocks();
                let mut conflict = false;
                'outer: for other in &session.project.chains {
                    if other.id != chain_id && other.enabled {
                        for (_, other_input) in other.input_blocks() {
                            for (_, our_input) in &our_inputs {
                                let other_entries_conflict = other_input.entries.iter().any(|oe| {
                                    our_input.entries.iter().any(|ue| {
                                        oe.device_id == ue.device_id
                                            && oe.channels.iter().any(|ch| ue.channels.contains(ch))
                                    })
                                });
                                if other_entries_conflict {
                                    let other_name_default =
                                        rust_i18n::t!("default-other-chain").to_string();
                                    let other_name =
                                        other.description.as_deref().unwrap_or(&other_name_default);
                                    set_status_error(
                                        &window,
                                        &toast_timer,
                                        &rust_i18n::t!("error-channel-in-use", name = other_name)
                                            .to_string(),
                                    );
                                    conflict = true;
                                    break 'outer;
                                }
                            }
                        }
                    }
                }
                if conflict {
                    return;
                }
            }
            let Some(chain) = session.project.chains.get_mut(index) else {
                return;
            };
            chain.enabled = will_enable;
            let chain_id = chain.id.clone();
            if let Err(error) = sync_live_chain_runtime(&project_runtime, session, &chain_id) {
                set_status_error(&window, &toast_timer, &error.to_string());
                return;
            }
            replace_project_chains(
                &project_chains,
                &session.project,
                &input_chain_devices.borrow(),
                &output_chain_devices.borrow(),
            );
            // enabled is runtime-only state — do NOT mark project as dirty
            clear_status(&window, &toast_timer);
        });
    }
    // --- chain reorder (issue #246) ---
    // Reordering swaps Chain entries inside `Project::chains`. ChainIds stay
    // stable, so the live runtime doesn't need to be torn down — only the
    // ProjectChainItem VecModel needs to be rebuilt so the UI reflects the
    // new order. The project YAML preserves the order on save, so the new
    // arrangement persists.
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
            if !session.project.move_chain_up(index as usize) {
                return;
            }
            replace_project_chains(
                &project_chains,
                &session.project,
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
            if !session.project.move_chain_down(index as usize) {
                return;
            }
            replace_project_chains(
                &project_chains,
                &session.project,
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
