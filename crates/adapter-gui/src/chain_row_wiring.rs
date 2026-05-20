//! Wiring for the per-chain row actions on the main window.
//!
//! Owns `on_remove_chain` (confirms with the user, dispatches
//! `Command::RemoveChain`, kills its runtime, and refreshes the chain list)
//! and `on_toggle_chain_enabled` (dispatches `Command::ToggleChainEnabled`;
//! channel-conflict validation is performed inside the dispatcher via
//! `chain_validation::validate_no_channel_conflict`).
//!
//! `on_move_chain_up` and `on_move_chain_down` route through the pure
//! [`apply_move_chain_up`] / [`apply_move_chain_down`] helpers (issue #502)
//! so the in-memory project is in sync with the dispatcher's mutation and
//! the GUI can reseat the selection cursor by [`ChainId`] rather than slot
//! index. Both are no-ops when the chain is already at the boundary.

use std::cell::RefCell;
use std::rc::Rc;

use slint::{ComponentHandle, Timer, VecModel};

use anyhow::Result;
use application::command::Command;
use application::dispatcher::CommandDispatcher;
use domain::ids::ChainId;
use infra_cpal::{AudioDeviceDescriptor, ProjectRuntimeController};

use crate::helpers::{clear_status, set_status_error};
use crate::project_ops::sync_project_dirty;
use crate::project_view::replace_project_chains;
use crate::state::ProjectSession;
use crate::sync_live_chain_runtime;
use crate::{remove_live_chain_runtime, AppWindow, ProjectChainItem};

/// Result of a successful chain reorder, used by the GUI to reseat the
/// selection cursor so it follows the moved chain by [`ChainId`] rather
/// than by slot index (issue #502, regression of #246).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MoveChainOutcome {
    pub moved_chain_id: ChainId,
    pub previous_slot: usize,
    pub new_slot: usize,
}

/// Dispatch [`Command::MoveChainUp`] for the chain at `slot` (no-op when
/// `slot == 0` or `slot` is out of range). Returns the chain that moved
/// plus its new slot so the caller can reseat the selection cursor by
/// `ChainId`. Pure (no `AppWindow`); fully unit-testable.
pub(crate) fn apply_move_chain_up(
    session: &ProjectSession,
    slot: usize,
) -> Result<Option<MoveChainOutcome>> {
    let chain_id = match session.project.borrow().chains.get(slot) {
        Some(chain) => chain.id.clone(),
        None => return Ok(None),
    };
    let events = session.dispatcher.dispatch(Command::MoveChainUp {
        chain: chain_id.clone(),
    })?;
    if events.is_empty() {
        return Ok(None);
    }
    let new_slot = session
        .project
        .borrow()
        .chains
        .iter()
        .position(|c| c.id == chain_id)
        .unwrap_or(slot.saturating_sub(1));
    Ok(Some(MoveChainOutcome {
        moved_chain_id: chain_id,
        previous_slot: slot,
        new_slot,
    }))
}

/// Dispatch [`Command::MoveChainDown`] for the chain at `slot` (no-op
/// when `slot` is the last index or out of range). See [`apply_move_chain_up`]
/// for the contract.
pub(crate) fn apply_move_chain_down(
    session: &ProjectSession,
    slot: usize,
) -> Result<Option<MoveChainOutcome>> {
    let chain_id = match session.project.borrow().chains.get(slot) {
        Some(chain) => chain.id.clone(),
        None => return Ok(None),
    };
    let events = session.dispatcher.dispatch(Command::MoveChainDown {
        chain: chain_id.clone(),
    })?;
    if events.is_empty() {
        return Ok(None);
    }
    let new_slot = session
        .project
        .borrow()
        .chains
        .iter()
        .position(|c| c.id == chain_id)
        .unwrap_or(slot + 1);
    Ok(Some(MoveChainOutcome {
        moved_chain_id: chain_id,
        previous_slot: slot,
        new_slot,
    }))
}

