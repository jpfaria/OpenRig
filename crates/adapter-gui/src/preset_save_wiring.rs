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

use slint::{ComponentHandle, Timer};

use application::command::Command;
use application::dispatcher::CommandDispatcher;
use domain::ids::ChainId;
use project::chain::Chain;

use crate::chain_preset_wiring::{default_preset_filename_slug, preset_overwrite_required};
use crate::helpers::{set_status_error, set_status_info};
use crate::state::ProjectSession;
use crate::AppWindow;

/// State carried across the in-window save flow: the user opens the
/// save overlay, optionally hits the overwrite confirm, and only then
/// commits. The chain clone is captured at open time so a later
/// project mutation can't slip into the saved file. Issue #510.
struct PendingSave {
    chain_id: ChainId,
    chain_clone: Chain,
    default_name: String,
}

pub(crate) fn wire(
    window: &AppWindow,
    project_session: Rc<RefCell<Option<ProjectSession>>>,
    toast_timer: Rc<Timer>,
) {
    let pending_save: Rc<RefCell<Option<PendingSave>>> = Rc::new(RefCell::new(None));

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
            let (chain_desc, chain_clone, chain_id) = {
                let proj = session.project.borrow();
                let Some(chain) = proj.chains.get(index as usize) else {
                    drop(proj);
                    set_status_error(&window, &toast_timer, &rust_i18n::t!("error-invalid-chain"));
                    return;
                };
                (
                    chain
                        .description
                        .clone()
                        .unwrap_or_else(|| format!("chain_{}", index + 1)),
                    chain.clone(),
                    chain.id.clone(),
                )
            };
            // Issue #518: name source = the active preset's name, not
            // the chain's title (which is `input.label` after #436).
            // Issue #510: pass the name through verbatim — no slug.
            let preset_name = session
                .rig
                .as_ref()
                .and_then(|r| default_preset_filename_slug(&chain_id, &r.borrow()));
            let default_name = preset_name.unwrap_or_else(|| chain_desc.clone());

            if window.get_touch_optimized() {
                // Kiosk: auto-save to presets dir, no dialog.
                // (Directory creation is handled inside the
                // `Command::SaveChainPreset` dispatcher; the GUI no
                // longer touches the filesystem here — #555.)
                perform_preset_save(
                    &window,
                    session,
                    &chain_id,
                    &chain_clone,
                    &default_name,
                    &toast_timer,
                );
            } else {
                // Issue #510 desktop: open the in-window save overlay
                // (replaces the native FileDialog). Stash the chain
                // id + clone + default name; final write happens when
                // the user confirms via `preset-save-request`. The
                // chain id is what `Command::RenameRigPreset` keys on
                // so the active preset's display name follows the
                // typed name end-to-end.
                *pending_save.borrow_mut() = Some(PendingSave {
                    chain_id,
                    chain_clone,
                    default_name: default_name.clone(),
                });
                window.set_preset_save_default_name(default_name.clone().into());
                window.set_preset_save_name_input(default_name.into());
                window.set_show_preset_save_overwrite(false);
                window.set_show_preset_save(true);
            }
        });
    }
    {
        let weak_window = window.as_weak();
        let project_session = project_session.clone();
        let toast_timer = toast_timer.clone();
        let pending_save = pending_save.clone();
        window.on_preset_save_request(move |name| {
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
            let (chain_id, chain_clone, default_name) = {
                let pending = pending_save.borrow();
                let Some(pending) = pending.as_ref() else {
                    log::warn!("[preset-save] dropped: no pending save state");
                    return;
                };
                (
                    pending.chain_id.clone(),
                    pending.chain_clone.clone(),
                    pending.default_name.clone(),
                )
            };
            let chosen = if name.trim().is_empty() {
                default_name
            } else {
                name.trim().to_string()
            };
            if preset_overwrite_required(&session.presets_path, &chosen) {
                window.set_preset_save_overwrite_name(chosen.clone().into());
                window.set_preset_save_name_input(chosen.into());
                window.set_show_preset_save_overwrite(true);
                return;
            }
            perform_preset_save(
                &window,
                session,
                &chain_id,
                &chain_clone,
                &chosen,
                &toast_timer,
            );
            *pending_save.borrow_mut() = None;
            window.set_show_preset_save(false);
        });
    }
    {
        let weak_window = window.as_weak();
        let project_session = project_session.clone();
        let toast_timer = toast_timer.clone();
        let pending_save = pending_save.clone();
        window.on_preset_save_overwrite_confirm(move || {
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
            let chosen = window.get_preset_save_overwrite_name().to_string();
            perform_preset_save(
                &window,
                session,
                &pending.chain_id,
                &pending.chain_clone,
                &chosen,
                &toast_timer,
            );
            window.set_show_preset_save_overwrite(false);
            window.set_show_preset_save(false);
        });
    }
    {
        let weak_window = window.as_weak();
        window.on_preset_save_overwrite_cancel(move || {
            if let Some(window) = weak_window.upgrade() {
                window.set_show_preset_save_overwrite(false);
            }
        });
    }
    {
        let weak_window = window.as_weak();
        let pending_save = pending_save.clone();
        window.on_preset_save_cancel(move || {
            if let Some(window) = weak_window.upgrade() {
                window.set_show_preset_save(false);
                window.set_show_preset_save_overwrite(false);
            }
            *pending_save.borrow_mut() = None;
        });
    }
}

/// Commit a preset save: write the YAML file under the configured
/// presets directory, dispatch `Command::SaveChainPreset` so
/// MCP/MIDI/gRPC observers see the same event, then dispatch
/// `Command::RenameRigPreset` so the active preset's display name
/// follows the name the user just typed. Without the rename the
/// chain title combobox stays on the old label and the user sees
/// "nothing happened". Issue #510.
fn perform_preset_save(
    window: &AppWindow,
    session: &mut ProjectSession,
    chain_id: &ChainId,
    _chain_clone: &Chain,
    name: &str,
    toast_timer: &Rc<Timer>,
) {
    // #555: the YAML write and the `create_dir_all` used to happen
    // here in the adapter. They now live inside the dispatcher
    // handler for `Command::SaveChainPreset`, so MCP/MIDI/gRPC
    // clients produce the same on-disk effect as the GUI.
    match session.dispatcher.dispatch(Command::SaveChainPreset {
        chain: chain_id.clone(),
        name: name.to_string(),
    }) {
        Ok(_) => {
            // Mirror the load flow: rename the active preset to the
            // chosen name and refresh the chain-rig-nav so the
            // combobox in the chain title reflects the new label
            // immediately.
            if let Err(e) = session.dispatcher.dispatch(Command::RenameRigPreset {
                chain: chain_id.clone(),
                name: name.to_string(),
            }) {
                log::warn!("[preset-save] Command::RenameRigPreset failed: {e}");
            }
            crate::chain_rig_nav_wiring::refresh_chain_rig_nav(window, session);
            set_status_info(window, toast_timer, &rust_i18n::t!("status-preset-saved"));
        }
        Err(error) => set_status_error(window, toast_timer, &error.to_string()),
    }
}
