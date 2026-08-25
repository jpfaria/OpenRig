//! Issue #360: the compact chain view deletes a chain through an overlay of its
//! OWN — the modal stays where the user clicked instead of surfacing on the main
//! window. The pending chain id is per-window, so cancel/confirm always resolve
//! to the chain this view captured.
//!
//! Split out of `compact_chain_callbacks.rs` to keep it under the file-size cap.

use std::cell::RefCell;
use std::rc::Rc;

use slint::{ComponentHandle, Global, Timer};

use domain::AudioDeviceDescriptor;

use crate::helpers::set_status_error;
use crate::state::ProjectSession;
use crate::{AppWindow, CompactChainViewWindow, ProjectChainItem};

pub(crate) struct CompactChainDeleteCtx {
    pub project_session: Rc<RefCell<Option<ProjectSession>>>,
    pub project_chains: Rc<slint::VecModel<ProjectChainItem>>,
    pub saved_project_snapshot: Rc<RefCell<Option<String>>>,
    pub project_dirty: Rc<RefCell<bool>>,
    pub input_chain_devices: Rc<RefCell<Vec<AudioDeviceDescriptor>>>,
    pub output_chain_devices: Rc<RefCell<Vec<AudioDeviceDescriptor>>>,
    pub toast_timer: Rc<Timer>,
    pub auto_save: bool,
}

pub(crate) fn wire(
    window: &AppWindow,
    compact_win: &CompactChainViewWindow,
    ctx: CompactChainDeleteCtx,
) {
    let pending: Rc<RefCell<Option<domain::ids::ChainId>>> = Rc::new(RefCell::new(None));

    {
        let weak_compact = compact_win.as_weak();
        let project_session = ctx.project_session.clone();
        let pending = pending.clone();
        compact_win.on_remove_chain(move |ci| {
            let Some(cw) = weak_compact.upgrade() else {
                return;
            };
            let session_borrow = project_session.borrow();
            let Some(session) = session_borrow.as_ref() else {
                return;
            };
            let (chain_id, chain_name) = {
                let proj = session.project.borrow();
                let Some(chain) = proj.chains.get(ci as usize) else {
                    return;
                };
                (
                    chain.id.clone(),
                    chain
                        .description
                        .clone()
                        .unwrap_or_else(|| chain.id.0.clone()),
                )
            };
            *pending.borrow_mut() = Some(chain_id);
            crate::OverlayBridge::get(&cw).set_confirm_delete_chain_name(chain_name.into());
            crate::OverlayBridge::get(&cw).set_show_confirm_delete_chain(true);
        });
    }
    {
        let weak_compact = compact_win.as_weak();
        let pending = pending.clone();
        crate::OverlayBridge::get(compact_win).on_cancel_delete_chain(move || {
            *pending.borrow_mut() = None;
            if let Some(cw) = weak_compact.upgrade() {
                crate::OverlayBridge::get(&cw).set_show_confirm_delete_chain(false);
            }
        });
    }
    {
        let weak_main = window.as_weak();
        let weak_compact = compact_win.as_weak();
        crate::OverlayBridge::get(compact_win).on_confirm_delete_chain(move || {
            let Some(cw) = weak_compact.upgrade() else {
                return;
            };
            crate::OverlayBridge::get(&cw).set_show_confirm_delete_chain(false);
            let Some(chain_id) = pending.borrow_mut().take() else {
                return;
            };
            let Some(main_win) = weak_main.upgrade() else {
                return;
            };
            let session_borrow = ctx.project_session.borrow();
            let Some(session) = session_borrow.as_ref() else {
                return;
            };
            // #127: the dispatch also drops the deleted chain's live runtime
            // (`RuntimeControl::remove_chain`), so this callback no longer
            // tears the audio down itself — and a chain deleted over MCP/gRPC
            // stops sounding too.
            if let Err(err) = session
                .dispatcher
                .dispatch(application::command::Command::Chain(
                    application::command::ChainCommand::RemoveChain {
                        chain: chain_id.clone(),
                    },
                ))
            {
                set_status_error(&main_win, &ctx.toast_timer, &err.to_string());
                return;
            }
            if session.rig.is_some() {
                crate::chain_rig_nav_wiring::refresh_chain_rig_nav(&main_win, session);
            }
            crate::project_view::replace_project_chains(
                &ctx.project_chains,
                &session.project.borrow(),
                &ctx.input_chain_devices.borrow(),
                &ctx.output_chain_devices.borrow(),
                &[],
            );
            crate::project_ops::sync_project_dirty(
                &main_win,
                session,
                &ctx.saved_project_snapshot,
                &ctx.project_dirty,
                ctx.auto_save,
            );
            let _ = cw.hide();
        });
    }
}