/// When two adjacent slots are swapped, return the new index for a
/// selection cursor anchored to whichever chain was originally at
/// `selected_chain_index`. Out-of-range cursors (e.g. `-1` meaning "no
/// selection") are left untouched.
pub(crate) fn shift_selected_chain_index_after_swap(
    selected_chain_index: i32,
    slot_a: usize,
    slot_b: usize,
) -> i32 {
    let a = slot_a as i32;
    let b = slot_b as i32;
    if selected_chain_index < 0 {
        selected_chain_index
    } else if selected_chain_index == a {
        b
    } else if selected_chain_index == b {
        a
    } else {
        selected_chain_index
    }
}

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
            // #436: the RigInput drop is done by the RemoveChain handler
            // now (business logic belongs behind the Command, not here).
            // The GUI only re-renders the rig-nav model.
            if session.rig.is_some() {
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
                &*session.project.borrow(),
                &input_chain_devices.borrow(),
                &output_chain_devices.borrow(),
            );
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
                &*session.project.borrow(),
                &input_chain_devices.borrow(),
                &output_chain_devices.borrow(),
            );
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

#[cfg(test)]
mod tests {
    //! Issue #502: cover the pure handlers powering the Chains list
    //! ▲/▼ buttons. Selection-cursor reseating is tested via
    //! [`shift_selected_chain_index_after_swap`] in isolation; the
    //! Slint integration (calling `window.set_…`) lives in the
    //! wiring above and is exercised by the chained tests below.
    use super::*;
    use application::local_dispatcher::LocalDispatcher;
    use project::chain::Chain;
    use project::project::Project;
    use std::cell::RefCell;
    use std::path::PathBuf;
    use std::rc::Rc;

    fn make_chain(id: &str, description: &str) -> Chain {
        Chain {
            id: ChainId(id.into()),
            description: Some(description.into()),
            instrument: "electric_guitar".into(),
            enabled: false,
            volume: 100.0,
            blocks: Vec::new(),
        }
    }

    fn session_with_chains(rows: &[(&str, &str)]) -> ProjectSession {
        let project = Rc::new(RefCell::new(Project {
            name: None,
            device_settings: Vec::new(),
            chains: rows.iter().map(|(id, desc)| make_chain(id, desc)).collect(),
        }));
        let dispatcher = Rc::new(LocalDispatcher::new(Rc::clone(&project)));
        ProjectSession {
            project,
            dispatcher,
            project_path: None,
            config_path: None,
            presets_path: PathBuf::from("./presets"),
            rig: None,
        }
    }

    fn chain_ids(session: &ProjectSession) -> Vec<String> {
        session
            .project
            .borrow()
            .chains
            .iter()
            .map(|c| c.id.0.clone())
            .collect()
    }

    #[test]
    fn apply_move_chain_up_swaps_session_chain_order() {
        let session = session_with_chains(&[("A", "alpha"), ("B", "beta")]);
        let outcome = apply_move_chain_up(&session, 1)
            .expect("dispatcher ok")
            .expect("not a no-op");
        assert_eq!(outcome.moved_chain_id.0, "B");
        assert_eq!(outcome.previous_slot, 1);
        assert_eq!(outcome.new_slot, 0);
        assert_eq!(chain_ids(&session), vec!["B", "A"]);
    }

    #[test]
    fn apply_move_chain_up_at_slot_zero_is_noop() {
        let session = session_with_chains(&[("A", "alpha"), ("B", "beta")]);
        let outcome = apply_move_chain_up(&session, 0).expect("dispatcher ok");
        assert!(outcome.is_none(), "moving slot 0 up is a no-op");
        assert_eq!(chain_ids(&session), vec!["A", "B"]);
    }

    #[test]
    fn apply_move_chain_up_invalid_slot_is_noop() {
        let session = session_with_chains(&[("A", "alpha")]);
        let outcome = apply_move_chain_up(&session, 99).expect("dispatcher ok");
        assert!(outcome.is_none(), "out-of-range slot returns None");
    }

