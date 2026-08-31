//! Responsibility: wires the in-window preset save flow.
//! In-window preset save flow (issue #510). Replaces the desktop's
//! native `FileDialog` with two overlays:
//!
//! - `PresetSaveOverlay` — single text field; user types the preset
//!   name and confirms. Touch mode still auto-saves to `presets_path`
//!   without showing the overlay.
//! - `PresetOverwriteOverlay` — second modal shown when the chosen
//!   name collides with an existing file under `presets_path`.
//!
//! The desktop callbacks (`preset-save-request`, `…-cancel`, and the
//! two `…-overwrite-*`) are owned here; the touch direct-save path
//! still lives behind `on_save_chain_preset` for symmetry with the
//! kiosk-only `auto_save` flow.

use std::cell::RefCell;
use std::rc::Rc;

use slint::{ComponentHandle, Global, Timer};

use domain::ids::ChainId;

use crate::chain_preset_wiring::preset_overwrite_required;
use crate::helpers::{set_status_error, set_status_info};
use crate::state::ProjectSession;
use crate::AppWindow;

pub(crate) fn wire(
    window: &AppWindow,
    project_session: Rc<RefCell<Option<ProjectSession>>>,
    toast_timer: Rc<Timer>,
) {
    let pending_save: crate::preset_save::PendingSaveCell = Rc::new(RefCell::new(None));

    {
        let weak_window = window.as_weak();
        let project_session = project_session.clone();
        let toast_timer = toast_timer.clone();
        let pending_save = pending_save.clone();
        window.on_save_chain_preset(move |index| {
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
            let pending = match crate::preset_save::pending_save_for(session, index as usize) {
                Ok(pending) => pending,
                Err(_) => {
                    set_status_error(&window, &toast_timer, &rust_i18n::t!("error-invalid-chain"));
                    return;
                }
            };
            let default_name = pending.default_name.clone();
            if window.get_touch_optimized() {
                // Kiosk: auto-save to presets dir, no dialog.
                // (Directory creation is handled inside the
                // `ChainCommand::SaveChainPreset` dispatcher; the GUI no
                // longer touches the filesystem here — #555.)
                perform_preset_save(
                    &window,
                    session,
                    &pending.chain_id,
                    &default_name,
                    &toast_timer,
                );
            } else {
                // Issue #510 desktop: open the in-window save overlay
                // (replaces the native FileDialog). Stash the chain
                // id + clone + default name; final write happens when
                // the user confirms via `preset-save-request`. The
                // chain id is what `SelectionCommand::RenameRigPreset` keys on
                // so the active preset's display name follows the
                // typed name end-to-end.
                *pending_save.borrow_mut() = Some(pending);
                crate::OverlayBridge::get(&window)
                    .set_preset_save_default_name(default_name.clone().into());
                crate::OverlayBridge::get(&window).set_preset_save_name_input(default_name.into());
                crate::OverlayBridge::get(&window).set_show_preset_save_overwrite(false);
                crate::OverlayBridge::get(&window).set_show_preset_save(true);
            }
        });
    }
    {
        let weak_window = window.as_weak();
        let project_session = project_session.clone();
        let toast_timer = toast_timer.clone();
        let pending_save = pending_save.clone();
        crate::OverlayBridge::get(window).on_preset_save_request(move |name| {
            log::info!("[preset-save] request received name={name:?}");
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            let mut session_borrow = project_session.borrow_mut();
            let Some(session) = session_borrow.as_mut() else {
                log::warn!("[preset-save] dropped: no session loaded");
                return;
            };
            // Peek without taking so the pending state survives if we
            // need to bounce to the overwrite-confirm overlay.
            let Some(pending) = pending_save.borrow().clone() else {
                log::warn!("[preset-save] dropped: no pending save state");
                return;
            };
            let chosen = crate::preset_save::chosen_name(name.as_str(), &pending.default_name);
            if preset_overwrite_required(&session.presets_path, &chosen) {
                crate::OverlayBridge::get(&window)
                    .set_preset_save_overwrite_name(chosen.clone().into());
                crate::OverlayBridge::get(&window).set_preset_save_name_input(chosen.into());
                crate::OverlayBridge::get(&window).set_show_preset_save_overwrite(true);
                return;
            }
            perform_preset_save(&window, session, &pending.chain_id, &chosen, &toast_timer);
            *pending_save.borrow_mut() = None;
            crate::OverlayBridge::get(&window).set_show_preset_save(false);
        });
    }
    {
        let weak_window = window.as_weak();
        let project_session = project_session.clone();
        let toast_timer = toast_timer.clone();
        let pending_save = pending_save.clone();
        crate::OverlayBridge::get(window).on_preset_save_overwrite_confirm(move || {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            let mut session_borrow = project_session.borrow_mut();
            let Some(session) = session_borrow.as_mut() else {
                return;
            };
            let Some(pending) = pending_save.borrow_mut().take() else {
                return;
            };
            let chosen = crate::OverlayBridge::get(&window)
                .get_preset_save_overwrite_name()
                .to_string();
            perform_preset_save(&window, session, &pending.chain_id, &chosen, &toast_timer);
            crate::OverlayBridge::get(&window).set_show_preset_save_overwrite(false);
            crate::OverlayBridge::get(&window).set_show_preset_save(false);
        });
    }
    {
        let weak_window = window.as_weak();
        crate::OverlayBridge::get(window).on_preset_save_overwrite_cancel(move || {
            if let Some(window) = weak_window.upgrade() {
                crate::OverlayBridge::get(&window).set_show_preset_save_overwrite(false);
            }
        });
    }
    {
        let weak_window = window.as_weak();
        let pending_save = pending_save.clone();
        crate::OverlayBridge::get(window).on_preset_save_cancel(move || {
            if let Some(window) = weak_window.upgrade() {
                crate::OverlayBridge::get(&window).set_show_preset_save(false);
                crate::OverlayBridge::get(&window).set_show_preset_save_overwrite(false);
            }
            *pending_save.borrow_mut() = None;
        });
    }
}

/// Commit a preset save: write the YAML file under the configured
/// presets directory, dispatch `ChainCommand::SaveChainPreset` so
/// MCP/MIDI/gRPC observers see the same event, then dispatch
/// `SelectionCommand::RenameRigPreset` so the active preset's display name
/// follows the name the user just typed. Without the rename the
/// chain title combobox stays on the old label and the user sees
/// "nothing happened". Issue #510.
fn perform_preset_save(
    window: &AppWindow,
    session: &mut ProjectSession,
    chain_id: &ChainId,
    name: &str,
    toast_timer: &Rc<Timer>,
) {
    match crate::preset_save::commit_preset_save(session, chain_id, name) {
        Ok(()) => {
            // Mirror the load flow: refresh the chain-rig-nav so the combobox
            // in the chain title shows the new label immediately.
            crate::chain_rig_nav_wiring::refresh_chain_rig_nav(window, session);
            set_status_info(window, toast_timer, &rust_i18n::t!("status-preset-saved"));
        }
        Err(error) => set_status_error(window, toast_timer, &error),
    }
}