    #[test]
    fn apply_move_chain_down_swaps_session_chain_order() {
        let session = session_with_chains(&[("A", "alpha"), ("B", "beta")]);
        let outcome = apply_move_chain_down(&session, 0)
            .expect("dispatcher ok")
            .expect("not a no-op");
        assert_eq!(outcome.moved_chain_id.0, "A");
        assert_eq!(outcome.previous_slot, 0);
        assert_eq!(outcome.new_slot, 1);
        assert_eq!(chain_ids(&session), vec!["B", "A"]);
    }

    #[test]
    fn apply_move_chain_down_at_last_slot_is_noop() {
        let session = session_with_chains(&[("A", "alpha"), ("B", "beta")]);
        let outcome = apply_move_chain_down(&session, 1).expect("dispatcher ok");
        assert!(outcome.is_none(), "moving last slot down is a no-op");
        assert_eq!(chain_ids(&session), vec!["A", "B"]);
    }

    #[test]
    fn apply_move_chain_down_invalid_slot_is_noop() {
        let session = session_with_chains(&[("A", "alpha")]);
        let outcome = apply_move_chain_down(&session, 99).expect("dispatcher ok");
        assert!(outcome.is_none(), "out-of-range slot returns None");
    }

    #[test]
    fn apply_move_chain_up_in_three_chain_project() {
        // The middle chain moves up; outcome reports it sat at slot 1 and
        // is now at slot 0 so the GUI can reseat the selection cursor.
        let session = session_with_chains(&[("A", "alpha"), ("B", "beta"), ("C", "gamma")]);
        let outcome = apply_move_chain_up(&session, 1)
            .expect("dispatcher ok")
            .expect("not a no-op");
        assert_eq!(outcome.moved_chain_id.0, "B");
        assert_eq!(outcome.new_slot, 0);
        assert_eq!(chain_ids(&session), vec!["B", "A", "C"]);
    }

    // ── selection cursor (no AppWindow) ──────────────────────────────────

    #[test]
    fn shift_selection_follows_moved_chain_on_up() {
        // User has chain at slot 1 selected; presses ▲ on that same chain.
        // The chain moves to slot 0 → cursor must follow to slot 0.
        let selected = 1;
        assert_eq!(
            shift_selected_chain_index_after_swap(selected, 1, 0),
            0,
            "cursor must follow the moved chain by ChainId, not stay on slot"
        );
    }

    #[test]
    fn shift_selection_follows_swapped_neighbour_on_up() {
        // User has chain at slot 0 selected; the user moves the chain at
        // slot 1 UP, which swaps slots 0 and 1. The originally-selected
        // chain is now at slot 1.
        let selected = 0;
        assert_eq!(
            shift_selected_chain_index_after_swap(selected, 1, 0),
            1,
            "the neighbour that got displaced must keep its ChainId selection"
        );
    }

    #[test]
    fn shift_selection_follows_moved_chain_on_down() {
        // User has chain at slot 0 selected; presses ▼ on it; chain moves
        // to slot 1 → cursor follows.
        let selected = 0;
        assert_eq!(shift_selected_chain_index_after_swap(selected, 0, 1), 1);
    }

    #[test]
    fn shift_selection_unaffected_for_unrelated_chain() {
        // User selected chain at slot 2; the move only swaps slots 0 and 1.
        let selected = 2;
        assert_eq!(
            shift_selected_chain_index_after_swap(selected, 0, 1),
            2,
            "an untouched slot's selection must not shift"
        );
    }

    #[test]
    fn shift_selection_preserves_no_selection_sentinel() {
        // `-1` is the Slint sentinel for "nothing selected"; it must not
        // be remapped.
        let selected = -1;
        assert_eq!(shift_selected_chain_index_after_swap(selected, 0, 1), -1);
    }
}
